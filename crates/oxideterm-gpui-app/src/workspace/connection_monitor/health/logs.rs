//! Owns the logs Host Tool UI and request lifecycle.

use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_logs_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let snapshot = self.host_tools.read(cx).log_snapshot_for(selected_id);
        let preset = self.host_tools.read(cx).log_preset();
        let rows = snapshot
            .as_ref()
            .map(|snapshot| {
                visible_log_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_log_search_query,
                    preset,
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_log_list_state(&rows, selected_id, cx);

        div()
            .id("host-logs-panel")
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
                        !self.host_tools.read(cx).log_snapshot_polling(),
                        cx,
                    ))
                    .child(self.render_host_log_search(cx))
                    .child(self.render_host_log_preset_row(cx))
                    .child(self.render_host_log_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        cx,
                    )),
            )
            .child(self.render_host_log_list(
                rows,
                self.host_tools.read(cx).log_snapshot_polling(),
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_log_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostLogSearch;
        let focused = self.connection_monitor.host_log_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_log_search_query,
                    placeholder: self.i18n.t("sidebar.host_logs.search_placeholder"),
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
                    this.connection_monitor.host_log_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
                    this.connection_monitor.host_tmux_search_focused = false;
                    this.connection_monitor.host_port_search_focused = false;
                    this.connection_monitor.host_schedule_search_focused = false;
                    this.connection_monitor.host_filesystem_search_focused = false;
                    this.connection_monitor.host_package_search_focused = false;
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

    pub(super) fn render_host_log_preset_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .id("host-log-preset-scroll")
            .flex()
            .items_center()
            .gap_1()
            .overflow_x_scroll();
        for preset in [
            LogPreset::All,
            LogPreset::Errors,
            LogPreset::Auth,
            LogPreset::Kernel,
            LogPreset::System,
        ] {
            row = row.child(self.render_host_log_preset_chip(preset, cx));
        }
        row.into_any_element()
    }

    pub(super) fn render_host_log_preset_chip(
        &self,
        preset: LogPreset,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.host_tools.read(cx).log_preset() == preset;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(log_preset_label_key(preset)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let host_tools = this.host_tools.clone();
                    if host_tools.update(cx, |host_tools, cx| {
                        host_tools.select_log_preset(preset, cx)
                    }) {
                        this.request_host_logs_snapshot_for_selected_connection(cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_log_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceLogStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let capability_label = match status {
            ResourceLogStatus::Available {
                capability: LogCommandCapability::Full,
                ..
            } => self.i18n.t("sidebar.host_logs.capability.full"),
            ResourceLogStatus::Available {
                capability: LogCommandCapability::Partial,
                ..
            } => self.i18n.t("sidebar.host_logs.capability.partial"),
            _ => self.i18n.t("sidebar.host_logs.capability.unknown"),
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
                self.i18n.t("sidebar.host_logs.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::Activity,
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
                        self.i18n.t("sidebar.host_logs.actions.follow"),
                        "host-log-follow",
                        true,
                        cx.listener({
                            let selected_id = selected_id.clone();
                            move |this, _event, window, cx| {
                                this.open_host_logs_follow_terminal(
                                    selected_id.clone(),
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                        cx.entity(),
                    ))
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::RefreshCw,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.host_tools.read(cx).log_snapshot_polling(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        self.i18n.t("sidebar.host_logs.actions.refresh"),
                        "host-log-refresh",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.request_host_logs_snapshot(
                                selected_id.clone(),
                                HostSnapshotFeedback::Toast,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                        cx.entity(),
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_host_log_list(
        &self,
        rows: Vec<ResourceLogEntry>,
        loading: bool,
        status: ResourceLogStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::FileText,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_logs.loading"),
                cx,
            );
        }
        match status {
            ResourceLogStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::FileText,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_logs.unavailable"),
                    cx,
                );
            }
            ResourceLogStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_logs.error", &[("error", message)]),
                    cx,
                );
            }
            ResourceLogStatus::Unknown | ResourceLogStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::FileText,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_logs.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.host_tools.read(cx).log_list_state();
        let spec = TauriVirtualListSpec::new(px(HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        let show_context_columns = self.ai.chat.sidebar_width >= HOST_LOG_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_log_table_header(show_context_columns))
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
                                this.render_host_log_row(
                                    selected_id.as_str(),
                                    index,
                                    rows.get(index).cloned(),
                                    show_context_columns,
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_host_log_table_header(&self, show_context_columns: bool) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_LOG_TABLE_HEADER_HEIGHT))
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
                    .flex_none()
                    .w(px(HOST_LOG_TIME_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_logs.columns.time")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_LOG_LEVEL_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_logs.columns.level")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_LOG_SOURCE_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_logs.columns.source")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_LOG_UNIT_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_logs.columns.unit")),
                    )
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(self.i18n.t("sidebar.host_logs.columns.message")),
            )
            .into_any_element()
    }

    pub(super) fn render_host_log_row(
        &self,
        _connection_id: &str,
        index: usize,
        entry: Option<ResourceLogEntry>,
        show_context_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.host_tools.read(cx).log_expanded_index() == Some(index);
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let level_label = self.i18n.t(log_level_label_key(&entry.level));
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
                    .h(px(HOST_PROCESS_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_LOG_TIME_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(host_log_timestamp_label(&entry.timestamp)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_LOG_LEVEL_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(log_level_color(&entry.level, theme.text_muted)))
                            .font_family(mono_font.clone())
                            .child(level_label),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_LOG_SOURCE_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(host_log_blank_dash(&entry.source)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_LOG_UNIT_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(host_log_blank_dash(&entry.unit)),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(entry.message.clone()),
                    ),
            )
            .when(!show_context_columns, |row| {
                row.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .px_3()
                        .pb_2()
                        .truncate()
                        .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                        .text_color(rgb(theme.text_muted))
                        .font_family(mono_font.clone())
                        .child(format!(
                            "{} · {}",
                            host_log_blank_dash(&entry.source),
                            host_log_blank_dash(&entry.unit)
                        )),
                )
            })
            .when(expanded, |row| {
                row.child(self.render_host_log_detail(&entry))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.toggle_log_expanded(index, cx);
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_log_detail(&self, entry: &ResourceLogEntry) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
            .mx_3()
            .mb_2()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .overflow_x_scrollbar()
            .child(
                div()
                    .p_3()
                    .min_w(px(520.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .font_family(mono_font)
                    .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_logs.columns.time"),
                        host_log_blank_dash(&entry.timestamp)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_logs.columns.source"),
                        host_log_blank_dash(&entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_logs.columns.unit"),
                        host_log_blank_dash(&entry.unit)
                    ))
                    .child(
                        div()
                            .pt_2()
                            .whitespace_nowrap()
                            .child(entry.message.clone()),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn sync_host_log_list_state(
        &self,
        rows: &[ResourceLogEntry],
        selected_id: &str,
        cx: &mut Context<Self>,
    ) {
        let signatures = rows.iter().map(log_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-logs:{selected_id}:{}:{}:{}",
            self.connection_monitor.host_log_search_query,
            self.host_tools.read(cx).log_preset() as u8,
            self.host_tools
                .read(cx)
                .log_expanded_index()
                .unwrap_or(usize::MAX)
        );
        self.host_tools
            .read(cx)
            .sync_log_list_signatures(&identity, &signatures);
    }

    pub(in crate::workspace) fn handle_host_log_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_log_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_log_search_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn request_host_logs_snapshot_for_selected_connection(
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
        self.request_host_logs_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_host_logs_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        let monitoring_enabled = self.host_tool_monitoring_enabled(ContextSidebarTool::Logs);
        let runtime = self.forwarding_runtime.handle().clone();
        let failure_fallback = self.i18n.t("sidebar.host_logs.toast.unknown_error");
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_log_snapshot(
                connection_id,
                feedback,
                monitoring_enabled,
                runtime,
                failure_fallback,
                cx,
            )
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    pub(super) fn open_host_logs_follow_terminal(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preset = self.host_tools.read(cx).log_preset();
        let (command, os_type) = match self
            .host_tools
            .read(cx)
            .prepare_log_follow_command(&connection_id)
        {
            Ok(command) => command,
            Err(error) => {
                self.push_host_log_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == LogCommandCapability::Partial {
            self.push_host_log_toast(
                self.i18n_replace(
                    "sidebar.host_logs.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            );
        }
        let preset_label = self.i18n.t(log_preset_label_key(preset));
        let title = self.i18n_replace(
            "sidebar.host_logs.follow_title",
            &[("preset", preset_label.clone())],
        );
        // Follow mode belongs in a visible terminal so Ctrl-C and terminal
        // lifecycle semantics stop the log stream without fake UI streaming.
        self.open_host_log_terminal_command(
            connection_id,
            preset_label,
            command.command,
            title,
            window,
            cx,
        );
    }

    pub(super) fn open_host_log_terminal_command(
        &mut self,
        connection_id: String,
        preset_label: String,
        command: String,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_log_toast(
                self.i18n.t("sidebar.host_logs.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        if !self.ssh_nodes.contains_key(&node_id) {
            self.push_host_log_toast(
                self.i18n.t("sidebar.host_logs.toast.exec_terminal_missing"),
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
            Ok(()) => self.push_host_log_toast(
                self.i18n_replace(
                    "sidebar.host_logs.toast.follow_opened",
                    &[("preset", preset_label)],
                ),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => self.push_host_log_toast(error.to_string(), TerminalNoticeVariant::Error),
        }
        cx.notify();
    }

    pub(super) fn push_host_log_toast(&mut self, message: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    pub(in crate::workspace) fn push_host_tools_notice(&mut self, notice: HostToolsNotice) {
        let (message, variant) = match notice {
            HostToolsNotice::ProcessActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_processes.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ProcessInvalidNice => (
                self.i18n.t("sidebar.host_processes.toast.invalid_nice"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ProcessConnectionMissing => (
                self.i18n
                    .t("sidebar.host_processes.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ProcessPartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_processes.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ProcessActionFailed => (
                self.i18n.t("sidebar.host_processes.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ProcessActionFinished { pid, succeeded } => {
                if succeeded {
                    (
                        self.i18n_replace(
                            "sidebar.host_processes.toast.action_succeeded",
                            &[("pid", pid)],
                        ),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_processes.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
            HostToolsNotice::DockerActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_docker.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::DockerLogsAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_docker.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::DockerConnectionMissing => (
                self.i18n.t("sidebar.host_docker.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::DockerActionFailed => (
                self.i18n.t("sidebar.host_docker.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::DockerLogsFailed => (
                self.i18n.t("sidebar.host_docker.toast.logs_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::DockerActionFinished {
                container_name,
                succeeded,
            } => {
                if succeeded {
                    (
                        self.i18n_replace(
                            "sidebar.host_docker.toast.action_succeeded",
                            &[("name", container_name)],
                        ),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_docker.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
            HostToolsNotice::ServiceActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_services.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ServiceLogsAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_services.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ServiceConnectionMissing => (
                self.i18n
                    .t("sidebar.host_services.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ServicePartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_services.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ServiceActionFailed => (
                self.i18n.t("sidebar.host_services.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ServiceLogsFailed => (
                self.i18n.t("sidebar.host_services.toast.logs_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ServiceActionFinished {
                description,
                succeeded,
            } => {
                if succeeded {
                    (
                        self.i18n_replace(
                            "sidebar.host_services.toast.action_succeeded",
                            &[("name", description)],
                        ),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_services.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
            HostToolsNotice::TmuxSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_tmux.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::TmuxConnectionMissing => (
                self.i18n.t("sidebar.host_tmux.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::TmuxSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_tmux.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::TmuxUnavailable => (
                self.i18n.t("sidebar.host_tmux.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::TmuxSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_tmux.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_tmux.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::TmuxActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_tmux.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::TmuxInputRequired => (
                self.i18n.t("sidebar.host_tmux.toast.input_required"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::TmuxActionFailed => (
                self.i18n.t("sidebar.host_tmux.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::TmuxActionFinished {
                target_label,
                succeeded,
            } => {
                if succeeded {
                    (
                        self.i18n_replace(
                            "sidebar.host_tmux.toast.action_succeeded",
                            &[("target", target_label)],
                        ),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_tmux.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
            HostToolsNotice::LogSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_logs.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::LogConnectionMissing => (
                self.i18n.t("sidebar.host_logs.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::LogPartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_logs.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::LogSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_logs.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::LogUnavailable => (
                self.i18n.t("sidebar.host_logs.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::LogSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_logs.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_logs.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PortSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_ports.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PortConnectionMissing => (
                self.i18n.t("sidebar.host_ports.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PortPartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_ports.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PortSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_ports.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::PortUnavailable => (
                self.i18n.t("sidebar.host_ports.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PortSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_ports.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_ports.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::FilesystemSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_filesystems.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::FilesystemConnectionMissing => (
                self.i18n
                    .t("sidebar.host_filesystems.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::FilesystemPartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_filesystems.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::FilesystemSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_filesystems.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::FilesystemUnavailable => (
                self.i18n.t("sidebar.host_filesystems.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::FilesystemSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_filesystems.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_filesystems.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PackageSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_packages.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PackageConnectionMissing => (
                self.i18n
                    .t("sidebar.host_packages.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::PackageSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_packages.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::PackageUnavailable => (
                self.i18n.t("sidebar.host_packages.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::PackageSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_packages.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_packages.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ScheduleSnapshotAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_schedules.toast.snapshot_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleConnectionMissing => (
                self.i18n
                    .t("sidebar.host_schedules.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::SchedulePartialSupport { os_type } => (
                self.i18n_replace(
                    "sidebar.host_schedules.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleSnapshotLoaded { count } => (
                self.i18n_replace(
                    "sidebar.host_schedules.toast.snapshot_loaded",
                    &[("count", count.to_string())],
                ),
                TerminalNoticeVariant::Success,
            ),
            HostToolsNotice::ScheduleUnavailable => (
                self.i18n.t("sidebar.host_schedules.toast.unavailable"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleSnapshotFailed => (
                self.i18n_replace(
                    "sidebar.host_schedules.toast.snapshot_failed",
                    &[(
                        "reason",
                        self.i18n.t("sidebar.host_schedules.toast.unknown_error"),
                    )],
                ),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ScheduleLogsAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_schedules.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleLogsFailed => (
                self.i18n.t("sidebar.host_schedules.toast.logs_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ScheduleActionAlreadyRunning => (
                self.i18n
                    .t("sidebar.host_schedules.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            ),
            HostToolsNotice::ScheduleActionFailed => (
                self.i18n.t("sidebar.host_schedules.toast.action_failed"),
                TerminalNoticeVariant::Error,
            ),
            HostToolsNotice::ScheduleActionFinished {
                kind,
                task_name,
                succeeded,
            } => {
                if succeeded {
                    let message_key = match kind {
                        ScheduleActionNoticeKind::RunNow => {
                            "sidebar.host_schedules.toast.run_now_started"
                        }
                        ScheduleActionNoticeKind::Enable => {
                            "sidebar.host_schedules.toast.enable_succeeded"
                        }
                        ScheduleActionNoticeKind::Disable => {
                            "sidebar.host_schedules.toast.disable_succeeded"
                        }
                    };
                    (
                        self.i18n_replace(message_key, &[("name", task_name)]),
                        TerminalNoticeVariant::Success,
                    )
                } else {
                    (
                        self.i18n.t("sidebar.host_schedules.toast.action_failed"),
                        TerminalNoticeVariant::Error,
                    )
                }
            }
        };
        self.push_host_log_toast(message, variant);
    }
}

impl HostToolsEntity {
    pub(super) fn log_snapshot_for(&self, connection_id: &str) -> Option<ResourceLogSnapshot> {
        self.host_logs
            .snapshot
            .as_ref()
            .filter(|_| self.host_logs.snapshot_connection_id.as_deref() == Some(connection_id))
            .cloned()
    }

    pub(super) fn log_preset(&self) -> LogPreset {
        self.host_logs.preset
    }

    pub(super) fn log_snapshot_polling(&self) -> bool {
        self.host_logs.polling
    }

    pub(super) fn log_list_state(&self) -> ListState {
        self.host_logs.list_state.clone()
    }

    pub(super) fn log_expanded_index(&self) -> Option<usize> {
        self.host_logs.expanded_index
    }

    pub(super) fn select_log_preset(&mut self, preset: LogPreset, cx: &mut Context<Self>) -> bool {
        if self.host_logs.preset == preset {
            return false;
        }
        self.host_logs.preset = preset;
        self.host_logs.expanded_index = None;
        cx.notify();
        true
    }

    pub(super) fn toggle_log_expanded(&mut self, index: usize, cx: &mut Context<Self>) {
        self.host_logs.expanded_index =
            (self.host_logs.expanded_index != Some(index)).then_some(index);
        cx.notify();
    }

    pub(in crate::workspace) fn clear_log_expanded(&mut self, cx: &mut Context<Self>) {
        if self.host_logs.expanded_index.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn sync_log_list_signatures(&self, identity: &str, signatures: &[u64]) {
        sync_tauri_variable_list_state_by_signatures(
            &self.host_logs.list_state,
            &mut self.host_logs.list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(px(HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn prepare_log_follow_command(
        &self,
        connection_id: &str,
    ) -> Result<(oxideterm_connection_monitor::LogCaptureCommand, String), String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_log_follow_command(&os_type, self.host_logs.preset).map(|command| (command, os_type))
    }

    pub(super) fn request_log_snapshot(
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
        if self.host_logs.polling {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::LogSnapshotAlreadyRunning)
                .into_iter()
                .collect();
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::LogConnectionMissing)
                .into_iter()
                .collect();
        };
        let command = match build_log_snapshot_command(
            &os_type,
            self.host_logs.preset,
            HOST_LOG_SNAPSHOT_LIMIT,
        ) {
            Ok(command) => command,
            Err(error) => {
                self.host_logs.snapshot_connection_id = Some(connection_id);
                self.host_logs.snapshot = Some(ResourceLogSnapshot {
                    status: ResourceLogStatus::Error { message: error },
                    entries: Vec::new(),
                });
                cx.notify();
                return feedback
                    .should_toast()
                    .then_some(HostToolsNotice::LogSnapshotFailed)
                    .into_iter()
                    .collect();
            }
        };
        let mut notices = Vec::new();
        if feedback.should_toast() && command.capability == LogCommandCapability::Partial {
            notices.push(HostToolsNotice::LogPartialSupport { os_type });
        }

        let request = HostLogSnapshotRequest {
            connection_id: connection_id.clone(),
            preset: self.host_logs.preset,
            limit: HOST_LOG_SNAPSHOT_LIMIT,
            feedback,
            failure_fallback,
        };
        self.host_logs.snapshot_connection_id = Some(connection_id);
        self.host_logs.running = Some(request.clone());
        self.host_logs.polling = true;
        let spawned = self.spawn_log_snapshot_capture(
            command.command,
            request,
            HOST_LOG_SNAPSHOT_TIMEOUT,
            HOST_LOG_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_logs.polling = false;
            self.host_logs.running = None;
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::LogConnectionMissing)
                .into_iter()
                .collect();
        }
        cx.notify();
        notices
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_logs_snapshot(
        &mut self,
        delivery: HostLogSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_logs.running.as_ref() != Some(&delivery.request) {
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_logs.polling = false;
        self.host_logs.running = None;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_log_snapshot(&output.stdout);
                if feedback.should_toast() {
                    match &snapshot.status {
                        ResourceLogStatus::Available { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::LogSnapshotLoaded {
                                    count: snapshot.entries.len(),
                                },
                            ));
                        }
                        ResourceLogStatus::Unavailable => {
                            cx.emit(HostToolsEvent::ShowNotice(HostToolsNotice::LogUnavailable));
                        }
                        ResourceLogStatus::Error { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::LogSnapshotFailed,
                            ));
                        }
                        ResourceLogStatus::Unknown => {}
                    }
                }
                self.host_logs.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_logs.snapshot = Some(snapshot);
            }
            Ok(_) => {
                self.host_logs.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_logs.snapshot = Some(ResourceLogSnapshot {
                    status: ResourceLogStatus::Error {
                        message: failure_fallback,
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::LogSnapshotFailed,
                    ));
                }
            }
            Err(()) => {
                self.host_logs.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_logs.snapshot = Some(ResourceLogSnapshot {
                    status: ResourceLogStatus::Error {
                        message: failure_fallback,
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::LogSnapshotFailed,
                    ));
                }
            }
        }
        cx.notify();
    }
}

fn host_log_blank_dash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "—".to_string()
    } else {
        trimmed.to_string()
    }
}

fn host_log_timestamp_label(timestamp: &str) -> String {
    let trimmed = timestamp.trim();
    if trimmed.is_empty() {
        return "—".to_string();
    }
    if let Some((_, time)) = trimmed.split_once('T') {
        return time.chars().take(8).collect::<String>();
    }
    let parts = trimmed.split_whitespace().collect::<Vec<_>>();
    if parts.len() >= 3 && parts[2].contains(':') {
        return parts[2].chars().take(8).collect::<String>();
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) && trimmed.len() > 6 {
        let seconds = &trimmed[..trimmed.len().saturating_sub(6)];
        let start = seconds.len().saturating_sub(6);
        return format!("{}s", &seconds[start..]);
    }
    trimmed.chars().take(12).collect()
}

fn log_level_color(level: &str, muted_color: u32) -> u32 {
    match level.trim().to_lowercase().as_str() {
        "error" | "critical" | "crit" | "err" | "failed" => MONITOR_RED,
        "warning" | "warn" => MONITOR_AMBER,
        "debug" => muted_color,
        "info" | "notice" => MONITOR_EMERALD,
        _ => muted_color,
    }
}
