//! Integration tests for NERS Phase 1
//!
//! Tests end-to-end request/response flows.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

/// Helper to start the server for testing
fn start_test_server() -> Option<Child> {
    // Try to start the server
    let child = Command::new("cargo")
        .args(["run", "--release"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .spawn()
        .ok()?;
    
    // Wait for server to be ready
    thread::sleep(Duration::from_millis(500));
    
    Some(child)
}

/// Helper to send a request and get response
fn send_request(request: &str) -> Result<String, std::io::Error> {
    let mut stream = TcpStream::connect("127.0.0.1:8080")?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    
    stream.write_all(request.as_bytes())?;
    
    let mut response = vec![0u8; 4096];
    let n = stream.read(&mut response)?;
    
    Ok(String::from_utf8_lossy(&response[..n]).to_string())
}

#[test]
#[ignore] // Requires running server
fn test_hello_route() {
    let response = send_request("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    
    assert!(response.contains("HTTP/1.1 200 OK"));
    assert!(response.contains("Hello, World!"));
}

#[test]
#[ignore] // Requires running server
fn test_api_route() {
    let response = send_request("GET /api/test HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    
    assert!(response.contains("HTTP/1.1 200 OK"));
    assert!(response.contains("application/json"));
    assert!(response.contains(r#""status": "ok""#));
}

#[test]
#[ignore] // Requires running server
fn test_404() {
    let response = send_request("GET /nonexistent HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    
    assert!(response.contains("HTTP/1.1 404 Not Found"));
}

#[test]
#[ignore] // Requires running server
fn test_concurrent_requests() {
    let handles: Vec<_> = (0..10)
        .map(|_| {
            thread::spawn(|| {
                let response = send_request("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
                response.is_ok()
            })
        })
        .collect();
    
    let successes: usize = handles
        .into_iter()
        .filter_map(|h| h.join().ok())
        .filter(|&success| success)
        .count();
    
    assert!(successes >= 8, "At least 80% of concurrent requests should succeed");
}

#[test]
#[ignore] // Requires running server
fn test_post_request() {
    let request = "POST /api/test HTTP/1.1\r\nHost: localhost\r\nContent-Length: 11\r\n\r\nHello World";
    let response = send_request(request).unwrap();
    
    // POST to /api/test should still return our JSON handler response
    assert!(response.contains("HTTP/1.1 200 OK"));
}

#[test]
#[ignore] // Requires running server
fn test_connection_close() {
    // Test that server handles client disconnect gracefully
    let mut stream = TcpStream::connect("127.0.0.1:8080").unwrap();
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n").unwrap();
    // Close without sending final \r\n
    drop(stream);
    
    // Server should not crash, verify by making another request
    thread::sleep(Duration::from_millis(100));
    let response = send_request("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert!(response.is_ok());
}
