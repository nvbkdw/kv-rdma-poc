#!/bin/bash
# Helper script to build KV cache with RDMA support
#
# This script sets the required environment variables for building
# with fabric-lib and RDMA dependencies.

set -e

# Set paths for RDMA libraries (adjust if your paths differ)
export GDRAPI_HOME=/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4
export LIBFABRIC_HOME=/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric

echo "======================================"
echo "Building KV-RDMA-POC with RDMA support"
echo "======================================"
echo "GDRAPI_HOME:    $GDRAPI_HOME"
echo "LIBFABRIC_HOME: $LIBFABRIC_HOME"
echo ""

# Check if paths exist
if [ ! -d "$GDRAPI_HOME" ]; then
    echo "ERROR: GDRAPI_HOME directory not found: $GDRAPI_HOME"
    echo "Please adjust the path in this script or set GDRAPI_HOME environment variable"
    exit 1
fi

if [ ! -d "$LIBFABRIC_HOME" ]; then
    echo "ERROR: LIBFABRIC_HOME directory not found: $LIBFABRIC_HOME"
    echo "Please adjust the path in this script or set LIBFABRIC_HOME environment variable"
    exit 1
fi

# Build with RDMA features
if [ "$1" = "bench" ]; then
    echo "Building benchmark binary..."
    cargo build --release --bin kv-bench --features rdma
    echo ""
    echo "Build complete!"
    echo "Run with: ./run-with-rdma.sh ./target/release/kv-bench"
elif [ "$1" = "server" ]; then
    echo "Building server binary..."
    cargo build --release --bin kv-server --features rdma
    echo ""
    echo "Build complete!"
    echo "Run with: ./run-with-rdma.sh ./target/release/kv-server"
elif [ "$1" = "client" ]; then
    echo "Building client binary..."
    cargo build --release --bin kv-client --features rdma
    echo ""
    echo "Build complete!"
    echo "Run with: ./run-with-rdma.sh ./target/release/kv-client"
elif [ "$1" = "all" ] || [ -z "$1" ]; then
    echo "Building all binaries..."
    cargo build --release --features rdma
    echo ""
    echo "Build complete!"
    echo "Binaries available:"
    echo "  ./target/release/kv-server"
    echo "  ./target/release/kv-client"
    echo "  ./target/release/kv-bench"
    echo ""
    echo "Run with: ./run-with-rdma.sh ./target/release/<binary>"
else
    echo "Usage: $0 [all|server|client|bench]"
    echo ""
    echo "Examples:"
    echo "  $0          # Build all binaries (default)"
    echo "  $0 all      # Build all binaries"
    echo "  $0 server   # Build server only"
    echo "  $0 client   # Build client only"
    echo "  $0 bench    # Build benchmark only"
    exit 1
fi
