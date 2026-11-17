use super::quick_commands::QuickCommandImportStrategy;
use super::*;
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_cloud_sync::operation::{ApplyLegacyPreviewOutcome, LegacyPreview};
pub(super) use oxideterm_gpui_cloud_sync::entity::{
    CloudSyncUiIntent, CloudSyncWorkspaceEntity, CloudSyncWorkspaceEvent,
};
use oxideterm_gpui_cloud_sync::{
    CloudSyncApplyOutcome, CloudSyncApplyUiOutcome, CloudSyncConfirmDescription,
    CloudSyncPreviewSource, CloudSyncTab, PendingLegacyImportAudit, cloud_sync_confirm_copy_spec,
    cloud_sync_legacy_apply_plan, cloud_sync_sidebar_empty,
    panel::{
        CloudSyncInputRenderProjection, CloudSyncListRenderProjection, CloudSyncPageRenderer,
        SelectableTextRole as CloudSyncTextRole, cloud_sync_select_anchor_id, cloud_sync_tab_index,
    },
    runtime::{
        CloudSyncHostEffect, CloudSyncLegacyImportStats, CloudSyncNoticeKind, CloudSyncRuntime,
        CloudSyncSources,
    },
};
pub(super) use oxideterm_gpui_cloud_sync::{
    CloudSyncConfirm, CloudSyncPreviewSelection, CloudSyncSelect,
};
use oxideterm_settings_model::cloud_sync_form_input_value_ref;

mod config;
mod confirm_dialog;
mod delivery;
mod local_file;
mod surface;
