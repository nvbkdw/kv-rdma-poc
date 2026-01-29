# KV-RDMA-POC

A distributed key-value cache that uses RDMA (Remote Direct Memory Access) for high-performance data transfers between nodes.

## Overview

This project implements a distributed KV cache where:
- **Control plane** uses gRPC for request coordination (GET/PUT/DELETE)
- **Data plane** uses RDMA Write for zero-copy data transfers directly to client memory

The architecture follows a **push model** - when a client requests data, the server writes it directly to the client's pre-registered memory buffer via RDMA, bypassing the CPU for maximum throughput.

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                         METADATA SERVICE                              │
│  HashMap<Key, ValueLocation { node_id, mr_descriptor, offset, len }> │
└──────────────────────────────────────────────────────────────────────┘
                                 │
                    Control Plane (gRPC)
              ┌──────────────────┴──────────────────┐
              │                                     │
              ▼                                     ▼
┌─────────────────────────────┐     ┌─────────────────────────────┐
│         SERVER              │     │         CLIENT              │
│  ┌───────────────────────┐  │     │  ┌───────────────────────┐  │
│  │  Memory Pool          │  │     │  │  Receive Buffer       │  │
│  │  (RDMA-Registered)    │  │     │  │  (RDMA-Registered)    │  │
│  └───────────────────────┘  │     │  └───────────────────────┘  │
│           │                 │     │           ▲                 │
│  ┌───────────────────────┐  │     │  ┌────────┴──────────────┐  │
│  │   RDMA Transport      │──┼─────┼──│   RDMA Transport      │  │
│  └───────────────────────┘  │     │  └───────────────────────┘  │
└─────────────────────────────┘     └─────────────────────────────┘
```

### GET Flow

```
CLIENT                                    SERVER
  │                                          │
  │  1. Allocate receive buffer              │
  │     Register with RDMA                   │
  │                                          │
  │  2. GET Request (key, buffer_location)   │
  │ ────────────────────────────────────────►│
  │              (gRPC)                      │
  │                                          │ 3. Lookup value
  │                                          │
  │  4. RDMA WRITE (zero-copy!)              │
  │ ◄━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━│
  │    Server Memory → Client Memory         │
  │                                          │
  │  5. GET Response (success, length)       │
  │ ◄────────────────────────────────────────│
  │              (gRPC)                      │
  │                                          │
  │  6. Read value from local buffer         │
  ▼                                          ▼
```

## Features

- **Zero-copy transfers**: Data flows directly from server memory to client memory via RDMA
- **Mock transport**: Test without RDMA hardware using in-memory simulation
- **gRPC control plane**: Standard protocol for request/response coordination
- **Memory pool management**: Efficient buffer allocation for RDMA-registered memory
- **Multiple client support**: Server handles concurrent client connections
- **TTL support**: Optional expiration for cached values

## Building

```bash
# Build with mock RDMA (no hardware required, default)
cargo build

# Build with real RDMA support (requires RDMA libraries)
cargo build --features rdma --release

# Run tests
cargo test
```

**Note**: By default, the project builds with mock RDMA for development. To use real EFA RDMA hardware, build with the `rdma` feature flag.

## Usage

### Starting the Server

**With Mock RDMA (development/testing):**
```bash
# Start with default settings (localhost:50051, 1GB memory pool, mock RDMA)
cargo run --bin kv-server

# Custom configuration
cargo run --bin kv-server -- \
  --listen-addr "0.0.0.0:50051" \
  --memory-mb 2048 \
  --node-id 0 \
  --log-level debug
```

**With Real EFA RDMA:**
```bash
# Server on machine 1
./run-with-rdma.sh ./target/release/kv-server \
  --mock false \
  --listen-addr "0.0.0.0:50051" \
  --memory-mb 2048 \
  --log-level info
```

### Using the Client

**With Mock RDMA (development/testing):**
```bash
# PUT a value
cargo run --bin kv-client -- put mykey "hello world"

# GET a value
cargo run --bin kv-client -- get mykey

# DELETE a value
cargo run --bin kv-client -- delete mykey

# Interactive REPL
cargo run --bin kv-client -- repl

# Run benchmark
cargo run --bin kv-client -- bench --ops 1000 --value-size 1024
```

**With Real EFA RDMA (client on machine 2):**
```bash
# PUT a value
./run-with-rdma.sh ./target/release/kv-client \
  --mock false \
  --server-addr "http://<server-ip>:50051" \
  put mykey "hello world"

# GET a value
./run-with-rdma.sh ./target/release/kv-client \
  --mock false \
  --server-addr "http://<server-ip>:50051" \
  get mykey

# Benchmark
./run-with-rdma.sh ./target/release/kv-client \
  --mock false \
  --server-addr "http://<server-ip>:50051" \
  bench --ops 10000 --value-size 4096
```

### Client CLI Options

```
Options:
  --client-id <ID>        Client node ID [default: 1]
  --server-addr <ADDR>    Server address [default: http://[::1]:50051]
  --buffer-mb <SIZE>      Receive buffer size in MB [default: 64]
  --mock                  Use mock transport [default: true]
  --log-level <LEVEL>     Log level [default: info]

Commands:
  get <key>               Get a value
  put <key> <value>       Put a value (--ttl for expiration)
  delete <key>            Delete a value
  repl                    Interactive mode
  bench                   Run benchmark
```

## Project Structure

```
kv-rdma-poc/
├── Cargo.toml
├── build.rs                 # Proto compilation
├── proto/
│   └── kv_cache.proto       # gRPC service definition
├── src/
│   ├── lib.rs               # Library exports
│   ├── main.rs              # Main entry point
│   ├── protocol.rs          # Shared types (MemoryRegionDescriptor, etc.)
│   ├── memory.rs            # Memory pool for RDMA buffers
│   ├── transport.rs         # RDMA transport abstraction
│   ├── server.rs            # KV cache server
│   ├── client.rs            # KV cache client
│   └── bin/
│       ├── server.rs        # Server CLI
│       └── client.rs        # Client CLI
└── tests/
    └── integration_test.rs  # Integration tests
```

## Key Concepts

### Memory Region Descriptor

To enable RDMA writes, memory must be registered with the RDMA transport. The `MemoryRegionDescriptor` contains:
- Base pointer address
- Per-domain address and remote key (rkey) pairs

This descriptor is shared with remote nodes to allow them to write to the registered memory.

### Push Model vs Pull Model

This implementation uses **RDMA Write (push model)**:
- The data owner (server) pushes data to the requestor (client)
- The requestor pre-allocates a receive buffer and shares its location
- This avoids the complexity of RDMA Read permissions

### Transport Abstraction

The `RdmaTransport` trait abstracts over:
- **Mock transport**: For testing without RDMA hardware (uses `memcpy`)
- **Real RDMA**: Integration with fabric-lib (not included in this POC)

## Integration with fabric-lib

To use real RDMA hardware, uncomment the fabric-lib dependency in `Cargo.toml` and implement the `RdmaTransportTrait` using:

```rust
use fabric_lib::{TransferEngine, TransferEngineBuilder, detect_topology};

// 1. Detect topology (GPU/NIC affinity)
let topo = detect_topology()?;

// 2. Build engine
let mut builder = TransferEngineBuilder::default();
builder.add_gpu_domains(gpu_id, domains, pin_cpu1, pin_cpu2);
let engine = builder.build()?;

// 3. Register memory
let (handle, descriptor) = engine.register_memory_allow_remote(ptr, len, device)?;

// 4. Submit transfer
engine.submit_transfer_async(request).await?;
```

## Running with Real RDMA

The `run-with-rdma.sh` helper script sets up the necessary library paths:

```bash
# The script sets LD_LIBRARY_PATH for:
# - libgdrapi (GPU Direct RDMA)
# - libfabric (custom build with EFA support)
# - libcudart (CUDA runtime)

# Then executes your command with proper environment
./run-with-rdma.sh <command> [args...]
```

Alternatively, set the paths manually:
```bash
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4/src:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric/lib:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/usr/local/cuda/lib64:${LD_LIBRARY_PATH}"

./target/release/kv-server --mock false --listen-addr "0.0.0.0:50051"
```

## Performance Considerations

- **Buffer size**: Larger receive buffers reduce allocation overhead but increase memory usage
- **Value size threshold**: Small values (<64KB) are sent inline via gRPC; large values use RDMA
- **Connection pooling**: gRPC connections are reused across requests
- **Memory alignment**: Buffers are page-aligned (4KB) for optimal RDMA performance
- **RDMA domains**: Configure `--num-domains` to use multiple NICs for higher throughput

## License

MIT
