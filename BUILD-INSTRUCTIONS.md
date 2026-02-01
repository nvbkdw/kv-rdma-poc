# Build Instructions for KV-RDMA-POC

This document explains how to build and run the KV cache with RDMA support.

## The Challenge

Building with `--features rdma` requires several environment variables to be set for the RDMA dependencies (GDRAPI, libfabric). Without these, you'll see errors like:

```
GDRAPI_HOME is not set and include/gdrapi.h is not found in the default paths
```

## The Solution: Helper Scripts

We provide two helper scripts to simplify building and running with RDMA:

### 1. `build-with-rdma.sh` - Build Script

Sets required environment variables and builds binaries.

**Usage:**
```bash
# Build all binaries (server, client, benchmark)
./build-with-rdma.sh all

# Build specific binary
./build-with-rdma.sh server
./build-with-rdma.sh client
./build-with-rdma.sh bench
```

**What it does:**
- Sets `GDRAPI_HOME` and `LIBFABRIC_HOME` environment variables
- Runs `cargo build --release --features rdma` with proper configuration
- Validates that the required directories exist

### 2. `run-with-rdma.sh` - Runtime Script

Sets library paths and runs the binary.

**Usage:**
```bash
# Run server
./run-with-rdma.sh ./target/release/kv-server

# Run benchmark
./run-with-rdma.sh ./target/release/kv-bench --num-keys 1000 --value-size 64KB

# Run client
./run-with-rdma.sh ./target/release/kv-client get mykey
```

**What it does:**
- Sets `LD_LIBRARY_PATH` for RDMA libraries (gdrcopy, libfabric, CUDA)
- Executes the binary with all arguments passed through

## Quick Start

```bash
# Step 1: Build everything
./build-with-rdma.sh all

# Step 2: Start server in one terminal
./run-with-rdma.sh ./target/release/kv-server

# Step 3: Run benchmark in another terminal
./run-with-rdma.sh ./target/release/kv-bench
```

## Manual Build (Advanced)

If you need to customize the paths or build manually:

```bash
# Set environment variables
export GDRAPI_HOME=/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4
export LIBFABRIC_HOME=/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric

# Build
cargo build --release --features rdma

# Set runtime library paths
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4/src:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric/lib:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/usr/local/cuda/lib64:${LD_LIBRARY_PATH}"

# Run
./target/release/kv-bench
```

## Customizing Paths

If your RDMA libraries are in different locations, edit the helper scripts:

**In `build-with-rdma.sh`:**
```bash
export GDRAPI_HOME=/your/path/to/gdrcopy
export LIBFABRIC_HOME=/your/path/to/libfabric
```

**In `run-with-rdma.sh`:**
```bash
export LD_LIBRARY_PATH="/your/path/to/gdrcopy/src:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/your/path/to/libfabric/lib:${LD_LIBRARY_PATH}"
```

## Testing Without RDMA Hardware

If you don't have RDMA hardware, you can still test the functionality using integration tests:

```bash
# Run all tests (uses mock transport within same process)
cargo test

# Run specific test with output
cargo test test_get_put -- --nocapture

# Run integration tests specifically
cargo test --test integration_test
```

**Important:** The mock transport only works within integration tests where client and server run in the same process. It will NOT work when running server and benchmark as separate binaries.

## Troubleshooting

### Build fails with "GDRAPI_HOME is not set"
**Solution:** Use `./build-with-rdma.sh` which sets these variables automatically

### Runtime error: "cannot open shared object file"
**Solution:** Use `./run-with-rdma.sh` to run the binary with proper library paths

### Benchmark fails with "Failed to resolve remote address"
**Cause:** Mock transport doesn't work across processes
**Solution:** Build and run with real RDMA using the helper scripts

### Want to verify build/runtime paths?
```bash
# Check what libraries the binary needs
ldd ./target/release/kv-bench

# Check if libraries are found
LD_LIBRARY_PATH="/path/to/libs" ldd ./target/release/kv-bench
```

## Summary

| Task | Command |
|------|---------|
| **Build all** | `./build-with-rdma.sh all` |
| **Build benchmark** | `./build-with-rdma.sh bench` |
| **Start server** | `./run-with-rdma.sh ./target/release/kv-server` |
| **Run benchmark** | `./run-with-rdma.sh ./target/release/kv-bench` |
| **Test without RDMA** | `cargo test` |
| **Help** | `./target/release/kv-bench --help` |
