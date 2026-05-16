use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use keyring::Entry;
use parking_lot::RwLock;
use zeroize::Zeroizing;

const AI_KEYCHAIN_SERVICE: &str = "com.oxideterm.ai";

#[derive(Clone)]
pub struct AiProviderKeyStore {
    service: String,
    cache: Arc<RwLock<HashMap<String, Zeroizing<String>>>>,
}

impl Default for AiProviderKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AiProviderKeyStore {
    pub fn new() -> Self {
        Self {
            service: AI_KEYCHAIN_SERVICE.to_string(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn store_provider_key(&self, provider_id: &str, api_key: Zeroizing<String>) -> Result<()> {
        if api_key.is_empty() {
            self.delete_provider_key(provider_id)?;
            return Ok(());
        }

        // UI drafts cross into the backend as Zeroizing<String>; keyring copies
        // into the OS keychain, and this clone is the Tauri-equivalent session
        // cache used to avoid repeated reads of the same provider secret.
        self.entry(provider_id)?
            .set_password(api_key.as_str())
            .with_context(|| format!("failed to save AI provider key for {provider_id}"))?;
        self.cache
            .write()
            .insert(provider_id.to_string(), Zeroizing::new(api_key.to_string()));
        Ok(())
    }

    pub fn get_provider_key(&self, provider_id: &str) -> Result<Option<Zeroizing<String>>> {
        if let Some(cached) = self.cache.read().get(provider_id) {
            return Ok(Some(Zeroizing::new(cached.to_string())));
        }

        match self.entry(provider_id)?.get_password() {
            Ok(secret) => {
                let secret = Zeroizing::new(secret);
                self.cache
                    .write()
                    .insert(provider_id.to_string(), Zeroizing::new(secret.to_string()));
                Ok(Some(secret))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("failed to load AI provider key for {provider_id}")),
        }
    }

    pub fn has_provider_key(&self, provider_id: &str) -> bool {
        if self.cache.read().contains_key(provider_id) {
            return true;
        }
        self.entry(provider_id)
            .map(|entry| credential_exists_without_secret_read(&entry))
            .unwrap_or(false)
    }

    pub fn delete_provider_key(&self, provider_id: &str) -> Result<()> {
        self.cache.write().remove(provider_id);
        match self.entry(provider_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("failed to delete AI provider key for {provider_id}")),
        }
    }

    fn entry(&self, provider_id: &str) -> Result<Entry> {
        let account = format!("{}@{}", whoami::username(), provider_id);
        Entry::new(&self.service, &account)
            .with_context(|| format!("failed to open AI keychain entry for {provider_id}"))
    }
}

fn credential_exists_without_secret_read(entry: &Entry) -> bool {
    // Tauri's has_ai_provider_api_key intentionally checks existence without
    // reading the secret, so model selector status probes do not trigger
    // Touch ID/keychain unlock prompts. Use keyring's platform credential
    // lookup when available and fall back to get_password only on stores that
    // do not expose an existence probe.
    platform_credential_exists(entry).unwrap_or_else(|| {
        matches!(
            entry.get_password(),
            Ok(_) | Err(keyring::Error::Ambiguous(_))
        )
    })
}

#[cfg(target_os = "macos")]
fn platform_credential_exists(entry: &Entry) -> Option<bool> {
    entry
        .get_credential()
        .downcast_ref::<keyring::macos::MacCredential>()
        .map(|credential| credential.get_credential().is_ok())
}

#[cfg(target_os = "windows")]
fn platform_credential_exists(entry: &Entry) -> Option<bool> {
    entry
        .get_credential()
        .downcast_ref::<keyring::windows::WinCredential>()
        .map(|credential| credential.get_credential().is_ok())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_credential_exists(_entry: &Entry) -> Option<bool> {
    None
}
