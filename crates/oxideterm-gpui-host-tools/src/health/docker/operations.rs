use super::*;

impl HostToolsEntity {
    pub(super) fn refresh_host_docker_snapshot(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        self.request_profiler_refresh(connection_id, cx);
    }

    pub fn render_host_docker_logs_dialog(
        &self,
        follow_terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.docker_logs_dialog()?;
        let theme = tokens.ui;
        let follow_connection_id = dialog.request.connection_id.clone();
        let follow_container_id = dialog.request.container_id.clone();
        let follow_title = i18n
            .t("sidebar.host_docker.follow_title")
            .replace("{{name}}", &dialog.request.container_name);
        let follow_opened_notice = i18n
            .t("sidebar.host_docker.toast.follow_opened")
            .replace("{{name}}", &dialog.request.container_name);
        let missing_notice = i18n.t("sidebar.host_docker.toast.exec_terminal_missing");
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(i18n.t("sidebar.host_docker.logs.loading"))
                .into_any_element()
        } else if let Some(error) = dialog.error.as_ref() {
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element()
        } else {
            let output = dialog.output.as_deref();
            // The retained Arc<Zeroizing<String>> remains the sole log owner.
            // Per-line strings are the bounded GPUI frame output boundary.
            let mut lines = div()
                .p_3()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .font_family(mono_font_family)
                .text_size(px(11.0))
                .text_color(rgb(theme.text));
            if let Some(output) = output {
                for (index, line) in output.lines().enumerate() {
                    let display_line = if line.is_empty() {
                        " ".to_string()
                    } else {
                        line.to_string()
                    };
                    lines = lines.child(
                        div()
                            .id(("host-docker-log-line", index))
                            .flex_none()
                            .whitespace_nowrap()
                            .child(display_line),
                    );
                }
            }
            lines.into_any_element()
        };

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|host_tools, _event, _window, cx| {
                        host_tools.dismiss_docker_logs_dialog(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(tokens)
                        .w(px(HOST_DOCKER_LOGS_DIALOG_WIDTH))
                        .max_h(px(HOST_DOCKER_LOGS_DIALOG_MAX_HEIGHT))
                        .child(
                            div()
                                .flex_none()
                                .px_4()
                                .py_3()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text))
                                                .child(
                                                    i18n
                                                        .t("sidebar.host_docker.logs.title")
                                                        .replace(
                                                            "{{name}}",
                                                            &dialog.request.container_name,
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(dialog.request.container_id.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(host_tools_tooltip_icon_button(
                                            tokens,
                                            "lucide/activity.svg",
                                            14.0,
                                            rgb(theme.text),
                                            oxideterm_gpui_ui::button::IconButtonOptions {
                                                size: 24.0,
                                                disabled: !follow_terminal_available
                                                    || build_docker_follow_logs_command(
                                                        &follow_container_id,
                                                    )
                                                    .is_err(),
                                                has_background: true,
                                                background: Some(rgb(theme.bg_hover)),
                                                hover_background: Some(rgb(theme.bg_panel)),
                                                idle_opacity: 1.0,
                                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                                    24.0,
                                                )
                                            },
                                            i18n.t("sidebar.host_docker.actions.follow_logs"),
                                            "host-docker-logs-follow",
                                            true,
                                            cx.listener(
                                                move |host_tools, _event, window, cx| {
                                                    host_tools.dismiss_docker_logs_dialog(cx);
                                                    host_tools
                                                        .dispatch_docker_follow_logs_terminal(
                                                            follow_connection_id.clone(),
                                                            follow_container_id.clone(),
                                                            follow_title.clone(),
                                                            follow_opened_notice.clone(),
                                                            missing_notice.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    cx.stop_propagation();
                                                },
                                            ),
                                        ))
                                        .child(host_tools_tooltip_icon_button(
                                            tokens,
                                            "lucide/x.svg",
                                            14.0,
                                            rgb(theme.text_muted),
                                            oxideterm_gpui_ui::button::IconButtonOptions {
                                                size: 24.0,
                                                has_background: true,
                                                background: Some(rgb(theme.bg_hover)),
                                                hover_background: Some(rgb(theme.bg_panel)),
                                                idle_opacity: 1.0,
                                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                                    24.0,
                                                )
                                            },
                                            i18n.t("sidebar.host_docker.logs.close"),
                                            "host-docker-logs-close",
                                            true,
                                            cx.listener(|host_tools, _event, _window, cx| {
                                                host_tools.dismiss_docker_logs_dialog(cx);
                                                cx.stop_propagation();
                                            }),
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .id("host-docker-logs-scroll")
                                .flex_1()
                                .min_h_0()
                                .max_h(px(HOST_DOCKER_LOGS_DIALOG_MAX_HEIGHT - 84.0))
                                .overflow_y_scroll()
                                // Long log lines scroll sideways instead of
                                // being clipped by the modal boundary.
                                .overflow_x_scrollbar()
                                .child(content),
                        ),
                ))
                .into_any_element(),
        )
    }

    pub fn render_host_docker_confirm_dialog(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        focused_action: Option<ConfirmDialogAction>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (request, phase) = self.docker_confirm_view()?;
        let description = i18n
            .t(host_docker_confirm_description_key(&request.action))
            .replace("{{id}}", &request.container_id)
            .replace("{{name}}", &request.container_name);
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let confirm_delay = exit_delay;

        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                tokens,
                "host-docker-confirm-motion",
                phase,
                ConfirmDialogView {
                    variant: if matches!(
                        request.action,
                        DockerActionKind::Stop
                            | DockerActionKind::Restart
                            | DockerActionKind::Remove
                    ) {
                        ConfirmDialogVariant::Danger
                    } else {
                        ConfirmDialogVariant::Default
                    },
                    title: div()
                        .child(i18n.t("sidebar.host_docker.confirm.title"))
                        .into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(i18n.t("sidebar.host_docker.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(i18n.t(host_docker_confirm_label_key(&request.action)))
                        .into_any_element(),
                },
                focused_action,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // The global keyboard-focus marker is transient render input;
                    // the confirmation lifecycle itself belongs to Host Tools.
                    host_tools.begin_docker_confirm_exit(exit_delay, cx);
                }),
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.confirm_docker_action_from_view(confirm_delay, cx);
                }),
            )
            .into_any_element(),
        )
    }

    pub fn confirm_docker_action_from_view(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            // A visible confirmation without a runtime cannot execute; close
            // the request while reporting the same safe connection failure.
            self.begin_docker_confirm_exit(delay, cx);
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::DockerConnectionMissing,
            ));
            return;
        };
        for notice in self.confirm_docker_action(delay, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(in crate::health) fn docker_action_running_for(&self, container_id: &str) -> bool {
        self.host_docker_operations
            .action_running
            .as_ref()
            .is_some_and(|request| request.container_id == container_id)
    }

    pub(in crate::health) fn docker_action_supported(
        &self,
        connection_id: &str,
        container_id: &str,
        action: DockerActionKind,
    ) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_docker_action_command(&os_type, container_id, action).ok())
            .is_some()
    }

    pub(in crate::health) fn docker_logs_supported(
        &self,
        connection_id: &str,
        container_id: &str,
    ) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_docker_logs_command(&os_type, container_id).ok())
            .is_some()
    }

    pub(crate) fn open_docker_action_confirm(
        &mut self,
        request: HostDockerActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_docker_operations.action_running.is_some() {
            return Some(HostToolsNotice::DockerActionAlreadyRunning);
        }
        HostToolConfirmState::open(&mut self.host_docker_operations.pending_confirm, request);
        cx.notify();
        None
    }

    pub fn docker_confirm_is_open(&self) -> bool {
        self.docker_confirm_view().is_some()
    }

    pub(crate) fn docker_confirm_view(
        &self,
    ) -> Option<(
        HostDockerActionRequest,
        oxideterm_gpui_ui::motion::ExitPhase,
    )> {
        self.host_docker_operations
            .pending_confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    /// Dismisses unsubmitted UI state without cancelling a running Docker action.
    pub(crate) fn dismiss_docker_confirm(&mut self, cx: &mut Context<Self>) {
        if self.host_docker_operations.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub fn begin_docker_confirm_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self
            .host_docker_operations
            .pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_docker_operations.pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_docker_operations
                    .pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_docker_operations.pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(in crate::health) fn confirm_docker_action(
        &mut self,
        delay: Duration,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(request) = self
            .host_docker_operations
            .pending_confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return Vec::new();
        };
        if !self.begin_docker_confirm_exit(delay, cx) {
            return Vec::new();
        }
        self.start_docker_action(request, runtime, cx)
    }

    pub(crate) fn start_docker_action(
        &mut self,
        request: HostDockerActionRequest,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(os_type) = self.connection_os_type(&request.connection_id) else {
            return vec![HostToolsNotice::DockerConnectionMissing];
        };
        let command = match build_docker_action_command(
            &os_type,
            &request.container_id,
            request.action.clone(),
        ) {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::DockerActionFailed],
        };
        self.host_docker_operations.action_running = Some(request.clone());
        let spawned = self.spawn_docker_action(
            command.command,
            request,
            HOST_DOCKER_ACTION_TIMEOUT,
            HOST_DOCKER_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_docker_operations.action_running = None;
            return vec![HostToolsNotice::DockerConnectionMissing];
        }
        cx.notify();
        Vec::new()
    }

    pub(in crate::health) fn request_docker_logs(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        runtime: tokio::runtime::Handle,
        failure_fallback: String,
        empty_fallback: String,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self
            .host_docker_operations
            .logs_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.loading)
        {
            return vec![HostToolsNotice::DockerLogsAlreadyRunning];
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return vec![HostToolsNotice::DockerConnectionMissing];
        };
        let command = match build_docker_logs_command(&os_type, &container_id) {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::DockerLogsFailed],
        };
        let request = HostDockerLogsRequest {
            connection_id,
            container_id,
            container_name,
            failure_fallback,
            empty_fallback,
        };
        self.host_docker_operations.logs_dialog = Some(HostDockerLogsDialog {
            request: request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        let spawned = self.spawn_docker_logs_capture(
            command.command,
            request,
            HOST_DOCKER_LOGS_TIMEOUT,
            HOST_DOCKER_LOGS_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_docker_operations.logs_dialog = None;
            return vec![HostToolsNotice::DockerConnectionMissing];
        }
        cx.notify();
        Vec::new()
    }

    pub fn docker_logs_connection_id(&self) -> Option<&str> {
        self.host_docker_operations
            .logs_dialog
            .as_ref()
            .map(|dialog| dialog.request.connection_id.as_str())
    }

    fn docker_logs_dialog(&self) -> Option<HostDockerLogsDialog> {
        self.host_docker_operations.logs_dialog.clone()
    }

    pub fn dismiss_docker_logs_dialog(&mut self, cx: &mut Context<Self>) {
        if self.host_docker_operations.logs_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn finish_host_docker_action(
        &mut self,
        delivery: HostDockerActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_docker_operations.action_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_docker_operations.action_running = None;
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::DockerActionFinished {
                container_name: delivery.request.container_name,
                succeeded: delivery.result.unwrap_or(false),
            },
        ));
        // Sample the remote state transition without clearing the current list.
        self.request_profiler_refresh(delivery.request.connection_id, cx);
        cx.notify();
    }

    pub(crate) fn finish_host_docker_logs(
        &mut self,
        delivery: HostDockerLogsDelivery,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .host_docker_operations
            .logs_dialog
            .as_mut()
            .filter(|dialog| dialog.request == delivery.request)
        else {
            return;
        };
        dialog.loading = false;
        match delivery.result {
            Ok(mut output) if docker_action_succeeded(output.exit_code) => {
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
