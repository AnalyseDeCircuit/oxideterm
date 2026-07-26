//! Owns the services Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::{
    ResourceServiceSnapshot, build_service_snapshot_command, parse_service_snapshot,
    service_action_availability,
};

impl WorkspaceApp {
    pub(super) fn render_host_services_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = self.i18n.clone();
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.selectable_text_render_state(cx);
        let search = self.render_host_service_search(cx);
        let connections = self.monitor_connections(cx);
        let selected_connection_id = self.host_tools.read(cx).selected_connection_id_owned();
        let selected_connection_id = selected_connection_id.as_deref().or_else(|| {
            connections
                .first()
                .map(|connection| connection.connection_id.as_str())
        });
        let follow_terminal_available = selected_connection_id
            .and_then(|connection_id| self.node_router.node_id_for_connection(connection_id))
            .is_some_and(|node_id| self.ssh_nodes.contains_key(&node_id));
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_services_panel(
                search,
                follow_terminal_available,
                &tokens,
                &i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(super) fn render_host_service_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostServiceSearch;
        let (focused, value) = {
            let ui = &self.host_tools.read(cx).ui;
            (
                ui.input_is_focused(HostToolsTextInput::ServiceSearch),
                ui.host_service_search_query.clone(),
            )
        };
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder: self.i18n.t("sidebar.host_services.search_placeholder"),
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
                        host_tools.ui.focus_input(HostToolsTextInput::ServiceSearch);
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

    pub(super) fn request_host_services_snapshot_for_selected_connection(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let connections = self.monitor_connections(cx);
        let Some(connection_id) = self
            .host_tools
            .read(cx)
            .selected_connection_id_owned()
            .or_else(|| {
                connections
                    .first()
                    .map(|connection| connection.connection_id.clone())
            })
        else {
            return;
        };
        self.request_host_service_snapshot(connection_id, cx);
    }

    pub(in crate::workspace) fn request_host_service_snapshot(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.host_tool_monitoring_enabled(ContextSidebarTool::Services)
            || !self.host_tools_surface_visible()
            || self.host_tools.read(cx).active_tool() != ContextSidebarTool::Services
        {
            return;
        }
        let runtime = self.forwarding_runtime.handle().clone();
        let connection_fallback = self
            .i18n
            .t("sidebar.host_services.toast.connection_missing");
        let failure_fallback = self.i18n.t("sidebar.host_services.toast.action_failed");
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_service_snapshot(
                connection_id,
                runtime,
                connection_fallback,
                failure_fallback,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn handle_host_service_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::ServiceSearch)
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

    pub(in crate::workspace) fn handle_host_service_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).service_confirm_view().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_service_confirm_exit(cx);
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
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_service_action(delay, runtime, cx)
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_service_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_service_confirm_exit(delay, cx)
        })
    }

    pub(in crate::workspace) fn render_host_service_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tokens = self.tokens;
        let i18n = self.i18n.clone();
        let focused_action = self.standard_confirm_focus();
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            &tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_service_confirm_dialog(
                &tokens,
                &i18n,
                focused_action,
                exit_delay,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn render_host_service_logs_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.host_tools.read(cx).service_logs_dialog()?;
        let follow_terminal_available = self
            .node_router
            .node_id_for_connection(&dialog.request.connection_id)
            .is_some_and(|node_id| self.ssh_nodes.contains_key(&node_id));
        let tokens = self.tokens;
        let i18n = self.i18n.clone();
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_service_logs_dialog(
                follow_terminal_available,
                &tokens,
                &i18n,
                mono_font_family,
                cx,
            )
        })
    }
}

impl HostToolsEntity {
    pub(in crate::workspace::connection_monitor) fn render_host_service_confirm_dialog(
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

    pub(in crate::workspace::connection_monitor) fn render_host_service_logs_dialog(
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

    pub(in crate::workspace::connection_monitor) fn render_host_services_panel(
        &self,
        search: AnyElement,
        follow_terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return host_tools_center_state(
                LucideIcon::WifiOff,
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
        let snapshot_polling = self.service_snapshot_polling();

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
                        !snapshot_polling,
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    // IME composition and anchor tracking remain a workspace service.
                    .child(search)
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
                snapshot_polling,
                service_status,
                selected_id,
                follow_terminal_available,
                tokens,
                i18n,
                mono_font_family,
                selectable_text,
                cx,
            ))
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
            .child(div().flex_none().child(format!(
                "{} {} · {}",
                visible_count,
                i18n.t("sidebar.host_services.count_suffix"),
                capability_label
            )))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::RefreshCw,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 24.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    disabled: self.service_snapshot_polling(),
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
        follow_terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::Wrench,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_services.sampling"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceServiceStatus::Unavailable => {
                return host_tools_center_state(
                    LucideIcon::Wrench,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_services.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceServiceStatus::Error { message } => {
                return host_tools_center_state(
                    LucideIcon::AlertTriangle,
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
                LucideIcon::Wrench,
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
            .child(Self::render_host_service_table_header(tokens, i18n))
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
                                    follow_terminal_available,
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

    fn render_host_service_table_header(tokens: &ThemeTokens, i18n: &I18n) -> AnyElement {
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
                    .w(px(HOST_SERVICE_STATE_COLUMN_WIDTH))
                    .child(i18n.t("sidebar.host_services.columns.state")),
            )
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
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_service_row(
        &self,
        connection_id: &str,
        service: Option<ResourceService>,
        follow_terminal_available: bool,
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
                            .w(px(HOST_SERVICE_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(state_color))
                            .font_family(mono_font_family.clone())
                            .child(state_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_ENABLED_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
                            .child(format!("{} · {}", service.sub_state, service.description)),
                    )
                    .child(self.render_host_service_inline_actions(
                        connection_id,
                        &service,
                        follow_terminal_available,
                        tokens,
                        i18n,
                        cx,
                    )),
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
                LucideIcon::Play,
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
                LucideIcon::Square,
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
                LucideIcon::RefreshCw,
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
                LucideIcon::RefreshCcw,
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
                LucideIcon::CheckCircle,
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
                LucideIcon::ShieldOff,
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
        icon: LucideIcon,
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
            LucideIcon::Activity,
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

    fn request_service_snapshot_from_ui(&mut self, connection_id: String, cx: &mut Context<Self>) {
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

    fn request_service_action_from_ui(
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

    fn request_service_logs_from_ui(
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

    fn dispatch_service_follow_logs_from_ui(
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

    pub(super) fn service_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<ResourceServiceSnapshot> {
        (self.host_services.snapshot_connection_id.as_deref() == Some(connection_id))
            .then(|| self.host_services.snapshot.clone())
            .flatten()
    }

    pub(super) fn service_snapshot_polling(&self) -> bool {
        self.host_services.snapshot_polling
    }

    pub(in crate::workspace::connection_monitor) fn pause_service_refreshes(&mut self) {
        // A bounded capture already in flight may finish, while coalesced
        // page-only refresh work is discarded when Services is hidden.
        self.host_services.snapshot_pending = None;
    }

    pub(super) fn service_action_running_for(&self, service_id: &str) -> bool {
        self.host_services
            .action_running
            .as_ref()
            .is_some_and(|request| request.service_id == service_id)
    }

    pub(super) fn service_action_supported(
        &self,
        connection_id: &str,
        service_id: &str,
        action: ServiceActionKind,
    ) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_service_action_command(&os_type, service_id, action).ok())
            .is_some()
    }

    pub(super) fn service_logs_supported(&self, connection_id: &str, service_id: &str) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_service_logs_command(&os_type, service_id).ok())
            .is_some()
    }

    pub(super) fn service_follow_logs_command(
        &self,
        connection_id: &str,
        service_id: &str,
    ) -> Result<oxideterm_connection_monitor::ServiceCaptureCommand, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_service_follow_logs_command(&os_type, service_id)
    }

    pub(in crate::workspace::connection_monitor) fn request_service_snapshot(
        &mut self,
        connection_id: String,
        runtime: tokio::runtime::Handle,
        connection_fallback: String,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) {
        let request = HostServiceSnapshotRequest {
            connection_id,
            connection_fallback,
            failure_fallback,
        };
        if self.host_services.snapshot_polling {
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
        runtime: tokio::runtime::Handle,
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
        self.host_services.snapshot_polling = true;
        let spawned = self.spawn_service_snapshot_capture(
            command.command,
            request.clone(),
            HOST_SERVICE_SNAPSHOT_TIMEOUT,
            HOST_SERVICE_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_services.snapshot_running = None;
            self.host_services.snapshot_polling = false;
            self.host_services.snapshot = Some(ResourceServiceSnapshot {
                status: ResourceServiceStatus::Error {
                    message: request.connection_fallback,
                },
                services: Vec::new(),
            });
        }
        cx.notify();
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_service_snapshot(
        &mut self,
        delivery: HostServiceSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_services.snapshot_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_services.snapshot_polling = false;
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

    pub(in crate::workspace::connection_monitor) fn open_service_action_confirm(
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

    pub(in crate::workspace::connection_monitor) fn service_confirm_view(
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
    pub(in crate::workspace::connection_monitor) fn dismiss_service_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.host_services.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn begin_service_confirm_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
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
            Timer::after(delay).await;
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

    pub(super) fn confirm_service_action(
        &mut self,
        delay: Duration,
        runtime: tokio::runtime::Handle,
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

    fn start_service_action(
        &mut self,
        request: HostServiceActionRequest,
        runtime: tokio::runtime::Handle,
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

    pub(in crate::workspace::connection_monitor) fn finish_host_service_action(
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
            || !self.visibility.is_visible()
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

    pub(super) fn request_service_logs(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        runtime: tokio::runtime::Handle,
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

    pub(super) fn service_logs_dialog(&self) -> Option<HostServiceLogsDialog> {
        self.host_services.logs_dialog.clone()
    }

    pub(super) fn dismiss_service_logs_dialog(&mut self, cx: &mut Context<Self>) {
        if self.host_services.logs_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_service_logs(
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
