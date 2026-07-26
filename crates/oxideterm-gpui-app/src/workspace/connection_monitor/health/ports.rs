//! Owns the ports Host Tool UI and request lifecycle.

use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_ports_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let snapshot = self.host_tools.read(cx).port_snapshot_for(selected_id);
        let filter = self.host_tools.read(cx).port_filter();
        let rows = snapshot
            .as_ref()
            .map(|snapshot| {
                visible_port_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_port_search_query,
                    filter,
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_port_list_state(&rows, selected_id, cx);

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
                    .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher_row(
                        &connections,
                        selected_id,
                        !self.host_tools.read(cx).port_snapshot_polling(),
                        cx,
                    ))
                    .child(self.render_host_port_search(cx))
                    .child(self.render_host_port_filter_row(cx))
                    .child(self.render_host_port_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        cx,
                    )),
            )
            .child(self.render_host_port_list(
                rows,
                self.host_tools.read(cx).port_snapshot_polling(),
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_port_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostPortSearch;
        let focused = self.connection_monitor.host_port_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_port_search_query,
                    placeholder: self.i18n.t("sidebar.host_ports.search_placeholder"),
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
                    this.connection_monitor.host_port_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
                    this.connection_monitor.host_log_search_focused = false;
                    this.connection_monitor.host_tmux_search_focused = false;
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

    pub(super) fn render_host_port_filter_row(&self, cx: &mut Context<Self>) -> AnyElement {
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
            row = row.child(self.render_host_port_filter_chip(filter, cx));
        }
        row.into_any_element()
    }

    pub(super) fn render_host_port_filter_chip(
        &self,
        filter: PortFilter,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.host_tools.read(cx).port_filter() == filter;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(port_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.select_port_filter(filter, cx);
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_port_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourcePortStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let capability_label = match status {
            ResourcePortStatus::Available {
                capability: PortCommandCapability::Full,
                ..
            } => self.i18n.t("sidebar.host_ports.capability.full"),
            ResourcePortStatus::Available {
                capability: PortCommandCapability::Partial,
                ..
            } => self.i18n.t("sidebar.host_ports.capability.partial"),
            _ => self.i18n.t("sidebar.host_ports.capability.unknown"),
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
                self.i18n.t("sidebar.host_ports.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.workspace_tooltip_icon_button(
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
                        self.i18n.t("sidebar.host_ports.actions.diagnostic"),
                        "host-port-diagnostic",
                        true,
                        cx.listener({
                            let selected_id = selected_id.clone();
                            move |this, _event, window, cx| {
                                this.open_host_port_diagnostic_terminal(
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
                            disabled: self.host_tools.read(cx).port_snapshot_polling(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        self.i18n.t("sidebar.host_ports.actions.refresh"),
                        "host-port-refresh",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.request_host_ports_snapshot(
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

    pub(super) fn render_host_port_list(
        &self,
        rows: Vec<ResourcePortEntry>,
        loading: bool,
        status: ResourcePortStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Network,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_ports.loading"),
                cx,
            );
        }
        match status {
            ResourcePortStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Network,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_ports.unavailable"),
                    cx,
                );
            }
            ResourcePortStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_ports.error", &[("error", message)]),
                    cx,
                );
            }
            ResourcePortStatus::Unknown | ResourcePortStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Network,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_ports.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.host_tools.read(cx).port_list_state();
        let spec = TauriVirtualListSpec::new(px(HOST_PORT_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        let show_context_columns =
            self.ai.chat.sidebar_width >= HOST_PORT_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_port_table_header(show_context_columns))
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
                                this.render_host_port_row(
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

    pub(super) fn render_host_port_table_header(&self, show_context_columns: bool) -> AnyElement {
        let theme = self.tokens.ui;
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
                    .child(self.i18n.t("sidebar.host_ports.columns.local")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PORT_PROTOCOL_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_ports.columns.protocol")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PORT_STATE_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_ports.columns.state")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PORT_PID_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_ports.columns.pid")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_PROCESS_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_ports.columns.process")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_REMOTE_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_ports.columns.remote")),
                    )
            })
            .into_any_element()
    }

    pub(super) fn render_host_port_row(
        &self,
        connection_id: &str,
        index: usize,
        entry: Option<ResourcePortEntry>,
        show_context_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.host_tools.read(cx).port_expanded_index() == Some(index);
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let local = host_port_endpoint_label(&entry.local_address, &entry.local_port);
        let remote = host_port_endpoint_label(&entry.remote_address, &entry.remote_port);
        let process = host_port_blank_dash(host_port_process_label(&entry).as_str());
        let pid = host_port_blank_dash(&entry.pid);
        let state = host_port_state_display(&self.i18n, &entry.state);

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
                            .font_family(mono_font.clone())
                            .child(local),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_PROTOCOL_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(entry.protocol.to_uppercase()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PORT_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_port_state_color(&entry.state, theme.text_muted)))
                            .font_family(mono_font.clone())
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
                            .font_family(mono_font.clone())
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
                                .font_family(mono_font.clone())
                                .child(process.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_PORT_REMOTE_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
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
                            .font_family(mono_font)
                            .child(if show_context_columns {
                                format!(
                                    "{} · {}",
                                    self.i18n.t("sidebar.host_ports.columns.source"),
                                    host_port_blank_dash(&entry.source)
                                )
                            } else {
                                format!("{} · {}", process, remote)
                            }),
                    )
                    .child(self.render_host_port_inline_actions(connection_id, &entry, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_port_detail(&entry))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.toggle_port_expanded(index, cx);
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_port_inline_actions(
        &self,
        connection_id: &str,
        entry: &ResourcePortEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let endpoint = host_port_endpoint_label(&entry.local_address, &entry.local_port);
        let pid = entry.pid.clone();
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.workspace_tooltip_icon_button(
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
                self.i18n.t("sidebar.host_ports.actions.copy_endpoint"),
                "host-port-copy-endpoint",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.copy_host_port_endpoint(endpoint.clone(), cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(self.workspace_tooltip_icon_button(
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
                self.i18n.t("sidebar.host_ports.actions.diagnostic"),
                "host-port-row-diagnostic",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |this, _event, window, cx| {
                        this.open_host_port_diagnostic_terminal(connection_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .child(self.workspace_tooltip_icon_button(
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
                self.i18n.t("sidebar.host_ports.actions.jump_process"),
                "host-port-jump-process",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    if !pid.is_empty() {
                        this.jump_host_port_to_process(pid.clone(), cx);
                    }
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(super) fn render_host_port_detail(&self, entry: &ResourcePortEntry) -> AnyElement {
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
                    .min_w(px(620.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .font_family(mono_font)
                    .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_ports.columns.local"),
                        host_port_endpoint_label(&entry.local_address, &entry.local_port)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_ports.columns.remote"),
                        host_port_endpoint_label(&entry.remote_address, &entry.remote_port)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_ports.columns.process"),
                        host_port_blank_dash(host_port_process_label(entry).as_str())
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_ports.columns.user"),
                        host_port_blank_dash(&entry.user)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_ports.columns.source"),
                        host_port_blank_dash(&entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_ports.columns.inode"),
                        host_port_blank_dash(&entry.inode)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_ports.columns.command"),
                        host_port_blank_dash(&entry.command)
                    ))),
            )
            .into_any_element()
    }

    pub(super) fn sync_host_port_list_state(
        &self,
        rows: &[ResourcePortEntry],
        selected_id: &str,
        cx: &mut Context<Self>,
    ) {
        let signatures = rows.iter().map(port_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-ports:{selected_id}:{}:{}:{}",
            self.connection_monitor.host_port_search_query,
            self.host_tools.read(cx).port_filter() as u8,
            self.host_tools
                .read(cx)
                .port_expanded_index()
                .unwrap_or(usize::MAX)
        );
        self.host_tools
            .read(cx)
            .sync_port_list_signatures(&identity, &signatures);
    }

    pub(in crate::workspace) fn handle_host_port_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_port_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_port_search_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn request_host_ports_snapshot_for_selected_connection(
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
        self.request_host_ports_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_host_ports_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        let monitoring_enabled = self.host_tool_monitoring_enabled(ContextSidebarTool::Ports);
        let runtime = self.forwarding_runtime.handle().clone();
        let failure_fallback = self.i18n.t("sidebar.host_ports.toast.unknown_error");
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_port_snapshot(
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

    pub(super) fn copy_host_port_endpoint(&mut self, endpoint: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(endpoint.clone()));
        self.push_host_port_toast(
            self.i18n_replace(
                "sidebar.host_ports.toast.copied_endpoint",
                &[("endpoint", endpoint)],
            ),
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(super) fn jump_host_port_to_process(&mut self, pid: String, cx: &mut Context<Self>) {
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.select_tool(ContextSidebarTool::Processes, cx);
        });
        self.connection_monitor.host_process_search_query = pid;
        self.connection_monitor.host_process_search_focused = false;
        self.connection_monitor.host_port_search_focused = false;
        self.clear_ime_selection();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn open_host_port_diagnostic_terminal(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = self
            .host_tools
            .read(cx)
            .port_diagnostic_command(&connection_id);
        let title = self.i18n.t("sidebar.host_ports.diagnostic_title");
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_port_toast(
                self.i18n
                    .t("sidebar.host_ports.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        if !self.ssh_nodes.contains_key(&node_id) {
            self.push_host_port_toast(
                self.i18n
                    .t("sidebar.host_ports.toast.exec_terminal_missing"),
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
            Ok(()) => self.push_host_port_toast(
                self.i18n.t("sidebar.host_ports.toast.diagnostic_opened"),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_port_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }

    pub(super) fn push_host_port_toast(&mut self, message: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }
}

impl HostToolsEntity {
    pub(super) fn port_snapshot_for(&self, connection_id: &str) -> Option<ResourcePortSnapshot> {
        self.host_ports
            .snapshot
            .as_ref()
            .filter(|_| self.host_ports.snapshot_connection_id.as_deref() == Some(connection_id))
            .cloned()
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

    pub(in crate::workspace) fn clear_port_expanded(&mut self, cx: &mut Context<Self>) {
        if self.host_ports.expanded_index.take().is_some() {
            cx.notify();
        }
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

    pub(super) fn request_port_snapshot(
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
