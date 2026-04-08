// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Wire Protocol codec for OxideTerm WebSocket bridge.
//!
//! Binary format: [Type:1B][Length:4B BE][Payload:NB]
//! Compatible with the backend's `FrameCodec` in `src-tauri/src/bridge/protocol.rs`.

use std::io::{self, Read};

/// Message types matching the backend Wire Protocol v1.
#[repr(u8)]
pub enum MessageType {
    Data = 0x00,
    #[allow(dead_code)]
    Resize = 0x01,
    Heartbeat = 0x02,
    Error = 0x03,
}

/// Maximum payload size (16 MB).
const MAX_PAYLOAD_SIZE: u32 = 16 * 1024 * 1024;

/// Encode a Data frame into the provided buffer.
pub fn encode_data(payload: &[u8], buf: &mut Vec<u8>) {
    buf.push(MessageType::Data as u8);
    buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    buf.extend_from_slice(payload);
}

/// Encode a Heartbeat frame into the provided buffer.
pub fn encode_heartbeat(buf: &mut Vec<u8>) {
    buf.push(MessageType::Heartbeat as u8);
    buf.extend_from_slice(&0u32.to_be_bytes());
}

/// A decoded frame from the wire.
#[derive(Debug)]
pub struct Frame {
    pub msg_type: u8,
    pub payload: Vec<u8>,
}

/// Decode a single frame from a reader containing one complete frame.
///
/// Returns `Err` if the frame is truncated or malformed.
pub fn decode_frame<R: Read>(reader: &mut R) -> io::Result<Frame> {
    let mut header = [0u8; 5];
    reader.read_exact(&mut header)?;

    let msg_type = header[0];
    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]);

    if length > MAX_PAYLOAD_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Payload too large: {length} bytes"),
        ));
    }

    let mut payload = vec![0u8; length as usize];
    if length > 0 {
        reader.read_exact(&mut payload)?;
    }

    Ok(Frame { msg_type, payload })
}

/// Extract Data payload from a frame.
pub fn frame_data(frame: &Frame) -> Option<&[u8]> {
    if frame.msg_type == MessageType::Data as u8 {
        Some(&frame.payload)
    } else {
        None
    }
}

/// Check if a frame is a Heartbeat.
pub fn is_heartbeat(frame: &Frame) -> bool {
    frame.msg_type == MessageType::Heartbeat as u8
}

/// Check if a frame is an Error.
pub fn is_error(frame: &Frame) -> bool {
    frame.msg_type == MessageType::Error as u8
}

#[cfg(test)]
mod tests {
    use super::{decode_frame, encode_data, MessageType, MAX_PAYLOAD_SIZE};
    use std::io::{Cursor, ErrorKind, Write};

    fn encode_to_writer<W: Write>(
        msg_type: u8,
        payload: &[u8],
        writer: &mut W,
    ) -> std::io::Result<()> {
        writer.write_all(&[msg_type])?;
        writer.write_all(&(payload.len() as u32).to_be_bytes())?;
        if !payload.is_empty() {
            writer.write_all(payload)?;
        }
        Ok(())
    }

    #[test]
    fn decodes_round_trip_data_frame() {
        let mut encoded = Vec::new();
        encode_data(b"hello", &mut encoded);

        let frame = decode_frame(&mut Cursor::new(encoded)).unwrap();

        assert_eq!(frame.msg_type, MessageType::Data as u8);
        assert_eq!(frame.payload, b"hello");
    }

    #[test]
    fn rejects_oversized_payload_header() {
        let mut encoded = Vec::new();
        encoded.push(MessageType::Data as u8);
        encoded.extend_from_slice(&(MAX_PAYLOAD_SIZE + 1).to_be_bytes());

        let err = decode_frame(&mut Cursor::new(encoded)).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_truncated_frame() {
        let mut encoded = Vec::new();
        encode_to_writer(MessageType::Data as u8, b"hello", &mut encoded).unwrap();
        encoded.pop();

        let err = decode_frame(&mut Cursor::new(encoded)).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }
}
