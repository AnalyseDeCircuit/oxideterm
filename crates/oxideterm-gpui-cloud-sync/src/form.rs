// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync form normalization and secret draft transitions.

use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, secret_keys, secrets::CloudSyncKeychainSecretProvider,
};
use oxideterm_settings_model::CloudSyncFormDraft;

use crate::{cloud_sync_number_string, non_empty_secret};

pub trait CloudSyncSecretWriter {
    fn write_secret(&mut self, key: &str, value: Option<&str>) -> anyhow::Result<()>;
}

impl CloudSyncSecretWriter for CloudSyncKeychainSecretProvider {
    fn write_secret(&mut self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        self.store_secret(key, value)
    }
}

pub fn cloud_sync_settings_from_form(form: &CloudSyncFormDraft) -> (CloudSyncSettings, f64) {
    let interval = form
        .auto_upload_interval_mins
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(60.0);
    let auth_mode = match form.backend_type {
        BackendType::Dropbox => AuthMode::Bearer,
        BackendType::GithubGist | BackendType::Git | BackendType::S3 => AuthMode::None,
        BackendType::Webdav | BackendType::HttpJson => form.auth_mode.clone(),
    };
    let settings = CloudSyncSettings {
        backend_type: form.backend_type.clone(),
        auth_mode,
        endpoint: if matches!(
            form.backend_type,
            BackendType::Dropbox | BackendType::GithubGist
        ) {
            String::new()
        } else {
            form.endpoint.trim().to_string()
        },
        namespace: if matches!(
            form.backend_type,
            BackendType::GithubGist | BackendType::Git | BackendType::S3
        ) {
            form.namespace.trim().to_string()
        } else {
            let namespace = form.namespace.trim();
            if namespace.is_empty() {
                CloudSyncSettings::default().namespace
            } else {
                namespace.to_string()
            }
        },
        s3_bucket: form.s3_bucket.trim().to_string(),
        s3_region: {
            let region = form.s3_region.trim();
            if region.is_empty() {
                CloudSyncSettings::default().s3_region
            } else {
                region.to_string()
            }
        },
        git_repository: form.git_repository.trim().to_string(),
        git_branch: {
            let branch = form.git_branch.trim();
            if branch.is_empty() {
                CloudSyncSettings::default().git_branch
            } else {
                branch.to_string()
            }
        },
        github_oauth_client_id: form.github_oauth_client_id.trim().to_string(),
        auto_upload_enabled: form.auto_upload_enabled,
        auto_upload_interval_mins: interval,
        default_conflict_strategy: form.default_conflict_strategy.clone(),
    };
    (settings, interval)
}

pub fn normalize_cloud_sync_interval_draft(form: &mut CloudSyncFormDraft, interval: f64) {
    form.auto_upload_interval_mins = cloud_sync_number_string(interval);
}

pub fn store_cloud_sync_touched_secrets(
    form: &CloudSyncFormDraft,
    provider: &mut impl CloudSyncSecretWriter,
) -> anyhow::Result<()> {
    if form.token_touched {
        provider.write_secret(secret_keys::TOKEN, non_empty_secret(&form.token))?;
    }
    if form.git_token_touched {
        provider.write_secret(secret_keys::GIT_TOKEN, non_empty_secret(&form.git_token))?;
    }
    if form.basic_username_touched {
        provider.write_secret(
            secret_keys::BASIC_USERNAME,
            non_empty_secret(&form.basic_username),
        )?;
    }
    if form.basic_password_touched {
        provider.write_secret(
            secret_keys::BASIC_PASSWORD,
            non_empty_secret(&form.basic_password),
        )?;
    }
    if form.access_key_id_touched {
        provider.write_secret(
            secret_keys::ACCESS_KEY_ID,
            non_empty_secret(&form.access_key_id),
        )?;
    }
    if form.secret_access_key_touched {
        provider.write_secret(
            secret_keys::SECRET_ACCESS_KEY,
            non_empty_secret(&form.secret_access_key),
        )?;
    }
    if form.session_token_touched {
        provider.write_secret(
            secret_keys::SESSION_TOKEN,
            non_empty_secret(&form.session_token),
        )?;
    }
    if form.sync_password_touched {
        provider.write_secret(
            secret_keys::SYNC_PASSWORD,
            non_empty_secret(&form.sync_password),
        )?;
    }
    Ok(())
}

pub fn reset_cloud_sync_secret_drafts(form: &mut CloudSyncFormDraft) {
    form.token.clear();
    form.git_token.clear();
    form.basic_username.clear();
    form.basic_password.clear();
    form.access_key_id.clear();
    form.secret_access_key.clear();
    form.session_token.clear();
    form.sync_password.clear();
    form.token_touched = false;
    form.git_token_touched = false;
    form.basic_username_touched = false;
    form.basic_password_touched = false;
    form.access_key_id_touched = false;
    form.secret_access_key_touched = false;
    form.session_token_touched = false;
    form.sync_password_touched = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_from_form_normalizes_interval_and_defaults() {
        let mut form = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        form.auto_upload_interval_mins = "bad".to_string();
        form.namespace.clear();
        form.s3_region.clear();

        let (settings, interval) = cloud_sync_settings_from_form(&form);

        assert_eq!(interval, 60.0);
        assert_eq!(settings.namespace, CloudSyncSettings::default().namespace);
        assert_eq!(settings.s3_region, CloudSyncSettings::default().s3_region);
    }

    #[test]
    fn settings_from_form_clears_hidden_gist_endpoint() {
        let mut form = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        form.backend_type = BackendType::GithubGist;
        form.auth_mode = AuthMode::Bearer;
        form.endpoint = "https://dav.example.test".to_string();
        form.git_repository = "abcdef123456".to_string();

        let (settings, _) = cloud_sync_settings_from_form(&form);

        assert_eq!(settings.auth_mode, AuthMode::None);
        assert!(settings.endpoint.is_empty());
        assert_eq!(settings.git_repository, "abcdef123456");
    }

    #[test]
    fn reset_secret_drafts_clears_values_and_touch_flags() {
        let mut form = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        form.token = "token".to_string();
        form.token_touched = true;
        form.sync_password = "password".to_string();
        form.sync_password_touched = true;

        reset_cloud_sync_secret_drafts(&mut form);

        assert!(form.token.is_empty());
        assert!(form.sync_password.is_empty());
        assert!(!form.token_touched);
        assert!(!form.sync_password_touched);
    }
}
