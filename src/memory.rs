//! Memory pool management for RDMA-registered buffers
//!
//! This module provides a simple memory pool that can be registered with RDMA
//! for zero-copy data transfers.

use crate::protocol::{DomainAddress, MemoryRegionDescriptor, MemoryRegionHandle, MemoryRegionRemoteKey};
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use std::collections::BTreeMap;

/// Configuration for the memory pool
#[derive(Clone, Debug)]
pub struct MemoryPoolConfig {
    /// Total size of the memory pool in bytes
    pub size: usize,
    /// Alignment for allocations (default: 4096 for page alignment)
    pub alignment: usize,
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            size: 1024 * 1024 * 1024, // 1GB default
            alignment: 4096,
        }
    }
}

/// A simple bump allocator for the memory pool
struct BumpAllocator {
    /// Current allocation offset
    offset: usize,
    /// Total capacity
    capacity: usize,
    /// Alignment requirement
    alignment: usize,
    /// Free list: offset -> size (for simple deallocation)
    free_list: BTreeMap<usize, usize>,
}

impl BumpAllocator {
    fn new(capacity: usize, alignment: usize) -> Self {
        Self {
            offset: 0,
            capacity,
            alignment,
            free_list: BTreeMap::new(),
        }
    }

    fn allocate(&mut self, size: usize) -> Option<usize> {
        // Try to find a suitable free block first
        let mut found_offset = None;
        for (&offset, &block_size) in &self.free_list {
            if block_size >= size {
                found_offset = Some(offset);
                break;
            }
        }

        if let Some(offset) = found_offset {
            let block_size = self.free_list.remove(&offset).unwrap();
            // If block is larger than needed, put remainder back
            if block_size > size {
                let remainder_offset = offset + size;
                let aligned_remainder = (remainder_offset + self.alignment - 1) & !(self.alignment - 1);
                let remainder_size = block_size - (aligned_remainder - offset);
                if remainder_size >= self.alignment {
                    self.free_list.insert(aligned_remainder, remainder_size);
                }
            }
            return Some(offset);
        }

        // Align the current offset
        let aligned_offset = (self.offset + self.alignment - 1) & !(self.alignment - 1);
        if aligned_offset + size > self.capacity {
            return None;
        }

        self.offset = aligned_offset + size;
        Some(aligned_offset)
    }

    fn deallocate(&mut self, offset: usize, size: usize) {
        // Simple strategy: just add to free list
        // A more sophisticated implementation would coalesce adjacent blocks
        self.free_list.insert(offset, size);
    }

    fn used(&self) -> usize {
        self.offset
    }

    fn available(&self) -> usize {
        self.capacity - self.offset + self.free_list.values().sum::<usize>()
    }
}

/// Memory pool for RDMA-registered buffers
pub struct MemoryPool {
    /// The actual memory buffer
    buffer: Vec<u8>,
    /// Memory region handle for local access
    handle: MemoryRegionHandle,
    /// Memory region descriptor for remote access
    descriptor: MemoryRegionDescriptor,
    /// Allocator state
    allocator: Mutex<BumpAllocator>,
}

impl MemoryPool {
    /// Create a new memory pool with the given configuration
    ///
    /// In a real implementation, this would:
    /// 1. Allocate page-aligned memory (or GPU memory)
    /// 2. Register it with the RDMA transport
    /// 3. Get the memory region descriptor for remote access
    pub fn new(config: MemoryPoolConfig, node_id: u32, domain_addresses: Vec<DomainAddress>) -> Result<Self> {
        // Allocate aligned buffer
        let mut buffer = vec![0u8; config.size];
        let ptr = buffer.as_mut_ptr() as u64;

        // Create handle for local access
        let handle = MemoryRegionHandle::new(ptr, config.size);

        // Create descriptor for remote access
        // In a real implementation, rkeys would come from RDMA registration
        let addr_rkey_list: Vec<_> = domain_addresses
            .into_iter()
            .enumerate()
            .map(|(i, addr)| (addr, MemoryRegionRemoteKey(i as u64)))
            .collect();
        let descriptor = MemoryRegionDescriptor::new(ptr, addr_rkey_list);

        let allocator = Mutex::new(BumpAllocator::new(config.size, config.alignment));

        Ok(Self {
            buffer,
            handle,
            descriptor,
            allocator,
        })
    }

    /// Allocate a region within the pool
    pub fn allocate(&self, size: usize) -> Result<PoolAllocation> {
        let offset = self
            .allocator
            .lock()
            .allocate(size)
            .ok_or_else(|| anyhow!("Memory pool exhausted"))?;

        Ok(PoolAllocation {
            offset,
            size,
            ptr: unsafe { self.buffer.as_ptr().add(offset) as *mut u8 },
        })
    }

    /// Deallocate a region
    pub fn deallocate(&self, allocation: &PoolAllocation) {
        self.allocator.lock().deallocate(allocation.offset, allocation.size);
    }

    /// Write data to a specific offset in the pool
    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<()> {
        if offset + data.len() > self.buffer.len() {
            return Err(anyhow!("Write exceeds pool bounds"));
        }
        self.buffer[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    /// Read data from a specific offset in the pool
    pub fn read(&self, offset: usize, len: usize) -> Result<&[u8]> {
        if offset + len > self.buffer.len() {
            return Err(anyhow!("Read exceeds pool bounds"));
        }
        Ok(&self.buffer[offset..offset + len])
    }

    /// Get the local memory region handle
    pub fn handle(&self) -> MemoryRegionHandle {
        self.handle
    }

    /// Get the memory region descriptor for remote access
    pub fn descriptor(&self) -> &MemoryRegionDescriptor {
        &self.descriptor
    }

    /// Get a pointer to the buffer at a specific offset
    pub fn ptr_at(&self, offset: usize) -> *const u8 {
        unsafe { self.buffer.as_ptr().add(offset) }
    }

    /// Get a mutable pointer to the buffer at a specific offset
    pub fn ptr_at_mut(&mut self, offset: usize) -> *mut u8 {
        unsafe { self.buffer.as_mut_ptr().add(offset) }
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        let alloc = self.allocator.lock();
        PoolStats {
            total: self.buffer.len(),
            used: alloc.used(),
            available: alloc.available(),
        }
    }

    /// Get a reference to the underlying buffer
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Get a mutable reference to the underlying buffer
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
}

/// Represents an allocation within the memory pool
#[derive(Debug)]
pub struct PoolAllocation {
    pub offset: usize,
    pub size: usize,
    pub ptr: *mut u8,
}

// PoolAllocation needs to be Send for async usage
unsafe impl Send for PoolAllocation {}
unsafe impl Sync for PoolAllocation {}

/// Memory pool statistics
#[derive(Clone, Debug)]
pub struct PoolStats {
    pub total: usize,
    pub used: usize,
    pub available: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool_allocation() {
        let config = MemoryPoolConfig {
            size: 4096,
            alignment: 64,
        };
        let pool = MemoryPool::new(config, 1, vec![]).unwrap();

        let alloc1 = pool.allocate(100).unwrap();
        assert_eq!(alloc1.offset, 0);
        assert_eq!(alloc1.size, 100);

        let alloc2 = pool.allocate(200).unwrap();
        assert!(alloc2.offset >= 100); // Should be after first allocation
    }

    #[test]
    fn test_memory_pool_write_read() {
        let config = MemoryPoolConfig {
            size: 4096,
            alignment: 64,
        };
        let mut pool = MemoryPool::new(config, 1, vec![]).unwrap();

        let data = b"Hello, RDMA!";
        pool.write(0, data).unwrap();

        let read_data = pool.read(0, data.len()).unwrap();
        assert_eq!(read_data, data);
    }
}
