use super::*;

impl HostToolsEntity {
    pub(super) fn request_service_snapshot_from_ui(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        let (Some(runtime), Some(messages)) =
            (self.lifecycle_runtime.clone(), self.messages.as_ref())
        else {
            return;
        };
        self.request_service_snapshot(
            connection_id,
            runtime,
            messages.service_connection_missing.clone(),
            messages.service_action_failed.clone(),
            cx,
        );
    }

    pub(super) fn request_service_action_from_ui(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        action: ServiceActionKind,
        cx: &mut Context<Self>,
    ) {
        if let Some(notice) = self.open_service_action_confirm(
            HostServiceActionRequest {
                connection_id,
                service_id,
                description,
                action,
            },
            cx,
        ) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(super) fn request_service_logs_from_ui(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        failure_fallback: String,
        empty_fallback: String,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ServiceConnectionMissing,
            ));
            return;
        };
        for notice in self.request_service_logs(
            connection_id,
            service_id,
            description,
            runtime,
            failure_fallback,
            empty_fallback,
            cx,
        ) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(super) fn dispatch_service_follow_logs_from_ui(
        &mut self,
        connection_id: String,
        service_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ServiceConnectionMissing,
            ));
            return;
        };
        let command = match build_service_follow_logs_command(&os_type, &service_id) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::ServiceLogsFailed,
                ));
                return;
            }
        };
        if command.capability == ServiceCommandCapability::Partial {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ServicePartialSupport { os_type },
            ));
        }
        // The command moves into a one-shot action so GPUI cloning never duplicates its contents.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command: command.command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }

    pub(in crate::health) fn service_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<ResourceServiceSnapshot> {
        (self.host_services.snapshot_connection_id.as_deref() == Some(connection_id))
            .then(|| self.host_services.snapshot.clone())
            .flatten()
    }

    pub(in crate::health) fn service_snapshot_in_flight(&self) -> bool {
        self.host_services.snapshot_in_flight
    }

    pub fn pause_service_refreshes(&mut self) {
        // A bounded capture already in flight may finish, while coalesced
        // page-only refresh work is discarded when Services is hidden.
        self.host_services.snapshot_pending = None;
    }

    pub(in crate::health) fn service_action_running_for(&self, service_id: &str) -> bool {
        self.host_services
            .action_running
            .as_ref()
            .is_some_and(|request| request.service_id == service_id)
    }

    pub(in crate::health) fn service_action_supported(
        &self,
        connection_id: &str,
        service_id: &str,
        action: ServiceActionKind,
    ) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_service_action_command(&os_type, service_id, action).ok())
            .is_some()
    }

    pub(in crate::health) fn service_logs_supported(
        &self,
        connection_id: &str,
        service_id: &str,
    ) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_service_logs_command(&os_type, service_id).ok())
            .is_some()
    }

    pub(in crate::health) fn service_follow_logs_command(
        &self,
        connection_id: &str,
        service_id: &str,
    ) -> Result<oxideterm_connection_monitor::ServiceCaptureCommand, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_service_follow_logs_command(&os_type, service_id)
    }

    pub(crate) fn request_service_snapshot(
        &mut self,
        connection_id: String,
        runtime: HostToolsRuntime,
        connection_fallback: String,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) {
        let request = HostServiceSnapshotRequest {
            connection_id,
            connection_fallback,
            failure_fallback,
        };
        if self.host_services.snapshot_in_flight {
            // Keep only the newest selection or post-action refresh while the
            // current bounded snapshot command completes.
            self.host_services.snapshot_pending =
                Some(HostServiceSnapshotPending { request, runtime });
            return;
        }
        self.start_service_snapshot(request, runtime, cx);
    }

    fn start_service_snapshot(
        &mut self,
        request: HostServiceSnapshotRequest,
        runtime: HostToolsRuntime,
        cx: &mut Context<Self>,
    ) {
        let Some(os_type) = self.connection_os_type(&request.connection_id) else {
            self.host_services.snapshot_connection_id = Some(request.connection_id);
            self.host_services.snapshot = Some(ResourceServiceSnapshot {
                status: ResourceServiceStatus::Error {
                    message: request.connection_fallback,
                },
                services: Vec::new(),
            });
            cx.notify();
            return;
        };
        let command = build_service_snapshot_command(&os_type);
        if self.host_services.snapshot_connection_id.as_deref()
            != Some(request.connection_id.as_str())
        {
            self.host_services.snapshot = None;
        }
        self.host_services.snapshot_connection_id = Some(request.connection_id.clone());
        self.host_services.snapshot_running = Some(request.clone());
        self.host_services.snapshot_in_flight = true;
        let spawned = self.spawn_service_snapshot_capture(
            command.command,
            request.clone(),
            HOST_SERVICE_SNAPSHOT_TIMEOUT,
            HOST_SERVICE_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_services.snapshot_running = None;
            self.host_services.snapshot_in_flight = false;
            self.host_services.snapshot = Some(ResourceServiceSnapshot {
                status: ResourceServiceStatus::Error {
                    message: request.connection_fallback,
                },
                services: Vec::new(),
            });
        }
        cx.notify();
    }

    pub(crate) fn finish_host_service_snapshot(
        &mut self,
        delivery: HostServiceSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_services.snapshot_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_services.snapshot_in_flight = false;
        self.host_services.snapshot_running = None;
        let snapshot = match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_service_snapshot(&output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stderr);
                snapshot
            }
            Ok(mut output) => {
                // Snapshot failures use a localized safe fallback, never the
                // captured remote output.
                zeroize::Zeroize::zeroize(&mut output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stderr);
                ResourceServiceSnapshot {
                    status: ResourceServiceStatus::Error {
                        message: delivery.request.failure_fallback.clone(),
                    },
                    services: Vec::new(),
                }
            }
            Err(()) => ResourceServiceSnapshot {
                status: ResourceServiceStatus::Error {
                    message: delivery.request.failure_fallback.clone(),
                },
                services: Vec::new(),
            },
        };
        self.host_services.snapshot_connection_id = Some(delivery.request.connection_id);
        self.host_services.snapshot = Some(snapshot);
        let pending = self.host_services.snapshot_pending.take();
        if let Some(pending) = pending {
            self.start_service_snapshot(pending.request, pending.runtime, cx);
        }
        cx.notify();
    }

    pub(crate) fn open_service_action_confirm(
        &mut self,
        request: HostServiceActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_services.action_running.is_some() {
            return Some(HostToolsNotice::ServiceActionAlreadyRunning);
        }
        HostToolConfirmState::open(&mut self.host_services.pending_confirm, request);
        cx.notify();
        None
    }

    pub fn service_confirm_is_open(&self) -> bool {
        self.service_confirm_view().is_some()
    }

    pub(crate) fn service_confirm_view(
        &self,
    ) -> Option<(
        HostServiceActionRequest,
        oxideterm_gpui_ui::motion::ExitPhase,
    )> {
        self.host_services
            .pending_confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    /// Dismisses unsubmitted UI state without cancelling a running service action.
    pub(crate) fn dismiss_service_confirm(&mut self, cx: &mut Context<Self>) {
        if self.host_services.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub fn begin_service_confirm_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self
            .host_services
            .pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_services.pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_services
                    .pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_services.pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub fn confirm_service_action(
        &mut self,
        delay: Duration,
        runtime: HostToolsRuntime,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(request) = self
            .host_services
            .pending_confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return Vec::new();
        };
        if !self.begin_service_confirm_exit(delay, cx) {
            return Vec::new();
        }
        self.start_service_action(request, runtime, cx)
    }

    pub(crate) fn start_service_action(
        &mut self,
        request: HostServiceActionRequest,
        runtime: HostToolsRuntime,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(os_type) = self.connection_os_type(&request.connection_id) else {
            return vec![HostToolsNotice::ServiceConnectionMissing];
        };
        let command = match build_service_action_command(
            &os_type,
            &request.service_id,
            request.action.clone(),
        ) {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::ServiceActionFailed],
        };
        let partial_support = command.capability == ServiceCommandCapability::Partial;
        self.host_services.action_running = Some(request.clone());
        let spawned = self.spawn_service_action(
            command.command,
            request,
            HOST_SERVICE_ACTION_TIMEOUT,
            HOST_SERVICE_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_services.action_running = None;
            return vec![HostToolsNotice::ServiceConnectionMissing];
        }
        cx.notify();
        if partial_support {
            vec![HostToolsNotice::ServicePartialSupport { os_type }]
        } else {
            Vec::new()
        }
    }

    pub(crate) fn finish_host_service_action(
        &mut self,
        delivery: HostServiceActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_services.action_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_services.action_running = None;
        let HostServiceActionRequest {
            connection_id,
            description,
            ..
        } = delivery.request;
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::ServiceActionFinished {
                description,
                succeeded: delivery.result.unwrap_or(false),
            },
        ));
        self.refresh_service_snapshot_after_action(connection_id, cx);
        cx.notify();
    }

    fn refresh_service_snapshot_after_action(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.monitoring.services_enabled
            || !self.visibility.sidebar_is_visible()
            || self.active_tool() != ContextSidebarTool::Services
        {
            return;
        }
        let (Some(runtime), Some(messages)) =
            (self.lifecycle_runtime.clone(), self.messages.as_ref())
        else {
            return;
        };
        let connection_fallback = messages.service_connection_missing.clone();
        let failure_fallback = messages.service_action_failed.clone();
        self.request_service_snapshot(
            connection_id,
            runtime,
            connection_fallback,
            failure_fallback,
            cx,
        );
    }

    pub(in crate::health) fn request_service_logs(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        runtime: HostToolsRuntime,
        failure_fallback: String,
        empty_fallback: String,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self
            .host_services
            .logs_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.loading)
        {
            return vec![HostToolsNotice::ServiceLogsAlreadyRunning];
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return vec![HostToolsNotice::ServiceConnectionMissing];
        };
        let command = match build_service_logs_command(&os_type, &service_id) {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::ServiceLogsFailed],
        };
        let partial_support = command.capability == ServiceCommandCapability::Partial;
        let request = HostServiceLogsRequest {
            connection_id,
            service_id,
            description,
            failure_fallback,
            empty_fallback,
        };
        self.host_services.logs_dialog = Some(HostServiceLogsDialog {
            request: request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        let spawned = self.spawn_service_logs_capture(
            command.command,
            request,
            HOST_SERVICE_LOGS_TIMEOUT,
            HOST_SERVICE_LOGS_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_services.logs_dialog = None;
            return vec![HostToolsNotice::ServiceConnectionMissing];
        }
        cx.notify();
        if partial_support {
            vec![HostToolsNotice::ServicePartialSupport { os_type }]
        } else {
            Vec::new()
        }
    }

    pub fn service_logs_connection_id(&self) -> Option<&str> {
        self.host_services
            .logs_dialog
            .as_ref()
            .map(|dialog| dialog.request.connection_id.as_str())
    }

    pub(super) fn service_logs_dialog(&self) -> Option<HostServiceLogsDialog> {
        self.host_services.logs_dialog.clone()
    }

    pub fn dismiss_service_logs_dialog(&mut self, cx: &mut Context<Self>) {
        if self.host_services.logs_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn finish_host_service_logs(
        &mut self,
        delivery: HostServiceLogsDelivery,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .host_services
            .logs_dialog
            .as_mut()
            .filter(|dialog| dialog.request == delivery.request)
        else {
            return;
        };
        dialog.loading = false;
        match delivery.result {
            Ok(mut output) if service_action_succeeded(output.exit_code) => {
                zeroize::Zeroize::zeroize(&mut output.stderr);
                let retained_output = if output.stdout.trim().is_empty() {
                    delivery.request.empty_fallback
                } else {
                    std::mem::take(&mut output.stdout)
                };
                // The last Entity/render owner clears the retained log buffer.
                dialog.output = Some(Arc::new(zeroize::Zeroizing::new(retained_output)));
                dialog.error = None;
            }
            Ok(mut output) => {
                // Failed output is never rendered or copied into an error.
                zeroize::Zeroize::zeroize(&mut output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stderr);
                dialog.output = None;
                dialog.error = Some(delivery.request.failure_fallback);
            }
            Err(()) => {
                dialog.output = None;
                dialog.error = Some(delivery.request.failure_fallback);
            }
        }
        cx.notify();
    }
}
