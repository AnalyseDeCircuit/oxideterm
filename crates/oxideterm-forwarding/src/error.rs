// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io;

use oxideterm_backend_classification::{BackendErrorClass, classify_io_error_kind};

#[derive(Debug, thiserror::Error)]
pub enum ForwardingError {
    #[error("forward rule not found: {0}")]
    NotFound(String),
    #[error("forward rule already exists: {0}")]
    AlreadyExists(String),
    #[error("forward rule is active and cannot be edited: {0}")]
    ActiveRuleCannotBeEdited(String),
    #[error("forward type is not implemented in native yet: {0}")]
    UnsupportedForwardType(&'static str),
    #[error("invalid forward rule: {0}")]
    InvalidRule(String),
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("SSH forwarding failed: {0}")]
    Ssh(String),
    #[error("I/O forwarding failed: {0}")]
    Io(#[from] std::io::Error),
}

impl From<oxideterm_ssh::SshTransportError> for ForwardingError {
    fn from(error: oxideterm_ssh::SshTransportError) -> Self {
        match error {
            oxideterm_ssh::SshTransportError::ConnectionFailed(message) => {
                Self::ConnectionFailed(message)
            }
            other => Self::Ssh(other.to_string()),
        }
    }
}

pub(crate) fn tauri_local_bind_error(
    bind_address: &str,
    bind_port: u16,
    error: io::Error,
) -> ForwardingError {
    tauri_bind_error("local", bind_address, bind_port, error)
}

pub(crate) fn tauri_dynamic_bind_error(
    bind_address: &str,
    bind_port: u16,
    error: io::Error,
) -> ForwardingError {
    tauri_bind_error("dynamic", bind_address, bind_port, error)
}

fn tauri_bind_error(
    forward_kind: &str,
    bind_address: &str,
    bind_port: u16,
    error: io::Error,
) -> ForwardingError {
    // Tauri surfaces listener setup failures through SshError::ConnectionFailed
    // from the forwarding runner, and the Forwards UI displays that string
    // directly. Keep native bind errors in the same user-visible class instead
    // of leaking raw std::io wording through the forwarding abstraction.
    let local_addr = format!("{bind_address}:{bind_port}");
    let message = match classify_io_error_kind(error.kind()) {
        Some(BackendErrorClass::PortInUse) => {
            format!(
                "Port already in use: {local_addr}. Another application may be using this port."
            )
        }
        Some(BackendErrorClass::PermissionDenied) => {
            format!(
                "Permission denied binding to {local_addr}. Ports below 1024 require elevated privileges."
            )
        }
        _ if error.kind() == io::ErrorKind::AddrNotAvailable => {
            format!(
                "Address not available: {local_addr}. The specified address is not valid on this system."
            )
        }
        _ if forward_kind == "dynamic" => {
            format!("Failed to bind SOCKS5 proxy to {local_addr}: {error}")
        }
        _ => format!("Failed to bind to {local_addr}: {error}"),
    };
    ForwardingError::ConnectionFailed(message)
}
