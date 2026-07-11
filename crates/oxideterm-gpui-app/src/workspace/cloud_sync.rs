use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::mpsc::{self, TryRecvError},
    time::UNIX_EPOCH,
};

use crate::workspace::ime::WorkspaceImeTarget;
use chrono::Utc;
use gpui::prelude::*;
use gpui::{Div, FontWeight, Rgba, point};
use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, CloudSyncStatus, ConflictStrategy,
    OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope, StructuredLocalState, StructuredSectionRevisions,
    normalize_sync_scope,
    operation::{
        ApplyLegacyPreviewOutcome, ApplyStructuredPreviewOutcome, LegacyPreview, UploadOptions,
        UploadOutcome,
    },
    progress::CloudSyncProgress,
    secrets::{CloudSyncKeychainSecretProvider, backend_uses_auth_mode},
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::{
        CloudSyncHistoryEntry, CloudSyncHistorySummary, CloudSyncPersistedState,
        CloudSyncRollbackBackup,
    },
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
    close_cloud_sync_select_on_container_scroll, cloud_sync_action_grid,
    cloud_sync_app_settings_section_label_key, cloud_sync_apply_diff_items,
    cloud_sync_apply_field_diff_items, cloud_sync_backend_label_key, cloud_sync_check_row,
    cloud_sync_config_rows, cloud_sync_confirm_copy_spec, cloud_sync_conflict_info,
    cloud_sync_coverage_model, cloud_sync_error_message_spec, cloud_sync_error_view,
    cloud_sync_fact_card, cloud_sync_fact_grid, cloud_sync_field_row, cloud_sync_focusable_selects,
    cloud_sync_form_grid, cloud_sync_format_timestamp, cloud_sync_forward_detail_rows,
    cloud_sync_guide_card, cloud_sync_guide_spec, cloud_sync_health_items,
    cloud_sync_history_action_label_key, cloud_sync_history_empty, cloud_sync_history_entry,
    cloud_sync_history_signature, cloud_sync_inline_button_options, cloud_sync_legacy_apply_plan,
    cloud_sync_list_item, cloud_sync_list_more, cloud_sync_meta_line, cloud_sync_platform_label,
    cloud_sync_preview_block, cloud_sync_preview_card, cloud_sync_preview_card_model,
    cloud_sync_preview_record_group_model, cloud_sync_preview_record_label_key,
    cloud_sync_preview_summary, cloud_sync_progress_stage_label_key, cloud_sync_progress_unit,
    cloud_sync_progress_view, cloud_sync_rollback_backup_row, cloud_sync_rollback_backup_signature,
    cloud_sync_rollback_backup_summary_spec, cloud_sync_secret_row, cloud_sync_section_signature,
    cloud_sync_section_title, cloud_sync_sections, cloud_sync_select_field,
    cloud_sync_select_label_key, cloud_sync_select_options as cloud_sync_select_option_specs,
    cloud_sync_select_trigger,
    cloud_sync_selected_option_index as cloud_sync_selected_option_spec_index,
    cloud_sync_settings_from_form, cloud_sync_should_create_rollback_backup,
    cloud_sync_sidebar_empty, cloud_sync_status_label_key, cloud_sync_status_list,
    cloud_sync_status_row, cloud_sync_tab_bar, cloud_sync_tab_button, cloud_sync_toggle,
    cloud_sync_toggle_grid, cloud_sync_upload_diff_items, cloud_sync_upload_field_diff_items,
    cloud_sync_value_prefers_mono, cloud_sync_version_info_rows, deliver_cloud_sync_apply_preview,
    deliver_cloud_sync_check, deliver_cloud_sync_github_oauth, deliver_cloud_sync_google_oauth,
    deliver_cloud_sync_microsoft_oauth, deliver_cloud_sync_pull_preview,
    deliver_cloud_sync_restore_backup_preview, deliver_cloud_sync_upload,
    deliver_cloud_sync_upload_preview, finish_cloud_sync_automatic_upload_error_state,
    finish_cloud_sync_check_state, finish_cloud_sync_error_state,
    finish_cloud_sync_pull_preview_state, finish_cloud_sync_upload_state,
    finish_legacy_cloud_sync_apply_state, finish_structured_cloud_sync_apply_state,
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
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};
use oxideterm_gpui_ui::{
    StatusPillOptions, StatusTone, SurfaceKind, SurfaceOptions, SurfacePadding, semantic_surface,
    status_pill, status_pill_element,
};
use oxideterm_settings_model::CloudSyncFormDraft;

use super::quick_commands::QuickCommandImportStrategy;
use super::*;
use oxideterm_gpui_ui::modal::overlay_content_boundary;
use oxideterm_gpui_ui::select::{
    select_option_action, select_option_highlighted, select_panel_overlay_popup_with_max_height,
};

mod config;
mod confirm_dialog;
mod delivery;
mod history;
mod maintenance;
mod preview;
mod surface;

#[derive(Clone)]
pub(super) struct CloudSyncLocalSnapshotCache {
    key: u64,
    result: std::result::Result<CloudSyncLocalSnapshot, String>,
}

#[derive(Clone)]
pub(super) struct CloudSyncUploadDiffCache {
    key: u64,
    items: Vec<CloudSyncSectionDiffItem>,
}

/// Owns the persisted service and asynchronous operation lifecycle for Cloud Sync.
pub(super) struct CloudSyncControllerState {
    pub(super) store: oxideterm_cloud_sync::state::CloudSyncStateStore,
    pub(super) service: oxideterm_cloud_sync::operation::CloudSyncOperationService,
    pub(super) progress: Option<CloudSyncProgress>,
    pub(super) delivery_rx: Option<std::sync::mpsc::Receiver<CloudSyncDelivery>>,
    pub(super) polling: bool,
    pub(super) active_action: Option<&'static str>,
    pub(super) auto_upload_generation: u64,
    pub(super) dirty_refresh_scheduled: bool,
    pub(super) dirty_refresh_generation: u64,
    pub(super) upload_after_current: Option<bool>,
    pub(super) pull_preview_after_current: bool,
}

impl CloudSyncControllerState {
    fn new(store: oxideterm_cloud_sync::state::CloudSyncStateStore) -> Self {
        // Operation lifecycle begins idle while retaining the loaded persisted state.
        Self {
            store,
            service: oxideterm_cloud_sync::operation::CloudSyncOperationService::new(),
            progress: None,
            delivery_rx: None,
            polling: false,
            active_action: None,
            auto_upload_generation: 0,
            dirty_refresh_scheduled: false,
            dirty_refresh_generation: 0,
            upload_after_current: None,
            pull_preview_after_current: false,
        }
    }
}

/// Owns Cloud Sync form drafts, navigation, dialogs, previews, and virtual-list caches.
pub(super) struct CloudSyncViewState {
    pub(super) form: CloudSyncFormDraft,
    pub(super) section_list_state: ListState,
    pub(super) section_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) local_snapshot_cache: RefCell<Option<CloudSyncLocalSnapshotCache>>,
    pub(super) upload_diff_cache: RefCell<Option<CloudSyncUploadDiffCache>>,
    pub(super) rollback_backup_list_state: ListState,
    pub(super) rollback_backup_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) history_list_state: ListState,
    pub(super) history_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) open_select: Option<CloudSyncSelect>,
    pub(super) rendered_select: Option<CloudSyncSelect>,
    pub(super) focused_select: Option<CloudSyncSelect>,
    pub(super) select_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    pub(super) select_highlighted: Option<(CloudSyncSelect, usize)>,
    pub(super) confirm: Option<CloudSyncConfirm>,
    pub(super) confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) confirm_focused_action: Option<ConfirmDialogAction>,
    pub(super) pending_preview: Option<CloudSyncPendingPreview>,
    pub(super) upload_preview: Option<CloudSyncPendingPreview>,
    pub(super) preview_selection: Option<CloudSyncPreviewSelection>,
    pub(super) upload_selection: Option<CloudSyncUploadSelection>,
    pub(super) active_tab: CloudSyncTab,
}

impl CloudSyncViewState {
    fn new(settings: &CloudSyncSettings) -> Self {
        // Cloud Sync is a variable-height browser page with optional preview
        // and rollback sections; keep it on the shared section-list path.
        let section_list_state = ListState::new(
            CLOUD_SYNC_SECTION_LIST_INITIAL_ITEM_COUNT,
            ListAlignment::Top,
            TauriVirtualListSpec::new(
                px(CLOUD_SYNC_SECTION_LIST_ESTIMATED_HEIGHT),
                CLOUD_SYNC_SECTION_LIST_OVERSCAN,
            )
            .overdraw(),
        );
        // Rollback backups and history are independent nested virtual lists.
        let rollback_backup_list_state = ListState::new(
            CLOUD_SYNC_ROLLBACK_BACKUP_LIST_INITIAL_ITEM_COUNT,
            ListAlignment::Top,
            TauriVirtualListSpec::new(
                px(CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT),
                CLOUD_SYNC_ROLLBACK_BACKUP_LIST_OVERSCAN,
            )
            .overdraw(),
        );
        let history_list_state = ListState::new(
            CLOUD_SYNC_HISTORY_LIST_INITIAL_ITEM_COUNT,
            ListAlignment::Top,
            TauriVirtualListSpec::new(
                px(CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT),
                CLOUD_SYNC_HISTORY_LIST_OVERSCAN,
            )
            .overdraw(),
        );

        Self {
            form: CloudSyncFormDraft::from_settings(settings),
            section_list_state,
            section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            local_snapshot_cache: RefCell::new(None),
            upload_diff_cache: RefCell::new(None),
            rollback_backup_list_state,
            rollback_backup_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            history_list_state,
            history_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            open_select: None,
            rendered_select: None,
            focused_select: None,
            select_focus_origin: None,
            select_highlighted: None,
            confirm: None,
            confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            confirm_focused_action: None,
            pending_preview: None,
            upload_preview: None,
            preview_selection: None,
            upload_selection: None,
            active_tab: CloudSyncTab::Overview,
        }
    }
}

/// Groups the Cloud Sync controller lifecycle and its ephemeral GPUI view state.
pub(super) struct CloudSyncWorkspaceState {
    pub(super) controller: CloudSyncControllerState,
    pub(super) view: CloudSyncViewState,
}

impl CloudSyncWorkspaceState {
    pub(super) fn new(store: oxideterm_cloud_sync::state::CloudSyncStateStore) -> Self {
        // Build the form projection before moving the loaded store into the controller.
        let view = CloudSyncViewState::new(&store.state().settings);
        Self {
            controller: CloudSyncControllerState::new(store),
            view,
        }
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

fn hash_raw_sync_scope(scope: &RawSyncScope, hasher: &mut DefaultHasher) {
    scope.sync_connections.hash(hasher);
    scope.sync_forwards.hash(hasher);
    scope.sync_quick_commands.hash(hasher);
    scope.sync_serial_profiles.hash(hasher);
    scope.sync_raw_tcp_profiles.hash(hasher);
    scope.sync_raw_udp_profiles.hash(hasher);
    scope.sync_sensitive_credentials.hash(hasher);
    scope.sync_app_settings.hash(hasher);
    scope.app_settings_sections.hash(hasher);
    scope.include_local_terminal_env_vars.hash(hasher);
    scope.sync_plugin_settings.hash(hasher);
    scope.plugin_ids.hash(hasher);
}

fn hash_structured_local_state_option(
    state: Option<&StructuredLocalState>,
    hasher: &mut DefaultHasher,
) {
    let Some(state) = state else {
        false.hash(hasher);
        return;
    };
    true.hash(hasher);
    state.connections.hash(hasher);
    state.forwards.hash(hasher);
    state.quick_commands.hash(hasher);
    state.serial_profiles.hash(hasher);
    state.raw_tcp_profiles.hash(hasher);
    state.raw_udp_profiles.hash(hasher);
    state.sensitive_credentials.hash(hasher);
    state.app_settings.hash(hasher);
    state.plugin_settings.hash(hasher);
}

fn hash_structured_section_revisions_option(
    revisions: Option<&StructuredSectionRevisions>,
    hasher: &mut DefaultHasher,
) {
    let Some(revisions) = revisions else {
        false.hash(hasher);
        return;
    };
    true.hash(hasher);
    revisions.connections.hash(hasher);
    revisions.forwards.hash(hasher);
    revisions.quick_commands.hash(hasher);
    revisions.serial_profiles.hash(hasher);
    revisions.raw_tcp_profiles.hash(hasher);
    revisions.raw_udp_profiles.hash(hasher);
    revisions.sensitive_credentials.hash(hasher);
    revisions.app_settings.hash(hasher);
    revisions.plugin_settings.hash(hasher);
}

fn hash_quick_commands_file_stamp(settings_path: &std::path::Path, hasher: &mut DefaultHasher) {
    let path = oxideterm_quick_commands::quick_commands_path(settings_path);
    let Ok(metadata) = std::fs::metadata(path) else {
        0_u8.hash(hasher);
        return;
    };
    metadata.len().hash(hasher);
    if let Ok(modified) = metadata.modified() {
        modified
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_nanos())
            .hash(hasher);
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
