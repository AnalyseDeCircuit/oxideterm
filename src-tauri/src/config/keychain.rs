//! Keychain Integration
//!
//! Securely stores passwords and passphrases in the system keychain.
//! Uses the `keyring` crate for cross-platform keychain access.

use keyring::Entry;
use uuid::Uuid;

/// Service name for keychain entries
const SERVICE_NAME: &str = "com.oxideterm.ssh";

/// Keychain errors
#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    #[error("Keychain error: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("Secret not found for ID: {0}")]
    NotFound(String),
}

/// Keychain manager for storing SSH credentials
pub struct Keychain {
    service: String,
}

impl Keychain {
    /// Create a new keychain manager
    pub fn new() -> Self {
        Self {
            service: SERVICE_NAME.to_string(),
        }
    }

    /// Create with custom service name (for testing)
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    /// Generate a new unique keychain ID
    pub fn generate_id() -> String {
        format!("oxideterm-{}", Uuid::new_v4())
    }

    /// Store a secret in the keychain
    /// Returns the keychain ID used
    pub fn store(&self, id: &str, secret: &str) -> Result<(), KeychainError> {
        tracing::info!("Keychain store: service={}, id={}", self.service, id);
        // Use explicit username to ensure stable keychain identity on macOS
        let username = whoami::username();
        let entry = Entry::new(&self.service, &format!("{}@{}", username, id))?;
        match entry.set_password(secret) {
            Ok(()) => {
                tracing::info!("Keychain store called successfully, verifying...");
                // Verify the store actually worked by reading it back
                match entry.get_password() {
                    Ok(read_back) => {
                        if read_back == secret {
                            tracing::info!("Keychain store verified: id={}", id);
                            Ok(())
                        } else {
                            tracing::error!("Keychain store verification failed: content mismatch");
                            Err(KeychainError::Keyring(keyring::Error::NoEntry))
                        }
                    }
                    Err(e) => {
                        tracing::error!("Keychain store verification failed: {:?}", e);
                        Err(KeychainError::Keyring(e))
                    }
                }
            }
            Err(e) => {
                tracing::error!("Keychain store failed: id={}, error={:?}", id, e);
                Err(KeychainError::Keyring(e))
            }
        }
    }

    /// Store a new secret and return its generated ID
    pub fn store_new(&self, secret: &str) -> Result<String, KeychainError> {
        let id = Self::generate_id();
        self.store(&id, secret)?;
        Ok(id)
    }

    /// Retrieve a secret from the keychain
    pub fn get(&self, id: &str) -> Result<String, KeychainError> {
        tracing::info!("Keychain get: service={}, id={}", self.service, id);
        // Use same username-prefixed account as store()
        let username = whoami::username();
        let entry = Entry::new(&self.service, &format!("{}@{}", username, id))?;
        match entry.get_password() {
            Ok(secret) => {
                tracing::info!("Keychain get success: id={}, len={}", id, secret.len());
                Ok(secret)
            }
            Err(keyring::Error::NoEntry) => {
                tracing::warn!("Keychain get: no entry for id={}", id);
                Err(KeychainError::NotFound(id.to_string()))
            }
            Err(e) => {
                tracing::error!("Keychain get failed: id={}, error={:?}", id, e);
                Err(KeychainError::Keyring(e))
            }
        }
    }

    /// Delete a secret from the keychain
    pub fn delete(&self, id: &str) -> Result<(), KeychainError> {
        // Use same username-prefixed account
        let username = whoami::username();
        let entry = Entry::new(&self.service, &format!("{}@{}", username, id))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
            Err(e) => Err(KeychainError::Keyring(e)),
        }
    }

    /// Check if a secret exists
    pub fn exists(&self, id: &str) -> Result<bool, KeychainError> {
        // Use same username-prefixed account as store()/get()/delete()
        let username = whoami::username();
        let entry = Entry::new(&self.service, &format!("{}@{}", username, id))?;
        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(KeychainError::Keyring(e)),
        }
    }

    /// Update an existing secret
    pub fn update(&self, id: &str, new_secret: &str) -> Result<(), KeychainError> {
        // keyring will overwrite existing entry
        self.store(id, new_secret)
    }
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create a keychain entry label
pub fn make_label(host: &str, username: &str) -> String {
    format!("OxideTerm: {}@{}", username, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests interact with the real system keychain
    // They use a unique service name to avoid conflicts

    #[test]
    #[ignore] // Run manually: cargo test keychain -- --ignored
    fn test_keychain_operations() {
        let keychain = Keychain::with_service("com.oxideterm.test");
        let id = Keychain::generate_id();

        // Store
        keychain.store(&id, "test-secret").unwrap();

        // Get
        let secret = keychain.get(&id).unwrap();
        assert_eq!(secret, "test-secret");

        // Exists
        assert!(keychain.exists(&id).unwrap());

        // Update
        keychain.update(&id, "new-secret").unwrap();
        let secret = keychain.get(&id).unwrap();
        assert_eq!(secret, "new-secret");

        // Delete
        keychain.delete(&id).unwrap();
        assert!(!keychain.exists(&id).unwrap());
    }

    #[test]
    fn test_generate_id() {
        let id1 = Keychain::generate_id();
        let id2 = Keychain::generate_id();

        assert!(id1.starts_with("oxideterm-"));
        assert!(id2.starts_with("oxideterm-"));
        assert_ne!(id1, id2);
    }
}
