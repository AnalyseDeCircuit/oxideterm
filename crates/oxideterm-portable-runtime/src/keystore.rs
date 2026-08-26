// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Encrypted secret storage for portable mode.

use std::{collections::BTreeMap, fmt, fs, path::Path, sync::LazyLock};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use oxideterm_atomic_file::{durable_remove, durable_write};
use oxideterm_secret_store::NativeSecretStore;
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
const PORTABLE_AUTO_UNLOCK_SERVICE: &str = "com.oxideterm.portable-auto-unlock";

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

    #[error("Multiple legacy secret entries match one portable account")]
    AmbiguousLegacyAccounts,

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

    #[error("Portable automatic unlock credential is invalid")]
    InvalidAutoUnlockCredential,

    #[error("Failed to access the system credential manager")]
    CredentialStore,
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

struct DecodedPortableKeystoreEnvelope {
    kdf_version: u32,
    salt: [u8; PORTABLE_KEYSTORE_SALT_LEN],
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortableAutoUnlockOutcome {
    NotConfigured,
    Unlocked,
    InvalidCredentialRemoved,
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
    let decoded = decode_envelope_from_path(path)?;
    let key = derive_key(password, &decoded.salt, decoded.kdf_version)?;
    decrypt_session(decoded, key)
}

fn decode_envelope_from_path(
    path: &Path,
) -> Result<DecodedPortableKeystoreEnvelope, PortableKeystoreError> {
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

    if envelope.kdf_version != PORTABLE_KEYSTORE_KDF_V1 && envelope.kdf_version != 0 {
        return Err(PortableKeystoreError::UnsupportedVersion(
            envelope.kdf_version,
        ));
    }
    Ok(DecodedPortableKeystoreEnvelope {
        kdf_version: envelope.kdf_version,
        salt,
        nonce: nonce_vec,
        ciphertext,
    })
}

fn decrypt_session(
    decoded: DecodedPortableKeystoreEnvelope,
    key: Zeroizing<[u8; 32]>,
) -> Result<PortableKeystoreSession, PortableKeystoreError> {
    let cipher =
        ChaCha20Poly1305::new_from_slice(&*key).map_err(|_| PortableKeystoreError::Crypto)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(&decoded.nonce),
                decoded.ciphertext.as_ref(),
            )
            .map_err(|_| PortableKeystoreError::DecryptionFailed)?,
    );
    let payload = rmp_serde::from_slice::<PortableKeystorePayload>(&plaintext)?;
    Ok(PortableKeystoreSession {
        salt: decoded.salt,
        key,
        payload,
    })
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

    let serialized = Zeroizing::new(serde_json::to_vec_pretty(&envelope)?);
    durable_write(path, &serialized)?;
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

fn portable_auto_unlock_account(path: &Path) -> String {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format!("v1:{}", resolved.display())
}

fn credential_store_error<T>(_error: T) -> PortableKeystoreError {
    // Credential backends may include account metadata in their error chain.
    // Keep that detail outside UI, logs, and diagnostics.
    PortableKeystoreError::CredentialStore
}

fn encode_auto_unlock_key(key: &[u8; 32]) -> Zeroizing<String> {
    // The encoded derived key is an unavoidable OS-store handoff owner and is
    // wiped immediately after the credential manager consumes it.
    Zeroizing::new(BASE64.encode(key))
}

fn decode_auto_unlock_key(token: &str) -> Result<Zeroizing<[u8; 32]>, PortableKeystoreError> {
    let decoded = Zeroizing::new(
        BASE64
            .decode(token)
            .map_err(|_| PortableKeystoreError::InvalidAutoUnlockCredential)?,
    );
    let key = decoded
        .as_slice()
        .try_into()
        .map_err(|_| PortableKeystoreError::InvalidAutoUnlockCredential)?;
    Ok(Zeroizing::new(key))
}

fn store_auto_unlock_key(path: &Path, key: &[u8; 32]) -> Result<(), PortableKeystoreError> {
    let account = portable_auto_unlock_account(path);
    let token = encode_auto_unlock_key(key);
    NativeSecretStore::new(PORTABLE_AUTO_UNLOCK_SERVICE)
        .store(&account, token.as_str())
        .map_err(credential_store_error)
}

pub fn portable_auto_unlock_enabled() -> Result<bool, PortableKeystoreError> {
    let path = portable_keystore_path()?;
    if !path.exists() {
        return Ok(false);
    }
    NativeSecretStore::new(PORTABLE_AUTO_UNLOCK_SERVICE)
        .exists(&portable_auto_unlock_account(&path))
        .map_err(credential_store_error)
}

pub fn enable_portable_auto_unlock() -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    let token = {
        let guard = PORTABLE_KEYSTORE_SESSION.read();
        let session = guard.as_ref().ok_or(PortableKeystoreError::Locked)?;
        encode_auto_unlock_key(&session.key)
    };
    NativeSecretStore::new(PORTABLE_AUTO_UNLOCK_SERVICE)
        .store(&portable_auto_unlock_account(&path), token.as_str())
        .map_err(credential_store_error)
}

pub fn disable_portable_auto_unlock() -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    NativeSecretStore::new(PORTABLE_AUTO_UNLOCK_SERVICE)
        .delete(&portable_auto_unlock_account(&path))
        .map_err(credential_store_error)
}

pub fn try_portable_auto_unlock() -> Result<PortableAutoUnlockOutcome, PortableKeystoreError> {
    let path = portable_keystore_path()?;
    if !path.exists() {
        return Ok(PortableAutoUnlockOutcome::NotConfigured);
    }
    let store = NativeSecretStore::new(PORTABLE_AUTO_UNLOCK_SERVICE);
    let account = portable_auto_unlock_account(&path);
    let Some(token) = store.get(&account).map_err(credential_store_error)? else {
        return Ok(PortableAutoUnlockOutcome::NotConfigured);
    };
    let key = match decode_auto_unlock_key(token.as_str()) {
        Ok(key) => key,
        Err(PortableKeystoreError::InvalidAutoUnlockCredential) => {
            store.delete(&account).map_err(credential_store_error)?;
            return Ok(PortableAutoUnlockOutcome::InvalidCredentialRemoved);
        }
        Err(error) => return Err(error),
    };
    let decoded = decode_envelope_from_path(&path)?;
    match decrypt_session(decoded, key) {
        Ok(session) => {
            *PORTABLE_KEYSTORE_SESSION.write() = Some(session);
            let _ = crate::set_portable_bootstrap_status(crate::PortableBootstrapStatus::Unlocked);
            Ok(PortableAutoUnlockOutcome::Unlocked)
        }
        Err(PortableKeystoreError::DecryptionFailed) => {
            // A stale device credential must not block the password fallback or
            // be retried on every subsequent launch.
            store.delete(&account).map_err(credential_store_error)?;
            Ok(PortableAutoUnlockOutcome::InvalidCredentialRemoved)
        }
        Err(error) => Err(error),
    }
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
    let auto_unlock_enabled = portable_auto_unlock_enabled()?;
    let mut new_salt = [0u8; PORTABLE_KEYSTORE_SALT_LEN];
    rand::rngs::OsRng.fill_bytes(&mut new_salt);
    let new_key = derive_key(new_password, &new_salt, PORTABLE_KEYSTORE_CURRENT_KDF)?;
    let next_session = PortableKeystoreSession {
        salt: new_salt,
        key: new_key,
        payload: current_session.payload,
    };
    if auto_unlock_enabled {
        // Publish the matching device credential first so a successful vault
        // replacement can never leave automatic unlock bound to the old key.
        store_auto_unlock_key(&path, &next_session.key)?;
    }
    if let Err(error) = persist_session_to_path(&path, &next_session) {
        if auto_unlock_enabled {
            // The old vault is still authoritative. Best-effort restoration
            // avoids turning an atomic file-write failure into a stale binding.
            let _ = store_auto_unlock_key(&path, &current_session.key);
        }
        return Err(error);
    }
    *PORTABLE_KEYSTORE_SESSION.write() = Some(next_session);
    let _ = crate::set_portable_bootstrap_status(crate::PortableBootstrapStatus::Unlocked);
    Ok(())
}

pub fn delete_portable_keystore() -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    let auto_unlock_account = portable_auto_unlock_account(&path);
    lock_portable_keystore();
    durable_remove(&path)?;
    // Once the vault is gone, an orphaned derived key cannot reveal data. Do
    // not make a completed reset fail solely because the OS store is offline.
    let _ = NativeSecretStore::new(PORTABLE_AUTO_UNLOCK_SERVICE).delete(&auto_unlock_account);
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

/// Stores the canonical portable account and removes obsolete username-bound aliases.
pub fn store_secret_replacing_legacy_accounts(
    service: &str,
    account: &str,
    legacy_account_suffix: &str,
    secret: &str,
) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    let mut guard = PORTABLE_KEYSTORE_SESSION.write();
    let session = guard.as_mut().ok_or(PortableKeystoreError::Locked)?;
    store_secret_replacing_legacy_accounts_at_path(
        &path,
        session,
        service,
        account,
        legacy_account_suffix,
        secret,
    )
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

/// Loads a canonical portable account or atomically migrates one legacy alias.
pub fn get_secret_migrating_legacy_account(
    service: &str,
    account: &str,
    legacy_account_suffix: &str,
) -> Result<Zeroizing<String>, PortableKeystoreError> {
    let path = portable_keystore_path()?;
    let mut guard = PORTABLE_KEYSTORE_SESSION.write();
    let session = guard.as_mut().ok_or(PortableKeystoreError::Locked)?;
    get_secret_migrating_legacy_account_at_path(
        &path,
        session,
        service,
        account,
        legacy_account_suffix,
    )
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

/// Deletes a canonical portable account together with every obsolete legacy alias.
pub fn delete_secret_with_legacy_accounts(
    service: &str,
    account: &str,
    legacy_account_suffix: &str,
) -> Result<(), PortableKeystoreError> {
    let path = portable_keystore_path()?;
    let mut guard = PORTABLE_KEYSTORE_SESSION.write();
    let session = guard.as_mut().ok_or(PortableKeystoreError::Locked)?;
    delete_secret_with_legacy_accounts_at_path(
        &path,
        session,
        service,
        account,
        legacy_account_suffix,
    )
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

fn store_secret_replacing_legacy_accounts_at_path(
    path: &Path,
    session: &mut PortableKeystoreSession,
    service: &str,
    account: &str,
    legacy_account_suffix: &str,
    secret: &str,
) -> Result<(), PortableKeystoreError> {
    let (previous, legacy_entries) = {
        let accounts = session
            .payload
            .services
            .entry(service.to_string())
            .or_default();
        let legacy_accounts = matching_legacy_accounts(accounts, account, legacy_account_suffix);
        let legacy_entries = remove_accounts(accounts, legacy_accounts);
        let previous = accounts.remove(account);
        accounts.insert(account.to_string(), Zeroizing::new(secret.to_string()));
        (previous, legacy_entries)
    };

    if let Err(error) = persist_session_to_path(path, session) {
        // Restore the unlocked in-memory session if durable persistence fails.
        let accounts = session
            .payload
            .services
            .entry(service.to_string())
            .or_default();
        accounts.remove(account);
        if let Some(previous) = previous {
            accounts.insert(account.to_string(), previous);
        }
        restore_accounts(accounts, legacy_entries);
        remove_empty_service(&mut session.payload, service);
        return Err(error);
    }

    // Removed values stay zeroizing and are wiped when these owners drop.
    Ok(())
}

fn get_secret_migrating_legacy_account_at_path(
    path: &Path,
    session: &mut PortableKeystoreSession,
    service: &str,
    account: &str,
    legacy_account_suffix: &str,
) -> Result<Zeroizing<String>, PortableKeystoreError> {
    let Some(accounts) = session.payload.services.get_mut(service) else {
        return Err(PortableKeystoreError::NotFound(account.to_string()));
    };
    if let Some(secret) = accounts.get(account) {
        // The returned copy is an unavoidable output owner and remains zeroizing.
        return Ok(Zeroizing::new(secret.to_string()));
    }

    let mut legacy_accounts = matching_legacy_accounts(accounts, account, legacy_account_suffix);
    if legacy_accounts.len() > 1 {
        return Err(PortableKeystoreError::AmbiguousLegacyAccounts);
    }
    let Some(legacy_account) = legacy_accounts.pop() else {
        return Err(PortableKeystoreError::NotFound(account.to_string()));
    };
    let secret = accounts
        .remove(&legacy_account)
        .ok_or_else(|| PortableKeystoreError::NotFound(account.to_string()))?;
    accounts.insert(account.to_string(), secret);

    if let Err(error) = persist_session_to_path(path, session) {
        // Roll the alias move back without cloning the secret value.
        let accounts = session
            .payload
            .services
            .get_mut(service)
            .expect("portable service disappeared during migration");
        let secret = accounts
            .remove(account)
            .expect("canonical portable account disappeared during migration");
        accounts.insert(legacy_account, secret);
        return Err(error);
    }

    let secret = session
        .payload
        .services
        .get(service)
        .and_then(|accounts| accounts.get(account))
        .expect("migrated portable account disappeared after persistence");
    // Authentication receives one zeroizing owner; the encrypted session keeps
    // the durable canonical copy until the vault is locked.
    Ok(Zeroizing::new(secret.to_string()))
}

fn delete_secret_with_legacy_accounts_at_path(
    path: &Path,
    session: &mut PortableKeystoreSession,
    service: &str,
    account: &str,
    legacy_account_suffix: &str,
) -> Result<(), PortableKeystoreError> {
    let removed_entries = {
        let Some(accounts) = session.payload.services.get_mut(service) else {
            return persist_session_to_path(path, session);
        };
        let mut accounts_to_remove =
            matching_legacy_accounts(accounts, account, legacy_account_suffix);
        accounts_to_remove.push(account.to_string());
        remove_accounts(accounts, accounts_to_remove)
    };
    remove_empty_service(&mut session.payload, service);

    if let Err(error) = persist_session_to_path(path, session) {
        let accounts = session
            .payload
            .services
            .entry(service.to_string())
            .or_default();
        restore_accounts(accounts, removed_entries);
        remove_empty_service(&mut session.payload, service);
        return Err(error);
    }

    // Deleted secret values are wiped when the removed zeroizing owners drop.
    Ok(())
}

fn matching_legacy_accounts(
    accounts: &BTreeMap<String, Zeroizing<String>>,
    account: &str,
    legacy_account_suffix: &str,
) -> Vec<String> {
    accounts
        .keys()
        .filter(|candidate| {
            candidate.as_str() != account && candidate.ends_with(legacy_account_suffix)
        })
        .cloned()
        .collect()
}

fn remove_accounts(
    accounts: &mut BTreeMap<String, Zeroizing<String>>,
    accounts_to_remove: Vec<String>,
) -> Vec<(String, Zeroizing<String>)> {
    accounts_to_remove
        .into_iter()
        .filter_map(|account| accounts.remove(&account).map(|secret| (account, secret)))
        .collect()
}

fn restore_accounts(
    accounts: &mut BTreeMap<String, Zeroizing<String>>,
    removed_entries: Vec<(String, Zeroizing<String>)>,
) {
    accounts.extend(removed_entries);
}

fn remove_empty_service(payload: &mut PortableKeystorePayload, service: &str) {
    if payload
        .services
        .get(service)
        .is_some_and(BTreeMap::is_empty)
    {
        payload.services.remove(service);
    }
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
    fn encoded_auto_unlock_key_decrypts_the_vault() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let session = sample_session("secret123");
        persist_session_to_path(&path, &session).unwrap();

        let token = encode_auto_unlock_key(&session.key);
        let key = decode_auto_unlock_key(token.as_str()).unwrap();
        let restored = decrypt_session(decode_envelope_from_path(&path).unwrap(), key).unwrap();

        assert_eq!(restored.salt, session.salt);
    }

    #[test]
    fn malformed_auto_unlock_key_is_rejected() {
        for token in ["not-base64", "c2hvcnQ="] {
            assert!(matches!(
                decode_auto_unlock_key(token),
                Err(PortableKeystoreError::InvalidAutoUnlockCredential)
            ));
        }
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

    #[test]
    fn legacy_username_account_is_migrated_and_persisted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let mut session = sample_session("secret123");
        session
            .payload
            .services
            .entry("svc".to_string())
            .or_default()
            .insert(
                "old-user@managed-key-id".to_string(),
                Zeroizing::new("private-key-material".to_string()),
            );
        persist_session_to_path(&path, &session).unwrap();

        let secret = get_secret_migrating_legacy_account_at_path(
            &path,
            &mut session,
            "svc",
            "portable:v1:managed-key-id",
            "@managed-key-id",
        )
        .unwrap();

        assert_eq!(secret.as_str(), "private-key-material");
        let restored = load_session_from_path(&path, "secret123").unwrap();
        let accounts = restored.payload.services.get("svc").unwrap();
        assert!(accounts.contains_key("portable:v1:managed-key-id"));
        assert!(!accounts.contains_key("old-user@managed-key-id"));
    }

    #[test]
    fn ambiguous_legacy_accounts_are_not_guessed_or_modified() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let mut session = sample_session("secret123");
        let accounts = session
            .payload
            .services
            .entry("svc".to_string())
            .or_default();
        accounts.insert(
            "alice@credential-id".to_string(),
            Zeroizing::new("alice-secret".to_string()),
        );
        accounts.insert(
            "bob@credential-id".to_string(),
            Zeroizing::new("bob-secret".to_string()),
        );
        persist_session_to_path(&path, &session).unwrap();

        let error = get_secret_migrating_legacy_account_at_path(
            &path,
            &mut session,
            "svc",
            "portable:v1:credential-id",
            "@credential-id",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            PortableKeystoreError::AmbiguousLegacyAccounts
        ));
        let accounts = session.payload.services.get("svc").unwrap();
        assert!(accounts.contains_key("alice@credential-id"));
        assert!(accounts.contains_key("bob@credential-id"));
        assert!(!accounts.contains_key("portable:v1:credential-id"));
    }

    #[test]
    fn canonical_store_replaces_all_legacy_aliases() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let mut session = sample_session("secret123");
        let accounts = session
            .payload
            .services
            .entry("svc".to_string())
            .or_default();
        accounts.insert(
            "alice@credential-id".to_string(),
            Zeroizing::new("old-alice-secret".to_string()),
        );
        accounts.insert(
            "bob@credential-id".to_string(),
            Zeroizing::new("old-bob-secret".to_string()),
        );
        persist_session_to_path(&path, &session).unwrap();

        store_secret_replacing_legacy_accounts_at_path(
            &path,
            &mut session,
            "svc",
            "portable:v1:credential-id",
            "@credential-id",
            "new-secret",
        )
        .unwrap();

        let restored = load_session_from_path(&path, "secret123").unwrap();
        let accounts = restored.payload.services.get("svc").unwrap();
        assert_eq!(
            accounts
                .get("portable:v1:credential-id")
                .map(|secret| secret.as_str()),
            Some("new-secret")
        );
        assert!(!accounts.contains_key("alice@credential-id"));
        assert!(!accounts.contains_key("bob@credential-id"));
    }

    #[test]
    fn canonical_delete_removes_all_legacy_aliases() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PORTABLE_KEYSTORE_FILENAME);
        let mut session = sample_session("secret123");
        let accounts = session
            .payload
            .services
            .entry("svc".to_string())
            .or_default();
        accounts.insert(
            "portable:v1:credential-id".to_string(),
            Zeroizing::new("current-secret".to_string()),
        );
        accounts.insert(
            "old-user@credential-id".to_string(),
            Zeroizing::new("legacy-secret".to_string()),
        );
        persist_session_to_path(&path, &session).unwrap();

        delete_secret_with_legacy_accounts_at_path(
            &path,
            &mut session,
            "svc",
            "portable:v1:credential-id",
            "@credential-id",
        )
        .unwrap();

        let restored = load_session_from_path(&path, "secret123").unwrap();
        assert!(!restored.payload.services.contains_key("svc"));
    }
}
