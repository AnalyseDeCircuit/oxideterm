//! Owns the scheduled tasks Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::{
    ScheduledTaskToggleAction, scheduled_task_action_availability, scheduled_task_capture_snapshot,
};

impl WorkspaceApp {
    pub(super) fn render_host_schedules_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .host_schedule_snapshot
            .as_ref()
            .filter(|_| {
                self.connection_monitor
                    .host_schedule_snapshot_connection_id
                    .as_deref()
                    == Some(selected_id)
            });
        let rows = snapshot
            .map(|snapshot| {
                visible_scheduled_task_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_schedule_search_query,
                    self.connection_monitor.host_schedule_filter,
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_schedule_list_state(&rows, selected_id);

        div()
            .id("host-schedules-panel")
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
                        !self.connection_monitor.host_schedule_snapshot_polling,
                        cx,
                    ))
                    .child(self.render_host_schedule_search(cx))
                    .child(self.render_host_schedule_filter_row(cx))
                    .child(self.render_host_schedule_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        cx,
                    )),
            )
            .child(self.render_host_schedule_list(
                rows,
                self.connection_monitor.host_schedule_snapshot_polling,
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_schedule_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostScheduleSearch;
        let focused = self.connection_monitor.host_schedule_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_schedule_search_query,
                    placeholder: self.i18n.t("sidebar.host_schedules.search_placeholder"),
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
                    this.connection_monitor.host_schedule_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
                    this.connection_monitor.host_log_search_focused = false;
                    this.connection_monitor.host_tmux_search_focused = false;
                    this.connection_monitor.host_port_search_focused = false;
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

    pub(super) fn render_host_schedule_filter_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .id("host-schedule-filter-scroll")
            .flex()
            .items_center()
            .gap_1()
            .overflow_x_scroll();
        for filter in [
            ScheduledTaskFilter::All,
            ScheduledTaskFilter::Enabled,
            ScheduledTaskFilter::Disabled,
            ScheduledTaskFilter::Systemd,
            ScheduledTaskFilter::Cron,
            ScheduledTaskFilter::Launchd,
            ScheduledTaskFilter::Windows,
            ScheduledTaskFilter::Failed,
        ] {
            row = row.child(self.render_host_schedule_filter_chip(filter, cx));
        }
        row.into_any_element()
    }

    pub(super) fn render_host_schedule_filter_chip(
        &self,
        filter: ScheduledTaskFilter,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.connection_monitor.host_schedule_filter == filter;
        div()
            .flex_none()
            .h(px(24.0))
            .px_2()
            .flex()
            .items_center()
            .rounded(px(12.0))
            .cursor_pointer()
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .text_size(px(11.0))
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
            .child(self.i18n.t(scheduled_task_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_schedule_filter != filter {
                        this.connection_monitor.host_schedule_filter = filter;
                        this.connection_monitor.host_schedule_expanded_index = None;
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_schedule_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceScheduledTaskStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let capability_label = match status {
            ResourceScheduledTaskStatus::Available {
                capability: ScheduledTaskCapability::Full,
                ..
            } => self.i18n.t("sidebar.host_schedules.capability.full"),
            ResourceScheduledTaskStatus::Available {
                capability: ScheduledTaskCapability::Partial,
                ..
            } => self.i18n.t("sidebar.host_schedules.capability.partial"),
            _ => self.i18n.t("sidebar.host_schedules.capability.unknown"),
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
                self.i18n.t("sidebar.host_schedules.count_suffix"),
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
                        self.i18n.t("sidebar.host_schedules.actions.diagnostic"),
                        "host-schedule-diagnostic",
                        true,
                        cx.listener({
                            let selected_id = selected_id.clone();
                            move |this, _event, window, cx| {
                                this.open_host_schedule_diagnostic_terminal(
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
                            disabled: self.connection_monitor.host_schedule_snapshot_polling,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        self.i18n.t("sidebar.host_schedules.actions.refresh"),
                        "host-schedule-refresh",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.request_host_schedules_snapshot(
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

    pub(super) fn render_host_schedule_list(
        &self,
        rows: Vec<ResourceScheduledTask>,
        loading: bool,
        status: ResourceScheduledTaskStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Clock,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_schedules.loading"),
                cx,
            );
        }
        match status {
            ResourceScheduledTaskStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Clock,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_schedules.unavailable"),
                    cx,
                );
            }
            ResourceScheduledTaskStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_schedules.error", &[("error", message)]),
                    cx,
                );
            }
            ResourceScheduledTaskStatus::Unknown
            | ResourceScheduledTaskStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Clock,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_schedules.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.connection_monitor.host_schedule_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_SCHEDULE_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        let show_context_columns =
            self.ai.chat.sidebar_width >= HOST_SCHEDULE_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_schedule_table_header(show_context_columns))
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
                                this.render_host_schedule_row(
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

    pub(super) fn render_host_schedule_table_header(
        &self,
        show_context_columns: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_SCHEDULE_TABLE_HEADER_HEIGHT))
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
                    .child(self.i18n.t("sidebar.host_schedules.columns.task")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SCHEDULE_SOURCE_COLUMN_WIDTH))
                    .truncate()
                    .child(self.i18n.t("sidebar.host_schedules.columns.source")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SCHEDULE_STATE_COLUMN_WIDTH))
                    .truncate()
                    .child(self.i18n.t("sidebar.host_schedules.columns.state")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SCHEDULE_ENABLED_COLUMN_WIDTH))
                    .truncate()
                    .child(self.i18n.t("sidebar.host_schedules.columns.enabled")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SCHEDULE_NEXT_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_schedules.columns.next")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SCHEDULE_LAST_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_schedules.columns.last")),
                    )
            })
            .into_any_element()
    }

    pub(super) fn render_host_schedule_row(
        &self,
        connection_id: &str,
        index: usize,
        entry: Option<ResourceScheduledTask>,
        show_context_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.connection_monitor.host_schedule_expanded_index == Some(index);
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let source = host_schedule_source_display(&self.i18n, &entry.source);
        let active = host_schedule_active_display(&self.i18n, &entry.active);
        let enabled = host_schedule_enabled_display(&self.i18n, &entry.enabled);
        let next = host_schedule_blank_dash(&entry.next_run);
        let last = host_schedule_blank_dash(&entry.last_run);

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
                    .h(px(HOST_SCHEDULE_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // The task name is the identity column. Keep it as the
                    // first-level flex child so fixed metadata/actions cannot
                    // collapse it during right-sidebar resizing.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(host_schedule_blank_dash(&entry.name)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SCHEDULE_SOURCE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(source.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SCHEDULE_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_schedule_active_color(
                                &entry.active,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(active.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SCHEDULE_ENABLED_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_schedule_enabled_color(
                                &entry.enabled,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(enabled.clone()),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_SCHEDULE_NEXT_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(next.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_SCHEDULE_LAST_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(last.clone()),
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
                                    self.i18n.t("sidebar.host_schedules.columns.schedule"),
                                    host_schedule_blank_dash(&entry.schedule)
                                )
                            } else {
                                format!(
                                    "{} · {} · {}",
                                    source,
                                    next,
                                    host_schedule_blank_dash(&entry.command)
                                )
                            }),
                    )
                    .child(self.render_host_schedule_inline_actions(connection_id, &entry, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_schedule_detail(&entry))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_schedule_expanded_index == Some(index) {
                        this.connection_monitor.host_schedule_expanded_index = None;
                    } else {
                        this.connection_monitor.host_schedule_expanded_index = Some(index);
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_schedule_inline_actions(
        &self,
        connection_id: &str,
        entry: &ResourceScheduledTask,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let logs_task = entry.clone();
        let follow_task = entry.clone();
        let run_task = entry.clone();
        let toggle_task = entry.clone();
        let availability = scheduled_task_action_availability(entry);
        let can_run_now = availability.can_run_now;
        let can_toggle_enabled = availability.can_toggle_enabled;
        let should_enable = matches!(availability.next_toggle, ScheduledTaskToggleAction::Enable);
        let action_running = self
            .connection_monitor
            .host_schedule_action_running
            .as_ref()
            .is_some_and(|request| request.task_id == entry.id);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::FileText,
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
                self.i18n.t("sidebar.host_schedules.actions.logs"),
                "host-schedule-logs",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |this, _event, _window, cx| {
                        this.request_host_schedule_logs(
                            connection_id.clone(),
                            logs_task.clone(),
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::Activity,
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
                self.i18n.t("sidebar.host_schedules.actions.follow_logs"),
                "host-schedule-follow",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |this, _event, window, cx| {
                        this.open_host_schedule_follow_terminal(
                            connection_id.clone(),
                            follow_task.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::Play,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: !can_run_now || action_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: if can_run_now && !action_running {
                        1.0
                    } else {
                        0.45
                    },
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                self.i18n.t("sidebar.host_schedules.actions.run_now"),
                "host-schedule-run-now",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |this, _event, _window, cx| {
                        if can_run_now {
                            this.request_host_schedule_run_now(
                                connection_id.clone(),
                                run_task.clone(),
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .child(self.workspace_tooltip_icon_button(
                if should_enable {
                    LucideIcon::CheckCircle
                } else {
                    LucideIcon::ShieldOff
                },
                12.0,
                rgb(if should_enable {
                    theme.text
                } else {
                    MONITOR_RED
                }),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: !can_toggle_enabled || action_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: if can_toggle_enabled && !action_running {
                        1.0
                    } else {
                        0.45
                    },
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                self.i18n.t(if should_enable {
                    "sidebar.host_schedules.actions.enable"
                } else {
                    "sidebar.host_schedules.actions.disable"
                }),
                "host-schedule-toggle-enabled",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |this, _event, _window, cx| {
                        if can_toggle_enabled && !action_running {
                            this.request_host_schedule_toggle_enabled(
                                connection_id.clone(),
                                toggle_task.clone(),
                                should_enable,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(super) fn render_host_schedule_detail(&self, entry: &ResourceScheduledTask) -> AnyElement {
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
                    .min_w(px(640.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .font_family(mono_font)
                    .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.task"),
                        host_schedule_blank_dash(&entry.name)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.source"),
                        host_schedule_source_display(&self.i18n, &entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.state"),
                        host_schedule_active_display(&self.i18n, &entry.active)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.enabled"),
                        host_schedule_enabled_display(&self.i18n, &entry.enabled)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.next"),
                        host_schedule_blank_dash(&entry.next_run)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.last"),
                        host_schedule_blank_dash(&entry.last_run)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.result"),
                        host_schedule_blank_dash(&entry.last_result)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.user"),
                        host_schedule_blank_dash(&entry.user)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.unit"),
                        host_schedule_blank_dash(&entry.unit)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.schedule"),
                        host_schedule_blank_dash(&entry.schedule)
                    )))
                    .child(div().whitespace_nowrap().child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.command"),
                        host_schedule_blank_dash(&entry.command)
                    )))
                    .child(div().whitespace_nowrap().child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_schedules.columns.description"),
                        host_schedule_blank_dash(&entry.description)
                    ))),
            )
            .into_any_element()
    }

    pub(super) fn sync_host_schedule_list_state(
        &self,
        rows: &[ResourceScheduledTask],
        selected_id: &str,
    ) {
        let signatures = rows
            .iter()
            .map(scheduled_task_row_signature)
            .collect::<Vec<_>>();
        let identity = format!(
            "host-schedules:{selected_id}:{}:{}:{}",
            self.connection_monitor.host_schedule_search_query,
            self.connection_monitor.host_schedule_filter as u8,
            self.connection_monitor
                .host_schedule_expanded_index
                .unwrap_or(usize::MAX)
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_schedule_list_state,
            &mut self
                .connection_monitor
                .host_schedule_list_cache
                .borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_SCHEDULE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn host_schedule_snapshot_command(
        &self,
        connection_id: &str,
    ) -> (
        oxideterm_connection_monitor::ScheduledTaskCaptureCommand,
        String,
    ) {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        (build_scheduled_task_snapshot_command(&os_type), os_type)
    }

    pub(super) fn host_schedule_logs_command(
        &self,
        connection_id: &str,
        task: &ResourceScheduledTask,
        follow: bool,
        limit: usize,
    ) -> Result<
        (
            oxideterm_connection_monitor::ScheduledTaskCaptureCommand,
            String,
        ),
        String,
    > {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_scheduled_task_logs_command(&os_type, task, follow, limit)
            .map(|command| (command, os_type))
    }

    pub(super) fn host_schedule_action_command(
        &self,
        connection_id: &str,
        action: ScheduledTaskActionKind,
    ) -> Result<
        (
            oxideterm_connection_monitor::ScheduledTaskActionCommand,
            String,
        ),
        String,
    > {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_scheduled_task_action_command(&os_type, action).map(|command| (command, os_type))
    }

    pub(super) fn host_schedule_diagnostic_command(&self, connection_id: &str) -> (String, String) {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        (build_scheduled_task_diagnostic_command(&os_type), os_type)
    }

    pub(in crate::workspace) fn handle_host_schedule_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_schedule_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_schedule_search_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn request_host_schedules_snapshot_for_selected_connection(
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
        self.request_host_schedules_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_host_schedules_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_schedule_snapshot_polling {
            if feedback.should_toast() {
                self.push_host_schedule_toast(
                    self.i18n
                        .t("sidebar.host_schedules.toast.snapshot_already_running"),
                    TerminalNoticeVariant::Warning,
                );
            }
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            if feedback.should_toast() {
                self.push_host_schedule_toast(
                    self.i18n
                        .t("sidebar.host_schedules.toast.connection_missing"),
                    TerminalNoticeVariant::Error,
                );
            }
            cx.notify();
            return;
        };
        let (command, os_type) = self.host_schedule_snapshot_command(&connection_id);
        if feedback.should_toast() && command.capability == ScheduledTaskCapability::Partial {
            self.push_host_schedule_toast(
                self.i18n_replace(
                    "sidebar.host_schedules.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let request = HostScheduleSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_schedule_snapshot_connection_id = Some(connection_id);
        self.connection_monitor.host_schedule_snapshot_running = Some(request.clone());
        self.connection_monitor.host_schedule_snapshot_rx = Some(rx);
        self.connection_monitor.host_schedule_snapshot_polling = true;
        self.connection_monitor.host_schedule_last_error = None;
        // Scheduled tasks are inventory data, not high-frequency metrics.
        // Keep the sampler out of the profiler loop to avoid expensive cron/systemd scans.
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_SCHEDULE_SNAPSHOT_TIMEOUT,
                    HOST_SCHEDULE_SNAPSHOT_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostScheduleSnapshotDelivery { request, result });
        });
        cx.notify();
    }

    pub(super) fn request_host_schedule_logs(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_schedule_logs_polling {
            self.push_host_schedule_toast(
                self.i18n
                    .t("sidebar.host_schedules.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            self.push_host_schedule_toast(
                self.i18n
                    .t("sidebar.host_schedules.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let (command, os_type) =
            match self.host_schedule_logs_command(&connection_id, &task, false, 200) {
                Ok(command) => command,
                Err(error) => {
                    self.push_host_schedule_toast(error, TerminalNoticeVariant::Error);
                    cx.notify();
                    return;
                }
            };
        if command.capability == ScheduledTaskCapability::Partial {
            self.push_host_schedule_toast(
                self.i18n_replace(
                    "sidebar.host_schedules.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }
        let request = HostScheduleLogsRequest {
            connection_id,
            task,
        };
        self.connection_monitor.host_schedule_logs_dialog = Some(HostScheduleLogsDialog {
            request: request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_schedule_logs_rx = Some(rx);
        self.connection_monitor.host_schedule_logs_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_SCHEDULE_LOGS_TIMEOUT,
                    HOST_SCHEDULE_LOGS_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostScheduleLogsDelivery { request, result });
        });
        cx.notify();
    }

    pub(super) fn request_host_schedule_run_now(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_schedule_action_running
            .is_some()
        {
            self.push_host_schedule_toast(
                self.i18n
                    .t("sidebar.host_schedules.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        self.connection_monitor.host_schedule_pending_confirm = Some(HostScheduleActionRequest {
            connection_id,
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            unit: task.unit.clone(),
            action: ScheduledTaskActionKind::RunNow {
                id: task.id,
                unit: task.unit,
            },
        });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(super) fn request_host_schedule_toggle_enabled(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        enable: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_schedule_action_running
            .is_some()
        {
            self.push_host_schedule_toast(
                self.i18n
                    .t("sidebar.host_schedules.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        let action = if enable {
            ScheduledTaskActionKind::Enable {
                id: task.id.clone(),
                source: task.source.clone(),
            }
        } else {
            ScheduledTaskActionKind::Disable {
                id: task.id.clone(),
                source: task.source.clone(),
            }
        };
        self.connection_monitor.host_schedule_pending_confirm = Some(HostScheduleActionRequest {
            connection_id,
            task_id: task.id,
            task_name: task.name,
            unit: task.unit,
            action,
        });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(super) fn open_host_schedule_follow_terminal(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (command, os_type) =
            match self.host_schedule_logs_command(&connection_id, &task, true, 200) {
                Ok(command) => command,
                Err(error) => {
                    self.push_host_schedule_toast(error, TerminalNoticeVariant::Error);
                    cx.notify();
                    return;
                }
            };
        if command.capability == ScheduledTaskCapability::Partial {
            self.push_host_schedule_toast(
                self.i18n_replace(
                    "sidebar.host_schedules.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }
        let title = self.i18n_replace(
            "sidebar.host_schedules.follow_title",
            &[("name", task.name.clone())],
        );
        self.open_host_schedule_terminal_command(
            connection_id,
            task.name,
            command.command,
            title,
            "sidebar.host_schedules.toast.follow_opened",
            window,
            cx,
        );
    }

    pub(super) fn open_host_schedule_diagnostic_terminal(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (command, _os_type) = self.host_schedule_diagnostic_command(&connection_id);
        let title = self.i18n.t("sidebar.host_schedules.diagnostic_title");
        self.open_host_schedule_terminal_command(
            connection_id,
            self.i18n.t("sidebar.host_schedules.diagnostic_title"),
            command,
            title,
            "sidebar.host_schedules.toast.diagnostic_opened",
            window,
            cx,
        );
    }

    pub(super) fn open_host_schedule_terminal_command(
        &mut self,
        connection_id: String,
        name: String,
        command: String,
        title: String,
        opened_toast_key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_schedule_toast(
                self.i18n
                    .t("sidebar.host_schedules.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_schedule_toast(
                self.i18n
                    .t("sidebar.host_schedules.toast.exec_terminal_missing"),
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
            Ok(()) => self.push_host_schedule_toast(
                self.i18n_replace(opened_toast_key, &[("name", name)]),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_schedule_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_host_schedule_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .connection_monitor
            .host_schedule_pending_confirm
            .is_none()
        {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.connection_monitor.host_schedule_pending_confirm = None;
                self.clear_standard_confirm_focus();
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_schedule_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn confirm_host_schedule_action(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.connection_monitor.host_schedule_pending_confirm.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        self.start_host_schedule_action(request, cx);
    }

    pub(super) fn start_host_schedule_action(
        &mut self,
        request: HostScheduleActionRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            self.push_host_schedule_toast(
                self.i18n
                    .t("sidebar.host_schedules.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let (command, os_type) = match self
            .host_schedule_action_command(&request.connection_id, request.action.clone())
        {
            Ok(command) => command,
            Err(error) => {
                self.push_host_schedule_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == ScheduledTaskCapability::Partial {
            self.push_host_schedule_toast(
                self.i18n_replace(
                    "sidebar.host_schedules.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let delivery_request = request.clone();
        self.connection_monitor.host_schedule_action_running = Some(request);
        self.connection_monitor.host_schedule_action_rx = Some(rx);
        self.connection_monitor.host_schedule_action_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_SCHEDULE_ACTION_TIMEOUT,
                    HOST_SCHEDULE_ACTION_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostScheduleActionDelivery {
                request: delivery_request,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::workspace) fn poll_host_schedules_snapshot_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_monitor.host_schedule_snapshot_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_schedule_snapshot_rx.take() else {
            self.connection_monitor.host_schedule_snapshot_polling = false;
            self.connection_monitor.host_schedule_snapshot_running = None;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.finish_host_schedules_snapshot(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_schedule_snapshot_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let feedback = self
                    .connection_monitor
                    .host_schedule_snapshot_running
                    .as_ref()
                    .map(|request| request.feedback)
                    .unwrap_or(HostSnapshotFeedback::Silent);
                self.connection_monitor.host_schedule_snapshot_polling = false;
                self.connection_monitor.host_schedule_snapshot_running = None;
                let reason = self.i18n.t("sidebar.host_schedules.toast.unknown_error");
                self.connection_monitor.host_schedule_last_error = Some(reason.clone());
                if feedback.should_toast() {
                    self.push_host_schedule_toast(
                        self.i18n_replace(
                            "sidebar.host_schedules.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn poll_host_schedule_logs_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_schedule_logs_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_schedule_logs_rx.take() else {
            self.connection_monitor.host_schedule_logs_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_schedule_logs_polling = false;
                self.finish_host_schedule_logs(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_schedule_logs_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_schedule_logs_polling = false;
                if let Some(dialog) = self.connection_monitor.host_schedule_logs_dialog.as_mut() {
                    dialog.loading = false;
                    dialog.error = Some(self.i18n.t("sidebar.host_schedules.toast.logs_failed"));
                }
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn poll_host_schedule_action_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_monitor.host_schedule_action_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_schedule_action_rx.take() else {
            self.connection_monitor.host_schedule_action_polling = false;
            self.connection_monitor.host_schedule_action_running = None;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_schedule_action_polling = false;
                self.connection_monitor.host_schedule_action_running = None;
                self.finish_host_schedule_action(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_schedule_action_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_schedule_action_polling = false;
                self.connection_monitor.host_schedule_action_running = None;
                self.push_host_schedule_toast(
                    self.i18n.t("sidebar.host_schedules.toast.action_failed"),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
            }
        }
    }

    pub(super) fn finish_host_schedules_snapshot(
        &mut self,
        delivery: HostScheduleSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_schedule_snapshot_running
            .as_ref()
            .is_some_and(|running| running != &delivery.request)
        {
            cx.notify();
            return;
        }
        let feedback = delivery.request.feedback;
        self.connection_monitor.host_schedule_snapshot_polling = false;
        self.connection_monitor.host_schedule_snapshot_running = None;
        self.connection_monitor.host_schedule_snapshot_rx = None;
        match delivery.result {
            Ok(output) => {
                let snapshot = scheduled_task_capture_snapshot(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                );
                let visible_count = visible_scheduled_task_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_schedule_search_query,
                    self.connection_monitor.host_schedule_filter,
                )
                .len();
                match &snapshot.status {
                    ResourceScheduledTaskStatus::Available { .. } => {
                        self.connection_monitor.host_schedule_last_error = None;
                        if feedback.should_toast() {
                            self.push_host_schedule_toast(
                                self.i18n_replace(
                                    "sidebar.host_schedules.toast.snapshot_loaded",
                                    &[("count", visible_count.to_string())],
                                ),
                                TerminalNoticeVariant::Success,
                            );
                        }
                    }
                    ResourceScheduledTaskStatus::Unavailable => {
                        self.connection_monitor.host_schedule_last_error =
                            Some(self.i18n.t("sidebar.host_schedules.unavailable"));
                        if feedback.should_toast() {
                            self.push_host_schedule_toast(
                                self.i18n.t("sidebar.host_schedules.toast.unavailable"),
                                TerminalNoticeVariant::Warning,
                            );
                        }
                    }
                    ResourceScheduledTaskStatus::Error { message } => {
                        self.connection_monitor.host_schedule_last_error = Some(message.clone());
                        if feedback.should_toast() {
                            self.push_host_schedule_toast(
                                self.i18n_replace(
                                    "sidebar.host_schedules.toast.snapshot_failed",
                                    &[("reason", message.clone())],
                                ),
                                TerminalNoticeVariant::Error,
                            );
                        }
                    }
                    ResourceScheduledTaskStatus::Unknown => {}
                }
                self.connection_monitor.host_schedule_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_schedule_snapshot = Some(snapshot);
            }
            Err(error) => {
                self.connection_monitor.host_schedule_last_error = Some(error.clone());
                self.connection_monitor.host_schedule_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_schedule_snapshot =
                    Some(ResourceScheduledTaskSnapshot {
                        status: ResourceScheduledTaskStatus::Error {
                            message: error.clone(),
                        },
                        entries: Vec::new(),
                    });
                if feedback.should_toast() {
                    self.push_host_schedule_toast(
                        self.i18n_replace(
                            "sidebar.host_schedules.toast.snapshot_failed",
                            &[("reason", error)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
            }
        }
        cx.notify();
    }

    pub(super) fn finish_host_schedule_logs(
        &mut self,
        delivery: HostScheduleLogsDelivery,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .connection_monitor
            .host_schedule_logs_dialog
            .as_mut()
            .filter(|dialog| dialog.request == delivery.request)
        else {
            cx.notify();
            return;
        };
        dialog.loading = false;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let logs = if output.stdout.trim().is_empty() {
                    self.i18n.t("sidebar.host_schedules.logs.empty")
                } else {
                    output.stdout
                };
                dialog.output = Some(logs);
                dialog.error = None;
            }
            Ok(output) => {
                dialog.output = None;
                dialog.error = Some(host_tool_capture_failure_message(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                    &self.i18n.t("sidebar.host_schedules.toast.unknown_error"),
                ));
            }
            Err(error) => {
                dialog.output = None;
                dialog.error = Some(error);
            }
        }
        cx.notify();
    }

    pub(super) fn finish_host_schedule_action(
        &mut self,
        delivery: HostScheduleActionDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery.result {
            Ok(output) => {
                let success_message = self.i18n_replace(
                    host_schedule_action_success_key(&delivery.request.action),
                    &[("name", delivery.request.task_name.clone())],
                );
                match interpret_scheduled_task_action_output(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                    success_message,
                    &self.i18n.t("sidebar.host_schedules.toast.unknown_error"),
                ) {
                    HostToolActionOutcome::Succeeded { message } => {
                        self.push_host_schedule_toast(message, TerminalNoticeVariant::Success);
                    }
                    HostToolActionOutcome::Failed { message } => {
                        self.push_host_schedule_toast(message, TerminalNoticeVariant::Error);
                    }
                }
            }
            Err(error) => {
                self.push_host_schedule_toast(error, TerminalNoticeVariant::Error);
            }
        }
        self.request_host_schedules_snapshot(
            delivery.request.connection_id,
            HostSnapshotFeedback::Silent,
            cx,
        );
    }

    pub(super) fn push_host_schedule_toast(
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

    pub(in crate::workspace) fn render_host_schedule_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let request = self
            .connection_monitor
            .host_schedule_pending_confirm
            .as_ref()?;
        let title = self.i18n.t("sidebar.host_schedules.confirm.title");
        let description = self.i18n_replace(
            host_schedule_confirm_description_key(&request.action),
            &[
                ("name", request.task_name.clone()),
                ("unit", host_schedule_blank_dash(&request.unit)),
            ],
        );
        Some(
            confirm_dialog_with_focus(
                &self.tokens,
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Default,
                    title: div().child(title).into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("sidebar.host_schedules.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(
                            self.i18n
                                .t(host_schedule_confirm_label_key(&request.action)),
                        )
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.connection_monitor.host_schedule_pending_confirm = None;
                    this.clear_standard_confirm_focus();
                    cx.notify();
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_host_schedule_action(cx);
                }),
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_host_schedule_logs_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.connection_monitor.host_schedule_logs_dialog.as_ref()?;
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let follow_connection_id = dialog.request.connection_id.clone();
        let follow_task = dialog.request.task.clone();
        let follow_logs_disabled = self
            .host_schedule_logs_command(&follow_connection_id, &follow_task, true, 200)
            .is_err()
            || self
                .node_router
                .node_id_for_connection(&follow_connection_id)
                .is_none();
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("sidebar.host_schedules.logs.loading"))
                .into_any_element()
        } else if let Some(error) = dialog.error.as_ref() {
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element()
        } else {
            let output = dialog.output.clone().unwrap_or_default();
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
                        .id(("host-schedule-log-line", index))
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
                        this.connection_monitor.host_schedule_logs_dialog = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .w(px(HOST_SCHEDULE_LOGS_DIALOG_WIDTH))
                        .max_h(px(HOST_SCHEDULE_LOGS_DIALOG_MAX_HEIGHT))
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
                                                    "sidebar.host_schedules.logs.title",
                                                    &[("name", dialog.request.task.name.clone())],
                                                )),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(dialog.request.task.id.clone()),
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
                                                .t("sidebar.host_schedules.actions.follow_logs"),
                                            "host-schedule-logs-follow",
                                            true,
                                            cx.listener({
                                                let connection_id = follow_connection_id.clone();
                                                let task = follow_task.clone();
                                                move |this, _event, window, cx| {
                                                    this.connection_monitor.host_schedule_logs_dialog =
                                                        None;
                                                    this.open_host_schedule_follow_terminal(
                                                        connection_id.clone(),
                                                        task.clone(),
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
                                            self.i18n.t("sidebar.host_schedules.logs.close"),
                                            "host-schedule-logs-close",
                                            true,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.connection_monitor.host_schedule_logs_dialog =
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
                                .id("host-schedule-logs-scroll")
                                .flex_1()
                                .min_h_0()
                                .max_h(px(HOST_SCHEDULE_LOGS_DIALOG_MAX_HEIGHT - 84.0))
                                .overflow_y_scroll()
                                .overflow_x_scrollbar()
                                .child(content),
                        ),
                ))
                .into_any_element(),
        )
    }
}

fn host_schedule_blank_dash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        "—".to_string()
    } else {
        trimmed.to_string()
    }
}

fn host_schedule_source_display(i18n: &I18n, source: &str) -> String {
    let key = scheduled_task_source_label_key(source);
    if key == "sidebar.host_schedules.sources.unknown" && !source.trim().is_empty() {
        source.trim().to_string()
    } else {
        i18n.t(key)
    }
}

fn host_schedule_enabled_display(i18n: &I18n, enabled: &str) -> String {
    let key = scheduled_task_enabled_label_key(enabled);
    if key == "sidebar.host_schedules.enabled.unknown" && !enabled.trim().is_empty() {
        enabled.trim().to_string()
    } else {
        i18n.t(key)
    }
}

fn host_schedule_active_display(i18n: &I18n, active: &str) -> String {
    let key = scheduled_task_active_label_key(active);
    if key == "sidebar.host_schedules.active.unknown" && !active.trim().is_empty() {
        active.trim().to_string()
    } else {
        i18n.t(key)
    }
}

fn host_schedule_active_color(active: &str, muted_color: u32) -> u32 {
    match active.trim().to_lowercase().as_str() {
        "active" | "running" | "loaded" | "ready" => MONITOR_EMERALD,
        "failed" | "error" => MONITOR_RED,
        "activating" | "waiting" | "queued" => MONITOR_AMBER,
        _ => muted_color,
    }
}

fn host_schedule_enabled_color(enabled: &str, muted_color: u32) -> u32 {
    match enabled.trim().to_lowercase().as_str() {
        "enabled" => MONITOR_EMERALD,
        "masked" => MONITOR_RED,
        "static" | "generated" | "indirect" | "transient" => MONITOR_AMBER,
        "disabled" => muted_color,
        _ => muted_color,
    }
}

fn host_schedule_confirm_description_key(action: &ScheduledTaskActionKind) -> &'static str {
    match action {
        ScheduledTaskActionKind::RunNow { .. } => "sidebar.host_schedules.confirm.run_now_desc",
        ScheduledTaskActionKind::Enable { .. } => "sidebar.host_schedules.confirm.enable_desc",
        ScheduledTaskActionKind::Disable { .. } => "sidebar.host_schedules.confirm.disable_desc",
    }
}

fn host_schedule_confirm_label_key(action: &ScheduledTaskActionKind) -> &'static str {
    match action {
        ScheduledTaskActionKind::RunNow { .. } => "sidebar.host_schedules.actions.run_now",
        ScheduledTaskActionKind::Enable { .. } => "sidebar.host_schedules.actions.enable",
        ScheduledTaskActionKind::Disable { .. } => "sidebar.host_schedules.actions.disable",
    }
}

fn host_schedule_action_success_key(action: &ScheduledTaskActionKind) -> &'static str {
    match action {
        ScheduledTaskActionKind::RunNow { .. } => "sidebar.host_schedules.toast.run_now_started",
        ScheduledTaskActionKind::Enable { .. } => "sidebar.host_schedules.toast.enable_succeeded",
        ScheduledTaskActionKind::Disable { .. } => "sidebar.host_schedules.toast.disable_succeeded",
    }
}
