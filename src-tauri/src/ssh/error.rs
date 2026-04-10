// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH Error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("DNS resolution failed for {address}: {message}")]
    DnsResolution { address: String, message: String },

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Session error: {0}")]
    SessionError(String),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("SSH protocol error: {0}")]
    ProtocolError(String),

    #[error("Key error: {0}")]
    KeyError(String),

    #[error("Certificate load error: {0}")]
    CertificateLoadError(String),

    #[error("Certificate parse error: {0}")]
    CertificateParseError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Disconnected")]
    Disconnected,

    #[error("SSH Agent not available: {0}")]
    AgentNotAvailable(String),

    #[error("SSH Agent error: {0}")]
    AgentError(String),

    #[error(
        "Host key verification failed: unknown host {host}:{port}. Fingerprint: {fingerprint}. Add to known_hosts or disable strict mode."
    )]
    HostKeyUnknown {
        host: String,
        port: u16,
        fingerprint: String,
    },

    #[error(
        "Host key verification failed for {host}:{port}. Expected: {expected_fingerprint}. Actual: {actual_fingerprint}."
    )]
    HostKeyChanged {
        host: String,
        port: u16,
        expected_fingerprint: String,
        actual_fingerprint: String,
    },
}

impl From<russh::Error> for SshError {
    fn from(err: russh::Error) -> Self {
        SshError::ProtocolError(err.to_string())
    }
}

impl From<russh::keys::Error> for SshError {
    fn from(err: russh::keys::Error) -> Self {
        SshError::KeyError(err.to_string())
    }
}

// Make SshError serializable for Tauri commands
impl serde::Serialize for SshError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_error_serializes_as_display_string() {
        let json = serde_json::to_string(&SshError::AuthenticationFailed("bad credentials".into()))
            .unwrap();

        assert_eq!(json, "\"Authentication failed: bad credentials\"");
    }

    #[test]
    fn test_ssh_error_from_io_preserves_message() {
        let error: SshError =
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope").into();

        assert!(matches!(error, SshError::IoError(_)));
        assert!(error.to_string().contains("nope"));
    }

    #[test]
    fn test_ssh_error_from_russh_maps_to_protocol_error() {
        let error: SshError = russh::Error::CouldNotReadKey.into();

        assert!(matches!(error, SshError::ProtocolError(_)));
        assert_eq!(error.to_string(), "SSH protocol error: Could not read key");
    }

    #[test]
    fn test_ssh_error_from_russh_keys_maps_to_key_error() {
        let key_error = russh::keys::decode_secret_key("not-a-key", None).unwrap_err();
        let error: SshError = key_error.into();

        assert!(matches!(error, SshError::KeyError(_)));
        assert!(error.to_string().starts_with("Key error: "));
    }
}
