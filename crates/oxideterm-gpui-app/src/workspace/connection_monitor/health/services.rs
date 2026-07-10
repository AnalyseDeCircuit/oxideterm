//! Owns the services Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::service_action_availability;

impl WorkspaceApp {
    pub(super) fn render_host_services_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::WifiOff,
                self.tokens.ui.text_muted,
                self.i18n.t("profiler.panel.no_connection"),
                cx,
            );
        }

        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let current = self
            .connection_monitor
            .profiler_registry
            .current(&active_connection.connection_id);
        let metrics = current.as_ref().and_then(|(metrics, _)| metrics.as_ref());
        let rows = metrics
            .map(|metrics| {
                visible_service_rows(
                    &metrics.services.services,
                    &self.connection_monitor.host_service_search_query,
                )
            })
            .unwrap_or_default();
        let service_status = metrics
            .map(|metrics| metrics.services.status.clone())
            .unwrap_or_default();
        self.sync_host_service_list_state(&rows, selected_id);

        div()
            .id("host-services-panel")
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
                    .child(self.render_host_service_search(cx))
                    .child(self.render_host_service_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        service_status.clone(),
                        cx,
                    )),
            )
            .child(self.render_host_service_list(
                rows,
                current.is_some(),
                service_status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_service_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostServiceSearch;
        let focused = self.connection_monitor.host_service_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_service_search_query,
                    placeholder: self.i18n.t("sidebar.host_services.search_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.connection_monitor.host_service_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_log_search_focused = false;
                    this.connection_monitor.host_tmux_search_focused = false;
                    this.connection_monitor.host_port_search_focused = false;
                    this.connection_monitor.host_schedule_search_focused = false;
                    this.connection_monitor.host_filesystem_search_focused = false;
                    this.connection_monitor.host_package_search_focused = false;
                    this.ime_marked_text = None;
                    this.new_connection_caret_visible = true;
                    window.focus(&this.focus_handle);
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

    pub(super) fn render_host_service_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceServiceStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let capability_label = match status {
            ResourceServiceStatus::Available {
                capability: ServiceCommandCapability::Full,
                ..
            } => self.i18n.t("sidebar.host_services.capability.full"),
            ResourceServiceStatus::Available {
                capability: ServiceCommandCapability::Partial,
                ..
            } => self.i18n.t("sidebar.host_services.capability.partial"),
            _ => self.i18n.t("sidebar.host_services.capability.unknown"),
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().flex_none().child(format!(
                "{} {} · {}",
                visible_count,
                self.i18n.t("sidebar.host_services.count_suffix"),
                capability_label
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
                self.i18n.t("sidebar.host_services.actions.refresh"),
                "host-service-refresh",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.refresh_host_service_snapshot(selected_id.clone(), cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(super) fn render_host_service_list(
        &self,
        rows: Vec<ResourceService>,
        has_metrics: bool,
        status: ResourceServiceStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_metrics {
            return monitor_center_state(
                self,
                LucideIcon::Wrench,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_services.sampling"),
                cx,
            );
        }
        match status {
            ResourceServiceStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Wrench,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_services.unavailable"),
                    cx,
                );
            }
            ResourceServiceStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_services.error", &[("error", message)]),
                    cx,
                );
            }
            ResourceServiceStatus::Unknown | ResourceServiceStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Wrench,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_services.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.connection_monitor.host_service_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_service_table_header())
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
                                this.render_host_service_row(
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

    pub(super) fn render_host_service_table_header(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_SERVICE_TABLE_HEADER_HEIGHT))
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
                    .child(self.i18n.t("sidebar.host_services.columns.service")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SERVICE_STATE_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_services.columns.state")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SERVICE_ENABLED_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_services.columns.enabled")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SERVICE_PID_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_services.columns.pid")),
            )
            .into_any_element()
    }

    pub(super) fn render_host_service_row(
        &self,
        connection_id: &str,
        service: Option<ResourceService>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(service) = service else {
            return div().into_any_element();
        };
        let expanded = self.connection_monitor.host_service_expanded_id.as_deref()
            == Some(service.id.as_str());
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let state_label = self.i18n.t(service_state_label_key(&service.active_state));
        let enabled_label = self
            .i18n
            .t(service_enabled_label_key(&service.enabled_state));
        let main_pid = service.main_pid.clone().unwrap_or_else(|| "—".to_string());
        let state_color = service_state_color(&service.active_state, theme.text_muted);

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
                    .h(px(HOST_SERVICE_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Keep the service identity as the first-level flex item.
                    // Nested name columns caused Docker names to collapse to `...`.
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
                            .child(service.id.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(state_color))
                            .font_family(mono_font.clone())
                            .child(state_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_ENABLED_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(enabled_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_PID_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(main_pid),
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
                            .child(format!("{} · {}", service.sub_state, service.description)),
                    )
                    .child(self.render_host_service_inline_actions(connection_id, &service, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_service_detail(&service))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = service.id.clone();
                    move |this, _event, _window, cx| {
                        if this.connection_monitor.host_service_expanded_id.as_deref()
                            == Some(id.as_str())
                        {
                            this.connection_monitor.host_service_expanded_id = None;
                        } else {
                            this.connection_monitor.host_service_expanded_id = Some(id.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_service_inline_actions(
        &self,
        connection_id: &str,
        service: &ResourceService,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self
            .connection_monitor
            .host_service_action_running
            .as_ref()
            .is_some_and(|request| request.service_id == service.id);
        let availability = service_action_availability(service);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.render_host_service_logs_button(connection_id, service, is_running, cx))
            .child(self.render_host_service_follow_logs_button(
                connection_id,
                service,
                is_running,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Start,
                LucideIcon::Play,
                "sidebar.host_services.actions.start",
                false,
                is_running || !availability.can_start,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Stop,
                LucideIcon::Square,
                "sidebar.host_services.actions.stop",
                true,
                is_running || !availability.can_stop,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Restart,
                LucideIcon::RefreshCw,
                "sidebar.host_services.actions.restart",
                true,
                is_running || !availability.can_restart,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Reload,
                LucideIcon::RefreshCcw,
                "sidebar.host_services.actions.reload",
                false,
                is_running || !availability.can_reload,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Enable,
                LucideIcon::CheckCircle,
                "sidebar.host_services.actions.enable",
                false,
                is_running || !availability.can_enable,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Disable,
                LucideIcon::ShieldOff,
                "sidebar.host_services.actions.disable",
                true,
                is_running || !availability.can_disable,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_service_action_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        action: ServiceActionKind,
        icon: LucideIcon,
        label_key: &'static str,
        danger: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key);
        let unsupported = self
            .host_service_action_command(connection_id, &service.id, action.clone())
            .is_err();
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
            "host-service-action",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let service_id = service.id.clone();
                let description = service.description.clone();
                move |this, _event, _window, cx| {
                    this.request_host_service_action(
                        connection_id.clone(),
                        service_id.clone(),
                        description.clone(),
                        action.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_host_service_logs_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = self
            .host_service_logs_command(connection_id, &service.id)
            .is_err();
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
            self.i18n.t("sidebar.host_services.actions.logs"),
            "host-service-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let service_id = service.id.clone();
                let description = service.description.clone();
                move |this, _event, _window, cx| {
                    this.request_host_service_logs(
                        connection_id.clone(),
                        service_id.clone(),
                        description.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_host_service_follow_logs_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = self
            .host_service_follow_logs_command(connection_id, &service.id)
            .is_err()
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
            self.i18n.t("sidebar.host_services.actions.follow_logs"),
            "host-service-follow-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let service_id = service.id.clone();
                let description = service.description.clone();
                move |this, _event, window, cx| {
                    this.open_host_service_follow_logs_terminal(
                        connection_id.clone(),
                        service_id.clone(),
                        description.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_host_service_detail(&self, service: &ResourceService) -> AnyElement {
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
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_services.columns.description"),
                service.description.clone(),
            ))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_services.columns.load"),
                service.load_state.clone(),
            ))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_services.columns.sub_state"),
                service.sub_state.clone(),
            ))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font)
                    .text_color(rgb(theme.text))
                    .child(service.id.clone()),
            )
            .into_any_element()
    }

    pub(super) fn sync_host_service_list_state(&self, rows: &[ResourceService], selected_id: &str) {
        let signatures = rows.iter().map(service_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-services:{selected_id}:{}",
            self.connection_monitor.host_service_search_query
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_service_list_state,
            &mut self.connection_monitor.host_service_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn host_service_action_command(
        &self,
        connection_id: &str,
        service_id: &str,
        action: ServiceActionKind,
    ) -> Result<oxideterm_connection_monitor::ServiceActionCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_service_action_command(&os_type, service_id, action)
    }

    pub(super) fn host_service_logs_command(
        &self,
        connection_id: &str,
        service_id: &str,
    ) -> Result<oxideterm_connection_monitor::ServiceCaptureCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_service_logs_command(&os_type, service_id)
    }

    pub(super) fn host_service_follow_logs_command(
        &self,
        connection_id: &str,
        service_id: &str,
    ) -> Result<oxideterm_connection_monitor::ServiceCaptureCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_service_follow_logs_command(&os_type, service_id)
    }

    pub(super) fn refresh_host_service_snapshot(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        self.connection_monitor
            .profiler_registry
            .stop(&connection_id);
        self.start_connection_monitor_profiler(connection_id, cx);
    }

    pub(in crate::workspace) fn handle_host_service_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_service_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_service_search_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn request_host_service_action(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        action: ServiceActionKind,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_service_action_running
            .is_some()
        {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        self.connection_monitor.host_service_pending_confirm = Some(HostServiceActionRequest {
            connection_id,
            service_id,
            description,
            action,
        });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(super) fn request_host_service_logs(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_service_logs_polling {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Unknown".to_string());
        let command = match build_service_logs_command(&os_type, &service_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_service_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == ServiceCommandCapability::Partial {
            self.push_host_service_toast(
                self.i18n_replace(
                    "sidebar.host_services.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }
        let request = HostServiceLogsRequest {
            connection_id,
            service_id,
            description,
        };
        self.connection_monitor.host_service_logs_dialog = Some(HostServiceLogsDialog {
            request: request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_service_logs_rx = Some(rx);
        self.connection_monitor.host_service_logs_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_SERVICE_LOGS_TIMEOUT,
                    HOST_SERVICE_LOGS_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostServiceLogsDelivery { request, result });
        });
        cx.notify();
    }

    pub(super) fn open_host_service_follow_logs_terminal(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Unknown".to_string());
        let command = match build_service_follow_logs_command(&os_type, &service_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_service_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == ServiceCommandCapability::Partial {
            self.push_host_service_toast(
                self.i18n_replace(
                    "sidebar.host_services.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }
        let title = self.i18n_replace(
            "sidebar.host_services.follow_title",
            &[("name", service_id.clone())],
        );
        // Follow mode belongs in a visible terminal so Ctrl-C and tab lifecycle stop the stream.
        self.open_host_service_terminal_command(
            connection_id,
            description,
            command.command,
            title,
            "sidebar.host_services.toast.follow_opened",
            window,
            cx,
        );
    }

    pub(super) fn open_host_service_terminal_command(
        &mut self,
        connection_id: String,
        description: String,
        command: String,
        title: String,
        opened_toast_key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        match self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id,
            Some(command),
            node.config,
            title,
            node.saved_connection_id,
            None,
            None,
            window,
            cx,
        ) {
            Ok(()) => self.push_host_service_toast(
                self.i18n_replace(opened_toast_key, &[("name", description)]),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_service_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_host_service_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .connection_monitor
            .host_service_pending_confirm
            .is_none()
        {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.connection_monitor.host_service_pending_confirm = None;
                self.clear_standard_confirm_focus();
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_service_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn confirm_host_service_action(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.connection_monitor.host_service_pending_confirm.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        self.start_host_service_action(request, cx);
    }

    pub(super) fn start_host_service_action(
        &mut self,
        request: HostServiceActionRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Unknown".to_string());
        let command = match build_service_action_command(
            &os_type,
            &request.service_id,
            request.action.clone(),
        ) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_service_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == ServiceCommandCapability::Partial {
            self.push_host_service_toast(
                self.i18n_replace(
                    "sidebar.host_services.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let delivery_request = request.clone();
        self.connection_monitor.host_service_action_running = Some(request);
        self.connection_monitor.host_service_action_rx = Some(rx);
        self.connection_monitor.host_service_action_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_SERVICE_ACTION_TIMEOUT,
                    HOST_SERVICE_ACTION_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostServiceActionDelivery {
                request: delivery_request,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::workspace) fn poll_host_service_action_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_monitor.host_service_action_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_service_action_rx.take() else {
            self.connection_monitor.host_service_action_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_service_action_polling = false;
                self.connection_monitor.host_service_action_running = None;
                self.finish_host_service_action(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_service_action_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_service_action_polling = false;
                self.connection_monitor.host_service_action_running = None;
                self.push_host_service_toast(
                    self.i18n.t("sidebar.host_services.toast.action_failed"),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn poll_host_service_logs_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_service_logs_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_service_logs_rx.take() else {
            self.connection_monitor.host_service_logs_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_service_logs_polling = false;
                self.finish_host_service_logs(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_service_logs_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_service_logs_polling = false;
                if let Some(dialog) = self.connection_monitor.host_service_logs_dialog.as_mut() {
                    dialog.loading = false;
                    dialog.error = Some(self.i18n.t("sidebar.host_services.toast.logs_failed"));
                }
                cx.notify();
            }
        }
    }

    pub(super) fn finish_host_service_action(
        &mut self,
        delivery: HostServiceActionDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery.result {
            Ok(output) => match interpret_service_action_output(
                &output.stdout,
                &output.stderr,
                output.exit_code,
            ) {
                HostToolActionOutcome::Succeeded { message } => {
                    self.push_host_service_toast(message, TerminalNoticeVariant::Success);
                }
                HostToolActionOutcome::Failed { message } => {
                    self.push_host_service_toast(message, TerminalNoticeVariant::Error);
                }
            },
            Err(error) => {
                self.push_host_service_toast(error, TerminalNoticeVariant::Error);
            }
        }
        self.refresh_host_service_snapshot(delivery.request.connection_id, cx);
    }

    pub(super) fn finish_host_service_logs(
        &mut self,
        delivery: HostServiceLogsDelivery,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .connection_monitor
            .host_service_logs_dialog
            .as_mut()
            .filter(|dialog| dialog.request == delivery.request)
        else {
            cx.notify();
            return;
        };
        dialog.loading = false;
        match delivery.result {
            Ok(output) if service_action_succeeded(output.exit_code) => {
                let logs = if output.stdout.trim().is_empty() {
                    self.i18n.t("sidebar.host_services.logs.empty")
                } else {
                    output.stdout
                };
                dialog.output = Some(logs);
                dialog.error = None;
            }
            Ok(output) => {
                dialog.output = None;
                dialog.error = Some(service_action_failure_message(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                ));
            }
            Err(error) => {
                dialog.output = None;
                dialog.error = Some(error);
            }
        }
        cx.notify();
    }

    pub(super) fn push_host_service_toast(
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

    pub(in crate::workspace) fn render_host_service_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let request = self
            .connection_monitor
            .host_service_pending_confirm
            .as_ref()?;
        let title = self.i18n.t("sidebar.host_services.confirm.title");
        let description = self.i18n_replace(
            host_service_confirm_description_key(&request.action),
            &[
                ("name", request.description.clone()),
                ("id", request.service_id.clone()),
            ],
        );
        Some(
            confirm_dialog_with_focus(
                &self.tokens,
                ConfirmDialogView {
                    variant: if matches!(
                        request.action,
                        ServiceActionKind::Stop
                            | ServiceActionKind::Restart
                            | ServiceActionKind::Disable
                    ) {
                        ConfirmDialogVariant::Danger
                    } else {
                        ConfirmDialogVariant::Default
                    },
                    title: div().child(title).into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("sidebar.host_services.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t(host_service_confirm_label_key(&request.action)))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.connection_monitor.host_service_pending_confirm = None;
                    this.clear_standard_confirm_focus();
                    cx.notify();
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_host_service_action(cx);
                }),
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_host_service_logs_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.connection_monitor.host_service_logs_dialog.as_ref()?;
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let follow_connection_id = dialog.request.connection_id.clone();
        let follow_service_id = dialog.request.service_id.clone();
        let follow_description = dialog.request.description.clone();
        let follow_logs_disabled = self
            .host_service_follow_logs_command(&follow_connection_id, &follow_service_id)
            .is_err()
            || self
                .node_router
                .node_id_for_connection(&follow_connection_id)
                .is_none();
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("sidebar.host_services.logs.loading"))
                .into_any_element()
        } else if let Some(error) = dialog.error.as_ref() {
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element()
        } else {
            let output = dialog.output.clone().unwrap_or_default();
            // Service logs keep their original line shape, so the dialog body
            // owns horizontal overflow just like Docker logs.
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
                        .id(("host-service-log-line", index))
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
                        this.connection_monitor.host_service_logs_dialog = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .w(px(HOST_SERVICE_LOGS_DIALOG_WIDTH))
                        .max_h(px(HOST_SERVICE_LOGS_DIALOG_MAX_HEIGHT))
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
                                                    "sidebar.host_services.logs.title",
                                                    &[("name", dialog.request.service_id.clone())],
                                                )),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(dialog.request.description.clone()),
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
                                            self.i18n
                                                .t("sidebar.host_services.actions.follow_logs"),
                                            "host-service-logs-follow",
                                            true,
                                            cx.listener({
                                                let connection_id = follow_connection_id.clone();
                                                let service_id = follow_service_id.clone();
                                                let description = follow_description.clone();
                                                move |this, _event, window, cx| {
                                                    this.connection_monitor.host_service_logs_dialog =
                                                        None;
                                                    this.open_host_service_follow_logs_terminal(
                                                        connection_id.clone(),
                                                        service_id.clone(),
                                                        description.clone(),
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
                                            self.i18n.t("sidebar.host_services.logs.close"),
                                            "host-service-logs-close",
                                            true,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.connection_monitor.host_service_logs_dialog =
                                                    None;
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                            cx.entity(),
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .id("host-service-logs-scroll")
                                .flex_1()
                                .min_h_0()
                                .max_h(px(HOST_SERVICE_LOGS_DIALOG_MAX_HEIGHT - 84.0))
                                .overflow_y_scroll()
                                .overflow_x_scrollbar()
                                .child(content),
                        ),
                ))
                .into_any_element(),
        )
    }
}

fn host_service_confirm_description_key(action: &ServiceActionKind) -> &'static str {
    match action {
        ServiceActionKind::Start => "sidebar.host_services.confirm.start_desc",
        ServiceActionKind::Stop => "sidebar.host_services.confirm.stop_desc",
        ServiceActionKind::Restart => "sidebar.host_services.confirm.restart_desc",
        ServiceActionKind::Reload => "sidebar.host_services.confirm.reload_desc",
        ServiceActionKind::Enable => "sidebar.host_services.confirm.enable_desc",
        ServiceActionKind::Disable => "sidebar.host_services.confirm.disable_desc",
    }
}

fn host_service_confirm_label_key(action: &ServiceActionKind) -> &'static str {
    match action {
        ServiceActionKind::Start => "sidebar.host_services.actions.start",
        ServiceActionKind::Stop => "sidebar.host_services.actions.stop",
        ServiceActionKind::Restart => "sidebar.host_services.actions.restart",
        ServiceActionKind::Reload => "sidebar.host_services.actions.reload",
        ServiceActionKind::Enable => "sidebar.host_services.actions.enable",
        ServiceActionKind::Disable => "sidebar.host_services.actions.disable",
    }
}

fn service_state_color(state: &str, muted_color: u32) -> u32 {
    match state.trim().to_lowercase().as_str() {
        "active" | "running" => MONITOR_EMERALD,
        "activating" | "deactivating" | "reloading" => MONITOR_AMBER,
        "failed" => MONITOR_RED,
        _ => muted_color,
    }
}
