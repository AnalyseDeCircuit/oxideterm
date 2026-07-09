use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex},
};

use anyhow::{Context, Result, anyhow};
use keyring::Entry;
use oxideterm_portable_runtime::keystore::{self as portable_keystore, PortableKeystoreError};
use parking_lot::RwLock;
use zeroize::Zeroizing;

const AI_KEYCHAIN_SERVICE: &str = "com.oxideterm.ai";
const AI_KEYCHAIN_TOUCH_ID_REASON: &str = "OxideTerm needs to access your AI API key";

#[derive(Clone)]
pub struct AiProviderKeyStore {
    service: String,
    cache: Arc<RwLock<HashMap<String, Zeroizing<String>>>>,
    in_flight_reads: Arc<Mutex<HashMap<String, Arc<ProviderKeyReadSlot>>>>,
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
            in_flight_reads: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn store_provider_key(&self, provider_id: &str, api_key: Zeroizing<String>) -> Result<()> {
        if api_key.is_empty() {
            self.delete_provider_key(provider_id)?;
            return Ok(());
        }

        // UI drafts cross into the backend as Zeroizing<String>; the OS keychain
        // receives the secret, and the session cache mirrors Tauri's post-save
        // cache so the next chat send does not immediately re-authenticate.
        self.store_provider_key_to_os(provider_id, api_key.as_str())?;
        self.cache
            .write()
            .insert(provider_id.to_string(), Zeroizing::new(api_key.to_string()));
        Ok(())
    }

    pub fn get_provider_key(&self, provider_id: &str) -> Result<Option<Zeroizing<String>>> {
        if let Some(cached) = self.cache.read().get(provider_id) {
            return Ok(Some(Zeroizing::new(cached.to_string())));
        }

        match self.begin_provider_key_read(provider_id) {
            ProviderKeyReadTicket::Owner { provider_id, slot } => {
                let result = self.load_provider_key_from_os(&provider_id);
                if let Ok(Some(secret)) = result.as_ref() {
                    self.cache
                        .write()
                        .insert(provider_id.clone(), Zeroizing::new(secret.to_string()));
                }
                let shared_result = share_provider_key_read_result(&result);
                slot.finish(shared_result);
                self.finish_provider_key_read(&provider_id);
                result
            }
            ProviderKeyReadTicket::Waiter { slot } => match slot.wait() {
                Ok(secret) => Ok(secret),
                Err(error) => Err(anyhow!(error)),
            },
        }
    }

    pub fn get_provider_keys(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<(String, Zeroizing<String>)>> {
        let mut secrets = Vec::new();
        let mut missing = Vec::new();
        {
            let cache = self.cache.read();
            for provider_id in provider_ids {
                if let Some(cached) = cache.get(provider_id) {
                    secrets.push((provider_id.clone(), Zeroizing::new(cached.to_string())));
                } else {
                    missing.push(provider_id.clone());
                }
            }
        }

        if missing.is_empty() {
            return Ok(secrets);
        }

        if portable_keychain_enabled()? {
            for provider_id in missing {
                if let Some(secret) = self.get_provider_key(&provider_id)? {
                    secrets.push((provider_id, secret));
                }
            }
            return Ok(secrets);
        }

        #[cfg(target_os = "macos")]
        {
            if crate::touch_id::is_biometric_available() {
                crate::touch_id::authenticate(AI_KEYCHAIN_TOUCH_ID_REASON)
                    .map_err(anyhow::Error::msg)
                    .context("failed to authenticate AI provider key export")?;
            }

            for provider_id in missing {
                if let Some(secret) = self.load_provider_key_from_macos_after_auth(&provider_id)? {
                    self.cache
                        .write()
                        .insert(provider_id.clone(), Zeroizing::new(secret.to_string()));
                    secrets.push((provider_id, secret));
                }
            }
            return Ok(secrets);
        }

        #[cfg(not(target_os = "macos"))]
        {
            for provider_id in missing {
                if let Some(secret) = self.get_provider_key(&provider_id)? {
                    secrets.push((provider_id, secret));
                }
            }
            Ok(secrets)
        }
    }

    fn begin_provider_key_read(&self, provider_id: &str) -> ProviderKeyReadTicket {
        let mut reads = self
            .in_flight_reads
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(slot) = reads.get(provider_id) {
            return ProviderKeyReadTicket::Waiter { slot: slot.clone() };
        }

        let slot = Arc::new(ProviderKeyReadSlot::default());
        reads.insert(provider_id.to_string(), slot.clone());
        ProviderKeyReadTicket::Owner {
            provider_id: provider_id.to_string(),
            slot,
        }
    }

    fn finish_provider_key_read(&self, provider_id: &str) {
        self.in_flight_reads
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(provider_id);
    }

    pub fn has_provider_key(&self, provider_id: &str) -> bool {
        if self.cache.read().contains_key(provider_id) {
            return true;
        }
        match portable_keychain_enabled() {
            Ok(true) => {
                return portable_keystore::secret_exists(&self.service, &self.account(provider_id))
                    .unwrap_or(false);
            }
            Ok(false) => {}
            Err(_) => return false,
        }
        self.entry(provider_id)
            .map(|entry| credential_exists_without_secret_read(&entry))
            .unwrap_or(false)
    }

    pub fn delete_provider_key(&self, provider_id: &str) -> Result<()> {
        self.cache.write().remove(provider_id);
        if portable_keychain_enabled()? {
            return portable_keystore::delete_secret(&self.service, &self.account(provider_id))
                .with_context(|| {
                    format!(
                        "failed to delete AI provider key from portable keystore for {provider_id}"
                    )
                });
        }
        match self.entry(provider_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("failed to delete AI provider key for {provider_id}")),
        }
    }

    pub fn store_acp_auth_token(&self, agent_id: &str, token: Zeroizing<String>) -> Result<()> {
        // ACP tokens share the AI keychain service but use their own namespace
        // so provider, MCP, and ACP credentials cannot overwrite each other.
        self.store_provider_key(&format!("acp:{agent_id}"), token)
    }

    pub fn has_acp_auth_token(&self, agent_id: &str) -> bool {
        self.has_provider_key(&format!("acp:{agent_id}"))
    }

    pub fn delete_acp_auth_token(&self, agent_id: &str) -> Result<()> {
        self.delete_provider_key(&format!("acp:{agent_id}"))
    }

    fn load_provider_key_from_os(&self, provider_id: &str) -> Result<Option<Zeroizing<String>>> {
        if portable_keychain_enabled()? {
            return self.load_provider_key_from_portable(provider_id);
        }

        #[cfg(target_os = "macos")]
        {
            if crate::touch_id::is_biometric_available() {
                crate::touch_id::authenticate(AI_KEYCHAIN_TOUCH_ID_REASON)
                    .map_err(anyhow::Error::msg)
                    .with_context(|| {
                        format!("failed to authenticate AI provider key access for {provider_id}")
                    })?;
            }

            self.load_provider_key_from_macos_after_auth(provider_id)
        }

        #[cfg(not(target_os = "macos"))]
        {
            match self.entry(provider_id)?.get_password() {
                Ok(secret) => Ok(Some(Zeroizing::new(secret))),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(error) => Err(error)
                    .with_context(|| format!("failed to load AI provider key for {provider_id}")),
            }
        }
    }

    fn load_provider_key_from_portable(
        &self,
        provider_id: &str,
    ) -> Result<Option<Zeroizing<String>>> {
        // Portable mode must not touch the host OS keychain. Secrets live in
        // the unlocked portable vault so a copied app directory remains usable.
        match portable_keystore::get_secret(&self.service, &self.account(provider_id)) {
            Ok(secret) => Ok(Some(secret)),
            Err(PortableKeystoreError::NotFound(_)) => Ok(None),
            Err(error) => Err(error).with_context(|| {
                format!("failed to load AI provider key from portable keystore for {provider_id}")
            }),
        }
    }

    #[cfg(target_os = "macos")]
    fn load_provider_key_from_macos_after_auth(
        &self,
        provider_id: &str,
    ) -> Result<Option<Zeroizing<String>>> {
        match self.entry(provider_id)?.get_password() {
            // Move the backend-owned String directly into a zeroizing owner.
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("failed to load AI provider key for {provider_id}")),
        }
    }

    fn store_provider_key_to_os(&self, provider_id: &str, api_key: &str) -> Result<()> {
        if portable_keychain_enabled()? {
            return portable_keystore::store_secret(
                &self.service,
                &self.account(provider_id),
                api_key,
            )
            .with_context(|| {
                format!("failed to save AI provider key to portable keystore for {provider_id}")
            });
        }

        // keyring's apple-native backend keeps the secret out of process argv.
        self.entry(provider_id)?
            .set_password(api_key)
            .with_context(|| format!("failed to save AI provider key for {provider_id}"))
    }

    fn account(&self, provider_id: &str) -> String {
        format!("{}@{}", whoami::username(), provider_id)
    }

    fn entry(&self, provider_id: &str) -> Result<Entry> {
        Entry::new(&self.service, &self.account(provider_id))
            .with_context(|| format!("failed to open AI keychain entry for {provider_id}"))
    }
}

type ProviderKeyReadResult = std::result::Result<Option<Zeroizing<String>>, String>;

enum ProviderKeyReadTicket {
    Owner {
        provider_id: String,
        slot: Arc<ProviderKeyReadSlot>,
    },
    Waiter {
        slot: Arc<ProviderKeyReadSlot>,
    },
}

#[derive(Default)]
struct ProviderKeyReadSlot {
    result: Mutex<Option<ProviderKeyReadResult>>,
    cvar: Condvar,
}

impl ProviderKeyReadSlot {
    fn finish(&self, result: ProviderKeyReadResult) {
        let mut slot = self
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *slot = Some(result);
        self.cvar.notify_all();
    }

    fn wait(&self) -> ProviderKeyReadResult {
        let mut result = self
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(result) = result.as_ref() {
                return clone_provider_key_read_result(result);
            }
            result = self
                .cvar
                .wait(result)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}

fn share_provider_key_read_result(
    result: &Result<Option<Zeroizing<String>>>,
) -> ProviderKeyReadResult {
    result
        .as_ref()
        .map(|secret| {
            secret
                .as_ref()
                .map(|secret| Zeroizing::new(secret.to_string()))
        })
        .map_err(|error| error.to_string())
}

fn clone_provider_key_read_result(result: &ProviderKeyReadResult) -> ProviderKeyReadResult {
    result
        .as_ref()
        .map(|secret| {
            secret
                .as_ref()
                .map(|secret| Zeroizing::new(secret.to_string()))
        })
        .map_err(Clone::clone)
}

fn credential_exists_without_secret_read(entry: &Entry) -> bool {
    // Tauri's has_ai_provider_api_key intentionally checks existence without
    // reading the secret, so model selector status probes do not trigger
    // Touch ID/keychain unlock prompts. Use keyring's platform credential
    // lookup when available and fall back to get_password only on stores that
    // do not expose an existence probe.
    platform_credential_exists(entry).unwrap_or_else(|| match entry.get_password() {
        Ok(secret) => {
            // Some backends do not expose metadata-only lookup. Move their
            // temporary String into a zeroizing owner immediately.
            let _secret = Zeroizing::new(secret);
            true
        }
        Err(keyring::Error::Ambiguous(_)) => true,
        Err(_) => false,
    })
}

#[cfg(target_os = "macos")]
fn platform_credential_exists(entry: &Entry) -> Option<bool> {
    use security_framework::item::{ItemClass, ItemSearchOptions};

    let credential = entry
        .get_credential()
        .downcast_ref::<keyring::macos::MacCredential>()?;
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(&credential.service)
        .account(&credential.account)
        .load_attributes(true)
        .limit(1)
        .skip_authenticated_items(true);
    Some(search.search().is_ok_and(|items| !items.is_empty()))
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

fn portable_keychain_enabled() -> Result<bool> {
    oxideterm_portable_runtime::is_portable_mode()
        .context("failed to determine OxideTerm portable mode")
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn provider_key_reads_are_singleflight_per_provider() {
        let store = AiProviderKeyStore::new();
        let first_read = store.begin_provider_key_read("provider-1");
        let ProviderKeyReadTicket::Owner { provider_id, slot } = first_read else {
            panic!("first reader owns provider");
        };
        let started = Arc::new(AtomicBool::new(false));
        let acquired = Arc::new(AtomicBool::new(false));

        let waiter_store = store.clone();
        let waiter_started = started.clone();
        let waiter_acquired = acquired.clone();
        let waiter = thread::spawn(move || {
            waiter_started.store(true, Ordering::SeqCst);
            let ProviderKeyReadTicket::Waiter { slot } =
                waiter_store.begin_provider_key_read("provider-1")
            else {
                panic!("second reader should share first read");
            };
            assert_eq!(slot.wait().unwrap(), None);
            waiter_acquired.store(true, Ordering::SeqCst);
        });

        while !started.load(Ordering::SeqCst) {
            thread::yield_now();
        }
        thread::sleep(Duration::from_millis(25));
        assert!(!acquired.load(Ordering::SeqCst));

        slot.finish(Ok(None));
        store.finish_provider_key_read(&provider_id);
        waiter.join().expect("waiter thread");
        assert!(acquired.load(Ordering::SeqCst));
    }

    #[test]
    fn native_key_store_does_not_spawn_the_macos_security_cli() {
        // This source-level invariant avoids touching the real keychain while
        // preventing secrets from being reintroduced into child-process argv.
        let source = include_str!("key_store.rs");
        for keychain_command in [
            ["add-generic", "-password"].concat(),
            ["find-generic", "-password"].concat(),
            ["delete-generic", "-password"].concat(),
        ] {
            assert!(!source.contains(&keychain_command));
        }
    }
}
