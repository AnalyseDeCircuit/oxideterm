// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Compatibility model for Oxide Cloud Sync.
//!
//! The native app exposes Cloud Sync as a built-in tab, but the remote
//! manifest, scope, state, and baseline semantics are specified by the Tauri
//! `com.oxideterm.cloud-sync` plugin. Keep this crate UI-free so format
//! compatibility can be tested independently of GPUI.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub mod backend;
pub mod operation;
pub mod plugin_settings;
pub mod progress;
pub mod secrets;
pub mod service;
pub mod state;
pub mod state_transitions;

pub const CLOUD_SYNC_PLUGIN_ID: &str = "com.oxideterm.cloud-sync";

pub const OXIDE_CONTENT_TYPE: &str = "application/vnd.oxideterm.oxide";
pub const STRUCTURED_MANIFEST_FORMAT: &str = "structured-v1";
pub const STRUCTURED_MANIFEST_CONTENT_TYPE: &str =
    "application/vnd.oxideterm.cloud-sync.manifest+json";

pub const MAX_REMOTE_SNAPSHOT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_ROLLBACK_BACKUP_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ROLLBACK_BACKUPS: usize = 5;
pub const MAX_SYNC_HISTORY: usize = 50;
pub const PREVIEW_RECORD_LIMIT: usize = 8;

pub const APP_SECTION_GENERAL: &str = "general";
pub const APP_SECTION_TERMINAL_APPEARANCE: &str = "terminalAppearance";
pub const APP_SECTION_TERMINAL_BEHAVIOR: &str = "terminalBehavior";
pub const APP_SECTION_APPEARANCE: &str = "appearance";
pub const APP_SECTION_CONNECTIONS: &str = "connections";
pub const APP_SECTION_NETWORK: &str = "network";
pub const APP_SECTION_FILE_AND_EDITOR: &str = "fileAndEditor";
pub const APP_SECTION_AI: &str = "ai";
pub const APP_SECTION_LOCAL_TERMINAL: &str = "localTerminal";
pub const APP_SECTION_NATIVE_PREFERENCES: &str = "nativePreferences";

pub const OXIDE_APP_SETTINGS_SECTION_IDS: &[&str] = &[
    APP_SECTION_GENERAL,
    APP_SECTION_TERMINAL_APPEARANCE,
    APP_SECTION_TERMINAL_BEHAVIOR,
    APP_SECTION_APPEARANCE,
    APP_SECTION_CONNECTIONS,
    APP_SECTION_NETWORK,
    APP_SECTION_FILE_AND_EDITOR,
    APP_SECTION_AI,
    APP_SECTION_LOCAL_TERMINAL,
    APP_SECTION_NATIVE_PREFERENCES,
];

pub const DEFAULT_APP_SETTINGS_SECTIONS: &[&str] = &[
    APP_SECTION_GENERAL,
    APP_SECTION_TERMINAL_APPEARANCE,
    APP_SECTION_TERMINAL_BEHAVIOR,
    APP_SECTION_APPEARANCE,
    APP_SECTION_CONNECTIONS,
    APP_SECTION_NETWORK,
    APP_SECTION_FILE_AND_EDITOR,
];

pub mod storage_keys {
    pub const DEVICE_ID: &str = "device-id";
    pub const REVISION_SEQ: &str = "revision-seq";
    pub const LAST_KNOWN_REMOTE_REVISION: &str = "last-known-remote-revision";
    pub const LAST_KNOWN_REMOTE_ETAG: &str = "last-known-remote-etag";
    pub const LAST_SYNC_AT: &str = "last-sync-at";
    pub const LAST_UPLOAD_AT: &str = "last-upload-at";
    pub const LAST_CHECK_AT: &str = "last-check-at";
    pub const LAST_SYNCED_LOCAL_METADATA: &str = "last-synced-local-metadata";
    pub const LAST_SYNCED_STRUCTURED_STATE: &str = "last-synced-structured-state";
    pub const LAST_SYNCED_REMOTE_SECTIONS: &str = "last-synced-remote-sections";
    pub const ROLLBACK_BACKUP: &str = "rollback-backup";
    pub const ROLLBACK_BACKUPS: &str = "rollback-backups";
    pub const SYNC_HISTORY: &str = "sync-history";
    pub const SECRET_HINTS: &str = "secret-hints";
    pub const SYNC_SCOPE: &str = "sync-scope";
}

pub mod secret_keys {
    pub const SYNC_PASSWORD: &str = "sync-password";
    pub const TOKEN: &str = "backend-token";
    pub const GIT_TOKEN: &str = "git-backend-token";
    pub const BASIC_USERNAME: &str = "basic-username";
    pub const BASIC_PASSWORD: &str = "basic-password";
    pub const ACCESS_KEY_ID: &str = "s3-access-key-id";
    pub const SECRET_ACCESS_KEY: &str = "s3-secret-access-key";
    pub const SESSION_TOKEN: &str = "s3-session-token";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudSyncStatus {
    Idle,
    Uploading,
    Checking,
    RemoteUpdate,
    Conflict,
    Error,
}

impl Default for CloudSyncStatus {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendType {
    Webdav,
    HttpJson,
    Dropbox,
    GithubGist,
    S3,
    Git,
}

impl Default for BackendType {
    fn default() -> Self {
        Self::Webdav
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthMode {
    Bearer,
    Basic,
    None,
}

impl Default for AuthMode {
    fn default() -> Self {
        Self::Bearer
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncSettings {
    #[serde(default)]
    pub backend_type: BackendType,
    #[serde(default)]
    pub auth_mode: AuthMode,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default)]
    pub s3_bucket: String,
    #[serde(default = "default_s3_region")]
    pub s3_region: String,
    #[serde(default)]
    pub git_repository: String,
    #[serde(default = "default_git_branch")]
    pub git_branch: String,
    #[serde(default)]
    pub github_oauth_client_id: String,
    #[serde(default)]
    pub auto_upload_enabled: bool,
    #[serde(default = "default_auto_upload_interval_mins")]
    pub auto_upload_interval_mins: f64,
    #[serde(default)]
    pub default_conflict_strategy: ConflictStrategy,
}

impl Default for CloudSyncSettings {
    fn default() -> Self {
        Self {
            backend_type: BackendType::default(),
            auth_mode: AuthMode::default(),
            endpoint: String::new(),
            namespace: default_namespace(),
            s3_bucket: String::new(),
            s3_region: default_s3_region(),
            git_repository: String::new(),
            git_branch: default_git_branch(),
            github_oauth_client_id: String::new(),
            auto_upload_enabled: false,
            auto_upload_interval_mins: default_auto_upload_interval_mins(),
            default_conflict_strategy: ConflictStrategy::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictStrategy {
    Merge,
    Replace,
    Skip,
    Rename,
}

impl Default for ConflictStrategy {
    fn default() -> Self {
        Self::Merge
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSyncScope {
    #[serde(default)]
    pub sync_connections: Option<bool>,
    #[serde(default)]
    pub sync_forwards: Option<bool>,
    #[serde(default)]
    pub sync_quick_commands: Option<bool>,
    #[serde(default)]
    pub sync_serial_profiles: Option<bool>,
    #[serde(default)]
    pub sync_sensitive_credentials: Option<bool>,
    #[serde(default)]
    pub sync_app_settings: Option<bool>,
    #[serde(default)]
    pub app_settings_sections: Option<Vec<String>>,
    #[serde(default)]
    pub include_local_terminal_env_vars: Option<bool>,
    #[serde(default)]
    pub sync_plugin_settings: Option<bool>,
    #[serde(default)]
    pub plugin_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncScope {
    #[serde(default = "default_true")]
    pub sync_connections: bool,
    #[serde(default = "default_true")]
    pub sync_forwards: bool,
    #[serde(default = "default_true")]
    pub sync_quick_commands: bool,
    #[serde(default = "default_true")]
    pub sync_serial_profiles: bool,
    #[serde(default)]
    pub sync_sensitive_credentials: bool,
    #[serde(default = "default_true")]
    pub sync_app_settings: bool,
    #[serde(default = "default_app_settings_section_ids")]
    pub app_settings_sections: Vec<String>,
    #[serde(default)]
    pub include_local_terminal_env_vars: bool,
    #[serde(default = "default_true")]
    pub sync_plugin_settings: bool,
    #[serde(default)]
    pub plugin_ids: Option<Vec<String>>,
}

impl Default for SyncScope {
    fn default() -> Self {
        normalize_sync_scope(None, &[])
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSyncMetadata {
    #[serde(default)]
    pub saved_connections_revision: Option<String>,
    #[serde(default)]
    pub saved_forwards_revision: Option<String>,
    #[serde(default)]
    pub quick_commands_revision: Option<String>,
    #[serde(default)]
    pub serial_profiles_revision: Option<String>,
    #[serde(default)]
    pub sensitive_credentials_revision: Option<String>,
    #[serde(default)]
    pub settings_revision: Option<String>,
    #[serde(default)]
    pub app_settings_section_revisions: BTreeMap<String, String>,
    #[serde(default)]
    pub plugin_settings_revisions: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredLocalState {
    #[serde(default)]
    pub connections: Option<String>,
    #[serde(default)]
    pub forwards: Option<String>,
    #[serde(default)]
    pub quick_commands: Option<String>,
    #[serde(default)]
    pub serial_profiles: Option<String>,
    #[serde(default)]
    pub sensitive_credentials: Option<String>,
    #[serde(default)]
    pub app_settings: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub plugin_settings: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredDirtySections {
    #[serde(default)]
    pub connections: bool,
    #[serde(default)]
    pub forwards: bool,
    #[serde(default)]
    pub quick_commands: bool,
    #[serde(default)]
    pub serial_profiles: bool,
    #[serde(default)]
    pub sensitive_credentials: bool,
    #[serde(default)]
    pub app_settings: BTreeMap<String, bool>,
    #[serde(default)]
    pub plugin_settings: BTreeMap<String, bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredDirtyInfo {
    pub current_state: StructuredLocalState,
    pub dirty_sections: StructuredDirtySections,
    pub has_dirty: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredSectionRevisions {
    #[serde(default)]
    pub connections: Option<String>,
    #[serde(default)]
    pub forwards: Option<String>,
    #[serde(default)]
    pub quick_commands: Option<String>,
    #[serde(default)]
    pub serial_profiles: Option<String>,
    #[serde(default)]
    pub sensitive_credentials: Option<String>,
    #[serde(default)]
    pub app_settings: BTreeMap<String, String>,
    #[serde(default)]
    pub plugin_settings: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredApplySelection {
    #[serde(default)]
    pub connections: bool,
    #[serde(default)]
    pub forwards: bool,
    #[serde(default)]
    pub quick_commands: bool,
    #[serde(default)]
    pub serial_profiles: bool,
    #[serde(default)]
    pub sensitive_credentials: bool,
    #[serde(default)]
    pub app_settings_sections: Vec<String>,
    #[serde(default)]
    pub plugin_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredObjectEntry {
    pub revision: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_count: Option<usize>,
    pub content_type: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredManifestSections {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<StructuredObjectEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forwards: Option<StructuredObjectEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick_commands: Option<StructuredObjectEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial_profiles: Option<StructuredObjectEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitive_credentials: Option<StructuredObjectEntry>,
    #[serde(default)]
    pub app_settings: BTreeMap<String, StructuredObjectEntry>,
    #[serde(default)]
    pub plugin_settings: BTreeMap<String, StructuredObjectEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredManifest {
    pub format: String,
    pub revision: String,
    pub uploaded_at: String,
    pub device_id: String,
    pub content_type: String,
    pub scope: SyncScope,
    pub sections: StructuredManifestSections,
    #[serde(default)]
    pub section_revisions: StructuredSectionRevisions,
}

pub fn normalize_sync_scope(
    scope: Option<&RawSyncScope>,
    available_plugin_ids: &[String],
) -> SyncScope {
    let raw_app_settings_sections = scope
        .and_then(|scope| scope.app_settings_sections.clone())
        .unwrap_or_else(|| {
            DEFAULT_APP_SETTINGS_SECTIONS
                .iter()
                .map(|section_id| (*section_id).to_string())
                .collect()
        });
    let app_settings_sections = unique_known_app_sections(&raw_app_settings_sections);

    let syncable_available_plugin_ids = get_syncable_plugin_ids(available_plugin_ids);
    let plugin_ids = scope
        .and_then(|scope| scope.plugin_ids.as_deref())
        .map(get_syncable_plugin_ids)
        .or_else(|| {
            if syncable_available_plugin_ids.is_empty() {
                None
            } else {
                Some(syncable_available_plugin_ids)
            }
        });

    SyncScope {
        sync_connections: scope
            .and_then(|scope| scope.sync_connections)
            .unwrap_or(true),
        sync_forwards: scope.and_then(|scope| scope.sync_forwards).unwrap_or(true),
        sync_quick_commands: scope
            .and_then(|scope| scope.sync_quick_commands)
            .unwrap_or(true),
        sync_serial_profiles: scope
            .and_then(|scope| scope.sync_serial_profiles)
            .unwrap_or(true),
        sync_sensitive_credentials: scope
            .and_then(|scope| scope.sync_sensitive_credentials)
            .unwrap_or(false),
        sync_app_settings: scope
            .and_then(|scope| scope.sync_app_settings)
            .unwrap_or(true),
        app_settings_sections,
        include_local_terminal_env_vars: scope
            .and_then(|scope| scope.include_local_terminal_env_vars)
            .unwrap_or(false),
        sync_plugin_settings: scope
            .and_then(|scope| scope.sync_plugin_settings)
            .unwrap_or(true),
        plugin_ids,
    }
}

pub fn get_syncable_plugin_ids(plugin_ids: &[String]) -> Vec<String> {
    plugin_ids
        .iter()
        .filter_map(|plugin_id| {
            let trimmed = plugin_id.trim();
            (!trimmed.is_empty() && trimmed != CLOUD_SYNC_PLUGIN_ID).then(|| trimmed.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn build_structured_local_state(
    local_metadata: &LocalSyncMetadata,
    scope: &SyncScope,
) -> StructuredLocalState {
    let mut app_settings = BTreeMap::new();
    if scope.sync_app_settings {
        for section_id in &scope.app_settings_sections {
            app_settings.insert(
                section_id.clone(),
                local_metadata
                    .app_settings_section_revisions
                    .get(section_id)
                    .cloned(),
            );
        }
    }

    let mut plugin_settings = BTreeMap::new();
    if scope.sync_plugin_settings {
        for plugin_id in scoped_plugin_ids(local_metadata, scope) {
            plugin_settings.insert(
                plugin_id.clone(),
                local_metadata
                    .plugin_settings_revisions
                    .get(&plugin_id)
                    .cloned(),
            );
        }
    }

    StructuredLocalState {
        connections: scope
            .sync_connections
            .then(|| local_metadata.saved_connections_revision.clone())
            .flatten(),
        forwards: scope
            .sync_forwards
            .then(|| local_metadata.saved_forwards_revision.clone())
            .flatten(),
        quick_commands: scope
            .sync_quick_commands
            .then(|| local_metadata.quick_commands_revision.clone())
            .flatten(),
        serial_profiles: scope
            .sync_serial_profiles
            .then(|| local_metadata.serial_profiles_revision.clone())
            .flatten(),
        sensitive_credentials: scope
            .sync_sensitive_credentials
            .then(|| local_metadata.sensitive_credentials_revision.clone())
            .flatten(),
        app_settings,
        plugin_settings,
    }
}

pub fn compute_structured_dirty_sections(
    local_metadata: &LocalSyncMetadata,
    baseline_state: Option<&StructuredLocalState>,
    scope: &SyncScope,
) -> StructuredDirtyInfo {
    let current_state = build_structured_local_state(local_metadata, scope);
    let mut dirty_sections = StructuredDirtySections {
        connections: scope.sync_connections
            && current_state.connections
                != baseline_state.and_then(|state| state.connections.clone()),
        forwards: scope.sync_forwards
            && current_state.forwards != baseline_state.and_then(|state| state.forwards.clone()),
        quick_commands: scope.sync_quick_commands
            && current_state.quick_commands
                != baseline_state.and_then(|state| state.quick_commands.clone()),
        serial_profiles: scope.sync_serial_profiles
            && current_state.serial_profiles
                != baseline_state.and_then(|state| state.serial_profiles.clone()),
        sensitive_credentials: scope.sync_sensitive_credentials
            && current_state.sensitive_credentials
                != baseline_state.and_then(|state| state.sensitive_credentials.clone()),
        ..StructuredDirtySections::default()
    };

    for section_id in &scope.app_settings_sections {
        let current = current_state
            .app_settings
            .get(section_id)
            .cloned()
            .flatten();
        let baseline = baseline_state
            .and_then(|state| state.app_settings.get(section_id))
            .cloned()
            .flatten();
        dirty_sections.app_settings.insert(
            section_id.clone(),
            scope.sync_app_settings && current != baseline,
        );
    }

    let plugin_ids_for_dirty = scope.plugin_ids.as_ref().map_or_else(
        || {
            get_syncable_plugin_ids(
                &current_state
                    .plugin_settings
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        },
        |plugin_ids| get_syncable_plugin_ids(plugin_ids),
    );
    for plugin_id in plugin_ids_for_dirty {
        let current = current_state
            .plugin_settings
            .get(&plugin_id)
            .cloned()
            .flatten();
        let baseline = baseline_state
            .and_then(|state| state.plugin_settings.get(&plugin_id))
            .cloned()
            .flatten();
        dirty_sections.plugin_settings.insert(
            plugin_id.clone(),
            scope.sync_plugin_settings && current != baseline,
        );
    }

    let has_dirty = dirty_sections.connections
        || dirty_sections.forwards
        || dirty_sections.quick_commands
        || dirty_sections.serial_profiles
        || dirty_sections.sensitive_credentials
        || dirty_sections.app_settings.values().any(|dirty| *dirty)
        || dirty_sections.plugin_settings.values().any(|dirty| *dirty);

    StructuredDirtyInfo {
        current_state,
        dirty_sections,
        has_dirty,
    }
}

pub fn create_manifest_base(
    revision: impl Into<String>,
    uploaded_at: impl Into<String>,
    device_id: impl Into<String>,
    scope: SyncScope,
) -> StructuredManifest {
    StructuredManifest {
        format: STRUCTURED_MANIFEST_FORMAT.to_string(),
        revision: revision.into(),
        uploaded_at: uploaded_at.into(),
        device_id: device_id.into(),
        content_type: STRUCTURED_MANIFEST_CONTENT_TYPE.to_string(),
        scope,
        sections: StructuredManifestSections::default(),
        section_revisions: StructuredSectionRevisions::default(),
    }
}

pub fn build_manifest_section_revisions(
    manifest: &StructuredManifest,
) -> StructuredSectionRevisions {
    StructuredSectionRevisions {
        connections: manifest
            .sections
            .connections
            .as_ref()
            .map(|entry| entry.revision.clone()),
        forwards: manifest
            .sections
            .forwards
            .as_ref()
            .map(|entry| entry.revision.clone()),
        quick_commands: manifest
            .sections
            .quick_commands
            .as_ref()
            .map(|entry| entry.revision.clone()),
        serial_profiles: manifest
            .sections
            .serial_profiles
            .as_ref()
            .map(|entry| entry.revision.clone()),
        sensitive_credentials: manifest
            .sections
            .sensitive_credentials
            .as_ref()
            .map(|entry| entry.revision.clone()),
        app_settings: manifest
            .sections
            .app_settings
            .iter()
            .map(|(section_id, entry)| (section_id.clone(), entry.revision.clone()))
            .collect(),
        plugin_settings: manifest
            .sections
            .plugin_settings
            .iter()
            .map(|(plugin_id, entry)| (plugin_id.clone(), entry.revision.clone()))
            .collect(),
    }
}

pub fn merge_structured_baseline(
    previous_state: Option<&StructuredLocalState>,
    next_state: &StructuredLocalState,
    selection: &StructuredApplySelection,
) -> StructuredLocalState {
    let mut merged = previous_state.cloned().unwrap_or_default();

    if selection.connections {
        merged.connections = next_state.connections.clone();
    }
    if selection.forwards {
        merged.forwards = next_state.forwards.clone();
    }
    if selection.quick_commands {
        merged.quick_commands = next_state.quick_commands.clone();
    }
    if selection.serial_profiles {
        merged.serial_profiles = next_state.serial_profiles.clone();
    }
    if selection.sensitive_credentials {
        merged.sensitive_credentials = next_state.sensitive_credentials.clone();
    }
    for section_id in &selection.app_settings_sections {
        merged.app_settings.insert(
            section_id.clone(),
            next_state.app_settings.get(section_id).cloned().flatten(),
        );
    }
    for plugin_id in &selection.plugin_ids {
        merged.plugin_settings.insert(
            plugin_id.clone(),
            next_state.plugin_settings.get(plugin_id).cloned().flatten(),
        );
    }

    merged
}

pub fn count_structured_upload_plan_units(
    local_metadata: &LocalSyncMetadata,
    scope: &SyncScope,
) -> usize {
    let mut total = 0;
    if scope.sync_connections {
        total += 1;
    }
    if scope.sync_forwards {
        total += 1;
    }
    if scope.sync_quick_commands {
        total += usize::from(local_metadata.quick_commands_revision.is_some());
    }
    if scope.sync_serial_profiles {
        total += usize::from(local_metadata.serial_profiles_revision.is_some());
    }
    if scope.sync_sensitive_credentials {
        total += usize::from(local_metadata.sensitive_credentials_revision.is_some());
    }
    if scope.sync_app_settings {
        total += scope
            .app_settings_sections
            .iter()
            .filter(|section_id| {
                local_metadata
                    .app_settings_section_revisions
                    .contains_key(*section_id)
            })
            .count();
    }
    if scope.sync_plugin_settings {
        total += scoped_plugin_ids(local_metadata, scope)
            .into_iter()
            .filter(|plugin_id| {
                local_metadata
                    .plugin_settings_revisions
                    .contains_key(plugin_id)
            })
            .count();
    }
    total
}

pub fn connections_object_path(revision: &str) -> String {
    format!("structured/connections/{revision}.json")
}

pub fn forwards_object_path(revision: &str) -> String {
    format!("structured/forwards/{revision}.json")
}

pub fn quick_commands_object_path(revision: &str) -> String {
    format!("structured/quick-commands/{revision}.json")
}

pub fn serial_profiles_object_path(revision: &str) -> String {
    format!("structured/serial-profiles/{revision}.json")
}

pub fn sensitive_credentials_object_path(revision: &str) -> String {
    format!("structured/sensitive-credentials/{revision}.oxide")
}

pub fn app_settings_object_path(section_id: &str, revision: &str) -> String {
    format!("structured/settings/app/{section_id}/{revision}.oxide")
}

pub fn plugin_settings_object_path(plugin_id: &str, revision: &str) -> String {
    format!("structured/settings/plugins/{plugin_id}/{revision}.oxide")
}

pub fn snapshot_object_paths(namespace: &str) -> SnapshotObjectPaths {
    let prefix = trim_slashes(namespace);
    SnapshotObjectPaths {
        metadata_key: join_key(&[prefix.as_deref(), Some("latest.json")]),
        blob_key: join_key(&[prefix.as_deref(), Some("latest.oxide")]),
    }
}

pub fn s3_revision_blob_key(namespace: &str, revision: &str) -> String {
    let prefix = trim_slashes(namespace);
    join_key(&[
        prefix.as_deref(),
        Some("blobs"),
        Some(&format!("{revision}.oxide")),
    ])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotObjectPaths {
    pub metadata_key: String,
    pub blob_key: String,
}

pub fn format_revision_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn revision_id(timestamp: DateTime<Utc>, device_id: &str, sequence: u64) -> String {
    format!(
        "{}-{}-{}",
        format_revision_timestamp(timestamp),
        device_id,
        format!("{sequence:03}")
    )
}

pub fn next_revision(
    timestamp: DateTime<Utc>,
    device_id: &str,
    previous_sequence: u64,
) -> (String, u64) {
    let sequence = previous_sequence + 1;
    (revision_id(timestamp, device_id, sequence), sequence)
}

fn default_namespace() -> String {
    "default".to_string()
}

fn default_true() -> bool {
    // Legacy persisted sync scopes predate several optional structured sections.
    true
}

fn default_app_settings_section_ids() -> Vec<String> {
    // Missing app section lists should behave like a newly created sync scope.
    DEFAULT_APP_SETTINGS_SECTIONS
        .iter()
        .map(|section_id| (*section_id).to_string())
        .collect()
}

fn default_s3_region() -> String {
    "auto".to_string()
}

fn default_git_branch() -> String {
    "main".to_string()
}

fn default_auto_upload_interval_mins() -> f64 {
    60.0
}

fn unique_known_app_sections(sections: &[String]) -> Vec<String> {
    let known = OXIDE_APP_SETTINGS_SECTION_IDS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for section in sections {
        let section = section.as_str();
        if known.contains(section) && seen.insert(section.to_string()) {
            result.push(section.to_string());
        }
    }
    result
}

fn scoped_plugin_ids(local_metadata: &LocalSyncMetadata, scope: &SyncScope) -> Vec<String> {
    match scope.plugin_ids.as_deref() {
        Some(plugin_ids) => get_syncable_plugin_ids(plugin_ids),
        None => get_syncable_plugin_ids(
            &local_metadata
                .plugin_settings_revisions
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
        ),
    }
}

fn trim_slashes(value: &str) -> Option<String> {
    let trimmed = value.trim_matches('/');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn join_key(parts: &[Option<&str>]) -> String {
    parts
        .iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .map(|part| part.trim_matches('/'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn normalizes_default_scope_like_tauri_plugin() {
        let available = strings(&["zeta", CLOUD_SYNC_PLUGIN_ID, "alpha", "alpha"]);
        let scope = normalize_sync_scope(None, &available);

        assert!(scope.sync_connections);
        assert!(scope.sync_forwards);
        assert!(scope.sync_app_settings);
        assert_eq!(scope.app_settings_sections, DEFAULT_APP_SETTINGS_SECTIONS);
        assert!(!scope.include_local_terminal_env_vars);
        assert!(scope.sync_plugin_settings);
        assert_eq!(scope.plugin_ids, Some(strings(&["alpha", "zeta"])));
    }

    #[test]
    fn normalizes_partial_scope_and_filters_unknown_sections() {
        let raw = RawSyncScope {
            sync_connections: Some(false),
            app_settings_sections: Some(strings(&[
                "general",
                "unknown",
                "localTerminal",
                "general",
            ])),
            plugin_ids: Some(strings(&[
                CLOUD_SYNC_PLUGIN_ID,
                "plugin-b",
                "",
                "plugin-a",
                "plugin-a",
            ])),
            ..RawSyncScope::default()
        };

        let scope = normalize_sync_scope(Some(&raw), &[]);

        assert!(!scope.sync_connections);
        assert!(scope.sync_forwards);
        assert_eq!(
            scope.app_settings_sections,
            strings(&["general", "localTerminal"])
        );
        assert_eq!(scope.plugin_ids, Some(strings(&["plugin-a", "plugin-b"])));
    }

    #[test]
    fn preserves_fractional_auto_upload_interval_like_tauri_settings() {
        let settings: CloudSyncSettings =
            serde_json::from_str(r#"{"autoUploadIntervalMins":7.5}"#).unwrap();

        assert_eq!(settings.auto_upload_interval_mins, 7.5);
        assert_eq!(
            serde_json::to_value(&settings).unwrap()["autoUploadIntervalMins"],
            serde_json::json!(7.5)
        );
    }

    #[test]
    fn computes_dirty_sections_against_structured_baseline() {
        let scope = SyncScope {
            plugin_ids: Some(strings(&["plugin-a"])),
            ..SyncScope::default()
        };
        let metadata = LocalSyncMetadata {
            saved_connections_revision: Some("conn-2".into()),
            saved_forwards_revision: Some("fwd-1".into()),
            app_settings_section_revisions: BTreeMap::from([
                ("general".into(), "gen-2".into()),
                ("appearance".into(), "app-1".into()),
            ]),
            plugin_settings_revisions: BTreeMap::from([
                ("plugin-a".into(), "pa-1".into()),
                (CLOUD_SYNC_PLUGIN_ID.into(), "self".into()),
            ]),
            ..LocalSyncMetadata::default()
        };
        let baseline = StructuredLocalState {
            connections: Some("conn-1".into()),
            forwards: Some("fwd-1".into()),
            quick_commands: None,
            serial_profiles: None,
            sensitive_credentials: None,
            app_settings: BTreeMap::from([
                ("general".into(), Some("gen-1".into())),
                ("appearance".into(), Some("app-1".into())),
            ]),
            plugin_settings: BTreeMap::from([("plugin-a".into(), Some("pa-1".into()))]),
        };

        let dirty = compute_structured_dirty_sections(&metadata, Some(&baseline), &scope);

        assert!(dirty.has_dirty);
        assert!(dirty.dirty_sections.connections);
        assert!(!dirty.dirty_sections.forwards);
        assert_eq!(dirty.dirty_sections.app_settings["general"], true);
        assert_eq!(dirty.dirty_sections.app_settings["appearance"], false);
        assert_eq!(dirty.dirty_sections.plugin_settings["plugin-a"], false);
        assert!(
            !dirty
                .current_state
                .plugin_settings
                .contains_key(CLOUD_SYNC_PLUGIN_ID)
        );
    }

    #[test]
    fn builds_manifest_paths_and_section_revisions() {
        let mut manifest = create_manifest_base(
            "rev-root",
            "2026-05-19T00:00:00.000Z",
            "macos-abcd1234",
            SyncScope::default(),
        );
        manifest.sections.connections = Some(StructuredObjectEntry {
            revision: "conn-rev".into(),
            path: connections_object_path("conn-rev"),
            record_count: Some(2),
            content_type: "application/json".into(),
        });
        manifest.sections.app_settings.insert(
            "general".into(),
            StructuredObjectEntry {
                revision: "gen-rev".into(),
                path: app_settings_object_path("general", "gen-rev"),
                record_count: None,
                content_type: OXIDE_CONTENT_TYPE.into(),
            },
        );
        manifest.section_revisions = build_manifest_section_revisions(&manifest);

        assert_eq!(
            manifest.sections.connections.as_ref().unwrap().path,
            "structured/connections/conn-rev.json"
        );
        assert_eq!(
            manifest.sections.app_settings["general"].path,
            "structured/settings/app/general/gen-rev.oxide"
        );
        assert_eq!(
            manifest.section_revisions.connections.as_deref(),
            Some("conn-rev")
        );
        assert_eq!(
            manifest.section_revisions.app_settings["general"],
            "gen-rev"
        );

        let json = serde_json::to_value(&manifest).unwrap();
        assert_eq!(json["format"], STRUCTURED_MANIFEST_FORMAT);
        assert_eq!(json["contentType"], STRUCTURED_MANIFEST_CONTENT_TYPE);
        assert_eq!(
            json["sectionRevisions"]["appSettings"]["general"],
            "gen-rev"
        );
    }

    #[test]
    fn merges_only_applied_baseline_sections() {
        let previous = StructuredLocalState {
            connections: Some("conn-old".into()),
            forwards: Some("fwd-old".into()),
            quick_commands: None,
            serial_profiles: None,
            sensitive_credentials: None,
            app_settings: BTreeMap::from([
                ("general".into(), Some("gen-old".into())),
                ("appearance".into(), Some("app-old".into())),
            ]),
            plugin_settings: BTreeMap::from([("plugin-a".into(), Some("pa-old".into()))]),
        };
        let next = StructuredLocalState {
            connections: Some("conn-new".into()),
            forwards: Some("fwd-new".into()),
            quick_commands: None,
            serial_profiles: None,
            sensitive_credentials: None,
            app_settings: BTreeMap::from([
                ("general".into(), Some("gen-new".into())),
                ("appearance".into(), Some("app-new".into())),
            ]),
            plugin_settings: BTreeMap::from([("plugin-a".into(), Some("pa-new".into()))]),
        };
        let selection = StructuredApplySelection {
            connections: true,
            app_settings_sections: strings(&["general"]),
            ..StructuredApplySelection::default()
        };

        let merged = merge_structured_baseline(Some(&previous), &next, &selection);

        assert_eq!(merged.connections.as_deref(), Some("conn-new"));
        assert_eq!(merged.forwards.as_deref(), Some("fwd-old"));
        assert_eq!(merged.app_settings["general"].as_deref(), Some("gen-new"));
        assert_eq!(
            merged.app_settings["appearance"].as_deref(),
            Some("app-old")
        );
        assert_eq!(
            merged.plugin_settings["plugin-a"].as_deref(),
            Some("pa-old")
        );
    }

    #[test]
    fn preserves_tauri_revision_shape_and_snapshot_paths() {
        let timestamp = Utc.with_ymd_and_hms(2026, 5, 19, 4, 5, 6).unwrap();
        let (revision, sequence) = next_revision(timestamp, "macos-abcd1234", 8);

        assert_eq!(sequence, 9);
        assert_eq!(revision, "2026-05-19T04:05:06.000Z-macos-abcd1234-009");
        assert_eq!(
            snapshot_object_paths("/team/default/"),
            SnapshotObjectPaths {
                metadata_key: "team/default/latest.json".into(),
                blob_key: "team/default/latest.oxide".into(),
            }
        );
        assert_eq!(
            s3_revision_blob_key("team/default", "rev-1"),
            "team/default/blobs/rev-1.oxide"
        );
    }

    #[test]
    fn counts_upload_units_like_structured_sync() {
        let scope = SyncScope {
            app_settings_sections: strings(&["general", "localTerminal"]),
            plugin_ids: Some(strings(&["plugin-a", "plugin-b"])),
            ..SyncScope::default()
        };
        let metadata = LocalSyncMetadata {
            app_settings_section_revisions: BTreeMap::from([("general".into(), "gen".into())]),
            plugin_settings_revisions: BTreeMap::from([("plugin-b".into(), "pb".into())]),
            ..LocalSyncMetadata::default()
        };

        assert_eq!(count_structured_upload_plan_units(&metadata, &scope), 4);
    }
}
