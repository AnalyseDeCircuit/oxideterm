//! Owns the tmux Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::tmux_capture_snapshot;

use oxideterm_gpui_ui::button::ButtonVariant;

impl WorkspaceApp {
    pub(super) fn render_host_tmux_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let snapshot = self.host_tools.read(cx).tmux_snapshot_for(selected_id);
        let tmux_search_query = self.host_tools.read(cx).ui.host_tmux_search_query.clone();
        let rows = snapshot
            .as_ref()
            .map(|snapshot| visible_tmux_session_rows(snapshot, &tmux_search_query))
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_tmux_list_state(&rows, snapshot.as_ref(), selected_id, cx);

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
                    .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher_row(
                        &connections,
                        selected_id,
                        !self.host_tools.read(cx).tmux_snapshot_polling(),
                        cx,
                    ))
                    .child(self.render_host_tmux_search(cx))
                    .child(self.render_host_tmux_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        cx,
                    )),
            )
            .child(self.render_host_tmux_list(
                rows,
                snapshot.as_ref(),
                self.host_tools.read(cx).tmux_snapshot_polling(),
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_tmux_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostTmuxSearch;
        let (focused, value) = {
            let ui = &self.host_tools.read(cx).ui;
            (
                ui.input_is_focused(HostToolsTextInput::TmuxSearch),
                ui.host_tmux_search_query.clone(),
            )
        };
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder: self.i18n.t("sidebar.host_tmux.search_placeholder"),
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
                        host_tools.ui.focus_input(HostToolsTextInput::TmuxSearch);
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

    pub(super) fn render_host_tmux_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceTmuxStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let capability_label = match status {
            ResourceTmuxStatus::Available {
                capability: TmuxCommandCapability::Full,
                ..
            } => self.i18n.t("sidebar.host_tmux.capability.full"),
            ResourceTmuxStatus::Available {
                capability: TmuxCommandCapability::Partial,
                ..
            } => self.i18n.t("sidebar.host_tmux.capability.partial"),
            _ => self.i18n.t("sidebar.host_tmux.capability.unknown"),
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
                self.i18n.t("sidebar.host_tmux.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::Plus,
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
                        self.i18n.t("sidebar.host_tmux.actions.new_session"),
                        "host-tmux-new-session",
                        true,
                        cx.listener({
                            let selected_id = selected_id.clone();
                            move |this, _event, window, cx| {
                                this.open_host_tmux_new_session_terminal(
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
                            disabled: self.host_tools.read(cx).tmux_snapshot_polling(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        self.i18n.t("sidebar.host_tmux.actions.refresh"),
                        "host-tmux-refresh",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.request_host_tmux_snapshot(
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

    pub(super) fn render_host_tmux_list(
        &self,
        rows: Vec<ResourceTmuxSession>,
        snapshot: Option<&ResourceTmuxSnapshot>,
        loading: bool,
        status: ResourceTmuxStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Terminal,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_tmux.loading"),
                cx,
            );
        }
        match status {
            ResourceTmuxStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Terminal,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_tmux.unavailable"),
                    cx,
                );
            }
            ResourceTmuxStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_tmux.error", &[("error", message)]),
                    cx,
                );
            }
            ResourceTmuxStatus::Unknown | ResourceTmuxStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Terminal,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_tmux.empty"),
                cx,
            );
        }

        let snapshot = Arc::new(snapshot.cloned().unwrap_or_default());
        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.host_tools.read(cx).ui.host_tmux_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        let show_context_columns =
            self.ai.chat.sidebar_width >= HOST_TMUX_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_tmux_table_header(show_context_columns))
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
                            let snapshot = snapshot.clone();
                            let selected_id = selected_id.clone();
                            workspace.update(cx, |this, cx| {
                                this.render_host_tmux_row(
                                    selected_id.as_str(),
                                    snapshot.as_ref(),
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

    pub(super) fn render_host_tmux_table_header(&self, show_context_columns: bool) -> AnyElement {
        let theme = self.tokens.ui;
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
                    .child(self.i18n.t("sidebar.host_tmux.columns.session")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_TMUX_ATTACHED_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_tmux.columns.attached")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_TMUX_WINDOWS_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_tmux.columns.windows")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_TMUX_PANES_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_tmux.columns.panes")),
            )
            .when(show_context_columns, |header| {
                header.child(
                    div()
                        .flex_none()
                        .w(px(HOST_TMUX_ACTIVITY_COLUMN_WIDTH))
                        .truncate()
                        .child(self.i18n.t("sidebar.host_tmux.columns.activity")),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_host_tmux_row(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: Option<ResourceTmuxSession>,
        show_context_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = session else {
            return div().into_any_element();
        };
        let expanded = self
            .host_tools
            .read(cx)
            .ui
            .host_tmux_expanded_session_id
            .as_deref()
            == Some(session.id.as_str());
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let pane_count = snapshot.pane_count_for_session(&session.id);
        let attached_label = if session.attached {
            self.i18n.t("sidebar.host_tmux.attached.yes")
        } else {
            self.i18n.t("sidebar.host_tmux.attached.no")
        };

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
                    .h(px(HOST_TMUX_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Keep the session identity as a first-level flex child.
                    // Nested fixed wrappers are how earlier Host Tools tables collapsed names to `...`.
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
                            .w(px(HOST_TMUX_ATTACHED_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(tmux_attached_color(
                                session.attached,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(attached_label),
                    )
                    .child(
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
                    .when(show_context_columns, |row| {
                        row.child(
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
                            .font_family(mono_font)
                            .child(format!(
                                "{} · {}",
                                session.id,
                                self.active_tmux_window_label(snapshot, &session.id)
                            )),
                    )
                    .child(self.render_host_tmux_inline_actions(connection_id, &session, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_tmux_session_detail(
                    connection_id,
                    snapshot,
                    &session,
                    cx,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = session.id.clone();
                    move |this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, _cx| {
                            let ui = &mut host_tools.ui;
                            if ui.host_tmux_expanded_session_id.as_deref() == Some(id.as_str()) {
                                ui.host_tmux_expanded_session_id = None;
                            } else {
                                ui.host_tmux_expanded_session_id = Some(id.clone());
                            }
                            ui.host_tmux_expanded_window_id = None;
                        });
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_tmux_inline_actions(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self
            .host_tools
            .read(cx)
            .tmux_action_running_for(&session.id);
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::Terminal,
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
                self.i18n.t("sidebar.host_tmux.actions.attach"),
                "host-tmux-attach",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    let session_id = session.id.clone();
                    let session_name = session.name.clone();
                    move |this, _event, window, cx| {
                        this.open_host_tmux_attach_terminal(
                            connection_id.clone(),
                            session_id.clone(),
                            session_name.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::Pencil,
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
                self.i18n.t("sidebar.host_tmux.actions.rename_session"),
                "host-tmux-rename-session",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    let session_id = session.id.clone();
                    let session_name = session.name.clone();
                    move |this, _event, window, cx| {
                        this.open_host_tmux_rename_session_dialog(
                            connection_id.clone(),
                            session_id.clone(),
                            session_name.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::Trash2,
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
                self.i18n.t("sidebar.host_tmux.actions.kill_session"),
                "host-tmux-kill-session",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    let session_id = session.id.clone();
                    let session_name = session.name.clone();
                    move |this, _event, _window, cx| {
                        this.request_host_tmux_kill_session(
                            connection_id.clone(),
                            session_id.clone(),
                            session_name.clone(),
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(super) fn render_host_tmux_session_detail(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: &ResourceTmuxSession,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let windows = snapshot.windows_for_session(&session.id);
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
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_tmux.columns.created"),
                tmux_time_label(&session.created),
            ))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_tmux.columns.activity"),
                tmux_time_label(&session.activity),
            ));
        for window in windows {
            detail = detail.child(self.render_host_tmux_window_detail(
                connection_id,
                snapshot,
                session,
                &window,
                cx,
            ));
        }
        detail.into_any_element()
    }

    pub(super) fn render_host_tmux_window_detail(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: &ResourceTmuxSession,
        window: &ResourceTmuxWindow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let expanded = self
            .host_tools
            .read(cx)
            .ui
            .host_tmux_expanded_window_id
            .as_deref()
            == Some(window.id.as_str());
        let panes = snapshot.panes_for_window(&window.id);
        div()
            .mt_1()
            .rounded(px(self.tokens.radii.md))
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
                            .font_family(mono_font)
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
                                self.i18n.t("sidebar.host_tmux.columns.panes")
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(3.0))
                            .child(
                                self.workspace_tooltip_icon_button(
                                    LucideIcon::Pencil,
                                    12.0,
                                    rgb(theme.text),
                                    oxideterm_gpui_ui::button::IconButtonOptions {
                                        size: 20.0,
                                        disabled: self
                                            .host_tools
                                            .read(cx)
                                            .tmux_action_running_for(&session.id),
                                        has_background: true,
                                        background: Some(rgb(theme.bg_hover)),
                                        hover_background: Some(rgb(theme.bg_panel)),
                                        idle_opacity: 1.0,
                                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                            20.0,
                                        )
                                    },
                                    self.i18n.t("sidebar.host_tmux.actions.rename_window"),
                                    "host-tmux-rename-window",
                                    true,
                                    cx.listener({
                                        let connection_id = connection_id.to_string();
                                        let session_id = session.id.clone();
                                        let session_name = session.name.clone();
                                        let window_id = window.id.clone();
                                        let window_label =
                                            format!("#{} {}", window.index, window.name);
                                        let window_name = window.name.clone();
                                        move |this, _event, window, cx| {
                                            this.open_host_tmux_rename_window_dialog(
                                                connection_id.clone(),
                                                session_id.clone(),
                                                session_name.clone(),
                                                window_id.clone(),
                                                window_label.clone(),
                                                window_name.clone(),
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }
                                    }),
                                    cx.entity(),
                                ),
                            )
                            .child(
                                self.workspace_tooltip_icon_button(
                                    LucideIcon::Trash2,
                                    12.0,
                                    rgb(MONITOR_RED),
                                    oxideterm_gpui_ui::button::IconButtonOptions {
                                        size: 20.0,
                                        disabled: self
                                            .host_tools
                                            .read(cx)
                                            .tmux_action_running_for(&session.id),
                                        has_background: true,
                                        background: Some(rgba(
                                            (MONITOR_RED << 8) | MONITOR_TINT_ALPHA,
                                        )),
                                        hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                                        idle_opacity: 1.0,
                                        ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                            20.0,
                                        )
                                    },
                                    self.i18n.t("sidebar.host_tmux.actions.kill_window"),
                                    "host-tmux-kill-window",
                                    true,
                                    cx.listener({
                                        let connection_id = connection_id.to_string();
                                        let session_id = session.id.clone();
                                        let session_name = session.name.clone();
                                        let window_id = window.id.clone();
                                        let window_label =
                                            format!("#{} {}", window.index, window.name);
                                        move |this, _event, _window, cx| {
                                            this.request_host_tmux_kill_window(
                                                connection_id.clone(),
                                                session_id.clone(),
                                                session_name.clone(),
                                                window_id.clone(),
                                                window_label.clone(),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }
                                    }),
                                    cx.entity(),
                                ),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let id = window.id.clone();
                            move |this, _event, _window, cx| {
                                this.host_tools.update(cx, |host_tools, _cx| {
                                    let expanded_window_id =
                                        &mut host_tools.ui.host_tmux_expanded_window_id;
                                    if expanded_window_id.as_deref() == Some(id.as_str()) {
                                        *expanded_window_id = None;
                                    } else {
                                        *expanded_window_id = Some(id.clone());
                                    }
                                });
                                cx.notify();
                                cx.stop_propagation();
                            }
                        }),
                    ),
            )
            .when(expanded, |card| {
                let mut body = div()
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA));
                for pane in panes {
                    body = body.child(self.render_host_tmux_pane_detail(
                        connection_id,
                        session,
                        &pane,
                        cx,
                    ));
                }
                card.child(body)
            })
            .into_any_element()
    }

    pub(super) fn render_host_tmux_pane_detail(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        pane: &ResourceTmuxPane,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
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
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .child(
                        self.workspace_tooltip_icon_button(
                            LucideIcon::Keyboard,
                            12.0,
                            rgb(theme.text),
                            oxideterm_gpui_ui::button::IconButtonOptions {
                                size: 20.0,
                                disabled: self
                                    .host_tools
                                    .read(cx)
                                    .tmux_action_running_for(&session.id),
                                has_background: true,
                                background: Some(rgb(theme.bg_hover)),
                                hover_background: Some(rgb(theme.bg_panel)),
                                idle_opacity: 1.0,
                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                            },
                            self.i18n.t("sidebar.host_tmux.actions.send_command"),
                            "host-tmux-send-pane-command",
                            true,
                            cx.listener({
                                let connection_id = connection_id.to_string();
                                let session_id = session.id.clone();
                                let session_name = session.name.clone();
                                let pane_id = pane.id.clone();
                                let pane_label = format!("%{} {}", pane.index, pane.command);
                                move |this, _event, window, cx| {
                                    this.open_host_tmux_send_pane_command_dialog(
                                        connection_id.clone(),
                                        session_id.clone(),
                                        session_name.clone(),
                                        pane_id.clone(),
                                        pane_label.clone(),
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            }),
                            cx.entity(),
                        ),
                    )
                    .child(
                        self.workspace_tooltip_icon_button(
                            LucideIcon::Trash2,
                            12.0,
                            rgb(MONITOR_RED),
                            oxideterm_gpui_ui::button::IconButtonOptions {
                                size: 20.0,
                                disabled: self
                                    .host_tools
                                    .read(cx)
                                    .tmux_action_running_for(&session.id),
                                has_background: true,
                                background: Some(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)),
                                hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                                idle_opacity: 1.0,
                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                            },
                            self.i18n.t("sidebar.host_tmux.actions.kill_pane"),
                            "host-tmux-kill-pane",
                            true,
                            cx.listener({
                                let connection_id = connection_id.to_string();
                                let session_id = session.id.clone();
                                let session_name = session.name.clone();
                                let pane_id = pane.id.clone();
                                let pane_label = format!("%{} {}", pane.index, pane.command);
                                move |this, _event, _window, cx| {
                                    this.request_host_tmux_kill_pane(
                                        connection_id.clone(),
                                        session_id.clone(),
                                        session_name.clone(),
                                        pane_id.clone(),
                                        pane_label.clone(),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            }),
                            cx.entity(),
                        ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn active_tmux_window_label(
        &self,
        snapshot: &ResourceTmuxSnapshot,
        session_id: &str,
    ) -> String {
        snapshot
            .windows_for_session(session_id)
            .into_iter()
            .find(|window| window.active)
            .map(|window| {
                self.i18n_replace(
                    "sidebar.host_tmux.active_window",
                    &[("name", window.name), ("index", window.index.to_string())],
                )
            })
            .unwrap_or_else(|| self.i18n.t("sidebar.host_tmux.no_active_window"))
    }

    pub(super) fn sync_host_tmux_list_state(
        &self,
        rows: &[ResourceTmuxSession],
        snapshot: Option<&ResourceTmuxSnapshot>,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.host_tools.update(cx, |host_tools, _cx| {
            let ui = &host_tools.ui;
            let signatures = rows
                .iter()
                .map(|session| {
                    let expanded =
                        ui.host_tmux_expanded_session_id.as_deref() == Some(session.id.as_str());
                    let child_count = if expanded {
                        let window_count = snapshot
                            .map(|snapshot| snapshot.windows_for_session(&session.id).len())
                            .unwrap_or_default();
                        let pane_count = ui
                            .host_tmux_expanded_window_id
                            .as_deref()
                            .and_then(|window_id| {
                                snapshot.map(|snapshot| snapshot.panes_for_window(window_id).len())
                            })
                            .unwrap_or_default();
                        window_count + pane_count
                    } else {
                        0
                    };
                    tmux_session_row_signature(session, expanded, child_count)
                })
                .collect::<Vec<_>>();
            let identity = format!(
                "host-tmux:{selected_id}:{}:{}:{}",
                ui.host_tmux_search_query,
                ui.host_tmux_expanded_session_id
                    .as_deref()
                    .unwrap_or_default(),
                ui.host_tmux_expanded_window_id
                    .as_deref()
                    .unwrap_or_default()
            );
            sync_tauri_variable_list_state_by_signatures(
                &ui.host_tmux_list_state,
                &mut ui.host_tmux_list_cache.borrow_mut(),
                &identity,
                &signatures,
                TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8),
            );
        });
    }

    pub(in crate::workspace) fn handle_host_tmux_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::TmuxSearch)
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

    pub(super) fn request_host_tmux_snapshot_for_selected_connection(
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
        self.request_host_tmux_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(in crate::workspace) fn request_host_tmux_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if !self.host_tool_monitoring_enabled(ContextSidebarTool::Tmux)
            || !self.host_tools_surface_visible()
            || self.host_tools.read(cx).active_tool() != ContextSidebarTool::Tmux
        {
            return;
        }
        let failure_fallback = self.i18n.t("sidebar.host_tmux.toast.unknown_error");
        let unavailable_fallback = self.i18n.t("sidebar.host_tmux.unavailable");
        let search_query = self.host_tools.read(cx).ui.host_tmux_search_query.clone();
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_tmux_snapshot(
                connection_id,
                feedback,
                search_query,
                failure_fallback,
                unavailable_fallback,
                runtime,
                cx,
            )
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    pub(super) fn request_host_tmux_kill_session(
        &mut self,
        connection_id: String,
        session_id: String,
        session_name: String,
        cx: &mut Context<Self>,
    ) {
        let notice = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.open_tmux_action_confirm(
                HostTmuxActionRequest {
                    connection_id,
                    session_id: session_id.clone(),
                    session_name: session_name.clone(),
                    target_label: session_name,
                    action: HostTmuxDestructiveAction::KillSession { target: session_id },
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

    pub(super) fn request_host_tmux_kill_window(
        &mut self,
        connection_id: String,
        session_id: String,
        session_name: String,
        window_id: String,
        window_label: String,
        cx: &mut Context<Self>,
    ) {
        let notice = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.open_tmux_action_confirm(
                HostTmuxActionRequest {
                    connection_id,
                    session_id,
                    session_name,
                    target_label: window_label,
                    action: HostTmuxDestructiveAction::KillWindow { target: window_id },
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

    pub(super) fn request_host_tmux_kill_pane(
        &mut self,
        connection_id: String,
        session_id: String,
        session_name: String,
        pane_id: String,
        pane_label: String,
        cx: &mut Context<Self>,
    ) {
        let notice = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.open_tmux_action_confirm(
                HostTmuxActionRequest {
                    connection_id,
                    session_id,
                    session_name,
                    target_label: pane_label,
                    action: HostTmuxDestructiveAction::KillPane { target: pane_id },
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

    pub(super) fn open_host_tmux_rename_session_dialog(
        &mut self,
        connection_id: String,
        session_id: String,
        session_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_host_tmux_input_dialog(
            HostTmuxInputDialog {
                connection_id,
                session_id: session_id.clone(),
                session_name: session_name.clone(),
                target_label: session_name.clone(),
                value: zeroize::Zeroizing::new(session_name),
                kind: HostTmuxInputDialogKind::RenameSession { target: session_id },
            },
            window,
            cx,
        );
    }

    pub(super) fn open_host_tmux_rename_window_dialog(
        &mut self,
        connection_id: String,
        session_id: String,
        session_name: String,
        window_id: String,
        window_label: String,
        window_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_host_tmux_input_dialog(
            HostTmuxInputDialog {
                connection_id,
                session_id,
                session_name,
                target_label: window_label,
                value: zeroize::Zeroizing::new(window_name),
                kind: HostTmuxInputDialogKind::RenameWindow { target: window_id },
            },
            window,
            cx,
        );
    }

    pub(super) fn open_host_tmux_send_pane_command_dialog(
        &mut self,
        connection_id: String,
        session_id: String,
        session_name: String,
        pane_id: String,
        pane_label: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_host_tmux_input_dialog(
            HostTmuxInputDialog {
                connection_id,
                session_id,
                session_name,
                target_label: pane_label,
                value: zeroize::Zeroizing::new(String::new()),
                kind: HostTmuxInputDialogKind::SendPaneCommand { target: pane_id },
            },
            window,
            cx,
        );
    }

    pub(super) fn open_host_tmux_input_dialog(
        &mut self,
        dialog: HostTmuxInputDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.open_tmux_input_dialog(dialog, cx);
        });
        self.ime_marked_text = None;
        self.clear_ime_selection();
        self.new_connection_caret_visible = true;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(super) fn open_host_tmux_attach_terminal(
        &mut self,
        connection_id: String,
        session_id: String,
        session_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self
            .host_tools
            .read(cx)
            .tmux_attach_command(&connection_id, &session_id)
        {
            Ok(command) => command,
            Err(error) => {
                self.push_host_tmux_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        let title = self.i18n_replace(
            "sidebar.host_tmux.attach_title",
            &[("name", session_name.clone())],
        );
        self.open_host_tmux_terminal_command(
            connection_id,
            session_name,
            command,
            title,
            "sidebar.host_tmux.toast.attach_opened",
            window,
            cx,
        );
    }

    pub(super) fn open_host_tmux_new_session_terminal(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self
            .host_tools
            .read(cx)
            .tmux_new_session_command(&connection_id)
        {
            Ok(command) => command,
            Err(error) => {
                self.push_host_tmux_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        let name = self.i18n.t("sidebar.host_tmux.new_session_name");
        let title = self.i18n.t("sidebar.host_tmux.new_session_title");
        self.open_host_tmux_terminal_command(
            connection_id,
            name,
            command,
            title,
            "sidebar.host_tmux.toast.new_session_opened",
            window,
            cx,
        );
    }

    pub(super) fn open_host_tmux_terminal_command(
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
            self.push_host_tmux_toast(
                self.i18n.t("sidebar.host_tmux.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        if !self.ssh_nodes.contains_key(&node_id) {
            self.push_host_tmux_toast(
                self.i18n.t("sidebar.host_tmux.toast.exec_terminal_missing"),
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
            Ok(()) => self.push_host_tmux_toast(
                self.i18n_replace(opened_toast_key, &[("name", name)]),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_tmux_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_host_tmux_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).tmux_confirm_view().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_tmux_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_tmux_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn handle_host_tmux_input_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).ui.host_tmux_input_dialog.is_none() {
            return false;
        }
        if event.keystroke.modifiers.platform {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_tmux_input_dialog(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                true
            }
            "enter" => {
                self.submit_host_tmux_input_dialog(cx);
                true
            }
            _ => false,
        }
    }

    pub(super) fn confirm_host_tmux_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_tmux_action(delay, runtime, cx)
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_tmux_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_tmux_confirm_exit(delay, cx)
        })
    }

    pub(super) fn submit_host_tmux_input_dialog(&mut self, cx: &mut Context<Self>) {
        if self.host_tools.read(cx).tmux_action_running() {
            self.push_host_tools_notice(HostToolsNotice::TmuxActionAlreadyRunning);
            return;
        }
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.submit_tmux_input(runtime, cx)
        });
        self.ime_marked_text = None;
        self.clear_ime_selection();
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
    }

    pub(super) fn push_host_tmux_toast(&mut self, message: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    pub(in crate::workspace) fn render_host_tmux_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (request, phase) = self.host_tools.read(cx).tmux_confirm_view()?;
        let title = self.i18n.t("sidebar.host_tmux.confirm.title");
        let description = self.i18n_replace(
            host_tmux_confirm_description_key(&request.action),
            &[
                ("name", request.session_name.clone()),
                ("id", request.session_id.clone()),
                ("target", request.target_label.clone()),
            ],
        );
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "host-tmux-confirm-motion",
                phase,
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Danger,
                    title: div().child(title).into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("sidebar.host_tmux.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t(host_tmux_confirm_label_key(&request.action)))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.begin_host_tmux_confirm_exit(cx);
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_host_tmux_action(cx);
                }),
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_host_tmux_input_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::HostTmuxDialogInput;
        let (kind, session_name, target_label, submit_disabled, input_control) = {
            let host_tools = self.host_tools.read(cx);
            let ui = &host_tools.ui;
            let dialog = ui.host_tmux_input_dialog.as_ref()?;
            let input_control = text_input(
                &self.tokens,
                TextInputView {
                    value: dialog.value.as_str(),
                    placeholder: self.i18n.t(host_tmux_input_placeholder_key(&dialog.kind)),
                    focused: ui.input_is_focused(HostToolsTextInput::TmuxDialog),
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam);
            (
                dialog.kind.clone(),
                dialog.session_name.clone(),
                dialog.target_label.clone(),
                dialog.value.trim().is_empty() || host_tools.tmux_action_running(),
                input_control,
            )
        };
        let title = self.i18n.t(host_tmux_input_title_key(&kind));
        let description = self.i18n_replace(
            host_tmux_input_description_key(&kind),
            &[("name", session_name), ("target", target_label)],
        );
        let submit_label = self.i18n.t(host_tmux_input_submit_key(&kind));
        let workspace = cx.entity();

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_tmux_input_dialog(cx);
                        });
                        this.ime_marked_text = None;
                        this.clear_ime_selection();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
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
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.text_muted))
                                        .child(description),
                                ),
                        )
                        .child(
                            div().px_4().py_4().child(text_input_anchor_probe(
                                target.anchor_id(),
                                input_control
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, event: &MouseDownEvent, window, cx| {
                                                this.host_tools.update(cx, |host_tools, _cx| {
                                                    host_tools.ui.focus_input(
                                                        HostToolsTextInput::TmuxDialog,
                                                    );
                                                });
                                                this.ime_marked_text = None;
                                                this.new_connection_caret_visible = true;
                                                window.focus(&this.focus_handle, cx);
                                                this.begin_ime_selection_from_mouse_down(
                                                    target, event, window, cx,
                                                );
                                                cx.stop_propagation();
                                            },
                                        ),
                                    )
                                    .on_mouse_move(cx.listener(
                                        |this, event: &MouseMoveEvent, window, cx| {
                                            this.update_ime_selection_drag_from_mouse_move(
                                                event, window, cx,
                                            );
                                        },
                                    )),
                                move |anchor, _window, cx| {
                                    let _ = workspace.update(cx, |this, cx| {
                                        this.update_text_input_anchor(anchor, cx);
                                    });
                                },
                            )),
                        )
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
                                .child(self.workspace_confirm_footer_action_button(
                                    self.i18n.t("sidebar.host_tmux.confirm.cancel"),
                                    ButtonVariant::Secondary,
                                    ConfirmDialogAction::Cancel,
                                    false,
                                    None,
                                    |this, _event, _window, cx| {
                                        this.host_tools.update(cx, |host_tools, cx| {
                                            host_tools.dismiss_tmux_input_dialog(cx);
                                        });
                                        this.ime_marked_text = None;
                                        this.clear_ime_selection();
                                        cx.notify();
                                    },
                                    cx,
                                ))
                                .child(self.workspace_confirm_footer_action_button(
                                    submit_label,
                                    ButtonVariant::Default,
                                    ConfirmDialogAction::Confirm,
                                    submit_disabled,
                                    None,
                                    |this, _event, _window, cx| {
                                        this.submit_host_tmux_input_dialog(cx);
                                    },
                                    cx,
                                )),
                        ),
                ))
                .into_any_element(),
        )
    }
}

impl HostToolsEntity {
    pub(super) fn tmux_snapshot_for(&self, connection_id: &str) -> Option<ResourceTmuxSnapshot> {
        (self.host_tmux.snapshot_connection_id.as_deref() == Some(connection_id))
            .then(|| self.host_tmux.snapshot.clone())
            .flatten()
    }

    pub(super) fn tmux_snapshot_polling(&self) -> bool {
        self.host_tmux.snapshot_polling
    }

    pub(super) fn tmux_action_running_for(&self, session_id: &str) -> bool {
        self.host_tmux
            .action_running
            .as_ref()
            .is_some_and(|request| request.session_id == session_id)
    }

    pub(super) fn tmux_action_running(&self) -> bool {
        self.host_tmux.action_running.is_some()
    }

    pub(super) fn tmux_attach_command(
        &self,
        connection_id: &str,
        target: &str,
    ) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_attach_command(&os_type, target)
    }

    pub(super) fn tmux_new_session_command(&self, connection_id: &str) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_new_session_command(&os_type, None)
    }

    pub(super) fn request_tmux_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        search_query: String,
        failure_fallback: String,
        unavailable_fallback: String,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self.host_tmux.snapshot_polling {
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxSnapshotAlreadyRunning]
            } else {
                Vec::new()
            };
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxConnectionMissing]
            } else {
                Vec::new()
            };
        };
        let command = build_tmux_snapshot_command(&os_type);
        let request = HostTmuxSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            search_query,
            failure_fallback,
            unavailable_fallback,
        };
        self.host_tmux.snapshot_connection_id = Some(connection_id);
        self.host_tmux.snapshot_running = Some(request.clone());
        self.host_tmux.snapshot_polling = true;
        self.host_tmux.last_error = None;
        let spawned = self.spawn_tmux_snapshot_capture(
            command.command,
            request,
            HOST_TMUX_SNAPSHOT_TIMEOUT,
            HOST_TMUX_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_tmux.snapshot_running = None;
            self.host_tmux.snapshot_polling = false;
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxConnectionMissing]
            } else {
                Vec::new()
            };
        }
        cx.notify();
        Vec::new()
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_tmux_snapshot(
        &mut self,
        delivery: HostTmuxSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.snapshot_running.as_ref() != Some(&delivery.request) {
            return;
        }
        let feedback = delivery.request.feedback;
        self.host_tmux.snapshot_polling = false;
        self.host_tmux.snapshot_running = None;
        let (snapshot, notice) = match delivery.result {
            Ok(mut output) => {
                let mut snapshot =
                    tmux_capture_snapshot(&output.stdout, &output.stderr, output.exit_code);
                zeroize::Zeroize::zeroize(&mut output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stderr);
                let notice = match snapshot.status.clone() {
                    ResourceTmuxStatus::Available { .. } => {
                        self.host_tmux.last_error = None;
                        Some(HostToolsNotice::TmuxSnapshotLoaded {
                            count: visible_tmux_session_rows(
                                &snapshot,
                                &delivery.request.search_query,
                            )
                            .len(),
                        })
                    }
                    ResourceTmuxStatus::Unavailable => {
                        self.host_tmux.last_error =
                            Some(delivery.request.unavailable_fallback.clone());
                        Some(HostToolsNotice::TmuxUnavailable)
                    }
                    ResourceTmuxStatus::Error { .. } => {
                        snapshot.status = ResourceTmuxStatus::Error {
                            message: delivery.request.failure_fallback.clone(),
                        };
                        self.host_tmux.last_error = Some(delivery.request.failure_fallback.clone());
                        Some(HostToolsNotice::TmuxSnapshotFailed)
                    }
                    ResourceTmuxStatus::Unknown => None,
                };
                (snapshot, notice)
            }
            Err(()) => {
                self.host_tmux.last_error = Some(delivery.request.failure_fallback.clone());
                (
                    ResourceTmuxSnapshot {
                        status: ResourceTmuxStatus::Error {
                            message: delivery.request.failure_fallback.clone(),
                        },
                        sessions: Vec::new(),
                        windows: Vec::new(),
                        panes: Vec::new(),
                    },
                    Some(HostToolsNotice::TmuxSnapshotFailed),
                )
            }
        };
        self.host_tmux.snapshot_connection_id = Some(delivery.request.connection_id);
        self.host_tmux.snapshot = Some(snapshot);
        if feedback.should_toast()
            && let Some(notice) = notice
        {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
        cx.notify();
    }

    pub(in crate::workspace::connection_monitor) fn open_tmux_action_confirm(
        &mut self,
        request: HostTmuxActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_tmux.action_running.is_some() {
            return Some(HostToolsNotice::TmuxActionAlreadyRunning);
        }
        HostToolConfirmState::open(&mut self.host_tmux.pending_confirm, request);
        cx.notify();
        None
    }

    pub(in crate::workspace::connection_monitor) fn tmux_confirm_view(
        &self,
    ) -> Option<(HostTmuxActionRequest, oxideterm_gpui_ui::motion::ExitPhase)> {
        self.host_tmux
            .pending_confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    pub(in crate::workspace::connection_monitor) fn dismiss_tmux_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn begin_tmux_confirm_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self
            .host_tmux
            .pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_tmux.pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_tmux
                    .pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_tmux.pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(super) fn confirm_tmux_action(
        &mut self,
        delay: Duration,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(request) = self
            .host_tmux
            .pending_confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return Vec::new();
        };
        if !self.begin_tmux_confirm_exit(delay, cx) {
            return Vec::new();
        }
        self.start_tmux_action(request, runtime, cx)
    }

    fn start_tmux_action(
        &mut self,
        request: HostTmuxActionRequest,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let HostTmuxActionRequest {
            connection_id,
            session_id,
            session_name,
            target_label,
            action,
        } = request;
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return vec![HostToolsNotice::TmuxConnectionMissing];
        };
        let action = match action {
            HostTmuxDestructiveAction::KillSession { target } => {
                TmuxActionKind::KillSession { target }
            }
            HostTmuxDestructiveAction::KillWindow { target } => {
                TmuxActionKind::KillWindow { target }
            }
            HostTmuxDestructiveAction::KillPane { target } => TmuxActionKind::KillPane { target },
        };
        let command = match build_tmux_action_command(&os_type, action) {
            Ok(command) => zeroize::Zeroizing::new(command.command),
            Err(_) => return vec![HostToolsNotice::TmuxActionFailed],
        };
        let request = HostTmuxActionRun {
            connection_id,
            session_id,
            session_name,
            target_label,
        };
        self.start_tmux_action_command(command, request, runtime, cx)
    }

    fn start_tmux_action_command(
        &mut self,
        command: zeroize::Zeroizing<String>,
        request: HostTmuxActionRun,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        self.host_tmux.action_running = Some(request.clone());
        let spawned = self.spawn_tmux_action(
            command,
            request,
            HOST_TMUX_ACTION_TIMEOUT,
            HOST_TMUX_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_tmux.action_running = None;
            return vec![HostToolsNotice::TmuxConnectionMissing];
        }
        cx.notify();
        Vec::new()
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_tmux_action(
        &mut self,
        delivery: HostTmuxActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.action_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_tmux.action_running = None;
        let HostTmuxActionRun {
            connection_id,
            target_label,
            ..
        } = delivery.request;
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::TmuxActionFinished {
                target_label,
                succeeded: delivery.result.unwrap_or(false),
            },
        ));
        self.refresh_tmux_snapshot_after_action(connection_id, cx);
        cx.notify();
    }

    fn refresh_tmux_snapshot_after_action(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.tmux_enabled
            || !self.visibility.is_visible()
            || self.active_tool() != ContextSidebarTool::Tmux
        {
            return;
        }
        let (Some(runtime), Some(messages)) =
            (self.lifecycle_runtime.clone(), self.messages.as_ref())
        else {
            return;
        };
        let search_query = self.ui.host_tmux_search_query.clone();
        let failure_fallback = messages.tmux_unknown_error.clone();
        let unavailable_fallback = messages.tmux_unavailable.clone();
        let notices = self.request_tmux_snapshot(
            connection_id,
            HostSnapshotFeedback::Silent,
            search_query,
            failure_fallback,
            unavailable_fallback,
            runtime,
            cx,
        );
        debug_assert!(notices.is_empty());
    }

    pub(in crate::workspace::connection_monitor) fn open_tmux_input_dialog(
        &mut self,
        dialog: HostTmuxInputDialog,
        cx: &mut Context<Self>,
    ) {
        self.ui.host_tmux_input_dialog = Some(dialog);
        self.ui.focus_input(HostToolsTextInput::TmuxDialog);
        cx.notify();
    }

    pub(in crate::workspace::connection_monitor) fn dismiss_tmux_input_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ui.host_tmux_input_dialog.take().is_some() {
            if self.ui.input_is_focused(HostToolsTextInput::TmuxDialog) {
                self.ui.clear_input_focus();
            }
            cx.notify();
        }
    }

    pub(in crate::workspace::connection_monitor) fn submit_tmux_input(
        &mut self,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self.host_tmux.action_running.is_some() {
            return vec![HostToolsNotice::TmuxActionAlreadyRunning];
        }
        let Some(dialog) = self.ui.host_tmux_input_dialog.as_ref() else {
            return Vec::new();
        };
        if dialog.value.trim().is_empty() {
            return vec![HostToolsNotice::TmuxInputRequired];
        }
        let mut dialog = self
            .ui
            .host_tmux_input_dialog
            .take()
            .expect("tmux input dialog remains present after validation");
        self.ui.clear_input_focus();
        let trimmed_start = dialog.value.len() - dialog.value.trim_start().len();
        let trimmed_end = dialog.value.trim_end().len();
        dialog.value.truncate(trimmed_end);
        if trimmed_start > 0 {
            dialog.value.drain(..trimmed_start);
        }
        let Some(os_type) = self.connection_os_type(&dialog.connection_id) else {
            return vec![HostToolsNotice::TmuxConnectionMissing];
        };
        let command = match &dialog.kind {
            HostTmuxInputDialogKind::RenameSession { target } => {
                build_tmux_rename_session_command(&os_type, target, dialog.value.as_str())
            }
            HostTmuxInputDialogKind::RenameWindow { target } => {
                build_tmux_rename_window_command(&os_type, target, dialog.value.as_str())
            }
            HostTmuxInputDialogKind::SendPaneCommand { target } => {
                build_tmux_send_pane_command(&os_type, target, dialog.value.as_str())
            }
        };
        // The original input clears here; the generated shell command has its
        // own zeroizing buffer until the SSH worker finishes.
        zeroize::Zeroize::zeroize(&mut dialog.value);
        let command = match command {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::TmuxActionFailed],
        };
        let request = HostTmuxActionRun {
            connection_id: dialog.connection_id,
            session_id: dialog.session_id,
            session_name: dialog.session_name,
            target_label: dialog.target_label,
        };
        self.start_tmux_action_command(command, request, runtime, cx)
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

fn host_tmux_input_placeholder_key(kind: &HostTmuxInputDialogKind) -> &'static str {
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

fn host_tmux_input_submit_key(kind: &HostTmuxInputDialogKind) -> &'static str {
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
