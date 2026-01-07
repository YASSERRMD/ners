//! HTTP Response Builder
//!
//! Provides HTTP response creation and serialization.

use bytes::{Bytes, BytesMut};
use std::collections::HashMap;

/// HTTP response
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// Status code (200, 404, 500, etc.)
    pub status: u16,
    /// Status text (OK, Not Found, etc.)
    pub status_text: String,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body
    pub body: Bytes,
}

impl HttpResponse {
    /// Create a new HTTP response
    pub fn new(status: u16, body: impl Into<Bytes>) -> Self {
        let status_text = match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        }
        .to_string();
        
        let body = body.into();
        let mut headers = HashMap::new();
        headers.insert("Content-Length".to_string(), body.len().to_string());
        headers.insert("Content-Type".to_string(), "text/plain".to_string());
        
        Self {
            status,
            status_text,
            headers,
            body,
        }
    }

    /// Create an OK (200) response
    pub fn ok(body: impl Into<Bytes>) -> Self {
        Self::new(200, body)
    }

    /// Create a Not Found (404) response
    pub fn not_found() -> Self {
        Self::new(404, "404 Not Found")
    }

    /// Create an Internal Server Error (500) response
    pub fn internal_error() -> Self {
        Self::new(500, "500 Internal Server Error")
    }

    /// Set a header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set content type to JSON
    pub fn json(mut self) -> Self {
        self.headers.insert("Content-Type".to_string(), "application/json".to_string());
        self
    }

    /// Set content type to HTML
    pub fn html(mut self) -> Self {
        self.headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
        self
    }

    /// Serialize the response to bytes
    pub fn to_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(256 + self.body.len());
        
        // Status line
        buf.extend_from_slice(format!("HTTP/1.1 {} {}\r\n", self.status, self.status_text).as_bytes());
        
        // Headers
        for (key, value) in &self.headers {
            buf.extend_from_slice(format!("{}: {}\r\n", key, value).as_bytes());
        }
        
        // End of headers
        buf.extend_from_slice(b"\r\n");
        
        // Body
        buf.extend_from_slice(&self.body);
        
        buf.freeze()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_response() {
        let resp = HttpResponse::ok("Hello, World!");
        let bytes = resp.to_bytes();
        let text = std::str::from_utf8(&bytes).unwrap();
        
        assert!(text.contains("HTTP/1.1 200 OK"));
        assert!(text.contains("Hello, World!"));
    }

    #[test]
    fn test_json_response() {
        let resp = HttpResponse::ok(r#"{"status":"ok"}"#).json();
        let bytes = resp.to_bytes();
        let text = std::str::from_utf8(&bytes).unwrap();
        
        assert!(text.contains("application/json"));
    }

    #[test]
    fn test_not_found() {
        let resp = HttpResponse::not_found();
        let bytes = resp.to_bytes();
        let text = std::str::from_utf8(&bytes).unwrap();
        
        assert!(text.contains("HTTP/1.1 404 Not Found"));
    }
}
