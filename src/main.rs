//! KV Cache with RDMA - Main entry point
//!
//! This crate implements a distributed KV cache that uses RDMA for data transfers.
//!
//! ## Architecture
//!
//! The system uses a push model where:
//! 1. Client registers its receive buffer with the server
//! 2. Client sends GET request via gRPC with response buffer location
//! 3. Server RDMA writes the value directly to client's buffer
//! 4. Server responds with success/failure via gRPC
//!
//! ## Usage
//!
//! Start the server:
//! ```bash
//! cargo run --bin kv-server -- --listen-addr [::1]:50051
//! ```
//!
//! Run the client:
//! ```bash
//! cargo run --bin kv-client -- put mykey myvalue
//! cargo run --bin kv-client -- get mykey
//! cargo run --bin kv-client -- repl
//! ```

fn main() {
    println!("KV Cache with RDMA");
    println!();
    println!("Use the following binaries:");
    println!("  cargo run --bin kv-server -- --help");
    println!("  cargo run --bin kv-client -- --help");
}
