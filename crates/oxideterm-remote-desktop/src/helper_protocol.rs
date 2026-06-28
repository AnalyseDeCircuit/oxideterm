// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    RemoteDesktopEndpoint, RemoteDesktopFrame, RemoteDesktopProtocol, RemoteDesktopSecret,
    RemoteDesktopSessionStatus, RemoteDesktopSize,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RemoteDesktopHelperRequest {
    Connect {
        protocol: RemoteDesktopProtocol,
        endpoint: RemoteDesktopEndpoint,
        username: Option<String>,
        password: Option<RemoteDesktopSecret>,
        domain: Option<String>,
        size: RemoteDesktopSize,
        read_only: bool,
    },
    Resize {
        size: RemoteDesktopSize,
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
                read_only,
            } => formatter
                .debug_struct("Connect")
                .field("protocol", protocol)
                .field("endpoint", endpoint)
                .field("username", &username.as_ref().map(|_| "<present>"))
                .field("password", &password.as_ref().map(|_| "[redacted secret]"))
                .field("domain", &domain.as_ref().map(|_| "<present>"))
                .field("size", size)
                .field("read_only", read_only)
                .finish(),
            Self::Resize { size } => formatter.debug_struct("Resize").field("size", size).finish(),
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
            Self::Wheel { delta } => formatter.debug_struct("Wheel").field("delta", delta).finish(),
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
            Self::Close => formatter.write_str("Close"),
            Self::Reconnect => formatter.write_str("Reconnect"),
        }
    }
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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
    Cursor {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    ClipboardText {
        text: String,
    },
    ConnectionFailure {
        message: String,
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
            Self::ClipboardText { text } => formatter
                .debug_struct("ClipboardText")
                .field("text", &format_args!("<redacted:{}>", text.chars().count()))
                .finish(),
            Self::ConnectionFailure { message } => formatter
                .debug_struct("ConnectionFailure")
                .field("message", message)
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
            read_only: false,
        };

        let debug = format!("{request:?}");

        assert!(debug.contains("redacted"));
        assert!(!debug.contains("super-secret"));
        assert!(!debug.contains("admin"));
        assert!(!debug.contains("corp"));
    }

    #[test]
    fn helper_protocol_round_trips_json() {
        let request = RemoteDesktopHelperRequest::Resize {
            size: RemoteDesktopSize {
                width: 1024,
                height: 768,
            },
        };

        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: RemoteDesktopHelperRequest = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, request);
    }
}

