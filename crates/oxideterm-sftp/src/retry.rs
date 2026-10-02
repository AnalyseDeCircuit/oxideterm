// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use super::SftpError;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_backoff_secs: u64,
    pub backoff_multiplier: f64,
    pub max_backoff_secs: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_secs: 1,
            backoff_multiplier: 2.0,
            max_backoff_secs: 30,
        }
    }
}

pub fn calculate_backoff(attempt: usize, config: &RetryConfig) -> Duration {
    let delay_secs = (config.initial_backoff_secs as f64
        * config.backoff_multiplier.powi(attempt as i32))
    .min(config.max_backoff_secs as f64);
    Duration::from_secs(delay_secs as u64)
}

pub fn is_retryable_error(error: &SftpError) -> bool {
    match error {
        SftpError::IoError(_) | SftpError::ChannelError(_) | SftpError::TransferError(_) => true,
        SftpError::ProtocolError(message) => {
            message.contains("timeout") || message.contains("connection")
        }
        _ => false,
    }
}

/// Classifies transport ownership failures reported across string-only adapters.
pub fn error_is_connection_unavailable(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "stale",
        "link_down",
        "link down",
        "disconnected",
        "transport is closed",
        "transport is missing",
        "ssh connection is closed",
        "connection closed",
        "connection reset",
        "reset by peer",
        "broken pipe",
        "unexpected eof",
        "channel closed",
        "closed channel",
        "no active ssh connection",
        "session not found",
        "not initialized",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Returns whether initialization can be retried while the node owner reconnects.
pub fn error_should_retry_initialization(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if error_is_auth_failure(error)
        || error_is_permission_denied(error)
        || error_is_not_found(error)
    {
        return false;
    }
    error_is_connection_unavailable(error)
        || lower.contains("not connected")
        || lower.contains("connection timeout")
        || lower.contains("timeout")
}

pub fn error_is_permission_denied(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    !error_is_auth_failure(error)
        && (lower.contains("permission denied") || lower.contains("permissiondenied"))
}

pub fn error_is_not_found(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if [
        "session not found",
        "node not found",
        "connection not found",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return false;
    }
    [
        "file not found",
        "directory not found",
        "path not found",
        "no such file",
        "no such directory",
        "no such path",
        "filenotfound",
        "directorynotfound",
        "pathnotfound",
        "nosuchfile",
        "nosuchdirectory",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub fn error_is_auth_failure(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "authentication failed",
        "auth failed",
        "permission denied (publickey",
        "permission denied (password",
        "permission denied (keyboard-interactive",
        "all authentication methods failed",
        "agent authentication failed",
        "keyboard-interactive",
        "password authentication timed out",
        "host key",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod classification_tests {
    use super::*;

    #[test]
    fn retry_classification_separates_transport_from_path_and_auth_errors() {
        for error in [
            "SSH connection is closed",
            "Connection abc is stale: transport is closed",
            "SFTP init failed: Channel error: SSH connection is closed and cannot open an SFTP channel",
            "Capability unavailable: Session not found: node-1",
            "SFTP subsystem not available: failed to open SFTP channel: channel closed",
        ] {
            assert!(error_is_connection_unavailable(error), "{error}");
        }
        assert!(!error_is_connection_unavailable(
            "Permission denied: /home/me/secret"
        ));
        assert!(error_should_retry_initialization("connection timeout"));
        assert!(!error_should_retry_initialization(
            "Permission denied: /root"
        ));
        assert!(!error_should_retry_initialization("authentication failed"));
        for error in [
            "No such file: /tmp/missing",
            "Directory not found: /home/me/missing",
            "No such file or directory: /home/me/missing",
        ] {
            assert!(error_is_not_found(error), "{error}");
        }
        for error in [
            "Node not found: node-1",
            "Capability unavailable: Session not found: node-1",
        ] {
            assert!(!error_is_not_found(error), "{error}");
        }
        assert!(error_is_auth_failure("permission denied (publickey)"));
        assert!(!error_is_permission_denied("permission denied (publickey)"));
        assert!(error_is_permission_denied("Permission denied: /root"));
    }
}
