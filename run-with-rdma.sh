#!/bin/bash
# Helper script to run KV cache with proper library paths for RDMA

# Set library paths for custom-built RDMA libraries
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4/src:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric/lib:${LD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="/usr/local/cuda/lib64:${LD_LIBRARY_PATH}"

echo "RDMA Library paths set:"
echo "  GDR API: /mnt/user-data/home/nvbkdw/workspace/fabric/build/gdrcopy-2.4.4/src"
echo "  libfabric: /mnt/user-data/home/nvbkdw/workspace/fabric/build/libfabric/lib"
echo "  CUDA: /usr/local/cuda/lib64"
echo ""

# Execute the command
exec "$@"
