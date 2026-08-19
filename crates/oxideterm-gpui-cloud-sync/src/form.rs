// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync form normalization and secret draft transitions.

use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, ConflictStrategy, OXIDE_APP_SETTINGS_SECTION_IDS,
    RawSyncScope, get_syncable_plugin_ids, secret_keys, secrets::CloudSyncKeychainSecretProvider,
};
use oxideterm_settings_model::{CloudSyncFormDraft, CloudSyncSecretDraftHandoff};

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
        BackendType::GithubGist
        | BackendType::Git
        | BackendType::OneDrive
        | BackendType::GoogleDrive
        | BackendType::S3 => AuthMode::None,
        BackendType::Webdav | BackendType::HttpJson => form.auth_mode.clone(),
    };
    let settings = CloudSyncSettings {
        backend_type: form.backend_type.clone(),
        auth_mode,
        endpoint: if matches!(
            form.backend_type,
            BackendType::Dropbox
                | BackendType::GithubGist
                | BackendType::OneDrive
                | BackendType::GoogleDrive
        ) {
            String::new()
        } else {
            form.endpoint.trim().to_string()
        },
        namespace: if matches!(
            form.backend_type,
            BackendType::GithubGist
                | BackendType::Git
                | BackendType::OneDrive
                | BackendType::GoogleDrive
                | BackendType::S3
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
        microsoft_oauth_client_id: form.microsoft_oauth_client_id.trim().to_string(),
        google_oauth_client_id: form.google_oauth_client_id.trim().to_string(),
        auto_upload_enabled: form.auto_upload_enabled,
        auto_upload_interval_mins: interval,
        default_conflict_strategy: form.default_conflict_strategy.clone(),
    };
    (settings, interval)
}

pub fn apply_cloud_sync_configuration_patch(
    current_settings: &CloudSyncSettings,
    current_scope: &RawSyncScope,
    arguments: &serde_json::Value,
) -> Result<(CloudSyncSettings, RawSyncScope, Vec<String>), String> {
    let object = arguments
        .as_object()
        .ok_or_else(|| "Cloud Sync configuration arguments must be an object.".to_string())?;
    let mut settings = current_settings.clone();
    let mut scope = current_scope.clone();
    let mut updated_fields = Vec::with_capacity(object.len());

    if let Some(value) = object
        .get("backend_type")
        .and_then(serde_json::Value::as_str)
    {
        settings.backend_type = cloud_sync_backend_from_id(value)?;
        updated_fields.push("backend_type".to_string());
    }
    if let Some(value) = object.get("auth_mode").and_then(serde_json::Value::as_str) {
        settings.auth_mode = cloud_sync_auth_mode_from_id(value)?;
        updated_fields.push("auth_mode".to_string());
    }
    for (field, destination) in [
        ("endpoint", &mut settings.endpoint),
        ("namespace", &mut settings.namespace),
        ("s3_bucket", &mut settings.s3_bucket),
        ("s3_region", &mut settings.s3_region),
        ("git_repository", &mut settings.git_repository),
        ("git_branch", &mut settings.git_branch),
        (
            "github_oauth_client_id",
            &mut settings.github_oauth_client_id,
        ),
        (
            "microsoft_oauth_client_id",
            &mut settings.microsoft_oauth_client_id,
        ),
        (
            "google_oauth_client_id",
            &mut settings.google_oauth_client_id,
        ),
    ] {
        if let Some(value) = object.get(field).and_then(serde_json::Value::as_str) {
            if matches!(field, "endpoint" | "git_repository") && location_contains_userinfo(value) {
                // Endpoint credentials belong in protected storage, never persisted configuration.
                return Err("Cloud Sync locations cannot contain embedded credentials.".to_string());
            }
            *destination = value.to_string();
            updated_fields.push(field.to_string());
        }
    }
    if let Some(value) = object
        .get("auto_upload_enabled")
        .and_then(serde_json::Value::as_bool)
    {
        settings.auto_upload_enabled = value;
        updated_fields.push("auto_upload_enabled".to_string());
    }
    if let Some(value) = object
        .get("auto_upload_interval_mins")
        .and_then(serde_json::Value::as_f64)
    {
        settings.auto_upload_interval_mins = value;
        updated_fields.push("auto_upload_interval_mins".to_string());
    }
    if let Some(value) = object
        .get("default_conflict_strategy")
        .and_then(serde_json::Value::as_str)
    {
        settings.default_conflict_strategy = cloud_sync_conflict_strategy_from_id(value)?;
        updated_fields.push("default_conflict_strategy".to_string());
    }
    apply_cloud_sync_scope_patch(&mut scope, object.get("scope"), &mut updated_fields)?;

    let form = CloudSyncFormDraft::from_settings(&settings);
    settings = cloud_sync_settings_from_form(&form).0;
    Ok((settings, scope, updated_fields))
}

fn cloud_sync_backend_from_id(value: &str) -> Result<BackendType, String> {
    match value {
        "webdav" => Ok(BackendType::Webdav),
        "http-json" => Ok(BackendType::HttpJson),
        "dropbox" => Ok(BackendType::Dropbox),
        "one-drive" => Ok(BackendType::OneDrive),
        "google-drive" => Ok(BackendType::GoogleDrive),
        "github-gist" => Ok(BackendType::GithubGist),
        "s3" => Ok(BackendType::S3),
        "git" => Ok(BackendType::Git),
        _ => Err("Unsupported Cloud Sync backend type.".to_string()),
    }
}

fn cloud_sync_auth_mode_from_id(value: &str) -> Result<AuthMode, String> {
    match value {
        "bearer" => Ok(AuthMode::Bearer),
        "basic" => Ok(AuthMode::Basic),
        "none" => Ok(AuthMode::None),
        _ => Err("Unsupported Cloud Sync authentication mode.".to_string()),
    }
}

fn cloud_sync_conflict_strategy_from_id(value: &str) -> Result<ConflictStrategy, String> {
    match value {
        "merge" => Ok(ConflictStrategy::Merge),
        "replace" => Ok(ConflictStrategy::Replace),
        "skip" => Ok(ConflictStrategy::Skip),
        "rename" => Ok(ConflictStrategy::Rename),
        _ => Err("Unsupported Cloud Sync conflict strategy.".to_string()),
    }
}

fn location_contains_userinfo(value: &str) -> bool {
    url::Url::parse(value)
        .map(|location| !location.username().is_empty() || location.password().is_some())
        .unwrap_or(false)
}

fn apply_cloud_sync_scope_patch(
    scope: &mut RawSyncScope,
    value: Option<&serde_json::Value>,
    updated_fields: &mut Vec<String>,
) -> Result<(), String> {
    let Some(scope_object) = value.and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    macro_rules! apply_scope_bool {
        ($field:literal, $member:ident) => {
            if let Some(value) = scope_object
                .get($field)
                .and_then(serde_json::Value::as_bool)
            {
                scope.$member = Some(value);
                updated_fields.push(format!("scope.{}", $field));
            }
        };
    }
    apply_scope_bool!("sync_connections", sync_connections);
    apply_scope_bool!("sync_forwards", sync_forwards);
    apply_scope_bool!("sync_quick_commands", sync_quick_commands);
    apply_scope_bool!("sync_serial_profiles", sync_serial_profiles);
    apply_scope_bool!("sync_telnet_profiles", sync_telnet_profiles);
    apply_scope_bool!("sync_mosh_profiles", sync_mosh_profiles);
    apply_scope_bool!("sync_remote_desktop_profiles", sync_remote_desktop_profiles);
    apply_scope_bool!("sync_sensitive_credentials", sync_sensitive_credentials);
    apply_scope_bool!("sync_app_settings", sync_app_settings);
    apply_scope_bool!(
        "include_local_terminal_env_vars",
        include_local_terminal_env_vars
    );
    apply_scope_bool!("sync_plugin_settings", sync_plugin_settings);

    if let Some(values) = scope_object
        .get("app_settings_sections")
        .and_then(serde_json::Value::as_array)
    {
        let values = values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        if let Some(unknown) = values
            .iter()
            .find(|value| !OXIDE_APP_SETTINGS_SECTION_IDS.contains(&value.as_str()))
        {
            return Err(format!(
                "Unknown Cloud Sync app settings section: {unknown}"
            ));
        }
        scope.app_settings_sections = Some(values);
        updated_fields.push("scope.app_settings_sections".to_string());
    }
    if let Some(values) = scope_object
        .get("plugin_ids")
        .and_then(serde_json::Value::as_array)
    {
        let values = values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        scope.plugin_ids = Some(get_syncable_plugin_ids(&values));
        updated_fields.push("scope.plugin_ids".to_string());
    }
    Ok(())
}

pub fn normalize_cloud_sync_interval_draft(form: &mut CloudSyncFormDraft, interval: f64) {
    form.auto_upload_interval_mins = cloud_sync_number_string(interval);
}

pub fn store_cloud_sync_touched_secrets(
    secrets: &CloudSyncSecretDraftHandoff,
    provider: &mut impl CloudSyncSecretWriter,
) -> anyhow::Result<()> {
    if let Some(value) = secrets.token.as_deref() {
        provider.write_secret(secret_keys::TOKEN, non_empty_secret(value))?;
    }
    if let Some(value) = secrets.git_token.as_deref() {
        provider.write_secret(secret_keys::GIT_TOKEN, non_empty_secret(value))?;
    }
    if let Some(value) = secrets.basic_username.as_deref() {
        provider.write_secret(secret_keys::BASIC_USERNAME, non_empty_secret(value))?;
    }
    if let Some(value) = secrets.basic_password.as_deref() {
        provider.write_secret(secret_keys::BASIC_PASSWORD, non_empty_secret(value))?;
    }
    if let Some(value) = secrets.access_key_id.as_deref() {
        provider.write_secret(secret_keys::ACCESS_KEY_ID, non_empty_secret(value))?;
    }
    if let Some(value) = secrets.secret_access_key.as_deref() {
        provider.write_secret(secret_keys::SECRET_ACCESS_KEY, non_empty_secret(value))?;
    }
    if let Some(value) = secrets.session_token.as_deref() {
        provider.write_secret(secret_keys::SESSION_TOKEN, non_empty_secret(value))?;
    }
    if let Some(value) = secrets.sync_password.as_deref() {
        provider.write_secret(secret_keys::SYNC_PASSWORD, non_empty_secret(value))?;
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
    fn settings_from_form_clears_hidden_backend_endpoints() {
        let mut form = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        form.backend_type = BackendType::GithubGist;
        form.auth_mode = AuthMode::Bearer;
        form.endpoint = "https://dav.example.test".to_string();
        form.git_repository = "abcdef123456".to_string();

        let (settings, _) = cloud_sync_settings_from_form(&form);

        assert_eq!(settings.auth_mode, AuthMode::None);
        assert!(settings.endpoint.is_empty());
        assert_eq!(settings.git_repository, "abcdef123456");

        let mut form = CloudSyncFormDraft::from_settings(&CloudSyncSettings::default());
        form.backend_type = BackendType::GoogleDrive;
        form.auth_mode = AuthMode::Bearer;
        form.endpoint = "https://dav.example.test".to_string();
        form.google_oauth_client_id = "google-client-id".to_string();

        let (settings, _) = cloud_sync_settings_from_form(&form);

        assert_eq!(settings.backend_type, BackendType::GoogleDrive);
        assert_eq!(settings.auth_mode, AuthMode::None);
        assert!(settings.endpoint.is_empty());
        assert_eq!(settings.google_oauth_client_id, "google-client-id");
    }

    #[test]
    fn configuration_patch_updates_requested_fields_and_preserves_others() {
        let mut settings = CloudSyncSettings::default();
        settings.namespace = "existing".to_string();
        let scope = RawSyncScope::default();

        let (patched, patched_scope, fields) = apply_cloud_sync_configuration_patch(
            &settings,
            &scope,
            &serde_json::json!({
                "backend_type": "http-json",
                "endpoint": "https://sync.example.test",
                "github_oauth_client_id": "public-client-id",
                "scope": {
                    "sync_connections": false,
                    "app_settings_sections": ["general", "network"]
                }
            }),
        )
        .expect("valid non-secret configuration patch");

        assert_eq!(patched.backend_type, BackendType::HttpJson);
        assert_eq!(patched.endpoint, "https://sync.example.test");
        assert_eq!(patched.namespace, "existing");
        assert_eq!(patched.github_oauth_client_id, "public-client-id");
        assert_eq!(patched_scope.sync_connections, Some(false));
        assert_eq!(
            patched_scope.app_settings_sections,
            Some(vec!["general".to_string(), "network".to_string()])
        );
        assert!(fields.contains(&"scope.sync_connections".to_string()));
    }

    #[test]
    fn configuration_patch_rejects_credentials_embedded_in_locations() {
        let result = apply_cloud_sync_configuration_patch(
            &CloudSyncSettings::default(),
            &RawSyncScope::default(),
            &serde_json::json!({
                "endpoint": "https://user:password@sync.example.test"
            }),
        );

        assert_eq!(
            result.expect_err("embedded credentials must be rejected"),
            "Cloud Sync locations cannot contain embedded credentials."
        );
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
