use super::*;

impl HostToolsEntity {
    pub(super) fn sync_host_tmux_list_state(
        &self,
        rows: &[ResourceTmuxSession],
        snapshot: Option<&ResourceTmuxSnapshot>,
        selected_id: &str,
    ) {
        let ui = &self.ui;
        let signatures = rows
            .iter()
            .map(|session| {
                let expanded =
                    ui.host_tmux_expanded_session_id.as_deref() == Some(session.id.as_str());
                let child_count = if expanded {
                    let window_count = snapshot
                        .map(|snapshot| snapshot.windows_for_session(&session.id).len())
                        .unwrap_or_default();
                    let pane_count = ui
                        .host_tmux_expanded_window_id
                        .as_deref()
                        .and_then(|window_id| {
                            snapshot.map(|snapshot| snapshot.panes_for_window(window_id).len())
                        })
                        .unwrap_or_default();
                    window_count + pane_count
                } else {
                    0
                };
                tmux_session_row_signature(session, expanded, child_count)
            })
            .collect::<Vec<_>>();
        let identity = format!(
            "host-tmux:{selected_id}:{}:{}:{}",
            ui.host_tmux_search_query,
            ui.host_tmux_expanded_session_id
                .as_deref()
                .unwrap_or_default(),
            ui.host_tmux_expanded_window_id
                .as_deref()
                .unwrap_or_default()
        );
        sync_tauri_variable_list_state_by_signatures(
            &ui.host_tmux_list_state,
            &mut ui.host_tmux_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn dispatch_tmux_attach_terminal(
        &self,
        connection_id: String,
        session_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self.tmux_attach_command(&connection_id, &session_id) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::TmuxActionFailed,
                ));
                return;
            }
        };
        // The builder is the only source of terminal commands for this action.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }

    pub(super) fn dispatch_tmux_new_session_terminal(
        &self,
        connection_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self.tmux_new_session_command(&connection_id) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::TmuxActionFailed,
                ));
                return;
            }
        };
        // The builder is the only source of terminal commands for this action.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }

    pub(super) fn open_tmux_confirm_from_view(
        &mut self,
        request: HostTmuxActionRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(notice) = self.open_tmux_action_confirm(request, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
            return;
        }
        // The root owns only the shared focus adapter, not the confirm state.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::PrepareTmuxConfirm,
            )),
            cx,
        );
    }

    pub(super) fn open_tmux_input_from_view(
        &mut self,
        dialog: HostTmuxInputDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The zeroizing dialog remains the sole owner of its secret-capable input.
        self.open_tmux_input_dialog(dialog, cx);
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::PrepareTmuxInputDialog,
            )),
            cx,
        );
    }

    pub(in crate::health) fn tmux_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<ResourceTmuxSnapshot> {
        (self.host_tmux.snapshot_connection_id.as_deref() == Some(connection_id))
            .then(|| self.host_tmux.snapshot.clone())
            .flatten()
    }

    pub(in crate::health) fn tmux_snapshot_in_flight(&self) -> bool {
        self.host_tmux.snapshot_in_flight
    }

    pub(in crate::health) fn tmux_action_running_for(&self, session_id: &str) -> bool {
        self.host_tmux
            .action_running
            .as_ref()
            .is_some_and(|request| request.session_id == session_id)
    }

    pub fn tmux_action_running(&self) -> bool {
        self.host_tmux.action_running.is_some()
    }

    pub(in crate::health) fn tmux_attach_command(
        &self,
        connection_id: &str,
        target: &str,
    ) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_attach_command(&os_type, target)
    }

    pub(in crate::health) fn tmux_new_session_command(
        &self,
        connection_id: &str,
    ) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_new_session_command(&os_type, None)
    }

    pub(crate) fn request_tmux_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        search_query: String,
        failure_fallback: String,
        unavailable_fallback: String,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self.host_tmux.snapshot_in_flight {
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxSnapshotAlreadyRunning]
            } else {
                Vec::new()
            };
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxConnectionMissing]
            } else {
                Vec::new()
            };
        };
        let command = build_tmux_snapshot_command(&os_type);
        let request = HostTmuxSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            search_query,
            failure_fallback,
            unavailable_fallback,
        };
        self.host_tmux.snapshot_connection_id = Some(connection_id);
        self.host_tmux.snapshot_running = Some(request.clone());
        self.host_tmux.snapshot_in_flight = true;
        self.host_tmux.last_error = None;
        let spawned = self.spawn_tmux_snapshot_capture(
            command.command,
            request,
            HOST_TMUX_SNAPSHOT_TIMEOUT,
            HOST_TMUX_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_tmux.snapshot_running = None;
            self.host_tmux.snapshot_in_flight = false;
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxConnectionMissing]
            } else {
                Vec::new()
            };
        }
        cx.notify();
        Vec::new()
    }

    pub(crate) fn finish_host_tmux_snapshot(
        &mut self,
        delivery: HostTmuxSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.snapshot_running.as_ref() != Some(&delivery.request) {
            return;
        }
        let feedback = delivery.request.feedback;
        self.host_tmux.snapshot_in_flight = false;
        self.host_tmux.snapshot_running = None;
        let (snapshot, notice) = match delivery.result {
            Ok(mut output) => {
                // The error-marker payload quotes raw host output, which can
                // carry credentials; only the request's fallback copy may
                // reach renderable state.
                let mut snapshot =
                    tmux_capture_snapshot(&output.stdout, &output.stderr, output.exit_code);
                zeroize::Zeroize::zeroize(&mut output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stderr);
                let notice = match snapshot.status.clone() {
                    ResourceTmuxStatus::Available { .. } => {
                        self.host_tmux.last_error = None;
                        Some(HostToolsNotice::TmuxSnapshotLoaded {
                            count: visible_tmux_session_rows(
                                &snapshot,
                                &delivery.request.search_query,
                            )
                            .len(),
                        })
                    }
                    ResourceTmuxStatus::Unavailable => {
                        self.host_tmux.last_error =
                            Some(delivery.request.unavailable_fallback.clone());
                        Some(HostToolsNotice::TmuxUnavailable)
                    }
                    ResourceTmuxStatus::Error { .. } => {
                        snapshot.status = ResourceTmuxStatus::Error {
                            message: delivery.request.failure_fallback.clone(),
                        };
                        self.host_tmux.last_error = Some(delivery.request.failure_fallback.clone());
                        Some(HostToolsNotice::TmuxSnapshotFailed)
                    }
                    ResourceTmuxStatus::Unknown => None,
                };
                (snapshot, notice)
            }
            Err(()) => {
                self.host_tmux.last_error = Some(delivery.request.failure_fallback.clone());
                (
                    ResourceTmuxSnapshot {
                        status: ResourceTmuxStatus::Error {
                            message: delivery.request.failure_fallback.clone(),
                        },
                        sessions: Vec::new(),
                        windows: Vec::new(),
                        panes: Vec::new(),
                    },
                    Some(HostToolsNotice::TmuxSnapshotFailed),
                )
            }
        };
        self.host_tmux.snapshot_connection_id = Some(delivery.request.connection_id);
        self.host_tmux.snapshot = Some(snapshot);
        if feedback.should_toast()
            && let Some(notice) = notice
        {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
        cx.notify();
    }

    pub(crate) fn open_tmux_action_confirm(
        &mut self,
        request: HostTmuxActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_tmux.action_running.is_some() {
            return Some(HostToolsNotice::TmuxActionAlreadyRunning);
        }
        HostToolConfirmState::open(&mut self.host_tmux.pending_confirm, request);
        cx.notify();
        None
    }

    pub fn tmux_confirm_is_open(&self) -> bool {
        self.tmux_confirm_view().is_some()
    }

    pub(crate) fn tmux_confirm_view(
        &self,
    ) -> Option<(HostTmuxActionRequest, oxideterm_gpui_ui::motion::ExitPhase)> {
        self.host_tmux
            .pending_confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    pub(crate) fn dismiss_tmux_confirm(&mut self, cx: &mut Context<Self>) {
        if self.host_tmux.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub fn begin_tmux_confirm_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self
            .host_tmux
            .pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_tmux.pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_tmux
                    .pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_tmux.pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(in crate::health) fn confirm_tmux_action(
        &mut self,
        delay: Duration,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(request) = self
            .host_tmux
            .pending_confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return Vec::new();
        };
        if !self.begin_tmux_confirm_exit(delay, cx) {
            return Vec::new();
        }
        self.start_tmux_action(request, runtime, cx)
    }

    pub fn confirm_tmux_action_from_view(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::TmuxConnectionMissing,
            ));
            return;
        };
        for notice in self.confirm_tmux_action(delay, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(crate) fn start_tmux_action(
        &mut self,
        request: HostTmuxActionRequest,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let HostTmuxActionRequest {
            connection_id,
            session_id,
            session_name,
            target_label,
            action,
        } = request;
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return vec![HostToolsNotice::TmuxConnectionMissing];
        };
        let action = match action {
            HostTmuxDestructiveAction::KillSession { target } => {
                TmuxActionKind::KillSession { target }
            }
            HostTmuxDestructiveAction::KillWindow { target } => {
                TmuxActionKind::KillWindow { target }
            }
            HostTmuxDestructiveAction::KillPane { target } => TmuxActionKind::KillPane { target },
        };
        let command = match build_tmux_action_command(&os_type, action) {
            Ok(command) => zeroize::Zeroizing::new(command.command),
            Err(_) => return vec![HostToolsNotice::TmuxActionFailed],
        };
        let request = HostTmuxActionRun {
            connection_id,
            session_id,
            session_name,
            target_label,
        };
        self.start_tmux_action_command(command, request, runtime, cx)
    }

    pub(crate) fn start_tmux_action_command(
        &mut self,
        command: zeroize::Zeroizing<String>,
        request: HostTmuxActionRun,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        self.host_tmux.action_running = Some(request.clone());
        let spawned = self.spawn_tmux_action(
            command,
            request,
            HOST_TMUX_ACTION_TIMEOUT,
            HOST_TMUX_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_tmux.action_running = None;
            return vec![HostToolsNotice::TmuxConnectionMissing];
        }
        cx.notify();
        Vec::new()
    }

    pub(crate) fn finish_host_tmux_action(
        &mut self,
        delivery: HostTmuxActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.action_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_tmux.action_running = None;
        let HostTmuxActionRun {
            connection_id,
            target_label,
            ..
        } = delivery.request;
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::TmuxActionFinished {
                target_label,
                succeeded: delivery.result.unwrap_or(false),
            },
        ));
        self.refresh_tmux_snapshot_after_action(connection_id, cx);
        cx.notify();
    }

    fn refresh_tmux_snapshot_after_action(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.monitoring.tmux_enabled
            || !self.visibility.sidebar_is_visible()
            || self.active_tool() != ContextSidebarTool::Tmux
        {
            return;
        }
        let (Some(runtime), Some(messages)) =
            (self.lifecycle_runtime.clone(), self.messages.as_ref())
        else {
            return;
        };
        let search_query = self.ui.host_tmux_search_query.clone();
        let failure_fallback = messages.tmux_unknown_error.clone();
        let unavailable_fallback = messages.tmux_unavailable.clone();
        let notices = self.request_tmux_snapshot(
            connection_id,
            HostSnapshotFeedback::Silent,
            search_query,
            failure_fallback,
            unavailable_fallback,
            runtime,
            cx,
        );
        debug_assert!(notices.is_empty());
    }

    pub(crate) fn open_tmux_input_dialog(
        &mut self,
        dialog: HostTmuxInputDialog,
        cx: &mut Context<Self>,
    ) {
        self.ui.host_tmux_input_dialog = Some(dialog);
        self.ui.focus_input(HostToolsTextInput::TmuxDialog);
        cx.notify();
    }

    pub fn dismiss_tmux_input_dialog(&mut self, cx: &mut Context<Self>) {
        if self.ui.host_tmux_input_dialog.take().is_some() {
            if self.ui.input_is_focused(HostToolsTextInput::TmuxDialog) {
                self.ui.clear_input_focus();
            }
            cx.notify();
        }
    }

    pub(crate) fn submit_tmux_input(
        &mut self,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self.host_tmux.action_running.is_some() {
            return vec![HostToolsNotice::TmuxActionAlreadyRunning];
        }
        let Some(dialog) = self.ui.host_tmux_input_dialog.as_ref() else {
            return Vec::new();
        };
        if dialog.value.trim().is_empty() {
            return vec![HostToolsNotice::TmuxInputRequired];
        }
        let mut dialog = self
            .ui
            .host_tmux_input_dialog
            .take()
            .expect("tmux input dialog remains present after validation");
        self.ui.clear_input_focus();
        let trimmed_start = dialog.value.len() - dialog.value.trim_start().len();
        let trimmed_end = dialog.value.trim_end().len();
        dialog.value.truncate(trimmed_end);
        if trimmed_start > 0 {
            dialog.value.drain(..trimmed_start);
        }
        let Some(os_type) = self.connection_os_type(&dialog.connection_id) else {
            return vec![HostToolsNotice::TmuxConnectionMissing];
        };
        let command = match &dialog.kind {
            HostTmuxInputDialogKind::RenameSession { target } => {
                build_tmux_rename_session_command(&os_type, target, dialog.value.as_str())
            }
            HostTmuxInputDialogKind::RenameWindow { target } => {
                build_tmux_rename_window_command(&os_type, target, dialog.value.as_str())
            }
            HostTmuxInputDialogKind::SendPaneCommand { target } => {
                build_tmux_send_pane_command(&os_type, target, dialog.value.as_str())
            }
        };
        // The original input clears here; the generated shell command has its
        // own zeroizing buffer until the SSH worker finishes.
        zeroize::Zeroize::zeroize(&mut dialog.value);
        let command = match command {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::TmuxActionFailed],
        };
        let request = HostTmuxActionRun {
            connection_id: dialog.connection_id,
            session_id: dialog.session_id,
            session_name: dialog.session_name,
            target_label: dialog.target_label,
        };
        self.start_tmux_action_command(command, request, runtime, cx)
    }

    pub fn submit_tmux_input_from_view(&mut self, cx: &mut Context<Self>) {
        if self.tmux_action_running() {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::TmuxActionAlreadyRunning,
            ));
            return;
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::TmuxConnectionMissing,
            ));
            return;
        };
        for notice in self.submit_tmux_input(runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }
}
