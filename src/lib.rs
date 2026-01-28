pub mod client;
pub mod memory;
pub mod protocol;
pub mod server;
pub mod transport;

// Re-export generated protobuf types
pub mod pb {
    tonic::include_proto!("kv_cache");
}

pub use client::KvCacheClient;
pub use protocol::{MemoryRegionDescriptor, ValueLocation};
pub use server::KvCacheServer;
pub use transport::{RdmaTransport, TransportConfig};
