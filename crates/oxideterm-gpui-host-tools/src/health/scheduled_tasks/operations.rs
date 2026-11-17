use super::*;

impl HostToolsEntity {
    pub fn confirm_schedule_action_from_view(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ScheduleConnectionMissing,
            ));
            return;
        };
        for notice in self.confirm_schedule_action(delay, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(super) fn schedule_logs_command(
        &self,
        connection_id: &str,
        task: &ResourceScheduledTask,
        follow: bool,
        limit: usize,
    ) -> Result<
        (
            oxideterm_connection_monitor::ScheduledTaskCaptureCommand,
            String,
        ),
        (),
    > {
        let Some(os_type) = self.connection_os_type(connection_id) else {
            return Err(());
        };
        build_scheduled_task_logs_command(&os_type, task, follow, limit)
            .map_err(|_| ())
            .map(|command| (command, os_type))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dispatch_schedule_follow_terminal(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (command, os_type) = match self.schedule_logs_command(
            &connection_id,
            &task,
            true,
            HOST_SCHEDULE_LOG_LINE_LIMIT,
        ) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::ScheduleLogsFailed,
                ));
                return;
            }
        };
        if command.capability == ScheduledTaskCapability::Partial {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::SchedulePartialSupport { os_type },
            ));
        }
        // The generated command moves into the one-shot window request and is never cloned.
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dispatch_schedule_diagnostic_terminal(
        &mut self,
        connection_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ScheduleConnectionMissing,
            ));
            return;
        };
        let command = build_scheduled_task_diagnostic_command(&os_type);
        // NodeRouter remains the physical-session owner; this intent adds one tab consumer.
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

    pub(in crate::health) fn schedule_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<ResourceScheduledTaskSnapshot> {
        self.host_schedules
            .snapshot
            .as_ref()
            .filter(|_| {
                self.host_schedules.snapshot_connection_id.as_deref() == Some(connection_id)
            })
            .cloned()
    }

    pub(in crate::health) fn schedule_snapshot_in_flight(&self) -> bool {
        self.host_schedules.snapshot_in_flight
    }

    pub(crate) fn schedule_filter(&self) -> ScheduledTaskFilter {
        self.host_schedules.filter
    }

    /// Source chips follow the host: a Windows snapshot offers no cron chip.
    pub(super) fn schedule_filter_options(&self) -> Vec<ScheduledTaskFilter> {
        self.host_schedules
            .snapshot
            .as_ref()
            .map(|snapshot| scheduled_task_filter_options(&snapshot.entries))
            .unwrap_or_else(|| {
                vec![
                    ScheduledTaskFilter::All,
                    ScheduledTaskFilter::Enabled,
                    ScheduledTaskFilter::Disabled,
                    ScheduledTaskFilter::Failed,
                ]
            })
    }

    pub(in crate::health) fn schedule_list_state(&self) -> ListState {
        self.host_schedules.list_state.clone()
    }

    pub(crate) fn schedule_expanded_index(&self) -> Option<usize> {
        self.host_schedules.expanded_index
    }

    pub(crate) fn select_schedule_filter(
        &mut self,
        filter: ScheduledTaskFilter,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_schedules.filter == filter {
            return false;
        }
        self.host_schedules.filter = filter;
        self.host_schedules.expanded_index = None;
        cx.notify();
        true
    }

    pub(crate) fn toggle_schedule_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        self.host_schedules.expanded_index =
            (self.host_schedules.expanded_index != Some(index)).then_some(index);
        cx.notify();
    }

    pub(in crate::health) fn sync_schedule_list_signatures(
        &self,
        identity: &str,
        signatures: &[u64],
    ) {
        sync_tauri_variable_list_state_by_signatures(
            &self.host_schedules.list_state,
            &mut self.host_schedules.list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(px(HOST_SCHEDULE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn sync_host_schedule_list_state(
        &self,
        rows: &[ResourceScheduledTask],
        selected_id: &str,
    ) {
        let expanded = self.schedule_expanded_index();
        let signatures = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                expandable_row_signature(scheduled_task_row_signature(row), expanded == Some(index))
            })
            .collect::<Vec<_>>();
        // Expansion changes one row's height, so it stays out of the identity:
        // a new identity resets the list and drops the user's scroll offset.
        let identity = format!(
            "host-schedules:{selected_id}:{}:{}",
            self.ui.host_schedule_search_query,
            self.schedule_filter() as u8
        );
        self.sync_schedule_list_signatures(&identity, &signatures);
    }

    pub(super) fn request_schedule_snapshot_from_view(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        let (Some(runtime), Some(messages)) =
            (self.lifecycle_runtime.clone(), self.messages.as_ref())
        else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ScheduleConnectionMissing,
            ));
            return;
        };
        let notices = self.request_schedule_snapshot(
            connection_id,
            feedback,
            self.monitoring.schedules_enabled,
            runtime,
            messages.schedule_unknown_error.clone(),
            cx,
        );
        for notice in notices {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(super) fn request_schedule_logs_from_view(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        failure_fallback: String,
        empty_fallback: String,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ScheduleConnectionMissing,
            ));
            return;
        };
        let notices = self.request_schedule_logs(
            connection_id,
            task,
            runtime,
            failure_fallback,
            empty_fallback,
            cx,
        );
        for notice in notices {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(crate) fn request_schedule_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        monitoring_enabled: bool,
        runtime: tokio::runtime::Handle,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if !monitoring_enabled {
            return Vec::new();
        }
        if self.host_schedules.snapshot_in_flight {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::ScheduleSnapshotAlreadyRunning)
                .into_iter()
                .collect();
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::ScheduleConnectionMissing)
                .into_iter()
                .collect();
        };
        let command = build_scheduled_task_snapshot_command(&os_type);
        let mut notices = Vec::new();
        if feedback.should_toast() && command.capability == ScheduledTaskCapability::Partial {
            notices.push(HostToolsNotice::SchedulePartialSupport { os_type });
        }
        let request = HostScheduleSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            failure_fallback,
        };
        self.host_schedules.snapshot_connection_id = Some(connection_id);
        self.host_schedules.running = Some(request.clone());
        self.host_schedules.snapshot_in_flight = true;
        // Inventory scans remain manual and never join the metric sampler.
        let spawned = self.spawn_schedule_snapshot_capture(
            command.command,
            request,
            HOST_SCHEDULE_SNAPSHOT_TIMEOUT,
            HOST_SCHEDULE_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_schedules.snapshot_in_flight = false;
            self.host_schedules.running = None;
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::ScheduleConnectionMissing)
                .into_iter()
                .collect();
        }
        cx.notify();
        notices
    }

    pub(crate) fn finish_host_schedules_snapshot(
        &mut self,
        mut delivery: HostScheduleSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_schedules.running.as_ref() != Some(&delivery.request) {
            if let Ok(output) = delivery.result.as_mut() {
                zeroize_host_snapshot_output(output);
            }
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_schedules.snapshot_in_flight = false;
        self.host_schedules.running = None;
        match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                let mut snapshot = parse_scheduled_task_snapshot(&output.stdout);
                if matches!(&snapshot.status, ResourceScheduledTaskStatus::Error { .. }) {
                    snapshot.status = ResourceScheduledTaskStatus::Error {
                        message: failure_fallback,
                    };
                }
                zeroize_host_snapshot_output(&mut output);
                if feedback.should_toast() {
                    match &snapshot.status {
                        ResourceScheduledTaskStatus::Available { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::ScheduleSnapshotLoaded {
                                    count: snapshot.entries.len(),
                                },
                            ));
                        }
                        ResourceScheduledTaskStatus::Unavailable => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::ScheduleUnavailable,
                            ));
                        }
                        ResourceScheduledTaskStatus::Error { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::ScheduleSnapshotFailed,
                            ));
                        }
                        ResourceScheduledTaskStatus::Unknown => {}
                    }
                }
                self.host_schedules.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_schedules.snapshot = Some(snapshot);
            }
            Ok(mut output) => {
                zeroize_host_snapshot_output(&mut output);
                self.host_schedules.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_schedules.snapshot = Some(ResourceScheduledTaskSnapshot {
                    status: ResourceScheduledTaskStatus::Error {
                        message: failure_fallback,
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::ScheduleSnapshotFailed,
                    ));
                }
            }
            Err(()) => {
                self.host_schedules.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_schedules.snapshot = Some(ResourceScheduledTaskSnapshot {
                    status: ResourceScheduledTaskStatus::Error {
                        message: failure_fallback,
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::ScheduleSnapshotFailed,
                    ));
                }
            }
        }
        // A filter the new snapshot does not offer would render an empty list.
        if !self
            .schedule_filter_options()
            .contains(&self.host_schedules.filter)
        {
            self.host_schedules.filter = ScheduledTaskFilter::All;
            self.host_schedules.expanded_index = None;
        }
        cx.notify();
    }

    pub(in crate::health) fn schedule_action_running_for(&self, task_id: &str) -> bool {
        self.host_schedules
            .action_running
            .as_ref()
            .is_some_and(|request| request.task_id == task_id)
    }

    pub(crate) fn open_schedule_action_confirm(
        &mut self,
        request: HostScheduleActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_schedules.action_running.is_some() {
            return Some(HostToolsNotice::ScheduleActionAlreadyRunning);
        }
        HostToolConfirmState::open(&mut self.host_schedules.pending_confirm, request);
        cx.notify();
        None
    }

    pub(crate) fn schedule_confirm_view(
        &self,
    ) -> Option<(
        HostScheduleActionRequest,
        oxideterm_gpui_ui::motion::ExitPhase,
    )> {
        self.host_schedules
            .pending_confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    /// Dismisses a pending confirmation without affecting an in-flight remote action.
    pub(crate) fn dismiss_schedule_confirm(&mut self, cx: &mut Context<Self>) {
        if self.host_schedules.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub fn begin_schedule_confirm_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self
            .host_schedules
            .pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_schedules.pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_schedules
                    .pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_schedules.pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(in crate::health) fn confirm_schedule_action(
        &mut self,
        delay: Duration,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(request) = self
            .host_schedules
            .pending_confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return Vec::new();
        };
        if !self.begin_schedule_confirm_exit(delay, cx) {
            return Vec::new();
        }
        self.start_schedule_action(request, runtime, cx)
    }

    pub(crate) fn start_schedule_action(
        &mut self,
        request: HostScheduleActionRequest,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(os_type) = self.connection_os_type(&request.connection_id) else {
            return vec![HostToolsNotice::ScheduleConnectionMissing];
        };
        let command = match build_scheduled_task_action_command(&os_type, request.action.clone()) {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::ScheduleActionFailed],
        };
        let mut notices = Vec::new();
        if command.capability == ScheduledTaskCapability::Partial {
            notices.push(HostToolsNotice::SchedulePartialSupport { os_type });
        }
        self.host_schedules.action_running = Some(request.clone());
        let spawned = self.spawn_schedule_action(
            command.command,
            request,
            HOST_SCHEDULE_ACTION_TIMEOUT,
            HOST_SCHEDULE_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_schedules.action_running = None;
            return vec![HostToolsNotice::ScheduleConnectionMissing];
        }
        cx.notify();
        notices
    }

    pub(in crate::health) fn request_schedule_logs(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        runtime: tokio::runtime::Handle,
        failure_fallback: String,
        empty_fallback: String,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self
            .host_schedules
            .logs_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.loading)
        {
            return vec![HostToolsNotice::ScheduleLogsAlreadyRunning];
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return vec![HostToolsNotice::ScheduleConnectionMissing];
        };
        let command = match build_scheduled_task_logs_command(
            &os_type,
            &task,
            false,
            HOST_SCHEDULE_LOG_LINE_LIMIT,
        ) {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::ScheduleLogsFailed],
        };
        let mut notices = Vec::new();
        if command.capability == ScheduledTaskCapability::Partial {
            notices.push(HostToolsNotice::SchedulePartialSupport { os_type });
        }
        let request = HostScheduleLogsRequest {
            connection_id,
            task_id: task.id,
            task_name: task.name,
            task_source: task.source,
            task_unit: task.unit,
            failure_fallback,
            empty_fallback,
        };
        self.host_schedules.logs_dialog = Some(HostScheduleLogsDialog {
            request: request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        let spawned = self.spawn_schedule_logs_capture(
            command.command,
            request,
            HOST_SCHEDULE_LOGS_TIMEOUT,
            HOST_SCHEDULE_LOGS_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_schedules.logs_dialog = None;
            return vec![HostToolsNotice::ScheduleConnectionMissing];
        }
        cx.notify();
        notices
    }

    pub fn schedule_logs_connection_id(&self) -> Option<&str> {
        self.host_schedules
            .logs_dialog
            .as_ref()
            .map(|dialog| dialog.request.connection_id.as_str())
    }

    pub(super) fn schedule_logs_dialog(&self) -> Option<HostScheduleLogsDialog> {
        self.host_schedules.logs_dialog.clone()
    }

    pub fn dismiss_schedule_logs_dialog(&mut self, cx: &mut Context<Self>) {
        if self.host_schedules.logs_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn finish_host_schedule_logs(
        &mut self,
        delivery: HostScheduleLogsDelivery,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .host_schedules
            .logs_dialog
            .as_mut()
            .filter(|dialog| dialog.request == delivery.request)
        else {
            return;
        };
        dialog.loading = false;
        match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                zeroize::Zeroize::zeroize(&mut output.stderr);
                let retained_output = if output.stdout.trim().is_empty() {
                    delivery.request.empty_fallback
                } else {
                    std::mem::take(&mut output.stdout)
                };
                // One shared owner retains the requested output and clears it
                // when both the Entity and current render tree release it.
                dialog.output = Some(Arc::new(zeroize::Zeroizing::new(retained_output)));
                dialog.error = None;
            }
            Ok(mut output) => {
                // Failed output is never user-facing and is cleared immediately.
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

    pub(crate) fn finish_host_schedule_action(
        &mut self,
        delivery: HostScheduleActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_schedules.action_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_schedules.action_running = None;
        let succeeded = delivery.result.unwrap_or(false);
        let HostScheduleActionRequest {
            connection_id,
            task_name,
            action,
            ..
        } = delivery.request;
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::ScheduleActionFinished {
                kind: schedule_action_notice_kind(&action),
                task_name,
                succeeded,
            },
        ));
        self.refresh_schedule_snapshot_after_action(connection_id, cx);
        cx.notify();
    }

    fn refresh_schedule_snapshot_after_action(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.monitoring.schedules_enabled
            || !self.visibility.sidebar_is_visible()
            || self.active_tool() != ContextSidebarTool::Schedules
        {
            return;
        }
        let (Some(runtime), Some(messages)) =
            (self.lifecycle_runtime.clone(), self.messages.as_ref())
        else {
            return;
        };
        let failure_fallback = messages.schedule_unknown_error.clone();
        let notices = self.request_schedule_snapshot(
            connection_id,
            HostSnapshotFeedback::Silent,
            true,
            runtime,
            failure_fallback,
            cx,
        );
        debug_assert!(notices.is_empty());
    }
}
