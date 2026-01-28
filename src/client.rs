//! KV Cache Client implementation
//!
//! The client registers its receive buffer with the server, then sends
//! GET/PUT requests via RPC. For GET requests, the server RDMA writes
//! the value directly to the client's registered buffer.

use crate::memory::{MemoryPool, MemoryPoolConfig, PoolAllocation};
use crate::pb::kv_cache_service_client::KvCacheServiceClient;
use crate::pb::{
    DeleteRequest, GetRequest, HeartbeatRequest, PutRequest, RegisterClientRequest,
};
use crate::protocol::{DomainAddress, ValueLocation};
use crate::transport::{RdmaTransport, TransportConfig};
use anyhow::{anyhow, Result};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tonic::transport::Channel;

/// Client configuration
#[derive(Clone, Debug)]
pub struct ClientConfig {
    /// Client node ID
    pub client_id: u32,
    /// Server address (gRPC endpoint)
    pub server_addr: String,
    /// Receive buffer size for RDMA transfers
    pub receive_buffer_size: usize,
    /// Transport configuration
    pub transport: TransportConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            client_id: 1,
            server_addr: "http://[::1]:50051".to_string(),
            receive_buffer_size: 64 * 1024 * 1024, // 64MB default
            transport: TransportConfig::default(),
        }
    }
}

/// Allocation tracking for pending requests
struct PendingAllocation {
    allocation: PoolAllocation,
    expected_length: u64,
}

/// KV Cache Client
pub struct KvCacheClient {
    config: ClientConfig,
    /// gRPC client for control plane
    grpc_client: Mutex<Option<KvCacheServiceClient<Channel>>>,
    /// RDMA transport
    transport: Arc<RdmaTransport>,
    /// Memory pool for receive buffer
    memory_pool: Arc<RwLock<MemoryPool>>,
    /// Pending allocations for in-flight requests
    pending: Arc<Mutex<HashMap<u64, PendingAllocation>>>,
    /// Request ID counter
    request_counter: AtomicU64,
    /// Server information after registration
    server_info: RwLock<Option<ServerInfo>>,
}

struct ServerInfo {
    server_id: u32,
    domain_addresses: Vec<DomainAddress>,
}

impl KvCacheClient {
    /// Create a new KV cache client
    pub fn new(config: ClientConfig) -> Result<Self> {
        let mut transport_config = config.transport.clone();
        transport_config.node_id = config.client_id;
        let transport = Arc::new(RdmaTransport::new(transport_config)?);

        let pool_config = MemoryPoolConfig {
            size: config.receive_buffer_size,
            alignment: 4096,
        };
        let memory_pool = Arc::new(RwLock::new(MemoryPool::new(
            pool_config,
            config.client_id,
            transport.domain_addresses(),
        )?));

        Ok(Self {
            config,
            grpc_client: Mutex::new(None),
            transport,
            memory_pool,
            pending: Arc::new(Mutex::new(HashMap::new())),
            request_counter: AtomicU64::new(0),
            server_info: RwLock::new(None),
        })
    }

    /// Connect to the server
    pub async fn connect(&self) -> Result<()> {
        tracing::info!("Connecting to server at {}", self.config.server_addr);

        let channel = Channel::from_shared(self.config.server_addr.clone())?
            .connect()
            .await?;

        let mut client = KvCacheServiceClient::new(channel);

        // Register with the server
        let domain_addresses: Vec<Vec<u8>> = self
            .transport
            .domain_addresses()
            .iter()
            .map(|a| a.0.clone())
            .collect();

        let response = client
            .register_client(RegisterClientRequest {
                client_id: self.config.client_id,
                domain_addresses,
                receive_buffer_size: self.config.receive_buffer_size as u64,
            })
            .await?
            .into_inner();

        if !response.success {
            return Err(anyhow!("Failed to register with server"));
        }

        tracing::info!(
            "Registered with server {}, got {} domain addresses",
            response.server_id,
            response.server_domain_addresses.len()
        );

        *self.server_info.write() = Some(ServerInfo {
            server_id: response.server_id,
            domain_addresses: response
                .server_domain_addresses
                .into_iter()
                .map(DomainAddress::new)
                .collect(),
        });

        *self.grpc_client.lock() = Some(client);

        Ok(())
    }

    /// Get a value from the server
    ///
    /// The server will RDMA write the value directly to our receive buffer.
    /// Returns the value data.
    pub async fn get(&self, key: &[u8]) -> Result<Vec<u8>> {
        let mut client = self
            .grpc_client
            .lock()
            .clone()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let request_id = self.request_counter.fetch_add(1, Ordering::Relaxed);

        // Allocate receive buffer
        // For simplicity, we allocate a reasonable max size
        // In production, you might want to query the value size first
        let max_value_size = 1024 * 1024; // 1MB max value

        let allocation = {
            let pool = self.memory_pool.read();
            pool.allocate(max_value_size)?
        };

        // Create response location with our buffer info
        let pool = self.memory_pool.read();
        let response_location = ValueLocation::new(
            self.config.client_id,
            pool.descriptor().clone(),
            allocation.offset as u64,
            max_value_size as u64,
        );

        // Track pending allocation
        self.pending.lock().insert(
            request_id,
            PendingAllocation {
                allocation,
                expected_length: max_value_size as u64,
            },
        );

        // Send GET request
        let pb_response_location: crate::pb::ValueLocation = (&response_location).into();

        let response = client
            .get(GetRequest {
                key: key.to_vec(),
                response_location: Some(pb_response_location),
                request_id,
            })
            .await?
            .into_inner();

        // Get the pending allocation
        let pending = self
            .pending
            .lock()
            .remove(&request_id)
            .ok_or_else(|| anyhow!("Request {} not found in pending", request_id))?;

        if !response.success {
            // Deallocate the buffer
            self.memory_pool.write().deallocate(&pending.allocation);
            return Err(anyhow!("GET failed: {}", response.error_message));
        }

        // Read the value from our receive buffer
        let value = {
            let pool = self.memory_pool.read();
            pool.read(pending.allocation.offset, response.value_length as usize)?
                .to_vec()
        };

        // Deallocate the buffer
        self.memory_pool.write().deallocate(&pending.allocation);

        Ok(value)
    }

    /// Put a value into the server's cache
    pub async fn put(&self, key: &[u8], value: &[u8], ttl_seconds: u64) -> Result<()> {
        let mut client = self
            .grpc_client
            .lock()
            .clone()
            .ok_or_else(|| anyhow!("Not connected"))?;

        // For small values, send inline
        // For large values, we could use RDMA (not implemented yet)
        let value_source = if value.len() < 64 * 1024 {
            crate::pb::put_request::ValueSource::InlineValue(value.to_vec())
        } else {
            // TODO: Implement RDMA-based PUT for large values
            return Err(anyhow!("Large value PUT via RDMA not yet implemented"));
        };

        let response = client
            .put(PutRequest {
                key: key.to_vec(),
                value_source: Some(value_source),
                ttl_seconds,
            })
            .await?
            .into_inner();

        if !response.success {
            return Err(anyhow!("PUT failed: {}", response.error_message));
        }

        Ok(())
    }

    /// Delete a value from the server's cache
    pub async fn delete(&self, key: &[u8]) -> Result<bool> {
        let mut client = self
            .grpc_client
            .lock()
            .clone()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let response = client
            .delete(DeleteRequest { key: key.to_vec() })
            .await?
            .into_inner();

        Ok(response.key_existed)
    }

    /// Send a heartbeat to the server
    pub async fn heartbeat(&self) -> Result<bool> {
        let mut client = self
            .grpc_client
            .lock()
            .clone()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let response = client
            .heartbeat(HeartbeatRequest {
                client_id: self.config.client_id,
            })
            .await?
            .into_inner();

        Ok(response.alive)
    }

    /// Check if connected to server
    pub fn is_connected(&self) -> bool {
        self.grpc_client.lock().is_some()
    }

    /// Get memory pool statistics
    pub fn memory_stats(&self) -> crate::memory::PoolStats {
        self.memory_pool.read().stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = ClientConfig {
            client_id: 1,
            receive_buffer_size: 1024 * 1024, // 1MB for testing
            ..Default::default()
        };
        let client = KvCacheClient::new(config).unwrap();
        assert!(!client.is_connected());
    }
}
