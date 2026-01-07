//! Sharded Slab Manager for Lock-Free Per-Core State
//!
//! Each core owns its own slab shard, eliminating lock contention.

use crate::conn::ConnState;
use crate::slab::numa::NumaInfo;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Per-core slab shard
pub struct SlabShard {
    /// Core ID that owns this shard
    pub core_id: usize,
    /// NUMA node for this shard
    pub numa_node: usize,
    /// Connection slots
    slots: Vec<Option<ConnState>>,
    /// Free slot indices
    free_list: Vec<usize>,
    /// Total allocated count
    allocated: AtomicUsize,
    /// Capacity
    capacity: usize,
}

impl SlabShard {
    /// Create a new slab shard
    pub fn new(core_id: usize, numa_node: usize, capacity: usize) -> Self {
        Self {
            core_id,
            numa_node,
            slots: (0..capacity).map(|_| None).collect(),
            free_list: (0..capacity).rev().collect(),
            allocated: AtomicUsize::new(0),
            capacity,
        }
    }

    /// Allocate a connection slot
    pub fn allocate(&mut self) -> Option<usize> {
        let slot_idx = self.free_list.pop()?;
        self.allocated.fetch_add(1, Ordering::Relaxed);
        Some(slot_idx)
    }

    /// Deallocate a connection slot
    pub fn deallocate(&mut self, slot_idx: usize) {
        if slot_idx < self.capacity {
            self.slots[slot_idx] = None;
            self.free_list.push(slot_idx);
            self.allocated.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Insert a connection at a slot
    pub fn insert(&mut self, slot_idx: usize, conn: ConnState) -> bool {
        if slot_idx < self.capacity {
            self.slots[slot_idx] = Some(conn);
            true
        } else {
            false
        }
    }

    /// Get a connection by slot index
    pub fn get(&self, slot_idx: usize) -> Option<&ConnState> {
        self.slots.get(slot_idx)?.as_ref()
    }

    /// Get a mutable connection by slot index
    pub fn get_mut(&mut self, slot_idx: usize) -> Option<&mut ConnState> {
        self.slots.get_mut(slot_idx)?.as_mut()
    }

    /// Get number of allocated slots
    pub fn len(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    /// Check if shard is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Sharded slab manager with per-core shards
pub struct ShardedSlabManager {
    /// Per-core shards
    shards: Vec<SlabShard>,
    /// Number of shards (cores)
    num_shards: usize,
    /// NUMA topology info
    numa_info: NumaInfo,
    /// Capacity per shard
    capacity_per_shard: usize,
}

impl ShardedSlabManager {
    /// Create a new sharded slab manager
    pub fn new(num_shards: usize, capacity_per_shard: usize) -> Self {
        let numa_info = NumaInfo::detect();
        
        let shards: Vec<SlabShard> = (0..num_shards)
            .map(|core_id| {
                let numa_node = numa_info.node_for_core(core_id);
                SlabShard::new(core_id, numa_node, capacity_per_shard)
            })
            .collect();

        Self {
            shards,
            num_shards,
            numa_info,
            capacity_per_shard,
        }
    }

    /// Get a shard by core ID
    pub fn get_shard(&self, core_id: usize) -> Option<&SlabShard> {
        self.shards.get(core_id)
    }

    /// Get a mutable shard by core ID
    pub fn get_shard_mut(&mut self, core_id: usize) -> Option<&mut SlabShard> {
        self.shards.get_mut(core_id)
    }

    /// Allocate a connection on a specific core
    pub fn allocate(&mut self, core_id: usize, conn: ConnState) -> Option<(usize, usize)> {
        let shard = self.shards.get_mut(core_id)?;
        let slot_idx = shard.allocate()?;
        shard.insert(slot_idx, conn);
        Some((core_id, slot_idx))
    }

    /// Deallocate a connection
    pub fn deallocate(&mut self, core_id: usize, slot_idx: usize) {
        if let Some(shard) = self.shards.get_mut(core_id) {
            shard.deallocate(slot_idx);
        }
    }

    /// Get a connection
    pub fn get(&self, core_id: usize, slot_idx: usize) -> Option<&ConnState> {
        self.shards.get(core_id)?.get(slot_idx)
    }

    /// Get a mutable connection
    pub fn get_mut(&mut self, core_id: usize, slot_idx: usize) -> Option<&mut ConnState> {
        self.shards.get_mut(core_id)?.get_mut(slot_idx)
    }

    /// Get total allocated connections across all shards
    pub fn total_allocated(&self) -> usize {
        self.shards.iter().map(|s| s.len()).sum()
    }

    /// Get number of shards
    pub fn num_shards(&self) -> usize {
        self.num_shards
    }

    /// Get NUMA info
    pub fn numa_info(&self) -> &NumaInfo {
        &self.numa_info
    }

    /// Find the least loaded core (for load balancing)
    pub fn least_loaded_core(&self) -> usize {
        self.shards
            .iter()
            .enumerate()
            .min_by_key(|(_, shard)| shard.len())
            .map(|(id, _)| id)
            .unwrap_or(0)
    }

    /// Find the least loaded core on a specific NUMA node
    pub fn least_loaded_core_on_node(&self, numa_node: usize) -> Option<usize> {
        self.shards
            .iter()
            .enumerate()
            .filter(|(_, shard)| shard.numa_node == numa_node)
            .min_by_key(|(_, shard)| shard.len())
            .map(|(id, _)| id)
    }
}

impl Default for ShardedSlabManager {
    fn default() -> Self {
        let num_cores = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        Self::new(num_cores, 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slab_shard_allocate() {
        let mut shard = SlabShard::new(0, 0, 10);
        
        let slot1 = shard.allocate();
        assert!(slot1.is_some());
        
        let slot2 = shard.allocate();
        assert!(slot2.is_some());
        assert_ne!(slot1, slot2);
        
        assert_eq!(shard.len(), 2);
    }

    #[test]
    fn test_slab_shard_deallocate() {
        let mut shard = SlabShard::new(0, 0, 10);
        
        let slot = shard.allocate().unwrap();
        assert_eq!(shard.len(), 1);
        
        shard.deallocate(slot);
        assert_eq!(shard.len(), 0);
    }

    #[test]
    fn test_sharded_manager_creation() {
        let manager = ShardedSlabManager::new(4, 100);
        
        assert_eq!(manager.num_shards(), 4);
        assert_eq!(manager.total_allocated(), 0);
    }

    #[test]
    fn test_least_loaded_core() {
        let manager = ShardedSlabManager::new(4, 100);
        let core = manager.least_loaded_core();
        assert!(core < 4);
    }

    #[test]
    fn test_get_shard() {
        let manager = ShardedSlabManager::new(4, 100);
        assert!(manager.get_shard(0).is_some());
        assert!(manager.get_shard(3).is_some());
        assert!(manager.get_shard(4).is_none());
    }
}

