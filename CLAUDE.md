# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a distributed key-value cache proof-of-concept that uses RDMA (Remote Direct Memory Access) for high-performance data transfers. The architecture separates control and data planes:
- **Control plane**: gRPC for request coordination (GET/PUT/DELETE)
- **Data plane**: RDMA Write for zero-copy data transfers

The implementation uses a **push model** where servers RDMA write data directly to pre-registered client memory buffers, bypassing the CPU for maximum throughput.

## Build and Test Commands

```bash
# Build with mock RDMA (no hardware required, default)
cargo build

# Build with real RDMA support (requires RDMA libraries installed)
cargo build --features rdma

# Build in release mode
cargo build --release --features rdma

# Run all tests
cargo test

# Run integration tests specifically
cargo test --test integration_test

# Run a specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Check code without building
cargo check
cargo check --features rdma
```

## Running the Server and Client

### Server
```bash
# Default configuration (localhost:50051, 1GB memory pool)
cargo run --bin kv-server

# With custom settings
cargo run --bin kv-server -- \
  --listen-addr "0.0.0.0:50051" \
  --memory-mb 2048 \
  --node-id 0 \
  --log-level debug
```

### Client
```bash
# PUT a value
cargo run --bin kv-client -- put mykey "hello world"

# GET a value
cargo run --bin kv-client -- get mykey

# DELETE a value
cargo run --bin kv-client -- delete mykey

# Interactive REPL mode
cargo run --bin kv-client -- repl

# Run simple benchmark
cargo run --bin kv-client -- bench --ops 1000 --value-size 1024

# Connect to remote server
cargo run --bin kv-client -- --server-addr "http://remote-ip:50051" get mykey
```

### Benchmark (kv-bench)

**IMPORTANT**: Requires RDMA hardware and `--features rdma` build flag.
Mock transport does NOT work for separate processes.

```bash
# Build with RDMA support (use helper script to set environment variables)
./build-with-rdma.sh all

# Start server first
./run-with-rdma.sh ./target/release/kv-server

# Run read throughput benchmark with default settings
./run-with-rdma.sh ./target/release/kv-bench

# Custom benchmark with specific parameters
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 5000 \
  --value-size 1MB \
  --num-threads 8 \
  --buffer-mb 128

# Test different value sizes
./run-with-rdma.sh ./target/release/kv-bench --value-size 16KB --num-threads 4
./run-with-rdma.sh ./target/release/kv-bench --value-size 1MB --num-threads 8

# Connect to remote server
./run-with-rdma.sh ./target/release/kv-bench \
  --server-addr "http://remote-ip:50051"

# For testing without RDMA hardware, use integration tests instead:
cargo test

# See BENCHMARK.md for more details
```

## Architecture

### Core Components

1. **protocol.rs**: Shared types for RDMA operations
   - `MemoryRegionDescriptor`: Contains remote access information (ptr, domain addresses, rkeys)
   - `ValueLocation`: Describes where data is stored or should be written
   - `CacheEntry`: Internal representation with TTL support
   - Conversions between internal types and protobuf types

2. **transport.rs**: RDMA transport abstraction
   - `RdmaTransport`: Main transport wrapper supporting both mock and real RDMA
   - `MockTransport`: In-memory simulation using `memcpy` for testing without hardware
   - `TransferRequest`: Describes a single RDMA write operation
   - `DomainRouting`: Strategy for selecting NICs in multi-domain setups

3. **memory.rs**: Memory pool management
   - `MemoryPool`: RDMA-registered memory with bump allocator
   - `BumpAllocator`: Simple allocator with free list for reusing deallocated blocks
   - Page-aligned allocations (4KB) for optimal RDMA performance
   - Tracks allocations and handles fragmentation

4. **server.rs**: KV cache server implementation
   - Stores values in RDMA-registered memory pool
   - On GET: performs RDMA write to client's pre-registered buffer
   - Uses `DashMap` for concurrent cache access
   - Handles TTL expiration on access

5. **client.rs**: KV cache client implementation
   - Registers receive buffer with server during connection
   - On GET: allocates from pool, sends buffer location to server, receives via RDMA
   - Tracks pending requests with request ID correlation
   - Handles buffer allocation/deallocation lifecycle

### GET Request Flow

1. Client allocates receive buffer from its memory pool
2. Client sends GET request via gRPC with buffer location (MemoryRegionDescriptor + offset)
3. Server looks up value in cache
4. Server performs RDMA write directly to client's buffer
5. Server responds via gRPC with success/length
6. Client reads value from local buffer and deallocates

### Key Design Decisions

- **Push vs Pull**: Uses RDMA Write (push) instead of RDMA Read to avoid permission complexity
- **Inline threshold**: Values <64KB sent inline via gRPC; larger values use RDMA
- **Mock transport**: Default mode uses in-memory copy for testing without RDMA hardware
- **Memory registration**: Buffers pre-registered during setup for zero-copy transfers
- **Per-domain addressing**: Supports multi-NIC topologies with domain-specific addresses and rkeys

## Protocol Buffer Definitions

Located in `proto/kv_cache.proto`. The build script (`build.rs`) compiles these using `tonic-build` into generated code accessible via `crate::pb::*`.

Key messages:
- `GetRequest`: Includes `response_location` where server should RDMA write
- `PutRequest`: Uses oneof for inline values or RDMA location
- `RegisterClientRequest`: Exchanges RDMA endpoint information

## Integration with fabric-lib

This project uses `fabric-lib` (path: `../pplx-garden/fabric-lib`) for real RDMA operations via EFA (Elastic Fabric Adapter).

### Transport Modes

The transport layer supports two modes controlled by the `--mock` CLI flag:

**Mock Transport** (`--mock true`, default for development):
- Uses in-memory `memcpy` for data transfers
- No RDMA hardware required
- **Only works when client and server run in same process** (e.g., integration tests)
- Will segfault if used across separate processes

**Real RDMA Transport** (`--mock false`):
- Uses fabric-lib TransferEngine with EFA
- Detects system topology automatically via `detect_topology()`
- Registers memory with RDMA NICs
- Performs zero-copy transfers over the network

### Using Real RDMA

First, build with the `rdma` feature on a machine with RDMA libraries installed:

```bash
# Build with RDMA support
cargo build --release --features rdma

# Server on machine 1
./target/release/kv-server --mock false --listen-addr "0.0.0.0:50051"

# Client on machine 2
./target/release/kv-client --mock false --server-addr "http://<server-ip>:50051" put mykey "hello"
./target/release/kv-client --mock false --server-addr "http://<server-ip>:50051" get mykey
```

**Note**: The binary must be compiled with `--features rdma` for real RDMA support. If you try to use `--mock false` without the rdma feature, you'll get an error at runtime.

### Memory Registration

Memory pools are automatically registered with the RDMA transport during initialization:
- `RdmaTransport::register_memory()` registers host memory buffers
- Returns `(MemoryRegionHandle, MemoryRegionDescriptor)` containing domain addresses and remote keys
- fabric-lib handles the low-level registration with Device::Host for CPU memory

### Required System Libraries

For real RDMA mode, the following libraries must be installed:
- `libibverbs` - InfiniBand verbs library
- `libfabric` - OpenFabrics libfabric (EFA support)
- `libgdrapi` - GPU Direct RDMA (for GPU memory, optional)
- `libcudart` - CUDA runtime (for GPU support)
- EFA kernel driver and device firmware

## Important Implementation Notes

- All memory operations are unsafe (raw pointer manipulation for RDMA)
- Lock ordering: Always acquire pool locks briefly and release before await points
- DashMap entries must be dropped before acquiring other locks to avoid deadlocks
- Buffer lifecycle: allocate → use → deallocate (clients must deallocate even on errors)
- TTL checking: done lazily on access, expired entries removed immediately
- Request IDs: monotonically increasing for tracking in-flight operations

## Testing Strategy

- Unit tests in each module test individual components
- Integration tests in `tests/integration_test.rs` test full client-server flows
- Mock transport allows testing without RDMA hardware
- Tests use small memory pools (1-4MB) for fast execution
