//! Owns the services Host Tool UI and request lifecycle.

use super::*;

mod operations;

use oxideterm_connection_monitor::{
    ResourceServiceSnapshot, build_service_snapshot_command, parse_service_snapshot,
    service_action_availability,
};

impl HostToolsEntity {
    pub fn render_host_service_confirm_dialog(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        focused_action: Option<ConfirmDialogAction>,
        exit_delay: Duration,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (request, phase) = self.service_confirm_view()?;
        let title = i18n.t("sidebar.host_services.confirm.title");
        let description = i18n
            .t(host_service_confirm_description_key(&request.action))
            .replace("{{name}}", &request.description)
            .replace("{{id}}", &request.service_id);
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                tokens,
                "host-service-confirm-motion",
                phase,
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
                        .child(i18n.t("sidebar.host_services.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(i18n.t(host_service_confirm_label_key(&request.action)))
                        .into_any_element(),
                },
                focused_action,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.begin_service_confirm_exit(exit_delay, cx);
                }),
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.confirm_service_action_from_ui(exit_delay, cx);
                }),
            )
            .into_any_element(),
        )
    }

    fn confirm_service_action_from_ui(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ServiceConnectionMissing,
            ));
            return;
        };
        for notice in self.confirm_service_action(delay, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub fn render_host_service_logs_dialog(
        &self,
        follow_terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.service_logs_dialog()?;
        let theme = tokens.ui;
        let follow_logs_disabled = !follow_terminal_available
            || self
                .service_follow_logs_command(
                    &dialog.request.connection_id,
                    &dialog.request.service_id,
                )
                .is_err();
        let follow_connection_id = dialog.request.connection_id.clone();
        let follow_service_id = dialog.request.service_id.clone();
        let follow_description = dialog.request.description.clone();
        let follow_title = i18n
            .t("sidebar.host_services.follow_title")
            .replace("{{name}}", &follow_service_id);
        let follow_opened_notice = i18n
            .t("sidebar.host_services.toast.follow_opened")
            .replace("{{name}}", &follow_description);
        let follow_missing_notice = i18n.t("sidebar.host_services.toast.exec_terminal_missing");
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(i18n.t("sidebar.host_services.logs.loading"))
                .into_any_element()
        } else if let Some(error) = dialog.error.as_ref() {
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element()
        } else {
            let output = dialog.output.clone().unwrap_or_default();
            // The zeroizing Arc remains Entity-owned while rows borrow its text.
            let mut lines = div()
                .p_3()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .font_family(mono_font_family)
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
                    cx.listener(|host_tools, _event, _window, cx| {
                        host_tools.dismiss_service_logs_dialog(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(tokens)
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
                                                .child(
                                                    i18n
                                                        .t("sidebar.host_services.logs.title")
                                                        .replace(
                                                            "{{name}}",
                                                            &dialog.request.service_id,
                                                        ),
                                                ),
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
                                        .child(host_tools_tooltip_icon_button(
                                            tokens,
                                            "lucide/activity.svg",
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
                                            i18n.t(
                                                "sidebar.host_services.actions.follow_logs",
                                            ),
                                            "host-service-logs-follow",
                                            true,
                                            cx.listener(
                                                move |host_tools, _event, window, cx| {
                                                    host_tools.dismiss_service_logs_dialog(cx);
                                                    host_tools
                                                        .dispatch_service_follow_logs_from_ui(
                                                            follow_connection_id.clone(),
                                                            follow_service_id.clone(),
                                                            follow_title.clone(),
                                                            follow_opened_notice.clone(),
                                                            follow_missing_notice.clone(),
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
                                            i18n.t("sidebar.host_services.logs.close"),
                                            "host-service-logs-close",
                                            true,
                                            cx.listener(
                                                |host_tools, _event, _window, cx| {
                                                    host_tools
                                                        .dismiss_service_logs_dialog(cx);
                                                    cx.stop_propagation();
                                                },
                                            ),
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

    /// Sample-only bridges can inspect and expand rows without offering native host operations.
    pub fn render_sampled_service_list(
        &self,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.monitor_connections();
        let selected = self.selected_connection_id_owned();
        let Some(host) = connections
            .iter()
            .find(|c| Some(c.connection_id.as_str()) == selected.as_deref())
            .or_else(|| connections.first())
        else {
            return host_tools_center_state(
                "lucide/wifi-off.svg",
                render.tokens.ui.text_muted,
                render.i18n.t("profiler.panel.no_connection"),
                &render.selectable_text,
                cx,
            );
        };
        let id = host.connection_id.as_str();
        let metrics = self.profiler_current(id).and_then(|(metrics, _)| metrics);
        let loading = metrics.is_none();
        let services = metrics.map(|m| m.services).unwrap_or_default();
        let rows = visible_service_rows(&services.services, "");
        self.sync_host_service_list_state(&rows, id);
        self.render_host_service_list(
            rows,
            loading,
            services.status,
            id,
            RowActions::Inspect,
            render.sidebar_width,
            &render.tokens,
            &render.i18n,
            render.mono_font_family.clone(),
            &render.selectable_text,
            cx,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_host_services_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        follow_terminal_available: bool,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return host_tools_center_state(
                "lucide/wifi-off.svg",
                tokens.ui.text_muted,
                i18n.t("profiler.panel.no_connection"),
                selectable_text,
                cx,
            );
        }

        let selected_connection_id = self.selected_connection_id_owned();
        let selected_id = selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let snapshot = self.service_snapshot_for(selected_id);
        let rows = snapshot
            .as_ref()
            .map(|snapshot| {
                visible_service_rows(&snapshot.services, &self.ui.host_service_search_query)
            })
            .unwrap_or_default();
        let service_status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_service_list_state(&rows, selected_id);
        let snapshot_in_flight = self.service_snapshot_in_flight();

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
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &connections,
                        selected_id,
                        !snapshot_in_flight,
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_service_search(&search_ime, tokens, i18n, cx))
                    .child(self.render_host_service_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        service_status.clone(),
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(self.render_host_service_list(
                rows,
                snapshot_in_flight,
                service_status,
                selected_id,
                RowActions::Manage {
                    terminal_available: follow_terminal_available,
                },
                sidebar_width,
                tokens,
                i18n,
                mono_font_family,
                selectable_text,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_service_search(
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
                    value: &self.ui.host_service_search_query,
                    placeholder: i18n.t("sidebar.host_services.search_placeholder"),
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
                    // The Entity keeps input state while the root owns window IME selection.
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

    fn render_host_service_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceServiceStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match status {
            ResourceServiceStatus::Available {
                capability: ServiceCommandCapability::Full,
                ..
            } => i18n.t("sidebar.host_services.capability.full"),
            ResourceServiceStatus::Available {
                capability: ServiceCommandCapability::Partial,
                ..
            } => i18n.t("sidebar.host_services.capability.partial"),
            _ => i18n.t("sidebar.host_services.capability.unknown"),
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().min_w_0().flex_1().truncate().child(format!(
                "{} {} · {}",
                visible_count,
                i18n.t("sidebar.host_services.count_suffix"),
                capability_label
            )))
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
                    disabled: self.service_snapshot_in_flight(),
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                },
                i18n.t("sidebar.host_services.actions.refresh"),
                "host-service-refresh",
                true,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.request_service_snapshot_from_ui(selected_id.clone(), cx);
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_service_list(
        &self,
        rows: Vec<ResourceService>,
        loading: bool,
        status: ResourceServiceStatus,
        selected_id: &str,
        actions: RowActions,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                "lucide/wrench.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_services.sampling"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceServiceStatus::Unavailable => {
                return host_tools_center_state(
                    "lucide/wrench.svg",
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_services.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceServiceStatus::Error { message } => {
                return host_tools_center_state(
                    "lucide/alert-triangle.svg",
                    MONITOR_RED,
                    i18n.t("sidebar.host_services.error")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourceServiceStatus::Unknown | ResourceServiceStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                "lucide/wrench.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_services.empty"),
                selectable_text,
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.ui.host_service_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let show_context_columns = sidebar_width >= HOST_SERVICE_CONTEXT_COLUMNS_MIN_WIDTH;
        let row_tokens = *tokens;
        // I18n clones only the Arc-backed catalog handle for the 'static row factory.
        let row_i18n = i18n.clone();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(Self::render_host_service_table_header(
                show_context_columns,
                tokens,
                i18n,
            ))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_service_row(
                                    selected_id.as_str(),
                                    rows.get(index).cloned(),
                                    actions,
                                    show_context_columns,
                                    &row_tokens,
                                    &row_i18n,
                                    mono_font_family.clone(),
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_service_table_header(
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
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
                    .child(i18n.t("sidebar.host_services.columns.service")),
            )
            .child(
                div()
                    .flex_none()
                    .when(show_context_columns, |col| {
                        col.w(px(HOST_SERVICE_STATE_COLUMN_WIDTH))
                    })
                    .when(!show_context_columns, |col| col.flex().justify_end())
                    .child(i18n.t("sidebar.host_services.columns.state")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_ENABLED_COLUMN_WIDTH))
                            .child(i18n.t("sidebar.host_services.columns.enabled")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_PID_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .child(i18n.t("sidebar.host_services.columns.pid")),
                    )
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_service_row(
        &self,
        connection_id: &str,
        service: Option<ResourceService>,
        actions: RowActions,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(service) = service else {
            return div().into_any_element();
        };
        let expanded = self.ui.host_service_expanded_id.as_deref() == Some(service.id.as_str());
        let theme = tokens.ui;
        let state_label = i18n.t(service_state_label_key(&service.active_state));
        let enabled_label = i18n.t(service_enabled_label_key(&service.enabled_state));
        let main_pid = service.main_pid.clone().unwrap_or_else(|| "—".to_string());
        let state_color = service_state_color(&service.active_state, theme.text_muted);

        div()
            .id(host_tools_row_id(
                "host-service-row",
                connection_id,
                service.id.clone(),
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
                    .h(px(HOST_SERVICE_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Keep service identity above secondary metadata under compression.
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
                            .child(service.id.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .when(show_context_columns, |col| {
                                col.w(px(HOST_SERVICE_STATE_COLUMN_WIDTH))
                            })
                            .when(!show_context_columns, |col| col.max_w(px(110.0)))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(state_color))
                            .font_family(mono_font_family.clone())
                            .child(state_label),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_SERVICE_ENABLED_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font_family.clone())
                                .child(enabled_label.clone()),
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
                                .font_family(mono_font_family.clone())
                                .child(main_pid.clone()),
                        )
                    }),
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
                            .child(if show_context_columns {
                                format!("{} · {}", service.sub_state, service.description)
                            } else {
                                let mut parts = vec![enabled_label];
                                if main_pid != "—" {
                                    parts.push(format!("#{main_pid}"));
                                }
                                if !service.sub_state.trim().is_empty() && service.sub_state != "-"
                                {
                                    parts.push(service.sub_state.clone());
                                }
                                if !service.description.trim().is_empty()
                                    && service.description != "-"
                                {
                                    parts.push(service.description.clone());
                                }
                                parts.join(" · ")
                            }),
                    )
                    .when_some(actions.terminal_access(), |row, terminal_available| {
                        row.child(self.render_host_service_inline_actions(
                            connection_id,
                            &service,
                            terminal_available,
                            tokens,
                            i18n,
                            cx,
                        ))
                    }),
            )
            .when(expanded, |row| {
                row.child(Self::render_host_service_detail(
                    &service,
                    tokens,
                    i18n,
                    mono_font_family,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = service.id.clone();
                    move |host_tools, _event, _window, cx| {
                        let expanded_id = &mut host_tools.ui.host_service_expanded_id;
                        if expanded_id.as_deref() == Some(id.as_str()) {
                            *expanded_id = None;
                        } else {
                            *expanded_id = Some(id.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    fn render_host_service_inline_actions(
        &self,
        connection_id: &str,
        service: &ResourceService,
        follow_terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self.service_action_running_for(&service.id);
        let availability = service_action_availability(service);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.render_host_service_logs_button(
                connection_id,
                service,
                is_running,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_service_follow_logs_button(
                connection_id,
                service,
                is_running || !follow_terminal_available,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Start,
                "lucide/play.svg",
                "sidebar.host_services.actions.start",
                false,
                is_running || !availability.can_start,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Stop,
                "lucide/square.svg",
                "sidebar.host_services.actions.stop",
                true,
                is_running || !availability.can_stop,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Restart,
                "lucide/refresh-cw.svg",
                "sidebar.host_services.actions.restart",
                true,
                is_running || !availability.can_restart,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Reload,
                "lucide/refresh-ccw.svg",
                "sidebar.host_services.actions.reload",
                false,
                is_running || !availability.can_reload,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Enable,
                "lucide/check-circle.svg",
                "sidebar.host_services.actions.enable",
                false,
                is_running || !availability.can_enable,
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Disable,
                "lucide/shield-off.svg",
                "sidebar.host_services.actions.disable",
                true,
                is_running || !availability.can_disable,
                tokens,
                i18n,
                cx,
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_service_action_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        action: ServiceActionKind,
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
            !self.service_action_supported(connection_id, &service.id, action.clone());
        let disabled = disabled || unsupported;
        let icon_color = if danger { MONITOR_RED } else { theme.text };
        host_tools_tooltip_icon_button(
            tokens,
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
            i18n.t(label_key),
            "host-service-action",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let service_id = service.id.clone();
                let description = service.description.clone();
                move |host_tools, _event, _window, cx| {
                    host_tools.request_service_action_from_ui(
                        connection_id.clone(),
                        service_id.clone(),
                        description.clone(),
                        action.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
        )
    }

    fn render_host_service_logs_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        disabled: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let unsupported = !self.service_logs_supported(connection_id, &service.id);
        let failure_fallback = i18n.t("sidebar.host_services.toast.logs_failed");
        let empty_fallback = i18n.t("sidebar.host_services.logs.empty");
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
            i18n.t("sidebar.host_services.actions.logs"),
            "host-service-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let service_id = service.id.clone();
                let description = service.description.clone();
                move |host_tools, _event, _window, cx| {
                    host_tools.request_service_logs_from_ui(
                        connection_id.clone(),
                        service_id.clone(),
                        description.clone(),
                        failure_fallback.clone(),
                        empty_fallback.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
        )
    }

    fn render_host_service_follow_logs_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        disabled: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let connection_id = connection_id.to_string();
        let service_id = service.id.clone();
        let title = i18n
            .t("sidebar.host_services.follow_title")
            .replace("{{name}}", &service_id);
        let opened_notice = i18n
            .t("sidebar.host_services.toast.follow_opened")
            .replace("{{name}}", &service.description);
        let missing_notice = i18n.t("sidebar.host_services.toast.exec_terminal_missing");
        host_tools_tooltip_icon_button(
            tokens,
            "lucide/activity.svg",
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            i18n.t("sidebar.host_services.actions.follow_logs"),
            "host-service-follow-logs",
            true,
            cx.listener(move |host_tools, _event, window, cx| {
                host_tools.dispatch_service_follow_logs_from_ui(
                    connection_id.clone(),
                    service_id.clone(),
                    title.clone(),
                    opened_notice.clone(),
                    missing_notice.clone(),
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
    }

    fn render_host_service_detail(
        service: &ResourceService,
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
            .child(Self::render_host_service_detail_line(
                i18n.t("sidebar.host_services.columns.description"),
                service.description.clone(),
            ))
            .child(Self::render_host_service_detail_line(
                i18n.t("sidebar.host_services.columns.load"),
                service.load_state.clone(),
            ))
            .child(Self::render_host_service_detail_line(
                i18n.t("sidebar.host_services.columns.sub_state"),
                service.sub_state.clone(),
            ))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font_family)
                    .text_color(rgb(theme.text))
                    .child(service.id.clone()),
            )
            .into_any_element()
    }

    fn render_host_service_detail_line(label: String, value: String) -> AnyElement {
        div()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_none().w(px(92.0)).child(label))
            .child(div().min_w_0().flex_1().truncate().child(value))
            .into_any_element()
    }

    fn sync_host_service_list_state(&self, rows: &[ResourceService], selected_id: &str) {
        let signatures = rows.iter().map(service_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-services:{selected_id}:{}:{}",
            self.ui.host_service_search_query,
            self.ui
                .host_service_expanded_id
                .as_deref()
                .unwrap_or_default()
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.ui.host_service_list_state,
            &mut self.ui.host_service_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
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
