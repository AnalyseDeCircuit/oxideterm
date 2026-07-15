// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use oxideterm_connections::{
    ConnectionStore, SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    SerialProfilesSyncSnapshot,
    oxide_file::{
        AppSettingsSectionPreview, EncryptedPortableSecret, ImportConflictStrategy, ImportPreview,
        ImportResultEnvelope, OxideExportOptions, OxideFile, OxideImportOptions, OxideMetadata,
        apply_oxide_import_with_options_with_progress, export_connections_to_oxide_with_progress,
        preflight_export, preview_oxide_import_with_progress,
    },
};
use oxideterm_forwarding::{ForwardingRegistry, SavedForwardsSyncSnapshot};
use oxideterm_quick_commands::{QuickCommand, QuickCommandCategory, QuickCommandsSnapshot};
use oxideterm_settings::{SettingsStore, export_oxide_settings_snapshot_json};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    BackendType, CloudSyncSettings, ConflictStrategy, RawSyncScope,
    STRUCTURED_MANIFEST_CONTENT_TYPE, STRUCTURED_MANIFEST_FORMAT, StructuredApplySelection,
    StructuredLocalState, StructuredManifest, StructuredManifestSections, StructuredObjectEntry,
    StructuredSectionRevisions,
    backend::{CloudSyncBackend, RemoteMetadata, RemoteUploadObject},
    connections_object_path, forwards_object_path,
    progress::{
        CloudSyncProgressSink, CloudSyncProgressStage, report_fractional_progress, report_progress,
    },
    quick_commands_object_path, revision_id, secret_keys,
    secrets::{CloudSyncSecretProvider, SecretReadMode, get_action_secrets},
    sensitive_credentials_object_path, serial_profiles_object_path,
    service::{
        CloudSyncApplyOutcome, CloudSyncLocalSnapshot, apply_structured_snapshots,
        build_local_snapshot,
    },
    state::CloudSyncHistorySummary,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncOperationKind {
    Check,
    Upload,
    Pull,
    ApplyPreview,
}

#[derive(Clone, Debug, Default)]
pub struct CloudSyncOperationGuard {
    active: Arc<Mutex<Option<CloudSyncOperationKind>>>,
}

impl CloudSyncOperationGuard {
    pub fn begin(
        &self,
        kind: CloudSyncOperationKind,
        skip_if_busy: bool,
    ) -> Result<Option<CloudSyncOperationPermit>> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.is_some() {
            if skip_if_busy {
                return Ok(None);
            }
            bail!("operation_in_progress: another cloud sync operation is already running");
        }
        *active = Some(kind);
        Ok(Some(CloudSyncOperationPermit {
            guard: self.clone(),
            kind,
        }))
    }

    fn finish(&self, kind: CloudSyncOperationKind) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *active == Some(kind) {
            *active = None;
        }
    }
}

#[derive(Debug)]
pub struct CloudSyncOperationPermit {
    guard: CloudSyncOperationGuard,
    kind: CloudSyncOperationKind,
}

impl Drop for CloudSyncOperationPermit {
    fn drop(&mut self) {
        self.guard.finish(self.kind);
    }
}

#[derive(Clone, Debug)]
pub struct CloudSyncOperationService {
    backend: CloudSyncBackend,
    guard: CloudSyncOperationGuard,
}

impl Default for CloudSyncOperationService {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Default)]
pub struct UploadOptions {
    pub automatic: bool,
    pub skip_if_busy: bool,
    pub force: bool,
    pub device_id: String,
    pub revision_sequence: u64,
    pub previous_remote_revision: Option<String>,
    pub previous_remote_sections: Option<StructuredSectionRevisions>,
    pub last_synced_structured_state: Option<StructuredLocalState>,
    pub raw_sync_scope: Option<RawSyncScope>,
    pub item_filter: StructuredUploadItemFilter,
    pub portable_secrets: Vec<EncryptedPortableSecret>,
}

#[derive(Clone, Debug, Default)]
pub struct StructuredUploadItemFilter {
    // A missing set means the whole resource group is selected; an empty set means upload no items.
    pub connection_ids: Option<BTreeSet<String>>,
    pub forward_ids: Option<BTreeSet<String>>,
    pub quick_command_ids: Option<BTreeSet<String>>,
    pub serial_profile_ids: Option<BTreeSet<String>>,
}

#[derive(Clone, Debug)]
pub struct UploadOutcome {
    pub revision: String,
    pub revision_sequence: u64,
    pub etag: Option<String>,
    pub local_snapshot: CloudSyncLocalSnapshot,
    pub manifest: crate::StructuredManifest,
    pub created_remote_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CloudSyncUploadError {
    pub message: String,
    pub remote_metadata: Option<RemoteMetadata>,
    pub revision_sequence_consumed: Option<u64>,
}

impl std::fmt::Display for CloudSyncUploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CloudSyncUploadError {}

impl From<anyhow::Error> for CloudSyncUploadError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            message: error.to_string(),
            remote_metadata: None,
            revision_sequence_consumed: None,
        }
    }
}

impl From<crate::secrets::CloudSyncSecretError> for CloudSyncUploadError {
    fn from(error: crate::secrets::CloudSyncSecretError) -> Self {
        Self {
            message: error.to_string(),
            remote_metadata: None,
            revision_sequence_consumed: None,
        }
    }
}

impl From<serde_json::Error> for CloudSyncUploadError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            remote_metadata: None,
            revision_sequence_consumed: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StructuredPreview {
    pub remote_metadata: RemoteMetadata,
    pub manifest: StructuredManifest,
    pub connections_snapshot: Option<SavedConnectionsSyncSnapshot>,
    pub forwards_snapshot: Option<SavedForwardsSyncSnapshot>,
    pub quick_commands_snapshot_json: Option<String>,
    pub serial_profiles_snapshot: Option<SerialProfilesSyncSnapshot>,
    pub base_connections_snapshot: Option<SavedConnectionsSyncSnapshot>,
    pub base_forwards_snapshot: Option<SavedForwardsSyncSnapshot>,
    pub base_quick_commands_snapshot_json: Option<String>,
    pub base_serial_profiles_snapshot: Option<SerialProfilesSyncSnapshot>,
    pub sensitive_credentials_entry: Option<Vec<u8>>,
    pub sensitive_credentials_preview: Option<ImportPreview>,
    pub app_settings_entries: std::collections::BTreeMap<String, Vec<u8>>,
    pub app_settings_sections: std::collections::BTreeMap<String, AppSettingsSectionPreview>,
    pub plugin_settings_entries: std::collections::BTreeMap<String, Vec<u8>>,
    pub plugin_settings_counts: std::collections::BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct LegacyPreview {
    pub remote_metadata: RemoteMetadata,
    pub bytes: Vec<u8>,
    pub metadata: OxideMetadata,
    pub preview: ImportPreview,
}

#[derive(Clone, Debug)]
pub struct ApplyStructuredPreviewOutcome {
    pub local_snapshot: CloudSyncLocalSnapshot,
    pub applied: CloudSyncApplyOutcome,
    pub sensitive_credentials_envelope: Option<ImportResultEnvelope>,
    pub content_summary: CloudSyncHistorySummary,
    pub manifest: StructuredManifest,
    pub remote_metadata: RemoteMetadata,
    pub selection: StructuredApplySelection,
    pub requires_upload_after_merge: bool,
}

#[derive(Clone, Debug)]
pub struct ApplyLegacyPreviewOutcome {
    pub envelope: ImportResultEnvelope,
}

impl StructuredPreview {
    pub fn full_selection(&self) -> StructuredApplySelection {
        StructuredApplySelection {
            connections: self.connections_snapshot.is_some(),
            forwards: self.forwards_snapshot.is_some(),
            quick_commands: self.quick_commands_snapshot_json.is_some(),
            serial_profiles: self.serial_profiles_snapshot.is_some(),
            sensitive_credentials: self.sensitive_credentials_entry.is_some(),
            app_settings_sections: self.app_settings_entries.keys().cloned().collect(),
            plugin_ids: self.plugin_settings_entries.keys().cloned().collect(),
        }
    }
}

mod apply;
mod merge;
mod objects;
mod preview;
mod selection;
mod service;
mod upload;
mod upload_plan;

pub use merge::*;
use selection::*;

#[cfg(test)]
mod tests;
