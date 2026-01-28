//! KV Cache Server implementation
//!
//! The server handles control plane RPC requests and performs RDMA writes
//! to send data to clients.

use crate::memory::{MemoryPool, MemoryPoolConfig};
use crate::pb::kv_cache_service_server::{KvCacheService, KvCacheServiceServer};
use crate::pb::{
    DeleteRequest, DeleteResponse, GetRequest, GetResponse, HeartbeatRequest, HeartbeatResponse,
    PutRequest, PutResponse, RegisterClientRequest, RegisterClientResponse,
};
use crate::protocol::{CacheEntry, DomainAddress, ValueLocation};
use crate::transport::{DomainRouting, RdmaTransport, TransferRequest, TransportConfig};
use anyhow::Result;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tonic::{Request, Response, Status};

/// Server configuration
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Server node ID
    pub node_id: u32,
    /// gRPC listen address
    pub listen_addr: String,
    /// Memory pool size in bytes
    pub memory_pool_size: usize,
    /// Transport configuration
    pub transport: TransportConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            node_id: 0,
            listen_addr: "[::1]:50051".to_string(),
            memory_pool_size: 1024 * 1024 * 1024, // 1GB
            transport: TransportConfig::default(),
        }
    }
}

/// Registered client information
struct RegisteredClient {
    client_id: u32,
    domain_addresses: Vec<DomainAddress>,
    receive_buffer_size: u64,
}

/// KV Cache Server
pub struct KvCacheServer {
    config: ServerConfig,
    /// RDMA transport for data transfers
    transport: Arc<RdmaTransport>,
    /// Memory pool for storing cached values
    memory_pool: Arc<RwLock<MemoryPool>>,
    /// Cache entries: key -> CacheEntry
    cache: Arc<DashMap<Vec<u8>, CacheEntry>>,
    /// Registered clients
    clients: Arc<RwLock<HashMap<u32, RegisteredClient>>>,
}

impl KvCacheServer {
    /// Create a new KV cache server
    pub fn new(config: ServerConfig) -> Result<Self> {
        let mut transport_config = config.transport.clone();
        transport_config.node_id = config.node_id;
        let transport = Arc::new(RdmaTransport::new(transport_config)?);

        let pool_config = MemoryPoolConfig {
            size: config.memory_pool_size,
            alignment: 4096,
        };
        let memory_pool = Arc::new(RwLock::new(MemoryPool::new(
            pool_config,
            config.node_id,
            transport.domain_addresses(),
        )?));

        Ok(Self {
            config,
            transport,
            memory_pool,
            cache: Arc::new(DashMap::new()),
            clients: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get the gRPC service for this server
    pub fn into_service(self) -> KvCacheServiceServer<KvCacheServiceImpl> {
        KvCacheServiceServer::new(KvCacheServiceImpl {
            inner: Arc::new(self),
        })
    }

    /// Get the listen address
    pub fn listen_addr(&self) -> &str {
        &self.config.listen_addr
    }

    /// Store a value in the cache
    fn put_value(&self, key: Vec<u8>, value: Vec<u8>, ttl_seconds: u64) -> Result<()> {
        let mut pool = self.memory_pool.write();

        // Allocate space in the memory pool
        let allocation = pool.allocate(value.len())?;

        // Write data to the pool
        pool.write(allocation.offset, &value)?;

        // Create cache entry
        let entry = CacheEntry::new(value, allocation.offset as u64, ttl_seconds);

        // Store in cache (this will replace any existing entry)
        if let Some(old_entry) = self.cache.insert(key, entry) {
            // Deallocate old entry's memory
            pool.deallocate(&crate::memory::PoolAllocation {
                offset: old_entry.offset as usize,
                size: old_entry.len(),
                ptr: std::ptr::null_mut(),
            });
        }

        Ok(())
    }

    /// Get a value and RDMA write it to the client's buffer
    async fn get_and_transfer(
        &self,
        key: &[u8],
        response_location: &ValueLocation,
    ) -> Result<u64, Status> {
        // Look up the value
        let entry = self
            .cache
            .get(key)
            .ok_or_else(|| Status::not_found("Key not found"))?;

        // Check if expired
        if entry.is_expired() {
            drop(entry);
            self.cache.remove(key);
            return Err(Status::not_found("Key expired"));
        }

        let value_len = entry.len() as u64;
        let src_offset = entry.offset;
        drop(entry); // Release DashMap ref before acquiring pool lock

        // Get the pool's memory handle (release lock before await)
        let (src_handle, request) = {
            let pool = self.memory_pool.read();
            let src_handle = pool.handle();

            // Create transfer request
            let request = TransferRequest {
                src_handle,
                src_offset,
                length: value_len,
                imm_data: None,
                dst_descriptor: response_location.mr_descriptor.clone(),
                dst_offset: response_location.offset,
                routing: DomainRouting::default(),
            };
            (src_handle, request)
        };

        // Perform RDMA write to client's buffer
        let result = self
            .transport
            .submit_transfer_async(request)
            .await
            .map_err(|e| Status::internal(format!("Transfer failed: {}", e)))?;

        if !result.success {
            return Err(Status::internal(
                result.error.unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        Ok(value_len)
    }

    /// Delete a value from the cache
    fn delete_value(&self, key: &[u8]) -> bool {
        if let Some((_, entry)) = self.cache.remove(key) {
            let pool = self.memory_pool.write();
            pool.deallocate(&crate::memory::PoolAllocation {
                offset: entry.offset as usize,
                size: entry.len(),
                ptr: std::ptr::null_mut(),
            });
            true
        } else {
            false
        }
    }
}

/// gRPC service implementation wrapper
pub struct KvCacheServiceImpl {
    inner: Arc<KvCacheServer>,
}

#[tonic::async_trait]
impl KvCacheService for KvCacheServiceImpl {
    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
        let req = request.into_inner();
        let request_id = req.request_id;

        tracing::debug!("GET request: key={:?}, request_id={}", req.key, request_id);

        let response_location = req
            .response_location
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("Missing response_location"))?;

        let value_location: ValueLocation = response_location.into();

        match self.inner.get_and_transfer(&req.key, &value_location).await {
            Ok(value_length) => {
                tracing::debug!(
                    "GET success: key={:?}, length={}, request_id={}",
                    req.key,
                    value_length,
                    request_id
                );
                Ok(Response::new(GetResponse {
                    success: true,
                    value_length,
                    error_message: String::new(),
                    request_id,
                }))
            }
            Err(status) => {
                tracing::warn!(
                    "GET failed: key={:?}, error={}, request_id={}",
                    req.key,
                    status.message(),
                    request_id
                );
                Ok(Response::new(GetResponse {
                    success: false,
                    value_length: 0,
                    error_message: status.message().to_string(),
                    request_id,
                }))
            }
        }
    }

    async fn put(&self, request: Request<PutRequest>) -> Result<Response<PutResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!("PUT request: key={:?}", req.key);

        let value = match req.value_source {
            Some(crate::pb::put_request::ValueSource::InlineValue(v)) => v,
            Some(crate::pb::put_request::ValueSource::RdmaLocation(_loc)) => {
                // TODO: Implement RDMA read from client for large values
                return Err(Status::unimplemented("RDMA read for PUT not yet implemented"));
            }
            None => return Err(Status::invalid_argument("Missing value")),
        };

        match self.inner.put_value(req.key, value, req.ttl_seconds) {
            Ok(()) => {
                tracing::debug!("PUT success");
                Ok(Response::new(PutResponse {
                    success: true,
                    error_message: String::new(),
                }))
            }
            Err(e) => {
                tracing::warn!("PUT failed: {}", e);
                Ok(Response::new(PutResponse {
                    success: false,
                    error_message: e.to_string(),
                }))
            }
        }
    }

    async fn delete(
        &self,
        request: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!("DELETE request: key={:?}", req.key);

        let existed = self.inner.delete_value(&req.key);

        Ok(Response::new(DeleteResponse {
            success: true,
            key_existed: existed,
        }))
    }

    async fn register_client(
        &self,
        request: Request<RegisterClientRequest>,
    ) -> Result<Response<RegisterClientResponse>, Status> {
        let req = request.into_inner();

        tracing::info!(
            "Client registration: id={}, buffer_size={}",
            req.client_id,
            req.receive_buffer_size
        );

        let client = RegisteredClient {
            client_id: req.client_id,
            domain_addresses: req.domain_addresses.into_iter().map(DomainAddress::new).collect(),
            receive_buffer_size: req.receive_buffer_size,
        };

        self.inner.clients.write().insert(req.client_id, client);

        let server_addresses: Vec<Vec<u8>> = self
            .inner
            .transport
            .domain_addresses()
            .iter()
            .map(|a| a.0.clone())
            .collect();

        Ok(Response::new(RegisterClientResponse {
            success: true,
            server_id: self.inner.config.node_id,
            server_domain_addresses: server_addresses,
        }))
    }

    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        tracing::trace!("Heartbeat from client {}", req.client_id);
        Ok(Response::new(HeartbeatResponse { alive: true }))
    }
}

/// Run the server
pub async fn run_server(config: ServerConfig) -> Result<()> {
    let addr = config.listen_addr.parse()?;
    let server = KvCacheServer::new(config)?;

    tracing::info!("Starting KV cache server on {}", addr);

    tonic::transport::Server::builder()
        .add_service(server.into_service())
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let config = ServerConfig {
            node_id: 1,
            memory_pool_size: 1024 * 1024, // 1MB for testing
            ..Default::default()
        };
        let server = KvCacheServer::new(config).unwrap();
        assert!(server.cache.is_empty());
    }

    #[test]
    fn test_put_and_lookup() {
        let config = ServerConfig {
            node_id: 1,
            memory_pool_size: 1024 * 1024,
            ..Default::default()
        };
        let server = KvCacheServer::new(config).unwrap();

        // Put a value
        server
            .put_value(b"key1".to_vec(), b"value1".to_vec(), 0)
            .unwrap();

        // Verify it's in the cache
        assert!(server.cache.contains_key(&b"key1".to_vec()));

        // Check the value
        let entry = server.cache.get(&b"key1".to_vec()).unwrap();
        assert_eq!(entry.data, b"value1");
    }
}
