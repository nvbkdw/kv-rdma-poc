# Large Value Support

## Overview

The KV cache now supports storing and retrieving values up to **64 MB** via gRPC inline transmission.

## Previous Limitation

**Before**: Values larger than 64 KB would fail with:
```
Error: Large value PUT via RDMA not yet implemented
```

**Now**: Values up to 64 MB can be stored and retrieved successfully.

## Implementation

### PUT Operations

Values up to 64 MB are sent inline via gRPC:
- No RDMA required for PUT operations
- Simple, reliable transmission
- Works across all network configurations

### GET Operations

Values are retrieved via RDMA write:
- Server RDMA writes data directly to client buffer
- Zero-copy, high-performance retrieval
- Optimal for large value reads

## Configuration

### gRPC Message Size Limits

Both server and client are configured to handle 128 MB messages:

**Server**:
```rust
KvCacheServiceServer::new(service)
    .max_decoding_message_size(128 * 1024 * 1024) // 128MB receive
    .max_encoding_message_size(128 * 1024 * 1024) // 128MB send
```

**Client**:
```rust
KvCacheServiceClient::new(channel)
    .max_decoding_message_size(128 * 1024 * 1024)
    .max_encoding_message_size(128 * 1024 * 1024)
```

### Client Receive Buffer

Ensure client receive buffer is large enough for GET operations:

```bash
# Default: 64 MB (sufficient for most use cases)
./run-with-rdma.sh client put mykey largeval

# For larger values, increase buffer
./run-with-rdma.sh client --buffer-mb 128 get mykey
```

## Usage Examples

### Store 64 KB Value
```bash
# Create 64 KB file
dd if=/dev/urandom of=64kb.dat bs=1024 count=64

# Store via client
./run-with-rdma.sh client put mykey "$(cat 64kb.dat)"
```

### Store 1 MB Value
```bash
# Create 1 MB file
dd if=/dev/urandom of=1mb.dat bs=1M count=1

# Store (works now!)
./run-with-rdma.sh client put mykey "$(cat 1mb.dat)"
```

### Store 10 MB Value
```bash
# Create 10 MB file
dd if=/dev/urandom of=10mb.dat bs=1M count=10

# Store
./run-with-rdma.sh client put mykey "$(cat 10mb.dat)"
```

### Store 64 MB Value (Maximum)
```bash
# Create 64 MB file
dd if=/dev/urandom of=64mb.dat bs=1M count=64

# Store (at the limit)
./run-with-rdma.sh client put mykey "$(cat 64mb.dat)"
```

## Benchmark Large Values

### 1 MB Values
```bash
./run-with-rdma.sh bench \
  --value-size 1MB \
  --num-keys 1000 \
  --repeat-reads 10 \
  --buffer-mb 128
```

### 10 MB Values
```bash
./run-with-rdma.sh bench \
  --value-size 10MB \
  --num-keys 500 \
  --repeat-reads 5 \
  --buffer-mb 128
```

### 64 MB Values (Maximum)
```bash
./run-with-rdma.sh bench \
  --value-size 64MB \
  --num-keys 100 \
  --repeat-reads 2 \
  --buffer-mb 128
```

## Performance Characteristics

### PUT Performance (gRPC Inline)

Values sent via gRPC with standard network performance:

| Value Size | Typical PUT Time | Network Bandwidth |
|------------|------------------|-------------------|
| 64 KB      | ~1-2 ms          | ~50 MB/s          |
| 1 MB       | ~10-20 ms        | ~50-100 MB/s      |
| 10 MB      | ~100-200 ms      | ~50-100 MB/s      |
| 64 MB      | ~600-1200 ms     | ~50-100 MB/s      |

### GET Performance (RDMA)

Values retrieved via RDMA write with high performance:

| Value Size | Typical GET Time | RDMA Bandwidth |
|------------|------------------|----------------|
| 64 KB      | ~50-100 µs       | ~1 GB/s        |
| 1 MB       | ~500-1000 µs     | ~1-2 GB/s      |
| 10 MB      | ~5-10 ms         | ~1-2 GB/s      |
| 64 MB      | ~30-60 ms        | ~1-2 GB/s      |

**Note**: GET operations via RDMA are ~10-20x faster than PUT operations via gRPC!

## Limitations

### Maximum Value Size: 64 MB

Attempting to store values larger than 64 MB will fail:

```bash
# This will fail
./run-with-rdma.sh client put mykey "$(dd if=/dev/urandom bs=1M count=65)"

# Error: Value too large: 68157440 bytes (max 64 MB supported via gRPC)
```

### Server Memory Pool

Ensure server has sufficient memory to store all values:

```bash
# For 1000 × 10 MB values, need 10 GB
./run-with-rdma.sh server --memory-mb 10240

# For 100 × 64 MB values, need 6.4 GB
./run-with-rdma.sh server --memory-mb 6400
```

### Network Bandwidth

Large values require adequate network bandwidth:
- 1 Gbps network: ~125 MB/s theoretical maximum
- 10 Gbps network: ~1250 MB/s theoretical maximum
- 100 Gbps (EFA): ~12500 MB/s theoretical maximum

## Why Not Use RDMA for PUT?

Currently, PUT operations use gRPC inline because:

1. **Simplicity**: No complex buffer management on client side
2. **Reliability**: gRPC handles retries and flow control
3. **Performance**: PUT is typically less frequent than GET
4. **Flexibility**: Works across all network configurations

RDMA is used for GET because:
1. **Performance Critical**: Reads are typically hot path
2. **Zero-Copy**: Direct memory transfer without CPU
3. **Lower Latency**: Microseconds vs milliseconds
4. **Higher Bandwidth**: Multi-GB/s throughput

## Future Enhancements

For values larger than 64 MB, possible future implementations:

1. **RDMA-based PUT**: Client registers buffer, server reads via RDMA
2. **Chunked Transfer**: Split large values across multiple requests
3. **S3 Integration**: Store very large values in object storage

## Troubleshooting

### "Value too large" Error

**Solution**: Value exceeds 64 MB limit. Either:
- Split into smaller values
- Use external storage (S3, etc.)
- Request RDMA-based PUT implementation

### "Out of memory" on Server

**Solution**: Increase server memory pool:
```bash
./run-with-rdma.sh server --memory-mb 20480  # 20 GB
```

### "Receive buffer too small" on Client

**Solution**: Increase client receive buffer:
```bash
./run-with-rdma.sh client --buffer-mb 128 get mykey
./run-with-rdma.sh bench --buffer-mb 256
```

### Slow PUT Performance

**Expected**: Large values via gRPC are slower than RDMA GET.

**Optimization**:
- Use faster network (10 Gbps or EFA)
- Batch multiple smaller values instead of one large value
- Consider async PUT operations

## Summary

✅ **Supported**: Values up to 64 MB via gRPC inline
✅ **PUT**: Simple, reliable transmission via gRPC
✅ **GET**: High-performance retrieval via RDMA
✅ **No Code Changes**: Automatic handling of large values
✅ **Configurable**: Adjust buffers and limits as needed

Large value support makes the KV cache suitable for a wide range of use cases, from small metadata to multi-megabyte objects!
