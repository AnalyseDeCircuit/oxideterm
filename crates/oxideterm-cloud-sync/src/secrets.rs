// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result};
use keyring::Entry;

use crate::{AuthMode, BackendType, CLOUD_SYNC_PLUGIN_ID, secret_keys};

const CLOUD_SYNC_KEYCHAIN_SERVICE: &str = "com.oxideterm.ai";
const LEGACY_NATIVE_CLOUD_SYNC_KEYCHAIN_SERVICE: &str = "com.oxideterm.cloud-sync";
static SECRET_SESSION_CACHE: OnceLock<Mutex<BTreeMap<String, Option<String>>>> = OnceLock::new();

#[cfg(target_os = "macos")]
mod mac_keychain {
    use std::process::Command;

    pub fn store(service: &str, account: &str, password: &str) -> Result<(), String> {
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", service, "-a", account])
            .output();

        let output = Command::new("security")
            .args([
                "add-generic-password",
                "-s",
                service,
                "-a",
                account,
                "-w",
                password,
                "-A",
            ])
            .output()
            .map_err(|error| format!("security CLI: {error}"))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("security add-generic-password: {}", stderr.trim()))
        }
    }

    pub fn get(service: &str, account: &str) -> Result<String, String> {
        let output = Command::new("security")
            .args(["find-generic-password", "-s", service, "-a", account, "-w"])
            .output()
            .map_err(|error| format!("security CLI: {error}"))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end_matches('\n')
                .to_string())
        } else {
            Err("not found".to_string())
        }
    }

    pub fn delete(service: &str, account: &str) {
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", service, "-a", account])
            .output();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretReadMode {
    Prompt,
    Silent,
}

#[derive(Debug, thiserror::Error)]
pub enum CloudSyncSecretError {
    #[error("secret unlock required")]
    UnlockRequired,
    #[error("secret access cancelled")]
    AccessCancelled,
    #[error("secret access failed: {0}")]
    AccessFailed(String),
}

pub trait CloudSyncSecretProvider {
    fn has_hint(&self, key: &str) -> bool;
    fn get_secret(
        &mut self,
        key: &str,
        mode: SecretReadMode,
    ) -> Result<Option<String>, CloudSyncSecretError>;

    fn get_many_secrets(
        &mut self,
        keys: &[&str],
        mode: SecretReadMode,
    ) -> Result<BTreeMap<String, Option<String>>, CloudSyncSecretError> {
        let mut values = BTreeMap::new();
        for key in keys {
            values.insert((*key).to_string(), self.get_secret(key, mode)?);
        }
        Ok(values)
    }
}

#[derive(Clone, Debug)]
pub struct CloudSyncKeychainSecretProvider {
    service: String,
    legacy_service: String,
    hints: BTreeMap<String, bool>,
}

impl CloudSyncKeychainSecretProvider {
    pub fn new(hints: BTreeMap<String, bool>) -> Self {
        Self {
            service: CLOUD_SYNC_KEYCHAIN_SERVICE.to_string(),
            legacy_service: LEGACY_NATIVE_CLOUD_SYNC_KEYCHAIN_SERVICE.to_string(),
            hints,
        }
    }

    pub fn store_secret(&mut self, key: &str, value: Option<&str>) -> Result<()> {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            self.store_current_secret(key, value)?;
            self.delete_legacy_secret(key)?;
            self.hints.insert(key.to_string(), true);
            clear_session_cached_secret(&self.cache_key(key));
        } else {
            self.delete_current_secret(key)?;
            self.delete_legacy_secret(key)?;
            self.hints.insert(key.to_string(), false);
            clear_session_cached_secret(&self.cache_key(key));
        }
        Ok(())
    }

    pub fn hints(&self) -> &BTreeMap<String, bool> {
        &self.hints
    }

    fn account(&self, key: &str) -> String {
        format!("{}@{}", whoami::username(), plugin_secret_account_id(key))
    }

    fn cache_key(&self, key: &str) -> String {
        format!("{}:{}", self.service, self.account(key))
    }

    fn legacy_account(&self, key: &str) -> String {
        format!("{}@{}", whoami::username(), key)
    }

    fn store_current_secret(&self, key: &str, value: &str) -> Result<()> {
        let account = self.account(key);
        #[cfg(target_os = "macos")]
        {
            mac_keychain::store(&self.service, &account, value).map_err(|error| {
                anyhow::anyhow!("failed to store cloud sync secret {key}: {error}")
            })?;
            return Ok(());
        }

        #[cfg(not(target_os = "macos"))]
        {
            Entry::new(&self.service, &account)?
                .set_password(value)
                .with_context(|| format!("failed to store cloud sync secret {key}"))
        }
    }

    fn get_current_secret(&self, key: &str) -> Result<Option<String>, CloudSyncSecretError> {
        let account = self.account(key);
        #[cfg(target_os = "macos")]
        {
            if let Ok(value) = mac_keychain::get(&self.service, &account) {
                return Ok(Some(value));
            }
        }

        match Entry::new(&self.service, &account)
            .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?
            .get_password()
        {
            Ok(value) => {
                #[cfg(target_os = "macos")]
                {
                    let _ = mac_keychain::store(&self.service, &account, &value);
                }
                Ok(Some(value))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(CloudSyncSecretError::AccessFailed(error.to_string())),
        }
    }

    fn get_legacy_secret(&self, key: &str) -> Result<Option<String>, CloudSyncSecretError> {
        match Entry::new(&self.legacy_service, &self.legacy_account(key))
            .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?
            .get_password()
        {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(CloudSyncSecretError::AccessFailed(error.to_string())),
        }
    }

    fn delete_current_secret(&self, key: &str) -> Result<()> {
        let account = self.account(key);
        #[cfg(target_os = "macos")]
        {
            mac_keychain::delete(&self.service, &account);
        }
        match Entry::new(&self.service, &account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("failed to delete cloud sync secret {key}"))
            }
        }
    }

    fn delete_legacy_secret(&self, key: &str) -> Result<()> {
        match Entry::new(&self.legacy_service, &self.legacy_account(key))?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("failed to delete legacy cloud sync secret {key}")),
        }
    }
}

impl CloudSyncSecretProvider for CloudSyncKeychainSecretProvider {
    fn has_hint(&self, key: &str) -> bool {
        self.hints.get(key).copied().unwrap_or(false)
    }

    fn get_secret(
        &mut self,
        key: &str,
        mode: SecretReadMode,
    ) -> Result<Option<String>, CloudSyncSecretError> {
        let cache_key = self.cache_key(key);
        if let Some(value) = session_cached_secret(&cache_key) {
            self.hints.insert(key.to_string(), value.is_some());
            return Ok(value);
        }

        if matches!(mode, SecretReadMode::Silent) {
            return Ok(None);
        }
        if let Some(value) = self.get_current_secret(key)? {
            self.hints.insert(key.to_string(), true);
            set_session_cached_secret(cache_key, Some(value.clone()));
            return Ok(Some(value));
        }

        if let Some(value) = self.get_legacy_secret(key)? {
            self.store_current_secret(key, &value)
                .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?;
            self.delete_legacy_secret(key)
                .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?;
            self.hints.insert(key.to_string(), true);
            set_session_cached_secret(cache_key, Some(value.clone()));
            return Ok(Some(value));
        }

        self.hints.insert(key.to_string(), false);
        set_session_cached_secret(cache_key, None);
        Ok(None)
    }

    fn get_many_secrets(
        &mut self,
        keys: &[&str],
        mode: SecretReadMode,
    ) -> Result<BTreeMap<String, Option<String>>, CloudSyncSecretError> {
        let mut values = BTreeMap::new();
        let mut missing = Vec::new();

        for key in keys {
            let cache_key = self.cache_key(key);
            if let Some(value) = session_cached_secret(&cache_key) {
                self.hints.insert((*key).to_string(), value.is_some());
                values.insert((*key).to_string(), value);
            } else {
                missing.push(*key);
            }
        }

        if matches!(mode, SecretReadMode::Silent) {
            for key in missing {
                values.insert(key.to_string(), None);
            }
            return Ok(values);
        }

        for key in missing {
            let cache_key = self.cache_key(key);
            if let Some(value) = self.get_current_secret(key)? {
                self.hints.insert(key.to_string(), true);
                set_session_cached_secret(cache_key, Some(value.clone()));
                values.insert(key.to_string(), Some(value));
                continue;
            }

            if let Some(value) = self.get_legacy_secret(key)? {
                self.store_current_secret(key, &value)
                    .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?;
                self.delete_legacy_secret(key)
                    .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?;
                self.hints.insert(key.to_string(), true);
                set_session_cached_secret(cache_key, Some(value.clone()));
                values.insert(key.to_string(), Some(value));
                continue;
            }

            self.hints.insert(key.to_string(), false);
            set_session_cached_secret(cache_key, None);
            values.insert(key.to_string(), None);
        }

        Ok(values)
    }
}

fn secret_session_cache() -> &'static Mutex<BTreeMap<String, Option<String>>> {
    SECRET_SESSION_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn session_cached_secret(key: &str) -> Option<Option<String>> {
    secret_session_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(key).cloned())
}

fn set_session_cached_secret(key: String, value: Option<String>) {
    if let Ok(mut cache) = secret_session_cache().lock() {
        cache.insert(key, value);
    }
}

fn clear_session_cached_secret(key: &str) {
    if let Ok(mut cache) = secret_session_cache().lock() {
        cache.remove(key);
    }
}

fn plugin_secret_account_id(key: &str) -> String {
    format!(
        "plugin-secret:{}:{}:{}:{}",
        CLOUD_SYNC_PLUGIN_ID.len(),
        CLOUD_SYNC_PLUGIN_ID,
        key.len(),
        key
    )
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CloudSyncSecrets {
    pub sync_password: Option<String>,
    pub token: Option<String>,
    pub git_token: Option<String>,
    pub basic_username: Option<String>,
    pub basic_password: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub session_token: Option<String>,
}

pub fn backend_uses_auth_mode(backend_type: &BackendType) -> bool {
    matches!(backend_type, BackendType::Webdav | BackendType::HttpJson)
}

pub fn backend_uses_token(backend_type: &BackendType, auth_mode: &AuthMode) -> bool {
    matches!(backend_type, BackendType::Dropbox)
        || (backend_uses_auth_mode(backend_type) && matches!(auth_mode, AuthMode::Bearer))
}

pub fn backend_uses_git_token(backend_type: &BackendType) -> bool {
    matches!(backend_type, BackendType::Git)
}

pub fn backend_uses_basic(backend_type: &BackendType, auth_mode: &AuthMode) -> bool {
    backend_uses_auth_mode(backend_type) && matches!(auth_mode, AuthMode::Basic)
}

pub fn backend_uses_s3_credentials(backend_type: &BackendType) -> bool {
    matches!(backend_type, BackendType::S3)
}

pub fn get_action_secrets(
    settings: &crate::CloudSyncSettings,
    provider: &mut impl CloudSyncSecretProvider,
    include_sync_password: bool,
    mode: SecretReadMode,
) -> Result<CloudSyncSecrets, CloudSyncSecretError> {
    let mut secrets = CloudSyncSecrets::default();
    let mut reads = Vec::<(&str, fn(&mut CloudSyncSecrets, Option<String>))>::new();

    if include_sync_password {
        reads.push((secret_keys::SYNC_PASSWORD, |secrets, value| {
            secrets.sync_password = value
        }));
    }
    if backend_uses_token(&settings.backend_type, &settings.auth_mode) {
        reads.push((secret_keys::TOKEN, |secrets, value| secrets.token = value));
    }
    if backend_uses_git_token(&settings.backend_type) {
        reads.push((secret_keys::GIT_TOKEN, |secrets, value| {
            secrets.git_token = value
        }));
    }
    if backend_uses_basic(&settings.backend_type, &settings.auth_mode) {
        reads.push((secret_keys::BASIC_USERNAME, |secrets, value| {
            secrets.basic_username = value
        }));
        reads.push((secret_keys::BASIC_PASSWORD, |secrets, value| {
            secrets.basic_password = value
        }));
    }
    if backend_uses_s3_credentials(&settings.backend_type) {
        reads.push((secret_keys::ACCESS_KEY_ID, |secrets, value| {
            secrets.access_key_id = value
        }));
        reads.push((secret_keys::SECRET_ACCESS_KEY, |secrets, value| {
            secrets.secret_access_key = value
        }));
        reads.push((secret_keys::SESSION_TOKEN, |secrets, value| {
            secrets.session_token = value
        }));
    }

    if matches!(mode, SecretReadMode::Prompt) && !reads.is_empty() {
        let keys = reads.iter().map(|(key, _)| *key).collect::<Vec<_>>();
        let values = provider.get_many_secrets(&keys, mode)?;
        for (key, assign) in &reads {
            assign(&mut secrets, values.get(*key).cloned().unwrap_or(None));
        }
    } else {
        for (key, assign) in &reads {
            assign(&mut secrets, provider.get_secret(key, mode)?);
        }
    }

    if matches!(mode, SecretReadMode::Silent)
        && reads
            .iter()
            .any(|(key, _)| provider.has_hint(key) && secret_missing(key, &secrets))
    {
        return Err(CloudSyncSecretError::UnlockRequired);
    }

    Ok(secrets)
}

fn secret_missing(key: &str, secrets: &CloudSyncSecrets) -> bool {
    match key {
        secret_keys::SYNC_PASSWORD => secrets.sync_password.is_none(),
        secret_keys::TOKEN => secrets.token.is_none(),
        secret_keys::GIT_TOKEN => secrets.git_token.is_none(),
        secret_keys::BASIC_USERNAME => secrets.basic_username.is_none(),
        secret_keys::BASIC_PASSWORD => secrets.basic_password.is_none(),
        secret_keys::ACCESS_KEY_ID => secrets.access_key_id.is_none(),
        secret_keys::SECRET_ACCESS_KEY => secrets.secret_access_key.is_none(),
        secret_keys::SESSION_TOKEN => secrets.session_token.is_none(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::{AuthMode, CloudSyncSettings};

    #[derive(Default)]
    struct TestSecrets {
        hints: HashSet<String>,
        values: HashMap<String, String>,
        reads: Vec<(String, SecretReadMode)>,
        batch_reads: Vec<(Vec<String>, SecretReadMode)>,
    }

    impl CloudSyncSecretProvider for TestSecrets {
        fn has_hint(&self, key: &str) -> bool {
            self.hints.contains(key)
        }

        fn get_secret(
            &mut self,
            key: &str,
            mode: SecretReadMode,
        ) -> Result<Option<String>, CloudSyncSecretError> {
            self.reads.push((key.to_string(), mode));
            if matches!(mode, SecretReadMode::Silent) {
                return Ok(None);
            }
            Ok(self.values.get(key).cloned())
        }

        fn get_many_secrets(
            &mut self,
            keys: &[&str],
            mode: SecretReadMode,
        ) -> Result<BTreeMap<String, Option<String>>, CloudSyncSecretError> {
            self.batch_reads
                .push((keys.iter().map(|key| (*key).to_string()).collect(), mode));
            let mut values = BTreeMap::new();
            for key in keys {
                values.insert((*key).to_string(), self.values.get(*key).cloned());
            }
            Ok(values)
        }
    }

    #[test]
    fn plugin_secret_account_id_matches_tauri_namespace() {
        assert_eq!(
            plugin_secret_account_id(secret_keys::BASIC_PASSWORD),
            "plugin-secret:24:com.oxideterm.cloud-sync:14:basic-password"
        );
    }

    #[test]
    fn silent_read_reports_unlock_required_without_prompting_value() {
        let mut provider = TestSecrets {
            hints: HashSet::from([secret_keys::TOKEN.to_string()]),
            ..TestSecrets::default()
        };
        let settings = CloudSyncSettings {
            auth_mode: AuthMode::Bearer,
            ..CloudSyncSettings::default()
        };

        let error = get_action_secrets(&settings, &mut provider, false, SecretReadMode::Silent)
            .unwrap_err();

        assert!(matches!(error, CloudSyncSecretError::UnlockRequired));
        assert_eq!(
            provider.reads,
            vec![(secret_keys::TOKEN.to_string(), SecretReadMode::Silent)]
        );
    }

    #[test]
    fn prompt_read_batches_expected_backend_and_sync_secrets_contract() {
        let cache_provider =
            CloudSyncKeychainSecretProvider::new(std::collections::BTreeMap::new());
        clear_session_cached_secret(&cache_provider.cache_key(secret_keys::SYNC_PASSWORD));
        clear_session_cached_secret(&cache_provider.cache_key(secret_keys::BASIC_USERNAME));
        clear_session_cached_secret(&cache_provider.cache_key(secret_keys::BASIC_PASSWORD));

        let mut provider = TestSecrets {
            values: HashMap::from([
                (secret_keys::SYNC_PASSWORD.to_string(), "sync".to_string()),
                (secret_keys::BASIC_USERNAME.to_string(), "user".to_string()),
                (secret_keys::BASIC_PASSWORD.to_string(), "pass".to_string()),
            ]),
            ..TestSecrets::default()
        };
        let settings = CloudSyncSettings {
            auth_mode: AuthMode::Basic,
            ..CloudSyncSettings::default()
        };

        let secrets =
            get_action_secrets(&settings, &mut provider, true, SecretReadMode::Prompt).unwrap();

        assert_eq!(secrets.sync_password.as_deref(), Some("sync"));
        assert_eq!(secrets.basic_username.as_deref(), Some("user"));
        assert_eq!(secrets.basic_password.as_deref(), Some("pass"));
        assert!(provider.reads.is_empty());
        assert_eq!(
            provider.batch_reads,
            vec![(
                vec![
                    secret_keys::SYNC_PASSWORD.to_string(),
                    secret_keys::BASIC_USERNAME.to_string(),
                    secret_keys::BASIC_PASSWORD.to_string(),
                ],
                SecretReadMode::Prompt,
            )]
        );
    }

    #[test]
    fn prompt_read_populates_session_cache_used_by_silent_reads_like_tauri() {
        let mut keychain_provider =
            CloudSyncKeychainSecretProvider::new(std::collections::BTreeMap::new());
        let cache_key = keychain_provider.cache_key(secret_keys::TOKEN);
        clear_session_cached_secret(&cache_key);
        set_session_cached_secret(cache_key.clone(), Some("cached-token".to_string()));

        let settings = CloudSyncSettings {
            auth_mode: AuthMode::Bearer,
            ..CloudSyncSettings::default()
        };
        let secrets = get_action_secrets(
            &settings,
            &mut keychain_provider,
            false,
            SecretReadMode::Silent,
        )
        .unwrap();

        assert_eq!(secrets.token.as_deref(), Some("cached-token"));
        clear_session_cached_secret(&cache_key);
    }
}
