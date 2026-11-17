use std::sync::Arc;

use crate::workspace::ime::WorkspaceImeTarget;
use chrono::Utc;
use gpui::prelude::*;
use gpui::{Div, FontWeight, Rgba, point};
use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, CloudSyncStatus, ConflictStrategy,
    OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope, normalize_sync_scope,
    operation::{
        ApplyLegacyPreviewOutcome, ApplyStructuredPreviewOutcome, LegacyPreview, UploadOutcome,
    },
    progress::CloudSyncProgress,
    secret_keys,
    secrets::{CloudSyncKeychainSecretProvider, backend_uses_auth_mode},
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::{CloudSyncHistoryEntry, CloudSyncHistorySummary, CloudSyncPersistedState},
};
use oxideterm_gpui_cloud_sync::{
    CLOUD_SYNC_FIELD_REDACTED_VALUE, CLOUD_SYNC_GUIDE_STEP_KEYS, CloudSyncApplyOutcome,
    CloudSyncApplyUiOutcome, CloudSyncConfigRow, CloudSyncConfirmDescription,
    CloudSyncCoverageDetail, CloudSyncCoverageStatus, CloudSyncDiffLabel,
    CloudSyncErrorMessageSpec, CloudSyncFieldDiffItem, CloudSyncFieldDiffStatus,
    CloudSyncFieldMergeOutcome, CloudSyncForwardDetail, CloudSyncGuideExampleElements,
    CloudSyncHealthStatus, CloudSyncLocalDiffStatus, CloudSyncLocalFieldDiffSnapshot,
    CloudSyncPreviewBodySection, CloudSyncPreviewFactValue, CloudSyncPreviewImpactItem,
    CloudSyncPreviewRecord, CloudSyncPreviewRecordRow, CloudSyncPreviewSelectionAction,
    CloudSyncPreviewSelectionLabel, CloudSyncPreviewSource, CloudSyncPreviewSummary,
    CloudSyncRemoteDiffStatus, CloudSyncRollbackBackupSummarySpec, CloudSyncSection,
    CloudSyncSectionDiffItem, CloudSyncSelectAction, CloudSyncSelectKeyEffect,
    CloudSyncSelectKeyState, CloudSyncSelectOption, CloudSyncTab, CloudSyncUploadSelectionAction,
    cloud_sync_action_grid, cloud_sync_app_settings_section_label_key, cloud_sync_apply_diff_items,
    cloud_sync_apply_field_diff_items, cloud_sync_backend_label_key, cloud_sync_check_row,
    cloud_sync_config_rows, cloud_sync_confirm_copy_spec, cloud_sync_conflict_info,
    cloud_sync_coverage_model, cloud_sync_error_message_spec, cloud_sync_error_view,
    cloud_sync_fact_card, cloud_sync_fact_grid, cloud_sync_field_row, cloud_sync_focusable_selects,
    cloud_sync_form_grid, cloud_sync_form_toggle, cloud_sync_format_timestamp,
    cloud_sync_forward_detail_rows, cloud_sync_guide_card, cloud_sync_guide_spec,
    cloud_sync_health_items, cloud_sync_history_action_label_key, cloud_sync_history_empty,
    cloud_sync_history_entry, cloud_sync_inline_button_options, cloud_sync_legacy_apply_plan,
    cloud_sync_list_item, cloud_sync_list_more, cloud_sync_meta_line, cloud_sync_platform_label,
    cloud_sync_preview_block, cloud_sync_preview_card, cloud_sync_preview_card_model,
    cloud_sync_preview_record_group_model, cloud_sync_preview_record_label_key,
    cloud_sync_preview_summary, cloud_sync_progress_stage_label_key, cloud_sync_progress_unit,
    cloud_sync_progress_view, cloud_sync_rollback_backup_row, cloud_sync_rollback_backup_signature,
    cloud_sync_rollback_backup_summary_spec, cloud_sync_secret_row, cloud_sync_section_title,
    cloud_sync_select_field, cloud_sync_select_label_key,
    cloud_sync_select_options as cloud_sync_select_option_specs, cloud_sync_select_trigger,
    cloud_sync_selected_option_index as cloud_sync_selected_option_spec_index,
    cloud_sync_settings_from_form, cloud_sync_should_create_rollback_backup,
    cloud_sync_sidebar_empty, cloud_sync_status_label_key, cloud_sync_status_list,
    cloud_sync_status_row, cloud_sync_toggle, cloud_sync_toggle_grid, cloud_sync_upload_diff_items,
    cloud_sync_upload_field_diff_items, cloud_sync_value_prefers_mono,
    cloud_sync_version_info_rows, deliver_cloud_sync_apply_preview,
    deliver_cloud_sync_github_oauth, deliver_cloud_sync_google_oauth,
    deliver_cloud_sync_microsoft_oauth, deliver_cloud_sync_restore_backup_preview,
    finish_cloud_sync_automatic_upload_error_state, finish_cloud_sync_check_state,
    finish_cloud_sync_error_state, finish_cloud_sync_pull_preview_state,
    finish_cloud_sync_upload_state, finish_legacy_cloud_sync_apply_state,
    finish_structured_cloud_sync_apply_state,
    handle_cloud_sync_select_key as reduce_cloud_sync_select_key,
    normalize_cloud_sync_interval_draft, persist_remote_metadata, reset_cloud_sync_secret_drafts,
    store_cloud_sync_touched_secrets,
};
pub(super) use oxideterm_gpui_cloud_sync::{
    CloudSyncConfirm, CloudSyncDelivery, CloudSyncPendingPreview, CloudSyncPreviewSelection,
    CloudSyncSelect, CloudSyncUploadSelection,
};
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::button::ButtonVariant;
use oxideterm_gpui_ui::text_input::{TextInputView, text_input_anchor_probe};
use oxideterm_gpui_ui::{
    StatusPillOptions, StatusTone, SurfaceKind, SurfaceOptions, SurfacePadding, semantic_surface,
    status_pill, status_pill_element,
};
use oxideterm_settings_model::{CloudSyncFormDraft, cloud_sync_form_input_value_ref};

use super::quick_commands::QuickCommandImportStrategy;
use super::*;
use oxideterm_gpui_ui::modal::overlay_content_boundary;
use oxideterm_gpui_ui::select::{
    select_option_action, select_option_highlighted, select_panel_overlay_popup_with_max_height,
};

mod causal;
mod config;
mod confirm_dialog;
mod delivery;
mod history;
mod local_file;
mod maintenance;
mod preview;
mod surface;

pub(super) use oxideterm_gpui_cloud_sync::entity::{
    CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT, CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT,
    CloudSyncLocalSnapshotCache, CloudSyncUiIntent, CloudSyncUploadDiffCache,
    CloudSyncWorkspaceEntity, CloudSyncWorkspaceEvent,
};

/// Frame-scoped, read-only UI dependencies shared by Cloud Sync virtual rows.
#[derive(Clone)]
pub(super) struct CloudSyncListRenderProjection {
    pub(super) tokens: ThemeTokens,
    pub(super) i18n: I18n,
    pub(super) selectable_text: crate::workspace::selectable_text::SelectableTextRenderState,
    pub(super) has_background: bool,
    pub(super) input: CloudSyncInputRenderProjection,
    pub(super) local_snapshot: std::result::Result<Arc<CloudSyncLocalSnapshot>, Arc<str>>,
    pub(super) local_field_diff: Arc<CloudSyncLocalFieldDiffSnapshot>,
    pub(super) upload_diff_items: Arc<Vec<CloudSyncSectionDiffItem>>,
    pub(super) mono_font_family: SharedString,
    pub(super) tab_transition_active: bool,
    pub(super) upload_sensitive_summary: Option<String>,
}

/// Contains only frame-scoped text geometry; secret text is length-preserving masked.
#[derive(Clone, Default)]
pub(super) struct CloudSyncInputRenderProjection {
    pub(super) focused_input: Option<SettingsInput>,
    pub(super) active_value: Option<String>,
    pub(super) selected_range: Option<std::ops::Range<usize>>,
    pub(super) active_offset: Option<usize>,
    pub(super) marked_text: Option<String>,
    pub(super) caret_visible: bool,
}

/// Renders Cloud Sync virtual rows without retaining the workspace root.
#[derive(Clone)]
pub(super) struct CloudSyncPageRenderer {
    pub(super) cloud_sync: Entity<CloudSyncWorkspaceEntity>,
    pub(super) render: std::rc::Rc<CloudSyncListRenderProjection>,
    pub(super) input_anchors: super::ime::TextInputAnchorStore,
}

impl std::ops::Deref for CloudSyncPageRenderer {
    type Target = CloudSyncListRenderProjection;

    fn deref(&self) -> &Self::Target {
        &self.render
    }
}

fn cloud_sync_tab_index(tab: CloudSyncTab) -> usize {
    match tab {
        CloudSyncTab::Overview => 0,
        CloudSyncTab::Configure => 1,
        CloudSyncTab::History => 2,
    }
}

fn is_cloud_sync_remote_changed_before_upload(error: &str) -> bool {
    error
        .trim_start()
        .starts_with("remote_changed_before_upload")
}

const CLOUD_SYNC_TW_ALPHA_10: u32 = 0x1a;
const CLOUD_SYNC_TW_ALPHA_40: u32 = 0x66;
const CLOUD_SYNC_TW_ALPHA_50: u32 = 0x80;
const CLOUD_SYNC_BG_ACTIVE_THEME_ALPHA: u32 = 0x66;
const CLOUD_SYNC_BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60;
const CLOUD_SYNC_SECTION_DIFF_ITEM_MIN_WIDTH: f32 = 320.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloudSyncActionTone {
    Accent,
    Muted,
}

impl CloudSyncActionTone {
    fn color(self, tokens: &oxideterm_theme::ThemeTokens) -> u32 {
        match self {
            Self::Accent => tokens.ui.accent,
            Self::Muted => tokens.ui.text_muted,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloudSyncTone {
    Accent,
    Success,
    Warning,
    Error,
    Muted,
}

impl CloudSyncTone {
    fn color(self, tokens: &oxideterm_theme::ThemeTokens) -> u32 {
        match self {
            Self::Accent => tokens.ui.accent,
            Self::Success => tokens.ui.success,
            Self::Warning => tokens.ui.warning,
            Self::Error => tokens.ui.error,
            Self::Muted => tokens.ui.text_muted,
        }
    }
}

fn cloud_sync_status_tone(tone: CloudSyncTone) -> StatusTone {
    match tone {
        CloudSyncTone::Accent => StatusTone::Accent,
        CloudSyncTone::Success => StatusTone::Success,
        CloudSyncTone::Warning => StatusTone::Warning,
        CloudSyncTone::Error => StatusTone::Error,
        CloudSyncTone::Muted => StatusTone::Neutral,
    }
}

fn health_tone(status: CloudSyncHealthStatus) -> CloudSyncTone {
    match status {
        CloudSyncHealthStatus::Pass => CloudSyncTone::Success,
        CloudSyncHealthStatus::Warning => CloudSyncTone::Warning,
        CloudSyncHealthStatus::Fail => CloudSyncTone::Error,
    }
}

fn local_diff_tone(status: CloudSyncLocalDiffStatus) -> CloudSyncTone {
    match status {
        CloudSyncLocalDiffStatus::Added => CloudSyncTone::Success,
        CloudSyncLocalDiffStatus::Modified => CloudSyncTone::Accent,
        CloudSyncLocalDiffStatus::Deleted => CloudSyncTone::Error,
        CloudSyncLocalDiffStatus::Unchanged | CloudSyncLocalDiffStatus::Excluded => {
            CloudSyncTone::Muted
        }
    }
}

fn remote_diff_tone(status: CloudSyncRemoteDiffStatus) -> CloudSyncTone {
    match status {
        CloudSyncRemoteDiffStatus::Creates => CloudSyncTone::Success,
        CloudSyncRemoteDiffStatus::Overwrites => CloudSyncTone::Warning,
        CloudSyncRemoteDiffStatus::RemovedByScope => CloudSyncTone::Error,
        CloudSyncRemoteDiffStatus::Unchanged | CloudSyncRemoteDiffStatus::Excluded => {
            CloudSyncTone::Muted
        }
        CloudSyncRemoteDiffStatus::Unknown => CloudSyncTone::Warning,
    }
}

fn cloud_sync_root_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        cloud_sync_theme_alpha(0x000000, 0x00)
    } else {
        rgb(color)
    }
}

// Tauri switches bg-theme-* surfaces to alpha-backed colors under
// data-bg-active; Cloud Sync mirrors the plugin manager's native helpers.
fn cloud_sync_theme_panel_bg(color: u32, has_background: bool) -> Rgba {
    cloud_sync_theme_card_bg(color, has_background)
}

fn cloud_sync_theme_card_bg(color: u32, has_background: bool) -> Rgba {
    oxideterm_gpui_ui::surface::color_for_background(
        color,
        has_background,
        CLOUD_SYNC_BG_ACTIVE_THEME_ALPHA,
    )
}

fn cloud_sync_theme_border_half(color: u32, has_background: bool) -> Rgba {
    oxideterm_gpui_ui::surface::color_for_background_or_alpha(
        color,
        has_background,
        CLOUD_SYNC_BG_ACTIVE_BORDER_HALF_ALPHA,
        CLOUD_SYNC_TW_ALPHA_50,
    )
}

fn cloud_sync_theme_alpha(color: u32, alpha: u32) -> Rgba {
    rgba((color << 8) | alpha)
}
