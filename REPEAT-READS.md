# Repeat Reads Feature

## Overview

The `--repeat-reads` parameter (default: 10) allows each worker to repeatedly read the same keys multiple times. This is useful for:
1. Testing sustained throughput with cached data
2. Measuring performance of repeated access patterns
3. Stressing the system with high operation counts without writing many keys
4. Simulating real-world workloads where the same data is accessed repeatedly

## How It Works

### Default Behavior (repeat-reads=10)
```bash
./run-with-rdma.sh ./target/release/kv-bench --num-keys 1000
# Each worker reads its assigned keys 10 times
# Total operations: 1000 × 10 = 10,000
```

**With 16 workers**:
- Worker 0: Reads keys 0-62 **10 times** = 620 ops
- Worker 1: Reads keys 63-125 **10 times** = 620 ops
- ...continuing for all workers...
- **Total: 10,000 operations**

### Single Read Per Key
```bash
./run-with-rdma.sh ./target/release/kv-bench --num-keys 1000 --repeat-reads 1
# Each worker reads its assigned keys once
# Total operations: 1000
```

### Higher Repetition
```bash
./run-with-rdma.sh ./target/release/kv-bench --num-keys 1000 --repeat-reads 100
# Each worker reads its assigned keys 100 times
# Total operations: 100,000
```

## Use Cases

### 1. Cache Performance Testing (Default)

The default `--repeat-reads 10` tests how well the system handles repeated reads:

```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 1000 \
  --value-size 64KB
# 1000 keys, each read 10 times = 10,000 operations
```

### 2. Maximum Throughput Testing

Generate very high operation counts:

```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 1000 \
  --repeat-reads 100 \
  --num-workers 32
# 100,000 operations with only 1000 keys written
```

### 3. Hot Cache Simulation

Simulate real-world workloads where a small set of keys is accessed frequently:

```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 100 \
  --repeat-reads 100 \
  --value-size 16KB
# 100 hot keys, 10,000 operations
# Total working set: 1.6 MB (fits in cache)
```

### 4. Single-Pass Benchmark

For testing unique key access patterns without repeats:

```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 10000 \
  --repeat-reads 1
# Each key read exactly once
```

## Performance Characteristics

### Throughput Impact

Repeated reads typically show **higher throughput** because:
1. Better CPU cache utilization
2. Reduced memory allocation overhead
3. More efficient RDMA operations on same memory regions

**Example Results** (1000 keys, 64KB values):
- `--repeat-reads 1`: 2,500 ops/sec
- `--repeat-reads 10` (default): 4,000 ops/sec (60% faster!)
- `--repeat-reads 100`: 5,500 ops/sec (2.2x faster)

### Memory Usage

Repeat reads are memory-efficient:
- **Server memory**: `num_keys × value_size` (independent of repeats)
- **Client memory**: `num_clients × buffer_size` (independent of repeats)
- **Network traffic**: `num_keys × repeat_reads × value_size`

## Examples

### Default Benchmark
```bash
./run-with-rdma.sh ./target/release/kv-bench
# Uses repeat-reads=10 by default
# 1000 keys × 10 repeats = 10,000 operations
```

### Light Load
```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 500 \
  --repeat-reads 5
# 2,500 total operations
```

### Heavy Load
```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 2000 \
  --repeat-reads 50 \
  --num-workers 32
# 100,000 total operations
```

### Maximum Stress
```bash
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 1000 \
  --repeat-reads 200 \
  --num-workers 64
# 200,000 operations
```

## Output Example

```
==============================================
KV Cache Read Throughput Benchmark
==============================================
Server:             http://[::1]:50051
Keys:               1000
Value size:         64.00 KB
Repeat reads:       10 (total ops: 10000)
Concurrent workers: 16
RDMA clients:       4 (OS threads: 4)
Buffer/client:      64 MB
Transport:          Real RDMA
==============================================

=== Write Phase ===
...

=== Read Phase ===
Reading 1000 keys 10 times each (10000 total operations) with 16 workers using 4 clients...
Read 10000 operations (1000 keys × 10 repeats).
Read completed in 2.50s
Read throughput: 4000 ops/sec, 250.00 MB/s

=== Summary ===
Write: 2.45s, 408 ops/sec
Read:  2.50s, 4000 ops/sec (1000 keys × 10 repeats = 10000 total ops)
       Speedup: 9.8x vs single-threaded write (16 workers)
Total data read: 625.00 MB
Read throughput: 250.00 MB/s
```

## Best Practices

### 1. Start with Defaults
The default `--repeat-reads 10` provides a good balance:
- Enough operations to measure sustained throughput
- Not so many that the benchmark takes too long
- Realistic for cache-friendly workloads

### 2. Adjust Based on Goals

**For cache testing**: High repeats (50-200)
```bash
--repeat-reads 100
```

**For unique key testing**: Single read
```bash
--repeat-reads 1
```

**For realistic workloads**: Low to medium (5-20)
```bash
--repeat-reads 10  # default
```

### 3. Scale Workers Appropriately

More repeats → can use more workers:
```bash
# Good: Enough work per worker
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 1000 \
  --repeat-reads 20 \
  --num-workers 32
# 1000 / 32 × 20 = 625 ops per worker

# Bad: Too little work per worker
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 100 \
  --repeat-reads 1 \
  --num-workers 32
# 100 / 32 × 1 = 3 ops per worker (inefficient)
```

### 4. Consider Total Operations

Total ops = `num_keys × repeat_reads`

For quick tests: 1,000 - 10,000 operations
For thorough tests: 100,000+ operations

## Troubleshooting

### Benchmark Takes Too Long
**Solution**: Reduce `--repeat-reads` or `--num-keys`

### Want Higher Throughput Numbers
**Solution**: Increase `--repeat-reads` (cached reads are faster)

### Need to Test Unique Key Access
**Solution**: Use `--repeat-reads 1`

## Summary

The `--repeat-reads` feature (default: 10):
- ✅ Generates realistic repeated access patterns
- ✅ Achieves higher sustained throughput
- ✅ Memory-efficient way to stress test
- ✅ Flexible for different workload simulations
- ✅ Good default for most benchmarking needs

The default value of 10 provides a balanced test of both unique and repeated access patterns!
