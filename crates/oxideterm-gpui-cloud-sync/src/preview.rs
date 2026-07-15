// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync preview DTOs and summaries.

use std::collections::{BTreeMap, BTreeSet};

use oxideterm_cloud_sync::{
    ConflictStrategy, OXIDE_APP_SETTINGS_SECTION_IDS, PREVIEW_RECORD_LIMIT, RawSyncScope,
    StructuredSectionRevisions, SyncScope, normalize_sync_scope,
    operation::{LegacyPreview, StructuredPreview, merge_structured_model_fields},
    service::CloudSyncLocalSnapshot,
    state::CloudSyncPersistedState,
};
use oxideterm_connections::{
    ConnectionInfo, SavedConnectionsSyncSnapshot, SerialProfile, SerialProfilesSyncSnapshot,
    oxide_file::AppSettingsSectionPreview,
};
use oxideterm_forwarding::{PersistedForwardDto, SavedForwardsSyncSnapshot};
use oxideterm_quick_commands::{QuickCommand, QuickCommandsSnapshot};

use crate::selection::CloudSyncPreviewSelection;

pub const CLOUD_SYNC_FIELD_REDACTED_VALUE: &str = "<redacted>";

#[derive(Clone, Debug)]
pub enum CloudSyncPendingPreview {
    Structured(StructuredPreview),
    Legacy {
        preview: LegacyPreview,
        source: CloudSyncPreviewSource,
    },
}

impl CloudSyncPendingPreview {
    pub fn is_backup(&self) -> bool {
        matches!(
            self,
            Self::Legacy {
                source: CloudSyncPreviewSource::Backup { .. },
                ..
            }
        )
    }
}

#[derive(Clone, Debug)]
pub enum CloudSyncPreviewSource {
    Remote,
    Backup { id: String, created_at: String },
}

impl CloudSyncPreviewSource {
    pub fn is_backup(&self) -> bool {
        matches!(self, Self::Backup { .. })
    }
}

#[derive(Clone, Debug, Default)]
pub struct CloudSyncPreviewSummary {
    pub connections: usize,
    pub forwards: usize,
    pub quick_commands: usize,
    pub serial_profiles: usize,
    pub sensitive_credentials: usize,
    pub has_app_settings: bool,
    pub app_settings_sections: Vec<CloudSyncAppSettingsSection>,
    pub plugin_settings_count: usize,
    pub plugin_settings_by_plugin: BTreeMap<String, usize>,
    pub has_embedded_keys: bool,
    pub forward_details: Vec<CloudSyncForwardDetail>,
    pub records: Vec<CloudSyncPreviewRecord>,
}

#[derive(Clone, Debug)]
pub struct CloudSyncAppSettingsSection {
    pub id: String,
    pub field_count: usize,
}

#[derive(Clone, Debug)]
pub struct CloudSyncForwardDetail {
    pub owner_connection_name: String,
    pub direction: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewRecord {
    pub resource: String,
    pub name: String,
    pub action: String,
    pub reason_code: String,
    pub target_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncPreviewCardKind {
    Import,
    Rollback,
}

#[derive(Clone, Debug)]
pub struct CloudSyncPreviewCardModel {
    pub summary: CloudSyncPreviewSummary,
    pub selection: CloudSyncPreviewSelection,
    pub can_apply: bool,
    pub kind: CloudSyncPreviewCardKind,
    pub copy: CloudSyncPreviewCardCopySpec,
    pub fact_rows: Vec<Vec<CloudSyncPreviewFactSpec>>,
    pub body_sections: Vec<CloudSyncPreviewBodySection>,
    pub impact_items: Vec<CloudSyncPreviewImpactItem>,
    pub show_local_changes_warning: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewCardCopySpec {
    pub title_identity: &'static str,
    pub title_key: &'static str,
    pub apply_label_key: &'static str,
    pub warning_key: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewFactSpec {
    pub label_key: &'static str,
    pub value: CloudSyncPreviewFactValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncPreviewFactValue {
    Count(usize),
    YesNo(bool),
}

#[derive(Clone, Debug)]
pub enum CloudSyncPreviewBodySection {
    Selection,
    ForwardDetails(Vec<CloudSyncForwardDetail>),
    RecordGroup {
        action: &'static str,
        records: Vec<CloudSyncPreviewRecord>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncCoverageStatus {
    Included,
    Excluded,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncCoverageDetail {
    Static(&'static str),
    AppSettingsSections(Vec<String>),
    PluginSettings(Option<Vec<String>>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncCoverageItem {
    pub label_key: &'static str,
    pub status: CloudSyncCoverageStatus,
    pub detail: CloudSyncCoverageDetail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewImpactItem {
    pub label_key: &'static str,
    pub count: usize,
    pub status: CloudSyncCoverageStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncDiffLabel {
    Key(&'static str),
    AppSettingsSection(String),
    PluginSettings(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncLocalDiffStatus {
    Added,
    Modified,
    Deleted,
    Unchanged,
    Excluded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncRemoteDiffStatus {
    Creates,
    Overwrites,
    Unchanged,
    RemovedByScope,
    Excluded,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncSectionDiffItem {
    pub label: CloudSyncDiffLabel,
    pub local_status: CloudSyncLocalDiffStatus,
    pub remote_status: CloudSyncRemoteDiffStatus,
    pub count: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct CloudSyncLocalFieldDiffSnapshot {
    pub connections: Option<SavedConnectionsSyncSnapshot>,
    pub forwards: Option<SavedForwardsSyncSnapshot>,
    pub quick_commands: Option<QuickCommandsSnapshot>,
    pub serial_profiles: Option<SerialProfilesSyncSnapshot>,
    pub app_settings_sections: Vec<AppSettingsSectionPreview>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncFieldDiffStatus {
    Added,
    Modified,
    Deleted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncFieldDiffItem {
    pub section_label_key: &'static str,
    pub item_key: String,
    pub item_name: String,
    pub status: CloudSyncFieldDiffStatus,
    pub fields: Vec<CloudSyncFieldDiffField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncFieldDiffField {
    pub label_key: &'static str,
    pub before: Option<String>,
    pub after: Option<String>,
    pub merge_outcome: Option<CloudSyncFieldMergeOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncFieldMergeOutcome {
    Remote,
    Local,
    Merged,
    ConflictLocal,
    ConflictRemote,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncForwardDetailRow {
    pub title: String,
    pub meta: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncPreviewRecordRow {
    Connection {
        record: CloudSyncPreviewRecord,
        checked: bool,
        disabled: bool,
    },
    Item {
        record: CloudSyncPreviewRecord,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewListModel<T> {
    pub rows: Vec<T>,
    pub overflow_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewRecordGroupModel {
    pub title_key: &'static str,
    pub rows: Vec<CloudSyncPreviewRecordRow>,
    pub overflow_count: usize,
}

mod connection;
mod field_diff;
mod forwarding;
mod helpers;
mod profiles;
mod quick_commands;
mod summary;

use connection::*;
use field_diff::*;
use forwarding::*;
pub use helpers::*;
use profiles::*;
use quick_commands::*;
pub use summary::*;

#[cfg(test)]
mod tests;
