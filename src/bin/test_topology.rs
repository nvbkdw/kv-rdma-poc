// Test program to debug topology detection

fn main() {
    #[cfg(feature = "rdma")]
    {
        use fabric_lib::{detect_topology, RdmaDomainInfo};

        println!("Attempting to detect RDMA topology...");

        match detect_topology() {
            Ok(topology) => {
                println!("✓ Topology detection succeeded!");
                println!("Found {} GPU groups:", topology.len());

                for (i, topo) in topology.iter().enumerate() {
                    println!("\nGPU Group {}:", i);
                    println!("  CUDA Device: {}", topo.cuda_device);
                    println!("  NUMA Node: {}", topo.numa);
                    println!("  Domains: {}", topo.domains.len());
                    for (j, domain) in topo.domains.iter().enumerate() {
                        println!("    Domain {}: {} (speed: {} Gbps)",
                            j, domain.name(), domain.link_speed() / 1_000_000_000);
                    }
                    println!("  CPUs: {} cores", topo.cpus.len());
                    if !topo.cpus.is_empty() {
                        println!("    CPUs: {:?}", topo.cpus);
                    }
                }
            }
            Err(e) => {
                println!("✗ Topology detection failed: {}", e);
                println!("\nThis could mean:");
                println!("  - No GPU-RDMA association found");
                println!("  - System topology files are not accessible");
                println!("  - NUMA information is missing");
                std::process::exit(1);
            }
        }
    }

    #[cfg(not(feature = "rdma"))]
    {
        println!("This binary was not compiled with RDMA support.");
        println!("Rebuild with: cargo build --features rdma --bin test_topology");
    }
}
