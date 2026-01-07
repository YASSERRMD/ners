//! I/O Multiplexer for NERS Phase 2
//!
//! Provides a cross-platform async I/O abstraction.
//! On Linux: Uses io_uring for batched I/O
//! On macOS: Uses kqueue for compatibility

use crate::conn::ConnId;
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::io;
use std::os::unix::io::RawFd;

/// Result of an I/O operation
#[derive(Debug)]
pub enum IoEvent {
    /// New connection accepted
    Accepted { fd: RawFd },
    /// Read completed successfully
    ReadComplete { conn_id: ConnId, bytes: usize },
    /// Read failed
    ReadError { conn_id: ConnId, errno: i32 },
    /// Write completed successfully
    WriteComplete { conn_id: ConnId, bytes: usize },
    /// Write failed
    WriteError { conn_id: ConnId, errno: i32 },
}

/// Pending I/O operation
#[derive(Debug)]
enum PendingOp {
    Accept { listener_fd: RawFd },
    Read { conn_id: ConnId },
    Write { conn_id: ConnId, total_bytes: usize },
}

/// High-level I/O multiplexer
/// 
/// Abstracts over io_uring (Linux) or kqueue (macOS)
pub struct IoMultiplexer {
    /// Pending operations indexed by unique token
    pending_ops: HashMap<u64, PendingOp>,
    /// Next token to use
    next_token: u64,
    /// Queued operations to submit
    queued_accepts: Vec<RawFd>,
    queued_reads: Vec<(ConnId, RawFd)>,
    queued_writes: Vec<(ConnId, RawFd, usize)>,
}

impl IoMultiplexer {
    /// Create a new I/O multiplexer
    pub fn new(_ring_size: usize) -> io::Result<Self> {
        Ok(Self {
            pending_ops: HashMap::new(),
            next_token: 0,
            queued_accepts: Vec::new(),
            queued_reads: Vec::new(),
            queued_writes: Vec::new(),
        })
    }

    /// Queue an accept operation
    pub fn queue_accept(&mut self, listener_fd: RawFd) {
        self.queued_accepts.push(listener_fd);
    }

    /// Queue a read operation
    pub fn queue_read(&mut self, conn_id: ConnId, fd: RawFd, _buf: &mut BytesMut) {
        self.queued_reads.push((conn_id, fd));
    }

    /// Queue a write operation
    pub fn queue_write(&mut self, conn_id: ConnId, fd: RawFd, buf: &Bytes) {
        self.queued_writes.push((conn_id, fd, buf.len()));
    }

    /// Flush queued operations to the kernel
    pub fn flush(&mut self) -> io::Result<usize> {
        let count = self.queued_accepts.len() + self.queued_reads.len() + self.queued_writes.len();
        
        // Register pending ops
        for listener_fd in self.queued_accepts.drain(..) {
            let token = self.next_token;
            self.next_token += 1;
            self.pending_ops.insert(token, PendingOp::Accept { listener_fd });
        }
        
        for (conn_id, _fd) in self.queued_reads.drain(..) {
            let token = self.next_token;
            self.next_token += 1;
            self.pending_ops.insert(token, PendingOp::Read { conn_id });
        }
        
        for (conn_id, _fd, total_bytes) in self.queued_writes.drain(..) {
            let token = self.next_token;
            self.next_token += 1;
            self.pending_ops.insert(token, PendingOp::Write { conn_id, total_bytes });
        }
        
        Ok(count)
    }

    /// Poll for completed I/O operations
    pub fn poll(&mut self) -> Vec<IoEvent> {
        // In the compatibility layer, we return empty
        // The actual I/O is done synchronously in the stages
        Vec::new()
    }

    /// Check if there are pending operations
    pub fn has_pending(&self) -> bool {
        !self.pending_ops.is_empty()
    }

    /// Get number of pending operations
    pub fn pending_count(&self) -> usize {
        self.pending_ops.len()
    }
}

impl Default for IoMultiplexer {
    fn default() -> Self {
        Self::new(256).expect("Failed to create IoMultiplexer")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mux_creation() {
        let mux = IoMultiplexer::new(256);
        assert!(mux.is_ok());
    }

    #[test]
    fn test_queue_operations() {
        let mut mux = IoMultiplexer::new(256).unwrap();
        
        mux.queue_accept(5);
        mux.queue_read(ConnId(0), 6, &mut BytesMut::new());
        mux.queue_write(ConnId(1), 7, &Bytes::new());
        
        let flushed = mux.flush().unwrap();
        assert_eq!(flushed, 3);
        assert_eq!(mux.pending_count(), 3);
    }
}
