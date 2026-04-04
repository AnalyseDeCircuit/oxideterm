// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH Key Authentication Module
//!
//! Handles loading and parsing SSH private keys:
//! - RSA keys (id_rsa)
//! - Ed25519 keys (id_ed25519)
//! - ECDSA keys (id_ecdsa)
//! - Encrypted keys with passphrase

use russh::keys::PrivateKey as KeyPair;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, info};

/// Key authentication helper
#[derive(Debug)]
pub struct KeyAuth {
    /// Path to the private key
    pub key_path: PathBuf,
    /// Parsed key pair
    pub key_pair: KeyPair,
}

/// Errors that can occur during key loading
#[derive(Debug, Error)]
pub enum KeyError {
    #[error("Key file not found: {0}")]
    NotFound(PathBuf),

    #[error("Failed to read key file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse key: {0}")]
    ParseError(String),

    #[error("Encrypted key requires passphrase")]
    PassphraseRequired,

    #[error("Invalid passphrase")]
    InvalidPassphrase,

    #[error("Unsupported key type")]
    UnsupportedKeyType,
}

impl KeyAuth {
    /// Create a new KeyAuth from a key path
    pub fn new(key_path: impl AsRef<Path>, passphrase: Option<&str>) -> Result<Self, KeyError> {
        let key_path = crate::path_utils::expand_tilde_path(key_path.as_ref());

        if !key_path.exists() {
            return Err(KeyError::NotFound(key_path));
        }

        debug!("Loading key from: {:?}", key_path);
        let key_pair = load_private_key(&key_path, passphrase)?;

        Ok(Self { key_path, key_pair })
    }

    /// Try to load key from default locations
    pub fn from_default_locations(passphrase: Option<&str>) -> Result<Self, KeyError> {
        load_first_available_key(default_key_paths(), passphrase)
    }
}

fn load_first_available_key(
    paths: impl IntoIterator<Item = PathBuf>,
    passphrase: Option<&str>,
) -> Result<KeyAuth, KeyError> {
    let mut saw_encrypted_key = false;

    for path in paths {
        if path.exists() {
            debug!("Trying default key: {:?}", path);
            match load_private_key(&path, passphrase) {
                Ok(key_pair) => {
                    info!("Loaded key from: {:?}", path);
                    return Ok(KeyAuth {
                        key_path: path,
                        key_pair,
                    });
                }
                Err(KeyError::PassphraseRequired) => {
                    saw_encrypted_key = true;
                    debug!(
                        "Key {:?} requires a passphrase, trying next candidate",
                        path
                    );
                }
                Err(e) => {
                    debug!("Failed to load {:?}: {}", path, e);
                }
            }
        }
    }

    if saw_encrypted_key {
        Err(KeyError::PassphraseRequired)
    } else {
        Err(KeyError::NotFound(PathBuf::from("~/.ssh/id_*")))
    }
}

/// Load a private key from file (async version - preferred in async contexts)
pub async fn load_private_key_async(
    path: &Path,
    passphrase: Option<&str>,
) -> Result<KeyPair, KeyError> {
    let path = path.to_path_buf();
    let passphrase = passphrase.map(|s| s.to_string());

    tokio::task::spawn_blocking(move || load_private_key_sync(&path, passphrase.as_deref()))
        .await
        .map_err(|e| KeyError::ParseError(format!("Task join error: {}", e)))?
}

/// Load a private key from file (sync version - use spawn_blocking in async contexts)
pub fn load_private_key(path: &Path, passphrase: Option<&str>) -> Result<KeyPair, KeyError> {
    load_private_key_sync(path, passphrase)
}

fn map_key_decode_error(err: russh::keys::Error, missing_passphrase: bool) -> KeyError {
    match &err {
        russh::keys::Error::KeyIsEncrypted
        | russh::keys::Error::SshKey(russh::keys::ssh_key::Error::Encrypted) => {
            return if missing_passphrase {
                KeyError::PassphraseRequired
            } else {
                KeyError::InvalidPassphrase
            };
        }
        russh::keys::Error::Pad(_)
        | russh::keys::Error::Unpad(_)
        | russh::keys::Error::SshKey(russh::keys::ssh_key::Error::Crypto) => {
            return if missing_passphrase {
                KeyError::PassphraseRequired
            } else {
                KeyError::InvalidPassphrase
            };
        }
        _ => {}
    }

    let message = err.to_string();
    let normalized = message.to_ascii_lowercase();
    let passphrase_related = normalized.contains("decrypt")
        || normalized.contains("password")
        || normalized.contains("passphrase")
        || normalized.contains("encrypted")
        || normalized.contains("bcrypt")
        || normalized.contains("kdf");

    if passphrase_related {
        if missing_passphrase {
            KeyError::PassphraseRequired
        } else {
            KeyError::InvalidPassphrase
        }
    } else {
        KeyError::ParseError(message)
    }
}

/// Internal sync implementation
fn load_private_key_sync(path: &Path, passphrase: Option<&str>) -> Result<KeyPair, KeyError> {
    let key_data = std::fs::read_to_string(path)?;

    // Check if key is encrypted
    let is_encrypted =
        key_data.contains("ENCRYPTED") || key_data.contains("Proc-Type: 4,ENCRYPTED");

    if is_encrypted && passphrase.is_none() {
        return Err(KeyError::PassphraseRequired);
    }

    // Try to decode the key
    match passphrase {
        Some(pass) => russh::keys::decode_secret_key(&key_data, Some(pass))
            .map_err(|e| map_key_decode_error(e, false)),
        None => russh::keys::decode_secret_key(&key_data, None)
            .map_err(|e| map_key_decode_error(e, true)),
    }
}

/// Get default SSH key paths
pub fn default_key_paths() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let ssh_dir = home.join(".ssh");

    vec![
        ssh_dir.join("id_ed25519"), // Prefer Ed25519 (modern, fast)
        ssh_dir.join("id_ecdsa"),   // Then ECDSA
        ssh_dir.join("id_rsa"),     // Then RSA (legacy but common)
    ]
}

/// Check if any default keys exist
pub fn has_default_keys() -> bool {
    default_key_paths().iter().any(|p| p.exists())
}

/// List available default keys
pub fn list_available_keys() -> Vec<PathBuf> {
    default_key_paths()
        .into_iter()
        .filter(|p| p.exists())
        .collect()
}

/// Get key type description
pub fn describe_key(key: &KeyPair) -> String {
    key.algorithm().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use russh::keys::ssh_key::LineEnding;
    use russh::keys::{Algorithm, PrivateKey};
    use tempfile::tempdir;

    fn write_test_key(path: &Path, passphrase: Option<&str>) {
        let mut rng = OsRng;
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap();
        let key = match passphrase {
            Some(pass) => key.encrypt(&mut rng, pass).unwrap(),
            None => key,
        };

        key.write_openssh_file(path, LineEnding::LF).unwrap();
    }

    #[test]
    fn test_expand_tilde() {
        let path = crate::path_utils::expand_tilde_path(Path::new("~/.ssh/id_rsa"));
        assert!(!path.to_string_lossy().starts_with("~"));
    }

    #[test]
    fn test_default_key_paths() {
        let paths = default_key_paths();
        assert!(paths.len() >= 3);

        for path in &paths {
            let path_str = path.to_string_lossy();
            assert!(path_str.contains(".ssh"));
        }
    }

    #[test]
    fn test_load_private_key_round_trips_generated_key() {
        let temp_dir = tempdir().unwrap();
        let key_path = temp_dir.path().join("id_ed25519");
        write_test_key(&key_path, None);

        let key = load_private_key(&key_path, None).unwrap();

        assert!(describe_key(&key).contains("ed25519"));
    }

    #[tokio::test]
    async fn test_load_private_key_async_round_trips_generated_key() {
        let temp_dir = tempdir().unwrap();
        let key_path = temp_dir.path().join("id_ed25519_async");
        write_test_key(&key_path, None);

        let key = load_private_key_async(&key_path, None).await.unwrap();

        assert!(describe_key(&key).contains("ed25519"));
    }

    #[test]
    fn test_load_private_key_requires_passphrase_for_encrypted_key() {
        let temp_dir = tempdir().unwrap();
        let key_path = temp_dir.path().join("id_ed25519_encrypted");
        write_test_key(&key_path, Some("secret-pass"));

        let error = load_private_key(&key_path, None).unwrap_err();

        assert!(matches!(error, KeyError::PassphraseRequired));
    }

    #[test]
    fn test_load_private_key_rejects_invalid_passphrase() {
        let temp_dir = tempdir().unwrap();
        let key_path = temp_dir.path().join("id_ed25519_wrong_pass");
        write_test_key(&key_path, Some("secret-pass"));

        let error = load_private_key(&key_path, Some("wrong-pass")).unwrap_err();

        assert!(matches!(error, KeyError::InvalidPassphrase));
    }

    #[test]
    fn test_load_private_key_accepts_correct_passphrase() {
        let temp_dir = tempdir().unwrap();
        let key_path = temp_dir.path().join("id_ed25519_correct_pass");
        write_test_key(&key_path, Some("secret-pass"));

        let key = load_private_key(&key_path, Some("secret-pass")).unwrap();

        assert!(describe_key(&key).contains("ed25519"));
    }

    #[test]
    fn test_load_private_key_invalid_content_returns_parse_error() {
        let temp_dir = tempdir().unwrap();
        let key_path = temp_dir.path().join("not_a_key");
        std::fs::write(&key_path, "definitely not an ssh key").unwrap();

        let error = load_private_key(&key_path, None).unwrap_err();

        assert!(matches!(error, KeyError::ParseError(_)));
    }

    #[test]
    fn test_key_auth_new_loads_generated_key() {
        let temp_dir = tempdir().unwrap();
        let key_path = temp_dir.path().join("id_ed25519_key_auth");
        write_test_key(&key_path, None);

        let key_auth = KeyAuth::new(&key_path, None).unwrap();

        assert_eq!(key_auth.key_path, key_path);
        assert!(describe_key(&key_auth.key_pair).contains("ed25519"));
    }

    #[test]
    fn test_key_auth_new_returns_not_found_for_missing_path() {
        let error = KeyAuth::new("/definitely/missing/key", None).unwrap_err();

        assert!(matches!(error, KeyError::NotFound(_)));
    }

    #[test]
    fn test_load_first_available_key_falls_back_after_encrypted_candidate() {
        let temp_dir = tempdir().unwrap();
        let encrypted_path = temp_dir.path().join("id_ed25519");
        let fallback_path = temp_dir.path().join("id_rsa");
        write_test_key(&encrypted_path, Some("secret-pass"));
        write_test_key(&fallback_path, None);

        let key_auth =
            load_first_available_key(vec![encrypted_path.clone(), fallback_path.clone()], None)
                .unwrap();

        assert_eq!(key_auth.key_path, fallback_path);
    }

    #[test]
    fn test_load_first_available_key_returns_passphrase_required_if_all_candidates_are_encrypted() {
        let temp_dir = tempdir().unwrap();
        let encrypted_path = temp_dir.path().join("id_ed25519");
        write_test_key(&encrypted_path, Some("secret-pass"));

        let error = load_first_available_key(vec![encrypted_path], None).unwrap_err();

        assert!(matches!(error, KeyError::PassphraseRequired));
    }
}
