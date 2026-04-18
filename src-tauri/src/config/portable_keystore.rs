// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Portable-mode secret storage backed by a local encrypted keystore.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use parking_lot::RwLock;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use zeroize::Zeroizing;

const PORTABLE_KEYSTORE_FILENAME: &str = "keystore.vault";
const PORTABLE_KEYSTORE_FORMAT: &str = "oxideterm.portable.keystore";
const PORTABLE_KEYSTORE_VERSION: u32 = 1;
const PORTABLE_KEYSTORE_NONCE_LEN: usize = 12;
const PORTABLE_KEYSTORE_SALT_LEN: usize = 32;
const PORTABLE_BIOMETRIC_SERVICE: &str = "com.oxideterm.portable.biometric";

#[derive(Debug, thiserror::Error)]
pub enum PortableKeystoreError {
    #[error("Portable keystore is only available in portable mode")]
    NotPortableMode,

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

    #[error("Biometric unlock is not available on this device")]
    BiometricUnavailable,

    #[error("Biometric binding error: {0}")]
    Biometric(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PortableKeystorePayload {
    #[serde(default)]
    services: BTreeMap<String, BTreeMap<String, String>>,
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

#[derive(Debug)]
struct PortableKeystoreSession {
    salt: [u8; PORTABLE_KEYSTORE_SALT_LEN],
    key: Zeroizing<[u8; 32]>,
    payload: PortableKeystorePayload,
}

static PORTABLE_KEYSTORE_SESSION: LazyLock<RwLock<Option<PortableKeystoreSession>>> =
    LazyLock::new(|| RwLock::new(None));

fn biometric_binding_account() -> Result<String, PortableKeystoreError> {
    let binding_key = portable_keystore_file_path()?
        .ok_or(PortableKeystoreError::NotPortableMode)?
        .to_string_lossy()
        .to_string();
    let digest = Sha256::digest(binding_key.as_bytes());
    let suffix: String = digest[..16].iter().map(|byte| format!("{:02x}", byte)).collect();
    Ok(format!("portable-master-{}", suffix))
}

fn biometric_keychain() -> super::keychain::Keychain {
    super::keychain::Keychain::with_system_biometrics(PORTABLE_BIOMETRIC_SERVICE)
}

#[cfg(target_os = "macos")]
pub fn supports_biometric_binding() -> bool {
    super::touch_id::is_biometric_available()
}

#[cfg(not(target_os = "macos"))]
pub fn supports_biometric_binding() -> bool {
    false
}

fn portable_keystore_path() -> Result<PathBuf, PortableKeystoreError> {
    let data_dir = super::portable_data_dir()
        .map_err(|_| PortableKeystoreError::NotPortableMode)?
        .ok_or(PortableKeystoreError::NotPortableMode)?;
    Ok(data_dir.join(PORTABLE_KEYSTORE_FILENAME))
}

fn derive_key(
    password: &str,
    salt: &[u8; PORTABLE_KEYSTORE_SALT_LEN],
    kdf_version: u32,
) -> Result<Zeroizing<[u8; 32]>, PortableKeystoreError> {
    crate::oxide_file::crypto::derive_key(password, salt, kdf_version)
        .map_err(|_| PortableKeystoreError::Crypto)
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
    let nonce = Nonce::from_slice(&nonce_vec);
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(nonce, ciphertext.as_ref())
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
        kdf_version: crate::oxide_file::format::kdf_flags::CURRENT_KDF,
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

pub fn portable_keystore_file_path() -> Result<Option<PathBuf>, PortableKeystoreError> {
    match super::portable_data_dir() {
        Ok(Some(data_dir)) => Ok(Some(data_dir.join(PORTABLE_KEYSTORE_FILENAME))),
        Ok(None) => Ok(None),
        Err(_) => Err(PortableKeystoreError::NotPortableMode),
    }
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
    let key = derive_key(
        password,
        &salt,
        crate::oxide_file::format::kdf_flags::CURRENT_KDF,
    )?;

    let session = PortableKeystoreSession {
        salt,
        key,
        payload: PortableKeystorePayload::default(),
    };

    persist_session_to_path(&path, &session)?;
    *PORTABLE_KEYSTORE_SESSION.write() = Some(session);
    Ok(())
}

pub fn unlock_portable_keystore(password: &str) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    if !path.exists() {
        return Err(PortableKeystoreError::Missing);
    }

    let session = load_session_from_path(&path, password)?;
    *PORTABLE_KEYSTORE_SESSION.write() = Some(session);
    Ok(())
}

pub fn delete_portable_keystore() -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    lock_portable_keystore();
    if path.exists() {
        fs::remove_file(path)?;
    }
    let _ = clear_biometric_binding();
    Ok(())
}

pub fn verify_portable_keystore_password(password: &str) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    if !path.exists() {
        return Err(PortableKeystoreError::Missing);
    }
    load_session_from_path(&path, password).map(|_| ())
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
    let new_key = derive_key(
        new_password,
        &new_salt,
        crate::oxide_file::format::kdf_flags::CURRENT_KDF,
    )?;
    let next_session = PortableKeystoreSession {
        salt: new_salt,
        key: new_key,
        payload: current_session.payload,
    };

    let had_biometric_binding = has_biometric_binding()?;
    if had_biometric_binding {
        bind_biometric_unlock(new_password)?;
    }

    if let Err(err) = persist_session_to_path(&path, &next_session) {
        if had_biometric_binding {
            let _ = bind_biometric_unlock(current_password);
        }
        return Err(err);
    }

    *PORTABLE_KEYSTORE_SESSION.write() = Some(next_session);
    Ok(())
}

pub fn has_biometric_binding() -> Result<bool, PortableKeystoreError> {
    match super::is_portable_mode() {
        Ok(true) => {}
        Ok(false) => return Ok(false),
        Err(_) => return Ok(false),
    }

    if !supports_biometric_binding() {
        return Ok(false);
    }

    let account = biometric_binding_account()?;
    biometric_keychain()
        .exists(&account)
        .map_err(|err| PortableKeystoreError::Biometric(err.to_string()))
}

pub fn bind_biometric_unlock(password: &str) -> Result<(), PortableKeystoreError> {
    if !supports_biometric_binding() {
        return Err(PortableKeystoreError::BiometricUnavailable);
    }

    let account = biometric_binding_account()?;
    biometric_keychain()
        .store(&account, password)
        .map_err(|err| PortableKeystoreError::Biometric(err.to_string()))
}

pub fn clear_biometric_binding() -> Result<(), PortableKeystoreError> {
    if !supports_biometric_binding() {
        return Ok(());
    }

    let account = biometric_binding_account()?;
    biometric_keychain()
        .delete(&account)
        .map_err(|err| PortableKeystoreError::Biometric(err.to_string()))
}

pub fn read_biometric_bound_password() -> Result<String, PortableKeystoreError> {
    if !supports_biometric_binding() {
        return Err(PortableKeystoreError::BiometricUnavailable);
    }

    let account = biometric_binding_account()?;
    biometric_keychain()
        .get(&account)
        .map_err(|err| PortableKeystoreError::Biometric(err.to_string()))
}

pub fn store_secret(
    service: &str,
    account: &str,
    secret: &str,
) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    let mut guard = PORTABLE_KEYSTORE_SESSION.write();
    let session = guard.as_mut().ok_or(PortableKeystoreError::Locked)?;
    session
        .payload
        .services
        .entry(service.to_string())
        .or_default()
        .insert(account.to_string(), secret.to_string());
    persist_session_to_path(&path, session)
}

pub fn get_secret(service: &str, account: &str) -> Result<String, PortableKeystoreError> {
    let guard = PORTABLE_KEYSTORE_SESSION.read();
    let session = guard.as_ref().ok_or(PortableKeystoreError::Locked)?;
    session
        .payload
        .services
        .get(service)
        .and_then(|accounts| accounts.get(account))
        .cloned()
        .ok_or_else(|| PortableKeystoreError::NotFound(account.to_string()))
}

pub fn get_many_secrets(
    service: &str,
    accounts: &[String],
) -> Result<Vec<Option<String>>, PortableKeystoreError> {
    let guard = PORTABLE_KEYSTORE_SESSION.read();
    let session = guard.as_ref().ok_or(PortableKeystoreError::Locked)?;
    let service_map = session.payload.services.get(service);
    Ok(accounts
        .iter()
        .map(|account| service_map.and_then(|m| m.get(account)).cloned())
        .collect())
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
    use crate::oxide_file::format::kdf_flags;
    use tempfile::tempdir;

    fn sample_session(password: &str) -> PortableKeystoreSession {
        let salt = [7u8; PORTABLE_KEYSTORE_SALT_LEN];
        PortableKeystoreSession {
            salt,
            key: derive_key(password, &salt, kdf_flags::CURRENT_KDF).unwrap(),
            payload: PortableKeystorePayload::default(),
        }
    }

    #[test]
    fn round_trip_envelope_encrypts_and_decrypts() {
        let temp = tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let mut session = sample_session("secret123");
        session
            .payload
            .services
            .entry("svc".to_string())
            .or_default()
            .insert("account".to_string(), "value".to_string());

        persist_session_to_path(&path, &session).unwrap();
        let restored = load_session_from_path(&path, "secret123").unwrap();

        assert_eq!(
            restored
                .payload
                .services
                .get("svc")
                .and_then(|accounts| accounts.get("account")),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn wrong_password_fails_decryption() {
        let temp = tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let session = sample_session("secret123");

        persist_session_to_path(&path, &session).unwrap();
        let err = load_session_from_path(&path, "wrong-password").unwrap_err();

        assert!(matches!(err, PortableKeystoreError::DecryptionFailed));
    }

    #[test]
    fn corrupted_file_fails_cleanly() {
        let temp = tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        fs::write(&path, b"not-json").unwrap();

        let err = load_session_from_path(&path, "secret123").unwrap_err();
        assert!(matches!(err, PortableKeystoreError::Json(_)));
    }

    #[test]
    fn change_password_reencrypts_payload() {
        let temp = tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let mut session = sample_session("secret123");
        session
            .payload
            .services
            .entry("svc".to_string())
            .or_default()
            .insert("account".to_string(), "value".to_string());

        persist_session_to_path(&path, &session).unwrap();

        let restored = load_session_from_path(&path, "secret123").unwrap();
        let mut new_salt = [0u8; PORTABLE_KEYSTORE_SALT_LEN];
        rand::rngs::OsRng.fill_bytes(&mut new_salt);
        let rewritten = PortableKeystoreSession {
            salt: new_salt,
            key: derive_key("new-secret123", &new_salt, kdf_flags::CURRENT_KDF).unwrap(),
            payload: restored.payload,
        };
        persist_session_to_path(&path, &rewritten).unwrap();

        assert!(load_session_from_path(&path, "secret123").is_err());
        let updated = load_session_from_path(&path, "new-secret123").unwrap();
        assert_eq!(
            updated
                .payload
                .services
                .get("svc")
                .and_then(|accounts| accounts.get("account")),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn biometric_binding_is_false_outside_portable_mode() {
        assert!(!has_biometric_binding().unwrap());
    }
}
