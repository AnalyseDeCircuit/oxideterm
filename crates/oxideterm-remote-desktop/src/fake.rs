// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{BufRead, Write};

use crate::{
    RemoteDesktopEndpoint, RemoteDesktopErrorCategory, RemoteDesktopFrame,
    RemoteDesktopFrameFormat, RemoteDesktopHelperEvent, RemoteDesktopHelperRequest,
    RemoteDesktopJsonLineError, RemoteDesktopProtocol, RemoteDesktopSessionStatus,
    RemoteDesktopSize, read_request_line, write_event_line,
};

#[derive(Clone, Debug)]
pub struct RemoteDesktopFakeBackend {
    protocol: RemoteDesktopProtocol,
    endpoint: Option<RemoteDesktopEndpoint>,
    size: RemoteDesktopSize,
    status: RemoteDesktopSessionStatus,
    read_only: bool,
}

impl Default for RemoteDesktopFakeBackend {
    fn default() -> Self {
        Self::new(RemoteDesktopProtocol::Rdp)
    }
}

impl RemoteDesktopFakeBackend {
    pub fn new(protocol: RemoteDesktopProtocol) -> Self {
        Self {
            protocol,
            endpoint: None,
            size: RemoteDesktopSize::clamped(1024, 768),
            status: RemoteDesktopSessionStatus::Idle,
            read_only: false,
        }
    }

    pub fn status(&self) -> RemoteDesktopSessionStatus {
        self.status
    }

    pub fn handle_request(
        &mut self,
        request: RemoteDesktopHelperRequest,
    ) -> Vec<RemoteDesktopHelperEvent> {
        match request {
            RemoteDesktopHelperRequest::Connect {
                protocol,
                endpoint,
                username: _username,
                password: _password,
                domain: _domain,
                size,
                scale_factor: _scale_factor,
                read_only,
            } => {
                self.protocol = protocol;
                self.endpoint = Some(endpoint);
                self.size = RemoteDesktopSize::clamped(size.width, size.height);
                self.read_only = read_only;
                self.status = RemoteDesktopSessionStatus::Connected;
                vec![
                    RemoteDesktopHelperEvent::Status {
                        status: RemoteDesktopSessionStatus::Connecting,
                        message: Some("Fake remote desktop helper is opening.".to_string()),
                    },
                    RemoteDesktopHelperEvent::Connected { size: self.size },
                    RemoteDesktopHelperEvent::Frame {
                        frame: self.synthetic_frame(),
                    },
                ]
            }
            RemoteDesktopHelperRequest::Resize { size, .. } => {
                self.size = RemoteDesktopSize::clamped(size.width, size.height);
                vec![
                    RemoteDesktopHelperEvent::Connected { size: self.size },
                    RemoteDesktopHelperEvent::Frame {
                        frame: self.synthetic_frame(),
                    },
                ]
            }
            RemoteDesktopHelperRequest::Reconnect => {
                if self.endpoint.is_none() {
                    self.status = RemoteDesktopSessionStatus::Failed;
                    return vec![RemoteDesktopHelperEvent::ConnectionFailure {
                        message: "No previous fake remote desktop endpoint exists.".to_string(),
                        category: Some(RemoteDesktopErrorCategory::Configuration),
                    }];
                }

                self.status = RemoteDesktopSessionStatus::Connected;
                vec![
                    RemoteDesktopHelperEvent::Status {
                        status: RemoteDesktopSessionStatus::Reconnecting,
                        message: Some("Fake remote desktop helper is reconnecting.".to_string()),
                    },
                    RemoteDesktopHelperEvent::Connected { size: self.size },
                    RemoteDesktopHelperEvent::Frame {
                        frame: self.synthetic_frame(),
                    },
                ]
            }
            RemoteDesktopHelperRequest::Close => {
                self.status = RemoteDesktopSessionStatus::Disconnected;
                self.endpoint = None;
                vec![RemoteDesktopHelperEvent::Disconnected {
                    reason: Some("Fake remote desktop helper closed.".to_string()),
                }]
            }
            RemoteDesktopHelperRequest::MouseMove { .. }
            | RemoteDesktopHelperRequest::MouseButton { .. }
            | RemoteDesktopHelperRequest::Wheel { .. }
            | RemoteDesktopHelperRequest::Key { .. }
            | RemoteDesktopHelperRequest::Text { .. }
            | RemoteDesktopHelperRequest::ClipboardText { .. }
            | RemoteDesktopHelperRequest::ClipboardData { .. }
            | RemoteDesktopHelperRequest::SynchronizeLockKeys { .. }
            | RemoteDesktopHelperRequest::ReleaseAllInputs => Vec::new(),
        }
    }

    fn synthetic_frame(&self) -> RemoteDesktopFrame {
        let expected = RemoteDesktopFrame::expected_len(self.size).unwrap_or_default();
        let mut bytes = Vec::with_capacity(expected);
        let protocol_bias = match self.protocol {
            RemoteDesktopProtocol::Rdp => 0x30,
            RemoteDesktopProtocol::Vnc => 0x80,
        };

        // The fake frame is deterministic so tests can validate lifecycle code
        // without needing a real RDP/VNC server or image decoder.
        for index in 0..(expected / 4) {
            let stripe = ((index as u32 / self.size.width.max(1)) % 255) as u8;
            bytes.extend_from_slice(&[protocol_bias, stripe, 255_u8.saturating_sub(stripe), 255]);
        }

        RemoteDesktopFrame::new(self.size, RemoteDesktopFrameFormat::Rgba8, bytes)
    }
}

pub fn run_fake_backend_stdio(
    backend: &mut RemoteDesktopFakeBackend,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<usize, RemoteDesktopJsonLineError> {
    let mut handled = 0;
    while let Some(request) = read_request_line(reader)? {
        handled += 1;
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        for event in backend.handle_request(request) {
            write_event_line(writer, &event)?;
        }
        if should_close {
            break;
        }
    }

    Ok(handled)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use crate::{
        RemoteDesktopEndpoint, RemoteDesktopHelperRequest, RemoteDesktopProtocol,
        RemoteDesktopSessionStatus, RemoteDesktopSize, encode_request_line, read_event_line,
    };

    use super::*;

    #[test]
    fn connect_emits_status_connected_and_frame() {
        let mut backend = RemoteDesktopFakeBackend::new(RemoteDesktopProtocol::Rdp);

        let events = backend.handle_request(RemoteDesktopHelperRequest::Connect {
            protocol: RemoteDesktopProtocol::Rdp,
            endpoint: RemoteDesktopEndpoint::for_protocol("127.0.0.1", RemoteDesktopProtocol::Rdp),
            username: Some("tester".to_string()),
            password: Some(crate::RemoteDesktopSecret::from("secret")),
            domain: None,
            size: RemoteDesktopSize {
                width: 640,
                height: 480,
            },
            scale_factor: None,
            read_only: false,
        });

        assert_eq!(backend.status(), RemoteDesktopSessionStatus::Connected);
        assert!(matches!(
            events.first(),
            Some(RemoteDesktopHelperEvent::Status { .. })
        ));
        assert!(matches!(
            events.get(1),
            Some(RemoteDesktopHelperEvent::Connected { .. })
        ));
        assert!(
            matches!(events.get(2), Some(RemoteDesktopHelperEvent::Frame { frame }) if frame.is_complete())
        );
    }

    #[test]
    fn reconnect_without_endpoint_fails() {
        let mut backend = RemoteDesktopFakeBackend::new(RemoteDesktopProtocol::Vnc);

        let events = backend.handle_request(RemoteDesktopHelperRequest::Reconnect);

        assert_eq!(backend.status(), RemoteDesktopSessionStatus::Failed);
        assert!(matches!(
            events.as_slice(),
            [RemoteDesktopHelperEvent::ConnectionFailure { .. }]
        ));
    }

    #[test]
    fn close_clears_session() {
        let mut backend = RemoteDesktopFakeBackend::default();

        let events = backend.handle_request(RemoteDesktopHelperRequest::Close);

        assert_eq!(backend.status(), RemoteDesktopSessionStatus::Disconnected);
        assert!(matches!(
            events.as_slice(),
            [RemoteDesktopHelperEvent::Disconnected { .. }]
        ));
    }

    #[test]
    fn stdio_runner_handles_json_line_requests_until_close() {
        let mut input = Vec::new();
        input
            .write_all(
                encode_request_line(&RemoteDesktopHelperRequest::Resize {
                    size: RemoteDesktopSize {
                        width: 320,
                        height: 240,
                    },
                    scale_factor: None,
                })
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        input
            .write_all(
                encode_request_line(&RemoteDesktopHelperRequest::Close)
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();

        let mut backend = RemoteDesktopFakeBackend::default();
        let mut output = Vec::new();
        let handled =
            run_fake_backend_stdio(&mut backend, &mut Cursor::new(input), &mut output).unwrap();
        let mut decoded = Vec::new();
        let mut output_reader = Cursor::new(output);
        while let Some(event) = read_event_line(&mut output_reader).unwrap() {
            decoded.push(event);
        }

        assert_eq!(handled, 2);
        assert!(matches!(
            decoded.last(),
            Some(RemoteDesktopHelperEvent::Disconnected { .. })
        ));
    }
}
