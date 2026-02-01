//! KV Cache Read Throughput Benchmark
//!
//! This benchmark tests the read throughput of the KV cache server.
//! It first writes random keys with a specified value size using a single thread,
//! then reads those keys using multiple threads to measure read throughput.
//!
//! Run with: cargo run --bin kv-bench -- --help

use anyhow::Result;
use clap::Parser;
use kv_rdma_poc::client::{ClientConfig, KvCacheClient};
use kv_rdma_poc::transport::TransportConfig;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

#[derive(Parser, Debug, Clone)]
#[command(name = "kv-bench")]
#[command(about = "KV Cache Read Throughput Benchmark")]
struct Args {
    /// Server address (gRPC endpoint)
    #[arg(long, default_value = "http://[::1]:50051")]
    server_addr: String,

    /// Number of keys to write and read
    #[arg(long, default_value = "1000")]
    num_keys: usize,

    /// Value size in bytes (supports suffixes: KB, MB, e.g., 16KB, 1MB)
    #[arg(long, default_value = "64KB")]
    value_size: String,

    /// Number of concurrent workers (tokio tasks, not OS threads)
    #[arg(long, default_value = "16")]
    num_workers: usize,

    /// Number of RDMA clients to create (OS threads fixed at 4)
    #[arg(long, default_value = "4")]
    num_clients: usize,

    /// Receive buffer size per client in MB
    #[arg(long, default_value = "64")]
    buffer_mb: usize,

    /// Use mock transport (only works in same process, not for separate server)
    #[arg(long, default_value_t = false)]
    mock: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Base client ID (each thread gets a unique ID starting from this)
    #[arg(long, default_value = "100")]
    base_client_id: u32,

    /// TTL for written keys in seconds (0 = no expiration)
    #[arg(long, default_value = "300")]
    ttl: u64,

    /// Warmup iterations before actual benchmark
    #[arg(long, default_value = "100")]
    warmup: usize,

    /// Number of times each worker repeats reading its assigned keys
    #[arg(long, default_value = "100")]
    repeat_reads: usize,
}

/// Parse value size string like "16KB", "1MB", etc.
fn parse_size(s: &str) -> Result<usize> {
    let s = s.trim().to_uppercase();

    if let Some(stripped) = s.strip_suffix("KB") {
        Ok(stripped.parse::<usize>()? * 1024)
    } else if let Some(stripped) = s.strip_suffix("MB") {
        Ok(stripped.parse::<usize>()? * 1024 * 1024)
    } else if let Some(stripped) = s.strip_suffix("GB") {
        Ok(stripped.parse::<usize>()? * 1024 * 1024 * 1024)
    } else if let Some(stripped) = s.strip_suffix('B') {
        Ok(stripped.parse::<usize>()?)
    } else {
        // Assume bytes if no suffix
        Ok(s.parse::<usize>()?)
    }
}

/// Format size in human-readable form
fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Format throughput in human-readable form
fn format_throughput(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GB/s", bytes_per_sec / (1024.0 * 1024.0 * 1024.0))
    } else if bytes_per_sec >= 1024.0 * 1024.0 {
        format!("{:.2} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.2} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.2} B/s", bytes_per_sec)
    }
}

/// Create a client with the given ID
async fn create_client(args: &Args, client_id: u32) -> Result<KvCacheClient> {
    let config = ClientConfig {
        client_id,
        server_addr: args.server_addr.clone(),
        receive_buffer_size: args.buffer_mb * 1024 * 1024,
        transport: TransportConfig {
            node_id: client_id,
            num_domains: 1,
            use_mock: args.mock,
        },
    };

    let client = KvCacheClient::new(config)?;
    client.connect().await?;
    Ok(client)
}

/// Write phase: single thread writes all keys
async fn write_phase(
    args: &Args,
    value_size: usize,
    keys: &[String],
) -> Result<Duration> {
    println!("\n=== Write Phase ===");
    println!("Writing {} keys with {} values...", args.num_keys, format_size(value_size));

    let client = create_client(args, args.base_client_id).await?;

    // Create random value
    let value: Vec<u8> = (0..value_size).map(|i| (i % 256) as u8).collect();

    let start = Instant::now();

    for (i, key) in keys.iter().enumerate() {
        client.put(key.as_bytes(), &value, args.ttl).await?;

        if (i + 1) % 100 == 0 {
            print!("\rWrote {}/{} keys...", i + 1, args.num_keys);
            std::io::Write::flush(&mut std::io::stdout())?;
        }
    }

    let duration = start.elapsed();
    println!("\rWrote {}/{} keys.", args.num_keys, args.num_keys);

    let ops_per_sec = args.num_keys as f64 / duration.as_secs_f64();
    let bytes_per_sec = (args.num_keys * value_size) as f64 / duration.as_secs_f64();

    println!("Write completed in {:.2}s", duration.as_secs_f64());
    println!("Write throughput: {:.0} ops/sec, {}", ops_per_sec, format_throughput(bytes_per_sec));

    Ok(duration)
}

/// Warmup phase: warm up client connections
async fn warmup_phase(args: &Args, keys: &[String], clients: &[Arc<KvCacheClient>]) -> Result<()> {
    if args.warmup == 0 {
        return Ok(());
    }

    println!("\n=== Warmup Phase ===");
    println!("Running {} warmup iterations with {} clients...", args.warmup, clients.len());

    let keys = Arc::new(keys.to_vec());
    let mut tasks = JoinSet::new();
    let num_clients = clients.len();
    let warmup_per_client = args.warmup / num_clients;

    for (client_idx, client) in clients.iter().enumerate() {
        let client = Arc::clone(client);
        let keys = Arc::clone(&keys);

        tasks.spawn(async move {
            for i in 0..warmup_per_client {
                let key_idx = (client_idx + i * num_clients) % keys.len();
                let _ = client.get(keys[key_idx].as_bytes()).await?;
            }
            Ok::<_, anyhow::Error>(())
        });
    }

    while let Some(result) = tasks.join_next().await {
        result??;
    }

    println!("Warmup completed");
    Ok(())
}

/// Read phase: multiple workers read all keys using a pool of clients
async fn read_phase(
    args: &Args,
    value_size: usize,
    keys: &[String],
    clients: &[Arc<KvCacheClient>],
) -> Result<Duration> {
    let total_operations = args.num_keys * args.repeat_reads;

    println!("\n=== Read Phase ===");
    if args.repeat_reads > 1 {
        println!("Reading {} keys {} times each ({} total operations) with {} workers using {} clients...",
            args.num_keys, args.repeat_reads, total_operations, args.num_workers, clients.len());
    } else {
        println!("Reading {} keys with {} workers using {} clients...",
            args.num_keys, args.num_workers, clients.len());
    }

    let keys = Arc::new(keys.to_vec());
    let mut tasks = JoinSet::new();
    let errors = Arc::new(AtomicU64::new(0));

    let start = Instant::now();

    // Spawn worker tasks (each worker shares a client from the pool)
    for worker_id in 0..args.num_workers {
        let client = Arc::clone(&clients[worker_id % clients.len()]);
        let keys = Arc::clone(&keys);
        let errors = Arc::clone(&errors);
        let num_workers = args.num_workers;
        let num_keys = args.num_keys;
        let repeat_reads = args.repeat_reads;

        tasks.spawn(async move {
            // Each worker reads a portion of the keys
            let keys_per_worker = num_keys / num_workers;
            let start_idx = worker_id * keys_per_worker;
            let end_idx = if worker_id == num_workers - 1 {
                num_keys // Last worker handles remainder
            } else {
                start_idx + keys_per_worker
            };

            let mut worker_ops: u64 = 0;
            let mut worker_errors: u64 = 0;

            // Repeat reading the same keys multiple times
            for _repeat in 0..repeat_reads {
                for idx in start_idx..end_idx {
                    match client.get(keys[idx].as_bytes()).await {
                        Ok(value) => {
                            if value.len() != value_size {
                                tracing::warn!(
                                    "Worker {}: Expected {} bytes, got {}",
                                    worker_id,
                                    value_size,
                                    value.len()
                                );
                            }
                            worker_ops += 1;
                        }
                        Err(e) => {
                            tracing::error!("Worker {}: GET error: {}", worker_id, e);
                            worker_errors += 1;
                        }
                    }
                }
            }

            errors.fetch_add(worker_errors, Ordering::Relaxed);

            Ok::<(u64, u64), anyhow::Error>((worker_ops, worker_errors))
        });
    }

    // Collect results
    let mut total_ops: u64 = 0;
    let mut total_errors: u64 = 0;

    while let Some(result) = tasks.join_next().await {
        match result? {
            Ok((ops, errs)) => {
                total_ops += ops;
                total_errors += errs;
            }
            Err(e) => {
                eprintln!("\nWorker error: {}", e);
                total_errors += 1;
            }
        }
    }

    let duration = start.elapsed();

    let ops_per_sec = total_ops as f64 / duration.as_secs_f64();
    let bytes_per_sec = (total_ops * value_size as u64) as f64 / duration.as_secs_f64();

    println!("Read completed: {} operations ({} keys × {} repeats) in {:.2}s",
        total_ops, args.num_keys, args.repeat_reads, duration.as_secs_f64());
    println!("Read throughput: {:.0} ops/sec, {}", ops_per_sec, format_throughput(bytes_per_sec));

    if total_errors > 0 {
        println!("Errors: {}", total_errors);
    }

    Ok(duration)
}

/// Delete phase: delete all keys created during write phase
async fn delete_phase(
    args: &Args,
    keys: &[String],
    clients: &[Arc<KvCacheClient>],
) -> Result<Duration> {
    println!("\n=== Delete Phase ===");
    println!("Deleting {} keys with {} workers using {} clients...",
        args.num_keys, args.num_workers, clients.len());

    let keys = Arc::new(keys.to_vec());
    let mut tasks = JoinSet::new();
    let completed_ops = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));

    let start = Instant::now();

    // Spawn worker tasks to delete keys
    for worker_id in 0..args.num_workers {
        let client = Arc::clone(&clients[worker_id % clients.len()]);
        let keys = Arc::clone(&keys);
        let completed_ops = Arc::clone(&completed_ops);
        let errors = Arc::clone(&errors);
        let num_workers = args.num_workers;
        let num_keys = args.num_keys;

        tasks.spawn(async move {
            // Each worker deletes a portion of the keys
            let keys_per_worker = num_keys / num_workers;
            let start_idx = worker_id * keys_per_worker;
            let end_idx = if worker_id == num_workers - 1 {
                num_keys // Last worker handles remainder
            } else {
                start_idx + keys_per_worker
            };

            let mut worker_ops: u64 = 0;
            let mut worker_errors: u64 = 0;

            for idx in start_idx..end_idx {
                match client.delete(keys[idx].as_bytes()).await {
                    Ok(_existed) => {
                        worker_ops += 1;
                    }
                    Err(e) => {
                        tracing::error!("Worker {}: DELETE error: {}", worker_id, e);
                        worker_errors += 1;
                    }
                }

                // Report progress periodically
                let total_completed = completed_ops.fetch_add(1, Ordering::Relaxed) + 1;
                if total_completed % 100 == 0 {
                    print!("\rDeleted {}/{} keys...", total_completed, num_keys);
                    std::io::Write::flush(&mut std::io::stdout())?;
                }
            }

            errors.fetch_add(worker_errors, Ordering::Relaxed);

            Ok::<(u64, u64), anyhow::Error>((worker_ops, worker_errors))
        });
    }

    // Collect results
    let mut total_ops: u64 = 0;
    let mut total_errors: u64 = 0;

    while let Some(result) = tasks.join_next().await {
        match result? {
            Ok((ops, errs)) => {
                total_ops += ops;
                total_errors += errs;
            }
            Err(e) => {
                eprintln!("\nWorker error: {}", e);
                total_errors += 1;
            }
        }
    }

    let duration = start.elapsed();
    println!("\rDeleted {}/{} keys.", total_ops, args.num_keys);

    let ops_per_sec = total_ops as f64 / duration.as_secs_f64();

    println!("Delete completed in {:.2}s", duration.as_secs_f64());
    println!("Delete throughput: {:.0} ops/sec", ops_per_sec);

    if total_errors > 0 {
        println!("Errors: {}", total_errors);
    }

    Ok(duration)
}

/// Run latency analysis: measure individual operation latencies
async fn latency_analysis(
    args: &Args,
    _value_size: usize,
    keys: &[String],
    num_samples: usize,
) -> Result<()> {
    println!("\n=== Latency Analysis ===");
    println!("Measuring latency for {} random GET operations...", num_samples);

    let client = create_client(args, args.base_client_id + 1000).await?;
    let mut latencies = Vec::with_capacity(num_samples);

    for i in 0..num_samples {
        let key_idx = i % keys.len();
        let start = Instant::now();
        client.get(keys[key_idx].as_bytes()).await?;
        let latency = start.elapsed();
        latencies.push(latency);
    }

    // Calculate statistics
    latencies.sort();
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];
    let median = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() * 95) / 100];
    let p99 = latencies[(latencies.len() * 99) / 100];
    let avg = latencies.iter().sum::<Duration>() / latencies.len() as u32;

    println!("Latency statistics (microseconds):");
    println!("  Min:    {:8.2} µs", min.as_micros());
    println!("  Median: {:8.2} µs", median.as_micros());
    println!("  Avg:    {:8.2} µs", avg.as_micros());
    println!("  P95:    {:8.2} µs", p95.as_micros());
    println!("  P99:    {:8.2} µs", p99.as_micros());
    println!("  Max:    {:8.2} µs", max.as_micros());

    Ok(())
}

#[tokio::main(worker_threads = 4)]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&args.log_level)),
        )
        .init();

    let value_size = parse_size(&args.value_size)?;

    println!("==============================================");
    println!("KV Cache Read Throughput Benchmark");
    println!("==============================================");
    println!("Server:             {}", args.server_addr);
    println!("Keys:               {}", args.num_keys);
    println!("Value size:         {}", format_size(value_size));
    println!("Repeat reads:       {} (total ops: {})", args.repeat_reads, args.num_keys * args.repeat_reads);
    println!("Concurrent workers: {}", args.num_workers);
    println!("RDMA clients:       {} (OS threads: 4)", args.num_clients);
    println!("Buffer/client:      {} MB", args.buffer_mb);
    println!("Transport:          {}", if args.mock { "Mock (same process only)" } else { "Real RDMA" });
    println!("==============================================");

    if args.mock {
        println!("\nWARNING: Mock transport only works when server runs in same process.");
        println!("For separate server process, use --mock false (requires RDMA hardware)");
        println!("Or run integration tests instead: cargo test\n");
    }

    if args.num_clients > 8 {
        println!("\nWARNING: High client count ({}) may exhaust RDMA endpoint resources.", args.num_clients);
        println!("If you see 'Cannot allocate memory' errors, reduce --num-clients");
        println!("Recommended: 2-4 clients for RDMA workloads\n");
    }

    // Generate keys
    let keys: Vec<String> = (0..args.num_keys)
        .map(|i| format!("bench_key_{:08}", i))
        .collect();

    // Phase 1: Write all keys
    let write_duration = write_phase(&args, value_size, &keys).await?;

    // Phase 2: Create client pool
    println!("\n=== Creating Client Pool ===");
    println!("Creating {} RDMA clients...", args.num_clients);
    let mut clients: Vec<Arc<KvCacheClient>> = Vec::with_capacity(args.num_clients);
    for client_id in 0..args.num_clients {
        let client = create_client(&args, args.base_client_id + client_id as u32 + 1).await?;
        clients.push(Arc::new(client));
        print!("\rCreated {}/{} clients...", client_id + 1, args.num_clients);
        std::io::Write::flush(&mut std::io::stdout())?;
    }
    println!("\rCreated {}/{} clients.", args.num_clients, args.num_clients);

    // Phase 3: Warmup
    warmup_phase(&args, &keys, &clients).await?;

    // Phase 4: Read all keys with multiple workers
    let read_duration = read_phase(&args, value_size, &keys, &clients).await?;

    // Phase 5: Latency analysis
    latency_analysis(&args, value_size, &keys, 100.min(args.num_keys)).await?;

    // Phase 6: Delete all keys
    let delete_duration = delete_phase(&args, &keys, &clients).await?;

    // Summary
    let total_read_ops = (args.num_keys * args.repeat_reads) as f64;
    let read_ops_per_sec = total_read_ops / read_duration.as_secs_f64();

    println!("\n=== Summary ===");
    println!("Write:  {:.2}s, {:.0} ops/sec",
        write_duration.as_secs_f64(),
        args.num_keys as f64 / write_duration.as_secs_f64()
    );

    if args.repeat_reads > 1 {
        println!("Read:   {:.2}s, {:.0} ops/sec ({} keys × {} repeats = {:.0} total ops)",
            read_duration.as_secs_f64(),
            read_ops_per_sec,
            args.num_keys,
            args.repeat_reads,
            total_read_ops
        );
        println!("        Speedup: {:.1}x vs single-threaded write ({} workers)",
            (total_read_ops / read_duration.as_secs_f64()) / (args.num_keys as f64 / write_duration.as_secs_f64()),
            args.num_workers
        );
    } else {
        println!("Read:   {:.2}s, {:.0} ops/sec ({:.1}x speedup with {} workers)",
            read_duration.as_secs_f64(),
            read_ops_per_sec,
            write_duration.as_secs_f64() / read_duration.as_secs_f64(),
            args.num_workers
        );
    }

    println!("Delete: {:.2}s, {:.0} ops/sec",
        delete_duration.as_secs_f64(),
        args.num_keys as f64 / delete_duration.as_secs_f64()
    );

    let total_data = (args.num_keys * args.repeat_reads * value_size) as f64;
    println!("Total data read: {}", format_size(total_data as usize));
    println!("Read throughput: {}", format_throughput(total_data / read_duration.as_secs_f64()));

    Ok(())
}
