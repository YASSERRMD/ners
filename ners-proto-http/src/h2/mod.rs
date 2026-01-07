//! HTTP/2 Protocol Implementation
//!
//! Provides HTTP/2 frame parsing, HPACK compression, and stream multiplexing.

pub mod connection;
pub mod frame;
pub mod hpack;
pub mod stream;

pub use connection::H2Connection;
pub use frame::{Frame, FrameType, H2Error};
pub use hpack::{HpackDecoder, HpackEncoder};
pub use stream::{Stream, StreamState};
