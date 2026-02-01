# KV Cache Read Throughput Benchmark

This benchmark tool tests the read throughput of the KV cache server with configurable value sizes and multiple reader threads.

## Overview

The benchmark runs in the following phases:

1. **Write Phase**: Single thread writes all keys with random values of specified size
2. **Warmup Phase**: All reader threads perform warmup reads to establish connections
3. **Read Phase**: Multiple threads read all keys concurrently to measure throughput
4. **Latency Analysis**: Measures individual GET operation latencies (min, median, avg, p95, p99, max)

## Building

```bash
# Easy way: Use the build helper script (sets required environment variables)
./build-with-rdma.sh bench    # Build benchmark only
./build-with-rdma.sh all      # Build all binaries

# Manual way: Set environment variables and build
GDRAPI_HOME=/path/to/gdrcopy \
LIBFABRIC_HOME=/path/to/libfabric \
cargo build --release --bin kv-bench --features rdma
```

**IMPORTANT**: The benchmark requires real RDMA hardware when running against a separate server process. The mock transport only works within integration tests (same process).

**Build Requirements**:
- RDMA hardware (EFA adapters)
- GDRAPI and libfabric libraries installed
- Environment variables: `GDRAPI_HOME` and `LIBFABRIC_HOME`

## Usage

### Prerequisites

**You must have RDMA hardware and build with the `rdma` feature:**

```bash
# Step 1: Build with RDMA support
./build-with-rdma.sh all

# Step 2: Start server
./run-with-rdma.sh ./target/release/kv-server

# Step 3: Run benchmark
./run-with-rdma.sh ./target/release/kv-bench
```

**OR** for development without RDMA hardware, use the integration tests instead:
```bash
# Integration tests use mock transport within the same process
cargo test
```

### Basic Usage

```bash
# Run with defaults (1000 keys, 64KB values, 4 threads)
./run-with-rdma.sh ./target/release/kv-bench

# The benchmark connects to server at http://[::1]:50051 by default
```

### Configurable Parameters

```bash
# Specify number of keys
./run-with-rdma.sh ./target/release/kv-bench --num-keys 10000

# Specify value size (supports B, KB, MB, GB suffixes)
./run-with-rdma.sh ./target/release/kv-bench --value-size 16KB
./run-with-rdma.sh ./target/release/kv-bench --value-size 1MB
./run-with-rdma.sh ./target/release/kv-bench --value-size 10MB

# Specify number of reader threads (2-4 recommended to avoid resource limits)
./run-with-rdma.sh ./target/release/kv-bench --num-threads 4

# Connect to remote server
./run-with-rdma.sh ./target/release/kv-bench --server-addr "http://192.168.1.10:50051"

# Increase receive buffer per client
./run-with-rdma.sh ./target/release/kv-bench --buffer-mb 128

# Adjust warmup iterations
./run-with-rdma.sh ./target/release/kv-bench --warmup 200

# Combine multiple options
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 5000 \
  --value-size 1MB \
  --num-threads 8 \
  --buffer-mb 128
```

### Command-Line Options

- `--server-addr`: Server gRPC endpoint (default: `http://[::1]:50051`)
- `--num-keys`: Number of keys to write and read (default: 1000)
- `--value-size`: Size of each value with suffix support (default: `64KB`)
  - Examples: `16KB`, `1MB`, `10MB`, `1024` (bytes)
- `--num-threads`: Number of concurrent reader threads (default: 2, **recommended: 2-4** to avoid resource exhaustion)
- `--buffer-mb`: Receive buffer size per client in MB (default: 64)
- `--mock`: Use mock transport (default: false, **only works in integration tests**)
- `--log-level`: Logging level: trace, debug, info, warn, error (default: info)
- `--base-client-id`: Starting client ID for thread identification (default: 100)
- `--ttl`: TTL for written keys in seconds, 0 = no expiration (default: 300)
- `--warmup`: Number of warmup iterations (default: 100)

## Example Output

```
==============================================
KV Cache Read Throughput Benchmark
==============================================
Server:       http://[::1]:50051
Keys:         1000
Value size:   64.00 KB
Read threads: 4
Buffer/client: 64 MB
Transport:    Real RDMA
==============================================

=== Write Phase ===
Writing 1000 keys with 64.00 KB values...
Wrote 1000/1000 keys.
Write completed in 2.45s
Write throughput: 408 ops/sec, 25.48 MB/s

=== Warmup Phase ===
Running 100 warmup iterations with 4 threads...
Warmup completed

=== Read Phase ===
Reading 1000 keys with 4 threads...
Read 1000/1000 keys.
Read completed in 0.68s
Read throughput: 1471 ops/sec, 91.94 MB/s

=== Latency Analysis ===
Measuring latency for 100 random GET operations...
Latency statistics (microseconds):
  Min:       245.12 µs
  Median:    512.34 µs
  Avg:       523.45 µs
  P95:       892.67 µs
  P99:      1145.23 µs
  Max:      1256.78 µs

=== Summary ===
Write: 2.45s, 408 ops/sec
Read:  0.68s, 1471 ops/sec (3.6x speedup with 4 threads)
Total data transferred: 64.00 MB
Read throughput: 91.94 MB/s
```

## Benchmark Scenarios

### Small Values (4KB-16KB)

Good for testing latency and ops/sec:

```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 10000 \
  --value-size 4KB \
  --num-threads 4
```

### Medium Values (64KB-256KB)

Balanced throughput testing:

```bash
cargo run --release --bin kv-bench -- \
  --num-keys 5000 \
  --value-size 256KB \
  --num-threads 4
```

### Large Values (1MB-10MB)

Tests RDMA throughput:

```bash
cargo run --release --bin kv-bench -- \
  --num-keys 1000 \
  --value-size 1MB \
  --num-threads 4 \
  --buffer-mb 128
```

### High Load Test

High data volume with safe concurrency:

```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 10000 \
  --value-size 1MB \
  --num-threads 4 \
  --buffer-mb 256
```

**Note**: Using more than 4 threads may cause "Cannot allocate memory" errors due to RDMA endpoint limits.

## Important Notes

### Memory Requirements

Each client (thread) needs a receive buffer of `--buffer-mb` size. Total memory needed:
- Server: `memory_pool_size` (set via server config)
- Clients: `num_threads * buffer_mb` MB

Example: 8 threads with 128 MB buffers = 1 GB total client memory

### Mock vs Real RDMA

- **Mock mode** (`--mock true`):
  - Uses in-memory `memcpy` for data transfers
  - **Only works in integration tests** (same process as server)
  - Will FAIL when benchmark and server run as separate processes
  - Use `cargo test` instead for mock mode testing

- **Real RDMA** (`--mock false`, default):
  - Requires RDMA hardware (EFA adapters)
  - Requires building with `--features rdma`
  - Works across separate processes and machines
  - This is the normal mode for the benchmark

### Performance Tips

1. Always use `--release` builds for accurate performance measurements
2. Ensure server has sufficient memory pool size for all keys
3. Start with smaller `--num-keys` to verify everything works
4. Adjust `--buffer-mb` based on `--value-size` (buffer should be >= value size)
5. Monitor server logs with `--log-level debug` if issues occur
6. For large value sizes, ensure adequate network bandwidth
7. **IMPORTANT**: Keep `--num-threads` low (2-4) to avoid exhausting RDMA endpoint resources
   - Each thread creates a separate RDMA TransferEngine
   - libfabric has system limits on concurrent endpoints
   - Symptoms: "Cannot allocate memory" or "fi_enable failed" errors

## Troubleshooting

### "Out of memory" errors

Increase server memory pool or reduce `--num-keys`:

```bash
# Start server with larger pool
cargo run --bin kv-server -- --memory-mb 4096

# Or reduce keys in benchmark
cargo run --bin kv-bench -- --num-keys 500
```

### Connection errors

Verify server is running and address is correct:

```bash
# Check server is listening
cargo run --bin kv-server -- --log-level info

# Verify connection
cargo run --bin kv-bench -- --log-level debug
```

### Slow performance

- Use release builds: `cargo run --release`
- Reduce logging: `--log-level warn`
- Check network latency for remote servers
- Ensure adequate hardware resources
