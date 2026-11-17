//! Owns the docker Host Tool UI and request lifecycle.

use super::*;

mod operations;

use oxideterm_connection_monitor::docker_action_availability;

struct HostDockerPanelSnapshot {
    connections: Vec<MonitorConnectionOption>,
    selected_connection_id: String,
    rows: Vec<ResourceDockerContainer>,
    visible_count: Option<usize>,
    status: ResourceDockerStatus,
    has_metrics: bool,
}

impl HostToolsEntity {
    fn docker_panel_snapshot(&self) -> Option<HostDockerPanelSnapshot> {
        let connections = self.monitor_connections();
        let fallback_connection_id = connections.first()?.connection_id.clone();
        let selected_connection_id = self
            .selected_connection_id_owned()
            .filter(|selected_id| {
                connections
                    .iter()
                    .any(|connection| connection.connection_id == *selected_id)
            })
            .unwrap_or(fallback_connection_id);
        let current = self.profiler_registry().current(&selected_connection_id);
        let metrics = current.as_ref().and_then(|(metrics, _)| metrics.as_ref());
        let rows = metrics
            .map(|metrics| {
                visible_docker_rows(
                    &metrics.docker.containers,
                    &self.ui.host_docker_search_query,
                )
            })
            .unwrap_or_default();
        let mut status = metrics
            .map(|metrics| metrics.docker.status.clone())
            .unwrap_or_default();
        if matches!(status, ResourceDockerStatus::Unknown)
            && self
                .profiler_registry()
                .failure(&selected_connection_id)
                .is_some()
        {
            status = ResourceDockerStatus::Incomplete;
        }

        Some(HostDockerPanelSnapshot {
            connections,
            selected_connection_id,
            visible_count: matches!(status, ResourceDockerStatus::Available).then_some(rows.len()),
            rows,
            status,
            has_metrics: current.is_some(),
        })
    }

    pub fn render_host_docker_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(mut snapshot) = self.docker_panel_snapshot() else {
            return host_tools_center_state(
                "lucide/wifi-off.svg",
                tokens.ui.text_muted,
                i18n.t("profiler.panel.no_connection"),
                selectable_text,
                cx,
            );
        };
        self.sync_host_docker_list_state(&snapshot.rows, &snapshot.selected_connection_id);
        let selected_connection_id = snapshot.selected_connection_id.clone();
        let search = self.render_host_docker_search(&search_ime, tokens, i18n, cx);
        let list = self.render_host_docker_list(
            std::mem::take(&mut snapshot.rows),
            snapshot.has_metrics,
            std::mem::take(&mut snapshot.status),
            selected_connection_id.clone(),
            terminal_available,
            tokens,
            i18n,
            mono_font_family.clone(),
            selectable_text,
            cx,
        );

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
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &snapshot.connections,
                        &selected_connection_id,
                        snapshot.has_metrics,
                        tokens,
                        mono_font_family,
                        selectable_text,
                        cx,
                    ))
                    .child(search)
                    .child(self.render_host_docker_status_row(
                        snapshot.visible_count,
                        selected_connection_id,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(list)
            .into_any_element()
    }

    fn render_host_docker_search(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = ime.input();
        let anchor_frame = ime.clone();
        text_input_anchor_probe(
            ime.anchor_id(),
            ime.text_input(
                tokens,
                TextInputView {
                    value: &self.ui.host_docker_search_query,
                    placeholder: i18n.t("sidebar.host_docker.search_placeholder"),
                    focused: self.ui.input_is_focused(input),
                    caret_visible: ime.caret_visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: ime.selected_range(),
                    marked_text: ime.marked_text(),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, event: &MouseDownEvent, window, cx| {
                    host_tools.ui.focus_input(input);
                    // The one-shot request lets the root coordinate shared IME state
                    // without retaining WorkspaceApp in this Entity-owned input.
                    window.dispatch_action(
                        Box::new(HostToolsWindowRequest::new(
                            HostToolsWindowIntent::BeginPlainTextImeSelection {
                                input,
                                event: event.clone(),
                            },
                        )),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, _cx| {
                anchor_frame.update_anchor(anchor);
            },
        )
        .into_any_element()
    }

    fn render_host_docker_status_row(
        &self,
        visible_count: Option<usize>,
        selected_connection_id: String,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().flex_none().when_some(visible_count, |this, count| {
                this.child(format!(
                    "{} {}",
                    count,
                    i18n.t("sidebar.host_docker.count_suffix")
                ))
            }))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/refresh-cw.svg",
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
                i18n.t("sidebar.host_docker.actions.refresh"),
                "host-docker-refresh",
                true,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.refresh_host_docker_snapshot(selected_connection_id.clone(), cx);
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_docker_list(
        &self,
        rows: Vec<ResourceDockerContainer>,
        has_metrics: bool,
        status: ResourceDockerStatus,
        selected_connection_id: String,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_metrics {
            return host_tools_center_state(
                "lucide/layers.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_docker.sampling"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceDockerStatus::Unavailable => {
                return host_tools_center_state(
                    "lucide/layers.svg",
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_docker.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceDockerStatus::DaemonUnavailable { .. } => {
                return host_tools_center_state(
                    "lucide/alert-triangle.svg",
                    MONITOR_RED,
                    i18n.t("sidebar.host_docker.daemon_unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceDockerStatus::Error { message } => {
                return host_tools_center_state(
                    "lucide/alert-triangle.svg",
                    MONITOR_RED,
                    i18n.t("sidebar.host_docker.error")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourceDockerStatus::Unknown => {
                return host_tools_center_state(
                    "lucide/layers.svg",
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_docker.sampling"),
                    selectable_text,
                    cx,
                );
            }
            ResourceDockerStatus::Incomplete | ResourceDockerStatus::OutputLimit => {
                let key = if matches!(status, ResourceDockerStatus::OutputLimit) {
                    "sidebar.host_docker.output_limit"
                } else {
                    "sidebar.host_docker.incomplete"
                };
                return host_tools_center_state(
                    "lucide/alert-triangle.svg",
                    MONITOR_RED,
                    i18n.t(key),
                    selectable_text,
                    cx,
                );
            }
            ResourceDockerStatus::Available => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                "lucide/layers.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_docker.empty"),
                selectable_text,
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_connection_id = Arc::new(selected_connection_id);
        let list_state = self.ui.host_docker_list_state.clone();
        let list_spec = TauriVirtualListSpec::new(px(HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let row_tokens = *tokens;
        let row_i18n = i18n.clone();
        let row_mono_font_family = mono_font_family;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_docker_table_header(tokens, i18n))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        list_state,
                        list_spec,
                        move |index, _window, cx| {
                            let rows = Arc::clone(&rows);
                            let selected_connection_id = Arc::clone(&selected_connection_id);
                            let mono_font_family = row_mono_font_family.clone();
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_docker_row(
                                    selected_connection_id.as_str(),
                                    rows.get(index).cloned(),
                                    terminal_available,
                                    &row_tokens,
                                    &row_i18n,
                                    mono_font_family,
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_docker_table_header(&self, tokens: &ThemeTokens, i18n: &I18n) -> AnyElement {
        let theme = tokens.ui;
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
                    .child(i18n.t("sidebar.host_docker.columns.container")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_DOCKER_STATE_COLUMN_WIDTH))
                    .child(i18n.t("sidebar.host_docker.columns.state")),
            )
            .child(
                div()
                    .min_w(px(HOST_DOCKER_PORTS_COLUMN_MIN_WIDTH))
                    .flex_1()
                    .truncate()
                    .child(i18n.t("sidebar.host_docker.columns.ports")),
            )
            .into_any_element()
    }

    fn render_host_docker_row(
        &self,
        connection_id: &str,
        container: Option<ResourceDockerContainer>,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(container) = container else {
            return div().into_any_element();
        };
        let expanded = self.ui.host_docker_expanded_id.as_deref() == Some(container.id.as_str());
        let theme = tokens.ui;
        let state_label = i18n.t(docker_state_label_key(&container.state));
        let ports = container.ports.clone().unwrap_or_else(|| "—".to_string());
        let image_status = if container.image == "-" {
            container.status.clone()
        } else {
            format!("{} · {}", container.image, container.status)
        };

        div()
            .id(host_tools_row_id(
                "host-docker-row",
                connection_id,
                container.id.clone(),
            ))
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
                            .font_family(mono_font_family.clone())
                            .child(container.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_DOCKER_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(docker_state_color(&container.state, theme.text_muted)))
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
                            .child(image_status),
                    )
                    .child(self.render_host_docker_inline_actions(
                        connection_id,
                        &container,
                        terminal_available,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .when(expanded, |row| {
                row.child(self.render_host_docker_detail(
                    &container,
                    tokens,
                    i18n,
                    mono_font_family,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let container_id = container.id.clone();
                    move |host_tools, _event, _window, cx| {
                        let expanded_id = &mut host_tools.ui.host_docker_expanded_id;
                        if expanded_id.as_deref() == Some(container_id.as_str()) {
                            *expanded_id = None;
                        } else {
                            *expanded_id = Some(container_id.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    fn render_host_docker_inline_actions(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action_running = self.docker_action_running_for(&container.id);
        let availability = docker_action_availability(&container.state);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.render_host_docker_logs_button(
                connection_id,
                container,
                action_running,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_docker_follow_logs_button(
                connection_id,
                container,
                action_running || !availability.can_use_live_tools || !terminal_available,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_docker_exec_button(
                connection_id,
                container,
                action_running || !availability.can_use_live_tools || !terminal_available,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Start,
                "lucide/play.svg",
                "sidebar.host_docker.actions.start",
                false,
                action_running || !availability.can_start,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Stop,
                "lucide/square.svg",
                "sidebar.host_docker.actions.stop",
                true,
                action_running || !availability.can_stop,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Restart,
                "lucide/refresh-cw.svg",
                "sidebar.host_docker.actions.restart",
                true,
                action_running || !availability.can_restart,
                tokens,
                i18n,
                cx,
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_docker_action_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        action: DockerActionKind,
        icon: &'static str,
        label_key: &'static str,
        danger: bool,
        disabled: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let unsupported =
            !self.docker_action_supported(connection_id, &container.id, action.clone());
        let icon_color = if danger { MONITOR_RED } else { theme.text };
        host_tools_tooltip_icon_button(
            tokens,
            icon,
            13.0,
            rgb(icon_color),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
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
            i18n.t(label_key),
            "host-docker-action",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |host_tools, _event, _window, cx| {
                    host_tools.request_docker_action_from_view(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        action.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
        )
    }

    fn render_host_docker_logs_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let unsupported = !self.docker_logs_supported(connection_id, &container.id);
        let failure_fallback = i18n.t("sidebar.host_docker.toast.logs_failed");
        let empty_fallback = i18n.t("sidebar.host_docker.logs.empty");
        host_tools_tooltip_icon_button(
            tokens,
            "lucide/file-text.svg",
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
            i18n.t("sidebar.host_docker.actions.logs"),
            "host-docker-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |host_tools, _event, _window, cx| {
                    host_tools.request_docker_logs_from_view(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        failure_fallback.clone(),
                        empty_fallback.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
        )
    }

    fn render_host_docker_follow_logs_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let title = i18n
            .t("sidebar.host_docker.follow_title")
            .replace("{{name}}", &container.name);
        let opened_notice = i18n
            .t("sidebar.host_docker.toast.follow_opened")
            .replace("{{name}}", &container.name);
        let missing_notice = i18n.t("sidebar.host_docker.toast.exec_terminal_missing");
        host_tools_tooltip_icon_button(
            tokens,
            "lucide/activity.svg",
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || build_docker_follow_logs_command(&container.id).is_err(),
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            i18n.t("sidebar.host_docker.actions.follow_logs"),
            "host-docker-follow-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                move |host_tools, _event, window, cx| {
                    host_tools.dispatch_docker_follow_logs_terminal(
                        connection_id.clone(),
                        container_id.clone(),
                        title.clone(),
                        opened_notice.clone(),
                        missing_notice.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
        )
    }

    fn render_host_docker_exec_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let title = i18n
            .t("sidebar.host_docker.exec_title")
            .replace("{{name}}", &container.name);
        let opened_notice = i18n
            .t("sidebar.host_docker.toast.exec_opened")
            .replace("{{name}}", &container.name);
        let missing_notice = i18n.t("sidebar.host_docker.toast.exec_terminal_missing");
        host_tools_tooltip_icon_button(
            tokens,
            "lucide/terminal.svg",
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || build_docker_exec_shell_command(&container.id).is_err(),
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            i18n.t("sidebar.host_docker.actions.exec"),
            "host-docker-exec",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                move |host_tools, _event, window, cx| {
                    host_tools.dispatch_docker_exec_terminal(
                        connection_id.clone(),
                        container_id.clone(),
                        title.clone(),
                        opened_notice.clone(),
                        missing_notice.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
        )
    }

    fn request_docker_action_from_view(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        action: DockerActionKind,
        cx: &mut Context<Self>,
    ) {
        if let Some(notice) = self.open_docker_action_confirm(
            HostDockerActionRequest {
                connection_id,
                container_id,
                container_name,
                action,
            },
            cx,
        ) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    fn request_docker_logs_from_view(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        failure_fallback: String,
        empty_fallback: String,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::DockerConnectionMissing,
            ));
            return;
        };
        for notice in self.request_docker_logs(
            connection_id,
            container_id,
            container_name,
            runtime,
            failure_fallback,
            empty_fallback,
            cx,
        ) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    fn dispatch_docker_exec_terminal(
        &mut self,
        connection_id: String,
        container_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(command) = build_docker_exec_shell_command(&container_id) else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::DockerActionFailed,
            ));
            return;
        };
        self.dispatch_docker_terminal_request(
            connection_id,
            command,
            title,
            opened_notice,
            missing_notice,
            window,
            cx,
        );
    }

    fn dispatch_docker_follow_logs_terminal(
        &mut self,
        connection_id: String,
        container_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(command) = build_docker_follow_logs_command(&container_id) else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::DockerLogsFailed,
            ));
            return;
        };
        // Follow mode lives in a terminal consumer so Ctrl-C and tab closure stop only the stream.
        self.dispatch_docker_terminal_request(
            connection_id,
            command,
            title,
            opened_notice,
            missing_notice,
            window,
            cx,
        );
    }

    fn dispatch_docker_terminal_request(
        &self,
        connection_id: String,
        command: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Move the fixed-builder command through the one-shot request. GPUI action
        // cloning shares the envelope and never clones terminal command contents.
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

    fn render_host_docker_detail(
        &self,
        container: &ResourceDockerContainer,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
    ) -> AnyElement {
        let theme = tokens.ui;
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
            .child(Self::render_host_docker_detail_line(
                "ID".to_string(),
                container.id.clone(),
                mono_font_family.clone(),
            ))
            .child(Self::render_host_docker_detail_line(
                i18n.t("sidebar.host_docker.columns.image"),
                container.image.clone(),
                mono_font_family.clone(),
            ))
            .child(Self::render_host_docker_detail_line(
                i18n.t("sidebar.host_docker.columns.ports"),
                container.ports.clone().unwrap_or_else(|| "—".to_string()),
                mono_font_family.clone(),
            ))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font_family)
                    .text_color(rgb(theme.text))
                    .child(container.status.clone()),
            )
            .into_any_element()
    }

    fn render_host_docker_detail_line(
        label: String,
        value: String,
        mono_font_family: SharedString,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .child(div().flex_none().child(label))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(mono_font_family)
                    .child(value),
            )
            .into_any_element()
    }

    fn sync_host_docker_list_state(&self, rows: &[ResourceDockerContainer], selected_id: &str) {
        let signatures = rows.iter().map(docker_row_signature).collect::<Vec<_>>();
        let ui = &self.ui;
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
