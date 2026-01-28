//! KV Cache Server binary
//!
//! Run with: cargo run --bin kv-server -- --help

use anyhow::Result;
use clap::Parser;
use kv_rdma_poc::server::{run_server, ServerConfig};
use kv_rdma_poc::transport::TransportConfig;

#[derive(Parser, Debug)]
#[command(name = "kv-server")]
#[command(about = "Distributed KV Cache Server with RDMA support")]
struct Args {
    /// Server node ID
    #[arg(long, default_value = "0")]
    node_id: u32,

    /// gRPC listen address
    #[arg(long, default_value = "[::1]:50051")]
    listen_addr: String,

    /// Memory pool size in MB
    #[arg(long, default_value = "1024")]
    memory_mb: usize,

    /// Number of RDMA domains/NICs to use
    #[arg(long, default_value = "1")]
    num_domains: usize,

    /// Use mock transport (for testing without RDMA hardware)
    #[arg(long, default_value = "true")]
    mock: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&args.log_level)),
        )
        .init();

    let config = ServerConfig {
        node_id: args.node_id,
        listen_addr: args.listen_addr,
        memory_pool_size: args.memory_mb * 1024 * 1024,
        transport: TransportConfig {
            node_id: args.node_id,
            num_domains: args.num_domains,
            use_mock: args.mock,
        },
    };

    tracing::info!("Starting KV cache server with config: {:?}", config);

    run_server(config).await
}
