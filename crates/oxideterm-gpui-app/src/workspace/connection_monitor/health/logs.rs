//! Owns the logs Host Tool UI and request lifecycle.

use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_logs_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let snapshot = self
            .connection_monitor
            .host_log_snapshot
            .as_ref()
            .filter(|_| {
                self.connection_monitor
                    .host_log_snapshot_connection_id
                    .as_deref()
                    == Some(selected_id)
            });
        let rows = snapshot
            .map(|snapshot| {
                visible_log_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_log_search_query,
                    self.connection_monitor.host_log_preset,
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_log_list_state(&rows, selected_id);

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
                        !self.connection_monitor.host_log_snapshot_polling,
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
                self.connection_monitor.host_log_snapshot_polling,
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
        let active = self.connection_monitor.host_log_preset == preset;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(log_preset_label_key(preset)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_log_preset != preset {
                        this.connection_monitor.host_log_preset = preset;
                        this.connection_monitor.host_log_expanded_index = None;
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
                            disabled: self.connection_monitor.host_log_snapshot_polling,
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
        let state = self.connection_monitor.host_log_list_state.clone();
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
        let expanded = self.connection_monitor.host_log_expanded_index == Some(index);
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
                    if this.connection_monitor.host_log_expanded_index == Some(index) {
                        this.connection_monitor.host_log_expanded_index = None;
                    } else {
                        this.connection_monitor.host_log_expanded_index = Some(index);
                    }
                    cx.notify();
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

    pub(super) fn sync_host_log_list_state(&self, rows: &[ResourceLogEntry], selected_id: &str) {
        let signatures = rows.iter().map(log_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-logs:{selected_id}:{}:{}:{}",
            self.connection_monitor.host_log_search_query,
            self.connection_monitor.host_log_preset as u8,
            self.connection_monitor
                .host_log_expanded_index
                .unwrap_or(usize::MAX)
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_log_list_state,
            &mut self.connection_monitor.host_log_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn host_log_snapshot_command(
        &self,
        connection_id: &str,
        preset: LogPreset,
        limit: usize,
    ) -> Result<(oxideterm_connection_monitor::LogCaptureCommand, String), String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_log_snapshot_command(&os_type, preset, limit).map(|command| (command, os_type))
    }

    pub(super) fn host_log_follow_command(
        &self,
        connection_id: &str,
        preset: LogPreset,
    ) -> Result<(oxideterm_connection_monitor::LogCaptureCommand, String), String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_log_follow_command(&os_type, preset).map(|command| (command, os_type))
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
        let connections = self.monitor_connections();
        let Some(connection_id) = self
            .connection_monitor
            .selected_connection_id
            .clone()
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
        if self.connection_monitor.host_log_snapshot_polling {
            if feedback.should_toast() {
                self.push_host_log_toast(
                    self.i18n
                        .t("sidebar.host_logs.toast.snapshot_already_running"),
                    TerminalNoticeVariant::Warning,
                );
            }
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            if feedback.should_toast() {
                self.push_host_log_toast(
                    self.i18n.t("sidebar.host_logs.toast.connection_missing"),
                    TerminalNoticeVariant::Error,
                );
            }
            cx.notify();
            return;
        };
        let preset = self.connection_monitor.host_log_preset;
        let (command, os_type) =
            match self.host_log_snapshot_command(&connection_id, preset, HOST_LOG_SNAPSHOT_LIMIT) {
                Ok(command) => command,
                Err(error) => {
                    if feedback.should_toast() {
                        self.push_host_log_toast(error, TerminalNoticeVariant::Error);
                    }
                    cx.notify();
                    return;
                }
            };
        if feedback.should_toast() && command.capability == LogCommandCapability::Partial {
            self.push_host_log_toast(
                self.i18n_replace(
                    "sidebar.host_logs.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let request = HostLogSnapshotRequest {
            connection_id: connection_id.clone(),
            preset,
            limit: HOST_LOG_SNAPSHOT_LIMIT,
            feedback,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_log_snapshot_connection_id = Some(connection_id);
        self.connection_monitor.host_log_snapshot_running = Some(request.clone());
        self.connection_monitor.host_log_snapshot_rx = Some(rx);
        self.connection_monitor.host_log_snapshot_polling = true;
        self.connection_monitor.host_log_last_error = None;
        // Host logs are intentionally snapshot-driven. Do not join the profiler
        // refresh loop; journal/log commands are too expensive for high-frequency polling.
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_LOG_SNAPSHOT_TIMEOUT,
                    HOST_LOG_SNAPSHOT_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostLogSnapshotDelivery { request, result });
        });
        cx.notify();
    }

    pub(super) fn open_host_logs_follow_terminal(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preset = self.connection_monitor.host_log_preset;
        let (command, os_type) = match self.host_log_follow_command(&connection_id, preset) {
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
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_log_toast(
                self.i18n.t("sidebar.host_logs.toast.exec_terminal_missing"),
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

    pub(in crate::workspace) fn poll_host_logs_snapshot_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_log_snapshot_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_log_snapshot_rx.take() else {
            self.connection_monitor.host_log_snapshot_polling = false;
            self.connection_monitor.host_log_snapshot_running = None;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.finish_host_logs_snapshot(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_log_snapshot_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let feedback = self
                    .connection_monitor
                    .host_log_snapshot_running
                    .as_ref()
                    .map(|request| request.feedback)
                    .unwrap_or(HostSnapshotFeedback::Silent);
                self.connection_monitor.host_log_snapshot_polling = false;
                self.connection_monitor.host_log_snapshot_running = None;
                let reason = self.i18n.t("sidebar.host_logs.toast.unknown_error");
                self.connection_monitor.host_log_last_error = Some(reason.clone());
                if feedback.should_toast() {
                    self.push_host_log_toast(
                        self.i18n_replace(
                            "sidebar.host_logs.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
                cx.notify();
            }
        }
    }

    pub(super) fn finish_host_logs_snapshot(
        &mut self,
        delivery: HostLogSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_log_snapshot_running
            .as_ref()
            .is_some_and(|running| running != &delivery.request)
        {
            cx.notify();
            return;
        }
        let feedback = delivery.request.feedback;
        self.connection_monitor.host_log_snapshot_polling = false;
        self.connection_monitor.host_log_snapshot_running = None;
        self.connection_monitor.host_log_snapshot_rx = None;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_log_snapshot(&output.stdout);
                let visible_count = visible_log_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_log_search_query,
                    self.connection_monitor.host_log_preset,
                )
                .len();
                match &snapshot.status {
                    ResourceLogStatus::Available { .. } => {
                        self.connection_monitor.host_log_last_error = None;
                        if feedback.should_toast() {
                            self.push_host_log_toast(
                                self.i18n_replace(
                                    "sidebar.host_logs.toast.snapshot_loaded",
                                    &[("count", visible_count.to_string())],
                                ),
                                TerminalNoticeVariant::Success,
                            );
                        }
                    }
                    ResourceLogStatus::Unavailable => {
                        self.connection_monitor.host_log_last_error =
                            Some(self.i18n.t("sidebar.host_logs.unavailable"));
                        if feedback.should_toast() {
                            self.push_host_log_toast(
                                self.i18n.t("sidebar.host_logs.toast.unavailable"),
                                TerminalNoticeVariant::Warning,
                            );
                        }
                    }
                    ResourceLogStatus::Error { message } => {
                        self.connection_monitor.host_log_last_error = Some(message.clone());
                        if feedback.should_toast() {
                            self.push_host_log_toast(
                                self.i18n_replace(
                                    "sidebar.host_logs.toast.snapshot_failed",
                                    &[("reason", message.clone())],
                                ),
                                TerminalNoticeVariant::Error,
                            );
                        }
                    }
                    ResourceLogStatus::Unknown => {}
                }
                self.connection_monitor.host_log_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_log_snapshot = Some(snapshot);
            }
            Ok(output) => {
                let reason = host_log_capture_failure_message(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                    self.i18n.t("sidebar.host_logs.toast.unknown_error"),
                );
                self.connection_monitor.host_log_last_error = Some(reason.clone());
                self.connection_monitor.host_log_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_log_snapshot = Some(ResourceLogSnapshot {
                    status: ResourceLogStatus::Error {
                        message: reason.clone(),
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    self.push_host_log_toast(
                        self.i18n_replace(
                            "sidebar.host_logs.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
            }
            Err(error) => {
                self.connection_monitor.host_log_last_error = Some(error.clone());
                self.connection_monitor.host_log_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_log_snapshot = Some(ResourceLogSnapshot {
                    status: ResourceLogStatus::Error {
                        message: error.clone(),
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    self.push_host_log_toast(
                        self.i18n_replace(
                            "sidebar.host_logs.toast.snapshot_failed",
                            &[("reason", error)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
            }
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

fn host_log_capture_failure_message(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    fallback: String,
) -> String {
    let reason = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(fallback.as_str());
    match exit_code {
        Some(code) => format!("{reason} (exit {code})"),
        None => reason.to_string(),
    }
}
