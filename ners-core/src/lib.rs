//! NERS Core Kernel – Multi-core stage pipeline with io_uring
//!
//! This crate contains the core data structures and stages for the NERS web server.
//! 
//! ## Architecture (Phase 2)
//! 
//! Each stage runs on a dedicated core:
//! NetIn → Parse → Route → App → Encode → NetOut

pub mod affinity;
pub mod conn;
pub mod executor;
pub mod handlers;
pub mod mux;
pub mod net;
pub mod orchestrator;
pub mod queue;
pub mod stage;
