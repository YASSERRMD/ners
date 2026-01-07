//! Request Handlers for NERS
//!
//! Defines route handlers for the web server.

use crate::conn::ConnState;
use ners_proto_http::HttpResponse;

/// Handle the root path - Hello World
pub fn handle_hello(conn: &mut ConnState) {
    let response = HttpResponse::ok("Hello, World!");
    let bytes = response.to_bytes();
    conn.write_buf.extend_from_slice(&bytes);
}

/// Handle /api/test - JSON response
pub fn handle_json(conn: &mut ConnState) {
    let response = HttpResponse::ok(r#"{"status": "ok", "message": "NERS Phase 1"}"#).json();
    let bytes = response.to_bytes();
    conn.write_buf.extend_from_slice(&bytes);
}

/// Handle unknown paths - 404
pub fn handle_not_found(conn: &mut ConnState) {
    let response = HttpResponse::not_found();
    let bytes = response.to_bytes();
    conn.write_buf.extend_from_slice(&bytes);
}
