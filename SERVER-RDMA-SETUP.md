# RDMA Setup and Usage Guide

This document describes how the KV-RDMA-POC project is configured for real RDMA with EFA.

## Build Status

✅ **Successfully built with real RDMA support!**

```bash
cargo build --release --features rdma
```

## Library Dependencies

The project links against the following RDMA libraries:

| Library | Location | Purpose |
|---------|----------|---------|
| libfabric | `/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric/lib` | OpenFabrics libfabric with EFA support |
| libgdrapi | `/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4/src` | GPU Direct RDMA for GPU memory access |
| libcudart | `/usr/local/cuda/lib64` | CUDA runtime |
| libibverbs | `/lib/x86_64-linux-gnu` | InfiniBand verbs (system) |
| libefa | `/lib/x86_64-linux-gnu` | EFA provider (system) |

## Running the Server and Client

### Option 1: Using the Helper Script (Recommended)

```bash
# Server (machine 1)
./run-with-rdma.sh ./target/release/kv-server \
  --mock false \
  --listen-addr "0.0.0.0:50051" \
  --memory-mb 2048 \
  --num-domains 1 \
  --log-level info

# Client (machine 2)
./run-with-rdma.sh ./target/release/kv-client \
  --mock false \
  --server-addr "http://<server-ip>:50051" \
  put mykey "hello world"

./run-with-rdma.sh ./target/release/kv-client \
  --mock false \
  --server-addr "http://<server-ip>:50051" \
  get mykey
```

### Option 2: Setting LD_LIBRARY_PATH Manually

```bash
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4/src:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric/lib:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/usr/local/cuda/lib64:${LD_LIBRARY_PATH}"

./target/release/kv-server --mock false --listen-addr "0.0.0.0:50051"
```

## Transport Modes

The project supports two modes:

### Mock Transport (Default for Development)
- Uses in-memory `memcpy` instead of real RDMA
- No hardware required
- **Only works when client and server are in the same process**
- Build: `cargo build` (no features needed)
- Run: `cargo run --bin kv-server` (or with `--mock true`)

### Real RDMA Transport (Production)
- Uses fabric-lib TransferEngine with EFA
- Requires RDMA hardware (EFA-enabled EC2 instances)
- Works across network between different machines
- Build: `cargo build --release --features rdma`
- Run: Use `--mock false` flag with proper library paths

## Architecture

### Control Plane
- gRPC for request coordination (GET/PUT/DELETE)
- Handles metadata and orchestration
- Standard TCP/IP networking

### Data Plane
- RDMA Write for zero-copy transfers
- Server pushes data directly to client's pre-registered memory
- Bypasses CPU for maximum throughput
- Uses EFA (Elastic Fabric Adapter) on AWS

### Memory Registration Flow

1. **Server startup:**
   - Creates memory pool (1GB by default)
   - Registers with fabric-lib TransferEngine
   - Gets MemoryRegionDescriptor with domain addresses and rkeys

2. **Client startup:**
   - Creates receive buffer (64MB by default)
   - Registers with fabric-lib TransferEngine
   - Connects to server via gRPC

3. **GET request:**
   - Client allocates buffer from pool
   - Sends GET request with buffer location (MemoryRegionDescriptor + offset)
   - Server performs RDMA Write directly to client buffer
   - Server responds via gRPC with success/length
   - Client reads value from local buffer

## Testing

### Mock Mode (Integration Tests)
```bash
# These tests run client and server in-process
cargo test
```

### Real RDMA (Manual Testing)
```bash
# Terminal 1: Server
./run-with-rdma.sh ./target/release/kv-server --mock false --listen-addr "0.0.0.0:50051"

# Terminal 2: Client (can be on same or different machine with EFA)
./run-with-rdma.sh ./target/release/kv-client --mock false put test "hello"
./run-with-rdma.sh ./target/release/kv-client --mock false get test
```

## Performance Tuning

### Server Options
- `--memory-mb`: Size of RDMA-registered memory pool (default: 1024 MB)
- `--num-domains`: Number of RDMA NICs to use (default: 1)
- `--listen-addr`: gRPC listen address

### Client Options
- `--buffer-mb`: Size of receive buffer (default: 64 MB)
- `--server-addr`: Server gRPC endpoint

### Benchmarking
```bash
./run-with-rdma.sh ./target/release/kv-client \
  --mock false \
  --server-addr "http://<server-ip>:50051" \
  bench --ops 100000 --value-size 4096
```

## Troubleshooting

### "libgdrapi.so.2: cannot open shared object file"
**Solution:** Use the `run-with-rdma.sh` script or set `LD_LIBRARY_PATH` manually.

### "Real RDMA not available. Rebuild with '--features rdma'"
**Solution:** The binary was built without RDMA support. Rebuild with:
```bash
cargo build --release --features rdma
```

### "No GPU-RDMA topology found" or "Failed to detect topology"
**Cause:** fabric-lib requires GPU-NIC co-location for topology detection, even for host memory transfers.

**Solution:**
- **For development**: Use mock transport: `--mock true` (default)
- **For production**: See `KNOWN-ISSUES.md` for detailed workarounds
- **Quick check**: Run `lspci -tv | grep -A2 "NVIDIA\|EFA"` to verify if GPUs and EFA are on same PCI tree
- **Alternative**: Use instance types with GPU-EFA co-location (p4d, p5)

### Segmentation fault
**Solution:** If using `--mock true` with separate processes, switch to `--mock false` or run in same process (integration tests only).

## System Requirements for Real RDMA

- EFA-enabled EC2 instance types (e.g., p4d.24xlarge, p5.48xlarge)
- EFA kernel driver installed and loaded
- CUDA toolkit (for GPU Direct RDMA support)
- Libraries: libibverbs, libfabric, libefa, libgdrapi, libcudart

## Next Steps

1. **Deploy to EFA instances:** Copy binaries to EFA-enabled EC2 instances
2. **Run distributed tests:** Test with server on one instance, client on another
3. **Benchmark performance:** Measure throughput and latency with real RDMA
4. **Tune parameters:** Adjust buffer sizes, domain counts for optimal performance
5. **Add monitoring:** Integrate metrics collection for RDMA transfer stats
