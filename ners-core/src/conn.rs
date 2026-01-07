//! Connection state and slab allocator for NERS
//!
//! This module provides:
//! - `ConnId`: Zero-cost index into the connection slab
//! - `ConnState`: Per-connection state including buffers and lifecycle
//! - `ConnSlab`: Pre-allocated storage for connections

use bytes::BytesMut;
use std::net::TcpStream;
use std::time::Instant;

/// A unique identifier for a connection in the slab.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ConnId(pub usize);

/// Lifecycle states for a connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnLifecycle {
    /// Waiting for incoming request data
    WaitingForRequest,
    /// Currently parsing the HTTP request
    Parsing,
    /// Request has been routed to a handler
    Routed,
    /// Handler is processing the request
    Processing,
    /// Response is being sent
    Sending,
    /// Connection is closed
    Closed,
}

/// Route identifier for matched handlers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteId(pub usize);

/// Per-connection state
pub struct ConnState {
    /// Incoming data buffer
    pub read_buf: BytesMut,
    /// Outgoing data buffer
    pub write_buf: BytesMut,
    /// Current lifecycle state
    pub state: ConnLifecycle,
    /// Matched route after parsing
    pub route_id: Option<RouteId>,
    /// Parsed HTTP method
    pub request_method: Option<String>,
    /// Parsed HTTP path
    pub request_path: Option<String>,
    /// Connection creation time for latency tracking
    pub created_at: Instant,
    /// The underlying TCP stream
    pub stream: Option<TcpStream>,
    /// Bytes written so far (for partial writes)
    pub bytes_written: usize,
}

impl ConnState {
    /// Create a new connection state with the given stream
    pub fn new(stream: TcpStream) -> Self {
        Self {
            read_buf: BytesMut::with_capacity(4096),
            write_buf: BytesMut::with_capacity(4096),
            state: ConnLifecycle::WaitingForRequest,
            route_id: None,
            request_method: None,
            request_path: None,
            created_at: Instant::now(),
            stream: Some(stream),
            bytes_written: 0,
        }
    }

    /// Reset the connection state for reuse
    pub fn reset(&mut self) {
        self.read_buf.clear();
        self.write_buf.clear();
        self.state = ConnLifecycle::WaitingForRequest;
        self.route_id = None;
        self.request_method = None;
        self.request_path = None;
        self.created_at = Instant::now();
        self.stream = None;
        self.bytes_written = 0;
    }
}

/// Pre-allocated slab for connection storage
pub struct ConnSlab {
    /// Storage for connections
    slots: Vec<Option<ConnState>>,
    /// Free slot indices
    free_list: Vec<usize>,
    /// Capacity of the slab
    capacity: usize,
}

impl ConnSlab {
    /// Create a new slab with the given capacity
    pub fn new(capacity: usize) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        let mut free_list = Vec::with_capacity(capacity);
        
        for i in (0..capacity).rev() {
            slots.push(None);
            free_list.push(i);
        }
        
        Self {
            slots,
            free_list,
            capacity,
        }
    }

    /// Insert a new connection, returning its ID
    pub fn insert(&mut self, conn: ConnState) -> Option<ConnId> {
        if let Some(idx) = self.free_list.pop() {
            self.slots[idx] = Some(conn);
            Some(ConnId(idx))
        } else {
            None
        }
    }

    /// Get a mutable reference to a connection
    pub fn get_mut(&mut self, id: ConnId) -> Option<&mut ConnState> {
        self.slots.get_mut(id.0).and_then(|slot| slot.as_mut())
    }

    /// Get an immutable reference to a connection
    pub fn get(&self, id: ConnId) -> Option<&ConnState> {
        self.slots.get(id.0).and_then(|slot| slot.as_ref())
    }

    /// Remove a connection and free its slot
    pub fn remove(&mut self, id: ConnId) {
        if id.0 < self.capacity {
            if let Some(conn) = self.slots[id.0].as_mut() {
                conn.reset();
            }
            self.slots[id.0] = None;
            self.free_list.push(id.0);
        }
    }

    /// Get the number of vacant slots
    pub fn vacant_count(&self) -> usize {
        self.free_list.len()
    }

    /// Get the number of active connections
    pub fn active_count(&self) -> usize {
        self.capacity - self.free_list.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn create_test_stream() -> TcpStream {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        TcpStream::connect(addr).unwrap()
    }

    #[test]
    fn test_slab_insert_and_get() {
        let mut slab = ConnSlab::new(10);
        let stream = create_test_stream();
        let conn = ConnState::new(stream);
        
        let id = slab.insert(conn).unwrap();
        assert!(slab.get(id).is_some());
        assert_eq!(slab.active_count(), 1);
    }

    #[test]
    fn test_slab_remove() {
        let mut slab = ConnSlab::new(10);
        let stream = create_test_stream();
        let conn = ConnState::new(stream);
        
        let id = slab.insert(conn).unwrap();
        slab.remove(id);
        
        assert!(slab.get(id).is_none());
        assert_eq!(slab.vacant_count(), 10);
    }

    #[test]
    fn test_slab_capacity() {
        let mut slab = ConnSlab::new(2);
        
        let stream1 = create_test_stream();
        let stream2 = create_test_stream();
        let stream3 = create_test_stream();
        
        assert!(slab.insert(ConnState::new(stream1)).is_some());
        assert!(slab.insert(ConnState::new(stream2)).is_some());
        assert!(slab.insert(ConnState::new(stream3)).is_none()); // Full
    }
}
