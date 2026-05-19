// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    CloudSyncSettings, CloudSyncStatus, LocalSyncMetadata, MAX_SYNC_HISTORY, RawSyncScope,
    StructuredDirtySections, StructuredLocalState, StructuredSectionRevisions, SyncScope,
    normalize_sync_scope,
};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncConflictDetails {
    pub revision: Option<String>,
    pub device_id: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncHistorySummary {
    pub connections: usize,
    pub forwards: usize,
    pub has_app_settings: bool,
    pub plugin_settings_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncHistoryEntry {
    pub id: String,
    pub action: String,
    pub timestamp: String,
    pub success: bool,
    pub summary: CloudSyncHistorySummary,
    pub error: Option<String>,
    pub remote_revision: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncRollbackBackupMetadata {
    pub num_connections: usize,
    #[serde(default)]
    pub connection_names: Vec<String>,
    pub has_app_settings: bool,
    pub plugin_settings_count: usize,
    pub forwards: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncRollbackBackup {
    pub id: String,
    pub created_at: String,
    pub source_revision: Option<String>,
    pub size_bytes: usize,
    pub bytes_base64: String,
    pub metadata: Option<CloudSyncRollbackBackupMetadata>,
}

impl CloudSyncHistoryEntry {
    pub fn new(
        action: impl Into<String>,
        summary: CloudSyncHistorySummary,
        success: bool,
        error: Option<String>,
        remote_revision: Option<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            action: action.into(),
            timestamp: Utc::now().to_rfc3339(),
            success,
            summary,
            error,
            remote_revision,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncPersistedState {
    #[serde(default)]
    pub settings: CloudSyncSettings,
    #[serde(default)]
    pub sync_scope: RawSyncScope,
    #[serde(default)]
    pub status: CloudSyncStatus,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub revision_seq: u64,
    #[serde(default)]
    pub last_sync_at: Option<String>,
    #[serde(default)]
    pub last_upload_at: Option<String>,
    #[serde(default)]
    pub last_check_at: Option<String>,
    #[serde(default)]
    pub last_known_remote_revision: Option<String>,
    #[serde(default)]
    pub last_known_remote_etag: Option<String>,
    #[serde(default)]
    pub remote_updated_at: Option<String>,
    #[serde(default)]
    pub remote_device_id: Option<String>,
    #[serde(default)]
    pub remote_format: Option<String>,
    #[serde(default)]
    pub remote_section_revisions: Option<StructuredSectionRevisions>,
    #[serde(default)]
    pub remote_exists: bool,
    #[serde(default)]
    pub last_synced_local_metadata: Option<LocalSyncMetadata>,
    #[serde(default)]
    pub last_synced_structured_state: Option<StructuredLocalState>,
    #[serde(default)]
    pub last_synced_remote_sections: Option<StructuredSectionRevisions>,
    #[serde(default)]
    pub local_dirty: bool,
    #[serde(default)]
    pub local_dirty_sections: Option<StructuredDirtySections>,
    #[serde(default)]
    pub auto_upload_blocked_by_conflict: bool,
    #[serde(default)]
    pub conflict_details: Option<CloudSyncConflictDetails>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub secret_hints: BTreeMap<String, bool>,
    #[serde(default)]
    pub sync_history: Vec<CloudSyncHistoryEntry>,
    #[serde(default)]
    pub rollback_backups: Vec<CloudSyncRollbackBackup>,
}

impl Default for CloudSyncPersistedState {
    fn default() -> Self {
        Self {
            settings: CloudSyncSettings::default(),
            sync_scope: RawSyncScope::default(),
            status: CloudSyncStatus::Idle,
            device_id: None,
            revision_seq: 0,
            last_sync_at: None,
            last_upload_at: None,
            last_check_at: None,
            last_known_remote_revision: None,
            last_known_remote_etag: None,
            remote_updated_at: None,
            remote_device_id: None,
            remote_format: None,
            remote_section_revisions: None,
            remote_exists: false,
            last_synced_local_metadata: None,
            last_synced_structured_state: None,
            last_synced_remote_sections: None,
            local_dirty: false,
            local_dirty_sections: None,
            auto_upload_blocked_by_conflict: false,
            conflict_details: None,
            last_error: None,
            secret_hints: BTreeMap::new(),
            sync_history: Vec::new(),
            rollback_backups: Vec::new(),
        }
    }
}

impl CloudSyncPersistedState {
    pub fn sync_scope(&self, available_plugin_ids: &[String]) -> SyncScope {
        normalize_sync_scope(Some(&self.sync_scope), available_plugin_ids)
    }

    pub fn ensure_device_id(&mut self, platform: &str) -> String {
        if let Some(device_id) = self.device_id.as_ref().filter(|id| !id.trim().is_empty()) {
            return device_id.clone();
        }
        let uuid = uuid::Uuid::new_v4().to_string();
        let device_id = format!("{platform}-{}", &uuid[..8]);
        self.device_id = Some(device_id.clone());
        device_id
    }

    pub fn next_revision_sequence(&mut self) -> u64 {
        self.revision_seq += 1;
        self.revision_seq
    }

    pub fn append_history(&mut self, entry: CloudSyncHistoryEntry) {
        self.sync_history.retain(|item| item.id != entry.id);
        self.sync_history.insert(0, entry);
        self.sync_history.truncate(MAX_SYNC_HISTORY);
    }

    pub fn append_rollback_backup(&mut self, backup: CloudSyncRollbackBackup) {
        self.rollback_backups.retain(|item| item.id != backup.id);
        self.rollback_backups.insert(0, backup);
        self.rollback_backups.truncate(crate::MAX_ROLLBACK_BACKUPS);
    }
}

#[derive(Clone, Debug)]
pub struct CloudSyncStateStore {
    path: PathBuf,
    state: CloudSyncPersistedState,
}

impl CloudSyncStateStore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let state = match fs::read_to_string(&path) {
            Ok(contents) if !contents.trim().is_empty() => serde_json::from_str(&contents)
                .with_context(|| format!("failed to parse cloud sync state {}", path.display()))?,
            Ok(_) => CloudSyncPersistedState::default(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                CloudSyncPersistedState::default()
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to read cloud sync state {}", path.display())
                });
            }
        };
        Ok(Self { path, state })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn state(&self) -> &CloudSyncPersistedState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut CloudSyncPersistedState {
        &mut self.state
    }

    pub fn replace_state(&mut self, state: CloudSyncPersistedState) {
        self.state = state;
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create cloud sync state dir {}", parent.display())
            })?;
        }
        let bytes = serde_json::to_vec_pretty(&self.state)
            .context("failed to serialize cloud sync state")?;
        fs::write(&self.path, bytes)
            .with_context(|| format!("failed to write cloud sync state {}", self.path.display()))
    }
}

pub fn default_cloud_sync_state_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .map(|parent| parent.join("cloud_sync.json"))
        .unwrap_or_else(|| PathBuf::from("cloud_sync.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_deduped_and_capped_like_tauri_storage() {
        let mut state = CloudSyncPersistedState::default();
        let first = CloudSyncHistoryEntry::new(
            "upload",
            CloudSyncHistorySummary::default(),
            true,
            None,
            Some("rev-1".into()),
        );
        let first_id = first.id.clone();
        state.append_history(first.clone());
        state.append_history(first);
        assert_eq!(state.sync_history.len(), 1);
        assert_eq!(state.sync_history[0].id, first_id);

        for index in 0..(MAX_SYNC_HISTORY + 3) {
            state.append_history(CloudSyncHistoryEntry {
                id: format!("id-{index}"),
                action: "check".into(),
                timestamp: "now".into(),
                success: true,
                summary: CloudSyncHistorySummary::default(),
                error: None,
                remote_revision: None,
            });
        }
        assert_eq!(state.sync_history.len(), MAX_SYNC_HISTORY);
        assert_eq!(
            state.sync_history[0].id,
            format!("id-{}", MAX_SYNC_HISTORY + 2)
        );
    }

    #[test]
    fn rollback_backups_are_deduped_and_capped_like_tauri_storage() {
        let mut state = CloudSyncPersistedState::default();
        let backup = CloudSyncRollbackBackup {
            id: "same".into(),
            created_at: "2026-05-19T00:00:00Z".into(),
            source_revision: Some("rev-1".into()),
            size_bytes: 4,
            bytes_base64: "dGVzdA==".into(),
            metadata: None,
        };
        state.append_rollback_backup(backup.clone());
        state.append_rollback_backup(backup);
        assert_eq!(state.rollback_backups.len(), 1);

        for index in 0..(crate::MAX_ROLLBACK_BACKUPS + 2) {
            state.append_rollback_backup(CloudSyncRollbackBackup {
                id: format!("backup-{index}"),
                created_at: "2026-05-19T00:00:00Z".into(),
                source_revision: None,
                size_bytes: 0,
                bytes_base64: String::new(),
                metadata: None,
            });
        }
        assert_eq!(state.rollback_backups.len(), crate::MAX_ROLLBACK_BACKUPS);
        assert_eq!(
            state.rollback_backups[0].id,
            format!("backup-{}", crate::MAX_ROLLBACK_BACKUPS + 1)
        );
    }
}
