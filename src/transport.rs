//! RDMA Transport abstraction
//!
//! This module provides an abstraction over RDMA operations, with both
//! a mock implementation for testing and a real implementation using fabric-lib.

use crate::protocol::{DomainAddress, MemoryRegionDescriptor, MemoryRegionHandle};
use anyhow::{anyhow, Result};
use std::sync::Arc;
use std::ffi::c_void;
use std::ptr::NonNull;

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

    /// Register memory for RDMA access
    fn register_memory(
        &self,
        ptr: *mut u8,
        len: usize,
    ) -> Result<(MemoryRegionHandle, MemoryRegionDescriptor)>;

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
            #[cfg(feature = "rdma")]
            {
                Arc::new(FabricTransport::new(config.clone())?)
            }
            #[cfg(not(feature = "rdma"))]
            {
                tracing::error!("Real RDMA requested but binary was not compiled with 'rdma' feature");
                return Err(anyhow!(
                    "Real RDMA not available. Rebuild with '--features rdma' or use --mock true"
                ));
            }
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

    /// Register memory for RDMA access
    /// Returns (handle, descriptor) that can be used for transfers
    pub fn register_memory(
        &self,
        ptr: *mut u8,
        len: usize,
    ) -> Result<(MemoryRegionHandle, MemoryRegionDescriptor)> {
        self.inner.register_memory(ptr, len)
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

    fn register_memory(
        &self,
        ptr: *mut u8,
        len: usize,
    ) -> Result<(MemoryRegionHandle, MemoryRegionDescriptor)> {
        // Mock implementation: just create fake registration
        let handle = MemoryRegionHandle::new(ptr as u64, len);

        let addr_rkey_list: Vec<_> = self.domain_addresses
            .iter()
            .enumerate()
            .map(|(i, addr)| {
                (addr.clone(), crate::protocol::MemoryRegionRemoteKey(i as u64))
            })
            .collect();

        let descriptor = MemoryRegionDescriptor::new(ptr as u64, addr_rkey_list);

        Ok((handle, descriptor))
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

        // Validate pointers are non-null
        let src_ptr = (request.src_handle.ptr + request.src_offset) as *const u8;
        let dst_ptr = (request.dst_descriptor.ptr + request.dst_offset) as *mut u8;

        if src_ptr.is_null() || dst_ptr.is_null() {
            return Err(anyhow!(
                "Mock transfer failed: null pointer (src={:p}, dst={:p})",
                src_ptr, dst_ptr
            ));
        }

        // SAFETY: The mock transport only works when client and server are in the
        // same process (e.g., integration tests). When running as separate processes,
        // the destination pointer from the remote client is NOT valid in this address
        // space. Use real RDMA (use_mock: false) for cross-process transfers.
        //
        // We cannot easily validate the pointers are in our address space, so this
        // will segfault if used incorrectly across processes.
        tracing::warn!(
            "Mock transport performing memory copy - this only works when client \
             and server are in the SAME process. For separate processes, use real RDMA."
        );

        unsafe {
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

/// Real fabric-lib RDMA transport implementation
#[cfg(feature = "rdma")]
struct FabricTransport {
    config: TransportConfig,
    engine: Arc<fabric_lib::TransferEngine>,
    domain_addresses: Vec<DomainAddress>,
}

#[cfg(feature = "rdma")]
impl FabricTransport {
    fn new(config: TransportConfig) -> Result<Self> {
        use fabric_lib::{TransferEngine, RdmaEngine, Worker};

        tracing::info!("Initializing fabric-lib RDMA transport");

        // Try to get EFA domains directly (works for host memory without GPU affinity)
        let efa_domains = fabric_lib::detect_topology()
            .ok()
            .and_then(|topo| {
                if topo.is_empty() {
                    None
                } else {
                    Some(topo)
                }
            });

        let engine = if let Some(topology) = efa_domains {
            // GPU-aware topology found - use it
            tracing::info!("Using GPU-aware topology with {} groups", topology.len());
            Self::build_with_topology(config.num_domains, &topology[0])?
        } else {
            // No GPU topology - build for host memory only with EFA domains
            tracing::info!("No GPU topology found, building for host memory with EFA domains");
            Self::build_host_only(config.num_domains)?
        };

        let engine = Arc::new(engine);

        // Get domain addresses from the engine
        let domain_addresses = vec![
            DomainAddress(engine.main_address().0.to_vec())
        ];

        tracing::info!("Fabric-lib RDMA transport initialized successfully");

        Ok(Self {
            config,
            engine,
            domain_addresses,
        })
    }

    fn build_with_topology(
        num_domains: usize,
        topo_group: &fabric_lib::TopologyGroup
    ) -> Result<fabric_lib::TransferEngine> {
        use fabric_lib::TransferEngineBuilder;

        tracing::info!(
            "Building with GPU {}, NUMA {}, {} domains, {} CPUs",
            topo_group.cuda_device,
            topo_group.numa,
            topo_group.domains.len(),
            topo_group.cpus.len()
        );

        let num_domains = num_domains.min(topo_group.domains.len());
        let domains: Vec<_> = topo_group.domains.iter().take(num_domains).cloned().collect();

        if domains.is_empty() {
            return Err(anyhow!("No RDMA domains available"));
        }

        if topo_group.cpus.len() < 2 {
            return Err(anyhow!("Need at least 2 CPUs for worker threads"));
        }

        let pin_worker_cpu = topo_group.cpus[0];
        let pin_uvm_cpu = topo_group.cpus[1];

        let mut builder = TransferEngineBuilder::default();
        builder.add_gpu_domains(
            topo_group.cuda_device,
            domains,
            pin_worker_cpu,
            pin_uvm_cpu,
        );

        builder.build()
            .map_err(|e| anyhow!("Failed to build TransferEngine: {}", e))
    }

    fn build_host_only(num_domains: usize) -> Result<fabric_lib::TransferEngine> {
        use fabric_lib::TransferEngine;

        // Use fabric-lib's new_host_only() method for host memory transfers
        // without requiring GPU-NIC affinity
        tracing::info!("Building host-only TransferEngine with {} domains", num_domains);

        // Get CPUs from NUMA node 0 for pinning
        let cpus = Self::get_numa_cpus(0)?;
        if cpus.len() < 2 {
            return Err(anyhow!(
                "Need at least 2 CPUs on NUMA node 0 for worker threads. Found: {}",
                cpus.len()
            ));
        }

        let pin_worker_cpu = cpus[0];
        let pin_uvm_cpu = cpus[1];

        tracing::info!(
            "Using CPUs {} and {} for worker threads",
            pin_worker_cpu,
            pin_uvm_cpu
        );

        TransferEngine::new_host_only(num_domains, pin_worker_cpu, pin_uvm_cpu)
            .map_err(|e| anyhow!("Failed to create host-only TransferEngine: {}", e))
    }

    fn get_numa_cpus(numa_node: u8) -> Result<Vec<u16>> {
        let cpulist_path = format!("/sys/devices/system/node/node{}/cpulist", numa_node);
        let cpulist = std::fs::read_to_string(&cpulist_path)
            .map_err(|e| anyhow!("Failed to read {}: {}", cpulist_path, e))?;

        // Parse CPU list (format: "0-23,48-71" or "0,1,2,3")
        let mut cpus = Vec::new();
        for part in cpulist.trim().split(',') {
            if let Some((start, end)) = part.split_once('-') {
                let start: u16 = start.parse()
                    .map_err(|e| anyhow!("Failed to parse CPU range start: {}", e))?;
                let end: u16 = end.parse()
                    .map_err(|e| anyhow!("Failed to parse CPU range end: {}", e))?;
                cpus.extend(start..=end);
            } else {
                let cpu: u16 = part.parse()
                    .map_err(|e| anyhow!("Failed to parse CPU: {}", e))?;
                cpus.push(cpu);
            }
        }

        Ok(cpus)
    }


    /// Convert our MemoryRegionDescriptor to fabric-lib's format
    fn convert_mr_descriptor(desc: &MemoryRegionDescriptor) -> fabric_lib::api::MemoryRegionDescriptor {
        use fabric_lib::api::{MemoryRegionDescriptor as FabricMRD, SmallVec};
        use bytes::Bytes;

        let addr_rkey_list: SmallVec<(fabric_lib::api::DomainAddress, fabric_lib::api::MemoryRegionRemoteKey)> =
            desc.addr_rkey_list.iter()
                .map(|(addr, rkey)| {
                    (
                        fabric_lib::api::DomainAddress(Bytes::copy_from_slice(&addr.0)),
                        fabric_lib::api::MemoryRegionRemoteKey(rkey.0)
                    )
                })
                .collect();

        FabricMRD {
            ptr: desc.ptr,
            addr_rkey_list,
        }
    }

    /// Convert our MemoryRegionHandle to fabric-lib's format
    fn convert_mr_handle(handle: &MemoryRegionHandle) -> fabric_lib::api::MemoryRegionHandle {
        fabric_lib::api::MemoryRegionHandle::new(
            NonNull::new(handle.ptr as *mut c_void)
                .expect("Invalid memory region handle pointer")
        )
    }

    /// Convert our DomainRouting to fabric-lib's format
    fn convert_routing(routing: &DomainRouting) -> fabric_lib::api::DomainGroupRouting {
        use fabric_lib::api::DomainGroupRouting;
        use std::num::NonZeroU8;

        match routing {
            DomainRouting::RoundRobinSharded { num_shards } => {
                DomainGroupRouting::RoundRobinSharded {
                    num_shards: NonZeroU8::new(*num_shards).unwrap_or(NonZeroU8::new(1).unwrap())
                }
            }
            DomainRouting::Pinned { domain_idx } => {
                DomainGroupRouting::Pinned { domain_idx: *domain_idx }
            }
        }
    }
}

#[cfg(feature = "rdma")]
impl RdmaTransportTrait for FabricTransport {
    fn domain_addresses(&self) -> Vec<DomainAddress> {
        self.domain_addresses.clone()
    }

    fn register_memory(
        &self,
        ptr: *mut u8,
        len: usize,
    ) -> Result<(MemoryRegionHandle, MemoryRegionDescriptor)> {
        use fabric_lib::RdmaEngine;
        use cuda_lib::Device;

        let ptr_nonnull = NonNull::new(ptr as *mut c_void)
            .ok_or_else(|| anyhow!("Invalid memory pointer"))?;

        // Register memory with fabric-lib (Host memory, not GPU)
        let (fabric_handle, fabric_descriptor) = self.engine
            .register_memory_allow_remote(ptr_nonnull, len, Device::Host)
            .map_err(|e| anyhow!("Failed to register memory: {}", e))?;

        // Convert fabric-lib handle to our handle
        let handle = MemoryRegionHandle::new(fabric_handle.ptr.as_ptr() as u64, len);

        // Convert fabric-lib descriptor to our descriptor
        let addr_rkey_list: Vec<_> = fabric_descriptor.addr_rkey_list.iter()
            .map(|(addr, rkey)| {
                (
                    DomainAddress(addr.0.to_vec()),
                    crate::protocol::MemoryRegionRemoteKey(rkey.0)
                )
            })
            .collect();

        let descriptor = MemoryRegionDescriptor::new(
            fabric_descriptor.ptr,
            addr_rkey_list
        );

        Ok((handle, descriptor))
    }

    fn submit_transfer(&self, request: TransferRequest) -> Result<()> {
        use fabric_lib::api::{TransferRequest as FabricTR, SingleTransferRequest};

        let src_mr = Self::convert_mr_handle(&request.src_handle);
        let dst_mr = Self::convert_mr_descriptor(&request.dst_descriptor);
        let domain = Self::convert_routing(&request.routing);

        let fabric_request = FabricTR::Single(SingleTransferRequest {
            src_mr,
            src_offset: request.src_offset,
            length: request.length,
            imm_data: request.imm_data,
            dst_mr,
            dst_offset: request.dst_offset,
            domain,
        });

        let callback = fabric_lib::TransferCallback {
            on_done: Box::new(|| Ok(())),
            on_error: Box::new(|e| {
                tracing::error!("Transfer error: {}", e);
                Err(format!("Transfer error: {}", e))
            }),
        };

        self.engine.submit_transfer(fabric_request, callback)
            .map_err(|e| anyhow!("Failed to submit transfer: {}", e))
    }

    fn submit_transfer_async(
        &self,
        request: TransferRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<TransferResult>> + Send + '_>> {
        use fabric_lib::api::{TransferRequest as FabricTR, SingleTransferRequest};
        use fabric_lib::AsyncTransferEngine;

        Box::pin(async move {
            let src_mr = Self::convert_mr_handle(&request.src_handle);
            let dst_mr = Self::convert_mr_descriptor(&request.dst_descriptor);
            let domain = Self::convert_routing(&request.routing);
            let length = request.length;

            let fabric_request = FabricTR::Single(SingleTransferRequest {
                src_mr,
                src_offset: request.src_offset,
                length: request.length,
                imm_data: request.imm_data,
                dst_mr,
                dst_offset: request.dst_offset,
                domain,
            });

            match self.engine.submit_transfer_async(fabric_request).await {
                Ok(()) => Ok(TransferResult {
                    success: true,
                    bytes_transferred: length,
                    error: None,
                }),
                Err(e) => Ok(TransferResult {
                    success: false,
                    bytes_transferred: 0,
                    error: Some(format!("{}", e)),
                }),
            }
        })
    }

    fn poll_completion(&self) -> Option<TransferResult> {
        // fabric-lib handles completions internally via callbacks
        None
    }
}

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
