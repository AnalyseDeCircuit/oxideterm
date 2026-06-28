// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{BufRead, Write};

use crate::{RemoteDesktopHelperEvent, RemoteDesktopHelperRequest};

#[derive(Debug, thiserror::Error)]
pub enum RemoteDesktopJsonLineError {
    #[error("remote desktop helper line is empty")]
    EmptyLine,
    #[error("remote desktop helper line read failed: {0}")]
    ReadFailed(#[from] std::io::Error),
    #[error("remote desktop helper JSON failed: {0}")]
    JsonFailed(#[from] serde_json::Error),
}

pub fn encode_request_line(
    request: &RemoteDesktopHelperRequest,
) -> Result<String, RemoteDesktopJsonLineError> {
    encode_line(request)
}

pub fn decode_request_line(
    line: &str,
) -> Result<RemoteDesktopHelperRequest, RemoteDesktopJsonLineError> {
    decode_line(line)
}

pub fn write_request_line(
    writer: &mut impl Write,
    request: &RemoteDesktopHelperRequest,
) -> Result<(), RemoteDesktopJsonLineError> {
    write_line(writer, request)
}

pub fn read_request_line(
    reader: &mut impl BufRead,
) -> Result<Option<RemoteDesktopHelperRequest>, RemoteDesktopJsonLineError> {
    read_line(reader)
}

pub fn encode_event_line(
    event: &RemoteDesktopHelperEvent,
) -> Result<String, RemoteDesktopJsonLineError> {
    encode_line(event)
}

pub fn decode_event_line(
    line: &str,
) -> Result<RemoteDesktopHelperEvent, RemoteDesktopJsonLineError> {
    decode_line(line)
}

pub fn write_event_line(
    writer: &mut impl Write,
    event: &RemoteDesktopHelperEvent,
) -> Result<(), RemoteDesktopJsonLineError> {
    write_line(writer, event)
}

pub fn read_event_line(
    reader: &mut impl BufRead,
) -> Result<Option<RemoteDesktopHelperEvent>, RemoteDesktopJsonLineError> {
    read_line(reader)
}

fn encode_line<T: serde::Serialize>(value: &T) -> Result<String, RemoteDesktopJsonLineError> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    Ok(line)
}

fn decode_line<T: serde::de::DeserializeOwned>(
    line: &str,
) -> Result<T, RemoteDesktopJsonLineError> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed.trim().is_empty() {
        return Err(RemoteDesktopJsonLineError::EmptyLine);
    }
    Ok(serde_json::from_str(trimmed)?)
}

fn write_line<T: serde::Serialize>(
    writer: &mut impl Write,
    value: &T,
) -> Result<(), RemoteDesktopJsonLineError> {
    writer.write_all(encode_line(value)?.as_bytes())?;
    writer.flush()?;
    Ok(())
}

fn read_line<T: serde::de::DeserializeOwned>(
    reader: &mut impl BufRead,
) -> Result<Option<T>, RemoteDesktopJsonLineError> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    decode_line(&line).map(Some)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::{RemoteDesktopProtocol, RemoteDesktopSessionStatus, RemoteDesktopSize};

    use super::*;

    #[test]
    fn request_line_round_trips_and_has_trailing_newline() {
        let request = RemoteDesktopHelperRequest::Resize {
            size: RemoteDesktopSize {
                width: 800,
                height: 600,
            },
        };

        let line = encode_request_line(&request).unwrap();
        let decoded = decode_request_line(&line).unwrap();

        assert!(line.ends_with('\n'));
        assert_eq!(decoded, request);
    }

    #[test]
    fn event_line_reads_one_message_from_buffer() {
        let event = RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connecting,
            message: Some("opening".to_string()),
        };
        let mut bytes = Vec::new();

        write_event_line(&mut bytes, &event).unwrap();
        let decoded = read_event_line(&mut Cursor::new(bytes)).unwrap().unwrap();

        assert_eq!(decoded, event);
    }

    #[test]
    fn empty_lines_are_rejected() {
        let error = decode_request_line("\n").unwrap_err().to_string();

        assert!(error.contains("empty"));
    }

    #[test]
    fn request_line_does_not_require_protocol_specific_state() {
        let request = RemoteDesktopHelperRequest::Connect {
            protocol: RemoteDesktopProtocol::Vnc,
            endpoint: crate::RemoteDesktopEndpoint::for_protocol(
                "127.0.0.1",
                RemoteDesktopProtocol::Vnc,
            ),
            username: None,
            password: None,
            domain: None,
            size: RemoteDesktopSize {
                width: 1024,
                height: 768,
            },
            read_only: true,
        };

        assert!(encode_request_line(&request).unwrap().contains("\"vnc\""));
    }

    #[test]
    fn request_line_uses_camel_case_variant_fields() {
        let request = RemoteDesktopHelperRequest::Connect {
            protocol: RemoteDesktopProtocol::Vnc,
            endpoint: crate::RemoteDesktopEndpoint::for_protocol(
                "127.0.0.1",
                RemoteDesktopProtocol::Vnc,
            ),
            username: None,
            password: None,
            domain: None,
            size: RemoteDesktopSize {
                width: 1024,
                height: 768,
            },
            read_only: true,
        };

        let line = encode_request_line(&request).unwrap();
        let decoded = decode_request_line(&line).unwrap();

        assert!(line.contains("\"readOnly\":true"));
        assert!(!line.contains("read_only"));
        assert_eq!(decoded, request);
    }

    #[test]
    fn event_line_uses_camel_case_variant_fields() {
        let event = RemoteDesktopHelperEvent::Terminated { exit_code: Some(7) };

        let line = encode_event_line(&event).unwrap();
        let decoded = decode_event_line(&line).unwrap();

        assert!(line.contains("\"exitCode\":7"));
        assert!(!line.contains("exit_code"));
        assert_eq!(decoded, event);
    }
}
