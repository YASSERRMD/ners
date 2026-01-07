//! NERS Core Kernel – Single-threaded stage pipeline
//!
//! This crate contains the core data structures and stages for the NERS web server.
//! The architecture follows a 6-stage pipeline:
//! NetIn → Parse → Route → App → Encode → NetOut

pub mod conn;
pub mod handlers;
pub mod net;
pub mod queue;
pub mod stage;
