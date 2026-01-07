//! Request Handlers for NERS
//!
//! Defines route handlers for the web server.

use crate::conn::ConnState;
use ners_proto_http::HttpResponse;

/// The landing page HTML
const LANDING_PAGE: &str = include_str!("static/index.html");

/// Handle the root path - Landing page
pub fn handle_hello(conn: &mut ConnState) {
    let response = HttpResponse::ok(LANDING_PAGE).html();
    let bytes = response.to_bytes();
    conn.write_buf.extend_from_slice(&bytes);
}

/// Handle /api/test - JSON response
pub fn handle_json(conn: &mut ConnState) {
    let response = HttpResponse::ok(r#"{"status": "ok", "message": "NERS - High Performance Web Server"}"#).json();
    let bytes = response.to_bytes();
    conn.write_buf.extend_from_slice(&bytes);
}

/// Handle /health - Health check
pub fn handle_health(conn: &mut ConnState) {
    let response = HttpResponse::ok(r#"{"status": "healthy"}"#).json();
    let bytes = response.to_bytes();
    conn.write_buf.extend_from_slice(&bytes);
}

/// Handle unknown paths - 404
pub fn handle_not_found(conn: &mut ConnState) {
    let response = HttpResponse::not_found();
    let bytes = response.to_bytes();
    conn.write_buf.extend_from_slice(&bytes);
}
