//! SSH Error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

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
