# Known Issues and Workarounds

## fabric-lib Topology Detection Issue with Host Memory

### Problem
When starting the server with `--mock false`, you may see this error:

```
Error: No GPU-RDMA topology found. This typically means:
1. GPUs and EFA are on different PCI domains (common in some instance types)
2. For host memory transfers, we need manual configuration
```

### Root Cause
- fabric-lib's `detect_topology()` expects GPUs and RDMA NICs to be co-located on the same PCI tree for **GPU Direct RDMA**
- This is necessary for GPU-to-GPU transfers over RDMA
- However, for **host memory** (CPU) transfers, this tight coupling is not required
- In your system, the GPUs (PCI 0000:37-3d) and EFA (PCI 0000:31) are on different domains

### Current Status
The kv-rdma-poc uses **host memory** for the KV cache, not GPU memory. The `MemoryPool` is allocated in CPU RAM and registered with RDMA. Therefore, we don't need GPU-NIC affinity.

However, fabric-lib's current design requires passing through the topology detection even for host-only usage.

### Workarounds

#### Option 1: Use Mock Transport (Recommended for Development)
Mock transport works perfectly for testing the protocol and logic:

```bash
# Server (same process as client, for testing)
cargo run --bin kv-server

# In integration tests
cargo test
```

**Limitations**: Mock transport only works when client and server are in the same process (uses memcpy, not real RDMA).

#### Option 2: Modify fabric-lib (Production Solution)
To enable host-memory RDMA without GPU-NIC affinity:

1. Add a `TransferEngine::new_host_only()` constructor that:
   - Takes EFA/verbs domains directly
   - Doesn't require GPU device association
   - Uses CPU pinning on any available NUMA node

2. Or add a `Worker::host_memory()` constructor that uses a sentinel GPU ID

Example modification to fabric-lib:
```rust
// In fabric-lib/src/transfer_engine.rs
impl TransferEngine {
    pub fn new_host_only(
        domains: Vec<DomainInfo>,
        pin_worker_cpu: u16,
        pin_uvm_cpu: u16,
    ) -> Result<Self> {
        let worker = Worker {
            domain_list: domains,
            pin_worker_cpu: Some(pin_worker_cpu),
            pin_uvm_cpu: Some(pin_uvm_cpu),
        };
        // Use GPU ID 255 as sentinel for host-only
        Self::new(vec![(255, worker)])
    }
}
```

#### Option 3: Instance Type with GPU-EFA Co-location
Some AWS instance types have GPUs and EFA on the same PCI tree:
- p4d.24xlarge (8x A100 GPUs + 4x EFA)
- p5.48xlarge (8x H100 GPUs + 32x EFA)

These instances are designed for GPU Direct RDMA and would work without modification.

### Testing Your Setup

Check your PCI topology:
```bash
lspci -tv | grep -A2 -B2 "NVIDIA\|EFA"
```

Check NUMA affinity:
```bash
# GPUs
for gpu in /sys/class/drm/card*/device; do
    if [[ -e "$gpu/numa_node" ]]; then
        echo "GPU: $(basename $(dirname $gpu)) NUMA: $(cat $gpu/numa_node)"
    fi
done

# EFA
cat /sys/class/infiniband/rdmap49s0/device/numa_node
```

If GPUs and EFA are on the same NUMA node and PCI tree, topology detection should work.

### Impact
- **Development/Testing**: Use mock transport or in-process integration tests
- **Production with EFA**: Requires fabric-lib modification OR instance type with GPU-EFA co-location
- **Performance**: Once working, should achieve full EFA RDMA bandwidth for host memory transfers

### Next Steps
1. For immediate development: Use mock transport mode
2. For production EFA: Either modify fabric-lib or use appropriate instance type
3. Alternative: Consider using raw libfabric/libibverbs directly without fabric-lib wrapper
