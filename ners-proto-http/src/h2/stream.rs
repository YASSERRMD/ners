//! HTTP/2 Stream State Machine
//!
//! Implements per-stream state and buffers.

use bytes::BytesMut;

/// HTTP/2 stream states (RFC 7540 Section 5.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Idle,
    ReservedLocal,
    ReservedRemote,
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
}

/// HTTP/2 stream
#[derive(Debug)]
pub struct Stream {
    /// Stream ID
    pub id: u32,
    /// Current state
    pub state: StreamState,
    /// Request headers
    pub request_headers: Vec<(String, String)>,
    /// Response headers
    pub response_headers: Vec<(String, String)>,
    /// Request body
    pub request_body: BytesMut,
    /// Response body
    pub response_body: BytesMut,
    /// Send window size
    pub send_window: i32,
    /// Receive window size
    pub recv_window: i32,
    /// Stream priority weight
    pub weight: u8,
    /// Stream dependency
    pub dependency: u32,
    /// Is exclusive dependency
    pub exclusive: bool,
}

impl Stream {
    /// Create a new stream
    pub fn new(id: u32, initial_window_size: i32) -> Self {
        Self {
            id,
            state: StreamState::Idle,
            request_headers: Vec::new(),
            response_headers: Vec::new(),
            request_body: BytesMut::new(),
            response_body: BytesMut::new(),
            send_window: initial_window_size,
            recv_window: initial_window_size,
            weight: 16, // Default weight
            dependency: 0,
            exclusive: false,
        }
    }

    /// Transition to Open state (after receiving HEADERS)
    pub fn open(&mut self) {
        if self.state == StreamState::Idle {
            self.state = StreamState::Open;
        }
    }

    /// Transition to HalfClosedRemote (after receiving END_STREAM)
    pub fn half_close_remote(&mut self) {
        match self.state {
            StreamState::Open => self.state = StreamState::HalfClosedRemote,
            StreamState::HalfClosedLocal => self.state = StreamState::Closed,
            _ => {}
        }
    }

    /// Transition to HalfClosedLocal (after sending END_STREAM)
    pub fn half_close_local(&mut self) {
        match self.state {
            StreamState::Open => self.state = StreamState::HalfClosedLocal,
            StreamState::HalfClosedRemote => self.state = StreamState::Closed,
            _ => {}
        }
    }

    /// Close the stream
    pub fn close(&mut self) {
        self.state = StreamState::Closed;
    }

    /// Check if stream can send data
    pub fn can_send(&self) -> bool {
        matches!(self.state, StreamState::Open | StreamState::HalfClosedRemote)
    }

    /// Check if stream can receive data
    pub fn can_receive(&self) -> bool {
        matches!(self.state, StreamState::Open | StreamState::HalfClosedLocal)
    }

    /// Update send window
    pub fn update_send_window(&mut self, delta: i32) {
        self.send_window = self.send_window.saturating_add(delta);
    }

    /// Consume send window
    pub fn consume_send_window(&mut self, bytes: i32) {
        self.send_window -= bytes;
    }

    /// Update receive window
    pub fn update_recv_window(&mut self, delta: i32) {
        self.recv_window = self.recv_window.saturating_add(delta);
    }
}

impl Default for Stream {
    fn default() -> Self {
        Self::new(0, 65535)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_state_transitions() {
        let mut stream = Stream::new(1, 65535);
        assert_eq!(stream.state, StreamState::Idle);
        
        stream.open();
        assert_eq!(stream.state, StreamState::Open);
        
        stream.half_close_remote();
        assert_eq!(stream.state, StreamState::HalfClosedRemote);
        
        stream.half_close_local();
        assert_eq!(stream.state, StreamState::Closed);
    }

    #[test]
    fn test_flow_control() {
        let mut stream = Stream::new(1, 65535);
        
        stream.consume_send_window(1000);
        assert_eq!(stream.send_window, 64535);
        
        stream.update_send_window(2000);
        assert_eq!(stream.send_window, 66535);
    }
}
