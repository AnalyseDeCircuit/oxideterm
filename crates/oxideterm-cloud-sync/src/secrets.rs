// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use keyring::Entry;

use crate::{AuthMode, BackendType, secret_keys};

const CLOUD_SYNC_KEYCHAIN_SERVICE: &str = "com.oxideterm.cloud-sync";

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
}

#[derive(Clone, Debug)]
pub struct CloudSyncKeychainSecretProvider {
    service: String,
    hints: BTreeMap<String, bool>,
}

impl CloudSyncKeychainSecretProvider {
    pub fn new(hints: BTreeMap<String, bool>) -> Self {
        Self {
            service: CLOUD_SYNC_KEYCHAIN_SERVICE.to_string(),
            hints,
        }
    }

    pub fn store_secret(&mut self, key: &str, value: Option<&str>) -> Result<()> {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            Entry::new(&self.service, &self.account(key))?
                .set_password(value)
                .with_context(|| format!("failed to store cloud sync secret {key}"))?;
            self.hints.insert(key.to_string(), true);
        } else {
            let _ = Entry::new(&self.service, &self.account(key))?.delete_credential();
            self.hints.insert(key.to_string(), false);
        }
        Ok(())
    }

    pub fn hints(&self) -> &BTreeMap<String, bool> {
        &self.hints
    }

    fn account(&self, key: &str) -> String {
        format!("{}@{}", whoami::username(), key)
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
        if matches!(mode, SecretReadMode::Silent) {
            return Ok(None);
        }
        match Entry::new(&self.service, &self.account(key))
            .map_err(|error| CloudSyncSecretError::AccessFailed(error.to_string()))?
            .get_password()
        {
            Ok(value) => {
                self.hints.insert(key.to_string(), true);
                Ok(Some(value))
            }
            Err(keyring::Error::NoEntry) => {
                self.hints.insert(key.to_string(), false);
                Ok(None)
            }
            Err(error) => Err(CloudSyncSecretError::AccessFailed(error.to_string())),
        }
    }
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

    for (key, assign) in &reads {
        assign(&mut secrets, provider.get_secret(key, mode)?);
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
        assert_eq!(
            provider
                .reads
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            vec![
                secret_keys::SYNC_PASSWORD,
                secret_keys::BASIC_USERNAME,
                secret_keys::BASIC_PASSWORD
            ]
        );
    }
}
