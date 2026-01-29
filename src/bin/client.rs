//! KV Cache Client binary
//!
//! Run with: cargo run --bin kv-client -- --help

use anyhow::Result;
use clap::{Parser, Subcommand};
use kv_rdma_poc::client::{ClientConfig, KvCacheClient};
use kv_rdma_poc::transport::TransportConfig;

#[derive(Parser, Debug)]
#[command(name = "kv-client")]
#[command(about = "Distributed KV Cache Client with RDMA support")]
struct Args {
    /// Client node ID
    #[arg(long, default_value = "1")]
    client_id: u32,

    /// Server address (gRPC endpoint)
    #[arg(long, default_value = "http://[::1]:50051")]
    server_addr: String,

    /// Receive buffer size in MB
    #[arg(long, default_value = "64")]
    buffer_mb: usize,

    /// Use mock transport (for testing without RDMA hardware)
    #[arg(long, default_value_t = false)]
    mock: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Get a value from the cache
    Get {
        /// Key to get
        key: String,
    },
    /// Put a value into the cache
    Put {
        /// Key to set
        key: String,
        /// Value to set
        value: String,
        /// TTL in seconds (0 = no expiration)
        #[arg(long, default_value = "0")]
        ttl: u64,
    },
    /// Delete a value from the cache
    Delete {
        /// Key to delete
        key: String,
    },
    /// Run interactive REPL
    Repl,
    /// Run benchmark
    Bench {
        /// Number of operations
        #[arg(long, default_value = "1000")]
        ops: usize,
        /// Value size in bytes
        #[arg(long, default_value = "1024")]
        value_size: usize,
    },
}

async fn run_client(args: &Args) -> Result<KvCacheClient> {
    let config = ClientConfig {
        client_id: args.client_id,
        server_addr: args.server_addr.clone(),
        receive_buffer_size: args.buffer_mb * 1024 * 1024,
        transport: TransportConfig {
            node_id: args.client_id,
            num_domains: 1,
            use_mock: args.mock,
        },
    };

    let client = KvCacheClient::new(config)?;
    client.connect().await?;
    Ok(client)
}

async fn cmd_get(client: &KvCacheClient, key: &str) -> Result<()> {
    match client.get(key.as_bytes()).await {
        Ok(value) => {
            match String::from_utf8(value.clone()) {
                Ok(s) => println!("{}", s),
                Err(_) => println!("{:?}", value),
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
    Ok(())
}

async fn cmd_put(client: &KvCacheClient, key: &str, value: &str, ttl: u64) -> Result<()> {
    match client.put(key.as_bytes(), value.as_bytes(), ttl).await {
        Ok(()) => println!("OK"),
        Err(e) => eprintln!("Error: {}", e),
    }
    Ok(())
}

async fn cmd_delete(client: &KvCacheClient, key: &str) -> Result<()> {
    match client.delete(key.as_bytes()).await {
        Ok(existed) => {
            if existed {
                println!("Deleted");
            } else {
                println!("Key not found");
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
    Ok(())
}

async fn cmd_repl(client: &KvCacheClient) -> Result<()> {
    use std::io::{self, BufRead, Write};

    println!("KV Cache REPL - Commands: get <key>, put <key> <value>, delete <key>, quit");
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }

        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "get" => {
                if parts.len() < 2 {
                    println!("Usage: get <key>");
                    continue;
                }
                cmd_get(client, parts[1]).await?;
            }
            "put" => {
                if parts.len() < 3 {
                    println!("Usage: put <key> <value>");
                    continue;
                }
                cmd_put(client, parts[1], parts[2], 0).await?;
            }
            "delete" | "del" => {
                if parts.len() < 2 {
                    println!("Usage: delete <key>");
                    continue;
                }
                cmd_delete(client, parts[1]).await?;
            }
            "stats" => {
                let stats = client.memory_stats();
                println!(
                    "Memory: total={} MB, used={} MB, available={} MB",
                    stats.total / 1024 / 1024,
                    stats.used / 1024 / 1024,
                    stats.available / 1024 / 1024
                );
            }
            "quit" | "exit" | "q" => {
                println!("Bye!");
                break;
            }
            _ => {
                println!("Unknown command: {}", parts[0]);
            }
        }
    }

    Ok(())
}

async fn cmd_bench(client: &KvCacheClient, ops: usize, value_size: usize) -> Result<()> {
    use std::time::Instant;

    let value = vec![b'x'; value_size];

    println!(
        "Running benchmark: {} ops, {} byte values",
        ops, value_size
    );

    // PUT benchmark
    let start = Instant::now();
    for i in 0..ops {
        let key = format!("bench_key_{}", i);
        client.put(key.as_bytes(), &value, 0).await?;
    }
    let put_duration = start.elapsed();
    let put_ops_per_sec = ops as f64 / put_duration.as_secs_f64();
    println!(
        "PUT: {} ops in {:.2}s = {:.0} ops/sec",
        ops,
        put_duration.as_secs_f64(),
        put_ops_per_sec
    );

    // GET benchmark
    let start = Instant::now();
    for i in 0..ops {
        let key = format!("bench_key_{}", i);
        let _ = client.get(key.as_bytes()).await?;
    }
    let get_duration = start.elapsed();
    let get_ops_per_sec = ops as f64 / get_duration.as_secs_f64();
    println!(
        "GET: {} ops in {:.2}s = {:.0} ops/sec",
        ops,
        get_duration.as_secs_f64(),
        get_ops_per_sec
    );

    // DELETE benchmark
    let start = Instant::now();
    for i in 0..ops {
        let key = format!("bench_key_{}", i);
        let _ = client.delete(key.as_bytes()).await?;
    }
    let delete_duration = start.elapsed();
    let delete_ops_per_sec = ops as f64 / delete_duration.as_secs_f64();
    println!(
        "DELETE: {} ops in {:.2}s = {:.0} ops/sec",
        ops,
        delete_duration.as_secs_f64(),
        delete_ops_per_sec
    );

    Ok(())
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

    let client = run_client(&args).await?;

    match &args.command {
        Commands::Get { key } => cmd_get(&client, key).await?,
        Commands::Put { key, value, ttl } => cmd_put(&client, key, value, *ttl).await?,
        Commands::Delete { key } => cmd_delete(&client, key).await?,
        Commands::Repl => cmd_repl(&client).await?,
        Commands::Bench { ops, value_size } => cmd_bench(&client, *ops, *value_size).await?,
    }

    Ok(())
}
