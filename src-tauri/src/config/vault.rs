//! AI Vault - Legacy Local Encrypted File Storage (DEPRECATED)
//!
//! ⚠️ **DEPRECATED since v1.6.0**: AI API keys are now stored in OS keychain
//! (`com.oxideterm.ai` service via `keyring` crate). This module is retained
//! solely for **one-time migration** of existing vault files to keychain.
//!
//! This code will be removed in a future version once all users have migrated.
//!
//! Previous approach:
//! - XOR encryption was used for obfuscation, not cryptographic security
//! - Machine fingerprint (hostname + username) was fully predictable
//! - Per-provider keys stored in `ai_keys/{provider_id}.vault`

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

/// Legacy vault file name (single key)
const VAULT_FILENAME: &str = "ai.vault";

/// Directory for per-provider vault files
const PROVIDER_KEYS_DIR: &str = "ai_keys";

/// Magic header to identify vault files
const VAULT_MAGIC: &[u8; 8] = b"OXVAULT1";

/// Vault errors
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Vault file not found")]
    NotFound,

    #[error("Invalid vault file format")]
    InvalidFormat,

    #[error("Failed to get app data directory")]
    NoAppDataDir,

    #[error("UTF-8 decode error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Get machine fingerprint for XOR key derivation
/// Uses hostname + username combination as a stable machine identifier
fn get_machine_fingerprint() -> String {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown-host".to_string());
    let username = whoami::username();

    // Combine hostname and username for a unique machine identifier
    format!("oxideterm-vault-{}-{}", hostname, username)
}

/// Derive XOR key from machine fingerprint
/// Expands the fingerprint to create a repeating key pattern
fn derive_xor_key(fingerprint: &str, length: usize) -> Vec<u8> {
    let fp_bytes = fingerprint.as_bytes();
    let mut key = Vec::with_capacity(length);

    // Create a longer key by hashing the fingerprint multiple times
    let mut hash_input = fp_bytes.to_vec();
    while key.len() < length {
        // Simple mixing: rotate and XOR with position
        for (i, &b) in hash_input.iter().enumerate() {
            if key.len() >= length {
                break;
            }
            // Mix byte with its position to create variation
            let mixed = b.wrapping_add(i as u8).wrapping_mul(31);
            key.push(mixed);
        }
        // Rotate the hash input for next iteration
        if !hash_input.is_empty() {
            let first = hash_input.remove(0);
            hash_input.push(first.wrapping_add(1));
        }
    }

    key
}

/// XOR encrypt/decrypt data
fn xor_cipher(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % key.len()])
        .collect()
}

/// AI Vault manager for storing API keys locally
pub struct AiVault {
    vault_path: PathBuf,
    fingerprint: String,
}

impl AiVault {
    /// Create a new vault manager using the app data directory
    pub fn new(app_data_dir: PathBuf) -> Self {
        let vault_path = app_data_dir.join(VAULT_FILENAME);
        let fingerprint = get_machine_fingerprint();

        tracing::debug!("AiVault initialized: path={:?}", vault_path);

        Self {
            vault_path,
            fingerprint,
        }
    }

    /// Check if the vault file exists
    pub fn exists(&self) -> bool {
        self.vault_path.exists()
    }

    /// Save a key to the vault
    pub fn save(&self, api_key: &str) -> Result<(), VaultError> {
        tracing::info!("Saving API key to vault (length: {})", api_key.len());

        // Ensure parent directory exists
        if let Some(parent) = self.vault_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let plaintext = api_key.as_bytes();
        let xor_key = derive_xor_key(&self.fingerprint, plaintext.len());
        let encrypted = xor_cipher(plaintext, &xor_key);

        // Build vault file: MAGIC + length (4 bytes) + encrypted data
        let mut file_data = Vec::with_capacity(VAULT_MAGIC.len() + 4 + encrypted.len());
        file_data.extend_from_slice(VAULT_MAGIC);
        file_data.extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
        file_data.extend_from_slice(&encrypted);

        // Write atomically: write to temp file then rename
        let temp_path = self.vault_path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(&file_data)?;
        file.sync_all()?;
        drop(file);

        fs::rename(&temp_path, &self.vault_path)?;

        tracing::info!("API key saved to vault successfully");
        Ok(())
    }

    /// Load a key from the vault
    pub fn load(&self) -> Result<String, VaultError> {
        tracing::info!("Loading API key from vault");

        if !self.vault_path.exists() {
            tracing::debug!("Vault file not found");
            return Err(VaultError::NotFound);
        }

        let mut file = fs::File::open(&self.vault_path)?;
        let mut file_data = Vec::new();
        file.read_to_end(&mut file_data)?;

        // Verify magic header
        if file_data.len() < VAULT_MAGIC.len() + 4 {
            tracing::error!("Vault file too short");
            return Err(VaultError::InvalidFormat);
        }

        if &file_data[..VAULT_MAGIC.len()] != VAULT_MAGIC {
            tracing::error!("Invalid vault magic header");
            return Err(VaultError::InvalidFormat);
        }

        // Read length
        let len_bytes: [u8; 4] = file_data[VAULT_MAGIC.len()..VAULT_MAGIC.len() + 4]
            .try_into()
            .map_err(|_| VaultError::InvalidFormat)?;
        let data_len = u32::from_le_bytes(len_bytes) as usize;

        // Read encrypted data
        let data_start = VAULT_MAGIC.len() + 4;
        if file_data.len() < data_start + data_len {
            tracing::error!("Vault file data truncated");
            return Err(VaultError::InvalidFormat);
        }

        let encrypted = &file_data[data_start..data_start + data_len];
        let xor_key = derive_xor_key(&self.fingerprint, encrypted.len());
        let decrypted = xor_cipher(encrypted, &xor_key);

        let api_key = String::from_utf8(decrypted)?;

        tracing::info!("API key loaded from vault (length: {})", api_key.len());
        Ok(api_key)
    }

    /// Delete the vault file
    pub fn delete(&self) -> Result<(), VaultError> {
        tracing::info!("Deleting vault file");

        if self.vault_path.exists() {
            fs::remove_file(&self.vault_path)?;
            tracing::info!("Vault file deleted");
        } else {
            tracing::debug!("Vault file did not exist");
        }

        Ok(())
    }
}

// ═════════════════════��═════════════════════════════════════════════════════
// Multi-Provider Vault Support
// ═══════════════════════════════════════════════════════════════════════════

/// AI Provider Vault manager for storing per-provider API keys
///
/// Keys are stored in `{app_data_dir}/ai_keys/{provider_id}.vault`
pub struct AiProviderVault {
    keys_dir: PathBuf,
    fingerprint: String,
}

impl AiProviderVault {
    /// Create a new provider vault manager using the app data directory
    pub fn new(app_data_dir: PathBuf) -> Self {
        let keys_dir = app_data_dir.join(PROVIDER_KEYS_DIR);
        let fingerprint = get_machine_fingerprint();

        tracing::debug!("AiProviderVault initialized: dir={:?}", keys_dir);

        Self {
            keys_dir,
            fingerprint,
        }
    }

    /// Get the vault file path for a specific provider
    fn vault_path(&self, provider_id: &str) -> PathBuf {
        // Sanitize provider_id to prevent path traversal
        let safe_id = provider_id
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>();
        self.keys_dir.join(format!("{}.vault", safe_id))
    }

    /// Check if a key exists for a specific provider
    pub fn exists(&self, provider_id: &str) -> bool {
        self.vault_path(provider_id).exists()
    }

    /// Save a key for a specific provider
    pub fn save(&self, provider_id: &str, api_key: &str) -> Result<(), VaultError> {
        tracing::info!(
            "Saving API key for provider {} (length: {})",
            provider_id,
            api_key.len()
        );

        // Ensure keys directory exists
        fs::create_dir_all(&self.keys_dir)?;

        let vault_path = self.vault_path(provider_id);
        let plaintext = api_key.as_bytes();
        let xor_key = derive_xor_key(&self.fingerprint, plaintext.len());
        let encrypted = xor_cipher(plaintext, &xor_key);

        // Build vault file: MAGIC + length (4 bytes) + encrypted data
        let mut file_data = Vec::with_capacity(VAULT_MAGIC.len() + 4 + encrypted.len());
        file_data.extend_from_slice(VAULT_MAGIC);
        file_data.extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
        file_data.extend_from_slice(&encrypted);

        // Write atomically: write to temp file then rename
        let temp_path = vault_path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(&file_data)?;
        file.sync_all()?;
        drop(file);

        fs::rename(&temp_path, &vault_path)?;

        // Set file permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            let _ = fs::set_permissions(&vault_path, perms);
        }

        tracing::info!("API key saved for provider {} successfully", provider_id);
        Ok(())
    }

    /// Load a key for a specific provider
    pub fn load(&self, provider_id: &str) -> Result<String, VaultError> {
        tracing::debug!("Loading API key for provider {}", provider_id);

        let vault_path = self.vault_path(provider_id);

        if !vault_path.exists() {
            tracing::debug!("Vault file not found for provider {}", provider_id);
            return Err(VaultError::NotFound);
        }

        let mut file = fs::File::open(&vault_path)?;
        let mut file_data = Vec::new();
        file.read_to_end(&mut file_data)?;

        // Verify magic header
        if file_data.len() < VAULT_MAGIC.len() + 4 {
            tracing::error!("Vault file too short for provider {}", provider_id);
            return Err(VaultError::InvalidFormat);
        }

        if &file_data[..VAULT_MAGIC.len()] != VAULT_MAGIC {
            tracing::error!("Invalid vault magic header for provider {}", provider_id);
            return Err(VaultError::InvalidFormat);
        }

        // Read length
        let len_bytes: [u8; 4] = file_data[VAULT_MAGIC.len()..VAULT_MAGIC.len() + 4]
            .try_into()
            .map_err(|_| VaultError::InvalidFormat)?;
        let data_len = u32::from_le_bytes(len_bytes) as usize;

        // Read encrypted data
        let data_start = VAULT_MAGIC.len() + 4;
        if file_data.len() < data_start + data_len {
            tracing::error!("Vault file data truncated for provider {}", provider_id);
            return Err(VaultError::InvalidFormat);
        }

        let encrypted = &file_data[data_start..data_start + data_len];
        let xor_key = derive_xor_key(&self.fingerprint, encrypted.len());
        let decrypted = xor_cipher(encrypted, &xor_key);

        let api_key = String::from_utf8(decrypted)?;

        tracing::debug!(
            "API key loaded for provider {} (length: {})",
            provider_id,
            api_key.len()
        );
        Ok(api_key)
    }

    /// Delete the key for a specific provider
    pub fn delete(&self, provider_id: &str) -> Result<(), VaultError> {
        tracing::info!("Deleting vault file for provider {}", provider_id);

        let vault_path = self.vault_path(provider_id);

        if vault_path.exists() {
            fs::remove_file(&vault_path)?;
            tracing::info!("Vault file deleted for provider {}", provider_id);
        } else {
            tracing::debug!("Vault file did not exist for provider {}", provider_id);
        }

        Ok(())
    }

    /// List all provider IDs that have stored keys
    pub fn list_providers(&self) -> Result<Vec<String>, VaultError> {
        if !self.keys_dir.exists() {
            return Ok(Vec::new());
        }

        let mut providers = Vec::new();
        for entry in fs::read_dir(&self.keys_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "vault") {
                if let Some(stem) = path.file_stem() {
                    providers.push(stem.to_string_lossy().to_string());
                }
            }
        }

        Ok(providers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_xor_roundtrip() {
        let plaintext = b"test-api-key-12345";
        let key = derive_xor_key("test-fingerprint", plaintext.len());

        let encrypted = xor_cipher(plaintext, &key);
        assert_ne!(&encrypted, plaintext);

        let decrypted = xor_cipher(&encrypted, &key);
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn test_vault_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let vault = AiVault::new(temp_dir.path().to_path_buf());

        let api_key = "sk-test-key-1234567890";
        vault.save(api_key).unwrap();

        assert!(vault.exists());

        let loaded = vault.load().unwrap();
        assert_eq!(loaded, api_key);
    }

    #[test]
    fn test_vault_delete() {
        let temp_dir = TempDir::new().unwrap();
        let vault = AiVault::new(temp_dir.path().to_path_buf());

        vault.save("test-key").unwrap();
        assert!(vault.exists());

        vault.delete().unwrap();
        assert!(!vault.exists());
    }

    #[test]
    fn test_vault_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let vault = AiVault::new(temp_dir.path().to_path_buf());

        let result = vault.load();
        assert!(matches!(result, Err(VaultError::NotFound)));
    }
}
