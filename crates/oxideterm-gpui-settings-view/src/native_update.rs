use crate::entity::{SettingsWorkspaceEntity, SettingsWorkspaceEvent, SettingsWorkspaceToast};
use gpui::{Context, Task};
use oxideterm_settings::{GeneralSettings, UpdateChannel};
use oxideterm_workspace::delivery;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const AUTOMATIC_NATIVE_UPDATE_DELAY: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeUpdateCheckKind {
    Manual,
    Automatic,
}

#[derive(Debug)]
pub enum NativeUpdateUiState {
    Idle,
    Checking,
    UpToDate,
    ManagedByNix,
    Available(oxideterm_update::NativeUpdatePackage),
    Downloading(Option<oxideterm_update::ResumableUpdateStatus>),
    Verifying(Option<oxideterm_update::ResumableUpdateStatus>),
    Downloaded(oxideterm_update::NativeUpdateDownload),
    Installing(Option<oxideterm_update::NativeInstallPlan>),
    InstallFinished(oxideterm_update::NativeInstallOutcome),
    Error(String),
}

#[derive(Debug)]
pub enum NativeUpdateRenderState {
    Idle,
    Checking,
    UpToDate,
    ManagedByNix,
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

pub struct NativeUpdateReleaseNotes {
    pub body: String,
    pub description: Option<String>,
}

#[derive(Debug)]
pub enum NativeUpdateDelivery {
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

#[derive(Clone, Copy)]
pub enum NativeUpdateAction {
    Check(NativeUpdateCheckKind),
    Download,
    Install,
    Cancel,
}

pub struct NativeUpdateEnvironment<'a> {
    pub general: &'a GeneralSettings,
    pub settings_path: &'a Path,
    pub current_version: &'a str,
    pub runtime: Arc<tokio::runtime::Runtime>,
}

pub struct NativeUpdateCheckRequest {
    pub kind: NativeUpdateCheckKind,
    pub channel: UpdateChannel,
    pub current_version: String,
    pub install_flavor: Result<oxideterm_update::InstallFlavor, String>,
    pub update_proxy: oxideterm_settings::UpdateProxySettings,
    pub runtime: Arc<tokio::runtime::Runtime>,
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
    pub fn native_update_is_portable(&self) -> bool {
        self.portable_mode()
            .unwrap_or_else(|| oxideterm_portable_runtime::is_portable_mode().unwrap_or(false))
    }

    pub fn apply_native_update_action(
        &mut self,
        action: NativeUpdateAction,
        environment: NativeUpdateEnvironment<'_>,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) {
        let install_context = || oxideterm_update::NativeInstallContext::current(self.native_update_is_portable())
            .map_err(|error| error.to_string());
        let download_directory = || environment.settings_path.parent()
            .map(|parent| parent.join("updates"))
            .unwrap_or_else(|| PathBuf::from("updates"));
        match action {
            NativeUpdateAction::Check(kind) => {
                let install_flavor = install_context().map(|context| context.install_flavor);
                self.start_native_update_check(NativeUpdateCheckRequest {
                    kind,
                    channel: environment.general.update_channel,
                    current_version: environment.current_version.to_owned(),
                    install_flavor,
                    update_proxy: environment.general.update_proxy.clone(),
                    runtime: environment.runtime,
                }, cx);
            }
            NativeUpdateAction::Download => {
                self.start_native_update_download(
                    download_directory(),
                    environment.general.update_proxy.clone(),
                    environment.runtime,
                    i18n.t("settings_view.help.update_error"),
                    cx,
                );
            }
            NativeUpdateAction::Install => {
                let context = install_context();
                self.start_native_update_install(context, download_directory(), environment.runtime, cx);
            }
            NativeUpdateAction::Cancel => {
                self.cancel_native_update(cx);
            }
        }
    }

    pub fn native_update_render_state(&self) -> NativeUpdateRenderState {
        match &self.native_update.state {
            NativeUpdateUiState::Idle => NativeUpdateRenderState::Idle,
            NativeUpdateUiState::Checking => NativeUpdateRenderState::Checking,
            NativeUpdateUiState::UpToDate => NativeUpdateRenderState::UpToDate,
            NativeUpdateUiState::ManagedByNix => NativeUpdateRenderState::ManagedByNix,
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

    pub fn native_update_package_description(&self) -> Option<String> {
        self.native_update
            .package
            .as_ref()
            .map(|package| format!("v{} → v{}", package.current_version, package.version))
    }

    pub fn native_update_has_release_notes(&self) -> bool {
        self.native_update
            .package
            .as_ref()
            .and_then(|package| package.body.as_deref())
            .is_some_and(|body| !body.trim().is_empty())
    }

    pub fn native_update_release_notes(&self) -> Option<NativeUpdateReleaseNotes> {
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

    pub fn native_update_message(&self) -> Option<&str> {
        match &self.native_update.state {
            NativeUpdateUiState::InstallFinished(outcome) => Some(&outcome.message),
            NativeUpdateUiState::Error(error) => Some(error),
            _ => None,
        }
    }

    pub fn schedule_automatic_native_update_check(&mut self, cx: &mut Context<Self>) {
        if self.native_update.automatic_check_task.is_some() {
            return;
        }

        // Delay manifest traffic until session restoration and the first frame settle.
        self.native_update.automatic_check_task = Some(cx.spawn(async move |settings, cx| {
            cx.background_executor()
                .timer(AUTOMATIC_NATIVE_UPDATE_DELAY)
                .await;
            let _ = settings.update(cx, |settings, cx| {
                settings.native_update.automatic_check_task = None;
                if matches!(settings.native_update.state, NativeUpdateUiState::Idle) {
                    cx.emit(SettingsWorkspaceEvent::RequestAutomaticNativeUpdateCheck);
                }
            });
        }));
    }

    pub fn start_native_update_check(
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
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::System,
            "update_check",
            None,
            Some(if request.kind == NativeUpdateCheckKind::Automatic {
                "automatic"
            } else {
                "manual"
            }),
        );

        let install_flavor = match request.install_flavor {
            Ok(install_flavor) => install_flavor,
            Err(error) => {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
                self.native_update.state = if request.kind == NativeUpdateCheckKind::Automatic {
                    NativeUpdateUiState::Idle
                } else {
                    NativeUpdateUiState::Error(error)
                };
                cx.notify();
                return true;
            }
        };

        if install_flavor == oxideterm_update::InstallFlavor::LinuxNix {
            self.native_update.state = if request.kind == NativeUpdateCheckKind::Automatic {
                NativeUpdateUiState::Idle
            } else {
                NativeUpdateUiState::ManagedByNix
            };
            cx.notify();
            return true;
        }

        self.native_update.check_task = Some(cx.spawn(async move |settings, cx| {
            let result = request
                .runtime
                .spawn(async move {
                    let result = async {
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
                    }
                    .await;
                    audit.finish(
                        if result.is_ok() {
                            oxideterm_audit::AuditOutcome::Succeeded
                        } else {
                            oxideterm_audit::AuditOutcome::Failed
                        },
                        oxideterm_audit::AuditEvidence::Protocol,
                        None,
                        None,
                    );
                    result
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
                    Ok(oxideterm_update::NativeUpdateStatus::ManagedByNix)
                        if request.kind == NativeUpdateCheckKind::Automatic =>
                    {
                        NativeUpdateUiState::Idle
                    }
                    Ok(oxideterm_update::NativeUpdateStatus::ManagedByNix) => {
                        NativeUpdateUiState::ManagedByNix
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

    pub fn start_native_update_download(
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
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::System,
            "update_download",
            None,
            None,
        );

        self.native_update.operation_task = Some(cx.spawn(async move |_settings, _cx| {
            let _ = runtime
                .spawn(async move {
                    let cancelled = cancel.clone();
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
                    audit.finish(
                        match &result {
                            Ok(_) => oxideterm_audit::AuditOutcome::Succeeded,
                            Err(_) if cancelled.load(Ordering::Relaxed) => {
                                oxideterm_audit::AuditOutcome::Cancelled
                            }
                            Err(_) => oxideterm_audit::AuditOutcome::Failed,
                        },
                        oxideterm_audit::AuditEvidence::Protocol,
                        None,
                        None,
                    );
                    let _ = sender.send(NativeUpdateDelivery::Finished(result));
                })
                .await;
        }));
        cx.notify();
        true
    }

    pub fn start_native_update_install(
        &mut self,
        context: Result<oxideterm_update::NativeInstallContext, String>,
        cleanup_directory: std::path::PathBuf,
        runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.native_update.receiver.is_some() {
            return false;
        }
        let audit = oxideterm_audit::AuditOperation::begin(
            oxideterm_audit::AuditCategory::System,
            "update_install",
            None,
            None,
        );
        let context = match context {
            Ok(context) => context,
            Err(error) => {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Protocol,
                    None,
                    None,
                );
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
                    audit.finish(
                        oxideterm_audit::AuditOutcome::Unchanged,
                        oxideterm_audit::AuditEvidence::Protocol,
                        None,
                        None,
                    );
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
                    audit.finish(
                        match &result {
                            Ok(outcome) if outcome.status == oxideterm_update::NativeInstallStatus::ManualActionRequired => oxideterm_audit::AuditOutcome::Sent,
                            Ok(_) => oxideterm_audit::AuditOutcome::Succeeded,
                            Err(_) => oxideterm_audit::AuditOutcome::Failed,
                        },
                        oxideterm_audit::AuditEvidence::Protocol,
                        None,
                        None,
                    );
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

    pub fn cancel_native_update(&mut self, cx: &mut Context<Self>) {
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

pub fn native_update_progress_ratio(
    status: &oxideterm_update::ResumableUpdateStatus,
) -> Option<f32> {
    let total_bytes = status.total_bytes.filter(|total| *total > 0)?;
    Some((status.downloaded_bytes as f64 / total_bytes as f64).clamp(0.0, 1.0) as f32)
}

pub fn native_update_progress_hint(status: &oxideterm_update::ResumableUpdateStatus) -> String {
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
