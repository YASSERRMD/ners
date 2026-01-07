//! NERS HTTP Protocol Library
//!
//! Provides zero-copy HTTP/1.1 parsing and response generation.

pub mod parser;
pub mod response;

pub use parser::{HttpParser, HttpRequest, ParseError};
pub use response::HttpResponse;
