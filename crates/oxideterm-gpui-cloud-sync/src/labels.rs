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
}

pub fn cloud_sync_backend_label_key(backend: &BackendType) -> &'static str {
    match backend {
        BackendType::Webdav => "plugin.cloud_sync.backend.webdav",
        BackendType::HttpJson => "plugin.cloud_sync.backend.http_json",
        BackendType::Dropbox => "plugin.cloud_sync.backend.dropbox",
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
        "http_unauthorized" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.http_unauthorized")
        }
        "network_request_failed" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.network_request_failed")
        }
        "missing_git_repository" => {
            CloudSyncErrorMessageSpec::Key("plugin.cloud_sync.errors.missing_git_repository")
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
            }),
        };

        assert_eq!(
            cloud_sync_rollback_backup_summary_spec(&backup),
            CloudSyncRollbackBackupSummarySpec::Metadata {
                connections: 3,
                forwards: 4,
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
