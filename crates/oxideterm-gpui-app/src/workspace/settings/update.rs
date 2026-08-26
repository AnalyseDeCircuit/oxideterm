use super::*;
use gpui::Task;

const AUTOMATIC_NATIVE_UPDATE_DELAY: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeUpdateCheckKind {
    Manual,
    Automatic,
}

#[derive(Debug)]
pub(in crate::workspace) enum NativeUpdateUiState {
    Idle,
    Checking,
    UpToDate,
    Available(oxideterm_update::NativeUpdatePackage),
    Downloading(Option<oxideterm_update::ResumableUpdateStatus>),
    Verifying(Option<oxideterm_update::ResumableUpdateStatus>),
    Downloaded(oxideterm_update::NativeUpdateDownload),
    Installing(Option<oxideterm_update::NativeInstallPlan>),
    InstallFinished(oxideterm_update::NativeInstallOutcome),
    Error(String),
}

#[derive(Debug)]
pub(in crate::workspace) enum NativeUpdateRenderState {
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        has_release_notes: bool,
    },
    Downloading(Option<oxideterm_update::ResumableUpdateStatus>),
    Verifying(Option<oxideterm_update::ResumableUpdateStatus>),
    Downloaded,
    Installing(Option<String>),
    InstallFinished {
        status: oxideterm_update::NativeInstallStatus,
        message: String,
    },
    Error(String),
}

pub(in crate::workspace) struct NativeUpdateReleaseNotes {
    pub(in crate::workspace) body: String,
    pub(in crate::workspace) description: Option<String>,
}

#[derive(Debug)]
pub(in crate::workspace) enum NativeUpdateDelivery {
    Progress(oxideterm_update::DownloadProgress),
    Finished(Result<oxideterm_update::NativeUpdateDownload, String>),
    InstallFinished(Result<oxideterm_update::NativeInstallOutcome, String>),
}

pub(super) struct NativeUpdateRuntime {
    state: NativeUpdateUiState,
    receiver: Option<std::sync::mpsc::Receiver<NativeUpdateDelivery>>,
    wake: delivery::ActiveDeliveryWake,
    cancel: Option<Arc<AtomicBool>>,
    cancel_requested: bool,
    package: Option<oxideterm_update::NativeUpdatePackage>,
    check_task: Option<Task<()>>,
    operation_task: Option<Task<()>>,
    automatic_check_task: Option<Task<()>>,
    _delivery_task: Task<()>,
    error_fallback: String,
}

struct NativeUpdateCheckRequest {
    kind: NativeUpdateCheckKind,
    channel: UpdateChannel,
    current_version: String,
    install_flavor: Result<oxideterm_update::InstallFlavor, String>,
    update_proxy: oxideterm_settings::UpdateProxySettings,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl NativeUpdateRuntime {
    pub(super) fn new(cx: &mut Context<SettingsWorkspaceEntity>) -> Self {
        let wake = delivery::ActiveDeliveryWake::default();
        let delivery_wake = wake.clone();
        let release_wake = wake.clone();
        cx.on_release(move |_, _| {
            // External updater work may finish after its window Entity is released.
            release_wake.stop();
        })
        .detach();
        let delivery_task = cx.spawn(async move |settings, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = settings
                        .update(cx, |settings, cx| settings.drain_native_update_delivery(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        // Continue bounded delivery without introducing a timer poll.
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        });

        Self {
            state: NativeUpdateUiState::Idle,
            receiver: None,
            wake,
            cancel: None,
            cancel_requested: false,
            package: None,
            check_task: None,
            operation_task: None,
            automatic_check_task: None,
            _delivery_task: delivery_task,
            error_fallback: String::new(),
        }
    }
}

impl SettingsWorkspaceEntity {
    pub(in crate::workspace) fn native_update_render_state(&self) -> NativeUpdateRenderState {
        match &self.native_update.state {
            NativeUpdateUiState::Idle => NativeUpdateRenderState::Idle,
            NativeUpdateUiState::Checking => NativeUpdateRenderState::Checking,
            NativeUpdateUiState::UpToDate => NativeUpdateRenderState::UpToDate,
            NativeUpdateUiState::Available(package) => NativeUpdateRenderState::Available {
                version: package.version.clone(),
                has_release_notes: package
                    .body
                    .as_deref()
                    .is_some_and(|body| !body.trim().is_empty()),
            },
            NativeUpdateUiState::Downloading(status) => {
                NativeUpdateRenderState::Downloading(status.clone())
            }
            NativeUpdateUiState::Verifying(status) => {
                NativeUpdateRenderState::Verifying(status.clone())
            }
            NativeUpdateUiState::Downloaded(_) => NativeUpdateRenderState::Downloaded,
            NativeUpdateUiState::Installing(plan) => {
                NativeUpdateRenderState::Installing(plan.as_ref().and_then(|plan| {
                    (plan.strategy != oxideterm_update::InstallStrategy::PortableReplaceArchive)
                        .then(|| plan.summary.clone())
                }))
            }
            NativeUpdateUiState::InstallFinished(outcome) => {
                NativeUpdateRenderState::InstallFinished {
                    status: outcome.status.clone(),
                    message: outcome.message.clone(),
                }
            }
            NativeUpdateUiState::Error(error) => NativeUpdateRenderState::Error(error.clone()),
        }
    }

    pub(in crate::workspace) fn native_update_package_description(&self) -> Option<String> {
        self.native_update
            .package
            .as_ref()
            .map(|package| format!("v{} → v{}", package.current_version, package.version))
    }

    pub(in crate::workspace) fn native_update_has_release_notes(&self) -> bool {
        self.native_update
            .package
            .as_ref()
            .and_then(|package| package.body.as_deref())
            .is_some_and(|body| !body.trim().is_empty())
    }

    pub(in crate::workspace) fn native_update_release_notes(
        &self,
    ) -> Option<NativeUpdateReleaseNotes> {
        let package = self.native_update.package.as_ref()?;
        let body = package
            .body
            .as_deref()
            .filter(|body| !body.trim().is_empty())?
            .to_string();
        let description = package
            .date
            .as_ref()
            .map(|date| format!("v{} · {date}", package.version))
            .or_else(|| Some(format!("v{}", package.version)));
        Some(NativeUpdateReleaseNotes { body, description })
    }

    pub(in crate::workspace) fn native_update_message(&self) -> Option<&str> {
        match &self.native_update.state {
            NativeUpdateUiState::InstallFinished(outcome) => Some(&outcome.message),
            NativeUpdateUiState::Error(error) => Some(error),
            _ => None,
        }
    }

    pub(in crate::workspace) fn schedule_automatic_native_update_check(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.native_update.automatic_check_task.is_some() {
            return;
        }

        // Delay manifest traffic until session restoration and the first frame settle.
        self.native_update.automatic_check_task = Some(cx.spawn(async move |settings, cx| {
            Timer::after(AUTOMATIC_NATIVE_UPDATE_DELAY).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.native_update.automatic_check_task = None;
                if matches!(settings.native_update.state, NativeUpdateUiState::Idle) {
                    cx.emit(SettingsWorkspaceEvent::RequestAutomaticNativeUpdateCheck);
                }
            });
        }));
    }

    fn start_native_update_check(
        &mut self,
        request: NativeUpdateCheckRequest,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.native_update.receiver.is_some()
            || matches!(
                self.native_update.state,
                NativeUpdateUiState::Checking
                    | NativeUpdateUiState::Downloading(_)
                    | NativeUpdateUiState::Verifying(_)
                    | NativeUpdateUiState::Installing(_)
            )
        {
            return false;
        }

        self.native_update.state = NativeUpdateUiState::Checking;
        self.native_update.package = None;
        cx.emit(SettingsWorkspaceEvent::ResetNativeUpdateOverlay);

        let install_flavor = match request.install_flavor {
            Ok(install_flavor) => install_flavor,
            Err(error) => {
                self.native_update.state = if request.kind == NativeUpdateCheckKind::Automatic {
                    NativeUpdateUiState::Idle
                } else {
                    NativeUpdateUiState::Error(error)
                };
                cx.notify();
                return true;
            }
        };

        self.native_update.check_task = Some(cx.spawn(async move |settings, cx| {
            let result = request
                .runtime
                .spawn(async move {
                    let client = oxideterm_update::NativeUpdateClient::with_update_proxy(
                        &request.update_proxy,
                    )?;
                    client
                        .check(oxideterm_update::NativeUpdateRequest::current(
                            request.channel,
                            request.current_version,
                            install_flavor,
                        ))
                        .await
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));

            let _ = settings.update(cx, |settings, cx| {
                settings.native_update.check_task = None;
                settings.native_update.state = match result {
                    Ok(oxideterm_update::NativeUpdateStatus::UpToDate)
                        if request.kind == NativeUpdateCheckKind::Automatic =>
                    {
                        NativeUpdateUiState::Idle
                    }
                    Ok(oxideterm_update::NativeUpdateStatus::UpToDate) => {
                        NativeUpdateUiState::UpToDate
                    }
                    Ok(oxideterm_update::NativeUpdateStatus::Available(package)) => {
                        settings.native_update.package = Some(package.clone());
                        cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);
                        NativeUpdateUiState::Available(package)
                    }
                    Err(_error) if request.kind == NativeUpdateCheckKind::Automatic => {
                        NativeUpdateUiState::Idle
                    }
                    Err(error) => NativeUpdateUiState::Error(error),
                };
                cx.notify();
            });
        }));
        cx.notify();
        true
    }

    fn start_native_update_download(
        &mut self,
        directory: std::path::PathBuf,
        update_proxy: oxideterm_settings::UpdateProxySettings,
        runtime: Arc<tokio::runtime::Runtime>,
        error_fallback: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.native_update.receiver.is_some() {
            return false;
        }
        let package = match &self.native_update.state {
            NativeUpdateUiState::Available(package) => package.clone(),
            _ => return false,
        };

        let (sender, receiver) =
            delivery::ActiveDeliverySender::channel_with_wake(self.native_update.wake.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        self.native_update.receiver = Some(receiver);
        // The worker and owner share only the atomic cancellation capability.
        self.native_update.cancel = Some(cancel.clone());
        self.native_update.cancel_requested = false;
        self.native_update.error_fallback = error_fallback;
        self.native_update.state = NativeUpdateUiState::Downloading(None);
        cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);

        self.native_update.operation_task = Some(cx.spawn(async move |_settings, _cx| {
            let _ = runtime
                .spawn(async move {
                    let result = async {
                        let client =
                            oxideterm_update::NativeUpdateClient::with_update_proxy(&update_proxy)?;
                        // Preserve the resumable cache and verification contract from Tauri.
                        client
                            .download_resumable_package(package, &directory, cancel, |progress| {
                                let _ = sender.send(NativeUpdateDelivery::Progress(progress));
                            })
                            .await
                    }
                    .await
                    .map_err(|error: oxideterm_update::NativeUpdateError| error.to_string());
                    let _ = sender.send(NativeUpdateDelivery::Finished(result));
                })
                .await;
        }));
        cx.notify();
        true
    }

    fn start_native_update_install(
        &mut self,
        context: Result<oxideterm_update::NativeInstallContext, String>,
        cleanup_directory: std::path::PathBuf,
        runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.native_update.receiver.is_some() {
            return false;
        }
        let context = match context {
            Ok(context) => context,
            Err(error) => {
                self.native_update.state = NativeUpdateUiState::Error(error);
                cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);
                cx.notify();
                return true;
            }
        };
        let download =
            match std::mem::replace(&mut self.native_update.state, NativeUpdateUiState::Idle) {
                NativeUpdateUiState::Downloaded(download) => download,
                state => {
                    self.native_update.state = state;
                    return false;
                }
            };
        let plan = oxideterm_update::plan_native_install(&download.path, &context);
        let cleanup_version = download.package.version;
        let (sender, receiver) =
            delivery::ActiveDeliverySender::channel_with_wake(self.native_update.wake.clone());
        self.native_update.receiver = Some(receiver);
        self.native_update.cancel = None;
        self.native_update.cancel_requested = false;
        self.native_update.state = NativeUpdateUiState::Installing(Some(plan.clone()));
        cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);

        self.native_update.operation_task = Some(cx.spawn(async move |_settings, _cx| {
            let _ = runtime
                .spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        // Installation stays in the updater crate; the Entity owns its result.
                        oxideterm_update::execute_install_plan(&plan)
                    })
                    .await;
                    let result = result
                        .map_err(|error| error.to_string())
                        .and_then(|result| result.map_err(|error| error.to_string()));
                    if result.is_ok() {
                        let _ = oxideterm_update::prune_resumable_update_cache(
                            &cleanup_directory,
                            Some(&cleanup_version),
                        )
                        .await;
                    }
                    let _ = sender.send(NativeUpdateDelivery::InstallFinished(result));
                })
                .await;
        }));
        cx.notify();
        true
    }

    fn cancel_native_update(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.native_update.cancel.as_ref() {
            cancel.store(true, Ordering::Relaxed);
            self.native_update.cancel_requested = true;
        }
        self.native_update.state = self.native_update_available_state();
        cx.notify();
    }

    fn drain_native_update_delivery(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.native_update.receiver.as_ref() else {
            return false;
        };
        let delivery_batch =
            delivery::drain_channel(receiver, delivery::NOTIFICATION_DELIVERY_BUDGET);

        for update in delivery_batch.items {
            self.handle_native_update_delivery(update, cx);
        }
        if delivery_batch.disconnected {
            self.native_update.receiver = None;
            self.native_update.cancel = None;
            self.native_update.cancel_requested = false;
            self.native_update.operation_task = None;
        }
        cx.notify();
        delivery_batch.outcome.backlog_remaining
    }

    fn handle_native_update_delivery(
        &mut self,
        delivery: NativeUpdateDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery {
            NativeUpdateDelivery::Progress(progress) => {
                let stage = progress.status.stage;
                if self.native_update.cancel_requested
                    && stage != oxideterm_update::NativeUpdateStage::Cancelled
                {
                    return;
                }
                self.native_update.state = match stage {
                    oxideterm_update::NativeUpdateStage::Downloading => {
                        NativeUpdateUiState::Downloading(Some(progress.status))
                    }
                    oxideterm_update::NativeUpdateStage::Verifying
                    | oxideterm_update::NativeUpdateStage::Ready => {
                        NativeUpdateUiState::Verifying(Some(progress.status))
                    }
                    oxideterm_update::NativeUpdateStage::Error => NativeUpdateUiState::Error(
                        progress
                            .status
                            .error_message
                            .unwrap_or_else(|| self.native_update.error_fallback.clone()),
                    ),
                    oxideterm_update::NativeUpdateStage::Cancelled => {
                        self.native_update_available_state()
                    }
                };
                if stage == oxideterm_update::NativeUpdateStage::Error {
                    cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);
                }
            }
            NativeUpdateDelivery::Finished(Ok(download)) => {
                self.native_update.package = Some(download.package.clone());
                self.native_update.state = NativeUpdateUiState::Downloaded(download);
                self.native_update.cancel = None;
                self.native_update.cancel_requested = false;
                cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);
            }
            NativeUpdateDelivery::Finished(Err(error)) => {
                if error.contains("update cancelled") {
                    self.native_update.state = self.native_update_available_state();
                } else {
                    self.native_update.state = NativeUpdateUiState::Error(error);
                    cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);
                    cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateToast(
                        SettingsWorkspaceToast::Error,
                    ));
                }
                self.native_update.cancel = None;
                self.native_update.cancel_requested = false;
            }
            NativeUpdateDelivery::InstallFinished(Ok(outcome)) => {
                let is_success =
                    outcome.status != oxideterm_update::NativeInstallStatus::ManualActionRequired;
                let should_quit_app = outcome.should_quit_app;
                self.native_update.state = NativeUpdateUiState::InstallFinished(outcome);
                self.native_update.receiver = None;
                self.native_update.operation_task = None;
                cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);
                cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateToast(
                    if is_success {
                        SettingsWorkspaceToast::Success
                    } else {
                        SettingsWorkspaceToast::Warning
                    },
                ));
                if should_quit_app {
                    cx.emit(SettingsWorkspaceEvent::RequestQuitAfterNativeUpdate);
                }
            }
            NativeUpdateDelivery::InstallFinished(Err(error)) => {
                self.native_update.state = NativeUpdateUiState::Error(error);
                self.native_update.receiver = None;
                self.native_update.operation_task = None;
                cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateNotification);
                cx.emit(SettingsWorkspaceEvent::ShowNativeUpdateToast(
                    SettingsWorkspaceToast::Error,
                ));
            }
        }
    }

    fn native_update_available_state(&self) -> NativeUpdateUiState {
        self.native_update
            .package
            .clone()
            .map(NativeUpdateUiState::Available)
            .unwrap_or(NativeUpdateUiState::Idle)
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_settings_workspace_event(
        &mut self,
        settings: Entity<SettingsWorkspaceEntity>,
        event: &SettingsWorkspaceEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SettingsWorkspaceEvent::ExternalStoresChanged => {
                // The Entity owns change detection; Workspace applies the
                // cross-system settings and connection-store side effects.
                let _ = self.reload_after_external_sync(cx);
            }
            SettingsWorkspaceEvent::ResetNativeUpdateOverlay => {
                self.native_update_notification_open = false;
                self.native_update_notification_presence.reopen();
                self.overlay.update(cx, |overlay, cx| {
                    if overlay.confirm_snapshot().is_some_and(|snapshot| {
                        matches!(
                            snapshot.kind,
                            WorkspaceOverlayConfirmKind::NativeUpdateReleaseNotes
                        )
                    }) {
                        overlay.begin_confirm_exit(false, Duration::ZERO, cx);
                    }
                });
                // A new package must not inherit an old changelog scroll position.
                self.native_update_release_notes_scroll = MarkdownVirtualListScrollHandle::new();
                cx.notify();
            }
            SettingsWorkspaceEvent::ShowNativeUpdateNotification => {
                self.show_native_update_notification();
                cx.notify();
            }
            SettingsWorkspaceEvent::ShowNativeUpdateToast(toast) => {
                let portable_replacement = self.native_update_is_portable(cx)
                    && matches!(
                        settings.read(cx).native_update_render_state(),
                        NativeUpdateRenderState::InstallFinished {
                            status: oxideterm_update::NativeInstallStatus::ReplacementScheduled,
                            ..
                        }
                    );
                let message = if portable_replacement {
                    self.i18n.t("settings_view.help.replacement_scheduled")
                } else {
                    let Some(message) =
                        settings.read(cx).native_update_message().map(str::to_owned)
                    else {
                        return;
                    };
                    message
                };
                let variant = match toast {
                    SettingsWorkspaceToast::Success => TerminalNoticeVariant::Success,
                    SettingsWorkspaceToast::Warning => TerminalNoticeVariant::Warning,
                    SettingsWorkspaceToast::Error => TerminalNoticeVariant::Error,
                };
                self.push_ai_settings_toast(message, variant, cx);
            }
            SettingsWorkspaceEvent::RequestAutomaticNativeUpdateCheck => {
                self.check_native_update_with_kind(NativeUpdateCheckKind::Automatic, cx);
            }
            SettingsWorkspaceEvent::RequestQuitAfterNativeUpdate => {
                self.schedule_native_update_quit(cx);
            }
            SettingsWorkspaceEvent::DataDirectoryConfirmOpened => {
                self.reset_standard_confirm_focus();
                cx.notify();
            }
            SettingsWorkspaceEvent::DataDirectoryOperationReady => {
                let results =
                    settings.update(cx, |settings, _cx| settings.take_data_directory_results());
                for result in results {
                    match result {
                        DataDirectoryOperationResult::Changed => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.general.data_directory_changed"),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        DataDirectoryOperationResult::Reset => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.general.data_directory_reset"),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        DataDirectoryOperationResult::Failed(error) => {
                            self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::BackgroundBlurCommitReady(value) => {
                let value = *value;
                if self.settings_store.settings().terminal.background_blur != value {
                    self.edit_settings(|settings| settings.terminal.background_blur = value, cx);
                }
            }
            SettingsWorkspaceEvent::BackgroundGalleryOperationReady => {
                let results = settings.update(cx, |settings, _cx| {
                    settings.take_background_gallery_results()
                });
                for result in results {
                    match result {
                        BackgroundGalleryOperationResult::Updated(active_path) => {
                            if self.settings_store.settings().terminal.background_image
                                != active_path
                            {
                                self.edit_settings(
                                    move |settings| {
                                        settings.terminal.background_image = active_path
                                    },
                                    cx,
                                );
                            }
                        }
                        BackgroundGalleryOperationResult::Failed => {
                            self.send_settings_notice(
                                self.i18n.t("settings_view.terminal.bg_operation_failed"),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::ThemeImportReady => {
                let results =
                    settings.update(cx, |settings, _cx| settings.take_theme_import_results());
                for result in results {
                    match result {
                        ThemeImportResult::Imported {
                            theme_id,
                            name,
                            value,
                        } => {
                            // The identifier is persisted both as the map key
                            // and as the active theme, requiring two owners.
                            let selected_theme_id = theme_id.clone();
                            self.edit_settings(
                                move |settings| {
                                    settings.custom_themes.insert(theme_id, value);
                                    settings.terminal.theme = selected_theme_id;
                                },
                                cx,
                            );
                            self.send_settings_notice(
                                self.i18n
                                    .t("settings_view.appearance.theme_import_success")
                                    .replace("{{name}}", &name),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        ThemeImportResult::Failed(error) => {
                            self.send_settings_notice(
                                self.i18n
                                    .t("settings_view.appearance.theme_import_error")
                                    .replace("{{error}}", &error),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::ThemeEditorOperationReady => {
                let results =
                    settings.update(cx, |settings, _cx| settings.take_theme_editor_results());
                for result in results {
                    match result {
                        ThemeEditorOperationResult::Save(editor) => {
                            let mut saved_name = None;
                            self.edit_settings(
                                |settings| {
                                    saved_name =
                                        save_theme_editor_snapshot_to_settings(settings, &editor);
                                },
                                cx,
                            );
                            if let Some(name) = saved_name {
                                self.send_settings_notice(
                                    self.i18n
                                        .t("settings_view.appearance.theme_import_success")
                                        .replace("{{name}}", &name),
                                    TerminalNoticeVariant::Success,
                                    cx,
                                );
                            }
                        }
                        ThemeEditorOperationResult::Delete(editor) => {
                            self.edit_settings(
                                move |settings| {
                                    if let Some(theme_id) = editor.edit_theme_id.as_deref() {
                                        delete_custom_theme_from_settings(
                                            settings,
                                            theme_id,
                                            oxideterm_theme::DEFAULT_THEME.id,
                                        );
                                    }
                                },
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::KeybindingFileOperationReady => {
                let results = settings.update(cx, |settings, _cx| {
                    settings.take_keybinding_file_operation_results()
                });
                for result in results {
                    match result {
                        KeybindingFileOperationResult::Exported => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.keybindings.export_success"),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        KeybindingFileOperationResult::ExportFailed => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.keybindings.export_error"),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                        KeybindingFileOperationResult::Imported {
                            overrides: next_overrides,
                            target_window,
                        } => {
                            let side = crate::keybindings::KeybindingSide::current();
                            let runtime_bindings = {
                                let previous_overrides =
                                    &self.settings_store.settings().keybindings.overrides;
                                crate::keybindings::ACTION_DEFINITIONS
                                    .iter()
                                    .flat_map(|definition| {
                                        let previous = crate::keybindings::effective_combo(
                                            definition,
                                            previous_overrides,
                                            side,
                                        );
                                        let next = crate::keybindings::effective_combo(
                                            definition,
                                            &next_overrides,
                                            side,
                                        );
                                        crate::keybindings::runtime_rebind_key_bindings(
                                            definition.id,
                                            previous.as_ref(),
                                            next.as_ref(),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            };
                            self.edit_settings(
                                move |settings| {
                                    settings.keybindings.overrides = next_overrides;
                                },
                                cx,
                            );
                            self.apply_runtime_key_bindings_to_window_handle(
                                runtime_bindings,
                                target_window,
                                cx,
                            );
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.keybindings.import_success"),
                                TerminalNoticeVariant::Success,
                                cx,
                            );
                        }
                        KeybindingFileOperationResult::ImportFailed => {
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.keybindings.import_invalid"),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            SettingsWorkspaceEvent::PortablePasswordChangeFinished { success } => {
                if *success {
                    self.push_ai_settings_toast(
                        self.i18n
                            .t("settings_view.general.portable_password_changed"),
                        TerminalNoticeVariant::Success,
                        cx,
                    );
                    self.refresh_portable_settings_snapshot(true, cx);
                } else if let Some(error) =
                    settings.read(cx).portable_action_error().map(str::to_owned)
                {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                }
            }
            SettingsWorkspaceEvent::CliCompanionFinished { operation, success } => {
                if *operation == CliCompanionOperation::Refresh {
                    return;
                }
                if *success {
                    let message_key = match operation {
                        CliCompanionOperation::Install => "settings_view.general.cli_installed",
                        CliCompanionOperation::Uninstall => "settings_view.general.cli_uninstalled",
                        CliCompanionOperation::UninstallLegacy => {
                            "migration.cli_legacy_uninstalled"
                        }
                        CliCompanionOperation::Migrate => "migration.cli_migrated",
                        CliCompanionOperation::Refresh => unreachable!("handled above"),
                    };
                    self.push_ai_settings_toast(
                        self.i18n.t(message_key),
                        TerminalNoticeVariant::Success,
                        cx,
                    );
                } else if let Some(error) =
                    settings.read(cx).cli_companion_error().map(str::to_owned)
                {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                }
            }
        }
    }

    pub(in crate::workspace) fn check_native_update(&mut self, cx: &mut Context<Self>) {
        self.check_native_update_with_kind(NativeUpdateCheckKind::Manual, cx);
    }

    pub(in crate::workspace) fn schedule_automatic_native_update_check(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.schedule_automatic_native_update_check(cx);
        });
    }

    fn check_native_update_with_kind(
        &mut self,
        check_kind: NativeUpdateCheckKind,
        cx: &mut Context<Self>,
    ) {
        let channel = self.settings_store.settings().general.update_channel;
        let install_flavor =
            oxideterm_update::NativeInstallContext::current(self.native_update_is_portable(cx))
                .map(|context| context.install_flavor)
                .map_err(|error| error.to_string());
        let request = NativeUpdateCheckRequest {
            kind: check_kind,
            channel,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            install_flavor,
            update_proxy: self.settings_store.settings().general.update_proxy.clone(),
            runtime: self.forwarding_runtime.clone(),
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_native_update_check(request, cx);
        });
    }

    pub(in crate::workspace) fn download_native_update(&mut self, cx: &mut Context<Self>) {
        let directory = self.native_update_download_directory();
        let update_proxy = self.settings_store.settings().general.update_proxy.clone();
        let runtime = self.forwarding_runtime.clone();
        let error_fallback = self.i18n.t("settings_view.help.update_error");
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_native_update_download(
                directory,
                update_proxy,
                runtime,
                error_fallback,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn install_native_update(&mut self, cx: &mut Context<Self>) {
        let context =
            oxideterm_update::NativeInstallContext::current(self.native_update_is_portable(cx))
                .map_err(|error| error.to_string());
        let cleanup_directory = self.native_update_download_directory();
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_native_update_install(context, cleanup_directory, runtime, cx);
        });
    }

    pub(in crate::workspace) fn cancel_native_update(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.cancel_native_update(cx);
        });
    }

    pub(in crate::workspace) fn schedule_native_update_quit(&mut self, cx: &mut Context<Self>) {
        // Tauri's updater exits after platform installers that need the current
        // process out of the way. Delay one frame so the final toast/state can
        // render before GPUI begins app shutdown.
        cx.spawn(async move |_weak, cx| {
            Timer::after(std::time::Duration::from_millis(750)).await;
            cx.update(|cx| cx.quit());
        })
        .detach();
    }

    pub(in crate::workspace) fn native_update_download_directory(&self) -> std::path::PathBuf {
        self.settings_store
            .path()
            .parent()
            .map(|parent| parent.join("updates"))
            .unwrap_or_else(|| std::path::PathBuf::from("updates"))
    }

    pub(in crate::workspace) fn native_update_is_portable(&self, cx: &App) -> bool {
        // The portable runtime marker is the persisted source of truth. The
        // cached snapshot avoids repeating filesystem detection when available.
        self.settings_workspace
            .read(cx)
            .portable_mode()
            .unwrap_or_else(|| oxideterm_portable_runtime::is_portable_mode().unwrap_or(false))
    }
}

pub(in crate::workspace) fn native_update_progress_ratio(
    status: &oxideterm_update::ResumableUpdateStatus,
) -> Option<f32> {
    let total_bytes = status.total_bytes.filter(|total| *total > 0)?;
    Some((status.downloaded_bytes as f64 / total_bytes as f64).clamp(0.0, 1.0) as f32)
}

pub(in crate::workspace) fn native_update_progress_hint(
    status: &oxideterm_update::ResumableUpdateStatus,
) -> String {
    let downloaded = native_update_format_bytes(status.downloaded_bytes);
    match status.total_bytes {
        Some(total) if total > 0 => {
            let percent = (status.downloaded_bytes.saturating_mul(100) / total).min(100);
            format!(
                "{} / {} · {}%",
                downloaded,
                native_update_format_bytes(total),
                percent
            )
        }
        _ => downloaded,
    }
}

fn native_update_format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    for unit in ["KB", "MB", "GB"] {
        value /= 1024.0;
        if value < 1024.0 {
            return format!("{value:.1} {unit}");
        }
    }
    format!("{:.1} TB", value / 1024.0)
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext, TestAppContext};

    use super::*;

    fn update_status(
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> oxideterm_update::ResumableUpdateStatus {
        oxideterm_update::ResumableUpdateStatus {
            task_id: "update-test".to_string(),
            version: "2.0.0".to_string(),
            attempt: 1,
            downloaded_bytes,
            total_bytes,
            resumable: true,
            stage: oxideterm_update::NativeUpdateStage::Downloading,
            status: oxideterm_update::NativeUpdateStage::Downloading,
            error_code: None,
            error_message: None,
            timestamp: 0,
            retry_delay_ms: None,
            last_http_status: None,
            can_resume_after_restart: true,
        }
    }

    #[test]
    fn progress_hint_reports_bytes_without_internal_retry_details() {
        assert_eq!(
            native_update_progress_hint(&update_status(512, Some(1024))),
            "512 B / 1.0 KB · 50%"
        );
        assert_eq!(
            native_update_progress_hint(&update_status(512, None)),
            "512 B"
        );
    }

    #[gpui::test]
    fn native_update_delivery_changes_entity_state_without_workspace_polling(
        cx: &mut TestAppContext,
    ) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        let sender = settings.update(cx, |settings, _cx| {
            let (sender, receiver) = delivery::ActiveDeliverySender::channel();
            settings.native_update.receiver = Some(receiver);
            settings.native_update.state = NativeUpdateUiState::Downloading(None);
            sender
        });
        sender
            .send(NativeUpdateDelivery::Finished(Err(
                "download failed".to_string()
            )))
            .expect("delivery");

        settings.update(cx, |settings, cx| {
            assert!(!settings.drain_native_update_delivery(cx));
            assert!(matches!(
                settings.native_update.state,
                NativeUpdateUiState::Error(ref error) if error == "download failed"
            ));
        });
    }
}
