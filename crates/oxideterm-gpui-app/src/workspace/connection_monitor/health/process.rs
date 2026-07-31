//! Owns the process Host Tool UI and request lifecycle.

use super::*;

impl HostToolsEntity {
    fn render_host_processes_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        renice_ime: HostToolsPlainTextImeFrame,
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
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let current = self
            .profiler_registry()
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
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &connections,
                        selected_id,
                        current.is_some(),
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_process_search(&search_ime, tokens, i18n, cx))
                    .child(self.render_host_process_filter_row(tokens, i18n, selectable_text, cx))
                    .child(self.render_host_process_sort_row(rows.len(), tokens, i18n, cx)),
            )
            .child(self.render_host_process_list(
                rows,
                current.is_some(),
                selected_id,
                renice_ime,
                sidebar_width,
                tokens,
                i18n,
                mono_font_family,
                selectable_text,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_process_filter_row(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .child(self.render_host_process_filter_chip(
                ProcessFilter::All,
                "sidebar.host_processes.filters.all",
                tokens,
                i18n,
                selectable_text,
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::Running,
                "sidebar.host_processes.filters.running",
                tokens,
                i18n,
                selectable_text,
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::HighCpu,
                "sidebar.host_processes.filters.high_cpu",
                tokens,
                i18n,
                selectable_text,
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::HighMemory,
                "sidebar.host_processes.filters.high_memory",
                tokens,
                i18n,
                selectable_text,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_process_filter_chip(
        &self,
        filter: ProcessFilter,
        label_key: &'static str,
        tokens: &ThemeTokens,
        i18n: &I18n,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.ui.host_process_filter == filter;
        let theme = tokens.ui;
        div()
            .flex_none()
            .px_2()
            .h(px(24.0))
            .flex()
            .items_center()
            .rounded(px(tokens.radii.sm))
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
            .child(selectable_text.render_display_text_with_role_in_group(
                SelectableTextRole::NonSelectable,
                selectable_document_group_id(),
                "host-process-filter",
                label_key,
                0,
                i18n.t(label_key),
                if active { theme.text } else { theme.text_muted },
                cx,
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Filtering and expansion are process-page state transitions.
                    host_tools.ui.host_process_filter = filter;
                    host_tools.ui.host_process_expanded_pid = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_process_sort_row(
        &self,
        visible_count: usize,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(tokens.ui.text_muted))
            .child(div().flex_none().child(format!(
                "{} {}",
                visible_count,
                i18n.t("sidebar.host_processes.count_suffix")
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
                        tokens,
                        i18n,
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Memory,
                        "sidebar.host_processes.sort.memory",
                        tokens,
                        i18n,
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Pid,
                        "sidebar.host_processes.sort.pid",
                        tokens,
                        i18n,
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Command,
                        "sidebar.host_processes.sort.command",
                        tokens,
                        i18n,
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::User,
                        "sidebar.host_processes.sort.user",
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_host_process_sort_button(
        &self,
        sort: ProcessSort,
        label_key: &'static str,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.ui.host_process_sort == sort;
        let descending = self.ui.host_process_sort_descending;
        let theme = tokens.ui;
        let mut label = i18n.t(label_key);
        if active {
            label.push_str(if descending { " ↓" } else { " ↑" });
        }
        div()
            .flex_none()
            .px_1p5()
            .h(px(22.0))
            .flex()
            .items_center()
            .rounded(px(tokens.radii.sm))
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
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Sorting changes only the process view owned by this entity.
                    let ui = &mut host_tools.ui;
                    if ui.host_process_sort == sort {
                        ui.host_process_sort_descending = !ui.host_process_sort_descending;
                    } else {
                        ui.host_process_sort = sort;
                        ui.host_process_sort_descending =
                            !matches!(sort, ProcessSort::Command | ProcessSort::User);
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn visible_host_process_rows(
        &self,
        processes: &[ResourceTopProcess],
    ) -> Vec<ResourceTopProcess> {
        visible_process_rows(
            processes,
            &self.ui.host_process_search_query,
            self.ui.host_process_filter,
            self.ui.host_process_sort,
            self.ui.host_process_sort_descending,
        )
    }

    fn sync_host_process_list_state(&self, rows: &[ResourceTopProcess], selected_id: &str) {
        let signatures = rows.iter().map(process_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-processes:{selected_id}:{}:{}:{}:{}:{}",
            self.ui.host_process_search_query,
            self.ui.host_process_filter as u8,
            self.ui.host_process_sort as u8,
            self.ui.host_process_sort_descending,
            self.ui
                .host_process_expanded_pid
                .as_deref()
                .unwrap_or_default()
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.ui.host_process_list_state,
            &mut self.ui.host_process_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_process_list(
        &self,
        rows: Vec<ResourceTopProcess>,
        has_metrics: bool,
        selected_id: &str,
        renice_ime: HostToolsPlainTextImeFrame,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_metrics {
            return host_tools_center_state(
                LucideIcon::Activity,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_processes.sampling"),
                selectable_text,
                cx,
            );
        }
        if rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::ListChecks,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_processes.empty"),
                selectable_text,
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.ui.host_process_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let separate_user_column = host_process_table_uses_separate_user_column(sidebar_width);
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
            // Processes are an operational table, not a card stack; keep the
            // header fixed while the GPUI List owns only the scrolling rows.
            .child(Self::render_host_process_table_header(
                tokens,
                i18n,
                separate_user_column,
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
                                host_tools.render_host_process_row(
                                    selected_id.as_str(),
                                    rows.get(index).cloned(),
                                    separate_user_column,
                                    &renice_ime,
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

    fn render_host_process_table_header(
        tokens: &ThemeTokens,
        i18n: &I18n,
        separate_user_column: bool,
    ) -> AnyElement {
        let theme = tokens.ui;
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
                        i18n,
                        separate_user_column,
                    )),
            )
            .when(separate_user_column, |header| {
                header.child(
                    div()
                        .flex_none()
                        .w(px(HOST_PROCESS_USER_COLUMN_WIDTH))
                        .truncate()
                        .child(i18n.t("sidebar.host_processes.sort.user")),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_PID_COLUMN_WIDTH))
                    .child(i18n.t("sidebar.host_processes.sort.pid")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_CPU_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_processes.sort.cpu")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_MEMORY_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_processes.sort.memory")),
            )
            .into_any_element()
    }

    fn process_is_expanded(&self, pid: &str) -> bool {
        self.ui.host_process_expanded_pid.as_deref() == Some(pid)
    }

    fn render_host_process_row(
        &self,
        connection_id: &str,
        process: Option<ResourceTopProcess>,
        separate_user_column: bool,
        renice_ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(process) = process else {
            return div().into_any_element();
        };
        let theme = tokens.ui;
        let status = process
            .state
            .as_deref()
            .map(|state| i18n.t(process_state_label_key(state)))
            .unwrap_or_else(|| i18n.t("sidebar.host_processes.unknown"));
        let user = process
            .user
            .clone()
            .unwrap_or_else(|| i18n.t("sidebar.host_processes.unknown"));
        let cpu = process
            .cpu_percent
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(|| "—".to_string());
        let memory = format!("{:.1}%", process.memory_percent);
        let cpu_color = threshold_color(process.cpu_percent);
        let memory_color = threshold_color(Some(process.memory_percent));
        let inline_actions =
            self.render_host_process_inline_actions(connection_id, &process, tokens, i18n, cx);
        let detail = self.process_is_expanded(&process.pid).then(|| {
            self.render_host_process_detail(
                connection_id,
                &process,
                self.render_host_process_renice_input(renice_ime, tokens, i18n, cx),
                tokens,
                i18n,
                mono_font_family.clone(),
                cx,
            )
        });

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
                            .font_family(mono_font_family.clone())
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
                                .font_family(mono_font_family.clone())
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
                                .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family)
                            .child(format!("{status} · {}", process_display_command(&process))),
                    )
                    .child(inline_actions),
            )
            .when_some(detail, |row, detail| row.child(detail))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let pid = process.pid.clone();
                    move |host_tools, _event, _window, cx| {
                        // Expansion is local process-page state and never re-enters the root.
                        let expanded_pid = &mut host_tools.ui.host_process_expanded_pid;
                        if expanded_pid.as_deref() == Some(pid.as_str()) {
                            *expanded_pid = None;
                        } else {
                            *expanded_pid = Some(pid.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    fn render_host_process_search(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_host_process_text_input(
            ime,
            &self.ui.host_process_search_query,
            i18n.t("sidebar.host_processes.search_placeholder"),
            None,
            px(34.0),
            tokens,
            cx,
        )
    }

    fn render_host_process_renice_input(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_host_process_text_input(
            ime,
            &self.ui.host_process_renice_value,
            i18n.t("sidebar.host_processes.actions.renice_placeholder"),
            Some(px(54.0)),
            px(26.0),
            tokens,
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_process_text_input(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        value: &str,
        placeholder: String,
        width: Option<Pixels>,
        height: Pixels,
        tokens: &ThemeTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = ime.input();
        let anchor_frame = ime.clone();
        let input_control = text_input(
            tokens,
            TextInputView {
                value,
                placeholder,
                focused: self.ui.input_is_focused(input),
                caret_visible: ime.caret_visible(),
                secret: false,
                selected_all: false,
                selected_range: ime.selected_range(),
                marked_text: ime.marked_text(),
            },
        )
        .h(height)
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |host_tools, event: &MouseDownEvent, window, cx| {
                host_tools.ui.focus_input(input);
                // The event moves through a one-shot action so the root can
                // coordinate the shared window IME without being retained.
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
            if let Some(width) = width {
                input_control.w(width)
            } else {
                input_control
            },
            move |anchor, _window, _cx| {
                anchor_frame.update_anchor(anchor);
            },
        )
        .into_any_element()
    }

    fn render_host_process_inline_actions(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self.process_action_running_for(&process.pid);
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
                tokens,
                i18n,
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
                tokens,
                i18n,
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
                tokens,
                i18n,
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
                tokens,
                i18n,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_process_detail(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        renice_input: AnyElement,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
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
            .child(Self::render_host_process_detail_line(
                "PPID",
                process.ppid.clone().unwrap_or_else(|| "—".to_string()),
                mono_font_family.clone(),
            ))
            .child(Self::render_host_process_detail_line(
                "RSS",
                process
                    .rss_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "—".to_string()),
                mono_font_family.clone(),
            ))
            .child(Self::render_host_process_detail_line(
                "VSZ",
                process
                    .vsz_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "—".to_string()),
                mono_font_family.clone(),
            ))
            .child(Self::render_host_process_detail_line(
                i18n.t("sidebar.host_processes.elapsed"),
                process.elapsed.clone().unwrap_or_else(|| "—".to_string()),
                mono_font_family.clone(),
            ))
            .child(self.render_host_process_action_bar(
                connection_id,
                process,
                renice_input,
                tokens,
                i18n,
                cx,
            ))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font_family)
                    .text_color(rgb(theme.text))
                    .child(process_display_command(process)),
            )
            .into_any_element()
    }

    fn render_host_process_detail_line(
        label: impl Into<String>,
        value: String,
        mono_font_family: SharedString,
    ) -> AnyElement {
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
                    .font_family(mono_font_family)
                    .child(value),
            )
            .into_any_element()
    }

    fn render_host_process_action_bar(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        renice_input: AnyElement,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let is_running = self.process_action_running_for(&process.pid);
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
                            .child(i18n.t("sidebar.host_processes.actions.renice")),
                    )
                    .child(renice_input)
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
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_process_action_button(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        action: ProcessActionKind,
        icon: LucideIcon,
        label_key: &'static str,
        danger: bool,
        disabled: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let label = i18n.t(label_key);
        let unsupported =
            !self.process_action_supported(connection_id, &process.pid, action.clone());
        let disabled = disabled || unsupported;
        let icon_color = if danger { MONITOR_RED } else { theme.text };
        let connection_id = connection_id.to_string();
        let pid = process.pid.clone();
        // Share one zeroizing display value with the listener and confirmation state.
        let display_command = Arc::new(zeroize::Zeroizing::new(process_display_name(process)));
        host_tools_tooltip_icon_button(
            tokens,
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
            cx.listener(move |host_tools, _event, _window, cx| {
                host_tools.request_host_process_action(
                    connection_id.clone(),
                    pid.clone(),
                    display_command.clone(),
                    action.clone(),
                    cx,
                );
                cx.stop_propagation();
            }),
        )
    }

    fn host_process_renice_value(&self) -> i32 {
        self.ui
            .host_process_renice_value
            .trim()
            .parse::<i32>()
            .unwrap_or(0)
            .clamp(-20, 19)
    }

    fn request_host_process_action(
        &mut self,
        connection_id: String,
        pid: String,
        display_command: Arc<zeroize::Zeroizing<String>>,
        action: ProcessActionKind,
        cx: &mut Context<Self>,
    ) {
        let notice = self.open_process_action_confirm(
            HostProcessActionRequest {
                connection_id,
                pid,
                display_command,
                action,
            },
            cx,
        );
        if let Some(notice) = notice {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    fn render_host_process_confirm_dialog(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        focused_action: Option<ConfirmDialogAction>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (request, phase) = self.process_confirm_view()?;
        let description = i18n
            .t(host_process_confirm_description_key(&request.action))
            .replace("{{pid}}", &request.pid)
            // This is the explicit UI boundary for the retained display name.
            .replace("{{command}}", request.display_command.as_str());
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );

        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                tokens,
                "host-process-confirm-motion",
                phase,
                ConfirmDialogView {
                    variant: if matches!(request.action, ProcessActionKind::Kill) {
                        ConfirmDialogVariant::Danger
                    } else {
                        ConfirmDialogVariant::Default
                    },
                    title: div()
                        .child(i18n.t("sidebar.host_processes.confirm.title"))
                        .into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(i18n.t("sidebar.host_processes.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(i18n.t(host_process_confirm_label_key(&request.action)))
                        .into_any_element(),
                },
                focused_action,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Confirmation presence belongs to the Host Tools entity.
                    host_tools.begin_process_confirm_exit(exit_delay, cx);
                }),
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.confirm_process_action_from_view(exit_delay, cx);
                }),
            )
            .into_any_element(),
        )
    }

    fn confirm_process_action_from_view(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::ProcessConnectionMissing,
            ));
            return;
        };
        for notice in self.confirm_process_action(delay, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }
}

impl WorkspaceApp {
    pub(super) fn render_host_processes_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.selectable_text_render_state(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::ProcessSearch, cx)
            .expect("process search is a non-secret Host Tools input");
        let renice_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::ProcessRenice, cx)
            .expect("process renice is a non-secret Host Tools input");
        let sidebar_width = self.ai_entity.read(cx).chat_ui().sidebar_width;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_processes_panel(
                search_ime,
                renice_ime,
                sidebar_width,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_process_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let focused_input = self.host_tools.read(cx).ui.focused_input;
        if !matches!(
            focused_input,
            Some(HostToolsTextInput::ProcessSearch | HostToolsTextInput::ProcessRenice)
        ) {
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

    pub(in crate::workspace) fn handle_host_process_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).process_confirm_view().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_process_confirm_exit(cx);
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
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_process_action(delay, runtime, cx)
        });
        for notice in notices {
            self.push_host_tools_notice(notice, cx);
        }
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_process_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_process_confirm_exit(delay, cx)
        })
    }

    pub(in crate::workspace) fn render_host_process_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let focused_action = self.standard_confirm_focus();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_process_confirm_dialog(&tokens, i18n, focused_action, cx)
        })
    }
}

impl HostToolsEntity {
    pub(super) fn process_action_running_for(&self, pid: &str) -> bool {
        self.host_process_actions
            .running
            .as_ref()
            .is_some_and(|request| request.pid == pid)
    }

    pub(super) fn process_action_supported(
        &self,
        connection_id: &str,
        pid: &str,
        action: ProcessActionKind,
    ) -> bool {
        self.connection_os_type(connection_id)
            .and_then(|os_type| build_process_action_command(&os_type, pid, action).ok())
            .is_some()
    }

    pub(in crate::workspace::connection_monitor) fn open_process_action_confirm(
        &mut self,
        request: HostProcessActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_process_actions.running.is_some() {
            return Some(HostToolsNotice::ProcessActionAlreadyRunning);
        }
        if let ProcessActionKind::Renice { nice } = &request.action
            && !(-20..=19).contains(nice)
        {
            return Some(HostToolsNotice::ProcessInvalidNice);
        }
        HostToolConfirmState::open(&mut self.host_process_actions.pending_confirm, request);
        cx.notify();
        None
    }

    pub(in crate::workspace::connection_monitor) fn process_confirm_view(
        &self,
    ) -> Option<(
        HostProcessActionRequest,
        oxideterm_gpui_ui::motion::ExitPhase,
    )> {
        self.host_process_actions
            .pending_confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    /// Dismisses only UI confirmation state; a submitted remote action remains owned here.
    pub(in crate::workspace::connection_monitor) fn dismiss_process_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.host_process_actions.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn begin_process_confirm_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self
            .host_process_actions
            .pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_process_actions.pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_process_actions
                    .pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_process_actions.pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(super) fn confirm_process_action(
        &mut self,
        delay: Duration,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(request) = self.host_process_actions.pending_confirm.as_ref() else {
            return Vec::new();
        };
        // The worker request intentionally omits the secret-bearing display command.
        let request = HostProcessActionRun {
            connection_id: request.request.connection_id.clone(),
            pid: request.request.pid.clone(),
            action: request.request.action.clone(),
        };
        if !self.begin_process_confirm_exit(delay, cx) {
            return Vec::new();
        }
        self.start_process_action(request, runtime, cx)
    }

    pub(in crate::workspace::connection_monitor) fn start_process_action(
        &mut self,
        request: HostProcessActionRun,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(os_type) = self.connection_os_type(&request.connection_id) else {
            return vec![HostToolsNotice::ProcessConnectionMissing];
        };
        let command =
            match build_process_action_command(&os_type, &request.pid, request.action.clone()) {
                Ok(command) => command,
                Err(_) => return vec![HostToolsNotice::ProcessActionFailed],
            };
        let mut notices = Vec::new();
        if command.capability == ProcessCommandCapability::Partial {
            notices.push(HostToolsNotice::ProcessPartialSupport { os_type });
        }
        self.host_process_actions.running = Some(request.clone());
        let spawned = self.spawn_process_action(
            command.command,
            request,
            HOST_PROCESS_ACTION_TIMEOUT,
            HOST_PROCESS_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_process_actions.running = None;
            return vec![HostToolsNotice::ProcessConnectionMissing];
        }
        cx.notify();
        notices
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_process_action(
        &mut self,
        delivery: HostProcessActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_process_actions.running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_process_actions.running = None;
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::ProcessActionFinished {
                pid: delivery.request.pid,
                succeeded: delivery.result.unwrap_or(false),
            },
        ));
        // Force the next workspace integration call to rebuild the sampler
        // instead of treating the existing configuration as already running.
        self.profiler_registry.stop(&delivery.request.connection_id);
        self.request_profiler_refresh(delivery.request.connection_id, cx);
        cx.notify();
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
