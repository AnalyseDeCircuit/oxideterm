//! Owns the ports Host Tool UI and request lifecycle.

use super::*;

impl HostToolsEntity {
    #[allow(clippy::too_many_arguments)]
    fn render_host_ports_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        sidebar_width: f32,
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
        let snapshot = self.port_snapshot_for(selected_id);
        let rows = snapshot
            .map(|snapshot| {
                visible_port_rows(
                    &snapshot.entries,
                    &self.ui.host_port_search_query,
                    self.port_filter(),
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_port_render_rows(&rows, selected_id);

        div()
            .id("host-ports-panel")
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
                        !self.port_snapshot_polling(),
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_port_search(&search_ime, tokens, i18n, cx))
                    .child(self.render_host_port_filter_row(tokens, i18n, cx))
                    .child(self.render_host_port_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(self.render_host_port_list(
                rows,
                self.port_snapshot_polling(),
                status,
                selected_id,
                sidebar_width,
                tokens,
                i18n,
                mono_font_family,
                selectable_text,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_port_search(
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
            text_input(
                tokens,
                TextInputView {
                    value: &self.ui.host_port_search_query,
                    placeholder: i18n.t("sidebar.host_ports.search_placeholder"),
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
                    // The workspace coordinates only the shared window IME.
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

    fn render_host_port_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourcePortStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match status {
            ResourcePortStatus::Available {
                capability: PortCommandCapability::Full,
                ..
            } => i18n.t("sidebar.host_ports.capability.full"),
            ResourcePortStatus::Available {
                capability: PortCommandCapability::Partial,
                ..
            } => i18n.t("sidebar.host_ports.capability.partial"),
            _ => i18n.t("sidebar.host_ports.capability.unknown"),
        };
        let diagnostic_command = self.port_diagnostic_command(&selected_id);
        let diagnostic_title = i18n.t("sidebar.host_ports.diagnostic_title");
        let diagnostic_opened_notice = i18n.t("sidebar.host_ports.toast.diagnostic_opened");
        let diagnostic_missing_notice = i18n.t("sidebar.host_ports.toast.exec_terminal_missing");
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
                i18n.t("sidebar.host_ports.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::Terminal,
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
                        i18n.t("sidebar.host_ports.actions.diagnostic"),
                        "host-port-diagnostic",
                        true,
                        cx.listener(move |_host_tools, _event, window, cx| {
                            // The one-shot intent moves the generated command
                            // without retaining the workspace or SSH node.
                            window.dispatch_action(
                                Box::new(HostToolsWindowRequest::new(
                                    HostToolsWindowIntent::OpenExistingNodeTerminal {
                                        connection_id: selected_id.clone(),
                                        command: diagnostic_command.clone(),
                                        title: diagnostic_title.clone(),
                                        opened_notice: diagnostic_opened_notice.clone(),
                                        missing_notice: diagnostic_missing_notice.clone(),
                                    },
                                )),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::RefreshCw,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.port_snapshot_polling(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_ports.actions.refresh"),
                        "host-port-refresh",
                        true,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            host_tools
                                .request_active_tool_snapshot(HostSnapshotFeedback::Toast, cx);
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }
    fn render_host_port_filter_row(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div()
            .id("host-port-filter-scroll")
            .flex()
            .items_center()
            .gap_1()
            .overflow_x_scroll();
        for filter in [
            PortFilter::All,
            PortFilter::Listening,
            PortFilter::Connected,
            PortFilter::Tcp,
            PortFilter::Udp,
            PortFilter::Risky,
        ] {
            row = row.child(self.render_host_port_filter_chip(filter, tokens, i18n, cx));
        }
        row.into_any_element()
    }

    fn render_host_port_filter_chip(
        &self,
        filter: PortFilter,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.port_filter() == filter;
        host_port_filter_chip(active, tokens)
            .child(i18n.t(port_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.select_port_filter(filter, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}

impl HostToolsEntity {
    #[allow(clippy::too_many_arguments)]
    fn render_host_port_list(
        &self,
        rows: Vec<ResourcePortEntry>,
        loading: bool,
        status: ResourcePortStatus,
        selected_id: &str,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::Network,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_ports.loading"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourcePortStatus::Unavailable => {
                return host_tools_center_state(
                    LucideIcon::Network,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_ports.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourcePortStatus::Error { message } => {
                return host_tools_center_state(
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    i18n.t("sidebar.host_ports.error")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourcePortStatus::Unknown | ResourcePortStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::Network,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_ports.empty"),
                selectable_text,
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.port_list_state();
        let spec = TauriVirtualListSpec::new(px(HOST_PORT_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let show_context_columns = sidebar_width >= HOST_PORT_CONTEXT_COLUMNS_MIN_WIDTH;
        let row_tokens = *tokens;
        let row_i18n = i18n.clone();
        let row_mono_font_family = mono_font_family.clone();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(Self::render_host_port_table_header(
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
                            let rows = rows.clone();
                            let selected_id = selected_id.clone();
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_port_row(
                                    selected_id.as_str(),
                                    index,
                                    rows.get(index).cloned(),
                                    show_context_columns,
                                    &row_tokens,
                                    &row_i18n,
                                    row_mono_font_family.clone(),
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_port_table_header(
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_PORT_TABLE_HEADER_HEIGHT))
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
                    .child(i18n.t("sidebar.host_ports.columns.local")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PORT_PROTOCOL_COLUMN_WIDTH))
                    .child(i18n.t("sidebar.host_ports.columns.protocol")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PORT_STATE_COLUMN_WIDTH))
                    .child(i18n.t("sidebar.host_ports.columns.state")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PORT_PID_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_ports.columns.pid")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_PROCESS_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_ports.columns.process")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_REMOTE_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_ports.columns.remote")),
                    )
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_port_row(
        &self,
        connection_id: &str,
        index: usize,
        entry: Option<ResourcePortEntry>,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.port_expanded_index() == Some(index);
        let theme = tokens.ui;
        let local = host_port_endpoint_label(&entry.local_address, &entry.local_port);
        let remote = host_port_endpoint_label(&entry.remote_address, &entry.remote_port);
        let process = host_port_blank_dash(host_port_process_label(&entry).as_str());
        let pid = host_port_blank_dash(&entry.pid);
        let state = host_port_state_display(i18n, &entry.state);

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
                    .h(px(HOST_PORT_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Keep the endpoint identity as the first-level flex child.
                    // Buttons and secondary metadata live outside this row so
                    // resizing the companion sidebar cannot collapse the address into `...`.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(if port_is_risky_exposure(&entry) {
                                MONITOR_AMBER
                            } else {
                                theme.text
                            }))
                            .font_family(mono_font_family.clone())
                            .child(local),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_PROTOCOL_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font_family.clone())
                            .child(entry.protocol.to_uppercase()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_port_state_color(&entry.state, theme.text_muted)))
                            .font_family(mono_font_family.clone())
                            .child(state),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_PID_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font_family.clone())
                            .child(pid),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_PORT_PROCESS_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font_family.clone())
                                .child(process.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_PORT_REMOTE_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font_family.clone())
                                .child(remote.clone()),
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
                                format!(
                                    "{} · {}",
                                    i18n.t("sidebar.host_ports.columns.source"),
                                    host_port_blank_dash(&entry.source)
                                )
                            } else {
                                format!("{} · {}", process, remote)
                            }),
                    )
                    .child(self.render_host_port_inline_actions(
                        connection_id,
                        &entry,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .when(expanded, |row| {
                row.child(Self::render_host_port_detail(
                    &entry,
                    tokens,
                    i18n,
                    mono_font_family,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Expansion is local view state and never triggers a remote request.
                    host_tools.toggle_port_expanded(index, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_port_inline_actions(
        &self,
        connection_id: &str,
        entry: &ResourcePortEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let endpoint = host_port_endpoint_label(&entry.local_address, &entry.local_port);
        let pid = entry.pid.clone();
        let diagnostic_command = self.port_diagnostic_command(connection_id);
        let diagnostic_title = i18n.t("sidebar.host_ports.diagnostic_title");
        let diagnostic_opened_notice = i18n.t("sidebar.host_ports.toast.diagnostic_opened");
        let diagnostic_missing_notice = i18n.t("sidebar.host_ports.toast.exec_terminal_missing");
        let connection_id = connection_id.to_string();
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Copy,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_ports.actions.copy_endpoint"),
                "host-port-copy-endpoint",
                true,
                cx.listener(move |_host_tools, _event, _window, cx| {
                    // Clipboard ownership belongs to the GPUI Entity context.
                    cx.write_to_clipboard(ClipboardItem::new_string(endpoint.clone()));
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::PortEndpointCopied {
                            endpoint: endpoint.clone(),
                        },
                    ));
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Terminal,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_ports.actions.diagnostic"),
                "host-port-row-diagnostic",
                true,
                cx.listener(move |_host_tools, _event, window, cx| {
                    // The workspace resolves NodeRouter ownership after this
                    // one-shot page intent reaches the window boundary.
                    window.dispatch_action(
                        Box::new(HostToolsWindowRequest::new(
                            HostToolsWindowIntent::OpenExistingNodeTerminal {
                                connection_id: connection_id.clone(),
                                command: diagnostic_command.clone(),
                                title: diagnostic_title.clone(),
                                opened_notice: diagnostic_opened_notice.clone(),
                                missing_notice: diagnostic_missing_notice.clone(),
                            },
                        )),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Search,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: pid.is_empty(),
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: if pid.is_empty() { 0.45 } else { 1.0 },
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_ports.actions.jump_process"),
                "host-port-jump-process",
                true,
                cx.listener(move |host_tools, _event, _window, cx| {
                    if !pid.is_empty() {
                        host_tools.ui.host_process_search_query = pid.clone();
                        host_tools.ui.clear_input_focus();
                        // ToolSelected lets the root clear shared IME state
                        // without retaining a reverse workspace dependency.
                        host_tools.select_sidebar_tool(ContextSidebarTool::Processes, cx);
                    }
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }
}

impl WorkspaceApp {
    pub(super) fn render_host_ports_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.selectable_text_render_state(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::PortSearch, cx)
            .expect("port search is a non-secret Host Tools input");
        let sidebar_width = self.ai.chat.sidebar_width;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_ports_panel(
                search_ime,
                sidebar_width,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_port_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::PortSearch)
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
    fn render_host_port_detail(
        entry: &ResourcePortEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
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
                    .min_w(px(620.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .font_family(mono_font)
                    .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_ports.columns.local"),
                        host_port_endpoint_label(&entry.local_address, &entry.local_port)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_ports.columns.remote"),
                        host_port_endpoint_label(&entry.remote_address, &entry.remote_port)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_ports.columns.process"),
                        host_port_blank_dash(host_port_process_label(entry).as_str())
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_ports.columns.user"),
                        host_port_blank_dash(&entry.user)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_ports.columns.source"),
                        host_port_blank_dash(&entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_ports.columns.inode"),
                        host_port_blank_dash(&entry.inode)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_ports.columns.command"),
                        host_port_blank_dash(&entry.command)
                    ))),
            )
            .into_any_element()
    }

    fn sync_port_render_rows(&self, rows: &[ResourcePortEntry], selected_id: &str) {
        let signatures = rows.iter().map(port_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-ports:{selected_id}:{}:{}:{}",
            self.ui.host_port_search_query,
            self.port_filter() as u8,
            self.port_expanded_index().unwrap_or(usize::MAX)
        );
        self.sync_port_list_signatures(&identity, &signatures);
    }

    pub(super) fn port_snapshot_for(&self, connection_id: &str) -> Option<&ResourcePortSnapshot> {
        self.host_ports
            .snapshot
            .as_ref()
            .filter(|_| self.host_ports.snapshot_connection_id.as_deref() == Some(connection_id))
    }

    pub(in crate::workspace::connection_monitor) fn port_filter(&self) -> PortFilter {
        self.host_ports.filter
    }

    pub(super) fn port_snapshot_polling(&self) -> bool {
        self.host_ports.polling
    }

    pub(super) fn port_list_state(&self) -> ListState {
        self.host_ports.list_state.clone()
    }

    pub(in crate::workspace::connection_monitor) fn port_expanded_index(&self) -> Option<usize> {
        self.host_ports.expanded_index
    }

    pub(in crate::workspace::connection_monitor) fn select_port_filter(
        &mut self,
        filter: PortFilter,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_ports.filter == filter {
            return false;
        }
        self.host_ports.filter = filter;
        self.host_ports.expanded_index = None;
        cx.notify();
        true
    }

    pub(in crate::workspace::connection_monitor) fn toggle_port_expanded(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.host_ports.expanded_index =
            (self.host_ports.expanded_index != Some(index)).then_some(index);
        cx.notify();
    }

    pub(super) fn sync_port_list_signatures(&self, identity: &str, signatures: &[u64]) {
        sync_tauri_variable_list_state_by_signatures(
            &self.host_ports.list_state,
            &mut self.host_ports.list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(px(HOST_PORT_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn port_diagnostic_command(&self, connection_id: &str) -> String {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_port_diagnostic_command(&os_type)
    }

    pub(in crate::workspace::connection_monitor) fn request_port_snapshot(
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
        if self.host_ports.polling {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PortSnapshotAlreadyRunning)
                .into_iter()
                .collect();
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PortConnectionMissing)
                .into_iter()
                .collect();
        };
        let command = build_port_snapshot_command(&os_type);
        let mut notices = Vec::new();
        if feedback.should_toast() && command.capability == PortCommandCapability::Partial {
            notices.push(HostToolsNotice::PortPartialSupport { os_type });
        }

        let request = HostPortSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            failure_fallback,
        };
        self.host_ports.snapshot_connection_id = Some(connection_id);
        self.host_ports.running = Some(request.clone());
        self.host_ports.polling = true;
        // Port capture is a user-requested troubleshooting snapshot, not a sampler.
        let spawned = self.spawn_port_snapshot_capture(
            command.command,
            request,
            HOST_PORT_SNAPSHOT_TIMEOUT,
            HOST_PORT_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_ports.polling = false;
            self.host_ports.running = None;
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PortConnectionMissing)
                .into_iter()
                .collect();
        }
        cx.notify();
        notices
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_ports_snapshot(
        &mut self,
        delivery: HostPortSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_ports.running.as_ref() != Some(&delivery.request) {
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_ports.polling = false;
        self.host_ports.running = None;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_port_snapshot(&output.stdout);
                if feedback.should_toast() {
                    match &snapshot.status {
                        ResourcePortStatus::Available { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::PortSnapshotLoaded {
                                    count: snapshot.entries.len(),
                                },
                            ));
                        }
                        ResourcePortStatus::Unavailable => {
                            cx.emit(HostToolsEvent::ShowNotice(HostToolsNotice::PortUnavailable));
                        }
                        ResourcePortStatus::Error { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::PortSnapshotFailed,
                            ));
                        }
                        ResourcePortStatus::Unknown => {}
                    }
                }
                self.host_ports.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_ports.snapshot = Some(snapshot);
            }
            Ok(_) | Err(()) => {
                self.host_ports.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_ports.snapshot = Some(ResourcePortSnapshot {
                    status: ResourcePortStatus::Error {
                        message: failure_fallback,
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::PortSnapshotFailed,
                    ));
                }
            }
        }
        cx.notify();
    }
}

fn host_port_endpoint_label(address: &str, port: &str) -> String {
    host_port_blank_dash(&port_endpoint(address, port))
}

fn host_port_blank_dash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        "—".to_string()
    } else {
        trimmed.to_string()
    }
}

fn host_port_process_label(entry: &ResourcePortEntry) -> String {
    if !entry.process_name.trim().is_empty() {
        return entry.process_name.clone();
    }
    if !entry.command.trim().is_empty() {
        return entry.command.clone();
    }
    entry.pid.clone()
}

fn host_port_state_display(i18n: &I18n, state: &str) -> String {
    let key = port_state_label_key(state);
    if key == "sidebar.host_ports.states.unknown" && !state.trim().is_empty() {
        state.trim().to_string()
    } else {
        i18n.t(key)
    }
}

fn host_port_state_color(state: &str, muted_color: u32) -> u32 {
    match state.trim().to_lowercase().as_str() {
        "listen" | "listening" | "udp" | "unconn" | "open" => MONITOR_EMERALD,
        "estab" | "established" => MONITOR_BLUE,
        "syn-sent" | "syn-recv" | "close-wait" => MONITOR_AMBER,
        "time-wait" | "time_wait" => muted_color,
        _ => muted_color,
    }
}

fn host_port_filter_chip(active: bool, tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    // Keep the resource filter self-contained so it can update Entity state
    // without retaining a workspace render dependency.
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
