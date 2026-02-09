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
        let key_path = expand_tilde(key_path.as_ref());

        if !key_path.exists() {
            return Err(KeyError::NotFound(key_path));
        }

        debug!("Loading key from: {:?}", key_path);
        let key_pair = load_private_key(&key_path, passphrase)?;

        Ok(Self { key_path, key_pair })
    }

    /// Try to load key from default locations
    pub fn from_default_locations(passphrase: Option<&str>) -> Result<Self, KeyError> {
        let default_keys = default_key_paths();

        for path in default_keys {
            if path.exists() {
                debug!("Trying default key: {:?}", path);
                match load_private_key(&path, passphrase) {
                    Ok(key_pair) => {
                        info!("Loaded key from: {:?}", path);
                        return Ok(Self {
                            key_path: path,
                            key_pair,
                        });
                    }
                    Err(KeyError::PassphraseRequired) => {
                        // Key exists but is encrypted, propagate error
                        return Err(KeyError::PassphraseRequired);
                    }
                    Err(e) => {
                        debug!("Failed to load {:?}: {}", path, e);
                        continue;
                    }
                }
            }
        }

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
        Some(pass) => russh::keys::decode_secret_key(&key_data, Some(pass)).map_err(|e| {
            if e.to_string().contains("decrypt") || e.to_string().contains("password") {
                KeyError::InvalidPassphrase
            } else {
                KeyError::ParseError(e.to_string())
            }
        }),
        None => russh::keys::decode_secret_key(&key_data, None)
            .map_err(|e| KeyError::ParseError(e.to_string())),
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

/// Expand ~ to home directory
fn expand_tilde(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();

    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if path_str == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }

    path.to_path_buf()
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

    #[test]
    fn test_expand_tilde() {
        let path = expand_tilde(Path::new("~/.ssh/id_rsa"));
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
}
