use super::*;

const AUTOMATIC_NATIVE_UPDATE_DELAY: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeUpdateCheckKind {
    Manual,
    Automatic,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub(in crate::workspace) enum NativeUpdateDelivery {
    Progress(oxideterm_update::DownloadProgress),
    Finished(Result<oxideterm_update::NativeUpdateDownload, String>),
    InstallFinished(Result<oxideterm_update::NativeInstallOutcome, String>),
}

impl WorkspaceApp {
    pub(in crate::workspace) fn check_native_update(&mut self, cx: &mut Context<Self>) {
        self.check_native_update_with_kind(NativeUpdateCheckKind::Manual, cx);
    }

    pub(in crate::workspace) fn schedule_automatic_native_update_check(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.native_update_is_portable(cx) {
            return;
        }

        // Match the legacy startup delay so session restoration and the first
        // interactive frame are not competing with update manifest traffic.
        cx.spawn(async move |weak, cx| {
            Timer::after(AUTOMATIC_NATIVE_UPDATE_DELAY).await;
            let _ = weak.update(cx, |this, cx| {
                if matches!(this.native_update_state, NativeUpdateUiState::Idle) {
                    this.check_native_update_with_kind(NativeUpdateCheckKind::Automatic, cx);
                }
            });
        })
        .detach();
    }

    fn check_native_update_with_kind(
        &mut self,
        check_kind: NativeUpdateCheckKind,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.native_update_state,
            NativeUpdateUiState::Checking
                | NativeUpdateUiState::Downloading(_)
                | NativeUpdateUiState::Verifying(_)
                | NativeUpdateUiState::Installing(_)
        ) {
            return;
        }

        self.native_update_state = NativeUpdateUiState::Checking;
        self.native_update_package = None;
        self.native_update_notification_open = false;
        self.native_update_notification_presence.reopen();
        self.native_update_release_notes_open = false;
        self.native_update_release_notes_presence.reopen();
        // Release notes belong to the next update result; reset virtual scroll
        // so a previous changelog position cannot leak into the new package.
        self.native_update_release_notes_scroll = MarkdownVirtualListScrollHandle::new();
        let channel = self.settings_store.settings().general.update_channel;
        if channel == UpdateChannel::Stable && is_gpui_preview_version(env!("CARGO_PKG_VERSION")) {
            // Keep the persisted choice untouched because Tauri 1.x shares this
            // settings file. The update crate repeats this guard for non-UI callers.
            self.native_update_state = if check_kind == NativeUpdateCheckKind::Automatic {
                NativeUpdateUiState::Idle
            } else {
                NativeUpdateUiState::Error(
                    self.i18n
                        .t("settings_view.help.preview_stable_upgrade_hint"),
                )
            };
            cx.notify();
            return;
        }
        let update_proxy = self.settings_store.settings().general.update_proxy.clone();
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let install_flavor = match oxideterm_update::NativeInstallContext::current(
            self.native_update_is_portable(cx),
        ) {
            Ok(context) => context.install_flavor,
            Err(error) => {
                self.native_update_state = if check_kind == NativeUpdateCheckKind::Automatic {
                    NativeUpdateUiState::Idle
                } else {
                    NativeUpdateUiState::Error(error.to_string())
                };
                cx.notify();
                return;
            }
        };
        let runtime = self.forwarding_runtime.clone();

        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn(async move {
                    let client =
                        oxideterm_update::NativeUpdateClient::with_update_proxy(&update_proxy)?;
                    client
                        .check(oxideterm_update::NativeUpdateRequest::current(
                            channel,
                            current_version,
                            install_flavor,
                        ))
                        .await
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));

            let _ = weak.update(cx, |this, cx| {
                this.native_update_state = match result {
                    Ok(oxideterm_update::NativeUpdateStatus::UpToDate)
                        if check_kind == NativeUpdateCheckKind::Automatic =>
                    {
                        NativeUpdateUiState::Idle
                    }
                    Ok(oxideterm_update::NativeUpdateStatus::UpToDate) => {
                        NativeUpdateUiState::UpToDate
                    }
                    Ok(oxideterm_update::NativeUpdateStatus::Available(package)) => {
                        this.native_update_package = Some(package.clone());
                        this.show_native_update_notification();
                        NativeUpdateUiState::Available(package)
                    }
                    Err(_error) if check_kind == NativeUpdateCheckKind::Automatic => {
                        NativeUpdateUiState::Idle
                    }
                    Err(error) => NativeUpdateUiState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn download_native_update(&mut self, cx: &mut Context<Self>) {
        if self.native_update_rx.is_some() {
            return;
        }
        let package = match &self.native_update_state {
            NativeUpdateUiState::Available(package) => package.clone(),
            _ => return,
        };

        let (tx, rx) =
            delivery::ActiveDeliverySender::channel_with_wake(self.native_update_wake.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        self.native_update_rx = Some(rx);
        self.native_update_cancel = Some(cancel.clone());
        self.native_update_state = NativeUpdateUiState::Downloading(None);
        self.show_native_update_notification();

        let directory = self.native_update_download_directory();
        let runtime = self.forwarding_runtime.clone();
        let update_proxy = self.settings_store.settings().general.update_proxy.clone();

        cx.spawn(async move |_weak, _cx| {
            runtime.spawn(async move {
                let result = async {
                    let client =
                        oxideterm_update::NativeUpdateClient::with_update_proxy(&update_proxy)?;
                    // Match Tauri's resumable updater cache contract:
                    // package.part + state.json, Range resume, retry status,
                    // and minisign verification before the package is opened.
                    client
                        .download_resumable_package(package, &directory, cancel, |progress| {
                            let _ = tx.send(NativeUpdateDelivery::Progress(progress));
                        })
                        .await
                }
                .await
                .map_err(|error: oxideterm_update::NativeUpdateError| error.to_string());
                let _ = tx.send(NativeUpdateDelivery::Finished(result));
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn install_native_update(&mut self, cx: &mut Context<Self>) {
        let download = match &self.native_update_state {
            NativeUpdateUiState::Downloaded(download) => download.clone(),
            _ => return,
        };

        let context = match oxideterm_update::NativeInstallContext::current(
            self.native_update_is_portable(cx),
        ) {
            Ok(context) => context,
            Err(error) => {
                self.native_update_state = NativeUpdateUiState::Error(error.to_string());
                self.show_native_update_notification();
                cx.notify();
                return;
            }
        };
        let plan = oxideterm_update::plan_native_install(&download.path, &context);

        let (tx, rx) =
            delivery::ActiveDeliverySender::channel_with_wake(self.native_update_wake.clone());
        self.native_update_rx = Some(rx);
        self.native_update_cancel = None;
        self.native_update_state = NativeUpdateUiState::Installing(Some(plan.clone()));
        self.show_native_update_notification();

        let runtime = self.forwarding_runtime.clone();
        let cleanup_directory = self.native_update_download_directory();
        let cleanup_version = download.package.version;
        cx.spawn(async move |_weak, _cx| {
            runtime.spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    // Installation is intentionally delegated to the updater
                    // crate so GPUI keeps only UI-state orchestration here.
                    oxideterm_update::execute_install_plan(&plan)
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
                if result.is_ok() {
                    let _ = oxideterm_update::prune_resumable_update_cache(
                        &cleanup_directory,
                        Some(&cleanup_version),
                    )
                    .await;
                }
                let _ = tx.send(NativeUpdateDelivery::InstallFinished(result));
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn cancel_native_update(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.native_update_cancel.as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.native_update_state = self.native_update_available_state();
        self.native_update_cancel = None;
        cx.notify();
    }

    pub(in crate::workspace) fn schedule_native_update_delivery(&self, cx: &mut Context<Self>) {
        let update_wake = self.native_update_wake.clone();
        let release_wake = update_wake.clone();
        cx.on_release(move |_, _| {
            // Cancel state remains operation-owned; release only stops the UI waiter.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |weak, cx| {
            loop {
                update_wake.wait().await;
                let should_drain = update_wake.take();
                let stopped = update_wake.is_stopped();
                if !should_drain {
                    if stopped {
                        break;
                    }
                    continue;
                }
                let Ok(backlog_remaining) =
                    weak.update(cx, |this, cx| this.poll_native_update_delivery(cx))
                else {
                    break;
                };
                if backlog_remaining {
                    // Continue a bounded progress batch without a polling timer.
                    update_wake.mark();
                } else if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn poll_native_update_delivery(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(rx) = self.native_update_rx.as_ref() else {
            return false;
        };

        let delivery_batch = delivery::drain_channel(rx, delivery::NOTIFICATION_DELIVERY_BUDGET);

        for update in delivery_batch.items {
            self.handle_native_update_delivery(update, cx);
        }
        if delivery_batch.disconnected {
            self.native_update_rx = None;
            self.native_update_cancel = None;
        }
        cx.notify();
        delivery_batch.outcome.backlog_remaining
    }

    pub(in crate::workspace) fn handle_native_update_delivery(
        &mut self,
        delivery: NativeUpdateDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery {
            NativeUpdateDelivery::Progress(progress) => {
                let stage = progress.status.stage;
                self.native_update_state = match stage {
                    oxideterm_update::NativeUpdateStage::Downloading => {
                        NativeUpdateUiState::Downloading(Some(progress.status))
                    }
                    oxideterm_update::NativeUpdateStage::Verifying => {
                        NativeUpdateUiState::Verifying(Some(progress.status))
                    }
                    oxideterm_update::NativeUpdateStage::Ready => {
                        NativeUpdateUiState::Verifying(Some(progress.status))
                    }
                    oxideterm_update::NativeUpdateStage::Error => NativeUpdateUiState::Error(
                        progress
                            .status
                            .error_message
                            .unwrap_or_else(|| self.i18n.t("settings_view.help.update_error")),
                    ),
                    oxideterm_update::NativeUpdateStage::Cancelled => {
                        self.native_update_available_state()
                    }
                };
                if stage == oxideterm_update::NativeUpdateStage::Error {
                    self.show_native_update_notification();
                }
            }
            NativeUpdateDelivery::Finished(Ok(download)) => {
                self.native_update_package = Some(download.package.clone());
                self.native_update_state = NativeUpdateUiState::Downloaded(download);
                self.native_update_cancel = None;
                self.show_native_update_notification();
            }
            NativeUpdateDelivery::Finished(Err(error)) => {
                if error.contains("update cancelled") {
                    self.native_update_state = self.native_update_available_state();
                } else {
                    self.native_update_state = NativeUpdateUiState::Error(error.clone());
                    self.show_native_update_notification();
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                }
                self.native_update_cancel = None;
            }
            NativeUpdateDelivery::InstallFinished(Ok(outcome)) => {
                let is_success =
                    outcome.status != oxideterm_update::NativeInstallStatus::ManualActionRequired;
                let should_quit_app = outcome.should_quit_app;
                self.native_update_state = NativeUpdateUiState::InstallFinished(outcome.clone());
                self.native_update_rx = None;
                self.show_native_update_notification();
                let variant = if is_success {
                    TerminalNoticeVariant::Success
                } else {
                    TerminalNoticeVariant::Warning
                };
                self.push_ai_settings_toast(outcome.message, variant);
                if should_quit_app {
                    self.schedule_native_update_quit(cx);
                }
            }
            NativeUpdateDelivery::InstallFinished(Err(error)) => {
                self.native_update_state = NativeUpdateUiState::Error(error.clone());
                self.native_update_rx = None;
                self.show_native_update_notification();
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            }
        }
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

    fn native_update_available_state(&self) -> NativeUpdateUiState {
        self.native_update_package
            .clone()
            .map(NativeUpdateUiState::Available)
            .unwrap_or(NativeUpdateUiState::Idle)
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
    fn progress_ratio_requires_a_positive_total() {
        assert_eq!(native_update_progress_ratio(&update_status(10, None)), None);
        assert_eq!(
            native_update_progress_ratio(&update_status(10, Some(0))),
            None
        );
    }

    #[test]
    fn progress_ratio_is_clamped_to_the_complete_range() {
        assert_eq!(
            native_update_progress_ratio(&update_status(25, Some(100))),
            Some(0.25)
        );
        assert_eq!(
            native_update_progress_ratio(&update_status(125, Some(100))),
            Some(1.0)
        );
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
}
