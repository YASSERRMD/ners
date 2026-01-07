//! TCP Network I/O for NERS
//!
//! Provides non-blocking TCP listener and stream wrappers.

use bytes::BytesMut;
use std::io::{self, Read, Write};
use std::net::{TcpListener as StdTcpListener, TcpStream as StdTcpStream, ToSocketAddrs};

/// Non-blocking TCP listener wrapper
pub struct TcpListener {
    inner: StdTcpListener,
}

impl TcpListener {
    /// Create a new TCP listener bound to the given address
    pub fn new<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let listener = StdTcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        
        Ok(Self { inner: listener })
    }

    /// Accept all pending connections (non-blocking)
    pub fn accept_all(&mut self) -> Vec<StdTcpStream> {
        let mut streams = Vec::new();
        
        loop {
            match self.inner.accept() {
                Ok((stream, _addr)) => {
                    // Configure the stream
                    let _ = stream.set_nonblocking(true);
                    let _ = stream.set_nodelay(true);
                    streams.push(stream);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(_) => {
                    break;
                }
            }
        }
        
        streams
    }

    /// Get the local address this listener is bound to
    pub fn local_addr(&self) -> io::Result<std::net::SocketAddr> {
        self.inner.local_addr()
    }
}

/// Read all available data from a stream into a buffer (non-blocking)
pub fn read_all(stream: &mut StdTcpStream, buf: &mut BytesMut) -> io::Result<usize> {
    let mut total_read = 0;
    let mut temp_buf = [0u8; 4096];
    
    loop {
        match stream.read(&mut temp_buf) {
            Ok(0) => {
                // Connection closed
                return Err(io::Error::new(io::ErrorKind::ConnectionReset, "Connection closed"));
            }
            Ok(n) => {
                buf.extend_from_slice(&temp_buf[..n]);
                total_read += n;
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    
    Ok(total_read)
}

/// Write all data from a buffer to a stream (non-blocking)
/// Returns the number of bytes written
pub fn write_all(stream: &mut StdTcpStream, buf: &[u8], offset: usize) -> io::Result<usize> {
    let mut total_written = 0;
    let data = &buf[offset..];
    
    if data.is_empty() {
        return Ok(0);
    }
    
    loop {
        match stream.write(&data[total_written..]) {
            Ok(0) => {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "Write returned 0"));
            }
            Ok(n) => {
                total_written += n;
                if total_written >= data.len() {
                    break;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    
    Ok(total_written)
}

/// Check if a stream is closed
pub fn is_closed(stream: &StdTcpStream) -> bool {
    let mut buf = [0u8; 1];
    match stream.peek(&mut buf) {
        Ok(0) => true,
        Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_listener_new() {
        let listener = TcpListener::new("127.0.0.1:0").unwrap();
        assert!(listener.local_addr().is_ok());
    }

    #[test]
    fn test_accept_all() {
        let mut listener = TcpListener::new("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        
        // Create a client connection
        let _client = StdTcpStream::connect(addr).unwrap();
        
        // Give time for the connection to be established
        thread::sleep(Duration::from_millis(10));
        
        let streams = listener.accept_all();
        assert_eq!(streams.len(), 1);
    }
}
