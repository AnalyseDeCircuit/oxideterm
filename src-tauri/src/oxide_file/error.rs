//! Error types for .oxide file operations

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

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("MessagePack serialization error: {0}")]
    MsgPack(String),
}

impl From<rmp_serde::encode::Error> for OxideFileError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        OxideFileError::MsgPack(e.to_string())
    }
}

impl From<rmp_serde::decode::Error> for OxideFileError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        OxideFileError::MsgPack(e.to_string())
    }
}
