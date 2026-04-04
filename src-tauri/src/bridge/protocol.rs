// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Wire Protocol for OxideTerm Data Plane
//!
//! Protocol v1 Frame Format:
//! ```text
//! +--------+--------+--------+--------+--------+-- ... --+
//! | Type   | Length (4 bytes, big-endian)      | Payload |
//! +--------+--------+--------+--------+--------+-- ... --+
//! ```
//!
//! Message Types:
//! - 0x00: Data      - Terminal I/O data
//! - 0x01: Resize    - Window size change (cols: u16, rows: u16)
//! - 0x02: Heartbeat - Keep-alive ping/pong
//! - 0x03: Error     - Error notification

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io::{self, ErrorKind};

/// Protocol version
#[allow(dead_code)]
pub const PROTOCOL_VERSION: u8 = 1;

/// Header size: 1 byte type + 4 bytes length
pub const HEADER_SIZE: usize = 5;

/// Maximum payload size (16 MB)
pub const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;

/// Message types for the wire protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Terminal I/O data
    Data = 0x00,
    /// Window resize request (cols: u16, rows: u16)
    Resize = 0x01,
    /// Keep-alive heartbeat
    Heartbeat = 0x02,
    /// Error message
    Error = 0x03,
}

impl MessageType {
    /// Parse message type from byte
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Data),
            0x01 => Some(Self::Resize),
            0x02 => Some(Self::Heartbeat),
            0x03 => Some(Self::Error),
            _ => None,
        }
    }

    /// Convert to byte
    pub fn as_byte(&self) -> u8 {
        *self as u8
    }
}

/// A parsed protocol frame
#[derive(Debug, Clone)]
pub enum Frame {
    /// Terminal I/O data
    Data(Bytes),
    /// Window resize (cols, rows)
    Resize { cols: u16, rows: u16 },
    /// Heartbeat ping/pong with optional sequence number
    Heartbeat(u32),
    /// Error message
    Error(String),
}

impl Frame {
    /// Encode frame into bytes
    pub fn encode(&self) -> Bytes {
        match self {
            Frame::Data(data) => {
                let mut buf = BytesMut::with_capacity(HEADER_SIZE + data.len());
                buf.put_u8(MessageType::Data.as_byte());
                buf.put_u32(data.len() as u32);
                buf.extend_from_slice(data);
                buf.freeze()
            }
            Frame::Resize { cols, rows } => {
                let mut buf = BytesMut::with_capacity(HEADER_SIZE + 4);
                buf.put_u8(MessageType::Resize.as_byte());
                buf.put_u32(4); // 2 bytes cols + 2 bytes rows
                buf.put_u16(*cols);
                buf.put_u16(*rows);
                buf.freeze()
            }
            Frame::Heartbeat(seq) => {
                let mut buf = BytesMut::with_capacity(HEADER_SIZE + 4);
                buf.put_u8(MessageType::Heartbeat.as_byte());
                buf.put_u32(4); // 4 bytes sequence number
                buf.put_u32(*seq);
                buf.freeze()
            }
            Frame::Error(msg) => {
                let msg_bytes = msg.as_bytes();
                let mut buf = BytesMut::with_capacity(HEADER_SIZE + msg_bytes.len());
                buf.put_u8(MessageType::Error.as_byte());
                buf.put_u32(msg_bytes.len() as u32);
                buf.extend_from_slice(msg_bytes);
                buf.freeze()
            }
        }
    }

    /// Try to decode a frame from bytes
    /// Returns None if not enough data, Err if invalid
    pub fn decode(buf: &mut BytesMut) -> io::Result<Option<Self>> {
        // Check if we have enough for the header
        if buf.len() < HEADER_SIZE {
            return Ok(None);
        }

        // Peek at header without consuming
        let msg_type = buf[0];
        let length = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;

        // Validate length
        if length > MAX_PAYLOAD_SIZE {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Payload too large: {} bytes", length),
            ));
        }

        // Check if we have the full frame
        if buf.len() < HEADER_SIZE + length {
            return Ok(None);
        }

        // Consume header
        buf.advance(HEADER_SIZE);

        // Parse payload based on type
        let msg_type = MessageType::from_byte(msg_type).ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("Unknown message type: {}", msg_type),
            )
        })?;

        let frame = match msg_type {
            MessageType::Data => {
                let data = buf.split_to(length).freeze();
                Frame::Data(data)
            }
            MessageType::Resize => {
                if length != 4 {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        "Resize frame must have 4 bytes payload",
                    ));
                }
                let cols = buf.get_u16();
                let rows = buf.get_u16();
                Frame::Resize { cols, rows }
            }
            MessageType::Heartbeat => {
                if length != 4 {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        "Heartbeat frame must have 4 bytes payload",
                    ));
                }
                let seq = buf.get_u32();
                Frame::Heartbeat(seq)
            }
            MessageType::Error => {
                let data = buf.split_to(length);
                let msg = String::from_utf8_lossy(&data).to_string();
                Frame::Error(msg)
            }
        };

        Ok(Some(frame))
    }
}

/// Frame encoder/decoder for streaming
pub struct FrameCodec {
    buffer: BytesMut,
}

impl FrameCodec {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(8192),
        }
    }

    /// Feed raw bytes into the codec
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Try to decode the next frame
    pub fn decode_next(&mut self) -> io::Result<Option<Frame>> {
        Frame::decode(&mut self.buffer)
    }

    /// Clear internal buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Check if buffer is too large (possible malformed data)
    pub fn is_overflow(&self) -> bool {
        self.buffer.len() > MAX_PAYLOAD_SIZE
    }

    /// Get buffer length
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for FrameCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create Data frame from raw bytes
pub fn data_frame(data: impl Into<Bytes>) -> Frame {
    Frame::Data(data.into())
}

/// Helper to create Resize frame
pub fn resize_frame(cols: u16, rows: u16) -> Frame {
    Frame::Resize { cols, rows }
}

/// Helper to create Heartbeat frame
pub fn heartbeat_frame(seq: u32) -> Frame {
    Frame::Heartbeat(seq)
}

/// Helper to create Error frame
pub fn error_frame(msg: impl Into<String>) -> Frame {
    Frame::Error(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_frame_roundtrip() {
        let original = data_frame(Bytes::from_static(b"hello world"));
        let encoded = original.encode();

        let mut buf = BytesMut::from(&encoded[..]);
        let decoded = Frame::decode(&mut buf).unwrap().unwrap();

        match decoded {
            Frame::Data(data) => assert_eq!(data, &b"hello world"[..]),
            _ => panic!("Expected Data frame"),
        }
    }

    #[test]
    fn test_resize_frame_roundtrip() {
        let original = resize_frame(120, 40);
        let encoded = original.encode();

        let mut buf = BytesMut::from(&encoded[..]);
        let decoded = Frame::decode(&mut buf).unwrap().unwrap();

        match decoded {
            Frame::Resize { cols, rows } => {
                assert_eq!(cols, 120);
                assert_eq!(rows, 40);
            }
            _ => panic!("Expected Resize frame"),
        }
    }

    #[test]
    fn test_heartbeat_frame_roundtrip() {
        let original = heartbeat_frame(42);
        let encoded = original.encode();

        let mut buf = BytesMut::from(&encoded[..]);
        let decoded = Frame::decode(&mut buf).unwrap().unwrap();

        match decoded {
            Frame::Heartbeat(seq) => assert_eq!(seq, 42),
            _ => panic!("Expected Heartbeat frame"),
        }
    }

    #[test]
    fn test_error_frame_roundtrip() {
        let original = error_frame("Something went wrong");
        let encoded = original.encode();

        let mut buf = BytesMut::from(&encoded[..]);
        let decoded = Frame::decode(&mut buf).unwrap().unwrap();

        match decoded {
            Frame::Error(msg) => assert_eq!(msg, "Something went wrong"),
            _ => panic!("Expected Error frame"),
        }
    }

    #[test]
    fn test_partial_frame() {
        let frame = data_frame(Bytes::from_static(b"hello"));
        let encoded = frame.encode();

        // Only provide partial data
        let mut buf = BytesMut::from(&encoded[..3]);
        assert!(Frame::decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn test_codec_multiple_frames() {
        let mut codec = FrameCodec::new();

        let f1 = data_frame(Bytes::from_static(b"first"));
        let f2 = resize_frame(80, 24);
        let f3 = heartbeat_frame(1);

        // Feed all frames at once
        codec.feed(&f1.encode());
        codec.feed(&f2.encode());
        codec.feed(&f3.encode());

        // Decode them one by one
        match codec.decode_next().unwrap().unwrap() {
            Frame::Data(d) => assert_eq!(d, &b"first"[..]),
            _ => panic!(),
        }
        match codec.decode_next().unwrap().unwrap() {
            Frame::Resize { cols, rows } => {
                assert_eq!(cols, 80);
                assert_eq!(rows, 24);
            }
            _ => panic!(),
        }
        match codec.decode_next().unwrap().unwrap() {
            Frame::Heartbeat(seq) => assert_eq!(seq, 1),
            _ => panic!(),
        }

        // No more frames
        assert!(codec.decode_next().unwrap().is_none());
    }
}
