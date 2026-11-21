//! Owns the scheduled tasks Host Tool UI and request lifecycle.

use super::*;

mod operations;

use oxideterm_connection_monitor::{
    ScheduledTaskFilter, ScheduledTaskToggleAction, parse_scheduled_task_snapshot,
    scheduled_task_action_availability, scheduled_task_filter_options,
};

const HOST_SCHEDULE_LOG_LINE_LIMIT: usize = 200;

impl HostToolsEntity {
    #[allow(clippy::too_many_arguments)]
    pub fn render_host_schedules_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return host_tools_center_state(
                "lucide/wifi-off.svg",
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
        let snapshot = self.schedule_snapshot_for(selected_id);
        let rows = snapshot
            .as_ref()
            .map(|snapshot| {
                visible_scheduled_task_rows(
                    &snapshot.entries,
                    &self.ui.host_schedule_search_query,
                    self.schedule_filter(),
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
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
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &connections,
                        selected_id,
                        !self.schedule_snapshot_in_flight(),
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_schedule_search(&search_ime, tokens, i18n, cx))
                    .child(self.render_host_schedule_filter_row(tokens, i18n, cx))
                    .child(self.render_host_schedule_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(self.render_host_schedule_list(
                rows,
                self.schedule_snapshot_in_flight(),
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

    fn render_host_schedule_search(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = ime.input();
        let anchor_frame = ime.clone();
        let input_control = ime
            .text_input(
                tokens,
                TextInputView {
                    value: &self.ui.host_schedule_search_query,
                    placeholder: i18n.t("sidebar.host_schedules.search_placeholder"),
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
                    // The root coordinates only the shared window IME selection.
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

    #[allow(clippy::too_many_arguments)]
    fn render_host_schedule_list(
        &self,
        rows: Vec<ResourceScheduledTask>,
        loading: bool,
        status: ResourceScheduledTaskStatus,
        selected_id: &str,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                "lucide/clock.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_schedules.loading"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceScheduledTaskStatus::Unavailable => {
                return host_tools_center_state(
                    "lucide/clock.svg",
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_schedules.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceScheduledTaskStatus::Error { message } => {
                return host_tools_center_state(
                    "lucide/alert-triangle.svg",
                    MONITOR_RED,
                    i18n.t("sidebar.host_schedules.error")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourceScheduledTaskStatus::Unknown
            | ResourceScheduledTaskStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                "lucide/clock.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_schedules.empty"),
                selectable_text,
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.schedule_list_state();
        let spec = TauriVirtualListSpec::new(px(HOST_SCHEDULE_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let show_context_columns = sidebar_width >= HOST_SCHEDULE_CONTEXT_COLUMNS_MIN_WIDTH;
        let row_tokens = *tokens;
        let row_i18n = i18n.clone();
        let row_mono_font_family = mono_font_family;

        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(Self::render_host_schedule_table_header(
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
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_schedule_row(
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

    fn render_host_schedule_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceScheduledTaskStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match status {
            ResourceScheduledTaskStatus::Available {
                capability: ScheduledTaskCapability::Full,
                ..
            } => i18n.t("sidebar.host_schedules.capability.full"),
            ResourceScheduledTaskStatus::Available {
                capability: ScheduledTaskCapability::Partial,
                ..
            } => i18n.t("sidebar.host_schedules.capability.partial"),
            _ => i18n.t("sidebar.host_schedules.capability.unknown"),
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
                i18n.t("sidebar.host_schedules.count_suffix"),
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
                        "lucide/terminal.svg",
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
                        i18n.t("sidebar.host_schedules.actions.diagnostic"),
                        "host-schedule-diagnostic",
                        true,
                        cx.listener({
                            let selected_id = selected_id.clone();
                            let title = i18n.t("sidebar.host_schedules.diagnostic_title");
                            let opened_notice =
                                i18n.t("sidebar.host_schedules.toast.diagnostic_opened");
                            let missing_notice =
                                i18n.t("sidebar.host_schedules.toast.exec_terminal_missing");
                            move |host_tools, _event, window, cx| {
                                host_tools.dispatch_schedule_diagnostic_terminal(
                                    selected_id.clone(),
                                    title.clone(),
                                    opened_notice.clone(),
                                    missing_notice.clone(),
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/refresh-cw.svg",
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.schedule_snapshot_in_flight(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_schedules.actions.refresh"),
                        "host-schedule-refresh",
                        true,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            // Snapshot refresh is an Entity-owned worker transition.
                            host_tools.request_schedule_snapshot_from_view(
                                selected_id.clone(),
                                HostSnapshotFeedback::Toast,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }

    fn render_host_schedule_filter_row(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div()
            .id("host-schedule-filter-scroll")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .min_w_0();
        for filter in self.schedule_filter_options() {
            row = row.child(self.render_host_schedule_filter_chip(filter, tokens, i18n, cx));
        }
        row.into_any_element()
    }

    fn render_host_schedule_filter_chip(
        &self,
        filter: ScheduledTaskFilter,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let active = self.schedule_filter() == filter;
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
            .child(i18n.t(scheduled_task_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Filtering is local view state and never re-enters the workspace root.
                    host_tools.select_schedule_filter(filter, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_schedule_table_header(
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
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
                    .child(i18n.t("sidebar.host_schedules.columns.task")),
            )
            .when(show_context_columns, |header| {
                header.child(
                    div()
                        .flex_none()
                        .w(px(HOST_SCHEDULE_SOURCE_COLUMN_WIDTH))
                        .truncate()
                        .child(i18n.t("sidebar.host_schedules.columns.source")),
                )
            })
            .child(
                div()
                    .flex_none()
                    .when(show_context_columns, |col| {
                        col.w(px(HOST_SCHEDULE_STATE_COLUMN_WIDTH))
                    })
                    .when(!show_context_columns, |col| col.flex().justify_end())
                    .truncate()
                    .child(i18n.t("sidebar.host_schedules.columns.state")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SCHEDULE_ENABLED_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_schedules.columns.enabled")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SCHEDULE_NEXT_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_schedules.columns.next")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SCHEDULE_LAST_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_schedules.columns.last")),
                    )
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_schedule_row(
        &self,
        connection_id: &str,
        index: usize,
        entry: Option<ResourceScheduledTask>,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.schedule_expanded_index() == Some(index);
        let theme = tokens.ui;
        let source = host_schedule_source_display(i18n, &entry.source);
        let active = host_schedule_active_display(i18n, &entry.active);
        let enabled = host_schedule_enabled_display(i18n, &entry.enabled);
        let next = host_schedule_blank_dash(&entry.next_run);
        let last = host_schedule_blank_dash(&entry.last_run);

        div()
            .id(host_tools_row_id(
                "host-schedule-row",
                connection_id,
                entry.id.clone(),
            ))
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
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_SCHEDULE_SOURCE_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(source.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .when(show_context_columns, |col| {
                                col.w(px(HOST_SCHEDULE_STATE_COLUMN_WIDTH))
                            })
                            .when(!show_context_columns, |col| col.max_w(px(110.0)))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_schedule_active_color(
                                &entry.active,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(active),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
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
                        .child(
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
                            .font_family(mono_font.clone())
                            .child(if show_context_columns {
                                format!(
                                    "{} · {}",
                                    i18n.t("sidebar.host_schedules.columns.schedule"),
                                    host_schedule_blank_dash(&entry.schedule)
                                )
                            } else {
                                let mut parts = vec![source, enabled];
                                if next != "—" {
                                    parts.push(next);
                                }
                                let command = host_schedule_blank_dash(&entry.command);
                                if command != "—" {
                                    parts.push(command);
                                }
                                parts.join(" · ")
                            }),
                    )
                    .child(self.render_host_schedule_inline_actions(
                        connection_id,
                        &entry,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .when(expanded, |row| {
                row.child(Self::render_host_schedule_detail(
                    &entry, tokens, i18n, mono_font,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Expansion is page-local state and survives workspace mount changes.
                    host_tools.toggle_schedule_expanded(index, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_schedule_detail(
        entry: &ResourceScheduledTask,
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
                    .min_w(px(640.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .font_family(mono_font)
                    .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.task"),
                        host_schedule_blank_dash(&entry.name)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.source"),
                        host_schedule_source_display(i18n, &entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.state"),
                        host_schedule_active_display(i18n, &entry.active)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.enabled"),
                        host_schedule_enabled_display(i18n, &entry.enabled)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.next"),
                        host_schedule_blank_dash(&entry.next_run)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.last"),
                        host_schedule_blank_dash(&entry.last_run)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.result"),
                        host_schedule_blank_dash(&entry.last_result)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.user"),
                        host_schedule_blank_dash(&entry.user)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.unit"),
                        host_schedule_blank_dash(&entry.unit)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.schedule"),
                        host_schedule_blank_dash(&entry.schedule)
                    )))
                    .child(div().whitespace_nowrap().child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.command"),
                        host_schedule_blank_dash(&entry.command)
                    )))
                    .child(div().whitespace_nowrap().child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_schedules.columns.description"),
                        host_schedule_blank_dash(&entry.description)
                    ))),
            )
            .into_any_element()
    }

    fn render_host_schedule_logs_content(
        dialog: &HostScheduleLogsDialog,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
    ) -> AnyElement {
        let theme = tokens.ui;
        if dialog.loading {
            return div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(i18n.t("sidebar.host_schedules.logs.loading"))
                .into_any_element();
        }
        if let Some(error) = dialog.error.as_ref() {
            return div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element();
        }

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
    }

    fn render_host_schedule_confirm_view(
        &self,
        i18n: &I18n,
    ) -> Option<(oxideterm_gpui_ui::motion::ExitPhase, ConfirmDialogView)> {
        let (request, phase) = self.schedule_confirm_view()?;
        let description = i18n
            .t(host_schedule_confirm_description_key(&request.action))
            .replace("{{name}}", &request.task_name)
            .replace("{{unit}}", &host_schedule_blank_dash(&request.unit));
        Some((
            phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Default,
                title: div()
                    .child(i18n.t("sidebar.host_schedules.confirm.title"))
                    .into_any_element(),
                description: Some(div().child(description).into_any_element()),
                cancel_label: div()
                    .child(i18n.t("sidebar.host_schedules.confirm.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(i18n.t(host_schedule_confirm_label_key(&request.action)))
                    .into_any_element(),
            },
        ))
    }

    fn render_host_schedule_inline_actions(
        &self,
        connection_id: &str,
        entry: &ResourceScheduledTask,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let logs_task = host_schedule_command_identity(entry);
        let follow_task = host_schedule_command_identity(entry);
        let run_task = host_schedule_command_identity(entry);
        let toggle_task = host_schedule_command_identity(entry);
        let availability = scheduled_task_action_availability(entry);
        let can_run_now = availability.can_run_now;
        let can_toggle_enabled = availability.can_toggle_enabled;
        let should_enable = matches!(availability.next_toggle, ScheduledTaskToggleAction::Enable);
        let action_running = self.schedule_action_running_for(&entry.id);
        let logs_failure_fallback = i18n.t("sidebar.host_schedules.toast.logs_failed");
        let logs_empty_fallback = i18n.t("sidebar.host_schedules.logs.empty");
        let follow_title = i18n
            .t("sidebar.host_schedules.follow_title")
            .replace("{{name}}", &entry.name);
        let follow_opened_notice = i18n
            .t("sidebar.host_schedules.toast.follow_opened")
            .replace("{{name}}", &entry.name);
        let terminal_missing_notice = i18n.t("sidebar.host_schedules.toast.exec_terminal_missing");
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/file-text.svg",
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
                i18n.t("sidebar.host_schedules.actions.logs"),
                "host-schedule-logs",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |host_tools, _event, _window, cx| {
                        host_tools.request_schedule_logs_from_view(
                            connection_id.clone(),
                            logs_task.clone(),
                            logs_failure_fallback.clone(),
                            logs_empty_fallback.clone(),
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/activity.svg",
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
                i18n.t("sidebar.host_schedules.actions.follow_logs"),
                "host-schedule-follow",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |host_tools, _event, window, cx| {
                        host_tools.dispatch_schedule_follow_terminal(
                            connection_id.clone(),
                            follow_task.clone(),
                            follow_title.clone(),
                            follow_opened_notice.clone(),
                            terminal_missing_notice.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/play.svg",
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
                i18n.t("sidebar.host_schedules.actions.run_now"),
                "host-schedule-run-now",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |host_tools, _event, _window, cx| {
                        if can_run_now && !action_running {
                            host_tools.request_schedule_action_from_view(
                                connection_id.clone(),
                                run_task.clone(),
                                None,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                if should_enable {
                    "lucide/check-circle.svg"
                } else {
                    "lucide/shield-off.svg"
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
                i18n.t(if should_enable {
                    "sidebar.host_schedules.actions.enable"
                } else {
                    "sidebar.host_schedules.actions.disable"
                }),
                "host-schedule-toggle-enabled",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |host_tools, _event, _window, cx| {
                        if can_toggle_enabled && !action_running {
                            host_tools.request_schedule_action_from_view(
                                connection_id.clone(),
                                toggle_task.clone(),
                                Some(should_enable),
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }
                }),
            ))
            .into_any_element()
    }

    pub fn render_host_schedule_confirm_dialog(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        focused_action: Option<ConfirmDialogAction>,
        exit_delay: Duration,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (phase, view) = self.render_host_schedule_confirm_view(i18n)?;
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                tokens,
                "host-schedule-confirm-motion",
                phase,
                view,
                focused_action,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.begin_schedule_confirm_exit(exit_delay, cx);
                }),
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.confirm_schedule_action_from_view(exit_delay, cx);
                }),
            )
            .into_any_element(),
        )
    }

    pub fn render_host_schedule_logs_dialog(
        &self,
        follow_terminal_available: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.schedule_logs_dialog()?;
        let theme = tokens.ui;
        let follow_task = host_schedule_logs_request_identity(&dialog.request);
        let follow_connection_id = dialog.request.connection_id.clone();
        let follow_logs_disabled = !follow_terminal_available
            || self
                .schedule_logs_command(
                    &follow_connection_id,
                    &follow_task,
                    true,
                    HOST_SCHEDULE_LOG_LINE_LIMIT,
                )
                .is_err();
        let follow_title = i18n
            .t("sidebar.host_schedules.follow_title")
            .replace("{{name}}", &dialog.request.task_name);
        let follow_opened_notice = i18n
            .t("sidebar.host_schedules.toast.follow_opened")
            .replace("{{name}}", &dialog.request.task_name);
        let terminal_missing_notice = i18n.t("sidebar.host_schedules.toast.exec_terminal_missing");
        let content =
            Self::render_host_schedule_logs_content(&dialog, tokens, i18n, mono_font_family);

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|host_tools, _event, _window, cx| {
                        host_tools.dismiss_schedule_logs_dialog(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(tokens)
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
                                                .child(
                                                    i18n
                                                        .t("sidebar.host_schedules.logs.title")
                                                        .replace(
                                                            "{{name}}",
                                                            &dialog.request.task_name,
                                                        ),
                                                ),
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
                                        .child(host_tools_tooltip_icon_button(
                                            tokens,
                                            "lucide/activity.svg",
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
                                            i18n
                                                .t("sidebar.host_schedules.actions.follow_logs"),
                                            "host-schedule-logs-follow",
                                            true,
                                            cx.listener({
                                                move |host_tools, _event, window, cx| {
                                                    host_tools
                                                        .dismiss_schedule_logs_dialog(cx);
                                                    host_tools
                                                        .dispatch_schedule_follow_terminal(
                                                            follow_connection_id.clone(),
                                                            follow_task.clone(),
                                                            follow_title.clone(),
                                                            follow_opened_notice.clone(),
                                                            terminal_missing_notice.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    cx.stop_propagation();
                                                }
                                            }),
                                        ))
                                        .child(host_tools_tooltip_icon_button(
                                            tokens,
                                            "lucide/x.svg",
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
                                            i18n.t("sidebar.host_schedules.logs.close"),
                                            "host-schedule-logs-close",
                                            true,
                                            cx.listener(|host_tools, _event, _window, cx| {
                                                host_tools.dismiss_schedule_logs_dialog(cx);
                                                cx.stop_propagation();
                                            }),
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

    pub fn schedule_search_is_focused(&self) -> bool {
        self.ui.input_is_focused(HostToolsTextInput::ScheduleSearch)
    }

    pub fn clear_schedule_search_focus(&mut self, cx: &mut Context<Self>) {
        self.ui.clear_input_focus();
        cx.notify();
    }

    pub fn schedule_confirm_is_open(&self) -> bool {
        self.schedule_confirm_view().is_some()
    }

    fn request_schedule_action_from_view(
        &mut self,
        connection_id: String,
        task: ResourceScheduledTask,
        enable: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let action = match enable {
            None => ScheduledTaskActionKind::RunNow {
                id: task.id.clone(),
                unit: task.unit.clone(),
            },
            Some(true) => ScheduledTaskActionKind::Enable {
                id: task.id.clone(),
                source: task.source.clone(),
            },
            Some(false) => ScheduledTaskActionKind::Disable {
                id: task.id.clone(),
                source: task.source.clone(),
            },
        };
        if let Some(notice) = self.open_schedule_action_confirm(
            HostScheduleActionRequest {
                connection_id,
                task_id: task.id,
                task_name: task.name,
                unit: task.unit,
                action,
            },
            cx,
        ) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
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

fn host_schedule_command_identity(entry: &ResourceScheduledTask) -> ResourceScheduledTask {
    // Terminal and worker actions retain only fields consumed by fixed command builders.
    ResourceScheduledTask {
        id: entry.id.clone(),
        name: entry.name.clone(),
        source: entry.source.clone(),
        schedule: String::new(),
        command: String::new(),
        user: String::new(),
        enabled: String::new(),
        active: String::new(),
        last_run: String::new(),
        next_run: String::new(),
        last_result: String::new(),
        description: String::new(),
        unit: entry.unit.clone(),
    }
}

fn host_schedule_logs_request_identity(request: &HostScheduleLogsRequest) -> ResourceScheduledTask {
    // Captured log output never flows back into a generated terminal command.
    ResourceScheduledTask {
        id: request.task_id.clone(),
        name: request.task_name.clone(),
        source: request.task_source.clone(),
        schedule: String::new(),
        command: String::new(),
        user: String::new(),
        enabled: String::new(),
        active: String::new(),
        last_run: String::new(),
        next_run: String::new(),
        last_result: String::new(),
        description: String::new(),
        unit: request.task_unit.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sampled_task() -> ResourceScheduledTask {
        ResourceScheduledTask {
            id: "backup.timer".to_string(),
            name: "Backup".to_string(),
            source: "systemd".to_string(),
            schedule: "daily".to_string(),
            command: "SECRET_TOKEN=example backup".to_string(),
            user: "root".to_string(),
            enabled: "enabled".to_string(),
            active: "active".to_string(),
            last_run: "yesterday".to_string(),
            next_run: "tomorrow".to_string(),
            last_result: "success".to_string(),
            description: "Daily backup".to_string(),
            unit: "backup.service".to_string(),
        }
    }

    #[test]
    fn command_identity_omits_sampled_command_and_view_fields() {
        let identity = host_schedule_command_identity(&sampled_task());

        assert_eq!(identity.id, "backup.timer");
        assert_eq!(identity.name, "Backup");
        assert_eq!(identity.source, "systemd");
        assert_eq!(identity.unit, "backup.service");
        assert!(identity.command.is_empty());
        assert!(identity.schedule.is_empty());
        assert!(identity.description.is_empty());
    }
}
