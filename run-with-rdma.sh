#!/bin/bash
# Helper script to run KV cache with proper library paths for RDMA
#
# Usage:
#   ./run-with-rdma.sh server [args...]
#   ./run-with-rdma.sh client [args...]
#   ./run-with-rdma.sh bench [args...]

# Set library paths for custom-built RDMA libraries
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4/src:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric/lib:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/usr/local/cuda/lib64:${LD_LIBRARY_PATH}"

# Check if first argument is provided
if [ $# -eq 0 ]; then
    echo "Usage: $0 {server|client|bench} [args...]"
    echo ""
    echo "Examples:"
    echo "  $0 server                              # Start server"
    echo "  $0 server --memory-mb 2048             # Start server with 2GB memory"
    echo "  $0 client get mykey                    # Get a key"
    echo "  $0 client put mykey myvalue            # Put a key"
    echo "  $0 bench                               # Run benchmark with defaults"
    echo "  $0 bench --num-workers 32              # Run benchmark with 32 workers"
    exit 1
fi

# Determine which binary to run
COMMAND=$1
shift  # Remove first argument, keep the rest

case "$COMMAND" in
    server)
        BINARY="./target/release/kv-server"
        ;;
    client)
        BINARY="./target/release/kv-client"
        ;;
    bench|benchmark)
        BINARY="./target/release/kv-bench"
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        echo "Valid commands: server, client, bench"
        echo ""
        echo "Usage: $0 {server|client|bench} [args...]"
        exit 1
        ;;
esac

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found: $BINARY"
    echo ""
    echo "Please build first:"
    echo "  ./build-with-rdma.sh all"
    exit 1
fi

# Execute the binary with remaining arguments
exec "$BINARY" "$@"
