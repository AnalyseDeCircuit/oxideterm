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

use crate::{CloudSyncSection, CloudSyncTab};

pub fn cloud_sync_sections(
    state: &CloudSyncPersistedState,
    has_pending_preview: bool,
    active_tab: CloudSyncTab,
) -> Vec<CloudSyncSection> {
    let mut sections = vec![CloudSyncSection::Header];
    if has_pending_preview {
        sections.push(CloudSyncSection::Preview);
    }
    match active_tab {
        CloudSyncTab::Overview => {
            sections.push(CloudSyncSection::Status);
            sections.push(CloudSyncSection::RecentHistory);
            if !state.rollback_backups.is_empty() {
                sections.push(CloudSyncSection::Rollback);
            }
        }
        CloudSyncTab::Configure => {
            sections.push(CloudSyncSection::ConfigConnection);
            sections.push(CloudSyncSection::ConfigScope);
            sections.push(CloudSyncSection::ConfigCoverage);
            if state.local_dirty {
                sections.push(CloudSyncSection::ConfigPreflight);
            }
            sections.push(CloudSyncSection::ConfigHealth);
            sections.push(CloudSyncSection::ConfigNotes);
            sections.push(CloudSyncSection::Guide);
        }
        CloudSyncTab::History => {
            sections.push(CloudSyncSection::History);
            if !state.rollback_backups.is_empty() {
                sections.push(CloudSyncSection::Rollback);
            }
        }
    }
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
    active_tab: CloudSyncTab,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    section.hash(&mut hasher);
    // Tab switches can change which sections exist and how large they are, so
    // the active tab is part of every section signature.
    active_tab.hash(&mut hasher);
    match section {
        CloudSyncSection::Header => {}
        CloudSyncSection::Guide => {
            format!("{backend_type:?}").hash(&mut hasher);
        }
        CloudSyncSection::Status => {
            format!("{:?}", state.status).hash(&mut hasher);
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
        CloudSyncSection::History | CloudSyncSection::RecentHistory => {
            state.sync_history.len().hash(&mut hasher);
        }
        CloudSyncSection::ConfigConnection => {
            format!("{backend_type:?}").hash(&mut hasher);
            format!("{auth_mode:?}").hash(&mut hasher);
            format!("{conflict_strategy:?}").hash(&mut hasher);
        }
        CloudSyncSection::ConfigScope => {
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
        CloudSyncSection::ConfigCoverage | CloudSyncSection::ConfigNotes => {
            state.sync_scope.sync_connections.hash(&mut hasher);
            state.sync_scope.sync_forwards.hash(&mut hasher);
            state.sync_scope.sync_app_settings.hash(&mut hasher);
            state.sync_scope.sync_plugin_settings.hash(&mut hasher);
            state.sync_scope.app_settings_sections.hash(&mut hasher);
        }
        CloudSyncSection::ConfigPreflight => {
            state.local_dirty.hash(&mut hasher);
            state.revision_seq.hash(&mut hasher);
        }
        CloudSyncSection::ConfigHealth => {
            format!("{backend_type:?}").hash(&mut hasher);
            format!("{auth_mode:?}").hash(&mut hasher);
            state.sync_scope.sync_connections.hash(&mut hasher);
            state.sync_scope.sync_forwards.hash(&mut hasher);
            state.sync_scope.sync_app_settings.hash(&mut hasher);
            state.sync_scope.sync_plugin_settings.hash(&mut hasher);
            state.local_dirty.hash(&mut hasher);
            format!("{:?}", state.status).hash(&mut hasher);
            state.conflict_details.is_some().hash(&mut hasher);
            state.auto_upload_blocked_by_conflict.hash(&mut hasher);
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
    use oxideterm_cloud_sync::CloudSyncStatus;

    fn section_signature(section: CloudSyncSection, state: &CloudSyncPersistedState) -> u64 {
        cloud_sync_section_signature(
            section,
            state,
            &state.settings.backend_type,
            &state.settings.auth_mode,
            &state.settings.default_conflict_strategy,
            false,
            false,
            false,
            false,
            CloudSyncTab::Overview,
        )
    }

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
    fn cloud_sync_sections_overview_shows_status_and_recent_history() {
        let state = CloudSyncPersistedState::default();

        assert_eq!(
            cloud_sync_sections(&state, false, CloudSyncTab::Overview),
            vec![
                CloudSyncSection::Header,
                CloudSyncSection::Status,
                CloudSyncSection::RecentHistory,
            ]
        );
    }

    #[test]
    fn cloud_sync_sections_configure_shows_config_and_guide() {
        let state = CloudSyncPersistedState::default();

        assert_eq!(
            cloud_sync_sections(&state, false, CloudSyncTab::Configure),
            vec![
                CloudSyncSection::Header,
                CloudSyncSection::ConfigConnection,
                CloudSyncSection::ConfigScope,
                CloudSyncSection::ConfigCoverage,
                CloudSyncSection::ConfigHealth,
                CloudSyncSection::ConfigNotes,
                CloudSyncSection::Guide,
            ]
        );
    }

    #[test]
    fn cloud_sync_sections_history_keeps_history_and_backups() {
        let mut state = CloudSyncPersistedState::default();
        state.rollback_backups.push(rollback_backup());

        let sections = cloud_sync_sections(&state, true, CloudSyncTab::History);

        assert_eq!(sections.last(), Some(&CloudSyncSection::Rollback));
        assert_eq!(
            sections,
            vec![
                CloudSyncSection::Header,
                CloudSyncSection::Preview,
                CloudSyncSection::History,
                CloudSyncSection::Rollback,
            ]
        );
    }

    #[test]
    fn status_changes_remeasure_the_overview_card_instead_of_the_page_header() {
        let idle = CloudSyncPersistedState::default();
        let mut uploading = idle.clone();
        uploading.status = CloudSyncStatus::Uploading;

        assert_eq!(
            section_signature(CloudSyncSection::Header, &idle),
            section_signature(CloudSyncSection::Header, &uploading),
        );
        assert_ne!(
            section_signature(CloudSyncSection::Status, &idle),
            section_signature(CloudSyncSection::Status, &uploading),
        );
    }
}
