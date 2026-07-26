//! Owns the scheduled tasks Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::{
    ScheduledTaskToggleAction, parse_scheduled_task_snapshot, scheduled_task_action_availability,
};

impl WorkspaceApp {
    pub(super) fn render_host_schedules_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let snapshot = self.host_tools.read(cx).schedule_snapshot_for(selected_id);
        let filter = self.host_tools.read(cx).schedule_filter();
        let schedule_search_query = self
            .host_tools
            .read(cx)
            .ui
            .host_schedule_search_query
            .clone();
        let rows = snapshot
            .as_ref()
            .map(|snapshot| {
                visible_scheduled_task_rows(&snapshot.entries, &schedule_search_query, filter)
            })
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_schedule_list_state(&rows, selected_id, cx);

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
                        !self.host_tools.read(cx).schedule_snapshot_polling(),
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
                self.host_tools.read(cx).schedule_snapshot_polling(),
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_schedule_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostScheduleSearch;
        let (focused, value) = {
            let ui = &self.host_tools.read(cx).ui;
            (
                ui.input_is_focused(HostToolsTextInput::ScheduleSearch),
                ui.host_schedule_search_query.clone(),
            )
        };
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder: self.i18n.t("sidebar.host_schedules.search_placeholder"),
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
                        host_tools
                            .ui
                            .focus_input(HostToolsTextInput::ScheduleSearch);
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
        let active = self.host_tools.read(cx).schedule_filter() == filter;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(scheduled_task_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.select_schedule_filter(filter, cx);
                    });
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
                            disabled: self.host_tools.read(cx).schedule_snapshot_polling(),
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
        let state = self.host_tools.read(cx).schedule_list_state();
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
        let expanded = self.host_tools.read(cx).schedule_expanded_index() == Some(index);
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
                            .child(active),
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
                            .child(enabled),
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
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.toggle_schedule_expanded(index, cx);
                    });
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
            .host_tools
            .read(cx)
            .schedule_action_running_for(&entry.id);
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
        cx: &mut Context<Self>,
    ) {
        let signatures = rows
            .iter()
            .map(scheduled_task_row_signature)
            .collect::<Vec<_>>();
        let identity = format!(
            "host-schedules:{selected_id}:{}:{}:{}",
            self.host_tools.read(cx).ui.host_schedule_search_query,
            self.host_tools.read(cx).schedule_filter() as u8,
            self.host_tools
                .read(cx)
                .schedule_expanded_index()
                .unwrap_or(usize::MAX)
        );
        self.host_tools
            .read(cx)
            .sync_schedule_list_signatures(&identity, &signatures);
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
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::ScheduleSearch)
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

    pub(super) fn request_host_schedules_snapshot_for_selected_connection(
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
        self.request_host_schedules_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(in crate::workspace) fn request_host_schedules_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        let monitoring_enabled = self.host_tool_monitoring_enabled(ContextSidebarTool::Schedules);
        let runtime = self.forwarding_runtime.handle().clone();
        let failure_fallback = self.i18n.t("sidebar.host_schedules.toast.unknown_error");
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_schedule_snapshot(
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

    pub(super) fn request_host_schedule_logs(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.handle().clone();
        let failure_fallback = self.i18n.t("sidebar.host_schedules.toast.logs_failed");
        let empty_fallback = self.i18n.t("sidebar.host_schedules.logs.empty");
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_schedule_logs(
                connection_id,
                task,
                runtime,
                failure_fallback,
                empty_fallback,
                cx,
            )
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    pub(super) fn request_host_schedule_run_now(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        cx: &mut Context<Self>,
    ) {
        let notice = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.open_schedule_action_confirm(
                HostScheduleActionRequest {
                    connection_id,
                    task_id: task.id.clone(),
                    task_name: task.name.clone(),
                    unit: task.unit.clone(),
                    action: ScheduledTaskActionKind::RunNow {
                        id: task.id,
                        unit: task.unit,
                    },
                },
                cx,
            )
        });
        if let Some(notice) = notice {
            self.push_host_tools_notice(notice);
            return;
        }
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
        let notice = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.open_schedule_action_confirm(
                HostScheduleActionRequest {
                    connection_id,
                    task_id: task.id,
                    task_name: task.name,
                    unit: task.unit,
                    action,
                },
                cx,
            )
        });
        if let Some(notice) = notice {
            self.push_host_tools_notice(notice);
            return;
        }
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
                    &[("os", os_type)],
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
        if !self.ssh_nodes.contains_key(&node_id) {
            self.push_host_schedule_toast(
                self.i18n
                    .t("sidebar.host_schedules.toast.exec_terminal_missing"),
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
        if self.host_tools.read(cx).schedule_confirm_view().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_schedule_confirm_exit(cx);
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
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_schedule_action(delay, runtime, cx)
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_schedule_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_schedule_confirm_exit(delay, cx)
        })
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
        let (request, phase) = self.host_tools.read(cx).schedule_confirm_view()?;
        let title = self.i18n.t("sidebar.host_schedules.confirm.title");
        let description = self.i18n_replace(
            host_schedule_confirm_description_key(&request.action),
            &[
                ("name", request.task_name.clone()),
                ("unit", host_schedule_blank_dash(&request.unit)),
            ],
        );
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "host-schedule-confirm-motion",
                phase,
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
                    this.begin_host_schedule_confirm_exit(cx);
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
        let dialog = self.host_tools.read(cx).schedule_logs_dialog()?;
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let follow_connection_id = dialog.request.connection_id.clone();
        // Rebuild only the task identity required by the command builder.
        // The sampled task command is never copied into the async log request.
        let follow_task = ResourceScheduledTask {
            id: dialog.request.task_id.clone(),
            name: dialog.request.task_name.clone(),
            source: dialog.request.task_source.clone(),
            schedule: String::new(),
            command: String::new(),
            user: String::new(),
            enabled: String::new(),
            active: String::new(),
            last_run: String::new(),
            next_run: String::new(),
            last_result: String::new(),
            description: String::new(),
            unit: dialog.request.task_unit.clone(),
        };
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
            // Per-line strings are the explicit GPUI output boundary and live
            // only in the current render tree; the retained capture stays shared.
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
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_schedule_logs_dialog(cx);
                        });
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
                                                    &[("name", dialog.request.task_name.clone())],
                                                )),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(dialog.request.task_id.clone()),
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
                                                let connection_id = follow_connection_id;
                                                let task = follow_task;
                                                move |this, _event, window, cx| {
                                                    this.host_tools.update(
                                                        cx,
                                                        |host_tools, cx| {
                                                            host_tools
                                                                .dismiss_schedule_logs_dialog(cx);
                                                        },
                                                    );
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
                                                this.host_tools.update(
                                                    cx,
                                                    |host_tools, cx| {
                                                        host_tools
                                                            .dismiss_schedule_logs_dialog(cx);
                                                    },
                                                );
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

impl HostToolsEntity {
    pub(super) fn schedule_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<ResourceScheduledTaskSnapshot> {
        self.host_schedules
            .snapshot
            .as_ref()
            .filter(|_| {
                self.host_schedules.snapshot_connection_id.as_deref() == Some(connection_id)
            })
            .cloned()
    }

    pub(super) fn schedule_snapshot_polling(&self) -> bool {
        self.host_schedules.polling
    }

    pub(in crate::workspace::connection_monitor) fn schedule_filter(&self) -> ScheduledTaskFilter {
        self.host_schedules.filter
    }

    pub(super) fn schedule_list_state(&self) -> ListState {
        self.host_schedules.list_state.clone()
    }

    pub(in crate::workspace::connection_monitor) fn schedule_expanded_index(
        &self,
    ) -> Option<usize> {
        self.host_schedules.expanded_index
    }

    pub(in crate::workspace::connection_monitor) fn select_schedule_filter(
        &mut self,
        filter: ScheduledTaskFilter,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_schedules.filter == filter {
            return false;
        }
        self.host_schedules.filter = filter;
        self.host_schedules.expanded_index = None;
        cx.notify();
        true
    }

    pub(in crate::workspace::connection_monitor) fn toggle_schedule_expanded(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.host_schedules.expanded_index =
            (self.host_schedules.expanded_index != Some(index)).then_some(index);
        cx.notify();
    }

    pub(super) fn sync_schedule_list_signatures(&self, identity: &str, signatures: &[u64]) {
        sync_tauri_variable_list_state_by_signatures(
            &self.host_schedules.list_state,
            &mut self.host_schedules.list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(px(HOST_SCHEDULE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn request_schedule_snapshot(
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
        if self.host_schedules.polling {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::ScheduleSnapshotAlreadyRunning)
                .into_iter()
                .collect();
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::ScheduleConnectionMissing)
                .into_iter()
                .collect();
        };
        let command = build_scheduled_task_snapshot_command(&os_type);
        let mut notices = Vec::new();
        if feedback.should_toast() && command.capability == ScheduledTaskCapability::Partial {
            notices.push(HostToolsNotice::SchedulePartialSupport { os_type });
        }
        let request = HostScheduleSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            failure_fallback,
        };
        self.host_schedules.snapshot_connection_id = Some(connection_id);
        self.host_schedules.running = Some(request.clone());
        self.host_schedules.polling = true;
        // Inventory scans remain manual and never join the metric sampler.
        let spawned = self.spawn_schedule_snapshot_capture(
            command.command,
            request,
            HOST_SCHEDULE_SNAPSHOT_TIMEOUT,
            HOST_SCHEDULE_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_schedules.polling = false;
            self.host_schedules.running = None;
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::ScheduleConnectionMissing)
                .into_iter()
                .collect();
        }
        cx.notify();
        notices
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_schedules_snapshot(
        &mut self,
        delivery: HostScheduleSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_schedules.running.as_ref() != Some(&delivery.request) {
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_schedules.polling = false;
        self.host_schedules.running = None;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_scheduled_task_snapshot(&output.stdout);
                if feedback.should_toast() {
                    match &snapshot.status {
                        ResourceScheduledTaskStatus::Available { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::ScheduleSnapshotLoaded {
                                    count: snapshot.entries.len(),
                                },
                            ));
                        }
                        ResourceScheduledTaskStatus::Unavailable => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::ScheduleUnavailable,
                            ));
                        }
                        ResourceScheduledTaskStatus::Error { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::ScheduleSnapshotFailed,
                            ));
                        }
                        ResourceScheduledTaskStatus::Unknown => {}
                    }
                }
                self.host_schedules.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_schedules.snapshot = Some(snapshot);
            }
            Ok(_) | Err(()) => {
                self.host_schedules.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_schedules.snapshot = Some(ResourceScheduledTaskSnapshot {
                    status: ResourceScheduledTaskStatus::Error {
                        message: failure_fallback,
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::ScheduleSnapshotFailed,
                    ));
                }
            }
        }
        cx.notify();
    }

    pub(super) fn schedule_action_running_for(&self, task_id: &str) -> bool {
        self.host_schedules
            .action_running
            .as_ref()
            .is_some_and(|request| request.task_id == task_id)
    }

    pub(in crate::workspace::connection_monitor) fn open_schedule_action_confirm(
        &mut self,
        request: HostScheduleActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_schedules.action_running.is_some() {
            return Some(HostToolsNotice::ScheduleActionAlreadyRunning);
        }
        HostToolConfirmState::open(&mut self.host_schedules.pending_confirm, request);
        cx.notify();
        None
    }

    pub(in crate::workspace::connection_monitor) fn schedule_confirm_view(
        &self,
    ) -> Option<(
        HostScheduleActionRequest,
        oxideterm_gpui_ui::motion::ExitPhase,
    )> {
        self.host_schedules
            .pending_confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    /// Dismisses a pending confirmation without affecting an in-flight remote action.
    pub(in crate::workspace::connection_monitor) fn dismiss_schedule_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.host_schedules.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn begin_schedule_confirm_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self
            .host_schedules
            .pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_schedules.pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_schedules
                    .pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_schedules.pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(super) fn confirm_schedule_action(
        &mut self,
        delay: Duration,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(request) = self
            .host_schedules
            .pending_confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return Vec::new();
        };
        if !self.begin_schedule_confirm_exit(delay, cx) {
            return Vec::new();
        }
        self.start_schedule_action(request, runtime, cx)
    }

    fn start_schedule_action(
        &mut self,
        request: HostScheduleActionRequest,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(os_type) = self.connection_os_type(&request.connection_id) else {
            return vec![HostToolsNotice::ScheduleConnectionMissing];
        };
        let command = match build_scheduled_task_action_command(&os_type, request.action.clone()) {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::ScheduleActionFailed],
        };
        let mut notices = Vec::new();
        if command.capability == ScheduledTaskCapability::Partial {
            notices.push(HostToolsNotice::SchedulePartialSupport { os_type });
        }
        self.host_schedules.action_running = Some(request.clone());
        let spawned = self.spawn_schedule_action(
            command.command,
            request,
            HOST_SCHEDULE_ACTION_TIMEOUT,
            HOST_SCHEDULE_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_schedules.action_running = None;
            return vec![HostToolsNotice::ScheduleConnectionMissing];
        }
        cx.notify();
        notices
    }

    pub(super) fn request_schedule_logs(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        runtime: tokio::runtime::Handle,
        failure_fallback: String,
        empty_fallback: String,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self
            .host_schedules
            .logs_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.loading)
        {
            return vec![HostToolsNotice::ScheduleLogsAlreadyRunning];
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return vec![HostToolsNotice::ScheduleConnectionMissing];
        };
        let command = match build_scheduled_task_logs_command(&os_type, &task, false, 200) {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::ScheduleLogsFailed],
        };
        let mut notices = Vec::new();
        if command.capability == ScheduledTaskCapability::Partial {
            notices.push(HostToolsNotice::SchedulePartialSupport { os_type });
        }
        let request = HostScheduleLogsRequest {
            connection_id,
            task_id: task.id,
            task_name: task.name,
            task_source: task.source,
            task_unit: task.unit,
            failure_fallback,
            empty_fallback,
        };
        self.host_schedules.logs_dialog = Some(HostScheduleLogsDialog {
            request: request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        let spawned = self.spawn_schedule_logs_capture(
            command.command,
            request,
            HOST_SCHEDULE_LOGS_TIMEOUT,
            HOST_SCHEDULE_LOGS_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_schedules.logs_dialog = None;
            return vec![HostToolsNotice::ScheduleConnectionMissing];
        }
        cx.notify();
        notices
    }

    pub(super) fn schedule_logs_dialog(&self) -> Option<HostScheduleLogsDialog> {
        self.host_schedules.logs_dialog.clone()
    }

    pub(super) fn dismiss_schedule_logs_dialog(&mut self, cx: &mut Context<Self>) {
        if self.host_schedules.logs_dialog.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_schedule_logs(
        &mut self,
        delivery: HostScheduleLogsDelivery,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .host_schedules
            .logs_dialog
            .as_mut()
            .filter(|dialog| dialog.request == delivery.request)
        else {
            return;
        };
        dialog.loading = false;
        match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                zeroize::Zeroize::zeroize(&mut output.stderr);
                let retained_output = if output.stdout.trim().is_empty() {
                    delivery.request.empty_fallback
                } else {
                    std::mem::take(&mut output.stdout)
                };
                // One shared owner retains the requested output and clears it
                // when both the Entity and current render tree release it.
                dialog.output = Some(Arc::new(zeroize::Zeroizing::new(retained_output)));
                dialog.error = None;
            }
            Ok(mut output) => {
                // Failed output is never user-facing and is cleared immediately.
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

    pub(in crate::workspace::connection_monitor) fn finish_host_schedule_action(
        &mut self,
        delivery: HostScheduleActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_schedules.action_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_schedules.action_running = None;
        let succeeded = delivery.result.unwrap_or(false);
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::ScheduleActionFinished {
                kind: schedule_action_notice_kind(&delivery.request.action),
                task_name: delivery.request.task_name,
                succeeded,
            },
        ));
        cx.emit(HostToolsEvent::RefreshSchedules {
            connection_id: delivery.request.connection_id,
        });
        cx.notify();
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

fn schedule_action_notice_kind(action: &ScheduledTaskActionKind) -> ScheduleActionNoticeKind {
    match action {
        ScheduledTaskActionKind::RunNow { .. } => ScheduleActionNoticeKind::RunNow,
        ScheduledTaskActionKind::Enable { .. } => ScheduleActionNoticeKind::Enable,
        ScheduledTaskActionKind::Disable { .. } => ScheduleActionNoticeKind::Disable,
    }
}
