//! NERS Core Kernel – Multi-core stage pipeline with autotuning
//!
//! This crate contains the core data structures and stages for the NERS web server.
//! 
//! ## Architecture (Phase 3)
//! 
//! Each stage runs on a dedicated core with self-adaptive tuning:
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
pub mod stage;
pub mod tuner;
