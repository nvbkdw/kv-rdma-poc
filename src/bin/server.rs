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
    #[arg(long, default_value_t = false)]
    mock: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Number of worker threads for processing requests
    #[arg(long, default_value = "4")]
    worker_threads: usize,
}

async fn run_with_config(args: Args) -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&args.log_level)),
        )
        .init();

    let config = ServerConfig {
        node_id: args.node_id,
        listen_addr: args.listen_addr.clone(),
        memory_pool_size: args.memory_mb * 1024 * 1024,
        transport: TransportConfig {
            node_id: args.node_id,
            num_domains: args.num_domains,
            use_mock: args.mock,
        },
    };

    tracing::info!("=== KV Cache Server Configuration ===");
    tracing::info!("Worker threads: {}", args.worker_threads);
    tracing::info!("Listen address: {}", args.listen_addr);
    tracing::info!("Memory pool: {} MB", args.memory_mb);
    tracing::info!("Node ID: {}", args.node_id);
    tracing::info!("Transport: {}", if args.mock { "Mock" } else { "RDMA" });
    tracing::info!("======================================");

    run_server(config).await
}

fn main() -> Result<()> {
    let args = Args::parse();
    let worker_threads = args.worker_threads;

    // Build tokio runtime with specified number of worker threads
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()?
        .block_on(run_with_config(args))
}
