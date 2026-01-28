//! RDMA Transport abstraction
//!
//! This module provides an abstraction over RDMA operations, with both
//! a mock implementation for testing and a real implementation using fabric-lib.

use crate::protocol::{DomainAddress, MemoryRegionDescriptor, MemoryRegionHandle};
use anyhow::Result;
use std::sync::Arc;

/// Configuration for the RDMA transport
#[derive(Clone, Debug)]
pub struct TransportConfig {
    /// Node ID for this transport instance
    pub node_id: u32,
    /// Number of NICs/domains to use
    pub num_domains: usize,
    /// Whether to use mock transport (for testing without RDMA hardware)
    pub use_mock: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            node_id: 0,
            num_domains: 1,
            use_mock: true,
        }
    }
}

/// Routing strategy for domain selection
#[derive(Clone, Debug)]
pub enum DomainRouting {
    /// Round-robin across domains, sharding by transfer size
    RoundRobinSharded { num_shards: u8 },
    /// Use a specific domain
    Pinned { domain_idx: u8 },
}

impl Default for DomainRouting {
    fn default() -> Self {
        Self::RoundRobinSharded { num_shards: 1 }
    }
}

/// Request for a single RDMA transfer
#[derive(Clone, Debug)]
pub struct TransferRequest {
    /// Source memory region handle (local)
    pub src_handle: MemoryRegionHandle,
    /// Source offset within the memory region
    pub src_offset: u64,
    /// Transfer length in bytes
    pub length: u64,
    /// Optional immediate data (for signaling)
    pub imm_data: Option<u32>,
    /// Destination memory region descriptor (remote)
    pub dst_descriptor: MemoryRegionDescriptor,
    /// Destination offset within the memory region
    pub dst_offset: u64,
    /// Domain routing strategy
    pub routing: DomainRouting,
}

/// Result of a transfer operation
#[derive(Clone, Debug)]
pub struct TransferResult {
    pub success: bool,
    pub bytes_transferred: u64,
    pub error: Option<String>,
}

/// Trait for RDMA transport implementations
pub trait RdmaTransportTrait: Send + Sync {
    /// Get the domain addresses for this transport
    fn domain_addresses(&self) -> Vec<DomainAddress>;

    /// Submit a transfer request
    fn submit_transfer(&self, request: TransferRequest) -> Result<()>;

    /// Submit a transfer and wait for completion (async)
    fn submit_transfer_async(
        &self,
        request: TransferRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<TransferResult>> + Send + '_>>;

    /// Poll for completion (non-blocking)
    fn poll_completion(&self) -> Option<TransferResult>;
}

/// RDMA Transport implementation
///
/// This wraps either a mock transport or a real fabric-lib transport.
pub struct RdmaTransport {
    inner: Arc<dyn RdmaTransportTrait>,
    config: TransportConfig,
}

impl RdmaTransport {
    /// Create a new RDMA transport with the given configuration
    pub fn new(config: TransportConfig) -> Result<Self> {
        let inner: Arc<dyn RdmaTransportTrait> = if config.use_mock {
            Arc::new(MockTransport::new(config.clone()))
        } else {
            // In a real implementation, this would create a fabric-lib transport
            // For now, fall back to mock
            tracing::warn!("Real RDMA transport not available, using mock");
            Arc::new(MockTransport::new(config.clone()))
        };

        Ok(Self { inner, config })
    }

    /// Get the domain addresses for this transport
    pub fn domain_addresses(&self) -> Vec<DomainAddress> {
        self.inner.domain_addresses()
    }

    /// Submit a transfer request
    pub fn submit_transfer(&self, request: TransferRequest) -> Result<()> {
        self.inner.submit_transfer(request)
    }

    /// Submit a transfer and wait for completion
    pub async fn submit_transfer_async(&self, request: TransferRequest) -> Result<TransferResult> {
        self.inner.submit_transfer_async(request).await
    }

    /// Get the node ID
    pub fn node_id(&self) -> u32 {
        self.config.node_id
    }
}

/// Mock transport for testing without RDMA hardware
struct MockTransport {
    config: TransportConfig,
    domain_addresses: Vec<DomainAddress>,
}

impl MockTransport {
    fn new(config: TransportConfig) -> Self {
        // Generate mock domain addresses
        let domain_addresses = (0..config.num_domains)
            .map(|i| {
                DomainAddress::new(format!("mock://node{}/domain{}", config.node_id, i).into_bytes())
            })
            .collect();

        Self {
            config,
            domain_addresses,
        }
    }
}

impl RdmaTransportTrait for MockTransport {
    fn domain_addresses(&self) -> Vec<DomainAddress> {
        self.domain_addresses.clone()
    }

    fn submit_transfer(&self, request: TransferRequest) -> Result<()> {
        // In the mock implementation, we simulate the transfer by copying memory
        // In a real RDMA implementation, this would initiate the RDMA write

        tracing::debug!(
            "Mock transfer: src_offset={}, dst_offset={}, length={}",
            request.src_offset,
            request.dst_offset,
            request.length
        );

        // Simulate memory copy (in real RDMA, this happens on the NIC)
        unsafe {
            let src_ptr = (request.src_handle.ptr + request.src_offset) as *const u8;
            let dst_ptr = (request.dst_descriptor.ptr + request.dst_offset) as *mut u8;
            std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, request.length as usize);
        }

        Ok(())
    }

    fn submit_transfer_async(
        &self,
        request: TransferRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<TransferResult>> + Send + '_>>
    {
        Box::pin(async move {
            // Simulate async transfer with a small delay
            tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;

            self.submit_transfer(request.clone())?;

            Ok(TransferResult {
                success: true,
                bytes_transferred: request.length,
                error: None,
            })
        })
    }

    fn poll_completion(&self) -> Option<TransferResult> {
        // Mock always completes immediately
        None
    }
}

// Real fabric-lib transport implementation would go here
// #[cfg(feature = "fabric-lib")]
// mod fabric_transport {
//     use fabric_lib::{TransferEngine, TransferEngineBuilder};
//     ...
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_transfer() {
        let config = TransportConfig {
            node_id: 1,
            num_domains: 2,
            use_mock: true,
        };
        let transport = RdmaTransport::new(config).unwrap();

        // Check domain addresses
        let addrs = transport.domain_addresses();
        assert_eq!(addrs.len(), 2);

        // Allocate source and destination buffers
        let src_data = vec![1u8, 2, 3, 4, 5];
        let mut dst_data = vec![0u8; 5];

        let src_handle = MemoryRegionHandle::new(src_data.as_ptr() as u64, src_data.len());
        let dst_descriptor = MemoryRegionDescriptor::new(dst_data.as_mut_ptr() as u64, vec![]);

        let request = TransferRequest {
            src_handle,
            src_offset: 0,
            length: 5,
            imm_data: None,
            dst_descriptor,
            dst_offset: 0,
            routing: DomainRouting::default(),
        };

        let result = transport.submit_transfer_async(request).await.unwrap();
        assert!(result.success);
        assert_eq!(result.bytes_transferred, 5);

        // Verify data was copied
        assert_eq!(dst_data, src_data);
    }
}
