//! Protocol types for distributed KV cache
//!
//! These types are designed to be compatible with the fabric-lib RDMA library
//! and can be serialized for network transmission.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Network address of an RDMA domain (NIC)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DomainAddress(pub Vec<u8>);

impl DomainAddress {
    pub fn new(addr: Vec<u8>) -> Self {
        Self(addr)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Remote key for RDMA memory access
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRegionRemoteKey(pub u64);

/// Descriptor for a memory region that can be accessed remotely via RDMA
///
/// This contains all information needed for a remote node to write to this memory region.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryRegionDescriptor {
    /// Base pointer of the memory region
    pub ptr: u64,
    /// Per-domain address and remote key pairs
    pub addr_rkey_list: SmallVec<[(DomainAddress, MemoryRegionRemoteKey); 4]>,
}

impl MemoryRegionDescriptor {
    pub fn new(ptr: u64, addr_rkey_list: Vec<(DomainAddress, MemoryRegionRemoteKey)>) -> Self {
        Self {
            ptr,
            addr_rkey_list: SmallVec::from_vec(addr_rkey_list),
        }
    }

    /// Get the first domain address (for simple single-NIC setups)
    pub fn first_domain(&self) -> Option<&DomainAddress> {
        self.addr_rkey_list.first().map(|(addr, _)| addr)
    }
}

/// Location where a value is stored or where to write a response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValueLocation {
    /// Node ID that owns this memory
    pub node_id: u32,
    /// RDMA memory region descriptor
    pub mr_descriptor: MemoryRegionDescriptor,
    /// Offset within the memory region
    pub offset: u64,
    /// Length of the value
    pub length: u64,
}

impl ValueLocation {
    pub fn new(
        node_id: u32,
        mr_descriptor: MemoryRegionDescriptor,
        offset: u64,
        length: u64,
    ) -> Self {
        Self {
            node_id,
            mr_descriptor,
            offset,
            length,
        }
    }
}

/// Handle to a locally registered memory region
#[derive(Clone, Copy, Debug)]
pub struct MemoryRegionHandle {
    pub ptr: u64,
    pub len: usize,
}

impl MemoryRegionHandle {
    pub fn new(ptr: u64, len: usize) -> Self {
        Self { ptr, len }
    }
}

/// Internal cache entry storing value and its location
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// The actual value data
    pub data: Vec<u8>,
    /// Offset within the server's memory pool where this is stored
    pub offset: u64,
    /// TTL in seconds (0 = no expiration)
    pub ttl_seconds: u64,
    /// Timestamp when entry was created
    pub created_at: std::time::Instant,
}

impl CacheEntry {
    pub fn new(data: Vec<u8>, offset: u64, ttl_seconds: u64) -> Self {
        Self {
            data,
            offset,
            ttl_seconds,
            created_at: std::time::Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        if self.ttl_seconds == 0 {
            return false;
        }
        self.created_at.elapsed().as_secs() >= self.ttl_seconds
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// Conversion helpers between our types and protobuf types
impl From<&crate::pb::MemoryRegionDescriptor> for MemoryRegionDescriptor {
    fn from(pb: &crate::pb::MemoryRegionDescriptor) -> Self {
        let addr_rkey_list = pb
            .addr_rkey_list
            .iter()
            .map(|item| {
                (
                    DomainAddress(item.domain_address.clone()),
                    MemoryRegionRemoteKey(item.rkey),
                )
            })
            .collect();
        Self {
            ptr: pb.ptr,
            addr_rkey_list,
        }
    }
}

impl From<&MemoryRegionDescriptor> for crate::pb::MemoryRegionDescriptor {
    fn from(desc: &MemoryRegionDescriptor) -> Self {
        Self {
            ptr: desc.ptr,
            addr_rkey_list: desc
                .addr_rkey_list
                .iter()
                .map(|(addr, rkey)| crate::pb::DomainAddressKey {
                    domain_address: addr.0.clone(),
                    rkey: rkey.0,
                })
                .collect(),
        }
    }
}

impl From<&crate::pb::ValueLocation> for ValueLocation {
    fn from(pb: &crate::pb::ValueLocation) -> Self {
        Self {
            node_id: pb.node_id,
            mr_descriptor: pb
                .mr_descriptor
                .as_ref()
                .map(|d| d.into())
                .unwrap_or_else(|| MemoryRegionDescriptor::new(0, vec![])),
            offset: pb.offset,
            length: pb.length,
        }
    }
}

impl From<&ValueLocation> for crate::pb::ValueLocation {
    fn from(loc: &ValueLocation) -> Self {
        Self {
            node_id: loc.node_id,
            mr_descriptor: Some((&loc.mr_descriptor).into()),
            offset: loc.offset,
            length: loc.length,
        }
    }
}
