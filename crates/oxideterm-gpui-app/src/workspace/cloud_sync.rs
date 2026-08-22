use std::{cell::Cell, collections::VecDeque, sync::Arc};

use crate::workspace::ime::WorkspaceImeTarget;
use chrono::Utc;
use gpui::prelude::*;
use gpui::{Div, EventEmitter, FontWeight, Rgba, Task, Timer, point};
use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, CloudSyncStatus, ConflictStrategy,
    OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope, normalize_sync_scope,
    operation::{
        ApplyLegacyPreviewOutcome, ApplyStructuredPreviewOutcome, LegacyPreview, UploadOptions,
        UploadOutcome,
    },
    progress::CloudSyncProgress,
    secrets::{CloudSyncKeychainSecretProvider, backend_uses_auth_mode},
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::{CloudSyncHistoryEntry, CloudSyncHistorySummary, CloudSyncPersistedState},
};
use oxideterm_gpui_cloud_sync::{
    CLOUD_SYNC_FIELD_REDACTED_VALUE, CLOUD_SYNC_GUIDE_STEP_KEYS, CloudSyncApplyOutcome,
    CloudSyncApplyUiOutcome, CloudSyncConfigRow, CloudSyncConfirmDescription,
    CloudSyncCoverageDetail, CloudSyncCoverageStatus, CloudSyncDeliverySink, CloudSyncDiffLabel,
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
    cloud_sync_form_grid, cloud_sync_form_toggle, cloud_sync_format_timestamp,
    cloud_sync_forward_detail_rows, cloud_sync_guide_card, cloud_sync_guide_spec,
    cloud_sync_health_items, cloud_sync_history_action_label_key, cloud_sync_history_empty,
    cloud_sync_history_entry, cloud_sync_history_signature, cloud_sync_inline_button_options,
    cloud_sync_legacy_apply_plan, cloud_sync_list_item, cloud_sync_list_more, cloud_sync_meta_line,
    cloud_sync_platform_label, cloud_sync_preview_block, cloud_sync_preview_card,
    cloud_sync_preview_card_model, cloud_sync_preview_record_group_model,
    cloud_sync_preview_record_label_key, cloud_sync_preview_summary,
    cloud_sync_progress_stage_label_key, cloud_sync_progress_unit, cloud_sync_progress_view,
    cloud_sync_rollback_backup_row, cloud_sync_rollback_backup_signature,
    cloud_sync_rollback_backup_summary_spec, cloud_sync_secret_row, cloud_sync_section_signature,
    cloud_sync_section_title, cloud_sync_sections, cloud_sync_select_field,
    cloud_sync_select_label_key, cloud_sync_select_options as cloud_sync_select_option_specs,
    cloud_sync_select_trigger,
    cloud_sync_selected_option_index as cloud_sync_selected_option_spec_index,
    cloud_sync_settings_from_form, cloud_sync_should_create_rollback_backup,
    cloud_sync_sidebar_empty, cloud_sync_status_label_key, cloud_sync_status_list,
    cloud_sync_status_row, cloud_sync_toggle, cloud_sync_toggle_grid, cloud_sync_upload_diff_items,
    cloud_sync_upload_field_diff_items, cloud_sync_value_prefers_mono,
    cloud_sync_version_info_rows, deliver_cloud_sync_apply_preview, deliver_cloud_sync_check,
    deliver_cloud_sync_github_oauth, deliver_cloud_sync_google_oauth,
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
use oxideterm_settings_model::{CloudSyncFormDraft, cloud_sync_form_input_value_ref};

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
    generation: u64,
    result: std::result::Result<CloudSyncLocalSnapshot, String>,
}

#[derive(Clone)]
pub(super) struct CloudSyncUploadDiffCache {
    generation: u64,
    items: Vec<CloudSyncSectionDiffItem>,
}

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
    pub(super) marked_text: Option<String>,
    pub(super) caret_visible: bool,
}

/// Renders Cloud Sync virtual rows without retaining the workspace root.
#[derive(Clone)]
pub(super) struct CloudSyncPageRenderer {
    pub(super) cloud_sync: Entity<CloudSyncWorkspaceEntity>,
    pub(super) render: Arc<CloudSyncListRenderProjection>,
}

impl std::ops::Deref for CloudSyncPageRenderer {
    type Target = CloudSyncListRenderProjection;

    fn deref(&self) -> &Self::Target {
        &self.render
    }
}

/// Owns the persisted service and asynchronous operation lifecycle for Cloud Sync.
pub(super) struct CloudSyncControllerState {
    pub(super) store: oxideterm_cloud_sync::state::CloudSyncStateStore,
    pub(super) service: oxideterm_cloud_sync::operation::CloudSyncOperationService,
    pub(super) progress: Option<CloudSyncProgress>,
    pub(super) delivery_rx: Option<std::sync::mpsc::Receiver<CloudSyncDelivery>>,
    pub(super) active_action: Option<&'static str>,
    pub(super) auto_upload_generation: u64,
    pub(super) dirty_refresh_scheduled: bool,
    pub(super) dirty_refresh_generation: u64,
    pub(super) upload_after_current: Option<bool>,
}

impl CloudSyncControllerState {
    fn new(store: oxideterm_cloud_sync::state::CloudSyncStateStore) -> Self {
        // Operation lifecycle begins idle while retaining the loaded persisted state.
        Self {
            store,
            service: oxideterm_cloud_sync::operation::CloudSyncOperationService::new(),
            progress: None,
            delivery_rx: None,
            active_action: None,
            auto_upload_generation: 0,
            dirty_refresh_scheduled: false,
            dirty_refresh_generation: 0,
            upload_after_current: None,
        }
    }
}

/// Owns Cloud Sync form drafts, navigation, dialogs, previews, and virtual-list caches.
pub(super) struct CloudSyncViewState {
    pub(super) form: CloudSyncFormDraft,
    section_rows: Vec<CloudSyncSection>,
    pub(super) section_list_state: ListState,
    pub(super) section_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) snapshot_cache_generation: Cell<u64>,
    pub(super) local_snapshot_cache: RefCell<Option<CloudSyncLocalSnapshotCache>>,
    pub(super) upload_diff_cache: RefCell<Option<CloudSyncUploadDiffCache>>,
    pub(super) rollback_backup_list_state: ListState,
    pub(super) rollback_backup_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) history_list_state: ListState,
    pub(super) history_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) open_select: Option<CloudSyncSelect>,
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
    pub(super) previous_tab: CloudSyncTab,
}

impl CloudSyncViewState {
    fn set_active_tab(&mut self, tab: CloudSyncTab) {
        if self.active_tab != tab {
            self.previous_tab = self.active_tab;
            self.active_tab = tab;
        }
    }

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
            section_rows: Vec::new(),
            section_list_state,
            section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            snapshot_cache_generation: Cell::new(0),
            local_snapshot_cache: RefCell::new(None),
            upload_diff_cache: RefCell::new(None),
            rollback_backup_list_state,
            rollback_backup_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            history_list_state,
            history_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            open_select: None,
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
            previous_tab: CloudSyncTab::Overview,
        }
    }
}

fn cloud_sync_tab_index(tab: CloudSyncTab) -> usize {
    match tab {
        CloudSyncTab::Overview => 0,
        CloudSyncTab::Configure => 1,
        CloudSyncTab::History => 2,
    }
}

/// Requests root-only adapters without giving long-lived tasks a root handle.
#[derive(Clone, Debug)]
pub(super) enum CloudSyncWorkspaceEvent {
    DeliveriesReady,
    AutoUploadDue { generation: u64 },
    DirtyRefreshDue { generation: u64 },
    ConfirmExitFinished { presence_generation: u64 },
    UiIntent(CloudSyncUiIntent),
}

/// Typed, non-secret actions crossing from Entity-owned virtual rows to root adapters.
#[derive(Clone, Debug)]
pub(super) enum CloudSyncUiIntent {
    SelectTab {
        tab: CloudSyncTab,
    },
    StartGithubOauth,
    StartMicrosoftOauth,
    StartGoogleOauth,
    ImportLocalBackup,
    ExportLocalBackup,
    StartUploadPreview,
    CheckRemote,
    PullPreview,
    RestoreLatestBackup,
    SaveConfiguration,
    ApplyPreview,
    StartUpload,
    ForceUpload,
    FinishScopeEdit,
    BeginInputSelection {
        input: SettingsInput,
        event: MouseDownEvent,
        source_window: AnyWindowHandle,
    },
    UpdateInputSelection {
        event: MouseMoveEvent,
        source_window: AnyWindowHandle,
    },
    UpdateInputAnchor {
        anchor: oxideterm_gpui_ui::text_input::TextInputAnchor,
        source_window: AnyWindowHandle,
    },
    UpdateSelectAnchor {
        anchor: OverlayAnchor,
        source_window: AnyWindowHandle,
    },
    ClearRollbackBackups,
    RestoreRollbackBackup {
        signature: u64,
    },
    DeleteRollbackBackup {
        signature: u64,
    },
    ClearHistory,
}

/// Groups the Cloud Sync controller lifecycle and its ephemeral GPUI view state.
pub(super) struct CloudSyncWorkspaceEntity {
    pub(super) controller: CloudSyncControllerState,
    pub(super) view: CloudSyncViewState,
    delivery_wake: crate::workspace::delivery::ActiveDeliveryWake,
    pending_deliveries: VecDeque<CloudSyncDelivery>,
    delivery_closed: bool,
    _delivery_task: Task<()>,
    auto_upload_task: Option<Task<()>>,
    dirty_refresh_task: Option<Task<()>>,
    confirm_exit_generation: u64,
    confirm_exit_task: Option<Task<()>>,
}

impl EventEmitter<CloudSyncWorkspaceEvent> for CloudSyncWorkspaceEntity {}

impl CloudSyncWorkspaceEntity {
    pub(in crate::workspace) fn operation_in_flight(&self) -> bool {
        self.controller.delivery_rx.is_some() || self.controller.active_action.is_some()
    }

    pub(in crate::workspace) fn ai_snapshot(&self) -> serde_json::Value {
        let state = self.controller.store.state();
        let settings = &state.settings;
        let scope = normalize_sync_scope(Some(&state.sync_scope), &[]);
        serde_json::json!({
            "configured": !settings.endpoint.trim().is_empty()
                || !settings.git_repository.trim().is_empty()
                || !settings.s3_bucket.trim().is_empty(),
            "configuration": {
                "backendType": settings.backend_type,
                "authMode": settings.auth_mode,
                "endpoint": cloud_sync_location_for_ai(&settings.endpoint),
                "namespace": settings.namespace,
                "s3Bucket": settings.s3_bucket,
                "s3Region": settings.s3_region,
                "gitRepository": cloud_sync_location_for_ai(&settings.git_repository),
                "gitBranch": settings.git_branch,
                "githubOauthClientId": settings.github_oauth_client_id,
                "microsoftOauthClientId": settings.microsoft_oauth_client_id,
                "googleOauthClientId": settings.google_oauth_client_id,
                "autoUploadEnabled": settings.auto_upload_enabled,
                "autoUploadIntervalMins": settings.auto_upload_interval_mins,
                "defaultConflictStrategy": settings.default_conflict_strategy,
            },
            "scope": scope,
            "status": format!("{:?}", state.status).to_lowercase(),
            "activeAction": self.controller.active_action,
            "progress": self.controller.progress.as_ref().map(|progress| serde_json::json!({
                "stage": format!("{:?}", progress.stage).to_lowercase(),
                "current": progress.current,
                "total": progress.total,
                "message": progress.message,
            })),
            "localDirty": state.local_dirty,
            "remoteExists": state.remote_exists,
            "blockedByConflict": state.auto_upload_blocked_by_conflict,
            "hasConflict": state.conflict_details.is_some(),
            "lastSyncAt": state.last_sync_at,
            "lastUploadAt": state.last_upload_at,
            "lastCheckAt": state.last_check_at,
            "lastError": state.last_error,
            "historyCount": state.sync_history.len(),
        })
    }

    pub(super) fn new(
        store: oxideterm_cloud_sync::state::CloudSyncStateStore,
        cx: &mut Context<Self>,
    ) -> Self {
        // Build the form projection before moving the loaded store into the controller.
        let view = CloudSyncViewState::new(&store.state().settings);
        let delivery_wake = crate::workspace::delivery::ActiveDeliveryWake::default();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Worker completion after window release must not retain or wake the
            // released Cloud Sync Entity.
            release_wake.stop();
        })
        .detach();
        let task_wake = delivery_wake.clone();
        let delivery_task = cx.spawn(async move |cloud_sync, cx| {
            loop {
                task_wake.wait().await;
                let should_drain = task_wake.take();
                let stopped = task_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = cloud_sync
                        .update(cx, |cloud_sync, cx| cloud_sync.drain_deliveries(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        task_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        });
        Self {
            controller: CloudSyncControllerState::new(store),
            view,
            delivery_wake,
            pending_deliveries: VecDeque::new(),
            delivery_closed: false,
            _delivery_task: delivery_task,
            auto_upload_task: None,
            dirty_refresh_task: None,
            confirm_exit_generation: 0,
            confirm_exit_task: None,
        }
    }

    pub(super) fn begin_delivery(
        &mut self,
        action: &'static str,
        cx: &mut Context<Self>,
    ) -> crate::workspace::delivery::ActiveDeliverySender<CloudSyncDelivery> {
        let (sender, receiver) =
            crate::workspace::delivery::ActiveDeliverySender::channel_with_wake(
                self.delivery_wake.clone(),
            );
        self.controller.delivery_rx = Some(receiver);
        self.controller.active_action = Some(action);
        self.delivery_closed = false;
        cx.notify();
        sender
    }

    fn section_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_SECTION_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_SECTION_LIST_OVERSCAN,
        )
    }

    fn has_pending_preview(&self) -> bool {
        self.view.pending_preview.is_some() || self.view.upload_preview.is_some()
    }

    fn sections(&self) -> Vec<CloudSyncSection> {
        cloud_sync_sections(
            self.controller.store.state(),
            self.has_pending_preview(),
            self.view.active_tab,
        )
    }

    fn section_signature(&self, section: CloudSyncSection) -> u64 {
        cloud_sync_section_signature(
            section,
            self.controller.store.state(),
            &self.view.form.backend_type,
            &self.view.form.auth_mode,
            &self.view.form.default_conflict_strategy,
            self.controller.delivery_rx.is_some(),
            self.has_pending_preview(),
            self.view.preview_selection.is_some(),
            self.controller.progress.is_some(),
            self.view.active_tab,
        )
    }

    fn sync_section_rows(&mut self) {
        let section_rows = self.sections();
        let signatures = section_rows
            .iter()
            .copied()
            .map(|section| self.section_signature(section))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.view.section_list_state,
            &mut self.view.section_list_cache.borrow_mut(),
            "cloud-sync",
            &signatures,
            Self::section_list_spec(),
        );
        self.view.section_rows = section_rows;
    }

    fn section_at(&self, index: usize) -> Option<(CloudSyncSection, usize)> {
        self.view
            .section_rows
            .get(index)
            .copied()
            .map(|section| (section, self.view.section_rows.len()))
    }

    fn close_select_for_scroll(&mut self, cx: &mut Context<Self>) {
        if close_cloud_sync_select_on_container_scroll(
            &mut self.view.open_select,
            &mut self.view.focused_select,
            &mut self.view.select_highlighted,
        ) {
            cx.notify();
        }
    }

    pub(super) fn take_deliveries(&mut self) -> (VecDeque<CloudSyncDelivery>, bool) {
        (
            std::mem::take(&mut self.pending_deliveries),
            std::mem::take(&mut self.delivery_closed),
        )
    }

    fn drain_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.controller.delivery_rx.as_ref() else {
            return false;
        };
        let batch = crate::workspace::delivery::drain_channel(
            receiver,
            crate::workspace::delivery::USER_ACTION_DELIVERY_BUDGET,
        );
        let has_deliveries = !batch.items.is_empty();
        self.pending_deliveries.extend(batch.items);
        if batch.disconnected {
            self.controller.delivery_rx = None;
            self.delivery_closed = true;
        }
        if has_deliveries || batch.disconnected {
            cx.emit(CloudSyncWorkspaceEvent::DeliveriesReady);
            cx.notify();
        }
        batch.outcome.backlog_remaining
    }

    pub(super) fn reschedule_auto_upload(&mut self, cx: &mut Context<Self>) {
        self.controller.auto_upload_generation =
            self.controller.auto_upload_generation.wrapping_add(1);
        self.auto_upload_task.take();
        if !self.controller.store.state().settings.auto_upload_enabled {
            return;
        }
        let generation = self.controller.auto_upload_generation;
        let interval = Duration::from_secs_f64(
            self.controller
                .store
                .state()
                .settings
                .auto_upload_interval_mins
                .max(5.0)
                * 60.0,
        );
        self.auto_upload_task = Some(cx.spawn(async move |cloud_sync, cx| {
            loop {
                Timer::after(interval).await;
                let keep_running = cloud_sync
                    .update(cx, |cloud_sync, cx| {
                        if cloud_sync.controller.auto_upload_generation != generation
                            || !cloud_sync
                                .controller
                                .store
                                .state()
                                .settings
                                .auto_upload_enabled
                        {
                            return false;
                        }
                        cx.emit(CloudSyncWorkspaceEvent::AutoUploadDue { generation });
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        }));
    }

    pub(super) fn queue_dirty_refresh(&mut self, cx: &mut Context<Self>) {
        self.controller.dirty_refresh_generation =
            self.controller.dirty_refresh_generation.wrapping_add(1);
        let generation = self.controller.dirty_refresh_generation;
        self.controller.dirty_refresh_scheduled = true;
        self.dirty_refresh_task.take();
        self.dirty_refresh_task = Some(cx.spawn(async move |cloud_sync, cx| {
            Timer::after(Duration::from_millis(300)).await;
            let _ = cloud_sync.update(cx, |cloud_sync, cx| {
                if !cloud_sync.accept_dirty_refresh_generation(generation) {
                    return;
                }
                cx.emit(CloudSyncWorkspaceEvent::DirtyRefreshDue { generation });
            });
        }));
    }

    fn accept_dirty_refresh_generation(&mut self, generation: u64) -> bool {
        if self.controller.dirty_refresh_generation != generation {
            return false;
        }
        self.controller.dirty_refresh_scheduled = false;
        true
    }

    pub(super) fn schedule_confirm_exit(
        &mut self,
        presence_generation: u64,
        duration: Duration,
        cx: &mut Context<Self>,
    ) {
        self.confirm_exit_generation = self.confirm_exit_generation.wrapping_add(1);
        let generation = self.confirm_exit_generation;
        self.confirm_exit_task.take();
        self.confirm_exit_task = Some(cx.spawn(async move |cloud_sync, cx| {
            Timer::after(duration).await;
            let _ = cloud_sync.update(cx, |cloud_sync, cx| {
                if cloud_sync.confirm_exit_generation == generation {
                    cx.emit(CloudSyncWorkspaceEvent::ConfirmExitFinished {
                        presence_generation,
                    });
                }
            });
        }));
    }

    #[cfg(test)]
    fn delivery_wake(&self) -> crate::workspace::delivery::ActiveDeliveryWake {
        self.delivery_wake.clone()
    }
}

fn cloud_sync_location_for_ai(value: &str) -> String {
    let mut sanitized = oxideterm_ai::sanitize_for_ai(value);
    let Ok(mut parsed) = url::Url::parse(&sanitized) else {
        return sanitized;
    };
    if !parsed.username().is_empty() || parsed.password().is_some() {
        // Location fields are configuration, but embedded URL userinfo is still a secret.
        let _ = parsed.set_username("[REDACTED]");
        let _ = parsed.set_password(Some("[REDACTED]"));
        sanitized = parsed.to_string();
    }
    sanitized
}

impl CloudSyncDeliverySink for crate::workspace::delivery::ActiveDeliverySender<CloudSyncDelivery> {
    fn send(&self, delivery: CloudSyncDelivery) -> Result<(), CloudSyncDelivery> {
        crate::workspace::delivery::ActiveDeliverySender::send(self, delivery)
            .map_err(|error| error.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_cloud_sync::progress::CloudSyncProgressStage;

    fn test_cloud_sync_entity(cx: &mut TestAppContext) -> Entity<CloudSyncWorkspaceEntity> {
        let store = oxideterm_cloud_sync::state::CloudSyncStateStore::load(
            "target/cloud-sync-entity-test-state.json",
        )
        .expect("test cloud sync store");
        cx.new(|cx| CloudSyncWorkspaceEntity::new(store, cx))
    }

    #[test]
    fn cloud_sync_location_for_ai_redacts_url_userinfo() {
        let sanitized =
            cloud_sync_location_for_ai("https://cloud-user:cloud-password@sync.example.test/root");

        assert!(sanitized.contains("%5BREDACTED%5D"));
        assert!(!sanitized.contains("cloud-user"));
        assert!(!sanitized.contains("cloud-password"));
    }

    #[gpui::test]
    fn hidden_surface_delivery_is_woken_and_retained_by_entity(cx: &mut TestAppContext) {
        let entity = test_cloud_sync_entity(cx);
        let sender = entity.update(cx, |cloud_sync, cx| cloud_sync.begin_delivery("check", cx));

        sender
            .send(CloudSyncDelivery::Progress(CloudSyncProgress {
                stage: CloudSyncProgressStage::FetchMetadata,
                current: 1.0,
                total: 2.0,
                message: None,
            }))
            .expect("delivery");
        drop(sender);
        cx.run_until_parked();

        let (deliveries, closed) =
            entity.update(cx, |cloud_sync, _cx| cloud_sync.take_deliveries());
        assert_eq!(deliveries.len(), 1);
        assert!(matches!(
            deliveries.front(),
            Some(CloudSyncDelivery::Progress(progress))
                if progress.stage == CloudSyncProgressStage::FetchMetadata
        ));
        assert!(closed);
    }

    #[gpui::test]
    fn entity_release_stops_delivery_waiter(cx: &mut TestAppContext) {
        let entity = test_cloud_sync_entity(cx);
        let wake = entity.read_with(cx, |cloud_sync, _cx| cloud_sync.delivery_wake());

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(wake.is_stopped());
    }

    #[gpui::test]
    fn stale_dirty_refresh_generation_cannot_complete(cx: &mut TestAppContext) {
        let entity = test_cloud_sync_entity(cx);
        entity.update(cx, |cloud_sync, cx| cloud_sync.queue_dirty_refresh(cx));
        let stale_generation = entity.read_with(cx, |cloud_sync, _cx| {
            cloud_sync.controller.dirty_refresh_generation
        });
        entity.update(cx, |cloud_sync, cx| cloud_sync.queue_dirty_refresh(cx));
        let current_generation = entity.read_with(cx, |cloud_sync, _cx| {
            cloud_sync.controller.dirty_refresh_generation
        });

        assert!(!entity.update(cx, |cloud_sync, _cx| {
            cloud_sync.accept_dirty_refresh_generation(stale_generation)
        }));
        assert!(entity.update(cx, |cloud_sync, _cx| {
            cloud_sync.accept_dirty_refresh_generation(current_generation)
        }));
    }
}
