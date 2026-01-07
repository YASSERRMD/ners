//! NUMA Topology Detection
//!
//! Detects NUMA nodes and provides memory locality information.



/// NUMA topology information
#[derive(Debug, Clone)]
pub struct NumaInfo {
    /// Number of NUMA nodes
    pub num_nodes: usize,
    /// Number of cores per node
    pub cores_per_node: usize,
    /// Total number of cores
    pub total_cores: usize,
    /// Mapping from core ID to NUMA node
    core_to_node: Vec<usize>,
}

impl NumaInfo {
    /// Detect NUMA topology
    pub fn detect() -> Self {
        let total_cores = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1);
        
        #[cfg(target_os = "linux")]
        {
            Self::detect_linux(total_cores)
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // macOS and others: single NUMA node
            Self {
                num_nodes: 1,
                cores_per_node: total_cores,
                total_cores,
                core_to_node: vec![0; total_cores],
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn detect_linux(total_cores: usize) -> Self {
        // Try to read from sysfs
        let num_nodes = std::fs::read_dir("/sys/devices/system/node")
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("node"))
                    .count()
            })
            .unwrap_or(1)
            .max(1);

        let cores_per_node = total_cores / num_nodes;
        
        // Build core-to-node mapping
        let core_to_node: Vec<usize> = (0..total_cores)
            .map(|core| core / cores_per_node)
            .collect();

        Self {
            num_nodes,
            cores_per_node,
            total_cores,
            core_to_node,
        }
    }

    /// Get NUMA node for a core
    pub fn node_for_core(&self, core_id: usize) -> usize {
        self.core_to_node.get(core_id).copied().unwrap_or(0)
    }

    /// Get all cores in a NUMA node
    pub fn cores_in_node(&self, node: usize) -> Vec<usize> {
        self.core_to_node
            .iter()
            .enumerate()
            .filter(|(_, &n)| n == node)
            .map(|(core, _)| core)
            .collect()
    }

    /// Check if two cores are on the same NUMA node
    pub fn same_node(&self, core_a: usize, core_b: usize) -> bool {
        self.node_for_core(core_a) == self.node_for_core(core_b)
    }
}

impl Default for NumaInfo {
    fn default() -> Self {
        Self::detect()
    }
}

/// Get number of available CPUs
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_detection() {
        let info = NumaInfo::detect();
        assert!(info.num_nodes >= 1);
        assert!(info.total_cores >= 1);
        assert_eq!(info.core_to_node.len(), info.total_cores);
    }

    #[test]
    fn test_node_for_core() {
        let info = NumaInfo::detect();
        // Core 0 should always be on node 0
        assert_eq!(info.node_for_core(0), 0);
    }
}
