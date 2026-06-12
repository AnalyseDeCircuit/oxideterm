// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync status explanation models.

use oxideterm_cloud_sync::{
    BackendType, CloudSyncStatus, RawSyncScope, normalize_sync_scope, secret_keys,
    secrets::{backend_uses_basic, backend_uses_s3_credentials, backend_uses_token},
    state::CloudSyncPersistedState,
};
use oxideterm_settings_model::CloudSyncFormDraft;

use crate::cloud_sync_format_timestamp;

const EMPTY_VALUE: &str = "—";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncInfoRow {
    pub label_key: &'static str,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncConflictInfo {
    pub rows: Vec<CloudSyncInfoRow>,
    pub recommendation_key: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncHealthStatus {
    Pass,
    Warning,
    Fail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncHealthItem {
    pub label_key: &'static str,
    pub detail_key: &'static str,
    pub status: CloudSyncHealthStatus,
}

/// Builds a user-facing version/device ledger from persisted sync state.
pub fn cloud_sync_version_info_rows(
    state: &CloudSyncPersistedState,
    local_counts: Option<String>,
) -> Vec<CloudSyncInfoRow> {
    vec![
        info_row(
            "plugin.cloud_sync.fields.local_device",
            optional_value(state.device_id.clone()),
        ),
        info_row(
            "plugin.cloud_sync.fields.local_revision_sequence",
            state.revision_seq.to_string(),
        ),
        info_row(
            "plugin.cloud_sync.fields.last_upload",
            timestamp_value(state.last_upload_at.as_deref()),
        ),
        info_row(
            "plugin.cloud_sync.fields.last_check",
            timestamp_value(state.last_check_at.as_deref()),
        ),
        info_row(
            "plugin.cloud_sync.fields.remote_revision",
            optional_value(state.last_known_remote_revision.clone()),
        ),
        info_row(
            "plugin.cloud_sync.fields.remote_device",
            optional_value(state.remote_device_id.clone()),
        ),
        info_row(
            "plugin.cloud_sync.fields.remote_updated_at",
            timestamp_value(state.remote_updated_at.as_deref()),
        ),
        info_row(
            "plugin.cloud_sync.fields.remote_format",
            optional_value(state.remote_format.clone()),
        ),
        info_row(
            "plugin.cloud_sync.fields.remote_etag",
            optional_value(state.last_known_remote_etag.clone()),
        ),
        info_row(
            "plugin.cloud_sync.fields.local_counts",
            local_counts.unwrap_or_else(empty_value),
        ),
    ]
}

/// Explains the current conflict state when local and remote changes diverge.
pub fn cloud_sync_conflict_info(state: &CloudSyncPersistedState) -> Option<CloudSyncConflictInfo> {
    if !state.auto_upload_blocked_by_conflict && state.conflict_details.is_none() {
        return None;
    }
    let details = state.conflict_details.as_ref();
    Some(CloudSyncConflictInfo {
        rows: vec![
            info_row(
                "plugin.cloud_sync.fields.conflict_remote_revision",
                optional_value(details.and_then(|details| details.revision.clone())),
            ),
            info_row(
                "plugin.cloud_sync.fields.conflict_remote_device",
                optional_value(details.and_then(|details| details.device_id.clone())),
            ),
            info_row(
                "plugin.cloud_sync.fields.conflict_remote_updated_at",
                timestamp_value(details.and_then(|details| details.updated_at.as_deref())),
            ),
        ],
        recommendation_key: "plugin.cloud_sync.conflict.detail_recommendation",
    })
}

/// Builds local preflight checks for the Cloud Sync page.
///
/// Secret fields are only checked for presence through UI drafts and persisted
/// keychain hints; this model never clones or formats the secret values.
pub fn cloud_sync_health_items(
    form: &CloudSyncFormDraft,
    state: &CloudSyncPersistedState,
) -> Vec<CloudSyncHealthItem> {
    vec![
        backend_config_health_item(form, state),
        sync_password_health_item(form, state),
        remote_check_health_item(state),
        conflict_health_item(state),
        local_changes_health_item(state),
        sync_scope_health_item(&state.sync_scope),
    ]
}

fn backend_config_health_item(
    form: &CloudSyncFormDraft,
    state: &CloudSyncPersistedState,
) -> CloudSyncHealthItem {
    let ready =
        required_backend_text_fields_present(form) && required_backend_secrets_present(form, state);
    health_item(
        "plugin.cloud_sync.health.backend_config",
        if ready {
            "plugin.cloud_sync.health.backend_config_ok"
        } else {
            "plugin.cloud_sync.health.backend_config_missing"
        },
        if ready {
            CloudSyncHealthStatus::Pass
        } else {
            CloudSyncHealthStatus::Fail
        },
    )
}

fn sync_password_health_item(
    form: &CloudSyncFormDraft,
    state: &CloudSyncPersistedState,
) -> CloudSyncHealthItem {
    let ready = secret_present(state, secret_keys::SYNC_PASSWORD, &form.sync_password);
    health_item(
        "plugin.cloud_sync.health.sync_password",
        if ready {
            "plugin.cloud_sync.health.sync_password_ok"
        } else {
            "plugin.cloud_sync.health.sync_password_missing"
        },
        if ready {
            CloudSyncHealthStatus::Pass
        } else {
            CloudSyncHealthStatus::Fail
        },
    )
}

fn remote_check_health_item(state: &CloudSyncPersistedState) -> CloudSyncHealthItem {
    let has_remote_contact = state.last_check_at.is_some() || state.last_upload_at.is_some();
    if state.status == CloudSyncStatus::Error {
        return health_item(
            "plugin.cloud_sync.health.remote_check",
            "plugin.cloud_sync.health.remote_check_failed",
            CloudSyncHealthStatus::Fail,
        );
    }
    health_item(
        "plugin.cloud_sync.health.remote_check",
        if has_remote_contact {
            "plugin.cloud_sync.health.remote_check_ok"
        } else {
            "plugin.cloud_sync.health.remote_check_not_run"
        },
        if has_remote_contact {
            CloudSyncHealthStatus::Pass
        } else {
            CloudSyncHealthStatus::Warning
        },
    )
}

fn conflict_health_item(state: &CloudSyncPersistedState) -> CloudSyncHealthItem {
    let blocked = state.auto_upload_blocked_by_conflict
        || state.conflict_details.is_some()
        || state.status == CloudSyncStatus::Conflict;
    health_item(
        "plugin.cloud_sync.health.conflict_state",
        if blocked {
            "plugin.cloud_sync.health.conflict_state_blocked"
        } else {
            "plugin.cloud_sync.health.conflict_state_ok"
        },
        if blocked {
            CloudSyncHealthStatus::Fail
        } else {
            CloudSyncHealthStatus::Pass
        },
    )
}

fn local_changes_health_item(state: &CloudSyncPersistedState) -> CloudSyncHealthItem {
    health_item(
        "plugin.cloud_sync.health.local_changes",
        if state.local_dirty {
            "plugin.cloud_sync.health.local_changes_dirty"
        } else {
            "plugin.cloud_sync.health.local_changes_clean"
        },
        if state.local_dirty {
            CloudSyncHealthStatus::Warning
        } else {
            CloudSyncHealthStatus::Pass
        },
    )
}

fn sync_scope_health_item(raw_scope: &RawSyncScope) -> CloudSyncHealthItem {
    let scope = normalize_sync_scope(Some(raw_scope), &[]);
    let has_sync_content = scope.sync_connections
        || scope.sync_forwards
        || scope.sync_quick_commands
        || scope.sync_serial_profiles
        || scope.sync_app_settings
        || scope.sync_plugin_settings;
    if !has_sync_content {
        return health_item(
            "plugin.cloud_sync.health.sync_scope",
            "plugin.cloud_sync.health.sync_scope_empty",
            CloudSyncHealthStatus::Fail,
        );
    }
    health_item(
        "plugin.cloud_sync.health.sync_scope",
        if scope.sync_sensitive_credentials {
            "plugin.cloud_sync.health.sync_scope_ok"
        } else {
            "plugin.cloud_sync.health.sync_scope_warning"
        },
        if scope.sync_sensitive_credentials {
            CloudSyncHealthStatus::Pass
        } else {
            CloudSyncHealthStatus::Warning
        },
    )
}

fn required_backend_text_fields_present(form: &CloudSyncFormDraft) -> bool {
    match form.backend_type {
        BackendType::Dropbox => true,
        BackendType::Git => has_text(&form.git_repository),
        BackendType::S3 => {
            has_text(&form.endpoint) && has_text(&form.s3_bucket) && has_text(&form.s3_region)
        }
        BackendType::Webdav | BackendType::HttpJson => has_text(&form.endpoint),
    }
}

fn required_backend_secrets_present(
    form: &CloudSyncFormDraft,
    state: &CloudSyncPersistedState,
) -> bool {
    if backend_uses_token(&form.backend_type, &form.auth_mode)
        && !secret_present(state, secret_keys::TOKEN, &form.token)
    {
        return false;
    }
    if backend_uses_basic(&form.backend_type, &form.auth_mode)
        && (!secret_present(state, secret_keys::BASIC_USERNAME, &form.basic_username)
            || !secret_present(state, secret_keys::BASIC_PASSWORD, &form.basic_password))
    {
        return false;
    }
    if backend_uses_s3_credentials(&form.backend_type)
        && (!secret_present(state, secret_keys::ACCESS_KEY_ID, &form.access_key_id)
            || !secret_present(
                state,
                secret_keys::SECRET_ACCESS_KEY,
                &form.secret_access_key,
            ))
    {
        return false;
    }
    true
}

fn secret_present(state: &CloudSyncPersistedState, key: &str, draft: &str) -> bool {
    has_text(draft) || state.secret_hints.get(key).copied().unwrap_or(false)
}

fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}

fn health_item(
    label_key: &'static str,
    detail_key: &'static str,
    status: CloudSyncHealthStatus,
) -> CloudSyncHealthItem {
    CloudSyncHealthItem {
        label_key,
        detail_key,
        status,
    }
}

fn info_row(label_key: &'static str, value: String) -> CloudSyncInfoRow {
    CloudSyncInfoRow { label_key, value }
}

fn optional_value(value: Option<String>) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(empty_value)
}

fn timestamp_value(value: Option<&str>) -> String {
    value
        .map(cloud_sync_format_timestamp)
        .unwrap_or_else(empty_value)
}

fn empty_value() -> String {
    EMPTY_VALUE.to_string()
}

#[cfg(test)]
mod tests {
    use oxideterm_cloud_sync::{
        AuthMode, BackendType, CloudSyncSettings, secret_keys,
        state::{CloudSyncConflictDetails, CloudSyncPersistedState},
    };
    use oxideterm_settings_model::CloudSyncFormDraft;

    use super::*;

    #[test]
    fn version_info_rows_include_local_and_remote_identity() {
        let state = CloudSyncPersistedState {
            device_id: Some("mac-1234".to_string()),
            revision_seq: 7,
            last_known_remote_revision: Some("rev-2".to_string()),
            remote_device_id: Some("linux-5678".to_string()),
            last_known_remote_etag: Some("etag-2".to_string()),
            ..CloudSyncPersistedState::default()
        };

        let rows = cloud_sync_version_info_rows(&state, Some("3 / 4".to_string()));

        assert!(rows.iter().any(|row| {
            row.label_key == "plugin.cloud_sync.fields.local_device" && row.value == "mac-1234"
        }));
        assert!(rows.iter().any(|row| {
            row.label_key == "plugin.cloud_sync.fields.remote_revision" && row.value == "rev-2"
        }));
        assert!(rows.iter().any(|row| {
            row.label_key == "plugin.cloud_sync.fields.local_counts" && row.value == "3 / 4"
        }));
    }

    #[test]
    fn conflict_info_is_absent_without_conflict_and_present_with_details() {
        assert!(cloud_sync_conflict_info(&CloudSyncPersistedState::default()).is_none());

        let state = CloudSyncPersistedState {
            auto_upload_blocked_by_conflict: true,
            conflict_details: Some(CloudSyncConflictDetails {
                revision: Some("remote-rev".to_string()),
                device_id: Some("remote-device".to_string()),
                updated_at: None,
            }),
            ..CloudSyncPersistedState::default()
        };

        let info = cloud_sync_conflict_info(&state).expect("conflict info");

        assert_eq!(
            info.recommendation_key,
            "plugin.cloud_sync.conflict.detail_recommendation"
        );
        assert!(info.rows.iter().any(|row| {
            row.label_key == "plugin.cloud_sync.fields.conflict_remote_revision"
                && row.value == "remote-rev"
        }));
    }

    #[test]
    fn health_items_fail_when_required_cloud_sync_secrets_are_missing() {
        let mut settings = CloudSyncSettings::default();
        settings.backend_type = BackendType::HttpJson;
        settings.auth_mode = AuthMode::Bearer;
        settings.endpoint = "https://sync.example.test".to_string();
        let form = CloudSyncFormDraft::from_settings(&settings);

        let items = cloud_sync_health_items(&form, &CloudSyncPersistedState::default());

        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.health.backend_config"
                && item.status == CloudSyncHealthStatus::Fail
        }));
        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.health.sync_password"
                && item.status == CloudSyncHealthStatus::Fail
        }));
    }

    #[test]
    fn health_items_accept_secret_hints_without_secret_value_clones() {
        let mut settings = CloudSyncSettings::default();
        settings.backend_type = BackendType::S3;
        settings.endpoint = "https://s3.example.test".to_string();
        settings.s3_bucket = "oxide".to_string();
        settings.s3_region = "us-east-1".to_string();
        let form = CloudSyncFormDraft::from_settings(&settings);
        let mut state = CloudSyncPersistedState::default();
        state
            .secret_hints
            .insert(secret_keys::ACCESS_KEY_ID.to_string(), true);
        state
            .secret_hints
            .insert(secret_keys::SECRET_ACCESS_KEY.to_string(), true);
        state
            .secret_hints
            .insert(secret_keys::SYNC_PASSWORD.to_string(), true);
        state.last_check_at = Some("2026-06-12T00:00:00Z".to_string());

        let items = cloud_sync_health_items(&form, &state);

        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.health.backend_config"
                && item.status == CloudSyncHealthStatus::Pass
        }));
        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.health.sync_password"
                && item.status == CloudSyncHealthStatus::Pass
        }));
        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.health.remote_check"
                && item.status == CloudSyncHealthStatus::Pass
        }));
    }

    #[test]
    fn health_items_warn_for_unchecked_remote_and_excluded_sensitive_scope() {
        let mut settings = CloudSyncSettings::default();
        settings.endpoint = "https://sync.example.test".to_string();
        let mut form = CloudSyncFormDraft::from_settings(&settings);
        form.token = "draft-token".to_string();
        form.sync_password = "draft-password".to_string();

        let items = cloud_sync_health_items(&form, &CloudSyncPersistedState::default());

        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.health.remote_check"
                && item.status == CloudSyncHealthStatus::Warning
        }));
        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.health.sync_scope"
                && item.status == CloudSyncHealthStatus::Warning
        }));
    }
}
