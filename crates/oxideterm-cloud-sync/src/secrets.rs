// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result};
use keyring::Entry;
use oxideterm_portable_runtime::keystore::{self as portable_keystore, PortableKeystoreError};
use zeroize::Zeroizing;

use crate::{AuthMode, BackendType, CLOUD_SYNC_PLUGIN_ID, secret_keys};

const CLOUD_SYNC_KEYCHAIN_SERVICE: &str = "com.oxideterm.ai";
const LEGACY_NATIVE_CLOUD_SYNC_KEYCHAIN_SERVICE: &str = "com.oxideterm.cloud-sync";
static SECRET_SESSION_CACHE: OnceLock<Mutex<BTreeMap<String, Option<CloudSyncSecretValue>>>> =
    OnceLock::new();

pub type CloudSyncSecretValue = Zeroizing<String>;

#[cfg(target_os = "macos")]
mod mac_keychain {
    use std::process::Command;

    use zeroize::{Zeroize, Zeroizing};

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

    pub fn get(service: &str, account: &str) -> Result<Zeroizing<String>, String> {
        let mut output = Command::new("security")
            .args(["find-generic-password", "-s", service, "-a", account, "-w"])
            .output()
            .map_err(|error| format!("security CLI: {error}"))?;

        if output.status.success() {
            // `security -w` materializes the secret in stdout; move it into a
            // zeroizing owner and wipe the command buffer before returning.
            let secret = Zeroizing::new(
                String::from_utf8_lossy(&output.stdout)
                    .trim_end_matches('\n')
                    .to_string(),
            );
            output.stdout.zeroize();
            Ok(secret)
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
    ) -> Result<Option<CloudSyncSecretValue>, CloudSyncSecretError>;

    fn store_secret(
        &mut self,
        _key: &str,
        _value: Option<&str>,
    ) -> Result<(), CloudSyncSecretError> {
        Err(CloudSyncSecretError::AccessFailed(
            "secret provider is read-only".to_string(),
        ))
    }

    fn get_many_secrets(
        &mut self,
        keys: &[&str],
        mode: SecretReadMode,
    ) -> Result<BTreeMap<String, Option<CloudSyncSecretValue>>, CloudSyncSecretError> {
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
            if !portable_keychain_enabled()
                .with_context(|| "failed to determine OxideTerm portable mode")?
            {
                self.delete_legacy_secret(key)?;
            }
            self.hints.insert(key.to_string(), true);
            clear_session_cached_secret(&self.cache_key(key));
        } else {
            self.delete_current_secret(key)?;
            if !portable_keychain_enabled()
                .with_context(|| "failed to determine OxideTerm portable mode")?
            {
                self.delete_legacy_secret(key)?;
            }
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
        if portable_keychain_enabled()
            .with_context(|| "failed to determine OxideTerm portable mode")?
        {
            return portable_keystore::store_secret(&self.service, &account, value).with_context(
                || format!("failed to store cloud sync secret {key} in portable keystore"),
            );
        }

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

    fn get_current_secret(
        &self,
        key: &str,
    ) -> Result<Option<CloudSyncSecretValue>, CloudSyncSecretError> {
        let account = self.account(key);
        if portable_keychain_enabled()
            .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?
        {
            // Portable secrets are available only after the vault is unlocked;
            // prompt-capable callers can surface this as a bootstrap action.
            return match portable_keystore::get_secret(&self.service, &account) {
                Ok(value) => Ok(Some(value)),
                Err(PortableKeystoreError::NotFound(_)) => Ok(None),
                Err(PortableKeystoreError::Locked) => Err(CloudSyncSecretError::UnlockRequired),
                Err(error) => Err(CloudSyncSecretError::AccessFailed(error.to_string())),
            };
        }

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
                let value = Zeroizing::new(value);
                #[cfg(target_os = "macos")]
                {
                    let _ = mac_keychain::store(&self.service, &account, value.as_str());
                }
                Ok(Some(value))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(CloudSyncSecretError::AccessFailed(error.to_string())),
        }
    }

    fn get_legacy_secret(
        &self,
        key: &str,
    ) -> Result<Option<CloudSyncSecretValue>, CloudSyncSecretError> {
        match Entry::new(&self.legacy_service, &self.legacy_account(key))
            .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?
            .get_password()
        {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(CloudSyncSecretError::AccessFailed(error.to_string())),
        }
    }

    fn delete_current_secret(&self, key: &str) -> Result<()> {
        let account = self.account(key);
        if portable_keychain_enabled()
            .with_context(|| "failed to determine OxideTerm portable mode")?
        {
            return portable_keystore::delete_secret(&self.service, &account).with_context(|| {
                format!("failed to delete cloud sync secret {key} from portable keystore")
            });
        }

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
    ) -> Result<Option<CloudSyncSecretValue>, CloudSyncSecretError> {
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

        if portable_keychain_enabled()
            .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?
        {
            self.hints.insert(key.to_string(), false);
            set_session_cached_secret(cache_key, None);
            return Ok(None);
        }

        if let Some(value) = self.get_legacy_secret(key)? {
            self.store_current_secret(key, value.as_str())
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

    fn store_secret(&mut self, key: &str, value: Option<&str>) -> Result<(), CloudSyncSecretError> {
        CloudSyncKeychainSecretProvider::store_secret(self, key, value)
            .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))
    }

    fn get_many_secrets(
        &mut self,
        keys: &[&str],
        mode: SecretReadMode,
    ) -> Result<BTreeMap<String, Option<CloudSyncSecretValue>>, CloudSyncSecretError> {
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

            if portable_keychain_enabled()
                .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?
            {
                self.hints.insert(key.to_string(), false);
                set_session_cached_secret(cache_key, None);
                values.insert(key.to_string(), None);
                continue;
            }

            if let Some(value) = self.get_legacy_secret(key)? {
                self.store_current_secret(key, value.as_str())
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

fn secret_session_cache() -> &'static Mutex<BTreeMap<String, Option<CloudSyncSecretValue>>> {
    SECRET_SESSION_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn session_cached_secret(key: &str) -> Option<Option<CloudSyncSecretValue>> {
    secret_session_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(key).cloned())
}

fn set_session_cached_secret(key: String, value: Option<CloudSyncSecretValue>) {
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

fn portable_keychain_enabled() -> Result<bool, oxideterm_portable_runtime::PortableError> {
    oxideterm_portable_runtime::is_portable_mode()
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct CloudSyncSecrets {
    pub sync_password: Option<CloudSyncSecretValue>,
    pub token: Option<CloudSyncSecretValue>,
    pub git_token: Option<CloudSyncSecretValue>,
    pub microsoft_refresh_token: Option<CloudSyncSecretValue>,
    pub basic_username: Option<CloudSyncSecretValue>,
    pub basic_password: Option<CloudSyncSecretValue>,
    pub access_key_id: Option<CloudSyncSecretValue>,
    pub secret_access_key: Option<CloudSyncSecretValue>,
    pub session_token: Option<CloudSyncSecretValue>,
}

impl std::fmt::Debug for CloudSyncSecrets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CloudSyncSecrets")
            .field(
                "sync_password",
                &self.sync_password.as_ref().map(|_| "[redacted secret]"),
            )
            .field("token", &self.token.as_ref().map(|_| "[redacted secret]"))
            .field(
                "git_token",
                &self.git_token.as_ref().map(|_| "[redacted secret]"),
            )
            .field(
                "microsoft_refresh_token",
                &self
                    .microsoft_refresh_token
                    .as_ref()
                    .map(|_| "[redacted secret]"),
            )
            .field(
                "basic_username",
                &self.basic_username.as_ref().map(|_| "[redacted secret]"),
            )
            .field(
                "basic_password",
                &self.basic_password.as_ref().map(|_| "[redacted secret]"),
            )
            .field(
                "access_key_id",
                &self.access_key_id.as_ref().map(|_| "[redacted secret]"),
            )
            .field(
                "secret_access_key",
                &self.secret_access_key.as_ref().map(|_| "[redacted secret]"),
            )
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "[redacted secret]"),
            )
            .finish()
    }
}

pub fn backend_uses_auth_mode(backend_type: &BackendType) -> bool {
    matches!(backend_type, BackendType::Webdav | BackendType::HttpJson)
}

pub fn backend_uses_token(backend_type: &BackendType, auth_mode: &AuthMode) -> bool {
    matches!(backend_type, BackendType::Dropbox)
        || (backend_uses_auth_mode(backend_type) && matches!(auth_mode, AuthMode::Bearer))
}

pub fn backend_uses_git_token(backend_type: &BackendType) -> bool {
    matches!(backend_type, BackendType::Git | BackendType::GithubGist)
}

pub fn backend_uses_basic(backend_type: &BackendType, auth_mode: &AuthMode) -> bool {
    backend_uses_auth_mode(backend_type) && matches!(auth_mode, AuthMode::Basic)
}

pub fn backend_uses_s3_credentials(backend_type: &BackendType) -> bool {
    matches!(backend_type, BackendType::S3)
}

pub fn backend_uses_microsoft_refresh_token(backend_type: &BackendType) -> bool {
    matches!(backend_type, BackendType::OneDrive)
}

pub fn get_action_secrets(
    settings: &crate::CloudSyncSettings,
    provider: &mut impl CloudSyncSecretProvider,
    include_sync_password: bool,
    mode: SecretReadMode,
) -> Result<CloudSyncSecrets, CloudSyncSecretError> {
    let mut secrets = CloudSyncSecrets::default();
    let mut reads = Vec::<(
        &str,
        fn(&mut CloudSyncSecrets, Option<CloudSyncSecretValue>),
    )>::new();

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
    if backend_uses_microsoft_refresh_token(&settings.backend_type) {
        reads.push((secret_keys::MICROSOFT_REFRESH_TOKEN, |secrets, value| {
            secrets.microsoft_refresh_token = value
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
        secret_keys::MICROSOFT_REFRESH_TOKEN => secrets.microsoft_refresh_token.is_none(),
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
        ) -> Result<Option<CloudSyncSecretValue>, CloudSyncSecretError> {
            self.reads.push((key.to_string(), mode));
            if matches!(mode, SecretReadMode::Silent) {
                return Ok(None);
            }
            Ok(self.values.get(key).cloned().map(Zeroizing::new))
        }

        fn get_many_secrets(
            &mut self,
            keys: &[&str],
            mode: SecretReadMode,
        ) -> Result<BTreeMap<String, Option<CloudSyncSecretValue>>, CloudSyncSecretError> {
            self.batch_reads
                .push((keys.iter().map(|key| (*key).to_string()).collect(), mode));
            let mut values = BTreeMap::new();
            for key in keys {
                values.insert(
                    (*key).to_string(),
                    self.values.get(*key).cloned().map(Zeroizing::new),
                );
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
    fn onedrive_actions_read_refresh_token_instead_of_short_lived_access_token() {
        let mut provider = TestSecrets {
            hints: HashSet::from([
                secret_keys::TOKEN.to_string(),
                secret_keys::MICROSOFT_REFRESH_TOKEN.to_string(),
            ]),
            values: HashMap::from([(
                secret_keys::MICROSOFT_REFRESH_TOKEN.to_string(),
                "refresh".to_string(),
            )]),
            ..TestSecrets::default()
        };
        let settings = CloudSyncSettings {
            backend_type: BackendType::OneDrive,
            ..CloudSyncSettings::default()
        };

        let secrets =
            get_action_secrets(&settings, &mut provider, false, SecretReadMode::Prompt).unwrap();

        assert!(secrets.token.is_none());
        assert_eq!(
            secrets
                .microsoft_refresh_token
                .as_ref()
                .map(|secret| secret.as_str()),
            Some("refresh")
        );
        assert_eq!(
            provider.batch_reads,
            vec![(
                vec![secret_keys::MICROSOFT_REFRESH_TOKEN.to_string()],
                SecretReadMode::Prompt,
            )]
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

        assert_eq!(
            secrets.sync_password.as_ref().map(|secret| secret.as_str()),
            Some("sync")
        );
        assert_eq!(
            secrets
                .basic_username
                .as_ref()
                .map(|secret| secret.as_str()),
            Some("user")
        );
        assert_eq!(
            secrets
                .basic_password
                .as_ref()
                .map(|secret| secret.as_str()),
            Some("pass")
        );
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
        set_session_cached_secret(
            cache_key.clone(),
            Some(Zeroizing::new("cached-token".to_string())),
        );

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

        assert_eq!(
            secrets.token.as_ref().map(|secret| secret.as_str()),
            Some("cached-token")
        );
        clear_session_cached_secret(&cache_key);
    }
}
