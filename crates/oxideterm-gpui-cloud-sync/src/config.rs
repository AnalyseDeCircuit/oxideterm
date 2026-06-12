// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync configuration form visibility model.
//!
//! The GPUI app supplies localized labels and callbacks. This module owns which
//! fields are visible for each backend/auth combination so the rules stay near
//! the Cloud Sync form model instead of being embedded in view code.

use oxideterm_cloud_sync::{
    AuthMode, BackendType, secret_keys,
    secrets::{
        backend_uses_auth_mode, backend_uses_basic, backend_uses_git_token,
        backend_uses_s3_credentials, backend_uses_token,
    },
};
use oxideterm_gpui_settings_view::SettingsInput;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncConfigRow {
    BackendSelect,
    AuthModeSelect,
    Text(CloudSyncTextFieldSpec),
    Secret(CloudSyncSecretFieldSpec),
    AutoUploadToggle,
    ConflictSelect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncTextFieldSpec {
    pub label_key: &'static str,
    pub input: SettingsInput,
    pub placeholder_key: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncSecretFieldSpec {
    pub label_key: &'static str,
    pub input: SettingsInput,
    pub placeholder_key: &'static str,
    pub secret_key: &'static str,
}

pub fn cloud_sync_config_rows(
    backend: &BackendType,
    auth_mode: &AuthMode,
) -> Vec<CloudSyncConfigRow> {
    let mut rows = vec![CloudSyncConfigRow::BackendSelect];
    if backend_uses_auth_mode(backend) {
        rows.push(CloudSyncConfigRow::AuthModeSelect);
    }
    if !matches!(backend, BackendType::Dropbox | BackendType::GithubGist) {
        rows.push(CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
            label_key: "plugin.cloud_sync.settings.endpoint",
            input: SettingsInput::CloudSyncEndpoint,
            placeholder_key: cloud_sync_endpoint_placeholder_key(backend),
        }));
    }
    rows.push(CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
        label_key: cloud_sync_namespace_label_key(backend),
        input: SettingsInput::CloudSyncNamespace,
        placeholder_key: "plugin.cloud_sync.placeholders.namespace",
    }));
    if matches!(backend, BackendType::Git) {
        rows.extend([
            CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
                label_key: "plugin.cloud_sync.settings.git_repository",
                input: SettingsInput::CloudSyncGitRepository,
                placeholder_key: "plugin.cloud_sync.placeholders.git_repository",
            }),
            CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
                label_key: "plugin.cloud_sync.settings.git_branch",
                input: SettingsInput::CloudSyncGitBranch,
                placeholder_key: "plugin.cloud_sync.placeholders.git_branch",
            }),
        ]);
    }
    if matches!(backend, BackendType::GithubGist) {
        rows.extend([
            CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
                label_key: "plugin.cloud_sync.settings.gist_id",
                input: SettingsInput::CloudSyncGitRepository,
                placeholder_key: "plugin.cloud_sync.placeholders.gist_id",
            }),
            CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
                label_key: "plugin.cloud_sync.settings.github_oauth_client_id",
                input: SettingsInput::CloudSyncGithubOauthClientId,
                placeholder_key: "plugin.cloud_sync.placeholders.github_oauth_client_id",
            }),
        ]);
    }
    if backend_uses_s3_credentials(backend) {
        rows.extend([
            CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
                label_key: "plugin.cloud_sync.settings.s3_bucket",
                input: SettingsInput::CloudSyncS3Bucket,
                placeholder_key: "plugin.cloud_sync.placeholders.s3_bucket",
            }),
            CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
                label_key: "plugin.cloud_sync.settings.s3_region",
                input: SettingsInput::CloudSyncS3Region,
                placeholder_key: "plugin.cloud_sync.placeholders.s3_region",
            }),
        ]);
    }
    if backend_uses_token(backend, auth_mode) {
        rows.push(CloudSyncConfigRow::Secret(CloudSyncSecretFieldSpec {
            label_key: cloud_sync_token_label_key(backend),
            input: SettingsInput::CloudSyncToken,
            placeholder_key: "plugin.cloud_sync.placeholders.token",
            secret_key: secret_keys::TOKEN,
        }));
    }
    if backend_uses_git_token(backend) {
        rows.push(CloudSyncConfigRow::Secret(CloudSyncSecretFieldSpec {
            label_key: cloud_sync_git_token_label_key(backend),
            input: SettingsInput::CloudSyncGitToken,
            placeholder_key: cloud_sync_git_token_placeholder_key(backend),
            secret_key: secret_keys::GIT_TOKEN,
        }));
    }
    if backend_uses_basic(backend, auth_mode) {
        rows.extend([
            CloudSyncConfigRow::Secret(CloudSyncSecretFieldSpec {
                label_key: "plugin.cloud_sync.settings.basic_username",
                input: SettingsInput::CloudSyncBasicUsername,
                placeholder_key: "plugin.cloud_sync.placeholders.username",
                secret_key: secret_keys::BASIC_USERNAME,
            }),
            CloudSyncConfigRow::Secret(CloudSyncSecretFieldSpec {
                label_key: "plugin.cloud_sync.settings.basic_password",
                input: SettingsInput::CloudSyncBasicPassword,
                placeholder_key: "plugin.cloud_sync.placeholders.password",
                secret_key: secret_keys::BASIC_PASSWORD,
            }),
        ]);
    }
    if backend_uses_s3_credentials(backend) {
        rows.extend([
            CloudSyncConfigRow::Secret(CloudSyncSecretFieldSpec {
                label_key: "plugin.cloud_sync.settings.access_key_id",
                input: SettingsInput::CloudSyncAccessKeyId,
                placeholder_key: "plugin.cloud_sync.placeholders.access_key_id",
                secret_key: secret_keys::ACCESS_KEY_ID,
            }),
            CloudSyncConfigRow::Secret(CloudSyncSecretFieldSpec {
                label_key: "plugin.cloud_sync.settings.secret_access_key",
                input: SettingsInput::CloudSyncSecretAccessKey,
                placeholder_key: "plugin.cloud_sync.placeholders.secret_access_key",
                secret_key: secret_keys::SECRET_ACCESS_KEY,
            }),
            CloudSyncConfigRow::Secret(CloudSyncSecretFieldSpec {
                label_key: "plugin.cloud_sync.settings.session_token",
                input: SettingsInput::CloudSyncSessionToken,
                placeholder_key: "plugin.cloud_sync.placeholders.session_token",
                secret_key: secret_keys::SESSION_TOKEN,
            }),
        ]);
    }
    rows.extend([
        CloudSyncConfigRow::Secret(CloudSyncSecretFieldSpec {
            label_key: "plugin.cloud_sync.settings.sync_password",
            input: SettingsInput::CloudSyncSyncPassword,
            placeholder_key: "plugin.cloud_sync.placeholders.sync_password",
            secret_key: secret_keys::SYNC_PASSWORD,
        }),
        CloudSyncConfigRow::AutoUploadToggle,
        CloudSyncConfigRow::Text(CloudSyncTextFieldSpec {
            label_key: "plugin.cloud_sync.settings.auto_upload_interval",
            input: SettingsInput::CloudSyncAutoUploadInterval,
            placeholder_key: "60",
        }),
        CloudSyncConfigRow::ConflictSelect,
    ]);
    rows
}

pub fn cloud_sync_endpoint_placeholder_key(backend: &BackendType) -> &'static str {
    match backend {
        BackendType::S3 => "plugin.cloud_sync.placeholders.endpoint_s3",
        BackendType::Git => "plugin.cloud_sync.placeholders.endpoint_git",
        BackendType::GithubGist => "plugin.cloud_sync.placeholders.endpoint_git",
        BackendType::HttpJson => "plugin.cloud_sync.placeholders.endpoint_http_json",
        BackendType::Dropbox => "plugin.cloud_sync.placeholders.endpoint_http_json",
        BackendType::Webdav => "plugin.cloud_sync.placeholders.endpoint_webdav",
    }
}

pub fn cloud_sync_namespace_label_key(backend: &BackendType) -> &'static str {
    if matches!(
        backend,
        BackendType::Dropbox | BackendType::Git | BackendType::GithubGist
    ) {
        "plugin.cloud_sync.settings.path_prefix"
    } else if matches!(backend, BackendType::S3) {
        "plugin.cloud_sync.settings.object_prefix"
    } else {
        "plugin.cloud_sync.settings.namespace"
    }
}

pub fn cloud_sync_token_label_key(backend: &BackendType) -> &'static str {
    if matches!(backend, BackendType::Dropbox) {
        "plugin.cloud_sync.settings.access_token"
    } else {
        "plugin.cloud_sync.settings.token"
    }
}

pub fn cloud_sync_git_token_label_key(backend: &BackendType) -> &'static str {
    if matches!(backend, BackendType::GithubGist) {
        "plugin.cloud_sync.settings.github_access_token"
    } else {
        "plugin.cloud_sync.settings.git_access_token"
    }
}

pub fn cloud_sync_git_token_placeholder_key(backend: &BackendType) -> &'static str {
    if matches!(backend, BackendType::GithubGist) {
        "plugin.cloud_sync.placeholders.github_access_token"
    } else {
        "plugin.cloud_sync.placeholders.git_access_token"
    }
}
