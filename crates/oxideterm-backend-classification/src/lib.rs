// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared backend error classes for SSH-adjacent user surfaces.
//!
//! Tauri routes SFTP, forwarding, IDE FS, and trace-toast failures through a
//! small set of user-visible buckets. Keep the classification here so native
//! adapters do not each grow their own substring table and drift apart.

use std::io;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendErrorClass {
    Auth,
    Cancelled,
    Conflict,
    Disconnected,
    HostKey,
    NotFound,
    PermissionDenied,
    PortInUse,
    Timeout,
    Unsupported,
    Other,
}

pub fn classify_io_error_kind(kind: io::ErrorKind) -> Option<BackendErrorClass> {
    match kind {
        io::ErrorKind::PermissionDenied => Some(BackendErrorClass::PermissionDenied),
        io::ErrorKind::NotFound => Some(BackendErrorClass::NotFound),
        io::ErrorKind::TimedOut => Some(BackendErrorClass::Timeout),
        io::ErrorKind::AddrInUse => Some(BackendErrorClass::PortInUse),
        io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::UnexpectedEof
        | io::ErrorKind::NotConnected => Some(BackendErrorClass::Disconnected),
        _ => None,
    }
}

pub fn classify_message(message: &str) -> BackendErrorClass {
    let normalized = message.to_ascii_lowercase();
    if contains_any(
        &normalized,
        &[
            "cancelled",
            "canceled",
            "user_cancelled",
            "manual disconnect",
            "explicit disconnect",
        ],
    ) {
        BackendErrorClass::Cancelled
    } else if contains_any(
        &normalized,
        &[
            "permission denied",
            "eacces",
            "operation not permitted",
            "access denied",
        ],
    ) {
        BackendErrorClass::PermissionDenied
    } else if contains_any(
        &normalized,
        &[
            "session not found",
            "not initialized",
            "no active ssh connection",
            "transport is closed",
            "transport is missing",
            "link_down",
            "link down",
        ],
    ) {
        BackendErrorClass::Disconnected
    } else if contains_any(
        &normalized,
        &["not found", "no such file", "enoent", "does not exist"],
    ) {
        BackendErrorClass::NotFound
    } else if contains_any(&normalized, &["timeout", "timed out"]) {
        BackendErrorClass::Timeout
    } else if contains_any(
        &normalized,
        &[
            "host key",
            "known_hosts",
            "fingerprint",
            "server key changed",
        ],
    ) {
        BackendErrorClass::HostKey
    } else if contains_any(
        &normalized,
        &[
            "authentication failed",
            "auth failed",
            "permission denied (publickey",
            "too many authentication failures",
        ],
    ) {
        BackendErrorClass::Auth
    } else if contains_any(
        &normalized,
        &[
            "address already in use",
            "addrinuse",
            "port already in use",
            "port is already in use",
        ],
    ) {
        BackendErrorClass::PortInUse
    } else if contains_any(
        &normalized,
        &[
            "network",
            "connection",
            "disconnected",
            "eof",
            "broken pipe",
            "reset by peer",
            "channel closed",
            "transport is closed",
            "transport is missing",
            "stale",
            "not connected",
        ],
    ) {
        BackendErrorClass::Disconnected
    } else if contains_any(
        &normalized,
        &["unsupported", "not supported", "unavailable"],
    ) {
        BackendErrorClass::Unsupported
    } else {
        BackendErrorClass::Other
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_shared_backend_messages() {
        assert_eq!(
            classify_message("Permission denied: /srv/app"),
            BackendErrorClass::PermissionDenied
        );
        assert_eq!(
            classify_message("Agent channel closed"),
            BackendErrorClass::Disconnected
        );
        assert_eq!(
            classify_message("Port already in use: 127.0.0.1:8080"),
            BackendErrorClass::PortInUse
        );
        assert_eq!(
            classify_message("USER_CANCELLED"),
            BackendErrorClass::Cancelled
        );
        assert_eq!(
            classify_io_error_kind(io::ErrorKind::TimedOut),
            Some(BackendErrorClass::Timeout)
        );
    }
}
