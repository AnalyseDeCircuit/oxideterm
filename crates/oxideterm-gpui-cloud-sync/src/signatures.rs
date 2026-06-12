// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Stable signatures for Cloud Sync virtual lists.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use oxideterm_cloud_sync::{
    AuthMode, BackendType, ConflictStrategy,
    state::{CloudSyncHistoryEntry, CloudSyncPersistedState, CloudSyncRollbackBackup},
};

use crate::CloudSyncSection;

pub fn cloud_sync_sections(
    state: &CloudSyncPersistedState,
    has_pending_preview: bool,
) -> Vec<CloudSyncSection> {
    let mut sections = vec![
        CloudSyncSection::Header,
        CloudSyncSection::Guide,
        CloudSyncSection::Status,
        CloudSyncSection::Actions,
    ];
    if has_pending_preview {
        sections.push(CloudSyncSection::Preview);
    }
    if !state.rollback_backups.is_empty() {
        sections.push(CloudSyncSection::Rollback);
    }
    sections.extend([
        CloudSyncSection::Config,
        CloudSyncSection::Notes,
        // History is retrospective information, so keep it after all active
        // configuration, scope, preview, and safety notes sections.
        CloudSyncSection::History,
    ]);
    sections
}

#[allow(clippy::too_many_arguments)]
pub fn cloud_sync_section_signature(
    section: CloudSyncSection,
    state: &CloudSyncPersistedState,
    backend_type: &BackendType,
    auth_mode: &AuthMode,
    conflict_strategy: &ConflictStrategy,
    busy: bool,
    has_pending_preview: bool,
    has_preview_selection: bool,
    has_progress: bool,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    section.hash(&mut hasher);
    match section {
        CloudSyncSection::Header => {
            format!("{:?}", state.status).hash(&mut hasher);
        }
        CloudSyncSection::Guide => {
            format!("{backend_type:?}").hash(&mut hasher);
        }
        CloudSyncSection::Status => {
            state.last_error.is_some().hash(&mut hasher);
            has_progress.hash(&mut hasher);
            state.last_sync_at.hash(&mut hasher);
            state.last_known_remote_revision.hash(&mut hasher);
        }
        CloudSyncSection::Actions => {
            busy.hash(&mut hasher);
            state.rollback_backups.len().hash(&mut hasher);
        }
        CloudSyncSection::Preview => {
            has_pending_preview.hash(&mut hasher);
            has_preview_selection.hash(&mut hasher);
        }
        CloudSyncSection::Rollback => {
            state.rollback_backups.len().hash(&mut hasher);
            busy.hash(&mut hasher);
        }
        CloudSyncSection::History => {
            state.sync_history.len().hash(&mut hasher);
        }
        CloudSyncSection::Config => {
            format!("{backend_type:?}").hash(&mut hasher);
            format!("{auth_mode:?}").hash(&mut hasher);
            format!("{conflict_strategy:?}").hash(&mut hasher);
            state.sync_scope.sync_connections.hash(&mut hasher);
            state.sync_scope.sync_forwards.hash(&mut hasher);
            state.sync_scope.sync_quick_commands.hash(&mut hasher);
            state.sync_scope.sync_serial_profiles.hash(&mut hasher);
            state
                .sync_scope
                .sync_sensitive_credentials
                .hash(&mut hasher);
            state.sync_scope.sync_app_settings.hash(&mut hasher);
            state.sync_scope.app_settings_sections.hash(&mut hasher);
            state
                .sync_scope
                .include_local_terminal_env_vars
                .hash(&mut hasher);
            state.sync_scope.sync_plugin_settings.hash(&mut hasher);
            state.sync_scope.plugin_ids.hash(&mut hasher);
        }
        CloudSyncSection::Notes => {
            state.sync_scope.sync_connections.hash(&mut hasher);
            state.sync_scope.sync_forwards.hash(&mut hasher);
            state.sync_scope.sync_app_settings.hash(&mut hasher);
            state.sync_scope.sync_plugin_settings.hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn cloud_sync_rollback_backup_signature(backup: &CloudSyncRollbackBackup) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Backups are keyed by id; visible metadata changes remeasure rows.
    backup.id.hash(&mut hasher);
    backup.created_at.hash(&mut hasher);
    backup.source_revision.hash(&mut hasher);
    backup.size_bytes.hash(&mut hasher);
    if let Some(metadata) = backup.metadata.as_ref() {
        metadata.num_connections.hash(&mut hasher);
        metadata.connection_names.hash(&mut hasher);
        metadata.has_app_settings.hash(&mut hasher);
        metadata.plugin_settings_count.hash(&mut hasher);
        metadata.forwards.hash(&mut hasher);
        metadata.quick_commands.hash(&mut hasher);
        metadata.serial_profiles.hash(&mut hasher);
        metadata.sensitive_credentials.hash(&mut hasher);
    }
    hasher.finish()
}

pub fn cloud_sync_history_signature(entry: &CloudSyncHistoryEntry) -> u64 {
    let mut hasher = DefaultHasher::new();
    // History rows display action, timestamp, success/error, summary, and remote revision.
    entry.id.hash(&mut hasher);
    entry.action.hash(&mut hasher);
    entry.timestamp.hash(&mut hasher);
    entry.success.hash(&mut hasher);
    entry.summary.connections.hash(&mut hasher);
    entry.summary.forwards.hash(&mut hasher);
    entry.summary.quick_commands.hash(&mut hasher);
    entry.summary.serial_profiles.hash(&mut hasher);
    entry.summary.sensitive_credentials.hash(&mut hasher);
    entry.summary.has_app_settings.hash(&mut hasher);
    entry.summary.plugin_settings_count.hash(&mut hasher);
    entry.error.hash(&mut hasher);
    entry.remote_revision.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rollback_backup() -> CloudSyncRollbackBackup {
        CloudSyncRollbackBackup {
            id: "backup-1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            source_revision: Some("rev-1".to_string()),
            size_bytes: 1,
            bytes_base64: "AA==".to_string(),
            metadata: None,
        }
    }

    #[test]
    fn cloud_sync_sections_put_history_after_active_configuration() {
        let state = CloudSyncPersistedState::default();

        assert_eq!(
            cloud_sync_sections(&state, false),
            vec![
                CloudSyncSection::Header,
                CloudSyncSection::Guide,
                CloudSyncSection::Status,
                CloudSyncSection::Actions,
                CloudSyncSection::Config,
                CloudSyncSection::Notes,
                CloudSyncSection::History,
            ]
        );
    }

    #[test]
    fn cloud_sync_sections_keep_history_last_with_preview_and_backups() {
        let mut state = CloudSyncPersistedState::default();
        state.rollback_backups.push(rollback_backup());

        let sections = cloud_sync_sections(&state, true);

        assert_eq!(sections.last(), Some(&CloudSyncSection::History));
        assert_eq!(
            sections,
            vec![
                CloudSyncSection::Header,
                CloudSyncSection::Guide,
                CloudSyncSection::Status,
                CloudSyncSection::Actions,
                CloudSyncSection::Preview,
                CloudSyncSection::Rollback,
                CloudSyncSection::Config,
                CloudSyncSection::Notes,
                CloudSyncSection::History,
            ]
        );
    }
}
