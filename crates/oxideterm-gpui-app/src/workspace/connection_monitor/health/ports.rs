//! Owns the ports Host Tool UI and request lifecycle.

use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_ports_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .host_port_snapshot
            .as_ref()
            .filter(|_| {
                self.connection_monitor
                    .host_port_snapshot_connection_id
                    .as_deref()
                    == Some(selected_id)
            });
        let rows = snapshot
            .map(|snapshot| {
                visible_port_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_port_search_query,
                    self.connection_monitor.host_port_filter,
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_port_list_state(&rows, selected_id);

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
                        !self.connection_monitor.host_port_snapshot_polling,
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
                self.connection_monitor.host_port_snapshot_polling,
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
        let active = self.connection_monitor.host_port_filter == filter;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(port_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_port_filter != filter {
                        this.connection_monitor.host_port_filter = filter;
                        this.connection_monitor.host_port_expanded_index = None;
                    }
                    cx.notify();
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
                            disabled: self.connection_monitor.host_port_snapshot_polling,
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
        let state = self.connection_monitor.host_port_list_state.clone();
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
        let expanded = self.connection_monitor.host_port_expanded_index == Some(index);
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
                    if this.connection_monitor.host_port_expanded_index == Some(index) {
                        this.connection_monitor.host_port_expanded_index = None;
                    } else {
                        this.connection_monitor.host_port_expanded_index = Some(index);
                    }
                    cx.notify();
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

    pub(super) fn sync_host_port_list_state(&self, rows: &[ResourcePortEntry], selected_id: &str) {
        let signatures = rows.iter().map(port_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-ports:{selected_id}:{}:{}:{}",
            self.connection_monitor.host_port_search_query,
            self.connection_monitor.host_port_filter as u8,
            self.connection_monitor
                .host_port_expanded_index
                .unwrap_or(usize::MAX)
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_port_list_state,
            &mut self.connection_monitor.host_port_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_PORT_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn host_port_snapshot_command(
        &self,
        connection_id: &str,
    ) -> (oxideterm_connection_monitor::PortCaptureCommand, String) {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        (build_port_snapshot_command(&os_type), os_type)
    }

    pub(super) fn host_port_diagnostic_command(&self, connection_id: &str) -> (String, String) {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        (build_port_diagnostic_command(&os_type), os_type)
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
        self.request_host_ports_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_host_ports_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_port_snapshot_polling {
            if feedback.should_toast() {
                self.push_host_port_toast(
                    self.i18n
                        .t("sidebar.host_ports.toast.snapshot_already_running"),
                    TerminalNoticeVariant::Warning,
                );
            }
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            if feedback.should_toast() {
                self.push_host_port_toast(
                    self.i18n.t("sidebar.host_ports.toast.connection_missing"),
                    TerminalNoticeVariant::Error,
                );
            }
            cx.notify();
            return;
        };
        let (command, os_type) = self.host_port_snapshot_command(&connection_id);
        if feedback.should_toast() && command.capability == PortCommandCapability::Partial {
            self.push_host_port_toast(
                self.i18n_replace(
                    "sidebar.host_ports.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let request = HostPortSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_port_snapshot_connection_id = Some(connection_id);
        self.connection_monitor.host_port_snapshot_running = Some(request.clone());
        self.connection_monitor.host_port_snapshot_rx = Some(rx);
        self.connection_monitor.host_port_snapshot_polling = true;
        self.connection_monitor.host_port_last_error = None;
        // Port sampling is a troubleshooting snapshot, not a monitor metric.
        // Keep it out of the high-frequency profiler loop.
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_PORT_SNAPSHOT_TIMEOUT,
                    HOST_PORT_SNAPSHOT_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostPortSnapshotDelivery { request, result });
        });
        cx.notify();
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
        self.active_context_sidebar_tool = ContextSidebarTool::Processes;
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
        let (command, _os_type) = self.host_port_diagnostic_command(&connection_id);
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
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_port_toast(
                self.i18n
                    .t("sidebar.host_ports.toast.exec_terminal_missing"),
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

    pub(in crate::workspace) fn poll_host_ports_snapshot_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_monitor.host_port_snapshot_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_port_snapshot_rx.take() else {
            self.connection_monitor.host_port_snapshot_polling = false;
            self.connection_monitor.host_port_snapshot_running = None;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.finish_host_ports_snapshot(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_port_snapshot_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let feedback = self
                    .connection_monitor
                    .host_port_snapshot_running
                    .as_ref()
                    .map(|request| request.feedback)
                    .unwrap_or(HostSnapshotFeedback::Silent);
                self.connection_monitor.host_port_snapshot_polling = false;
                self.connection_monitor.host_port_snapshot_running = None;
                let reason = self.i18n.t("sidebar.host_ports.toast.unknown_error");
                self.connection_monitor.host_port_last_error = Some(reason.clone());
                if feedback.should_toast() {
                    self.push_host_port_toast(
                        self.i18n_replace(
                            "sidebar.host_ports.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
                cx.notify();
            }
        }
    }

    pub(super) fn finish_host_ports_snapshot(
        &mut self,
        delivery: HostPortSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_port_snapshot_running
            .as_ref()
            .is_some_and(|running| running != &delivery.request)
        {
            cx.notify();
            return;
        }
        let feedback = delivery.request.feedback;
        self.connection_monitor.host_port_snapshot_polling = false;
        self.connection_monitor.host_port_snapshot_running = None;
        self.connection_monitor.host_port_snapshot_rx = None;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_port_snapshot(&output.stdout);
                let visible_count = visible_port_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_port_search_query,
                    self.connection_monitor.host_port_filter,
                )
                .len();
                match &snapshot.status {
                    ResourcePortStatus::Available { .. } => {
                        self.connection_monitor.host_port_last_error = None;
                        if feedback.should_toast() {
                            self.push_host_port_toast(
                                self.i18n_replace(
                                    "sidebar.host_ports.toast.snapshot_loaded",
                                    &[("count", visible_count.to_string())],
                                ),
                                TerminalNoticeVariant::Success,
                            );
                        }
                    }
                    ResourcePortStatus::Unavailable => {
                        self.connection_monitor.host_port_last_error =
                            Some(self.i18n.t("sidebar.host_ports.unavailable"));
                        if feedback.should_toast() {
                            self.push_host_port_toast(
                                self.i18n.t("sidebar.host_ports.toast.unavailable"),
                                TerminalNoticeVariant::Warning,
                            );
                        }
                    }
                    ResourcePortStatus::Error { message } => {
                        self.connection_monitor.host_port_last_error = Some(message.clone());
                        if feedback.should_toast() {
                            self.push_host_port_toast(
                                self.i18n_replace(
                                    "sidebar.host_ports.toast.snapshot_failed",
                                    &[("reason", message.clone())],
                                ),
                                TerminalNoticeVariant::Error,
                            );
                        }
                    }
                    ResourcePortStatus::Unknown => {}
                }
                self.connection_monitor.host_port_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_port_snapshot = Some(snapshot);
            }
            Ok(output) => {
                let reason = host_port_capture_failure_message(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                    self.i18n.t("sidebar.host_ports.toast.unknown_error"),
                );
                self.connection_monitor.host_port_last_error = Some(reason.clone());
                self.connection_monitor.host_port_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_port_snapshot = Some(ResourcePortSnapshot {
                    status: ResourcePortStatus::Error {
                        message: reason.clone(),
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    self.push_host_port_toast(
                        self.i18n_replace(
                            "sidebar.host_ports.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
            }
            Err(error) => {
                self.connection_monitor.host_port_last_error = Some(error.clone());
                self.connection_monitor.host_port_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_port_snapshot = Some(ResourcePortSnapshot {
                    status: ResourcePortStatus::Error {
                        message: error.clone(),
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    self.push_host_port_toast(
                        self.i18n_replace(
                            "sidebar.host_ports.toast.snapshot_failed",
                            &[("reason", error)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
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

fn host_port_capture_failure_message(
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
