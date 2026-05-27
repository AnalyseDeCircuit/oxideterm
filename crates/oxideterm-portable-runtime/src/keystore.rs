// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Encrypted secret storage for portable mode.

use std::{collections::BTreeMap, fmt, fs, path::Path, sync::LazyLock};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use parking_lot::RwLock;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{PortableError, portable_keystore_file_path};

const PORTABLE_KEYSTORE_FORMAT: &str = "oxideterm.portable.keystore";
const PORTABLE_KEYSTORE_VERSION: u32 = 1;
const PORTABLE_KEYSTORE_NONCE_LEN: usize = 12;
const PORTABLE_KEYSTORE_SALT_LEN: usize = 32;
const PORTABLE_KEYSTORE_KDF_V1: u32 = 0x0001;
const PORTABLE_KEYSTORE_CURRENT_KDF: u32 = PORTABLE_KEYSTORE_KDF_V1;

static PORTABLE_KEYSTORE_SESSION: LazyLock<RwLock<Option<PortableKeystoreSession>>> =
    LazyLock::new(|| RwLock::new(None));

#[derive(Debug, thiserror::Error)]
pub enum PortableKeystoreError {
    #[error("Portable keystore is only available in portable mode")]
    NotPortableMode,

    #[error("Portable mode state error: {0}")]
    PortableState(String),

    #[error("Portable keystore is not initialized")]
    Missing,

    #[error("Portable keystore already exists")]
    AlreadyExists,

    #[error("Portable keystore is locked")]
    Locked,

    #[error("Secret not found for ID: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("MessagePack encode error: {0}")]
    MsgPackEncode(#[from] rmp_serde::encode::Error),

    #[error("MessagePack decode error: {0}")]
    MsgPackDecode(#[from] rmp_serde::decode::Error),

    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("Portable keystore is corrupted")]
    Corrupted,

    #[error("Unsupported portable keystore version {0}")]
    UnsupportedVersion(u32),

    #[error("Portable keystore cryptographic operation failed")]
    Crypto,

    #[error("Portable keystore decryption failed")]
    DecryptionFailed,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct PortableKeystorePayload {
    #[serde(default)]
    services: BTreeMap<String, BTreeMap<String, Zeroizing<String>>>,
}

impl fmt::Debug for PortableKeystorePayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secret_count = self.services.values().map(BTreeMap::len).sum::<usize>();
        formatter
            .debug_struct("PortableKeystorePayload")
            .field("services", &self.services.len())
            .field("secrets", &secret_count)
            .field("secret_values", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortableKeystoreEnvelope {
    format: String,
    version: u32,
    kdf_version: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}

struct PortableKeystoreSession {
    salt: [u8; PORTABLE_KEYSTORE_SALT_LEN],
    key: Zeroizing<[u8; 32]>,
    payload: PortableKeystorePayload,
}

impl fmt::Debug for PortableKeystoreSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortableKeystoreSession")
            .field("salt_len", &self.salt.len())
            .field("key", &"<redacted>")
            .field("payload", &self.payload)
            .finish()
    }
}

fn portable_keystore_path() -> Result<std::path::PathBuf, PortableKeystoreError> {
    portable_keystore_file_path()
        .map_err(map_portable_error)?
        .ok_or(PortableKeystoreError::NotPortableMode)
}

fn map_portable_error(error: PortableError) -> PortableKeystoreError {
    PortableKeystoreError::PortableState(error.to_string())
}

fn derive_key(
    password: &str,
    salt: &[u8; PORTABLE_KEYSTORE_SALT_LEN],
    kdf_version: u32,
) -> Result<Zeroizing<[u8; 32]>, PortableKeystoreError> {
    if kdf_version != PORTABLE_KEYSTORE_KDF_V1 && kdf_version != 0 {
        return Err(PortableKeystoreError::UnsupportedVersion(kdf_version));
    }

    // Match the .oxide/Tauri portable KDF envelope: Argon2id, 256 MiB memory,
    // 4 iterations, 4 lanes. The derived key is zeroized when replaced/dropped.
    let params = Params::new(262_144, 4, 4, Some(32)).map_err(|_| PortableKeystoreError::Crypto)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|_| PortableKeystoreError::Crypto)?;
    Ok(key)
}

fn load_session_from_path(
    path: &Path,
    password: &str,
) -> Result<PortableKeystoreSession, PortableKeystoreError> {
    let bytes = fs::read(path)?;
    let envelope: PortableKeystoreEnvelope = serde_json::from_slice(&bytes)?;
    if envelope.format != PORTABLE_KEYSTORE_FORMAT {
        return Err(PortableKeystoreError::Corrupted);
    }
    if envelope.version != PORTABLE_KEYSTORE_VERSION {
        return Err(PortableKeystoreError::UnsupportedVersion(envelope.version));
    }

    let salt_vec = BASE64.decode(envelope.salt)?;
    let nonce_vec = BASE64.decode(envelope.nonce)?;
    let ciphertext = BASE64.decode(envelope.ciphertext)?;
    let salt: [u8; PORTABLE_KEYSTORE_SALT_LEN] = salt_vec
        .try_into()
        .map_err(|_| PortableKeystoreError::Corrupted)?;
    if nonce_vec.len() != PORTABLE_KEYSTORE_NONCE_LEN {
        return Err(PortableKeystoreError::Corrupted);
    }

    let key = derive_key(password, &salt, envelope.kdf_version)?;
    let cipher =
        ChaCha20Poly1305::new_from_slice(&*key).map_err(|_| PortableKeystoreError::Crypto)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(Nonce::from_slice(&nonce_vec), ciphertext.as_ref())
            .map_err(|_| PortableKeystoreError::DecryptionFailed)?,
    );
    let payload = rmp_serde::from_slice::<PortableKeystorePayload>(&plaintext)?;
    Ok(PortableKeystoreSession { salt, key, payload })
}

fn persist_session_to_path(
    path: &Path,
    session: &PortableKeystoreSession,
) -> Result<(), PortableKeystoreError> {
    let plaintext = Zeroizing::new(rmp_serde::to_vec_named(&session.payload)?);
    let cipher = ChaCha20Poly1305::new_from_slice(&*session.key)
        .map_err(|_| PortableKeystoreError::Crypto)?;
    let mut nonce = [0u8; PORTABLE_KEYSTORE_NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| PortableKeystoreError::Crypto)?;

    let envelope = PortableKeystoreEnvelope {
        format: PORTABLE_KEYSTORE_FORMAT.to_string(),
        version: PORTABLE_KEYSTORE_VERSION,
        kdf_version: PORTABLE_KEYSTORE_CURRENT_KDF,
        salt: BASE64.encode(session.salt),
        nonce: BASE64.encode(nonce),
        ciphertext: BASE64.encode(ciphertext),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("vault.tmp");
    fs::write(&tmp_path, serde_json::to_vec_pretty(&envelope)?)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub fn portable_keystore_exists() -> Result<bool, PortableKeystoreError> {
    Ok(portable_keystore_path()?.exists())
}

pub fn is_portable_keystore_unlocked() -> bool {
    PORTABLE_KEYSTORE_SESSION.read().is_some()
}

pub fn lock_portable_keystore() {
    *PORTABLE_KEYSTORE_SESSION.write() = None;
}

pub fn create_portable_keystore(password: &str) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    if path.exists() {
        return Err(PortableKeystoreError::AlreadyExists);
    }
    let mut salt = [0u8; PORTABLE_KEYSTORE_SALT_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let key = derive_key(password, &salt, PORTABLE_KEYSTORE_CURRENT_KDF)?;
    let session = PortableKeystoreSession {
        salt,
        key,
        payload: PortableKeystorePayload::default(),
    };
    persist_session_to_path(&path, &session)?;
    *PORTABLE_KEYSTORE_SESSION.write() = Some(session);
    let _ = crate::set_portable_bootstrap_status(crate::PortableBootstrapStatus::Unlocked);
    Ok(())
}

pub fn unlock_portable_keystore(password: &str) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    if !path.exists() {
        return Err(PortableKeystoreError::Missing);
    }
    let session = load_session_from_path(&path, password)?;
    *PORTABLE_KEYSTORE_SESSION.write() = Some(session);
    let _ = crate::set_portable_bootstrap_status(crate::PortableBootstrapStatus::Unlocked);
    Ok(())
}

pub fn change_portable_keystore_password(
    current_password: &str,
    new_password: &str,
) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    if !path.exists() {
        return Err(PortableKeystoreError::Missing);
    }
    let current_session = load_session_from_path(&path, current_password)?;
    let mut new_salt = [0u8; PORTABLE_KEYSTORE_SALT_LEN];
    rand::rngs::OsRng.fill_bytes(&mut new_salt);
    let new_key = derive_key(new_password, &new_salt, PORTABLE_KEYSTORE_CURRENT_KDF)?;
    let next_session = PortableKeystoreSession {
        salt: new_salt,
        key: new_key,
        payload: current_session.payload,
    };
    persist_session_to_path(&path, &next_session)?;
    *PORTABLE_KEYSTORE_SESSION.write() = Some(next_session);
    let _ = crate::set_portable_bootstrap_status(crate::PortableBootstrapStatus::Unlocked);
    Ok(())
}

pub fn delete_portable_keystore() -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    lock_portable_keystore();
    if path.exists() {
        fs::remove_file(path)?;
    }
    let _ = crate::set_portable_bootstrap_status(crate::PortableBootstrapStatus::NeedsSetup);
    Ok(())
}

pub fn store_secret(
    service: &str,
    account: &str,
    secret: &str,
) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    let mut guard = PORTABLE_KEYSTORE_SESSION.write();
    let session = guard.as_mut().ok_or(PortableKeystoreError::Locked)?;
    // Secret crosses from provider-specific keychain code into the portable
    // vault here; it is immediately persisted inside an encrypted envelope.
    session
        .payload
        .services
        .entry(service.to_string())
        .or_default()
        .insert(account.to_string(), Zeroizing::new(secret.to_string()));
    persist_session_to_path(&path, session)
}

pub fn get_secret(
    service: &str,
    account: &str,
) -> Result<Zeroizing<String>, PortableKeystoreError> {
    let guard = PORTABLE_KEYSTORE_SESSION.read();
    let session = guard.as_ref().ok_or(PortableKeystoreError::Locked)?;
    session
        .payload
        .services
        .get(service)
        .and_then(|accounts| accounts.get(account))
        .map(|secret| Zeroizing::new(secret.to_string()))
        .ok_or_else(|| PortableKeystoreError::NotFound(account.to_string()))
}

pub fn delete_secret(service: &str, account: &str) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    let mut guard = PORTABLE_KEYSTORE_SESSION.write();
    let session = guard.as_mut().ok_or(PortableKeystoreError::Locked)?;
    if let Some(accounts) = session.payload.services.get_mut(service) {
        accounts.remove(account);
        if accounts.is_empty() {
            session.payload.services.remove(service);
        }
    }
    persist_session_to_path(&path, session)
}

pub fn secret_exists(service: &str, account: &str) -> Result<bool, PortableKeystoreError> {
    let guard = PORTABLE_KEYSTORE_SESSION.read();
    let session = guard.as_ref().ok_or(PortableKeystoreError::Locked)?;
    Ok(session
        .payload
        .services
        .get(service)
        .and_then(|accounts| accounts.get(account))
        .is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PORTABLE_KEYSTORE_FILENAME;

    fn sample_session(password: &str) -> PortableKeystoreSession {
        let salt = [7u8; PORTABLE_KEYSTORE_SALT_LEN];
        PortableKeystoreSession {
            salt,
            key: derive_key(password, &salt, PORTABLE_KEYSTORE_CURRENT_KDF).unwrap(),
            payload: PortableKeystorePayload::default(),
        }
    }

    #[test]
    fn round_trip_envelope_encrypts_and_decrypts() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let mut session = sample_session("secret123");
        session
            .payload
            .services
            .entry("svc".to_string())
            .or_default()
            .insert("account".to_string(), Zeroizing::new("value".to_string()));

        persist_session_to_path(&path, &session).unwrap();
        let restored = load_session_from_path(&path, "secret123").unwrap();

        let restored_secret = restored
            .payload
            .services
            .get("svc")
            .and_then(|accounts| accounts.get("account"));
        assert_eq!(restored_secret.map(|secret| secret.as_str()), Some("value"));
    }

    #[test]
    fn debug_output_redacts_payload_secret_values() {
        let mut session = sample_session("secret123");
        session
            .payload
            .services
            .entry("svc".to_string())
            .or_default()
            .insert(
                "account".to_string(),
                Zeroizing::new("do-not-print".to_string()),
            );

        let debug = format!("{session:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("do-not-print"));
    }

    #[test]
    fn portable_state_errors_are_not_reported_as_non_portable_mode() {
        let error = map_portable_error(PortableError::InvalidPortableDataDir(
            "../escape".to_string(),
        ));

        assert!(matches!(error, PortableKeystoreError::PortableState(_)));
    }

    #[test]
    fn wrong_password_fails_decryption() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let session = sample_session("secret123");

        persist_session_to_path(&path, &session).unwrap();
        let error = load_session_from_path(&path, "wrong-password").unwrap_err();

        assert!(matches!(error, PortableKeystoreError::DecryptionFailed));
    }

    #[test]
    fn change_password_reencrypts_payload() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let mut session = sample_session("secret123");
        session
            .payload
            .services
            .entry("svc".to_string())
            .or_default()
            .insert("account".to_string(), Zeroizing::new("value".to_string()));

        persist_session_to_path(&path, &session).unwrap();

        let restored = load_session_from_path(&path, "secret123").unwrap();
        let mut new_salt = [0u8; PORTABLE_KEYSTORE_SALT_LEN];
        rand::rngs::OsRng.fill_bytes(&mut new_salt);
        let rewritten = PortableKeystoreSession {
            salt: new_salt,
            key: derive_key("new-secret123", &new_salt, PORTABLE_KEYSTORE_CURRENT_KDF).unwrap(),
            payload: restored.payload,
        };
        persist_session_to_path(&path, &rewritten).unwrap();

        assert!(load_session_from_path(&path, "secret123").is_err());
        let updated = load_session_from_path(&path, "new-secret123").unwrap();
        let updated_secret = updated
            .payload
            .services
            .get("svc")
            .and_then(|accounts| accounts.get("account"));
        assert_eq!(updated_secret.map(|secret| secret.as_str()), Some("value"));
    }
}
