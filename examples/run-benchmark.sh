#!/bin/bash
# Example script to run various benchmark scenarios
#
# IMPORTANT: Requires RDMA hardware and --features rdma build
# For testing without RDMA, use: cargo test

set -e

echo "======================================"
echo "KV-RDMA-POC Benchmark Examples"
echo "======================================"
echo ""
echo "PREREQUISITES:"
echo "1. Build with RDMA support:"
echo "   ./build-with-rdma.sh all"
echo ""
echo "2. Start the server in another terminal:"
echo "   ./run-with-rdma.sh ./target/release/kv-server"
echo ""
echo "Press Enter to continue or Ctrl+C to exit..."
read

# Check if binaries exist
if [ ! -f "./target/release/kv-bench" ]; then
    echo "ERROR: kv-bench binary not found!"
    echo "Please run: ./build-with-rdma.sh all"
    exit 1
fi

# Scenario 1: Small values, high ops/sec
echo ""
echo "=== Scenario 1: Small Values (16KB) ==="
echo "Testing latency and ops/sec with small values"
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 1000 \
  --value-size 16KB \
  --num-threads 4

# Scenario 2: Medium values
echo ""
echo "=== Scenario 2: Medium Values (256KB) ==="
echo "Balanced throughput testing"
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 1000 \
  --value-size 256KB \
  --num-threads 4

# Scenario 3: Large values, RDMA throughput
echo ""
echo "=== Scenario 3: Large Values (1MB) ==="
echo "Testing RDMA throughput with large values"
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 500 \
  --value-size 1MB \
  --num-threads 4 \
  --buffer-mb 128

# Scenario 4: Moderate concurrency
echo ""
echo "=== Scenario 4: Moderate Concurrency (4 threads) ==="
echo "Testing scalability with multiple readers"
./run-with-rdma.sh ./target/release/kv-bench \
  --num-keys 2000 \
  --value-size 64KB \
  --num-threads 4

echo ""
echo "======================================"
echo "All benchmark scenarios completed!"
echo "======================================"
