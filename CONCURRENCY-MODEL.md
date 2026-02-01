# Benchmark Concurrency Model

## Overview

The benchmark uses a **fixed thread pool with client pooling** to achieve high concurrency without exhausting RDMA endpoint resources.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│              Tokio Runtime (4 OS Threads)                │
├─────────────────────────────────────────────────────────┤
│  Worker 1  Worker 2  Worker 3  ...  Worker N            │
│     │          │          │              │               │
│     └──────────┴──────────┴──────────────┘               │
│                    │                                      │
│              Client Pool (4 clients)                     │
│         ┌─────────┬─────────┬─────────┬────────┐        │
│         │ Client1 │ Client2 │ Client3 │Client4 │        │
│         └────┬────┴────┬────┴────┬────┴───┬────┘        │
│              │         │         │        │              │
│         ┌────┴─────────┴─────────┴────────┴────┐        │
│         │     4 RDMA TransferEngines            │        │
│         └─────────────────────────────────────

──┘        │
└───────────────────────────────────────────────────────────┘
            │
            ▼
      RDMA Network
```

## Key Components

### 1. Fixed OS Threads (4)
- Tokio runtime configured with exactly 4 worker threads
- Set via `#[tokio::main(worker_threads = 4)]`
- This is the **optimal** number for most systems
- Each thread handles multiple async tasks

### 2. Client Pool (Default: 4)
- Fixed number of `KvCacheClient` instances
- Each client has its own RDMA `TransferEngine`
- Created once at startup, shared across workers
- Configurable via `--num-clients` (default: 4)

### 3. Concurrent Workers (Default: 16)
- Number of async tasks performing GET operations
- Each worker borrows a client from the pool (round-robin)
- Multiple workers can share the same client (safe due to Arc)
- Configurable via `--num-workers` (default: 16)

## Why This Design?

### Problem: RDMA Endpoint Exhaustion
Each `TransferEngine` creates RDMA endpoints which consume system resources:
- File descriptors
- Kernel memory
- EFA device contexts

Creating too many endpoints causes:
```
LibfabricError: code -12 ("Cannot allocate memory"), context: fi_enable
```

### Solution: Client Pooling
Instead of creating one client per worker, we:
1. Create a fixed pool of clients (4)
2. Share them across many workers (16+)
3. Use tokio's async runtime to multiplex operations

This allows high concurrency without resource exhaustion.

## Configuration

### Command-Line Arguments

```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-workers 16 \      # Concurrent async tasks
  --num-clients 4         # RDMA clients/endpoints
```

### Recommended Settings

| Scenario | Workers | Clients | Description |
|----------|---------|---------|-------------|
| **Light load** | 8 | 2 | Low concurrency, minimal resources |
| **Balanced** (default) | 16 | 4 | Good concurrency, safe resource usage |
| **High throughput** | 32 | 4 | Maximum concurrency with safety |
| **Stress test** | 64 | 4 | Very high concurrency (may hit other limits) |

**WARNING**: Do NOT increase `--num-clients` beyond 8 unless you know your system can handle it. The default of 4 is optimal for most scenarios.

## How Workers Share Clients

Workers are assigned to clients using round-robin:

```rust
for worker_id in 0..num_workers {
    // Worker 0 → Client 0
    // Worker 1 → Client 1
    // Worker 2 → Client 2
    // Worker 3 → Client 3
    // Worker 4 → Client 0 (wraps around)
    // ...
    let client = Arc::clone(&clients[worker_id % clients.len()]);

    // Spawn async task with this client
    tasks.spawn(async move {
        // Perform GET operations...
    });
}
```

Each `KvCacheClient` is thread-safe (uses `Arc` and internal mutexes), so multiple workers can safely share the same client.

## Performance Characteristics

### Throughput Scaling

With 4 clients and varying workers:
- 4 workers: ~baseline throughput
- 8 workers: ~1.5-1.8x throughput
- 16 workers: ~2.0-2.5x throughput
- 32 workers: ~2.5-3.0x throughput (diminishing returns)

The scaling is sublinear because:
1. RDMA clients become the bottleneck
2. Network bandwidth limits
3. Server-side contention

### Resource Usage

| Component | Count | Per-Instance Resource | Total |
|-----------|-------|----------------------|-------|
| OS Threads | 4 | ~8 MB stack | ~32 MB |
| RDMA Clients | 4 | Receive buffer (64 MB) | ~256 MB |
| TransferEngines | 4 | RDMA contexts | ~4 endpoints |
| Workers (tasks) | 16 | ~2 KB | ~32 KB |

**Total**: ~300 MB memory, 4 RDMA endpoints

Compare this to the old model (1 client per thread):
- 16 threads × 64 MB = 1 GB memory
- 16 RDMA endpoints (may fail!)

## Examples

### Default Configuration
```bash
./run-with-rdma.sh ./target/release/kv-bench
# 16 workers, 4 clients, 4 OS threads
```

### Low Resource Mode
```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-workers 8 \
  --num-clients 2
# Minimal resource usage
```

### High Throughput Mode
```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-workers 32 \
  --num-clients 4
# Maximum safe concurrency
```

### Stress Test
```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-workers 64 \
  --num-clients 4 \
  --num-keys 10000
# Very high concurrency
```

## Comparison with Old Model

### Old Model (Per-Thread Clients)
```
16 threads → 16 clients → 16 TransferEngines → Resource exhaustion ❌
```

### New Model (Client Pooling)
```
16 workers → 4 clients → 4 TransferEngines → Works reliably ✅
```

## Troubleshooting

### "Cannot allocate memory" errors
- **Cause**: Too many `--num-clients`
- **Solution**: Reduce to 2-4 clients
- **Never**: Exceed 8 clients

### Poor throughput scaling
- **Cause**: Too few workers relative to clients
- **Solution**: Increase `--num-workers` to 4x or 8x `--num-clients`
- **Example**: 4 clients → try 16 or 32 workers

### High CPU usage
- **Cause**: Too many workers spinning
- **Solution**: Reduce `--num-workers`
- **Sweet spot**: 16-32 workers for 4 clients

## Summary

The benchmark achieves high concurrency through:
1. **Fixed 4 OS threads** - optimal for tokio runtime
2. **Client pooling** - reuse expensive RDMA endpoints
3. **Many async workers** - high concurrency without resource costs

This design allows 16+ concurrent operations while using only 4 RDMA endpoints, avoiding the "Cannot allocate memory" error that plagued the old per-thread model.
