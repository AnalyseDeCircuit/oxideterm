// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! .oxide file format module
//!
//! Provides encrypted configuration file export/import with:
//! - ChaCha20-Poly1305 AEAD encryption
//! - Argon2id key derivation (high strength)
//! - Binary file format with unencrypted metadata
//! - Git-friendly and offline-decryptable

pub mod crypto;
pub mod error;
pub mod format;

// Re-export main types
pub use crypto::{
    compute_checksum, decrypt_oxide_file, decrypt_oxide_file_with_progress, encrypt_oxide_file,
    encrypt_oxide_file_with_progress,
};
pub use error::OxideFileError;
pub use format::{
    EncryptedAuth, EncryptedConnection, EncryptedForward, EncryptedPayload, EncryptedPluginSetting,
    EncryptedProxyHop, OxideFile, OxideMetadata,
};
