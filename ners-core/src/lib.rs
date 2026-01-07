//! NERS Core Kernel – Multi-core stage pipeline with autotuning
//!
//! This crate contains the core data structures and stages for the NERS web server.
//! 
//! ## Architecture (Phase 4B)
//! 
//! Each stage runs on a dedicated core with per-core slab sharding:
//! NetIn → Parse → Route → App → Encode → NetOut

pub mod affinity;
pub mod analyzer;
pub mod conn;
pub mod executor;
pub mod handlers;
pub mod mux;
pub mod net;
pub mod orchestrator;
pub mod policy;
pub mod queue;
pub mod slab;
pub mod stage;
pub mod tuner;
