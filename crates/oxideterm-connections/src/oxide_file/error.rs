use thiserror::Error;

#[derive(Debug, Error)]
pub enum OxideFileError {
    #[error("Invalid magic number")]
    InvalidMagic,
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u32),
    #[error("Unsupported KDF version: {0}")]
    UnsupportedKdfVersion(u32),
    #[error("Invalid file format: {0}")]
    InvalidFormat(String),
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Decryption failed (wrong password or corrupted data)")]
    DecryptionFailed,
    #[error("Checksum mismatch (data corrupted or tampered)")]
    ChecksumMismatch,
    #[error("Cryptographic error")]
    CryptoError,
    #[error("Password must be at least 6 characters")]
    PasswordTooShort,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("MessagePack serialization error: {0}")]
    MsgPack(String),
    #[error("Connection store error: {0}")]
    Store(String),
}

impl From<rmp_serde::encode::Error> for OxideFileError {
    fn from(error: rmp_serde::encode::Error) -> Self {
        Self::MsgPack(error.to_string())
    }
}

impl From<rmp_serde::decode::Error> for OxideFileError {
    fn from(error: rmp_serde::decode::Error) -> Self {
        Self::MsgPack(error.to_string())
    }
}

impl From<anyhow::Error> for OxideFileError {
    fn from(error: anyhow::Error) -> Self {
        Self::Store(error.to_string())
    }
}
