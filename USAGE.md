# Quick Usage Guide

## Overview

The `run-with-rdma.sh` script provides a simple interface to run KV cache binaries with proper RDMA library paths.

## Syntax

```bash
./run-with-rdma.sh {server|client|bench} [args...]
```

## Server

### Start Server (Default)
```bash
./run-with-rdma.sh server
```

### Start Server with Options
```bash
# Custom memory size
./run-with-rdma.sh server --memory-mb 2048

# Custom listen address
./run-with-rdma.sh server --listen-addr "0.0.0.0:50051"

# Debug logging
./run-with-rdma.sh server --log-level debug

# Combined options
./run-with-rdma.sh server \
  --listen-addr "0.0.0.0:50051" \
  --memory-mb 4096 \
  --log-level info
```

## Client

### GET Operation
```bash
./run-with-rdma.sh client get mykey
```

### PUT Operation
```bash
./run-with-rdma.sh client put mykey "hello world"

# With TTL
./run-with-rdma.sh client put mykey "hello world" --ttl 300
```

### DELETE Operation
```bash
./run-with-rdma.sh client delete mykey
```

### Interactive REPL
```bash
./run-with-rdma.sh client repl
```

### Connect to Remote Server
```bash
./run-with-rdma.sh client \
  --server-addr "http://192.168.1.10:50051" \
  get mykey
```

## Benchmark

### Default Benchmark
```bash
./run-with-rdma.sh bench
# Uses: 1000 keys, 64KB values, 16 workers, 4 clients, 10 repeat reads
```

### Custom Parameters

**Number of workers** (concurrent async tasks):
```bash
./run-with-rdma.sh bench --num-workers 32
```

**Number of RDMA clients**:
```bash
./run-with-rdma.sh bench --num-clients 4
```

**Value size**:
```bash
./run-with-rdma.sh bench --value-size 16KB
./run-with-rdma.sh bench --value-size 1MB
./run-with-rdma.sh bench --value-size 10MB
```

**Number of keys**:
```bash
./run-with-rdma.sh bench --num-keys 5000
```

**Repeat reads**:
```bash
./run-with-rdma.sh bench --repeat-reads 20
# Each key read 20 times (20,000 total operations with 1000 keys)
```

**Remote server**:
```bash
./run-with-rdma.sh bench --server-addr "http://192.168.1.10:50051"
```

### Common Benchmark Scenarios

**Quick test**:
```bash
./run-with-rdma.sh bench \
  --num-keys 500 \
  --repeat-reads 5
```

**High throughput test**:
```bash
./run-with-rdma.sh bench \
  --num-keys 2000 \
  --repeat-reads 50 \
  --num-workers 32 \
  --num-clients 4
```

**Large value test**:
```bash
./run-with-rdma.sh bench \
  --value-size 1MB \
  --num-keys 1000 \
  --repeat-reads 10 \
  --buffer-mb 128
```

**Maximum stress test**:
```bash
./run-with-rdma.sh bench \
  --num-keys 1000 \
  --repeat-reads 100 \
  --num-workers 64 \
  --num-clients 4
```

## Build First

Before running, build the binaries:

```bash
# Build all binaries
./build-with-rdma.sh all

# Or build individually
./build-with-rdma.sh server
./build-with-rdma.sh client
./build-with-rdma.sh bench
```

## Help

Get help for any binary:

```bash
./run-with-rdma.sh server --help
./run-with-rdma.sh client --help
./run-with-rdma.sh bench --help
```

## Complete Example Workflow

### Terminal 1: Start Server
```bash
./build-with-rdma.sh all
./run-with-rdma.sh server
```

### Terminal 2: Run Benchmark
```bash
./run-with-rdma.sh bench
```

### Terminal 3: Manual Testing
```bash
./run-with-rdma.sh client put test "hello world"
./run-with-rdma.sh client get test
./run-with-rdma.sh client delete test
```

## Error Messages

If you see:
```
Error: Binary not found: ./target/release/kv-server
Please build first: ./build-with-rdma.sh all
```

Run: `./build-with-rdma.sh all`

If you see:
```
Usage: ./run-with-rdma.sh {server|client|bench} [args...]
```

You forgot to specify which binary to run. Use `server`, `client`, or `bench`.

## Old vs New Syntax

### Before (Verbose)
```bash
./run-with-rdma.sh ./target/release/kv-server --memory-mb 2048
./run-with-rdma.sh ./target/release/kv-client get mykey
./run-with-rdma.sh ./target/release/kv-bench --num-workers 32
```

### After (Simple)
```bash
./run-with-rdma.sh server --memory-mb 2048
./run-with-rdma.sh client get mykey
./run-with-rdma.sh bench --num-workers 32
```

Much cleaner! 🎉
