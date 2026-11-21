//! Owns the tmux Host Tool UI and request lifecycle.

use super::*;

mod operations;

use oxideterm_connection_monitor::tmux_capture_snapshot;

impl HostToolsEntity {
    #[allow(clippy::too_many_arguments)]
    pub fn render_host_tmux_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        sidebar_width: f32,
        terminal_available: bool,
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
        if self.host_tmux.mux.provider.is_some() {
            // Every non-tmux provider shares this tool's header and search box;
            // the panel below owns its own status row and tree.
            return self.render_mux_panel(
                &connections,
                selected_id,
                &search_ime,
                terminal_available,
                tokens,
                i18n,
                mono_font_family,
                selectable_text,
                cx,
            );
        }
        let provider_tabs = self.render_mux_tabs(cx, tokens);
        let snapshot = self.tmux_snapshot_for(selected_id);
        let rows = snapshot
            .as_ref()
            .map(|snapshot| visible_tmux_session_rows(snapshot, &self.ui.host_tmux_search_query))
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_tmux_list_state(&rows, snapshot.as_ref(), selected_id);

        div()
            .id("host-tmux-panel")
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
                        !self.tmux_snapshot_in_flight(),
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(provider_tabs)
                    .child(self.render_host_tmux_search(&search_ime, tokens, i18n, cx))
                    .child(self.render_host_tmux_status_row(
                        rows.len(),
                        selected_id,
                        &status,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(self.render_host_tmux_list(
                rows,
                snapshot,
                self.tmux_snapshot_in_flight(),
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

    pub(crate) fn render_host_tmux_search(
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
            ime.text_input(
                tokens,
                TextInputView {
                    value: &self.ui.host_tmux_search_query,
                    placeholder: i18n.t("sidebar.host_tmux.search_placeholder"),
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
                    // Only pointer metadata crosses to the workspace IME coordinator.
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

    fn render_host_tmux_status_row(
        &self,
        visible_count: usize,
        connection_id: &str,
        status: &ResourceTmuxStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match status {
            ResourceTmuxStatus::Available {
                capability: TmuxCommandCapability::Full,
                ..
            } => i18n.t("sidebar.host_tmux.capability.full"),
            ResourceTmuxStatus::Available {
                capability: TmuxCommandCapability::Partial,
                ..
            } => i18n.t("sidebar.host_tmux.capability.partial"),
            _ => i18n.t("sidebar.host_tmux.capability.unknown"),
        };
        let new_session_title = i18n.t("sidebar.host_tmux.new_session_title");
        let new_session_name = i18n.t("sidebar.host_tmux.new_session_name");
        let opened_notice = i18n
            .t("sidebar.host_tmux.toast.new_session_opened")
            .replace("{{name}}", &new_session_name);
        let missing_notice = i18n.t("sidebar.host_tmux.toast.exec_terminal_missing");
        let connection_id_for_terminal = connection_id.to_string();
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
                i18n.t("sidebar.host_tmux.count_suffix"),
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
                        "lucide/plus.svg",
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
                        i18n.t("sidebar.host_tmux.actions.new_session"),
                        "host-tmux-new-session",
                        true,
                        cx.listener(move |host_tools, _event, window, cx| {
                            host_tools.dispatch_tmux_new_session_terminal(
                                connection_id_for_terminal.clone(),
                                new_session_title.clone(),
                                opened_notice.clone(),
                                missing_notice.clone(),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        "lucide/refresh-cw.svg",
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.tmux_snapshot_in_flight(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_tmux.actions.refresh"),
                        "host-tmux-refresh",
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

    #[allow(clippy::too_many_arguments)]
    fn render_host_tmux_list(
        &self,
        rows: Vec<ResourceTmuxSession>,
        snapshot: Option<ResourceTmuxSnapshot>,
        loading: bool,
        status: ResourceTmuxStatus,
        connection_id: &str,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                "lucide/terminal.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_tmux.loading"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceTmuxStatus::Unavailable => {
                return host_tools_center_state(
                    "lucide/terminal.svg",
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_tmux.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceTmuxStatus::Error { message } => {
                return host_tools_center_state(
                    "lucide/alert-triangle.svg",
                    MONITOR_RED,
                    i18n.t("sidebar.host_tmux.error")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourceTmuxStatus::Unknown | ResourceTmuxStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                "lucide/terminal.svg",
                tokens.ui.text_muted,
                i18n.t("sidebar.host_tmux.empty"),
                selectable_text,
                cx,
            );
        }

        let snapshot = Arc::new(snapshot.unwrap_or_default());
        let rows = Arc::new(rows);
        let connection_id = Arc::new(connection_id.to_string());
        let state = self.ui.host_tmux_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let tokens = *tokens;
        let i18n = i18n.clone();
        let show_context_columns = sidebar_width >= HOST_TMUX_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_tmux_table_header(show_context_columns, &tokens, &i18n))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            let rows = Arc::clone(&rows);
                            let snapshot = Arc::clone(&snapshot);
                            let connection_id = Arc::clone(&connection_id);
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_tmux_row(
                                    connection_id.as_str(),
                                    snapshot.as_ref(),
                                    rows.get(index).cloned(),
                                    show_context_columns,
                                    &tokens,
                                    &i18n,
                                    mono_font_family.clone(),
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    pub fn render_host_tmux_confirm_view(
        &self,
        i18n: &I18n,
    ) -> Option<(oxideterm_gpui_ui::motion::ExitPhase, ConfirmDialogView)> {
        let (request, phase) = self.tmux_confirm_view()?;
        let description = i18n
            .t(host_tmux_confirm_description_key(&request.action))
            .replace("{{name}}", &request.session_name)
            .replace("{{id}}", &request.session_id)
            .replace("{{target}}", &request.target_label);
        Some((
            phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(i18n.t("sidebar.host_tmux.confirm.title"))
                    .into_any_element(),
                description: Some(div().child(description).into_any_element()),
                cancel_label: div()
                    .child(i18n.t("sidebar.host_tmux.confirm.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(i18n.t(host_tmux_confirm_label_key(&request.action)))
                    .into_any_element(),
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_host_tmux_input_dialog_shell(
        &self,
        kind: HostTmuxInputDialogKind,
        session_name: String,
        target_label: String,
        input_control: AnyElement,
        cancel_action: gpui::Div,
        submit_action: gpui::Div,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> gpui::Div {
        let theme = tokens.ui;
        let description = i18n
            .t(host_tmux_input_description_key(&kind))
            .replace("{{name}}", &session_name)
            .replace("{{target}}", &target_label);
        // The secret-bearing input stays materialized by WorkspaceApp; the Entity
        // receives only the element and safe labels, never a cloned input value.
        oxideterm_gpui_ui::modal::dialog_content(tokens)
            .w(px(HOST_TMUX_INPUT_DIALOG_WIDTH))
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(i18n.t(host_tmux_input_title_key(&kind))),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(description),
                    ),
            )
            .child(div().px_4().py_4().child(input_control))
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(cancel_action)
                    .child(submit_action),
            )
    }

    fn render_host_tmux_table_header(
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
            .h(px(HOST_TMUX_TABLE_HEADER_HEIGHT))
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
                    .child(i18n.t("sidebar.host_tmux.columns.session")),
            )
            .child(
                div()
                    .flex_none()
                    .when(show_context_columns, |col| {
                        col.w(px(HOST_TMUX_ATTACHED_COLUMN_WIDTH))
                    })
                    .when(!show_context_columns, |col| col.flex().justify_end())
                    .child(i18n.t("sidebar.host_tmux.columns.attached")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_TMUX_WINDOWS_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .child(i18n.t("sidebar.host_tmux.columns.windows")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_TMUX_PANES_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .child(i18n.t("sidebar.host_tmux.columns.panes")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_TMUX_ACTIVITY_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_tmux.columns.activity")),
                    )
            })
            .into_any_element()
    }

    fn render_host_tmux_row(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: Option<ResourceTmuxSession>,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = session else {
            return div().into_any_element();
        };
        let expanded =
            self.ui.host_tmux_expanded_session_id.as_deref() == Some(session.id.as_str());
        let theme = tokens.ui;
        let pane_count = snapshot.pane_count_for_session(&session.id);
        let attached_label = if session.attached {
            i18n.t("sidebar.host_tmux.attached.yes")
        } else {
            i18n.t("sidebar.host_tmux.attached.no")
        };
        let session_id = session.id.clone();

        div()
            .id(host_tools_row_id(
                "host-tmux-row",
                connection_id,
                session.id.clone(),
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
                    .h(px(HOST_TMUX_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Keep identity at the first flex level so narrow sidebars preserve names.
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
                            .child(session.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .when(show_context_columns, |col| {
                                col.w(px(HOST_TMUX_ATTACHED_COLUMN_WIDTH))
                            })
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(tmux_attached_color(
                                session.attached,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(attached_label),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_TMUX_WINDOWS_COLUMN_WIDTH))
                                .flex()
                                .justify_end()
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(session.windows.to_string()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_TMUX_PANES_COLUMN_WIDTH))
                                .flex()
                                .justify_end()
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(pane_count.to_string()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_TMUX_ACTIVITY_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(tmux_time_label(&session.activity)),
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
                                    session.id,
                                    Self::active_tmux_window_label(snapshot, &session.id, i18n)
                                )
                            } else {
                                format!(
                                    "{}w/{}p · {}",
                                    session.windows,
                                    pane_count,
                                    Self::active_tmux_window_label(snapshot, &session.id, i18n)
                                )
                            }),
                    )
                    .child(self.render_host_tmux_inline_actions(
                        connection_id,
                        &session,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .when(expanded, |row| {
                row.child(self.render_host_tmux_session_detail(
                    connection_id,
                    snapshot,
                    &session,
                    tokens,
                    i18n,
                    mono_font,
                    cx,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Expansion is page-owned view state and never re-enters WorkspaceApp.
                    if host_tools.ui.host_tmux_expanded_session_id.as_deref()
                        == Some(session_id.as_str())
                    {
                        host_tools.ui.host_tmux_expanded_session_id = None;
                    } else {
                        host_tools.ui.host_tmux_expanded_session_id = Some(session_id.clone());
                    }
                    host_tools.ui.host_tmux_expanded_window_id = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_tmux_inline_actions(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let is_running = self.tmux_action_running_for(&session.id);
        let connection_id_for_attach = connection_id.to_string();
        let session_id_for_attach = session.id.clone();
        let connection_id_for_rename = connection_id.to_string();
        let session_id_for_rename = session.id.clone();
        let session_name_for_rename = session.name.clone();
        let connection_id_for_kill = connection_id.to_string();
        let session_id_for_kill = session.id.clone();
        let session_name_for_kill = session.name.clone();
        let missing_notice = i18n.t("sidebar.host_tmux.toast.exec_terminal_missing");
        let attach_title = i18n
            .t("sidebar.host_tmux.attach_title")
            .replace("{{name}}", &session.name);
        let opened_notice = i18n
            .t("sidebar.host_tmux.toast.attach_opened")
            .replace("{{name}}", &session.name);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/terminal.svg",
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.attach"),
                "host-tmux-attach",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.dispatch_tmux_attach_terminal(
                        connection_id_for_attach.clone(),
                        session_id_for_attach.clone(),
                        attach_title.clone(),
                        opened_notice.clone(),
                        missing_notice.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/pencil.svg",
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.rename_session"),
                "host-tmux-rename-session",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_input_from_view(
                        HostTmuxInputDialog {
                            connection_id: connection_id_for_rename.clone(),
                            session_id: session_id_for_rename.clone(),
                            session_name: session_name_for_rename.clone(),
                            target_label: session_name_for_rename.clone(),
                            value: zeroize::Zeroizing::new(session_name_for_rename.clone()),
                            kind: HostTmuxInputDialogKind::RenameSession {
                                target: session_id_for_rename.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/trash-2.svg",
                13.0,
                rgb(MONITOR_RED),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)),
                    hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.kill_session"),
                "host-tmux-kill-session",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_confirm_from_view(
                        HostTmuxActionRequest {
                            connection_id: connection_id_for_kill.clone(),
                            session_id: session_id_for_kill.clone(),
                            session_name: session_name_for_kill.clone(),
                            target_label: session_name_for_kill.clone(),
                            action: HostTmuxDestructiveAction::KillSession {
                                target: session_id_for_kill.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn render_host_tmux_window_actions(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        tmux_window: &ResourceTmuxWindow,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let is_running = self.tmux_action_running_for(&session.id);
        let connection_id_for_rename = connection_id.to_string();
        let session_id_for_rename = session.id.clone();
        let session_name_for_rename = session.name.clone();
        let window_id_for_rename = tmux_window.id.clone();
        let window_label_for_rename = format!("#{} {}", tmux_window.index, tmux_window.name);
        let window_name_for_rename = tmux_window.name.clone();
        let connection_id_for_kill = connection_id.to_string();
        let session_id_for_kill = session.id.clone();
        let session_name_for_kill = session.name.clone();
        let window_id_for_kill = tmux_window.id.clone();
        let window_label_for_kill = format!("#{} {}", tmux_window.index, tmux_window.name);
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/pencil.svg",
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_tmux.actions.rename_window"),
                "host-tmux-rename-window",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_input_from_view(
                        HostTmuxInputDialog {
                            connection_id: connection_id_for_rename.clone(),
                            session_id: session_id_for_rename.clone(),
                            session_name: session_name_for_rename.clone(),
                            target_label: window_label_for_rename.clone(),
                            value: zeroize::Zeroizing::new(window_name_for_rename.clone()),
                            kind: HostTmuxInputDialogKind::RenameWindow {
                                target: window_id_for_rename.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/trash-2.svg",
                12.0,
                rgb(MONITOR_RED),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)),
                    hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_tmux.actions.kill_window"),
                "host-tmux-kill-window",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_confirm_from_view(
                        HostTmuxActionRequest {
                            connection_id: connection_id_for_kill.clone(),
                            session_id: session_id_for_kill.clone(),
                            session_name: session_name_for_kill.clone(),
                            target_label: window_label_for_kill.clone(),
                            action: HostTmuxDestructiveAction::KillWindow {
                                target: window_id_for_kill.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn render_host_tmux_pane_actions(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        pane: &ResourceTmuxPane,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let is_running = self.tmux_action_running_for(&session.id);
        let connection_id_for_command = connection_id.to_string();
        let session_id_for_command = session.id.clone();
        let session_name_for_command = session.name.clone();
        let pane_id_for_command = pane.id.clone();
        let pane_label_for_command = format!("%{} {}", pane.index, pane.command);
        let connection_id_for_kill = connection_id.to_string();
        let session_id_for_kill = session.id.clone();
        let session_name_for_kill = session.name.clone();
        let pane_id_for_kill = pane.id.clone();
        let pane_label_for_kill = format!("%{} {}", pane.index, pane.command);
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/keyboard.svg",
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_tmux.actions.send_command"),
                "host-tmux-send-pane-command",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_input_from_view(
                        HostTmuxInputDialog {
                            connection_id: connection_id_for_command.clone(),
                            session_id: session_id_for_command.clone(),
                            session_name: session_name_for_command.clone(),
                            target_label: pane_label_for_command.clone(),
                            value: zeroize::Zeroizing::new(String::new()),
                            kind: HostTmuxInputDialogKind::SendPaneCommand {
                                target: pane_id_for_command.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                "lucide/trash-2.svg",
                12.0,
                rgb(MONITOR_RED),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)),
                    hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_tmux.actions.kill_pane"),
                "host-tmux-kill-pane",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_confirm_from_view(
                        HostTmuxActionRequest {
                            connection_id: connection_id_for_kill.clone(),
                            session_id: session_id_for_kill.clone(),
                            session_name: session_name_for_kill.clone(),
                            target_label: pane_label_for_kill.clone(),
                            action: HostTmuxDestructiveAction::KillPane {
                                target: pane_id_for_kill.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn render_host_tmux_session_detail(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: &ResourceTmuxSession,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let mut detail = div()
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
            .child(Self::render_host_tmux_detail_line(
                i18n.t("sidebar.host_tmux.columns.created"),
                tmux_time_label(&session.created),
            ))
            .child(Self::render_host_tmux_detail_line(
                i18n.t("sidebar.host_tmux.columns.activity"),
                tmux_time_label(&session.activity),
            ));
        // Child rows and their actions are materialized only after the session expands.
        for window in snapshot.windows_for_session(&session.id) {
            detail = detail.child(self.render_host_tmux_window_detail(
                connection_id,
                snapshot,
                session,
                window,
                tokens,
                i18n,
                mono_font.clone(),
                cx,
            ));
        }
        detail.into_any_element()
    }

    fn render_host_tmux_window_detail(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: &ResourceTmuxSession,
        window: ResourceTmuxWindow,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let expanded = self.ui.host_tmux_expanded_window_id.as_deref() == Some(window.id.as_str());
        let window_id = window.id.clone();
        div()
            .id((gpui::ElementId::from("host-tmux-window"), window.id.clone()))
            .mt_1()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .overflow_hidden()
            .child(
                div()
                    .px_2()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .hover(|row| row.bg(rgb(theme.bg_hover)))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .font_family(mono_font.clone())
                            .text_color(rgb(if window.active {
                                theme.text
                            } else {
                                theme.text_muted
                            }))
                            .child(format!("#{} {}", window.index, window.name)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(format!(
                                "{} {}",
                                window.panes,
                                i18n.t("sidebar.host_tmux.columns.panes")
                            )),
                    )
                    .child(self.render_host_tmux_window_actions(
                        connection_id,
                        session,
                        &window,
                        tokens,
                        i18n,
                        cx,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            // Window disclosure state belongs to the Host Tools page entity.
                            if host_tools.ui.host_tmux_expanded_window_id.as_deref()
                                == Some(window_id.as_str())
                            {
                                host_tools.ui.host_tmux_expanded_window_id = None;
                            } else {
                                host_tools.ui.host_tmux_expanded_window_id =
                                    Some(window_id.clone());
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
            )
            .when(expanded, |card| {
                let mut body = div()
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA));
                // Pane rows stay lazy so collapsed windows do not allocate action closures.
                for pane in snapshot.panes_for_window(&window.id) {
                    body = body.child(self.render_host_tmux_pane_detail(
                        connection_id,
                        session,
                        pane,
                        tokens,
                        i18n,
                        mono_font.clone(),
                        cx,
                    ));
                }
                card.child(body)
            })
            .into_any_element()
    }

    fn render_host_tmux_pane_detail(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        pane: ResourceTmuxPane,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .id((gpui::ElementId::from("host-tmux-pane"), pane.id.clone()))
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
            .font_family(mono_font)
            .child(
                div()
                    .flex_none()
                    .w(px(42.0))
                    .text_color(rgb(if pane.active {
                        MONITOR_EMERALD
                    } else {
                        theme.text_muted
                    }))
                    .child(format!("%{}", pane.index)),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_color(rgb(theme.text))
                    .child(format!("{} · {}", pane.command, pane.path)),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{} · {}", pane.pid, pane.size)),
            )
            .child(self.render_host_tmux_pane_actions(
                connection_id,
                session,
                &pane,
                tokens,
                i18n,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_tmux_detail_line(label: String, value: String) -> AnyElement {
        div()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_none().child(label))
            .child(div().min_w_0().flex_1().truncate().child(value))
            .into_any_element()
    }

    fn active_tmux_window_label(
        snapshot: &ResourceTmuxSnapshot,
        session_id: &str,
        i18n: &I18n,
    ) -> String {
        snapshot
            .windows
            .iter()
            .find(|window| window.session_id == session_id && window.active)
            .map(|window| {
                i18n.t("sidebar.host_tmux.active_window")
                    .replace("{{name}}", &window.name)
                    .replace("{{index}}", &window.index.to_string())
            })
            .unwrap_or_else(|| i18n.t("sidebar.host_tmux.no_active_window"))
    }
}

fn host_tmux_confirm_description_key(action: &HostTmuxDestructiveAction) -> &'static str {
    match action {
        HostTmuxDestructiveAction::KillSession { .. } => {
            "sidebar.host_tmux.confirm.kill_session_desc"
        }
        HostTmuxDestructiveAction::KillWindow { .. } => {
            "sidebar.host_tmux.confirm.kill_window_desc"
        }
        HostTmuxDestructiveAction::KillPane { .. } => "sidebar.host_tmux.confirm.kill_pane_desc",
    }
}

fn host_tmux_confirm_label_key(action: &HostTmuxDestructiveAction) -> &'static str {
    match action {
        HostTmuxDestructiveAction::KillSession { .. } => "sidebar.host_tmux.actions.kill_session",
        HostTmuxDestructiveAction::KillWindow { .. } => "sidebar.host_tmux.actions.kill_window",
        HostTmuxDestructiveAction::KillPane { .. } => "sidebar.host_tmux.actions.kill_pane",
    }
}

fn host_tmux_input_title_key(kind: &HostTmuxInputDialogKind) -> &'static str {
    match kind {
        HostTmuxInputDialogKind::RenameSession { .. } => {
            "sidebar.host_tmux.input.rename_session_title"
        }
        HostTmuxInputDialogKind::RenameWindow { .. } => {
            "sidebar.host_tmux.input.rename_window_title"
        }
        HostTmuxInputDialogKind::SendPaneCommand { .. } => {
            "sidebar.host_tmux.input.send_command_title"
        }
    }
}

fn host_tmux_input_description_key(kind: &HostTmuxInputDialogKind) -> &'static str {
    match kind {
        HostTmuxInputDialogKind::RenameSession { .. } => {
            "sidebar.host_tmux.input.rename_session_desc"
        }
        HostTmuxInputDialogKind::RenameWindow { .. } => {
            "sidebar.host_tmux.input.rename_window_desc"
        }
        HostTmuxInputDialogKind::SendPaneCommand { .. } => {
            "sidebar.host_tmux.input.send_command_desc"
        }
    }
}

pub fn host_tmux_input_placeholder_key(kind: &HostTmuxInputDialogKind) -> &'static str {
    match kind {
        HostTmuxInputDialogKind::RenameSession { .. } => {
            "sidebar.host_tmux.input.rename_session_placeholder"
        }
        HostTmuxInputDialogKind::RenameWindow { .. } => {
            "sidebar.host_tmux.input.rename_window_placeholder"
        }
        HostTmuxInputDialogKind::SendPaneCommand { .. } => {
            "sidebar.host_tmux.input.send_command_placeholder"
        }
    }
}

pub fn host_tmux_input_submit_key(kind: &HostTmuxInputDialogKind) -> &'static str {
    match kind {
        HostTmuxInputDialogKind::RenameSession { .. } => "sidebar.host_tmux.actions.rename_session",
        HostTmuxInputDialogKind::RenameWindow { .. } => "sidebar.host_tmux.actions.rename_window",
        HostTmuxInputDialogKind::SendPaneCommand { .. } => "sidebar.host_tmux.actions.send_command",
    }
}

fn tmux_attached_color(attached: bool, muted_color: u32) -> u32 {
    if attached {
        MONITOR_EMERALD
    } else {
        muted_color
    }
}

fn tmux_time_label(timestamp: &str) -> String {
    let trimmed = timestamp.trim();
    if trimmed.is_empty() {
        "—".to_string()
    } else {
        trimmed.to_string()
    }
}
