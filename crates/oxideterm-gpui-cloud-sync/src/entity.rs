use gpui::{
    AnyWindowHandle, Context, EventEmitter, ListAlignment, ListState, MouseDownEvent,
    MouseMoveEvent, Task, Timer, px,
};
use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, normalize_sync_scope, progress::CloudSyncProgress,
    service::CloudSyncLocalSnapshot,
};
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::{
    confirm::ConfirmDialogAction,
    select::{BrowserFocusOrigin, OverlayAnchor},
    virtual_list::{
        TauriVirtualListSpec, VirtualListSignatureCache,
        sync_tauri_variable_list_state_by_signatures,
    },
};
use oxideterm_settings_model::CloudSyncFormDraft;
use oxideterm_workspace::delivery;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    time::Duration,
};

use crate::{
    CloudSyncConfirm, CloudSyncDelivery, CloudSyncDeliverySink, CloudSyncPendingPreview,
    CloudSyncPreviewSelection, CloudSyncSection, CloudSyncSectionDiffItem, CloudSyncSelect,
    CloudSyncSelectAction, CloudSyncTab, CloudSyncUploadSelection,
    close_cloud_sync_select_on_container_scroll, cloud_sync_history_signature,
    cloud_sync_rollback_backup_signature, cloud_sync_section_signature, cloud_sync_sections,
};

const CLOUD_SYNC_SECTION_LIST_INITIAL_ITEM_COUNT: usize = 7;
const CLOUD_SYNC_SECTION_LIST_ESTIMATED_HEIGHT: f32 = 240.0;
const CLOUD_SYNC_SECTION_LIST_OVERSCAN: usize = 1;
const CLOUD_SYNC_ROLLBACK_BACKUP_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub const CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT: f32 = 72.0;
const CLOUD_SYNC_ROLLBACK_BACKUP_LIST_OVERSCAN: usize = 4;
const CLOUD_SYNC_HISTORY_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub const CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT: f32 = 72.0;
const CLOUD_SYNC_HISTORY_LIST_OVERSCAN: usize = 4;

#[derive(Clone)]
pub struct CloudSyncLocalSnapshotCache {
    pub generation: u64,
    pub result: std::result::Result<CloudSyncLocalSnapshot, String>,
}

#[derive(Clone)]
pub struct CloudSyncUploadDiffCache {
    pub generation: u64,
    pub items: Vec<CloudSyncSectionDiffItem>,
}

/// Owns the persisted service and asynchronous operation lifecycle for Cloud Sync.
pub struct CloudSyncControllerState {
    pub store: oxideterm_cloud_sync::state::CloudSyncStateStore,
    pub service: oxideterm_cloud_sync::operation::CloudSyncOperationService,
    pub progress: Option<CloudSyncProgress>,
    pub delivery_rx: Option<std::sync::mpsc::Receiver<CloudSyncDelivery>>,
    pub active_action: Option<&'static str>,
    pub auto_upload_generation: u64,
    pub dirty_refresh_scheduled: bool,
    pub dirty_refresh_generation: u64,
    pub upload_after_current: Option<bool>,
    pub causal_pending: Option<oxideterm_cloud_sync::operation::PreparedSync>,
    pub password_change: Option<zeroize::Zeroizing<String>>,
    pub(crate) local_file_task: Option<Task<()>>,
    pub(crate) local_file_result: Option<crate::runtime::LocalFileResult>,
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
            causal_pending: None,
            password_change: None,
            local_file_task: None,
            local_file_result: None,
        }
    }
}

/// Owns Cloud Sync form drafts, navigation, dialogs, previews, and virtual-list caches.
pub struct CloudSyncViewState {
    pub form: CloudSyncFormDraft,
    pub local_file_mode: bool,
    section_rows: Vec<CloudSyncSection>,
    pub section_list_state: ListState,
    pub section_list_cache: RefCell<VirtualListSignatureCache>,
    pub snapshot_cache_generation: Cell<u64>,
    pub local_snapshot_cache: RefCell<Option<CloudSyncLocalSnapshotCache>>,
    pub upload_diff_cache: RefCell<Option<CloudSyncUploadDiffCache>>,
    pub rollback_backup_list_state: ListState,
    pub rollback_backup_list_cache: RefCell<VirtualListSignatureCache>,
    pub history_list_state: ListState,
    pub history_list_cache: RefCell<VirtualListSignatureCache>,
    pub open_select: Option<CloudSyncSelect>,
    pub focused_select: Option<CloudSyncSelect>,
    pub select_focus_origin: Option<BrowserFocusOrigin>,
    pub select_highlighted: Option<(CloudSyncSelect, usize)>,
    pub confirm: Option<CloudSyncConfirm>,
    pub confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub confirm_focused_action: Option<ConfirmDialogAction>,
    pub pending_preview: Option<CloudSyncPendingPreview>,
    pub causal_summary: Option<oxideterm_cloud_sync::operation::SyncPlanSummary>,
    pub causal_choices: std::collections::BTreeMap<usize, usize>,
    pub causal_conflicts: Vec<oxideterm_cloud_sync::operation::SyncConflictPreview>,
    pub upload_preview: Option<CloudSyncPendingPreview>,
    pub preview_selection: Option<CloudSyncPreviewSelection>,
    pub upload_selection: Option<CloudSyncUploadSelection>,
    pub active_tab: CloudSyncTab,
    pub previous_tab: CloudSyncTab,
}

impl CloudSyncViewState {
    pub fn set_active_tab(&mut self, tab: CloudSyncTab) {
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
            local_file_mode: settings.local_file_mode,
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
            causal_summary: None,
            causal_choices: Default::default(),
            causal_conflicts: Vec::new(),
            upload_preview: None,
            preview_selection: None,
            upload_selection: None,
            active_tab: CloudSyncTab::Overview,
            previous_tab: CloudSyncTab::Overview,
        }
    }
}

/// Requests root-only adapters without giving long-lived tasks a root handle.
#[derive(Clone, Debug)]
pub enum CloudSyncWorkspaceEvent {
    LocalFileFinished,
    DeliveriesReady,
    AutoUploadDue { generation: u64 },
    DirtyRefreshDue { generation: u64 },
    ConfirmExitFinished { presence_generation: u64 },
    UiIntent(CloudSyncUiIntent),
}

/// Typed, non-secret actions crossing from Entity-owned virtual rows to root adapters.
#[derive(Clone, Debug)]
pub enum CloudSyncUiIntent {
    SelectTab {
        tab: CloudSyncTab,
    },
    StartGithubOauth,
    StartMicrosoftOauth,
    StartGoogleOauth,
    StartUploadPreview,
    ViewRemote,
    ChooseDirection(oxideterm_cloud_sync::operation::SyncDirection),
    PullPreview,
    RestoreLatestBackup,
    SaveConfiguration,
    ApplyPreview,
    ApplyCausal,
    CancelCausal,
    ChooseCausal {
        conflict: usize,
        candidate: usize,
    },
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
pub struct CloudSyncWorkspaceEntity {
    pub controller: CloudSyncControllerState,
    pub view: CloudSyncViewState,
    delivery_wake: delivery::ActiveDeliveryWake,
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
    pub fn operation_in_flight(&self) -> bool {
        self.controller.delivery_rx.is_some()
            || self.controller.active_action.is_some()
            || self.controller.local_file_task.is_some()
    }

    pub fn apply_configuration_patch(
        &mut self,
        arguments: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<Vec<String>, String> {
        let (current_settings, current_scope) = {
            let state = self.controller.store.state();
            (state.settings.clone(), state.sync_scope.clone())
        };
        let (settings, scope, updated_fields) = crate::apply_cloud_sync_configuration_patch(
            &current_settings,
            &current_scope,
            arguments,
        )?;
        let settings_for_view = settings.clone();
        let state = self.controller.store.state_mut();
        state.settings = settings;
        state.sync_scope = scope;
        state.last_error = None;
        if let Err(error) = self.controller.store.save() {
            // Roll back only non-secret state; protected credentials were never read or moved.
            let message = error.to_string();
            let state = self.controller.store.state_mut();
            state.settings = current_settings.clone();
            state.sync_scope = current_scope.clone();
            state.last_error = Some(message.clone());
            return Err(format!(
                "Failed to save Cloud Sync configuration: {message}"
            ));
        }
        self.view
            .form
            .apply_changed_non_secret_settings(&current_settings, &settings_for_view);
        cx.notify();
        Ok(updated_fields)
    }

    pub fn ai_snapshot(&self) -> serde_json::Value {
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
            "hasConflict": state.auto_upload_blocked_by_conflict || state.conflict_details.is_some(),
            "lastSyncAt": state.last_sync_at,
            "lastUploadAt": state.last_upload_at,
            "lastCheckAt": state.last_check_at,
            "lastError": state.last_error,
            "historyCount": state.sync_history.len(),
        })
    }

    pub fn new(
        store: oxideterm_cloud_sync::state::CloudSyncStateStore,
        cx: &mut Context<Self>,
    ) -> Self {
        // Build the form projection before moving the loaded store into the controller.
        let view = CloudSyncViewState::new(&store.state().settings);
        let delivery_wake = delivery::ActiveDeliveryWake::default();
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

    pub fn begin_delivery(
        &mut self,
        action: &'static str,
        cx: &mut Context<Self>,
    ) -> CloudSyncSender {
        let (sender, receiver) =
            delivery::ActiveDeliverySender::channel_with_wake(self.delivery_wake.clone());
        self.controller.delivery_rx = Some(receiver);
        self.controller.active_action = Some(action);
        self.delivery_closed = false;
        cx.notify();
        CloudSyncSender(sender)
    }

    pub fn section_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_SECTION_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_SECTION_LIST_OVERSCAN,
        )
    }

    pub fn has_pending_preview(&self) -> bool {
        self.view.causal_summary.is_some()
            || self.view.pending_preview.is_some()
            || self.view.upload_preview.is_some()
    }

    fn sections(&self) -> Vec<CloudSyncSection> {
        let mut sections = cloud_sync_sections(
            self.controller.store.state(),
            self.has_pending_preview(),
            self.view.active_tab,
        );
        if self.view.local_file_mode {
            sections.retain(|section| {
                !matches!(
                    section,
                    CloudSyncSection::Guide
                        | CloudSyncSection::ConfigHealth
                        | CloudSyncSection::ConfigNotes
                )
            });
        }
        sections
    }

    fn section_signature(&self, section: CloudSyncSection) -> u64 {
        let signature = cloud_sync_section_signature(
            section,
            self.controller.store.state(),
            &self.view.form.backend_type,
            &self.view.form.auth_mode,
            &self.view.form.default_conflict_strategy,
            self.operation_in_flight(),
            self.has_pending_preview(),
            self.view.preview_selection.is_some(),
            self.controller.progress.is_some(),
            self.view.active_tab,
        );
        use std::hash::{Hash, Hasher};
        let mut preview = std::collections::hash_map::DefaultHasher::new();
        if let Some(summary) = &self.view.causal_summary {
            summary.changed_fields.hash(&mut preview);
            summary.conflicts.len().hash(&mut preview);
            summary.upgrading.hash(&mut preview);
            summary.direction.hash(&mut preview);
            summary.first_sync.hash(&mut preview);
            summary.has_remote.hash(&mut preview);
            self.view.causal_choices.hash(&mut preview);
        }
        signature ^ u64::from(self.view.local_file_mode) ^ preview.finish()
    }

    pub fn sync_section_rows(&mut self) {
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

    pub fn section_at(&self, index: usize) -> Option<(CloudSyncSection, usize)> {
        self.view
            .section_rows
            .get(index)
            .copied()
            .map(|section| (section, self.view.section_rows.len()))
    }

    pub fn close_select_for_scroll(&mut self, cx: &mut Context<Self>) {
        if close_cloud_sync_select_on_container_scroll(
            &mut self.view.open_select,
            &mut self.view.focused_select,
            &mut self.view.select_highlighted,
        ) {
            cx.notify();
        }
    }

    pub fn take_deliveries(&mut self) -> (VecDeque<CloudSyncDelivery>, bool) {
        (
            std::mem::take(&mut self.pending_deliveries),
            std::mem::take(&mut self.delivery_closed),
        )
    }

    fn drain_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.controller.delivery_rx.as_ref() else {
            return false;
        };
        let batch = delivery::drain_channel(receiver, delivery::USER_ACTION_DELIVERY_BUDGET);
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

    pub fn reschedule_auto_upload(&mut self, cx: &mut Context<Self>) {
        self.controller.auto_upload_generation =
            self.controller.auto_upload_generation.wrapping_add(1);
        self.auto_upload_task.take();
        if self.view.local_file_mode || !self.controller.store.state().settings.auto_upload_enabled
        {
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

    pub fn queue_dirty_refresh(&mut self, cx: &mut Context<Self>) {
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

    pub fn schedule_confirm_exit(
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
    fn delivery_wake(&self) -> delivery::ActiveDeliveryWake {
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

#[derive(Clone)]
pub struct CloudSyncSender(delivery::ActiveDeliverySender<CloudSyncDelivery>);

impl CloudSyncDeliverySink for CloudSyncSender {
    fn send(&self, delivery: CloudSyncDelivery) -> Result<(), Box<CloudSyncDelivery>> {
        self.0.send(delivery).map_err(|error| Box::new(error.0))
    }
}

impl CloudSyncWorkspaceEntity {
    pub fn rollback_backup_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_ROLLBACK_BACKUP_LIST_OVERSCAN,
        )
    }

    pub fn history_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_HISTORY_LIST_OVERSCAN,
        )
    }

    pub fn sync_rollback_rows(&mut self) {
        let signatures = self
            .controller
            .store
            .state()
            .rollback_backups
            .iter()
            .map(cloud_sync_rollback_backup_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.view.rollback_backup_list_state,
            &mut self.view.rollback_backup_list_cache.borrow_mut(),
            "cloud-sync-rollback-backups",
            &signatures,
            Self::rollback_backup_list_spec(),
        );
    }

    pub fn sync_history_rows(&mut self) {
        let signatures = self
            .controller
            .store
            .state()
            .sync_history
            .iter()
            .map(cloud_sync_history_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.view.history_list_state,
            &mut self.view.history_list_cache.borrow_mut(),
            "cloud-sync-history",
            &signatures,
            Self::history_list_spec(),
        );
    }

    pub fn apply_select_action(&mut self, action: CloudSyncSelectAction, cx: &mut Context<Self>) {
        if self.operation_in_flight() {
            return;
        }
        let trigger_select = match action {
            CloudSyncSelectAction::LocalFile => {
                self.view.local_file_mode = true;
                self.view.pending_preview = None;
                self.view.upload_preview = None;
                self.view.preview_selection = None;
                self.view.upload_selection = None;
                CloudSyncSelect::Backend
            }
            CloudSyncSelectAction::Backend(backend) => {
                if self.view.local_file_mode {
                    self.view.pending_preview = None;
                    self.view.upload_preview = None;
                    self.view.preview_selection = None;
                    self.view.upload_selection = None;
                }
                self.view.local_file_mode = false;
                if self.view.form.backend_type != backend {
                    self.view.set_active_tab(CloudSyncTab::Configure);
                }
                self.view.form.backend_type = backend.clone();
                if matches!(backend, BackendType::Dropbox) {
                    self.view.form.auth_mode = AuthMode::Bearer;
                } else if matches!(
                    backend,
                    BackendType::GithubGist | BackendType::Git | BackendType::S3
                ) {
                    self.view.form.auth_mode = AuthMode::None;
                }
                CloudSyncSelect::Backend
            }
            CloudSyncSelectAction::AuthMode(auth_mode) => {
                self.view.form.auth_mode = auth_mode;
                CloudSyncSelect::AuthMode
            }
            CloudSyncSelectAction::ConflictStrategy(strategy) => {
                self.view.form.default_conflict_strategy = strategy;
                CloudSyncSelect::ConflictStrategy
            }
        };
        self.reschedule_auto_upload(cx);
        self.close_select();
        self.view.focused_select = Some(trigger_select);
        cx.notify();
    }

    pub fn clear_select_focus(&mut self) {
        self.close_select();
        self.view.focused_select = None;
        self.view.select_focus_origin = None;
    }

    pub(crate) fn toggle_pointer_select(&mut self, select: CloudSyncSelect, selected_index: usize) {
        if self.view.open_select == Some(select) {
            self.close_select();
        } else {
            self.view.open_select = Some(select);
            self.view.focused_select = Some(select);
            self.view.select_focus_origin = Some(BrowserFocusOrigin::Pointer);
            self.view.select_highlighted = Some((select, selected_index));
        }
    }

    pub fn close_select(&mut self) {
        self.view.open_select = None;
        self.view.select_highlighted = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, Entity, TestAppContext};
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
    fn pointer_dropdown_keeps_trigger_focus_until_focus_moves(cx: &mut TestAppContext) {
        let entity = test_cloud_sync_entity(cx);
        entity.update(cx, |sync, _cx| {
            let select = CloudSyncSelect::Backend;
            sync.toggle_pointer_select(select, 1);
            assert_eq!(sync.view.open_select, Some(select));
            assert_eq!(sync.view.select_highlighted, Some((select, 1)));
            assert!(!oxideterm_gpui_ui::select::browser_focus_visible(
                true,
                sync.view.select_focus_origin,
            ));

            sync.toggle_pointer_select(select, 1);
            assert_eq!(sync.view.open_select, None);
            assert_eq!(sync.view.select_highlighted, None);
            assert_eq!(sync.view.focused_select, Some(select));
            assert_eq!(
                sync.view.select_focus_origin,
                Some(BrowserFocusOrigin::Pointer)
            );

            sync.clear_select_focus();
            assert_eq!(sync.view.focused_select, None);
            assert_eq!(sync.view.select_focus_origin, None);
        });
    }

    #[gpui::test]
    fn keyboard_switches_cloud_sync_backend_to_local_files(cx: &mut TestAppContext) {
        let entity = test_cloud_sync_entity(cx);
        let i18n = oxideterm_i18n::I18n::new(oxideterm_i18n::Locale::En);
        entity.update(cx, |sync, cx| {
            sync.view.set_active_tab(CloudSyncTab::Configure);
            sync.view.local_file_mode = false;
            sync.view.focused_select = Some(CloudSyncSelect::Backend);
            for key in ["enter", "end", "enter"] {
                assert!(sync.handle_select_key(
                    &gpui::KeyDownEvent {
                        keystroke: gpui::Keystroke::parse(key).unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    &i18n,
                    cx,
                ));
            }
            assert!(sync.view.local_file_mode);
            assert_eq!(sync.view.open_select, None);
            assert_eq!(sync.view.focused_select, Some(CloudSyncSelect::Backend));
        });
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
}
