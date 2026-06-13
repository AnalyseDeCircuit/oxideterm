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
    fn check_native_update(&mut self, cx: &mut Context<Self>) {
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
        let channel = self.settings_store.settings().general.update_channel;
        let update_proxy = self.settings_store.settings().general.update_proxy.clone();
        let current_version = env!("CARGO_PKG_VERSION").to_string();
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
                        ))
                        .await
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));

            let _ = weak.update(cx, |this, cx| {
                this.native_update_state = match result {
                    Ok(oxideterm_update::NativeUpdateStatus::UpToDate) => {
                        NativeUpdateUiState::UpToDate
                    }
                    Ok(oxideterm_update::NativeUpdateStatus::Available(package)) => {
                        NativeUpdateUiState::Available(package)
                    }
                    Err(error) => NativeUpdateUiState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn download_native_update(&mut self, cx: &mut Context<Self>) {
        let package = match &self.native_update_state {
            NativeUpdateUiState::Available(package) => package.clone(),
            _ => return,
        };

        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.native_update_rx = Some(rx);
        self.native_update_cancel = Some(cancel.clone());
        self.native_update_state = NativeUpdateUiState::Downloading(None);
        self.schedule_native_update_delivery_poll(cx);

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

    fn install_native_update(&mut self, cx: &mut Context<Self>) {
        let download = match &self.native_update_state {
            NativeUpdateUiState::Downloaded(download) => download.clone(),
            _ => return,
        };

        let is_portable = self
            .portable_status_snapshot
            .as_ref()
            .map(|status| status.is_portable)
            .unwrap_or_else(|| oxideterm_portable_runtime::is_portable_mode().unwrap_or(false));
        let context = match oxideterm_update::NativeInstallContext::current(is_portable) {
            Ok(context) => context,
            Err(error) => {
                self.native_update_state = NativeUpdateUiState::Error(error.to_string());
                cx.notify();
                return;
            }
        };
        let plan = oxideterm_update::plan_native_install(&download.path, &context);

        let (tx, rx) = std::sync::mpsc::channel();
        self.native_update_rx = Some(rx);
        self.native_update_cancel = None;
        self.native_update_state = NativeUpdateUiState::Installing(Some(plan.clone()));
        self.schedule_native_update_delivery_poll(cx);

        let runtime = self.forwarding_runtime.clone();
        let cleanup_directory = self.native_update_download_directory();
        let cleanup_version = download.package.version.clone();
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

    fn cancel_native_update(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.native_update_cancel.as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.native_update_state = NativeUpdateUiState::Idle;
        self.native_update_cancel = None;
        cx.notify();
    }

    fn schedule_native_update_delivery_poll(&mut self, cx: &mut Context<Self>) {
        if self.native_update_polling {
            return;
        }
        self.native_update_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(std::time::Duration::from_millis(100)).await;
                let keep_polling = weak
                    .update(cx, |this, cx| {
                        this.poll_native_update_delivery(cx);
                        this.native_update_polling
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_native_update_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.native_update_rx.as_ref() else {
            self.native_update_polling = false;
            return;
        };

        let mut deliveries = Vec::new();
        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(delivery) => deliveries.push(delivery),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        for delivery in deliveries {
            self.handle_native_update_delivery(delivery, cx);
        }
        if disconnected {
            self.native_update_rx = None;
            self.native_update_polling = false;
            self.native_update_cancel = None;
        }
        cx.notify();
    }

    fn handle_native_update_delivery(
        &mut self,
        delivery: NativeUpdateDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery {
            NativeUpdateDelivery::Progress(progress) => {
                self.native_update_state = match progress.status.stage {
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
                    oxideterm_update::NativeUpdateStage::Cancelled => NativeUpdateUiState::Idle,
                };
            }
            NativeUpdateDelivery::Finished(Ok(download)) => {
                self.native_update_state = NativeUpdateUiState::Downloaded(download);
                self.native_update_cancel = None;
            }
            NativeUpdateDelivery::Finished(Err(error)) => {
                if error.contains("update cancelled") {
                    self.native_update_state = NativeUpdateUiState::Idle;
                } else {
                    self.native_update_state = NativeUpdateUiState::Error(error.clone());
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
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            }
        }
    }

    fn schedule_native_update_quit(&mut self, cx: &mut Context<Self>) {
        // Tauri's updater exits after platform installers that need the current
        // process out of the way. Delay one frame so the final toast/state can
        // render before GPUI begins app shutdown.
        cx.spawn(async move |_weak, cx| {
            Timer::after(std::time::Duration::from_millis(750)).await;
            cx.update(|cx| cx.quit()).ok();
        })
        .detach();
    }

    fn native_update_download_directory(&self) -> std::path::PathBuf {
        self.settings_store
            .path()
            .parent()
            .map(|parent| parent.join("updates"))
            .unwrap_or_else(|| std::path::PathBuf::from("updates"))
    }
}
