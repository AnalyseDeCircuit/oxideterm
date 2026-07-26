//! Owns the docker Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::docker_action_availability;

impl WorkspaceApp {
    pub(super) fn render_host_docker_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections(cx);
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::WifiOff,
                self.tokens.ui.text_muted,
                self.i18n.t("profiler.panel.no_connection"),
                cx,
            );
        }

        let selected_connection_id = self.host_tools.read(cx).selected_connection_id_owned();
        let selected_id = selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let current = self
            .host_tools
            .read(cx)
            .profiler_registry()
            .current(&active_connection.connection_id);
        let metrics = current.as_ref().and_then(|(metrics, _)| metrics.as_ref());
        let docker_search_query = self.host_tools.read(cx).ui.host_docker_search_query.clone();
        let rows = metrics
            .map(|metrics| visible_docker_rows(&metrics.docker.containers, &docker_search_query))
            .unwrap_or_default();
        let docker_status = metrics
            .map(|metrics| metrics.docker.status.clone())
            .unwrap_or_default();
        self.sync_host_docker_list_state(&rows, selected_id, cx);

        div()
            .id("host-docker-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher_row(
                        &connections,
                        selected_id,
                        current.is_some(),
                        cx,
                    ))
                    .child(self.render_host_docker_search(cx))
                    .child(self.render_host_docker_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        cx,
                    )),
            )
            .child(self.render_host_docker_list(
                rows,
                current.is_some(),
                docker_status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_docker_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostDockerSearch;
        let (focused, value) = {
            let ui = &self.host_tools.read(cx).ui;
            (
                ui.input_is_focused(HostToolsTextInput::DockerSearch),
                ui.host_docker_search_query.clone(),
            )
        };
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder: self.i18n.t("sidebar.host_docker.search_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.host_tools.update(cx, |host_tools, _cx| {
                        host_tools.ui.focus_input(HostToolsTextInput::DockerSearch);
                    });
                    this.ime_marked_text = None;
                    this.new_connection_caret_visible = true;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    pub(super) fn render_host_docker_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().flex_none().child(format!(
                "{} {}",
                visible_count,
                self.i18n.t("sidebar.host_docker.count_suffix")
            )))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::RefreshCw,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 24.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                },
                self.i18n.t("sidebar.host_docker.actions.refresh"),
                "host-docker-refresh",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.refresh_host_docker_snapshot(selected_id.clone(), cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(super) fn render_host_docker_list(
        &self,
        rows: Vec<ResourceDockerContainer>,
        has_metrics: bool,
        status: ResourceDockerStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_metrics {
            return monitor_center_state(
                self,
                LucideIcon::Layers,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_docker.sampling"),
                cx,
            );
        }
        match status {
            ResourceDockerStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Layers,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_docker.unavailable"),
                    cx,
                );
            }
            ResourceDockerStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_docker.error", &[("error", message)]),
                    cx,
                );
            }
            ResourceDockerStatus::Unknown | ResourceDockerStatus::Available => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Layers,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_docker.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.host_tools.read(cx).ui.host_docker_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_docker_table_header())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            let rows = rows.clone();
                            let selected_id = selected_id.clone();
                            workspace.update(cx, |this, cx| {
                                this.render_host_docker_row(
                                    selected_id.as_str(),
                                    rows.get(index).cloned(),
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_host_docker_table_header(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_DOCKER_TABLE_HEADER_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg))
            .text_size(px(HOST_PROCESS_TABLE_HEADER_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(self.i18n.t("sidebar.host_docker.columns.container")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_DOCKER_STATE_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_docker.columns.state")),
            )
            .child(
                div()
                    .min_w(px(HOST_DOCKER_PORTS_COLUMN_MIN_WIDTH))
                    .flex_1()
                    .truncate()
                    .child(self.i18n.t("sidebar.host_docker.columns.ports")),
            )
            .into_any_element()
    }

    pub(super) fn render_host_docker_row(
        &self,
        connection_id: &str,
        container: Option<ResourceDockerContainer>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(container) = container else {
            return div().into_any_element();
        };
        let expanded = self
            .host_tools
            .read(cx)
            .ui
            .host_docker_expanded_id
            .as_deref()
            == Some(container.id.as_str());
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let state_label = self.i18n.t(docker_state_label_key(&container.state));
        let ports = container.ports.clone().unwrap_or_else(|| "—".to_string());
        let image_status = if container.image == "-" {
            container.status.clone()
        } else {
            format!("{} · {}", container.image, container.status)
        };

        div()
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(HOST_DOCKER_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(container.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_DOCKER_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(docker_state_color(&container.state, theme.text_muted)))
                            .font_family(mono_font.clone())
                            .child(state_label),
                    )
                    .child(
                        div()
                            .min_w(px(HOST_DOCKER_PORTS_COLUMN_MIN_WIDTH))
                            .flex_1()
                            .truncate()
                            .whitespace_nowrap()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(ports),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font)
                            .child(image_status),
                    )
                    .child(self.render_host_docker_inline_actions(connection_id, &container, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_docker_detail(&container))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = container.id.clone();
                    move |this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, _cx| {
                            let expanded_id = &mut host_tools.ui.host_docker_expanded_id;
                            if expanded_id.as_deref() == Some(id.as_str()) {
                                *expanded_id = None;
                            } else {
                                *expanded_id = Some(id.clone());
                            }
                        });
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_docker_inline_actions(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self
            .host_tools
            .read(cx)
            .docker_action_running_for(&container.id);
        let availability = docker_action_availability(&container.state);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.render_host_docker_logs_button(connection_id, container, is_running, cx))
            .child(self.render_host_docker_follow_logs_button(
                connection_id,
                container,
                is_running || !availability.can_use_live_tools,
                cx,
            ))
            .child(self.render_host_docker_exec_button(
                connection_id,
                container,
                is_running || !availability.can_use_live_tools,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Start,
                LucideIcon::Play,
                "sidebar.host_docker.actions.start",
                false,
                is_running || !availability.can_start,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Stop,
                LucideIcon::Square,
                "sidebar.host_docker.actions.stop",
                true,
                is_running || !availability.can_stop,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Restart,
                LucideIcon::RefreshCw,
                "sidebar.host_docker.actions.restart",
                true,
                is_running || !availability.can_restart,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_docker_action_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        action: DockerActionKind,
        icon: LucideIcon,
        label_key: &'static str,
        danger: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key);
        let unsupported = !self.host_tools.read(cx).docker_action_supported(
            connection_id,
            &container.id,
            action.clone(),
        );
        let disabled = disabled || unsupported;
        let icon_color = if danger { MONITOR_RED } else { theme.text };
        self.workspace_tooltip_icon_button(
            icon,
            13.0,
            rgb(icon_color),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled,
                has_background: true,
                background: Some(if danger {
                    rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)
                } else {
                    rgb(theme.bg_hover)
                }),
                hover_background: Some(if danger {
                    rgba((MONITOR_RED << 8) | 0x30)
                } else {
                    rgb(theme.bg_panel)
                }),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            label,
            "host-docker-action",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |this, _event, _window, cx| {
                    this.request_host_docker_action(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        action.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_host_docker_logs_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = !self
            .host_tools
            .read(cx)
            .docker_logs_supported(connection_id, &container.id);
        self.workspace_tooltip_icon_button(
            LucideIcon::FileText,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            self.i18n.t("sidebar.host_docker.actions.logs"),
            "host-docker-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |this, _event, _window, cx| {
                    this.request_host_docker_logs(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_host_docker_follow_logs_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = build_docker_follow_logs_command(&container.id).is_err()
            || self
                .node_router
                .node_id_for_connection(connection_id)
                .is_none();
        self.workspace_tooltip_icon_button(
            LucideIcon::Activity,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            self.i18n.t("sidebar.host_docker.actions.follow_logs"),
            "host-docker-follow-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |this, _event, window, cx| {
                    this.open_host_docker_follow_logs_terminal(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_host_docker_exec_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = build_docker_exec_shell_command(&container.id).is_err()
            || self
                .node_router
                .node_id_for_connection(connection_id)
                .is_none();
        self.workspace_tooltip_icon_button(
            LucideIcon::Terminal,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            self.i18n.t("sidebar.host_docker.actions.exec"),
            "host-docker-exec",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |this, _event, window, cx| {
                    this.open_host_docker_exec_terminal(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_host_docker_detail(
        &self,
        container: &ResourceDockerContainer,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
            .px_3()
            .pb_3()
            .pt_2()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(self.render_host_process_detail_line("ID", container.id.clone()))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_docker.columns.image"),
                container.image.clone(),
            ))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_docker.columns.ports"),
                container.ports.clone().unwrap_or_else(|| "—".to_string()),
            ))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font)
                    .text_color(rgb(theme.text))
                    .child(container.status.clone()),
            )
            .into_any_element()
    }

    pub(super) fn sync_host_docker_list_state(
        &self,
        rows: &[ResourceDockerContainer],
        selected_id: &str,
        cx: &mut Context<Self>,
    ) {
        let signatures = rows.iter().map(docker_row_signature).collect::<Vec<_>>();
        self.host_tools.update(cx, |host_tools, _cx| {
            let ui = &host_tools.ui;
            let identity = format!(
                "host-docker:{selected_id}:{}:{}",
                ui.host_docker_search_query,
                ui.host_docker_expanded_id.as_deref().unwrap_or_default()
            );
            sync_tauri_variable_list_state_by_signatures(
                &ui.host_docker_list_state,
                &mut ui.host_docker_list_cache.borrow_mut(),
                &identity,
                &signatures,
                TauriVirtualListSpec::new(px(HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT), 8),
            );
        });
    }

    pub(super) fn refresh_host_docker_snapshot(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        self.host_tools
            .read(cx)
            .profiler_registry()
            .stop(&connection_id);
        self.start_connection_monitor_profiler(connection_id, cx);
    }

    pub(in crate::workspace) fn handle_host_docker_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::DockerSearch)
        {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.host_tools.update(cx, |host_tools, _cx| {
                host_tools.ui.clear_input_focus();
            });
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn request_host_docker_action(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        action: DockerActionKind,
        cx: &mut Context<Self>,
    ) {
        let notice = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.open_docker_action_confirm(
                HostDockerActionRequest {
                    connection_id,
                    container_id,
                    container_name,
                    action,
                },
                cx,
            )
        });
        if let Some(notice) = notice {
            self.push_host_tools_notice(notice);
            return;
        }
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(super) fn request_host_docker_logs(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.handle().clone();
        let failure_fallback = self.i18n.t("sidebar.host_docker.toast.logs_failed");
        let empty_fallback = self.i18n.t("sidebar.host_docker.logs.empty");
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_docker_logs(
                connection_id,
                container_id,
                container_name,
                runtime,
                failure_fallback,
                empty_fallback,
                cx,
            )
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    pub(super) fn open_host_docker_exec_terminal(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match build_docker_exec_shell_command(&container_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_docker_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        let title = self.i18n_replace(
            "sidebar.host_docker.exec_title",
            &[("name", container_name.clone())],
        );
        self.open_host_docker_terminal_command(
            connection_id,
            container_name,
            command,
            title,
            "sidebar.host_docker.toast.exec_opened",
            window,
            cx,
        );
    }

    pub(super) fn open_host_docker_follow_logs_terminal(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match build_docker_follow_logs_command(&container_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_docker_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        let title = self.i18n_replace(
            "sidebar.host_docker.follow_title",
            &[("name", container_name.clone())],
        );
        // Follow mode belongs in a visible terminal so Ctrl-C and tab lifecycle stop the stream.
        self.open_host_docker_terminal_command(
            connection_id,
            container_name,
            command,
            title,
            "sidebar.host_docker.toast.follow_opened",
            window,
            cx,
        );
    }

    pub(super) fn open_host_docker_terminal_command(
        &mut self,
        connection_id: String,
        container_name: String,
        command: String,
        title: String,
        opened_toast_key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_docker_toast(
                self.i18n
                    .t("sidebar.host_docker.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        if !self.ssh_nodes.contains_key(&node_id) {
            self.push_host_docker_toast(
                self.i18n
                    .t("sidebar.host_docker.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        }
        match self.queue_ssh_terminal_tab_for_existing_node(
            node_id,
            Some(command),
            title,
            window,
            cx,
        ) {
            Ok(()) => self.push_host_docker_toast(
                self.i18n_replace(opened_toast_key, &[("name", container_name)]),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_docker_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_host_docker_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).docker_confirm_view().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_docker_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_docker_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn confirm_host_docker_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_docker_action(delay, runtime, cx)
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_docker_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_docker_confirm_exit(delay, cx)
        })
    }

    pub(super) fn push_host_docker_toast(
        &mut self,
        message: String,
        variant: TerminalNoticeVariant,
    ) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    pub(in crate::workspace) fn render_host_docker_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (request, phase) = self.host_tools.read(cx).docker_confirm_view()?;
        let title = self.i18n.t("sidebar.host_docker.confirm.title");
        let description = self.i18n_replace(
            host_docker_confirm_description_key(&request.action),
            &[
                ("id", request.container_id.clone()),
                ("name", request.container_name.clone()),
            ],
        );
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "host-docker-confirm-motion",
                phase,
                ConfirmDialogView {
                    variant: if matches!(
                        request.action,
                        DockerActionKind::Stop | DockerActionKind::Restart
                    ) {
                        ConfirmDialogVariant::Danger
                    } else {
                        ConfirmDialogVariant::Default
                    },
                    title: div().child(title).into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("sidebar.host_docker.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t(host_docker_confirm_label_key(&request.action)))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.begin_host_docker_confirm_exit(cx);
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_host_docker_action(cx);
                }),
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_host_docker_logs_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.host_tools.read(cx).docker_logs_dialog()?;
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let follow_connection_id = dialog.request.connection_id.clone();
        let follow_container_id = dialog.request.container_id.clone();
        let follow_container_name = dialog.request.container_name.clone();
        let follow_logs_disabled = build_docker_follow_logs_command(&follow_container_id).is_err()
            || self
                .node_router
                .node_id_for_connection(&follow_connection_id)
                .is_none();
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("sidebar.host_docker.logs.loading"))
                .into_any_element()
        } else if let Some(error) = dialog.error.as_ref() {
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element()
        } else {
            let output = dialog.output.clone().unwrap_or_default();
            // Docker logs keep their original line shape, so horizontal
            // overflow belongs to the dialog body. Per-line strings are the
            // explicit frame-owned GPUI output boundary.
            let mut lines = div()
                .p_3()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .font_family(mono_font)
                .text_size(px(11.0))
                .text_color(rgb(theme.text));
            for (index, line) in output.lines().enumerate() {
                let line = if line.is_empty() {
                    " ".to_string()
                } else {
                    line.to_string()
                };
                lines = lines.child(
                    div()
                        .id(("host-docker-log-line", index))
                        .flex_none()
                        .whitespace_nowrap()
                        .child(line),
                );
            }
            lines.into_any_element()
        };

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_docker_logs_dialog(cx);
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
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
                                                .child(self.i18n_replace(
                                                    "sidebar.host_docker.logs.title",
                                                    &[(
                                                        "name",
                                                        dialog.request.container_name.clone(),
                                                    )],
                                                )),
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
                                        .child(self.workspace_tooltip_icon_button(
                                            LucideIcon::Activity,
                                            14.0,
                                            rgb(theme.text),
                                            oxideterm_gpui_ui::button::IconButtonOptions {
                                                size: 24.0,
                                                disabled: follow_logs_disabled,
                                                has_background: true,
                                                background: Some(rgb(theme.bg_hover)),
                                                hover_background: Some(rgb(theme.bg_panel)),
                                                idle_opacity: 1.0,
                                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                                    24.0,
                                                )
                                            },
                                            self.i18n.t("sidebar.host_docker.actions.follow_logs"),
                                            "host-docker-logs-follow",
                                            true,
                                            cx.listener({
                                                let connection_id = follow_connection_id;
                                                let container_id = follow_container_id;
                                                let container_name = follow_container_name;
                                                move |this, _event, window, cx| {
                                                    this.host_tools.update(
                                                        cx,
                                                        |host_tools, cx| {
                                                            host_tools
                                                                .dismiss_docker_logs_dialog(cx);
                                                        },
                                                    );
                                                    this.open_host_docker_follow_logs_terminal(
                                                        connection_id.clone(),
                                                        container_id.clone(),
                                                        container_name.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }
                                            }),
                                            cx.entity(),
                                        ))
                                        .child(self.workspace_tooltip_icon_button(
                                            LucideIcon::X,
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
                                            self.i18n.t("sidebar.host_docker.logs.close"),
                                            "host-docker-logs-close",
                                            true,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.host_tools.update(
                                                    cx,
                                                    |host_tools, cx| {
                                                        host_tools
                                                            .dismiss_docker_logs_dialog(cx);
                                                    },
                                                );
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                            cx.entity(),
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
                                // Long log lines should scroll sideways instead
                                // of being clipped by the modal boundary.
                                .overflow_x_scrollbar()
                                .child(content),
                        ),
                ))
                .into_any_element(),
        )
    }
}

impl HostToolsEntity {
    pub(super) fn docker_action_running_for(&self, container_id: &str) -> bool {
        self.host_docker_operations
            .action_running
            .as_ref()
            .is_some_and(|request| request.container_id == container_id)
    }

    pub(super) fn docker_action_supported(
        &self,
        connection_id: &str,
        container_id: &str,
        action: DockerActionKind,
    ) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_docker_action_command(&os_type, container_id, action).ok())
            .is_some()
    }

    pub(super) fn docker_logs_supported(&self, connection_id: &str, container_id: &str) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_docker_logs_command(&os_type, container_id).ok())
            .is_some()
    }

    pub(in crate::workspace::connection_monitor) fn open_docker_action_confirm(
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

    pub(in crate::workspace::connection_monitor) fn docker_confirm_view(
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
    pub(in crate::workspace::connection_monitor) fn dismiss_docker_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.host_docker_operations.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn begin_docker_confirm_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
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

    pub(super) fn confirm_docker_action(
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

    fn start_docker_action(
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

    pub(super) fn request_docker_logs(
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

    pub(super) fn docker_logs_dialog(&self) -> Option<HostDockerLogsDialog> {
        self.host_docker_operations.logs_dialog.clone()
    }

    pub(super) fn dismiss_docker_logs_dialog(&mut self, cx: &mut Context<Self>) {
        if self.host_docker_operations.logs_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_docker_action(
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
        // Force a new Docker sample after the remote state transition.
        self.profiler_registry.stop(&delivery.request.connection_id);
        cx.emit(HostToolsEvent::RefreshProfiler {
            connection_id: delivery.request.connection_id,
        });
        cx.notify();
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_docker_logs(
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

fn host_docker_confirm_description_key(action: &DockerActionKind) -> &'static str {
    match action {
        DockerActionKind::Start => "sidebar.host_docker.confirm.start_desc",
        DockerActionKind::Stop => "sidebar.host_docker.confirm.stop_desc",
        DockerActionKind::Restart => "sidebar.host_docker.confirm.restart_desc",
    }
}

fn host_docker_confirm_label_key(action: &DockerActionKind) -> &'static str {
    match action {
        DockerActionKind::Start => "sidebar.host_docker.actions.start",
        DockerActionKind::Stop => "sidebar.host_docker.actions.stop",
        DockerActionKind::Restart => "sidebar.host_docker.actions.restart",
    }
}

fn docker_state_color(state: &str, muted_color: u32) -> u32 {
    match state.trim().to_lowercase().as_str() {
        "running" => MONITOR_EMERALD,
        "created" | "paused" | "restarting" => MONITOR_AMBER,
        "dead" | "removing" => MONITOR_RED,
        "exited" => muted_color,
        _ => muted_color,
    }
}
