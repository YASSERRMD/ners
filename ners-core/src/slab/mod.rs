//! Sharded Slab Module for Lock-Free Per-Core State
//!
//! Provides per-core slab sharding to eliminate lock contention.

pub mod sharded;
pub mod numa;

pub use sharded::{ShardedSlabManager, SlabShard};
pub use numa::NumaInfo;
