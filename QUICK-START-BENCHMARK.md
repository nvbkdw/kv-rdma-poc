# Quick Start: Running the Benchmark

## The Problem You Encountered

The error you saw:
```
Failed to resolve remote address: 6d6f636b3a2f2f6e6f64653130322f646f6d61696e30
```

This happens because **mock transport only works when client and server run in the same process** (like integration tests). When running as separate binaries, the mock addresses are invalid.

## Solution: Use Real RDMA

The benchmark is designed to run with real RDMA hardware. Here's how:

### Step 1: Build with RDMA Support

```bash
# Easy way: Use the build helper script
./build-with-rdma.sh all

# Or build specific binaries
./build-with-rdma.sh bench    # Build benchmark only
./build-with-rdma.sh server   # Build server only

# Manual way: Set environment variables
GDRAPI_HOME=/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4 \
LIBFABRIC_HOME=/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric \
cargo build --release --bin kv-bench --features rdma
```

### Step 2: Start the Server

```bash
# On the server machine (or same machine for testing)
./run-with-rdma.sh server
```

### Step 3: Run the Benchmark

```bash
# Run benchmark with default settings
./run-with-rdma.sh bench

# Or with custom parameters
./run-with-rdma.sh bench \
  --num-keys 1000 \
  --value-size 64KB \
  --num-workers 16
```

## Alternative: Use Integration Tests (No RDMA Hardware)

If you don't have RDMA hardware and want to test functionality:

```bash
# Integration tests run server and clients in same process with mock transport
cargo test

# Run specific test
cargo test test_get_put

# Run with output
cargo test -- --nocapture
```

## Why Mock Transport Doesn't Work for Separate Processes

Mock transport works by using `memcpy` to copy data between memory addresses. This only works when both processes share the same address space (i.e., same process). When the server runs in a separate process:

1. Client allocates buffer at address `0x123456` in **client process**
2. Client tells server to write to address `0x123456`
3. Server tries to write to `0x123456` in **server process** (different address space!)
4. Result: Segfault or address resolution error

Real RDMA works because the RDMA hardware translates virtual addresses across process/machine boundaries using registered memory regions.

## Quick Reference

| Scenario | Command |
|----------|---------|
| **Build** | `./build-with-rdma.sh all` |
| **Start server** | `./run-with-rdma.sh server` |
| **Run benchmark** (default) | `./run-with-rdma.sh bench` |
| **Without RDMA** (development) | `cargo test` |
| **Small values (16KB)** | `./run-with-rdma.sh bench --value-size 16KB` |
| **Large values (1MB)** | `./run-with-rdma.sh bench --value-size 1MB --buffer-mb 128` |
| **High concurrency (32 workers)** | `./run-with-rdma.sh bench --num-workers 32` |
| **Remote server** | `./run-with-rdma.sh bench --server-addr "http://remote-ip:50051"` |

## Expected Output

When running successfully, you should see:

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
...

=== Summary ===
...
```

## Troubleshooting

### Error: "Failed to resolve remote address"
**Solution**: Don't use `--mock true`. Build and run with `--features rdma`

### Error: "RDMA libraries not found" or "GDRAPI_HOME is not set"
**Solution**: Use the build helper script: `./build-with-rdma.sh all`

Or manually set the environment variables:
```bash
export GDRAPI_HOME=/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4
export LIBFABRIC_HOME=/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric
cargo build --release --features rdma
```

### Error: "Connection refused"
**Solution**: Make sure the server is running: `./run-with-rdma.sh server`

### Error: "Cannot allocate memory" or "fi_enable failed"
**Cause**: Too many RDMA clients (each client creates a TransferEngine)
**Solution**: Reduce `--num-clients` (not `--num-workers`):
```bash
./run-with-rdma.sh bench --num-clients 2
```

**Why this happens**: Each benchmark thread creates a separate RDMA endpoint, and libfabric has system limits on how many endpoints can be created. With EFA, typically 2-4 threads work well.

**Recommended thread counts**:
- 1-2 threads: Safe for all systems
- 3-4 threads: Usually works
- 5+ threads: May hit resource limits

### Want to test without RDMA?
**Solution**: Use integration tests: `cargo test`
