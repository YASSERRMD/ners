//! HTTP/1.1 Request Parser
//!
//! Simple, fast HTTP/1.1 request parser using string operations.

use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use thiserror::Error;

/// HTTP parse errors
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Incomplete request, need more data")]
    Incomplete,
    #[error("Invalid HTTP request: {0}")]
    Invalid(String),
}

/// Parsed HTTP request
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// Request path
    pub path: String,
    /// HTTP version (HTTP/1.0, HTTP/1.1)
    pub version: String,
    /// Request headers
    pub headers: HashMap<String, String>,
    /// Request body (for POST, PUT, etc.)
    pub body: Bytes,
}

/// HTTP/1.1 request parser
pub struct HttpParser {
    /// Minimum bytes needed before attempting to parse
    #[allow(dead_code)]
    min_bytes: usize,
}

impl HttpParser {
    /// Create a new HTTP parser
    pub fn new() -> Self {
        Self { min_bytes: 16 }
    }

    /// Parse an HTTP request from a buffer
    /// Returns the parsed request and the number of bytes consumed
    pub fn parse(&mut self, buf: &BytesMut) -> Result<(HttpRequest, usize), ParseError> {
        let data = std::str::from_utf8(buf).map_err(|_| ParseError::Invalid("Invalid UTF-8".to_string()))?;
        
        // Find the end of headers
        let header_end = data.find("\r\n\r\n").ok_or(ParseError::Incomplete)?;
        let headers_data = &data[..header_end];
        
        // Parse request line
        let first_line_end = headers_data.find("\r\n").ok_or(ParseError::Incomplete)?;
        let request_line = &headers_data[..first_line_end];
        
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(ParseError::Invalid("Invalid request line".to_string()));
        }
        
        let method = parts[0].to_string();
        let path = parts[1].to_string();
        let version = parts[2].to_string();
        
        // Parse headers
        let mut headers = HashMap::new();
        for line in headers_data[first_line_end + 2..].lines() {
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_lowercase();
                let value = line[colon_pos + 1..].trim().to_string();
                headers.insert(key, value);
            }
        }
        
        // Calculate content length
        let content_length: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        
        let body_start = header_end + 4;
        let total_length = body_start + content_length;
        
        // Check if we have the full body
        if buf.len() < total_length {
            return Err(ParseError::Incomplete);
        }
        
        let body = if content_length > 0 {
            Bytes::copy_from_slice(&buf[body_start..total_length])
        } else {
            Bytes::new()
        };
        
        Ok((
            HttpRequest {
                method,
                path,
                version,
                headers,
                body,
            },
            total_length,
        ))
    }
}

impl Default for HttpParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_get_request() {
        let mut parser = HttpParser::new();
        let mut buf = BytesMut::from("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
        
        let (req, consumed) = parser.parse(&buf).unwrap();
        
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/");
        assert_eq!(req.version, "HTTP/1.1");
        assert_eq!(consumed, 35);
    }

    #[test]
    fn test_parse_post_request() {
        let mut parser = HttpParser::new();
        let mut buf = BytesMut::from(
            "POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: 11\r\n\r\nHello World"
        );
        
        let (req, consumed) = parser.parse(&buf).unwrap();
        
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/api");
        assert_eq!(req.body.as_ref(), b"Hello World");
        assert_eq!(consumed, 70);
    }

    #[test]
    fn test_incomplete_request() {
        let mut parser = HttpParser::new();
        let mut buf = BytesMut::from("GET / HTTP/1.1\r\n");
        
        assert!(matches!(parser.parse(&buf), Err(ParseError::Incomplete)));
    }
}
