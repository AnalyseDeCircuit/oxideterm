//! Owns the process Host Tool UI and request lifecycle.

use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_processes_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .map(|metrics| self.visible_host_process_rows(&metrics.top_processes))
            .unwrap_or_default();
        self.sync_host_process_list_state(&rows, selected_id);

        div()
            .id("host-processes-panel")
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
                    .child(self.render_host_process_search(cx))
                    .child(self.render_host_process_filter_row(cx))
                    .child(self.render_host_process_sort_row(rows.len(), cx)),
            )
            .child(self.render_host_process_list(rows, current.is_some(), selected_id, cx))
            .into_any_element()
    }

    pub(super) fn render_host_process_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostProcessSearch;
        let focused = self.connection_monitor.host_process_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_process_search_query,
                    placeholder: self.i18n.t("sidebar.host_processes.search_placeholder"),
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
                    this.connection_monitor.host_process_search_focused = true;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
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

    pub(super) fn render_host_process_filter_row(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .child(self.render_host_process_filter_chip(
                ProcessFilter::All,
                "sidebar.host_processes.filters.all",
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::Running,
                "sidebar.host_processes.filters.running",
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::HighCpu,
                "sidebar.host_processes.filters.high_cpu",
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::HighMemory,
                "sidebar.host_processes.filters.high_memory",
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_process_filter_chip(
        &self,
        filter: ProcessFilter,
        label_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.connection_monitor.host_process_filter == filter;
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .px_2()
            .h(px(24.0))
            .flex()
            .items_center()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(11.0))
            .cursor_pointer()
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "host-process-filter",
                label_key,
                self.i18n.t(label_key),
                if active { theme.text } else { theme.text_muted },
                cx,
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.connection_monitor.host_process_filter = filter;
                    this.connection_monitor.host_process_expanded_pid = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_sort_row(
        &self,
        visible_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(div().flex_none().child(format!(
                "{} {}",
                visible_count,
                self.i18n.t("sidebar.host_processes.count_suffix")
            )))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Cpu,
                        "sidebar.host_processes.sort.cpu",
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Memory,
                        "sidebar.host_processes.sort.memory",
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Pid,
                        "sidebar.host_processes.sort.pid",
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Command,
                        "sidebar.host_processes.sort.command",
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::User,
                        "sidebar.host_processes.sort.user",
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_sort_button(
        &self,
        sort: ProcessSort,
        label_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.connection_monitor.host_process_sort == sort;
        let theme = self.tokens.ui;
        let mut label = self.i18n.t(label_key);
        if active {
            label.push_str(if self.connection_monitor.host_process_sort_descending {
                " ↓"
            } else {
                " ↑"
            });
        }
        div()
            .flex_none()
            .px_1p5()
            .h(px(22.0))
            .flex()
            .items_center()
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_process_sort == sort {
                        this.connection_monitor.host_process_sort_descending =
                            !this.connection_monitor.host_process_sort_descending;
                    } else {
                        this.connection_monitor.host_process_sort = sort;
                        this.connection_monitor.host_process_sort_descending =
                            !matches!(sort, ProcessSort::Command | ProcessSort::User);
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_list(
        &self,
        rows: Vec<ResourceTopProcess>,
        has_metrics: bool,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_metrics {
            return monitor_center_state(
                self,
                LucideIcon::Activity,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_processes.sampling"),
                cx,
            );
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::ListChecks,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_processes.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.connection_monitor.host_process_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        let separate_user_column =
            host_process_table_uses_separate_user_column(self.ai.chat.sidebar_width);
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            // Processes are an operational table, not a card stack; keep the
            // header fixed while the GPUI List owns only the scrolling rows.
            .child(self.render_host_process_table_header(separate_user_column))
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
                                this.render_host_process_row(
                                    selected_id.as_str(),
                                    rows.get(index).cloned(),
                                    separate_user_column,
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_table_header(
        &self,
        separate_user_column: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_PROCESS_TABLE_HEADER_HEIGHT))
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
                    .child(host_process_identity_header_label(
                        &self.i18n,
                        separate_user_column,
                    )),
            )
            .when(separate_user_column, |header| {
                header.child(
                    div()
                        .flex_none()
                        .w(px(HOST_PROCESS_USER_COLUMN_WIDTH))
                        .truncate()
                        .child(self.i18n.t("sidebar.host_processes.sort.user")),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_PID_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_processes.sort.pid")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_CPU_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_processes.sort.cpu")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_MEMORY_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_processes.sort.memory")),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_row(
        &self,
        connection_id: &str,
        process: Option<ResourceTopProcess>,
        separate_user_column: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(process) = process else {
            return div().into_any_element();
        };
        let expanded = self.connection_monitor.host_process_expanded_pid.as_deref()
            == Some(process.pid.as_str());
        let theme = self.tokens.ui;
        let status = process
            .state
            .as_deref()
            .map(|state| self.i18n.t(process_state_label_key(state)))
            .unwrap_or_else(|| self.i18n.t("sidebar.host_processes.unknown"));
        let user = process
            .user
            .clone()
            .unwrap_or_else(|| self.i18n.t("sidebar.host_processes.unknown"));
        let cpu = process
            .cpu_percent
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(|| "—".to_string());
        let memory = format!("{:.1}%", process.memory_percent);
        let cpu_color = threshold_color(process.cpu_percent);
        let memory_color = threshold_color(Some(process.memory_percent));
        let mono_font = settings_mono_font_family(self.settings_store.settings());

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
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(process_display_name(&process)),
                    )
                    .when(!separate_user_column, |main| {
                        main.child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(user.clone()),
                        )
                    })
                    .when(separate_user_column, |main| {
                        main.child(
                            div()
                                .flex_none()
                                .w(px(HOST_PROCESS_USER_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(user.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PROCESS_PID_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(process.pid.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PROCESS_CPU_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(cpu_color))
                            .font_family(mono_font.clone())
                            .child(cpu),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PROCESS_MEMORY_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(memory_color))
                            .font_family(mono_font.clone())
                            .child(memory),
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
                    // Keep actions visible without stealing the btop-like
                    // Program/User/PID/CPU/Mem columns in the narrow sidebar.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(format!("{status} · {}", process_display_command(&process))),
                    )
                    .child(self.render_host_process_inline_actions(connection_id, &process, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_process_detail(connection_id, &process, cx))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let pid = process.pid.clone();
                    move |this, _event, _window, cx| {
                        if this.connection_monitor.host_process_expanded_pid.as_deref()
                            == Some(pid.as_str())
                        {
                            this.connection_monitor.host_process_expanded_pid = None;
                        } else {
                            this.connection_monitor.host_process_expanded_pid = Some(pid.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_inline_actions(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self
            .connection_monitor
            .host_process_action_running
            .as_ref()
            .is_some_and(|request| request.pid == process.pid);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.render_host_process_action_button(
                connection_id,
                process,
                ProcessActionKind::Term,
                LucideIcon::Power,
                "sidebar.host_processes.actions.term",
                false,
                is_running,
                cx,
            ))
            .child(self.render_host_process_action_button(
                connection_id,
                process,
                ProcessActionKind::Kill,
                LucideIcon::Zap,
                "sidebar.host_processes.actions.kill",
                true,
                is_running,
                cx,
            ))
            .child(self.render_host_process_action_button(
                connection_id,
                process,
                ProcessActionKind::Stop,
                LucideIcon::Pause,
                "sidebar.host_processes.actions.stop",
                false,
                is_running,
                cx,
            ))
            .child(self.render_host_process_action_button(
                connection_id,
                process,
                ProcessActionKind::Cont,
                LucideIcon::Play,
                "sidebar.host_processes.actions.cont",
                false,
                is_running,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_process_detail(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                "PPID",
                process.ppid.clone().unwrap_or_else(|| "—".to_string()),
            ))
            .child(
                self.render_host_process_detail_line(
                    "RSS",
                    process
                        .rss_bytes
                        .map(format_bytes)
                        .unwrap_or_else(|| "—".to_string()),
                ),
            )
            .child(
                self.render_host_process_detail_line(
                    "VSZ",
                    process
                        .vsz_bytes
                        .map(format_bytes)
                        .unwrap_or_else(|| "—".to_string()),
                ),
            )
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_processes.elapsed"),
                process.elapsed.clone().unwrap_or_else(|| "—".to_string()),
            ))
            .child(self.render_host_process_action_bar(connection_id, process, cx))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font)
                    .text_color(rgb(theme.text))
                    .child(process_display_command(process)),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_detail_line(
        &self,
        label: impl Into<String>,
        value: String,
    ) -> AnyElement {
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .child(div().flex_none().child(label.into()))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(mono_font)
                    .child(value),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_action_bar(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let is_running = self
            .connection_monitor
            .host_process_action_running
            .as_ref()
            .is_some_and(|request| request.pid == process.pid);
        div()
            .mt_2()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("sidebar.host_processes.actions.renice")),
                    )
                    .child(self.render_host_process_renice_input(cx))
                    .child(self.render_host_process_action_button(
                        connection_id,
                        process,
                        ProcessActionKind::Renice {
                            nice: self.host_process_renice_value(),
                        },
                        LucideIcon::Gauge,
                        "sidebar.host_processes.actions.apply",
                        false,
                        is_running,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_host_process_action_button(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        action: ProcessActionKind,
        icon: LucideIcon,
        label_key: &'static str,
        danger: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key);
        let unsupported = self
            .host_process_action_command(connection_id, &process.pid, action.clone())
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
            "host-process-action",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let pid = process.pid.clone();
                let command = process_display_name(process);
                move |this, _event, _window, cx| {
                    this.request_host_process_action(
                        connection_id.clone(),
                        pid.clone(),
                        command.clone(),
                        action.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_host_process_renice_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostProcessRenice;
        let focused = self.connection_monitor.host_process_renice_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_process_renice_value,
                    placeholder: self
                        .i18n
                        .t("sidebar.host_processes.actions.renice_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(54.0))
            .h(px(26.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = true;
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

    pub(super) fn host_process_action_command(
        &self,
        connection_id: &str,
        pid: &str,
        action: ProcessActionKind,
    ) -> Result<oxideterm_connection_monitor::ProcessActionCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_process_action_command(&os_type, pid, action)
    }

    pub(super) fn visible_host_process_rows(
        &self,
        processes: &[ResourceTopProcess],
    ) -> Vec<ResourceTopProcess> {
        visible_process_rows(
            processes,
            &self.connection_monitor.host_process_search_query,
            self.connection_monitor.host_process_filter,
            self.connection_monitor.host_process_sort,
            self.connection_monitor.host_process_sort_descending,
        )
    }

    pub(super) fn sync_host_process_list_state(
        &self,
        rows: &[ResourceTopProcess],
        selected_id: &str,
    ) {
        let signatures = rows.iter().map(process_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-processes:{selected_id}:{}:{}:{}:{}",
            self.connection_monitor.host_process_search_query,
            self.connection_monitor.host_process_filter as u8,
            self.connection_monitor.host_process_sort as u8,
            self.connection_monitor.host_process_sort_descending
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_process_list_state,
            &mut self.connection_monitor.host_process_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(in crate::workspace) fn handle_host_process_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_process_search_focused
            && !self.connection_monitor.host_process_renice_focused
        {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_process_search_focused = false;
            self.connection_monitor.host_process_renice_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn host_process_renice_value(&self) -> i32 {
        self.connection_monitor
            .host_process_renice_value
            .trim()
            .parse::<i32>()
            .unwrap_or(0)
            .clamp(-20, 19)
    }

    pub(super) fn request_host_process_action(
        &mut self,
        connection_id: String,
        pid: String,
        command: String,
        action: ProcessActionKind,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_process_action_running
            .is_some()
        {
            self.push_host_process_toast(
                self.i18n
                    .t("sidebar.host_processes.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        if let ProcessActionKind::Renice { nice } = action
            && !(-20..=19).contains(&nice)
        {
            self.push_host_process_toast(
                self.i18n.t("sidebar.host_processes.toast.invalid_nice"),
                TerminalNoticeVariant::Error,
            );
            return;
        }
        self.connection_monitor.host_process_pending_confirm = Some(HostProcessActionRequest {
            connection_id,
            pid,
            command,
            action,
        });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn handle_host_process_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .connection_monitor
            .host_process_pending_confirm
            .is_none()
        {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.connection_monitor.host_process_pending_confirm = None;
                self.clear_standard_confirm_focus();
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_process_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn confirm_host_process_action(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.connection_monitor.host_process_pending_confirm.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        self.start_host_process_action(request, cx);
    }

    pub(super) fn start_host_process_action(
        &mut self,
        request: HostProcessActionRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            self.push_host_process_toast(
                self.i18n
                    .t("sidebar.host_processes.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Unknown".to_string());
        let command =
            match build_process_action_command(&os_type, &request.pid, request.action.clone()) {
                Ok(command) => command,
                Err(error) => {
                    self.push_host_process_toast(error, TerminalNoticeVariant::Error);
                    cx.notify();
                    return;
                }
            };
        if command.capability == ProcessCommandCapability::Partial {
            self.push_host_process_toast(
                self.i18n_replace(
                    "sidebar.host_processes.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let delivery_request = request.clone();
        self.connection_monitor.host_process_action_running = Some(request);
        self.connection_monitor.host_process_action_rx = Some(rx);
        self.connection_monitor.host_process_action_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_PROCESS_ACTION_TIMEOUT,
                    HOST_PROCESS_ACTION_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostProcessActionDelivery {
                request: delivery_request,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::workspace) fn poll_host_process_action_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_monitor.host_process_action_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_process_action_rx.take() else {
            self.connection_monitor.host_process_action_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_process_action_polling = false;
                self.connection_monitor.host_process_action_running = None;
                self.finish_host_process_action(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_process_action_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_process_action_polling = false;
                self.connection_monitor.host_process_action_running = None;
                self.push_host_process_toast(
                    self.i18n.t("sidebar.host_processes.toast.action_failed"),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
            }
        }
    }

    pub(super) fn finish_host_process_action(
        &mut self,
        delivery: HostProcessActionDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery.result {
            Ok(output) => match interpret_process_action_output(
                &output.stdout,
                &output.stderr,
                output.exit_code,
            ) {
                HostToolActionOutcome::Succeeded { message } => {
                    self.push_host_process_toast(message, TerminalNoticeVariant::Success);
                }
                HostToolActionOutcome::Failed { message } => {
                    self.push_host_process_toast(message, TerminalNoticeVariant::Error);
                }
            },
            Err(error) => {
                self.push_host_process_toast(error, TerminalNoticeVariant::Error);
            }
        }
        self.connection_monitor
            .profiler_registry
            .stop(&delivery.request.connection_id);
        self.start_connection_monitor_profiler(delivery.request.connection_id, cx);
    }

    pub(super) fn push_host_process_toast(
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

    pub(in crate::workspace) fn render_host_process_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let request = self
            .connection_monitor
            .host_process_pending_confirm
            .as_ref()?;
        let title = self.i18n.t("sidebar.host_processes.confirm.title");
        let description = self.i18n_replace(
            host_process_confirm_description_key(&request.action),
            &[
                ("pid", request.pid.clone()),
                ("command", request.command.clone()),
            ],
        );
        Some(
            confirm_dialog_with_focus(
                &self.tokens,
                ConfirmDialogView {
                    variant: if matches!(request.action, ProcessActionKind::Kill) {
                        ConfirmDialogVariant::Danger
                    } else {
                        ConfirmDialogVariant::Default
                    },
                    title: div().child(title).into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("sidebar.host_processes.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t(host_process_confirm_label_key(&request.action)))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.connection_monitor.host_process_pending_confirm = None;
                    this.clear_standard_confirm_focus();
                    cx.notify();
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_host_process_action(cx);
                }),
            )
            .into_any_element(),
        )
    }
}

fn host_process_confirm_description_key(action: &ProcessActionKind) -> &'static str {
    match action {
        ProcessActionKind::Term => "sidebar.host_processes.confirm.term_desc",
        ProcessActionKind::Kill => "sidebar.host_processes.confirm.kill_desc",
        ProcessActionKind::Stop => "sidebar.host_processes.confirm.stop_desc",
        ProcessActionKind::Cont => "sidebar.host_processes.confirm.cont_desc",
        ProcessActionKind::Renice { .. } => "sidebar.host_processes.confirm.renice_desc",
    }
}

fn host_process_confirm_label_key(action: &ProcessActionKind) -> &'static str {
    match action {
        ProcessActionKind::Term => "sidebar.host_processes.actions.term",
        ProcessActionKind::Kill => "sidebar.host_processes.actions.kill",
        ProcessActionKind::Stop => "sidebar.host_processes.actions.stop",
        ProcessActionKind::Cont => "sidebar.host_processes.actions.cont",
        ProcessActionKind::Renice { .. } => "sidebar.host_processes.actions.apply",
    }
}
