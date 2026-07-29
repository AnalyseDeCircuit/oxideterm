//! Owns the logs Host Tool UI and request lifecycle.

use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_logs_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.selectable_text_render_state(cx);
        let show_context_columns =
            self.ai_entity.read(cx).chat_ui().sidebar_width >= HOST_LOG_CONTEXT_COLUMNS_MIN_WIDTH;
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::LogSearch, cx)
            .expect("log search is a non-secret Host Tools input");
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_logs_panel(
                search_ime,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                show_context_columns,
                cx,
            )
        })
    }
}

impl HostToolsEntity {
    pub(in crate::workspace::connection_monitor) fn render_host_logs_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        show_context_columns: bool,
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
        let snapshot = self.log_snapshot_for(selected_id);
        let preset = self.log_preset();
        let rows = snapshot
            .as_ref()
            .map(|snapshot| {
                visible_log_rows(&snapshot.entries, &self.ui.host_log_search_query, preset)
            })
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
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
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &connections,
                        selected_id,
                        !self.log_snapshot_in_flight(),
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_log_search(&search_ime, tokens, i18n, cx))
                    .child(self.render_host_log_preset_row(tokens, i18n, cx))
                    .child(self.render_host_log_status_row(
                        rows.len(),
                        selected_id,
                        status.clone(),
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(self.render_host_log_list(
                rows,
                self.log_snapshot_in_flight(),
                status,
                tokens,
                i18n,
                mono_font_family,
                selectable_text,
                show_context_columns,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_log_search(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = ime.input();
        let anchor_frame = ime.clone();
        let input_control = text_input(
            tokens,
            TextInputView {
                value: &self.ui.host_log_search_query,
                placeholder: i18n.t("sidebar.host_logs.search_placeholder"),
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
                // The one-shot request lets the root coordinate shared window IME state.
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
        );
        text_input_anchor_probe(
            ime.anchor_id(),
            input_control,
            move |anchor, _window, _cx| {
                anchor_frame.update_anchor(anchor);
            },
        )
        .into_any_element()
    }

    fn render_host_log_preset_row(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            row = row.child(self.render_host_log_preset_chip(preset, tokens, i18n, cx));
        }
        row.into_any_element()
    }

    fn render_host_log_preset_chip(
        &self,
        preset: LogPreset,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.log_preset() == preset;
        host_log_preset_chip(active, tokens)
            .child(i18n.t(log_preset_label_key(preset)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.select_log_preset(preset, cx) {
                        this.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_log_status_row(
        &self,
        visible_count: usize,
        selected_connection_id: &str,
        status: ResourceLogStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match status {
            ResourceLogStatus::Available {
                capability: LogCommandCapability::Full,
                ..
            } => i18n.t("sidebar.host_logs.capability.full"),
            ResourceLogStatus::Available {
                capability: LogCommandCapability::Partial,
                ..
            } => i18n.t("sidebar.host_logs.capability.partial"),
            _ => i18n.t("sidebar.host_logs.capability.unknown"),
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
                i18n.t("sidebar.host_logs.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.render_host_log_follow_action(
                        selected_connection_id.to_string(),
                        tokens,
                        i18n,
                        cx,
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::RefreshCw,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.log_snapshot_in_flight(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_logs.actions.refresh"),
                        "host-log-refresh",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.request_active_tool_snapshot(HostSnapshotFeedback::Toast, cx);
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }

    fn render_host_log_follow_action(
        &self,
        selected_connection_id: String,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let follow_i18n = i18n.clone();
        host_tools_tooltip_icon_button(
            tokens,
            LucideIcon::Activity,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 24.0,
                disabled: selected_connection_id.is_empty(),
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: if selected_connection_id.is_empty() {
                    0.45
                } else {
                    1.0
                },
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
            },
            i18n.t("sidebar.host_logs.actions.follow"),
            "host-log-follow",
            true,
            cx.listener(move |host_tools, _event, window, cx| {
                if !selected_connection_id.is_empty() {
                    host_tools.dispatch_log_follow_from_ui(
                        selected_connection_id.clone(),
                        &follow_i18n,
                        window,
                        cx,
                    );
                }
                cx.stop_propagation();
            }),
        )
    }

    fn dispatch_log_follow_from_ui(
        &mut self,
        connection_id: String,
        i18n: &I18n,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preset = self.log_preset();
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::LogConnectionMissing,
            ));
            return;
        };
        let command = match build_log_follow_command(&os_type, preset) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(HostToolsNotice::LogUnavailable));
                return;
            }
        };
        if command.capability == LogCommandCapability::Partial {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::LogPartialSupport {
                    os_type: os_type.clone(),
                },
            ));
        }
        let preset_label = i18n.t(log_preset_label_key(preset));
        let title = i18n
            .t("sidebar.host_logs.follow_title")
            .replace("{{preset}}", &preset_label);
        let opened_notice = i18n
            .t("sidebar.host_logs.toast.follow_opened")
            .replace("{{preset}}", &preset_label);
        let missing_notice = i18n.t("sidebar.host_logs.toast.exec_terminal_missing");
        // Only a fixed platform template reaches the terminal action; no user input is captured.
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

    fn render_host_log_list(
        &self,
        rows: Vec<ResourceLogEntry>,
        loading: bool,
        status: ResourceLogStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        show_context_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::FileText,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_logs.loading"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceLogStatus::Unavailable => {
                return host_tools_center_state(
                    LucideIcon::FileText,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_logs.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceLogStatus::Error { message } => {
                return host_tools_center_state(
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    i18n.t("sidebar.host_logs.error")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourceLogStatus::Unknown | ResourceLogStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::FileText,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_logs.empty"),
                selectable_text,
                cx,
            );
        }

        let rows = Arc::new(rows);
        let state = self.log_list_state();
        let spec = TauriVirtualListSpec::new(px(HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let tokens = *tokens;
        let i18n = i18n.clone();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_log_table_header(show_context_columns, &tokens, &i18n))
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
                            let mono_font_family = mono_font_family.clone();
                            host_tools.update(cx, |this, cx| {
                                this.render_host_log_row(
                                    index,
                                    rows.get(index).cloned(),
                                    show_context_columns,
                                    &tokens,
                                    &i18n,
                                    mono_font_family,
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_log_table_header(
        &self,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
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
                    .child(i18n.t("sidebar.host_logs.columns.time")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_LOG_LEVEL_COLUMN_WIDTH))
                    .child(i18n.t("sidebar.host_logs.columns.level")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_LOG_SOURCE_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_logs.columns.source")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_LOG_UNIT_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_logs.columns.unit")),
                    )
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(i18n.t("sidebar.host_logs.columns.message")),
            )
            .into_any_element()
    }

    fn render_host_log_row(
        &self,
        index: usize,
        entry: Option<ResourceLogEntry>,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.log_expanded_index() == Some(index);
        let theme = tokens.ui;
        let level_label = i18n.t(log_level_label_key(&entry.level));
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
                            .font_family(mono_font_family.clone())
                            .child(host_log_timestamp_label(&entry.timestamp)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_LOG_LEVEL_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(log_level_color(&entry.level, theme.text_muted)))
                            .font_family(mono_font_family.clone())
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
                                .font_family(mono_font_family.clone())
                                .child(host_log_blank_dash(&entry.source)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_LOG_UNIT_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
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
                        .font_family(mono_font_family.clone())
                        .child(format!(
                            "{} · {}",
                            host_log_blank_dash(&entry.source),
                            host_log_blank_dash(&entry.unit)
                        )),
                )
            })
            .when(expanded, |row| {
                row.child(self.render_host_log_detail(
                    &entry,
                    tokens,
                    i18n,
                    mono_font_family.clone(),
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.toggle_log_expanded(index, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_log_detail(
        &self,
        entry: &ResourceLogEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .mx_3()
            .mb_2()
            .rounded(px(tokens.radii.md))
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
                    .font_family(mono_font_family)
                    .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_logs.columns.time"),
                        host_log_blank_dash(&entry.timestamp)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_logs.columns.source"),
                        host_log_blank_dash(&entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_logs.columns.unit"),
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

    fn sync_host_log_list_state(&self, rows: &[ResourceLogEntry], selected_id: &str) {
        let signatures = rows.iter().map(log_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-logs:{selected_id}:{}:{}:{}",
            self.ui.host_log_search_query,
            self.log_preset() as u8,
            self.log_expanded_index().unwrap_or(usize::MAX)
        );
        self.sync_log_list_signatures(&identity, &signatures);
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_host_log_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::LogSearch)
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

    pub(super) fn log_snapshot_in_flight(&self) -> bool {
        self.host_logs.snapshot_in_flight
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

    pub(super) fn sync_log_list_signatures(&self, identity: &str, signatures: &[u64]) {
        sync_tauri_variable_list_state_by_signatures(
            &self.host_logs.list_state,
            &mut self.host_logs.list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(px(HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(in crate::workspace::connection_monitor) fn request_log_snapshot(
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
        if self.host_logs.snapshot_in_flight {
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
        self.host_logs.snapshot_in_flight = true;
        let spawned = self.spawn_log_snapshot_capture(
            command.command,
            request,
            HOST_LOG_SNAPSHOT_TIMEOUT,
            HOST_LOG_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_logs.snapshot_in_flight = false;
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
        mut delivery: HostLogSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_logs.running.as_ref() != Some(&delivery.request) {
            if let Ok(output) = delivery.result.as_mut() {
                zeroize_host_snapshot_output(output);
            }
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_logs.snapshot_in_flight = false;
        self.host_logs.running = None;
        match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                let mut snapshot = parse_log_snapshot(&output.stdout);
                if matches!(&snapshot.status, ResourceLogStatus::Error { .. }) {
                    snapshot.status = ResourceLogStatus::Error {
                        message: failure_fallback,
                    };
                }
                zeroize_host_snapshot_output(&mut output);
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
            Ok(mut output) => {
                zeroize_host_snapshot_output(&mut output);
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

fn host_log_preset_chip(active: bool, tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    // Preset chips are page-local controls, so their visual state follows the
    // same token contract without depending on the workspace root.
    div()
        .flex_none()
        .h(px(tokens.metrics.ui_button_sm_height * 0.75))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.md))
        .cursor_pointer()
        .bg(if active {
            rgb(theme.bg_hover)
        } else {
            rgba(0x00000000)
        })
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(if active {
            rgb(theme.text)
        } else {
            rgb(theme.text_muted)
        })
        .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
}
