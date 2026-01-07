//! HTTP/2 Connection Management
//!
//! Manages HTTP/2 connections with stream multiplexing.

use super::frame::{Frame, H2Error, H2_PREFACE};
use super::hpack::{HpackDecoder, HpackEncoder};
use super::stream::{Stream, StreamState};
use bytes::Bytes;
use std::collections::HashMap;

/// HTTP/2 connection settings
#[derive(Debug, Clone)]
pub struct H2Settings {
    pub header_table_size: u32,
    pub enable_push: bool,
    pub max_concurrent_streams: u32,
    pub initial_window_size: u32,
    pub max_frame_size: u32,
    pub max_header_list_size: u32,
}

impl Default for H2Settings {
    fn default() -> Self {
        Self {
            header_table_size: 4096,
            enable_push: true,
            max_concurrent_streams: 100,
            initial_window_size: 65535,
            max_frame_size: 16384,
            max_header_list_size: 8192,
        }
    }
}

/// HTTP/2 connection
pub struct H2Connection {
    /// Local settings (what we advertise)
    local_settings: H2Settings,
    /// Remote settings (what peer advertises)
    remote_settings: H2Settings,
    /// Active streams
    streams: HashMap<u32, Stream>,
    /// HPACK encoder
    hpack_encoder: HpackEncoder,
    /// HPACK decoder
    hpack_decoder: HpackDecoder,
    /// Next stream ID for server-initiated streams (even)
    #[allow(dead_code)]
    next_server_stream_id: u32,
    /// Connection-level send window
    send_window: i32,
    /// Connection-level receive window
    recv_window: i32,
    /// Has connection preface been received
    preface_received: bool,
    /// Has settings ACK been sent
    settings_acked: bool,
    /// Outgoing frames queue
    outgoing: Vec<Frame>,
    /// Is connection going away
    going_away: bool,
    /// Last stream ID for GOAWAY
    last_stream_id: u32,
}

impl H2Connection {
    /// Create a new HTTP/2 connection (server side)
    pub fn new(settings: H2Settings) -> Self {
        let initial_window = settings.initial_window_size as i32;
        
        Self {
            local_settings: settings.clone(),
            remote_settings: H2Settings::default(),
            streams: HashMap::new(),
            hpack_encoder: HpackEncoder::new(settings.header_table_size as usize),
            hpack_decoder: HpackDecoder::new(4096),
            next_server_stream_id: 2,
            send_window: initial_window,
            recv_window: initial_window,
            preface_received: false,
            settings_acked: false,
            outgoing: Vec::new(),
            going_away: false,
            last_stream_id: 0,
        }
    }

    /// Check if connection preface has been received
    pub fn is_preface_received(&self) -> bool {
        self.preface_received
    }

    /// Process received data, returns whether preface was valid
    pub fn process_preface(&mut self, data: &[u8]) -> bool {
        if data.len() >= H2_PREFACE.len() && &data[..H2_PREFACE.len()] == H2_PREFACE {
            self.preface_received = true;
            // Send our settings
            self.send_settings();
            true
        } else {
            false
        }
    }

    /// Send settings frame
    fn send_settings(&mut self) {
        let settings = vec![
            (0x1, self.local_settings.header_table_size),
            (0x3, self.local_settings.max_concurrent_streams),
            (0x4, self.local_settings.initial_window_size),
            (0x5, self.local_settings.max_frame_size),
        ];
        
        self.outgoing.push(Frame::Settings {
            ack: false,
            settings,
        });
    }

    /// Process an incoming frame
    pub fn process_frame(&mut self, frame: Frame) -> Result<(), H2Error> {
        match frame {
            Frame::Settings { ack, settings } => {
                if ack {
                    self.settings_acked = true;
                } else {
                    // Apply settings
                    for (id, value) in settings {
                        match id {
                            0x1 => self.remote_settings.header_table_size = value,
                            0x2 => self.remote_settings.enable_push = value != 0,
                            0x3 => self.remote_settings.max_concurrent_streams = value,
                            0x4 => {
                                let delta = value as i32 - self.remote_settings.initial_window_size as i32;
                                self.remote_settings.initial_window_size = value;
                                // Update all stream windows
                                for stream in self.streams.values_mut() {
                                    stream.update_send_window(delta);
                                }
                            }
                            0x5 => self.remote_settings.max_frame_size = value,
                            0x6 => self.remote_settings.max_header_list_size = value,
                            _ => {}
                        }
                    }
                    // Send ACK
                    self.outgoing.push(Frame::Settings {
                        ack: true,
                        settings: Vec::new(),
                    });
                }
            }
            Frame::Headers { stream_id, end_stream, end_headers: _, header_block } => {
                // Decode headers
                let headers = self.hpack_decoder.decode(&header_block)
                    .map_err(|e| H2Error::ProtocolError(e))?;
                
                // Create or get stream
                let stream = self.streams.entry(stream_id)
                    .or_insert_with(|| Stream::new(stream_id, self.local_settings.initial_window_size as i32));
                
                stream.open();
                stream.request_headers = headers;
                
                if end_stream {
                    stream.half_close_remote();
                }
                
                self.last_stream_id = self.last_stream_id.max(stream_id);
            }
            Frame::Data { stream_id, end_stream, data } => {
                if let Some(stream) = self.streams.get_mut(&stream_id) {
                    if stream.can_receive() {
                        stream.request_body.extend_from_slice(&data);
                        
                        // Update window
                        let len = data.len() as i32;
                        stream.recv_window -= len;
                        self.recv_window -= len;
                        
                        if end_stream {
                            stream.half_close_remote();
                        }
                    }
                }
            }
            Frame::WindowUpdate { stream_id, increment } => {
                if stream_id == 0 {
                    self.send_window += increment as i32;
                } else if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.update_send_window(increment as i32);
                }
            }
            Frame::Ping { ack, data } => {
                if !ack {
                    self.outgoing.push(Frame::Ping { ack: true, data });
                }
            }
            Frame::GoAway { last_stream_id, error_code, .. } => {
                self.going_away = true;
                log::info!("Received GOAWAY: last_stream={}, error={}", last_stream_id, error_code);
            }
            Frame::RstStream { stream_id, .. } => {
                if let Some(stream) = self.streams.get_mut(&stream_id) {
                    stream.close();
                }
            }
            _ => {}
        }
        
        Ok(())
    }

    /// Send response headers for a stream
    pub fn send_headers(&mut self, stream_id: u32, headers: Vec<(String, String)>, end_stream: bool) {
        let header_block = self.hpack_encoder.encode(&headers);
        
        self.outgoing.push(Frame::Headers {
            stream_id,
            end_stream,
            end_headers: true,
            header_block,
        });
        
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.response_headers = headers;
            if end_stream {
                stream.half_close_local();
            }
        }
    }

    /// Send response data for a stream
    pub fn send_data(&mut self, stream_id: u32, data: Bytes, end_stream: bool) {
        self.outgoing.push(Frame::Data {
            stream_id,
            end_stream,
            data,
        });
        
        if end_stream {
            if let Some(stream) = self.streams.get_mut(&stream_id) {
                stream.half_close_local();
            }
        }
    }

    /// Get pending outgoing frames
    pub fn take_outgoing(&mut self) -> Vec<Frame> {
        std::mem::take(&mut self.outgoing)
    }

    /// Get a stream by ID
    pub fn get_stream(&self, stream_id: u32) -> Option<&Stream> {
        self.streams.get(&stream_id)
    }

    /// Get a mutable stream by ID
    pub fn get_stream_mut(&mut self, stream_id: u32) -> Option<&mut Stream> {
        self.streams.get_mut(&stream_id)
    }

    /// Get all streams ready for processing (headers received, not closed)
    pub fn ready_streams(&self) -> Vec<u32> {
        self.streams.iter()
            .filter(|(_, s)| {
                s.state == StreamState::HalfClosedRemote || 
                s.state == StreamState::Open && !s.request_headers.is_empty()
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get active stream count
    pub fn active_stream_count(&self) -> usize {
        self.streams.values()
            .filter(|s| s.state != StreamState::Closed)
            .count()
    }

    /// Is connection going away
    pub fn is_going_away(&self) -> bool {
        self.going_away
    }
}

impl Default for H2Connection {
    fn default() -> Self {
        Self::new(H2Settings::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_preface() {
        let mut conn = H2Connection::default();
        
        assert!(!conn.is_preface_received());
        assert!(conn.process_preface(H2_PREFACE));
        assert!(conn.is_preface_received());
        
        // Should have settings frame queued
        let outgoing = conn.take_outgoing();
        assert!(!outgoing.is_empty());
    }

    #[test]
    fn test_settings_exchange() {
        let mut conn = H2Connection::default();
        conn.process_preface(H2_PREFACE);
        
        // Process remote settings
        let frame = Frame::Settings {
            ack: false,
            settings: vec![(3, 200)], // Max concurrent streams
        };
        
        conn.process_frame(frame).unwrap();
        assert_eq!(conn.remote_settings.max_concurrent_streams, 200);
        
        // Should have ACK queued
        let outgoing = conn.take_outgoing();
        assert!(outgoing.iter().any(|f| matches!(f, Frame::Settings { ack: true, .. })));
    }
}
