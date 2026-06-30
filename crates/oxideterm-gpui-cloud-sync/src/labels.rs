// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! I18n key adapters for Cloud Sync UI copy.

use oxideterm_cloud_sync::{
    BackendType, CloudSyncStatus, progress::CloudSyncProgressStage, state::CloudSyncRollbackBackup,
};
use oxideterm_gpui_ui::ConfirmDialogVariant;

use crate::{
    CloudSyncPreviewSource, CloudSyncSelectLabelKey,
    format::{cloud_sync_error_code, cloud_sync_snapshot_limit_bytes, format_cloud_sync_bytes},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncConfirm {
    ImportPreview,
    ClearSecret { key: String, label: String },
    RestoreBackup { id: String, created_at: String },
    DeleteBackup { id: String, created_at: String },
    ClearBackups,
    ClearHistory,
    EnableSensitiveSync,
}

pub fn cloud_sync_backend_label_key(backend: &BackendType) -> &'static str {
    match backend {
        BackendType::Webdav => "plugin.cloud_sync.backend.webdav",
        BackendType::HttpJson => "plugin.cloud_sync.backend.http_json",
        BackendType::Dropbox => "plugin.cloud_sync.backend.dropbox",
        BackendType::OneDrive => "plugin.cloud_sync.backend.onedrive",
        BackendType::GoogleDrive => "plugin.cloud_sync.backend.google_drive",
        BackendType::GithubGist => "plugin.cloud_sync.backend.github_gist",
        BackendType::S3 => "plugin.cloud_sync.backend.s3",
        BackendType::Git => "plugin.cloud_sync.backend.git",
    }
}

pub fn cloud_sync_status_label_key(status: CloudSyncStatus) -> &'static str {
    match status {
        CloudSyncStatus::Idle => "plugin.cloud_sync.status.ready",
        CloudSyncStatus::Uploading => "plugin.cloud_sync.status.uploading",
        CloudSyncStatus::Checking => "plugin.cloud_sync.status.checking",
        CloudSyncStatus::RemoteUpdate => "plugin.cloud_sync.status.remote_update",
        CloudSyncStatus::Conflict => "plugin.cloud_sync.status.conflict",
        CloudSyncStatus::Error => "plugin.cloud_sync.status.error",
    }
}

pub fn cloud_sync_progress_stage_label_key(stage: CloudSyncProgressStage) -> &'static str {
    match stage {
        CloudSyncProgressStage::FetchMetadata => "plugin.cloud_sync.progress.fetch_metadata",
        CloudSyncProgressStage::Preflight => "plugin.cloud_sync.progress.preflight",
        CloudSyncProgressStage::Exporting => "plugin.cloud_sync.progress.exporting",
        CloudSyncProgressStage::UploadingBlob => "plugin.cloud_sync.progress.uploading_blob",
        CloudSyncProgressStage::Downloading => "plugin.cloud_sync.progress.downloading",
        CloudSyncProgressStage::Validating => "plugin.cloud_sync.progress.validating",
        CloudSyncProgressStage::PreviewingImport => "plugin.cloud_sync.progress.previewing_import",
        CloudSyncProgressStage::Importing => "plugin.cloud_sync.progress.importing",
        CloudSyncProgressStage::CreatingBackup => "plugin.cloud_sync.progress.creating_backup",
        CloudSyncProgressStage::Done => "plugin.cloud_sync.progress.done",
        _ => "plugin.cloud_sync.progress.done",
    }
}

pub fn cloud_sync_history_action_label_key(action: &str) -> Option<&'static str> {
    match action {
        "upload" => Some("plugin.cloud_sync.history.action_upload"),
        "pull" => Some("plugin.cloud_sync.history.action_pull"),
        "restore" => Some("plugin.cloud_sync.history.action_restore"),
        _ => None,
    }
}

pub fn cloud_sync_preview_record_label_key(action: &str) -> Option<&'static str> {
    match action {
        "import" => Some("plugin.cloud_sync.preview.record_import"),
        "rename" => Some("plugin.cloud_sync.preview.record_rename"),
        "skip" => Some("plugin.cloud_sync.preview.record_skip"),
        "replace" => Some("plugin.cloud_sync.preview.record_replace"),
        "merge" => Some("plugin.cloud_sync.preview.record_merge"),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncErrorMessageSpec {
    /// Raw backend text is preserved when the error is not a known Cloud Sync code.
    Raw(String),
    /// Known error codes are mapped to stable translation keys outside the app crate.
    Key(&'static str),
    /// Snapshot size errors need one dynamic replacement value for localized copy.
    SnapshotTooLarge { limit: Option<String> },
}

/// Converts backend error strings into UI copy specs without needing WorkspaceApp.
pub fn cloud_sync_error_message_spec(error: &str) -> CloudSyncErrorMessageSpec {
    let Some(code) = cloud_sync_error_code(error) else {
        return CloudSyncErrorMessageSpec::Raw(error.to_string());
    };
    match code {
        "missing_endpoint" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_endpoint")
        }
        "missing_namespace" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_namespace")
        }
        "missing_backend_token" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_backend_token")
        }
        "missing_github_oauth_client_id" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.missing_github_oauth_client_id",
        ),
        "missing_microsoft_oauth_client_id" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.missing_microsoft_oauth_client_id",
        ),
        "missing_microsoft_refresh_token" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.missing_microsoft_refresh_token",
        ),
        "missing_google_oauth_client_id" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.missing_google_oauth_client_id",
        ),
        "missing_google_refresh_token" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_google_refresh_token")
        }
        "http_unauthorized" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.http_unauthorized")
        }
        "network_request_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.network_request_failed")
        }
        "missing_git_repository" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_git_repository")
        }
        "missing_gist_id" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_gist_id")
        }
        "github_gist_bad_credentials" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_gist_bad_credentials")
        }
        "github_gist_missing_scope" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_gist_missing_scope")
        }
        "github_gist_rate_limited" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_gist_rate_limited")
        }
        "github_oauth_expired" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_oauth_expired")
        }
        "github_oauth_denied" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_oauth_denied")
        }
        "github_oauth_bad_client" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_oauth_bad_client")
        }
        "github_oauth_start_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_oauth_start_failed")
        }
        "github_oauth_poll_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_oauth_poll_failed")
        }
        "github_oauth_empty_response" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.github_oauth_empty_response")
        }
        "onedrive_bad_credentials" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_bad_credentials")
        }
        "onedrive_missing_scope" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_missing_scope")
        }
        "onedrive_access_denied" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_access_denied")
        }
        "onedrive_bad_request" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_bad_request")
        }
        "onedrive_locked" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_locked")
        }
        "onedrive_rate_limited" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_rate_limited")
        }
        "onedrive_quota_exceeded" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_quota_exceeded")
        }
        "onedrive_service_unavailable" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_service_unavailable")
        }
        "google_drive_bad_credentials" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_drive_bad_credentials")
        }
        "google_drive_missing_scope" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_drive_missing_scope")
        }
        "google_drive_access_denied" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_drive_access_denied")
        }
        "google_drive_bad_request" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_drive_bad_request")
        }
        "google_drive_api_not_enabled" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_drive_api_not_enabled")
        }
        "google_drive_rate_limited" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_drive_rate_limited")
        }
        "google_drive_quota_exceeded" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_drive_quota_exceeded")
        }
        "google_drive_service_unavailable" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.google_drive_service_unavailable",
        ),
        "microsoft_oauth_expired" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.microsoft_oauth_expired")
        }
        "microsoft_oauth_denied" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.microsoft_oauth_denied")
        }
        "microsoft_oauth_bad_code" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.microsoft_oauth_bad_code")
        }
        "microsoft_oauth_bad_client" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.microsoft_oauth_bad_client")
        }
        "microsoft_oauth_missing_scope" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.microsoft_oauth_missing_scope")
        }
        "microsoft_oauth_consent_required" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.microsoft_oauth_consent_required",
        ),
        "microsoft_oauth_invalid_request" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.microsoft_oauth_invalid_request",
        ),
        "microsoft_oauth_start_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.microsoft_oauth_start_failed")
        }
        "microsoft_oauth_poll_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.microsoft_oauth_poll_failed")
        }
        "microsoft_oauth_refresh_failed" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.microsoft_oauth_refresh_failed",
        ),
        "microsoft_oauth_empty_response" => CloudSyncErrorMessageSpec::Key(
            "plugin.cloud_sync.errors.microsoft_oauth_empty_response",
        ),
        "google_oauth_denied" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_denied")
        }
        "google_oauth_admin_policy" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_admin_policy")
        }
        "google_oauth_bad_client" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_bad_client")
        }
        "google_oauth_missing_scope" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_missing_scope")
        }
        "google_oauth_consent_required" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_consent_required")
        }
        "google_oauth_invalid_request" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_invalid_request")
        }
        "google_oauth_start_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_start_failed")
        }
        "google_oauth_redirect_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_redirect_failed")
        }
        "google_oauth_exchange_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_exchange_failed")
        }
        "google_oauth_refresh_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_refresh_failed")
        }
        "google_oauth_empty_response" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_empty_response")
        }
        "google_oauth_invalid_state" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_invalid_state")
        }
        "google_oauth_timeout" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_timeout")
        }
        "missing_s3_bucket" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_s3_bucket")
        }
        "missing_s3_region" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_s3_region")
        }
        "missing_s3_access_key_id" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_s3_access_key_id")
        }
        "missing_s3_secret_access_key" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_s3_secret_access_key")
        }
        "missing_sync_password" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_sync_password")
        }
        "operation_in_progress" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.operation_in_progress")
        }
        "secret_unlock_required" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.secret_unlock_required")
        }
        "secret_access_cancelled" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.secret_access_cancelled")
        }
        "secret_access_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.secret_access_failed")
        }
        "etag_conflict_detected" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.etag_conflict_detected")
        }
        "remote_changed_before_upload" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.remote_changed_before_upload")
        }
        "preflight_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.preflight_failed")
        }
        "remote_not_found" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.remote_not_found")
        }
        "snapshot_too_large" => CloudSyncErrorMessageSpec::SnapshotTooLarge {
            limit: cloud_sync_snapshot_limit_bytes(error).map(format_cloud_sync_bytes),
        },
        _ => CloudSyncErrorMessageSpec::Raw(error.to_string()),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncRollbackBackupSummarySpec {
    /// Older backups may only carry a byte size.
    SizeOnly(String),
    /// Newer backups carry count metadata that the app can localize.
    Metadata {
        connections: usize,
        forwards: usize,
        quick_commands: usize,
        serial_profiles: usize,
        raw_tcp_profiles: usize,
        raw_udp_profiles: usize,
        sensitive_credentials: usize,
        plugin_settings_count: usize,
        size: String,
    },
}

/// Builds a localizable backup summary model from persisted rollback metadata.
pub fn cloud_sync_rollback_backup_summary_spec(
    backup: &CloudSyncRollbackBackup,
) -> CloudSyncRollbackBackupSummarySpec {
    let size = format_cloud_sync_bytes(backup.size_bytes);
    match backup.metadata.as_ref() {
        Some(metadata) => CloudSyncRollbackBackupSummarySpec::Metadata {
            connections: metadata.num_connections,
            forwards: metadata.forwards,
            quick_commands: metadata.quick_commands,
            serial_profiles: metadata.serial_profiles,
            raw_tcp_profiles: metadata.raw_tcp_profiles,
            raw_udp_profiles: metadata.raw_udp_profiles,
            sensitive_credentials: metadata.sensitive_credentials,
            plugin_settings_count: metadata.plugin_settings_count,
            size,
        },
        None => CloudSyncRollbackBackupSummarySpec::SizeOnly(size),
    }
}

pub fn cloud_sync_select_label_key(label: CloudSyncSelectLabelKey) -> &'static str {
    match label {
        CloudSyncSelectLabelKey::BackendWebdav => "plugin.cloud_sync.backend.webdav",
        CloudSyncSelectLabelKey::BackendHttpJson => "plugin.cloud_sync.backend.http_json",
        CloudSyncSelectLabelKey::BackendDropbox => "plugin.cloud_sync.backend.dropbox",
        CloudSyncSelectLabelKey::BackendOneDrive => "plugin.cloud_sync.backend.onedrive",
        CloudSyncSelectLabelKey::BackendGoogleDrive => "plugin.cloud_sync.backend.google_drive",
        CloudSyncSelectLabelKey::BackendGithubGist => "plugin.cloud_sync.backend.github_gist",
        CloudSyncSelectLabelKey::BackendGit => "plugin.cloud_sync.backend.git",
        CloudSyncSelectLabelKey::BackendS3 => "plugin.cloud_sync.backend.s3",
        CloudSyncSelectLabelKey::AuthBearer => "plugin.cloud_sync.auth.bearer",
        CloudSyncSelectLabelKey::AuthBasic => "plugin.cloud_sync.auth.basic",
        CloudSyncSelectLabelKey::AuthNone => "plugin.cloud_sync.auth.none",
        CloudSyncSelectLabelKey::ConflictMerge => "plugin.cloud_sync.conflict.merge",
        CloudSyncSelectLabelKey::ConflictReplace => "plugin.cloud_sync.conflict.replace",
        CloudSyncSelectLabelKey::ConflictSkip => "plugin.cloud_sync.conflict.skip",
        CloudSyncSelectLabelKey::ConflictRename => "plugin.cloud_sync.conflict.rename",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncConfirmDescription {
    None,
    ClearSecret { label: String },
    RestoreBackup { created_at: String },
    DeleteBackup { created_at: String },
    ClearBackups,
    ClearHistory,
    EnableSensitiveSync,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncConfirmCopySpec {
    pub variant: ConfirmDialogVariant,
    pub title_key: &'static str,
    pub description: CloudSyncConfirmDescription,
    pub confirm_label_key: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CloudSyncApplySuccessCopySpec {
    pub title_key: &'static str,
    pub description_key: &'static str,
}

/// Legacy imports use different success copy when the source is a rollback backup.
pub fn cloud_sync_legacy_apply_success_copy_spec(
    source: &CloudSyncPreviewSource,
) -> CloudSyncApplySuccessCopySpec {
    if source.is_backup() {
        CloudSyncApplySuccessCopySpec {
            title_key: "plugin.cloud_sync.toast.restore_success_title",
            description_key: "plugin.cloud_sync.toast.restore_success_description",
        }
    } else {
        CloudSyncApplySuccessCopySpec {
            title_key: "plugin.cloud_sync.toast.pull_success_title",
            description_key: "plugin.cloud_sync.toast.pull_success_description",
        }
    }
}

pub fn cloud_sync_confirm_copy_spec(confirm: &CloudSyncConfirm) -> CloudSyncConfirmCopySpec {
    match confirm {
        CloudSyncConfirm::ImportPreview => CloudSyncConfirmCopySpec {
            variant: ConfirmDialogVariant::Default,
            title_key: "plugin.cloud_sync.confirm.import_title",
            description: CloudSyncConfirmDescription::None,
            confirm_label_key: "plugin.cloud_sync.actions.import_preview",
        },
        CloudSyncConfirm::ClearSecret { label, .. } => CloudSyncConfirmCopySpec {
            variant: ConfirmDialogVariant::Danger,
            title_key: "plugin.cloud_sync.confirm.clear_secret_title",
            description: CloudSyncConfirmDescription::ClearSecret {
                label: label.clone(),
            },
            confirm_label_key: "plugin.cloud_sync.actions.clear_secret",
        },
        CloudSyncConfirm::RestoreBackup { created_at, .. } => CloudSyncConfirmCopySpec {
            variant: ConfirmDialogVariant::Default,
            title_key: "plugin.cloud_sync.confirm.restore_backup_title",
            description: CloudSyncConfirmDescription::RestoreBackup {
                created_at: created_at.clone(),
            },
            confirm_label_key: "plugin.cloud_sync.actions.restore_backup",
        },
        CloudSyncConfirm::DeleteBackup { created_at, .. } => CloudSyncConfirmCopySpec {
            variant: ConfirmDialogVariant::Danger,
            title_key: "plugin.cloud_sync.confirm.delete_backup_title",
            description: CloudSyncConfirmDescription::DeleteBackup {
                created_at: created_at.clone(),
            },
            confirm_label_key: "plugin.cloud_sync.actions.delete_backup",
        },
        CloudSyncConfirm::ClearBackups => CloudSyncConfirmCopySpec {
            variant: ConfirmDialogVariant::Danger,
            title_key: "plugin.cloud_sync.confirm.clear_backups_title",
            description: CloudSyncConfirmDescription::ClearBackups,
            confirm_label_key: "plugin.cloud_sync.actions.clear_backups",
        },
        CloudSyncConfirm::ClearHistory => CloudSyncConfirmCopySpec {
            variant: ConfirmDialogVariant::Danger,
            title_key: "plugin.cloud_sync.confirm.clear_history_title",
            description: CloudSyncConfirmDescription::ClearHistory,
            confirm_label_key: "plugin.cloud_sync.actions.clear_history",
        },
        CloudSyncConfirm::EnableSensitiveSync => CloudSyncConfirmCopySpec {
            variant: ConfirmDialogVariant::Danger,
            title_key: "plugin.cloud_sync.confirm.enable_sensitive_sync_title",
            description: CloudSyncConfirmDescription::EnableSensitiveSync,
            confirm_label_key: "plugin.cloud_sync.actions.enable_sensitive_sync",
        },
    }
}

#[cfg(test)]
mod tests {
    use oxideterm_cloud_sync::state::{CloudSyncRollbackBackup, CloudSyncRollbackBackupMetadata};

    use super::*;

    #[test]
    fn maps_snapshot_limit_error_to_copy_spec() {
        let spec = cloud_sync_error_message_spec("snapshot_too_large: max 2097152 bytes");

        assert_eq!(
            spec,
            CloudSyncErrorMessageSpec::SnapshotTooLarge {
                limit: Some("2.0 MB".to_string())
            }
        );
    }

    #[test]
    fn maps_onedrive_and_microsoft_oauth_errors_to_copy_specs() {
        assert_eq!(
            cloud_sync_error_message_spec("onedrive_access_denied: tenant policy blocked access"),
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.onedrive_access_denied")
        );
        assert_eq!(
            cloud_sync_error_message_spec("microsoft_oauth_consent_required: admin consent needed"),
            CloudSyncErrorMessageSpec::Key(
                "plugin.cloud_sync.errors.microsoft_oauth_consent_required"
            )
        );
    }

    #[test]
    fn maps_google_drive_and_oauth_errors_to_copy_specs() {
        assert_eq!(
            cloud_sync_error_message_spec("google_drive_api_not_enabled: Drive API disabled"),
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_drive_api_not_enabled")
        );
        assert_eq!(
            cloud_sync_error_message_spec("google_oauth_admin_policy: blocked by admin"),
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_admin_policy")
        );
        assert_eq!(
            cloud_sync_error_message_spec("google_oauth_bad_client: wrong client type"),
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.google_oauth_bad_client")
        );
    }

    #[test]
    fn builds_backup_summary_from_metadata() {
        let backup = CloudSyncRollbackBackup {
            id: "backup-1".to_string(),
            created_at: "2026-05-26T00:00:00Z".to_string(),
            source_revision: Some("rev-1".to_string()),
            size_bytes: 1536,
            bytes_base64: "payload".to_string(),
            metadata: Some(CloudSyncRollbackBackupMetadata {
                num_connections: 3,
                connection_names: vec!["prod".to_string()],
                has_app_settings: true,
                plugin_settings_count: 2,
                forwards: 4,
                quick_commands: 5,
                serial_profiles: 6,
                raw_tcp_profiles: 7,
                raw_udp_profiles: 0,
                sensitive_credentials: 8,
            }),
        };

        assert_eq!(
            cloud_sync_rollback_backup_summary_spec(&backup),
            CloudSyncRollbackBackupSummarySpec::Metadata {
                connections: 3,
                forwards: 4,
                quick_commands: 5,
                serial_profiles: 6,
                raw_tcp_profiles: 7,
                raw_udp_profiles: 0,
                sensitive_credentials: 8,
                plugin_settings_count: 2,
                size: "1.5 KB".to_string()
            }
        );
    }

    #[test]
    fn picks_restore_copy_for_backup_apply_success() {
        let spec = cloud_sync_legacy_apply_success_copy_spec(&CloudSyncPreviewSource::Backup {
            id: "backup-1".to_string(),
            created_at: "2026-05-26T00:00:00Z".to_string(),
        });

        assert_eq!(
            spec,
            CloudSyncApplySuccessCopySpec {
                title_key: "plugin.cloud_sync.toast.restore_success_title",
                description_key: "plugin.cloud_sync.toast.restore_success_description",
            }
        );
    }
}
