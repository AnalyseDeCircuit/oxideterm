// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    RemoteDesktopCursorShape, RemoteDesktopEndpoint, RemoteDesktopFrame, RemoteDesktopFrameUpdate,
    RemoteDesktopProtocol, RemoteDesktopSecret, RemoteDesktopSessionStatus, RemoteDesktopSize,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopMouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopMouseButtonState {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopKeyState {
    Pressed,
    Released,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopKey {
    pub code: String,
    pub text: Option<String>,
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopWheelDelta {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopLockKeys {
    pub scroll_lock: bool,
    pub num_lock: bool,
    pub caps_lock: bool,
    pub kana_lock: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopClipboardFormat {
    ImagePng,
    ImageJpeg,
    ImageWebp,
    ImageGif,
    ImageSvg,
    ImageBmp,
    ImageTiff,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopClipboardData {
    pub format: RemoteDesktopClipboardFormat,
    pub bytes: Vec<u8>,
}

impl RemoteDesktopClipboardData {
    pub fn new(format: RemoteDesktopClipboardFormat, bytes: Vec<u8>) -> Self {
        Self { format, bytes }
    }
}

impl fmt::Debug for RemoteDesktopClipboardData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteDesktopClipboardData")
            .field("format", &self.format)
            .field("bytes", &format_args!("<{} bytes>", self.bytes.len()))
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteDesktopErrorCategory {
    Configuration,
    Network,
    Authentication,
    Protocol,
    LegacySecurity,
    Dependency,
    Unknown,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RemoteDesktopHelperRequest {
    Connect {
        protocol: RemoteDesktopProtocol,
        endpoint: RemoteDesktopEndpoint,
        username: Option<String>,
        password: Option<RemoteDesktopSecret>,
        domain: Option<String>,
        size: RemoteDesktopSize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale_factor: Option<u32>,
        read_only: bool,
    },
    Resize {
        size: RemoteDesktopSize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale_factor: Option<u32>,
    },
    MouseMove {
        x: u32,
        y: u32,
    },
    MouseButton {
        button: RemoteDesktopMouseButton,
        state: RemoteDesktopMouseButtonState,
    },
    Wheel {
        delta: RemoteDesktopWheelDelta,
    },
    Key {
        key: RemoteDesktopKey,
        state: RemoteDesktopKeyState,
    },
    Text {
        text: String,
    },
    ClipboardText {
        text: String,
    },
    ClipboardData {
        data: RemoteDesktopClipboardData,
    },
    SynchronizeLockKeys {
        keys: RemoteDesktopLockKeys,
    },
    ReleaseAllInputs,
    Close,
    Reconnect,
}

impl fmt::Debug for RemoteDesktopHelperRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect {
                protocol,
                endpoint,
                username,
                password,
                domain,
                size,
                scale_factor,
                read_only,
            } => formatter
                .debug_struct("Connect")
                .field("protocol", protocol)
                .field("endpoint", endpoint)
                .field("username", &username.as_ref().map(|_| "<present>"))
                .field("password", &password.as_ref().map(|_| "[redacted secret]"))
                .field("domain", &domain.as_ref().map(|_| "<present>"))
                .field("size", size)
                .field("scale_factor", scale_factor)
                .field("read_only", read_only)
                .finish(),
            Self::Resize { size, scale_factor } => formatter
                .debug_struct("Resize")
                .field("size", size)
                .field("scale_factor", scale_factor)
                .finish(),
            Self::MouseMove { x, y } => formatter
                .debug_struct("MouseMove")
                .field("x", x)
                .field("y", y)
                .finish(),
            Self::MouseButton { button, state } => formatter
                .debug_struct("MouseButton")
                .field("button", button)
                .field("state", state)
                .finish(),
            Self::Wheel { delta } => formatter
                .debug_struct("Wheel")
                .field("delta", delta)
                .finish(),
            Self::Key { key, state } => formatter
                .debug_struct("Key")
                .field("key", key)
                .field("state", state)
                .finish(),
            Self::Text { text } => formatter
                .debug_struct("Text")
                .field("text", &format_args!("<redacted:{}>", text.chars().count()))
                .finish(),
            Self::ClipboardText { text } => formatter
                .debug_struct("ClipboardText")
                .field("text", &format_args!("<redacted:{}>", text.chars().count()))
                .finish(),
            Self::ClipboardData { data } => formatter
                .debug_struct("ClipboardData")
                .field("format", &data.format)
                .field("bytes", &format_args!("<{} bytes>", data.bytes.len()))
                .finish(),
            Self::SynchronizeLockKeys { keys } => formatter
                .debug_struct("SynchronizeLockKeys")
                .field("keys", keys)
                .finish(),
            Self::ReleaseAllInputs => formatter.write_str("ReleaseAllInputs"),
            Self::Close => formatter.write_str("Close"),
            Self::Reconnect => formatter.write_str("Reconnect"),
        }
    }
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RemoteDesktopHelperEvent {
    Status {
        status: RemoteDesktopSessionStatus,
        message: Option<String>,
    },
    Connected {
        size: RemoteDesktopSize,
    },
    Frame {
        frame: RemoteDesktopFrame,
    },
    FrameUpdate {
        update: RemoteDesktopFrameUpdate,
    },
    Cursor {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    CursorShape {
        shape: RemoteDesktopCursorShape,
    },
    CursorDefault,
    CursorHidden,
    ClipboardText {
        text: String,
    },
    ClipboardData {
        data: RemoteDesktopClipboardData,
    },
    ConnectionFailure {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        category: Option<RemoteDesktopErrorCategory>,
    },
    Disconnected {
        reason: Option<String>,
    },
    Terminated {
        exit_code: Option<i32>,
    },
}

impl fmt::Debug for RemoteDesktopHelperEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Status { status, message } => formatter
                .debug_struct("Status")
                .field("status", status)
                .field("message", message)
                .finish(),
            Self::Connected { size } => formatter
                .debug_struct("Connected")
                .field("size", size)
                .finish(),
            Self::Frame { frame } => formatter
                .debug_struct("Frame")
                .field("size", &frame.size)
                .field("format", &frame.format)
                .field("bytes", &format_args!("<{} bytes>", frame.bytes.len()))
                .finish(),
            Self::FrameUpdate { update } => formatter
                .debug_struct("FrameUpdate")
                .field("size", &update.size)
                .field("rect", &update.rect)
                .field("format", &update.format)
                .field("compression", &update.compression)
                .field("bytes", &format_args!("<{} bytes>", update.bytes.len()))
                .finish(),
            Self::Cursor {
                x,
                y,
                width,
                height,
            } => formatter
                .debug_struct("Cursor")
                .field("x", x)
                .field("y", y)
                .field("width", width)
                .field("height", height)
                .finish(),
            Self::CursorShape { shape } => formatter
                .debug_struct("CursorShape")
                .field("size", &shape.size)
                .field("hotspot_x", &shape.hotspot_x)
                .field("hotspot_y", &shape.hotspot_y)
                .field("format", &shape.format)
                .field("bytes", &format_args!("<{} bytes>", shape.bytes.len()))
                .finish(),
            Self::CursorDefault => formatter.write_str("CursorDefault"),
            Self::CursorHidden => formatter.write_str("CursorHidden"),
            Self::ClipboardText { text } => formatter
                .debug_struct("ClipboardText")
                .field("text", &format_args!("<redacted:{}>", text.chars().count()))
                .finish(),
            Self::ClipboardData { data } => formatter
                .debug_struct("ClipboardData")
                .field("format", &data.format)
                .field("bytes", &format_args!("<{} bytes>", data.bytes.len()))
                .finish(),
            Self::ConnectionFailure { message, category } => formatter
                .debug_struct("ConnectionFailure")
                .field("message", message)
                .field("category", category)
                .finish(),
            Self::Disconnected { reason } => formatter
                .debug_struct("Disconnected")
                .field("reason", reason)
                .finish(),
            Self::Terminated { exit_code } => formatter
                .debug_struct("Terminated")
                .field("exit_code", exit_code)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_debug_redacts_secret_values() {
        let request = RemoteDesktopHelperRequest::Connect {
            protocol: RemoteDesktopProtocol::Rdp,
            endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
            username: Some("admin".to_string()),
            password: Some(RemoteDesktopSecret::from("super-secret")),
            domain: Some("corp".to_string()),
            size: RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
            scale_factor: Some(125),
            read_only: false,
        };

        let debug = format!("{request:?}");

        assert!(debug.contains("redacted"));
        assert!(!debug.contains("super-secret"));
        assert!(!debug.contains("admin"));
        assert!(!debug.contains("corp"));
    }

    #[test]
    fn connect_request_accepts_missing_scale_factor() {
        let decoded: RemoteDesktopHelperRequest = serde_json::from_str(
            r#"{"type":"connect","protocol":"rdp","endpoint":{"host":"example.test","port":3389},"username":null,"password":null,"domain":null,"size":{"width":1280,"height":720},"readOnly":false}"#,
        )
        .unwrap();

        assert_eq!(
            decoded,
            RemoteDesktopHelperRequest::Connect {
                protocol: RemoteDesktopProtocol::Rdp,
                endpoint: RemoteDesktopEndpoint::new("example.test", 3389),
                username: None,
                password: None,
                domain: None,
                size: RemoteDesktopSize {
                    width: 1280,
                    height: 720,
                },
                scale_factor: None,
                read_only: false,
            }
        );
    }

    #[test]
    fn helper_protocol_round_trips_json() {
        let request = RemoteDesktopHelperRequest::Resize {
            size: RemoteDesktopSize {
                width: 1024,
                height: 768,
            },
            scale_factor: Some(125),
        };

        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: RemoteDesktopHelperRequest = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, request);
    }

    #[test]
    fn resize_request_accepts_missing_scale_factor() {
        let decoded: RemoteDesktopHelperRequest =
            serde_json::from_str(r#"{"type":"resize","size":{"width":1024,"height":768}}"#)
                .unwrap();

        assert_eq!(
            decoded,
            RemoteDesktopHelperRequest::Resize {
                size: RemoteDesktopSize {
                    width: 1024,
                    height: 768,
                },
                scale_factor: None,
            }
        );
    }

    #[test]
    fn release_all_inputs_round_trips_json() {
        let request = RemoteDesktopHelperRequest::ReleaseAllInputs;

        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: RemoteDesktopHelperRequest = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, request);
    }

    #[test]
    fn synchronize_lock_keys_round_trips_json() {
        let request = RemoteDesktopHelperRequest::SynchronizeLockKeys {
            keys: RemoteDesktopLockKeys {
                scroll_lock: true,
                num_lock: false,
                caps_lock: true,
                kana_lock: false,
            },
        };

        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: RemoteDesktopHelperRequest = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, request);
    }

    #[test]
    fn connection_failure_category_round_trips_json() {
        let event = RemoteDesktopHelperEvent::ConnectionFailure {
            message: "legacy security".to_string(),
            category: Some(RemoteDesktopErrorCategory::LegacySecurity),
        };

        let encoded = serde_json::to_string(&event).unwrap();
        let decoded: RemoteDesktopHelperEvent = serde_json::from_str(&encoded).unwrap();

        assert!(encoded.contains("\"category\":\"legacy-security\""));
        assert_eq!(decoded, event);
    }

    #[test]
    fn connection_failure_accepts_missing_category() {
        let decoded: RemoteDesktopHelperEvent =
            serde_json::from_str(r#"{"type":"connectionFailure","message":"old helper failure"}"#)
                .unwrap();

        assert_eq!(
            decoded,
            RemoteDesktopHelperEvent::ConnectionFailure {
                message: "old helper failure".to_string(),
                category: None,
            }
        );
    }
}
