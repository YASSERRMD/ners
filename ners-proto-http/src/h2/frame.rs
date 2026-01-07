//! HTTP/2 Frame Types
//!
//! Implements parsing and serialization of HTTP/2 frames.

use bytes::{Buf, BufMut, Bytes, BytesMut};
use thiserror::Error;

/// HTTP/2 connection preface (sent by client)
pub const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// HTTP/2 frame header size (9 bytes)
pub const FRAME_HEADER_SIZE: usize = 9;

/// HTTP/2 error types
#[derive(Debug, Error)]
pub enum H2Error {
    #[error("Frame too short")]
    FrameTooShort,
    #[error("Invalid frame type: {0}")]
    InvalidFrameType(u8),
    #[error("Frame too large: {0} > {1}")]
    FrameTooLarge(usize, usize),
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("Flow control error")]
    FlowControlError,
    #[error("Stream closed")]
    StreamClosed,
}

/// HTTP/2 frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Data = 0x0,
    Headers = 0x1,
    Priority = 0x2,
    RstStream = 0x3,
    Settings = 0x4,
    PushPromise = 0x5,
    Ping = 0x6,
    GoAway = 0x7,
    WindowUpdate = 0x8,
    Continuation = 0x9,
}

impl TryFrom<u8> for FrameType {
    type Error = H2Error;
    
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(FrameType::Data),
            0x1 => Ok(FrameType::Headers),
            0x2 => Ok(FrameType::Priority),
            0x3 => Ok(FrameType::RstStream),
            0x4 => Ok(FrameType::Settings),
            0x5 => Ok(FrameType::PushPromise),
            0x6 => Ok(FrameType::Ping),
            0x7 => Ok(FrameType::GoAway),
            0x8 => Ok(FrameType::WindowUpdate),
            0x9 => Ok(FrameType::Continuation),
            _ => Err(H2Error::InvalidFrameType(value)),
        }
    }
}

/// HTTP/2 frame flags
pub mod flags {
    pub const END_STREAM: u8 = 0x1;
    pub const END_HEADERS: u8 = 0x4;
    pub const PADDED: u8 = 0x8;
    pub const PRIORITY: u8 = 0x20;
    pub const ACK: u8 = 0x1;
}

/// HTTP/2 settings identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SettingsId {
    HeaderTableSize = 0x1,
    EnablePush = 0x2,
    MaxConcurrentStreams = 0x3,
    InitialWindowSize = 0x4,
    MaxFrameSize = 0x5,
    MaxHeaderListSize = 0x6,
}

/// HTTP/2 frame
#[derive(Debug, Clone)]
pub enum Frame {
    /// DATA frame
    Data {
        stream_id: u32,
        end_stream: bool,
        data: Bytes,
    },
    /// HEADERS frame
    Headers {
        stream_id: u32,
        end_stream: bool,
        end_headers: bool,
        header_block: Bytes,
    },
    /// PRIORITY frame
    Priority {
        stream_id: u32,
        exclusive: bool,
        dependency: u32,
        weight: u8,
    },
    /// RST_STREAM frame
    RstStream {
        stream_id: u32,
        error_code: u32,
    },
    /// SETTINGS frame
    Settings {
        ack: bool,
        settings: Vec<(u16, u32)>,
    },
    /// PUSH_PROMISE frame
    PushPromise {
        stream_id: u32,
        promised_stream_id: u32,
        header_block: Bytes,
    },
    /// PING frame
    Ping {
        ack: bool,
        data: [u8; 8],
    },
    /// GOAWAY frame
    GoAway {
        last_stream_id: u32,
        error_code: u32,
        debug_data: Bytes,
    },
    /// WINDOW_UPDATE frame
    WindowUpdate {
        stream_id: u32,
        increment: u32,
    },
    /// CONTINUATION frame
    Continuation {
        stream_id: u32,
        end_headers: bool,
        header_block: Bytes,
    },
}

impl Frame {
    /// Parse a frame from bytes
    pub fn parse(buf: &mut BytesMut) -> Result<Option<Frame>, H2Error> {
        if buf.len() < FRAME_HEADER_SIZE {
            return Ok(None);
        }
        
        // Parse header
        let length = ((buf[0] as usize) << 16) | ((buf[1] as usize) << 8) | (buf[2] as usize);
        let frame_type = FrameType::try_from(buf[3])?;
        let flags = buf[4];
        let stream_id = u32::from_be_bytes([buf[5] & 0x7f, buf[6], buf[7], buf[8]]);
        
        // Check if we have the full frame
        let total_len = FRAME_HEADER_SIZE + length;
        if buf.len() < total_len {
            return Ok(None);
        }
        
        // Consume header
        buf.advance(FRAME_HEADER_SIZE);
        let payload = buf.split_to(length).freeze();
        
        // Parse frame based on type
        let frame = match frame_type {
            FrameType::Data => Frame::Data {
                stream_id,
                end_stream: flags & flags::END_STREAM != 0,
                data: payload,
            },
            FrameType::Headers => Frame::Headers {
                stream_id,
                end_stream: flags & flags::END_STREAM != 0,
                end_headers: flags & flags::END_HEADERS != 0,
                header_block: payload,
            },
            FrameType::Settings => {
                let ack = flags & flags::ACK != 0;
                let mut settings = Vec::new();
                let mut p = payload.as_ref();
                while p.len() >= 6 {
                    let id = u16::from_be_bytes([p[0], p[1]]);
                    let value = u32::from_be_bytes([p[2], p[3], p[4], p[5]]);
                    settings.push((id, value));
                    p = &p[6..];
                }
                Frame::Settings { ack, settings }
            }
            FrameType::WindowUpdate => {
                if payload.len() < 4 {
                    return Err(H2Error::FrameTooShort);
                }
                let increment = u32::from_be_bytes([
                    payload[0] & 0x7f,
                    payload[1],
                    payload[2],
                    payload[3],
                ]);
                Frame::WindowUpdate { stream_id, increment }
            }
            FrameType::Ping => {
                if payload.len() < 8 {
                    return Err(H2Error::FrameTooShort);
                }
                let mut data = [0u8; 8];
                data.copy_from_slice(&payload[..8]);
                Frame::Ping {
                    ack: flags & flags::ACK != 0,
                    data,
                }
            }
            FrameType::GoAway => {
                if payload.len() < 8 {
                    return Err(H2Error::FrameTooShort);
                }
                let last_stream_id = u32::from_be_bytes([
                    payload[0] & 0x7f,
                    payload[1],
                    payload[2],
                    payload[3],
                ]);
                let error_code = u32::from_be_bytes([
                    payload[4],
                    payload[5],
                    payload[6],
                    payload[7],
                ]);
                Frame::GoAway {
                    last_stream_id,
                    error_code,
                    debug_data: payload.slice(8..),
                }
            }
            FrameType::RstStream => {
                if payload.len() < 4 {
                    return Err(H2Error::FrameTooShort);
                }
                let error_code = u32::from_be_bytes([
                    payload[0],
                    payload[1],
                    payload[2],
                    payload[3],
                ]);
                Frame::RstStream { stream_id, error_code }
            }
            FrameType::Priority => {
                if payload.len() < 5 {
                    return Err(H2Error::FrameTooShort);
                }
                let exclusive = payload[0] & 0x80 != 0;
                let dependency = u32::from_be_bytes([
                    payload[0] & 0x7f,
                    payload[1],
                    payload[2],
                    payload[3],
                ]);
                let weight = payload[4];
                Frame::Priority {
                    stream_id,
                    exclusive,
                    dependency,
                    weight,
                }
            }
            FrameType::PushPromise => {
                if payload.len() < 4 {
                    return Err(H2Error::FrameTooShort);
                }
                let promised_stream_id = u32::from_be_bytes([
                    payload[0] & 0x7f,
                    payload[1],
                    payload[2],
                    payload[3],
                ]);
                Frame::PushPromise {
                    stream_id,
                    promised_stream_id,
                    header_block: payload.slice(4..),
                }
            }
            FrameType::Continuation => Frame::Continuation {
                stream_id,
                end_headers: flags & flags::END_HEADERS != 0,
                header_block: payload,
            },
        };
        
        Ok(Some(frame))
    }

    /// Serialize a frame to bytes
    pub fn serialize(&self, buf: &mut BytesMut) {
        let (frame_type, flags, stream_id, payload) = match self {
            Frame::Data { stream_id, end_stream, data } => {
                let flags = if *end_stream { flags::END_STREAM } else { 0 };
                (FrameType::Data, flags, *stream_id, data.clone())
            }
            Frame::Headers { stream_id, end_stream, end_headers, header_block } => {
                let mut flags = 0u8;
                if *end_stream { flags |= flags::END_STREAM; }
                if *end_headers { flags |= flags::END_HEADERS; }
                (FrameType::Headers, flags, *stream_id, header_block.clone())
            }
            Frame::Settings { ack, settings } => {
                let flags = if *ack { flags::ACK } else { 0 };
                let mut payload = BytesMut::new();
                for (id, value) in settings {
                    payload.put_u16(*id);
                    payload.put_u32(*value);
                }
                (FrameType::Settings, flags, 0, payload.freeze())
            }
            Frame::WindowUpdate { stream_id, increment } => {
                let mut payload = BytesMut::with_capacity(4);
                payload.put_u32(*increment & 0x7fffffff);
                (FrameType::WindowUpdate, 0, *stream_id, payload.freeze())
            }
            Frame::Ping { ack, data } => {
                let flags = if *ack { flags::ACK } else { 0 };
                (FrameType::Ping, flags, 0, Bytes::copy_from_slice(data))
            }
            Frame::GoAway { last_stream_id, error_code, debug_data } => {
                let mut payload = BytesMut::new();
                payload.put_u32(*last_stream_id & 0x7fffffff);
                payload.put_u32(*error_code);
                payload.extend_from_slice(debug_data);
                (FrameType::GoAway, 0, 0, payload.freeze())
            }
            Frame::RstStream { stream_id, error_code } => {
                let mut payload = BytesMut::with_capacity(4);
                payload.put_u32(*error_code);
                (FrameType::RstStream, 0, *stream_id, payload.freeze())
            }
            Frame::Priority { stream_id, exclusive, dependency, weight } => {
                let mut payload = BytesMut::with_capacity(5);
                let dep = if *exclusive { *dependency | 0x80000000 } else { *dependency };
                payload.put_u32(dep);
                payload.put_u8(*weight);
                (FrameType::Priority, 0, *stream_id, payload.freeze())
            }
            Frame::PushPromise { stream_id, promised_stream_id, header_block } => {
                let mut payload = BytesMut::new();
                payload.put_u32(*promised_stream_id & 0x7fffffff);
                payload.extend_from_slice(header_block);
                (FrameType::PushPromise, 0, *stream_id, payload.freeze())
            }
            Frame::Continuation { stream_id, end_headers, header_block } => {
                let flags = if *end_headers { flags::END_HEADERS } else { 0 };
                (FrameType::Continuation, flags, *stream_id, header_block.clone())
            }
        };
        
        // Write header
        let len = payload.len();
        buf.put_u8((len >> 16) as u8);
        buf.put_u8((len >> 8) as u8);
        buf.put_u8(len as u8);
        buf.put_u8(frame_type as u8);
        buf.put_u8(flags);
        buf.put_u32(stream_id);
        
        // Write payload
        buf.extend_from_slice(&payload);
    }

    /// Get stream ID for this frame
    pub fn stream_id(&self) -> u32 {
        match self {
            Frame::Data { stream_id, .. } => *stream_id,
            Frame::Headers { stream_id, .. } => *stream_id,
            Frame::Priority { stream_id, .. } => *stream_id,
            Frame::RstStream { stream_id, .. } => *stream_id,
            Frame::Settings { .. } => 0,
            Frame::PushPromise { stream_id, .. } => *stream_id,
            Frame::Ping { .. } => 0,
            Frame::GoAway { .. } => 0,
            Frame::WindowUpdate { stream_id, .. } => *stream_id,
            Frame::Continuation { stream_id, .. } => *stream_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_settings() {
        let mut buf = BytesMut::new();
        // SETTINGS frame: length=6, type=4, flags=0, stream=0
        buf.extend_from_slice(&[0, 0, 6, 4, 0, 0, 0, 0, 0]);
        // Setting: MAX_CONCURRENT_STREAMS = 100
        buf.extend_from_slice(&[0, 3, 0, 0, 0, 100]);
        
        let frame = Frame::parse(&mut buf).unwrap().unwrap();
        match frame {
            Frame::Settings { ack, settings } => {
                assert!(!ack);
                assert_eq!(settings.len(), 1);
                assert_eq!(settings[0], (3, 100));
            }
            _ => panic!("Expected Settings frame"),
        }
    }

    #[test]
    fn test_serialize_window_update() {
        let frame = Frame::WindowUpdate {
            stream_id: 1,
            increment: 65535,
        };
        
        let mut buf = BytesMut::new();
        frame.serialize(&mut buf);
        
        // Header: length=4, type=8, flags=0, stream=1
        assert_eq!(&buf[..5], &[0, 0, 4, 8, 0]);
        assert_eq!(buf[8], 1); // stream_id low byte
    }
}
