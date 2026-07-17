//! Owns the tmux Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::tmux_capture_snapshot;

use oxideterm_gpui_ui::button::ButtonVariant;

impl WorkspaceApp {
    pub(super) fn render_host_tmux_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
            .host_tmux_snapshot
            .as_ref()
            .filter(|_| {
                self.connection_monitor
                    .host_tmux_snapshot_connection_id
                    .as_deref()
                    == Some(selected_id)
            });
        let rows = snapshot
            .map(|snapshot| {
                visible_tmux_session_rows(snapshot, &self.connection_monitor.host_tmux_search_query)
            })
            .unwrap_or_default();
        let status = snapshot
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_tmux_list_state(&rows, selected_id);

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
                        !self.connection_monitor.host_tmux_snapshot_polling,
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
                snapshot,
                self.connection_monitor.host_tmux_snapshot_polling,
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_tmux_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostTmuxSearch;
        let focused = self.connection_monitor.host_tmux_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_tmux_search_query,
                    placeholder: self.i18n.t("sidebar.host_tmux.search_placeholder"),
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
                    this.connection_monitor.host_tmux_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
                    this.connection_monitor.host_log_search_focused = false;
                    this.connection_monitor.host_port_search_focused = false;
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
                            disabled: self.connection_monitor.host_tmux_snapshot_polling,
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
        let state = self.connection_monitor.host_tmux_list_state.clone();
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
            .connection_monitor
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
                        if this
                            .connection_monitor
                            .host_tmux_expanded_session_id
                            .as_deref()
                            == Some(id.as_str())
                        {
                            this.connection_monitor.host_tmux_expanded_session_id = None;
                            this.connection_monitor.host_tmux_expanded_window_id = None;
                        } else {
                            this.connection_monitor.host_tmux_expanded_session_id =
                                Some(id.clone());
                            this.connection_monitor.host_tmux_expanded_window_id = None;
                        }
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
            .connection_monitor
            .host_tmux_action_running
            .as_ref()
            .is_some_and(|request| request.session_id == session.id);
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
            .connection_monitor
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
                                            .connection_monitor
                                            .host_tmux_action_running
                                            .as_ref()
                                            .is_some_and(|request| {
                                                request.session_id == session.id
                                            }),
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
                                            .connection_monitor
                                            .host_tmux_action_running
                                            .as_ref()
                                            .is_some_and(|request| {
                                                request.session_id == session.id
                                            }),
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
                                if this
                                    .connection_monitor
                                    .host_tmux_expanded_window_id
                                    .as_deref()
                                    == Some(id.as_str())
                                {
                                    this.connection_monitor.host_tmux_expanded_window_id = None;
                                } else {
                                    this.connection_monitor.host_tmux_expanded_window_id =
                                        Some(id.clone());
                                }
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
                                    .connection_monitor
                                    .host_tmux_action_running
                                    .as_ref()
                                    .is_some_and(|request| request.session_id == session.id),
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
                                    .connection_monitor
                                    .host_tmux_action_running
                                    .as_ref()
                                    .is_some_and(|request| request.session_id == session.id),
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
        selected_id: &str,
    ) {
        let signatures = rows
            .iter()
            .map(tmux_session_row_signature)
            .collect::<Vec<_>>();
        let identity = format!(
            "host-tmux:{selected_id}:{}:{}:{}",
            self.connection_monitor.host_tmux_search_query,
            self.connection_monitor
                .host_tmux_expanded_session_id
                .as_deref()
                .unwrap_or_default(),
            self.connection_monitor
                .host_tmux_expanded_window_id
                .as_deref()
                .unwrap_or_default()
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_tmux_list_state,
            &mut self.connection_monitor.host_tmux_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn host_tmux_snapshot_command(
        &self,
        connection_id: &str,
    ) -> (oxideterm_connection_monitor::TmuxCaptureCommand, String) {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        (build_tmux_snapshot_command(&os_type), os_type)
    }

    pub(super) fn host_tmux_action_command(
        &self,
        connection_id: &str,
        action: TmuxActionKind,
    ) -> Result<(oxideterm_connection_monitor::TmuxActionCommand, String), String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_action_command(&os_type, action).map(|command| (command, os_type))
    }

    pub(super) fn host_tmux_attach_command(
        &self,
        connection_id: &str,
        target: &str,
    ) -> Result<(String, String), String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_attach_command(&os_type, target).map(|command| (command, os_type))
    }

    pub(super) fn host_tmux_new_session_command(
        &self,
        connection_id: &str,
    ) -> Result<(String, String), String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_new_session_command(&os_type, None).map(|command| (command, os_type))
    }

    pub(in crate::workspace) fn handle_host_tmux_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_tmux_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_tmux_search_focused = false;
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
        self.request_host_tmux_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_host_tmux_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_tmux_snapshot_polling {
            if feedback.should_toast() {
                self.push_host_tmux_toast(
                    self.i18n
                        .t("sidebar.host_tmux.toast.snapshot_already_running"),
                    TerminalNoticeVariant::Warning,
                );
            }
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            if feedback.should_toast() {
                self.push_host_tmux_toast(
                    self.i18n.t("sidebar.host_tmux.toast.connection_missing"),
                    TerminalNoticeVariant::Error,
                );
            }
            cx.notify();
            return;
        };
        let (command, _os_type) = self.host_tmux_snapshot_command(&connection_id);
        let request = HostTmuxSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_tmux_snapshot_connection_id = Some(connection_id);
        self.connection_monitor.host_tmux_snapshot_running = Some(request.clone());
        self.connection_monitor.host_tmux_snapshot_rx = Some(rx);
        self.connection_monitor.host_tmux_snapshot_polling = true;
        self.connection_monitor.host_tmux_last_error = None;
        // tmux is a session manager, not a metric source. Keep it snapshot-driven.
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_TMUX_SNAPSHOT_TIMEOUT,
                    HOST_TMUX_SNAPSHOT_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostTmuxSnapshotDelivery { request, result });
        });
        cx.notify();
    }

    pub(super) fn request_host_tmux_kill_session(
        &mut self,
        connection_id: String,
        session_id: String,
        session_name: String,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_tmux_action_running.is_some() {
            self.push_host_tmux_toast(
                self.i18n
                    .t("sidebar.host_tmux.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        HostToolConfirmState::open(
            &mut self.connection_monitor.host_tmux_pending_confirm,
            HostTmuxActionRequest {
                connection_id,
                session_id: session_id.clone(),
                session_name: session_name.clone(),
                target_label: session_name,
                action: TmuxActionKind::KillSession { target: session_id },
            },
        );
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
        if self.connection_monitor.host_tmux_action_running.is_some() {
            self.push_host_tmux_toast(
                self.i18n
                    .t("sidebar.host_tmux.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        HostToolConfirmState::open(
            &mut self.connection_monitor.host_tmux_pending_confirm,
            HostTmuxActionRequest {
                connection_id,
                session_id,
                session_name,
                target_label: window_label,
                action: TmuxActionKind::KillWindow { target: window_id },
            },
        );
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
        if self.connection_monitor.host_tmux_action_running.is_some() {
            self.push_host_tmux_toast(
                self.i18n
                    .t("sidebar.host_tmux.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        HostToolConfirmState::open(
            &mut self.connection_monitor.host_tmux_pending_confirm,
            HostTmuxActionRequest {
                connection_id,
                session_id,
                session_name,
                target_label: pane_label,
                action: TmuxActionKind::KillPane { target: pane_id },
            },
        );
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
                value: session_name,
                focused: true,
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
                value: window_name,
                focused: true,
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
                value: String::new(),
                focused: true,
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
        self.connection_monitor.host_tmux_search_focused = false;
        self.connection_monitor.host_tmux_input_dialog = Some(dialog);
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
        let (command, _os_type) = match self.host_tmux_attach_command(&connection_id, &session_id) {
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
        let (command, _os_type) = match self.host_tmux_new_session_command(&connection_id) {
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
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_tmux_toast(
                self.i18n.t("sidebar.host_tmux.toast.exec_terminal_missing"),
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
        if self.connection_monitor.host_tmux_pending_confirm.is_none() {
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
        if self.connection_monitor.host_tmux_input_dialog.is_none() {
            return false;
        }
        if event.keystroke.modifiers.platform {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.connection_monitor.host_tmux_input_dialog = None;
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
        let Some(request) = self
            .connection_monitor
            .host_tmux_pending_confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return;
        };
        if self.begin_host_tmux_confirm_exit(cx) {
            self.start_host_tmux_action(request, cx);
        }
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_tmux_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self
            .connection_monitor
            .host_tmux_pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if delay.is_zero() {
            self.connection_monitor.host_tmux_pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .connection_monitor
                    .host_tmux_pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    this.connection_monitor.host_tmux_pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(super) fn submit_host_tmux_input_dialog(&mut self, cx: &mut Context<Self>) {
        if self.connection_monitor.host_tmux_action_running.is_some() {
            self.push_host_tmux_toast(
                self.i18n
                    .t("sidebar.host_tmux.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }
        let Some(dialog) = self.connection_monitor.host_tmux_input_dialog.as_ref() else {
            return;
        };
        let value = dialog.value.trim().to_string();
        if value.is_empty() {
            self.push_host_tmux_toast(
                self.i18n.t("sidebar.host_tmux.toast.input_required"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }
        let dialog = self
            .connection_monitor
            .host_tmux_input_dialog
            .take()
            .expect("tmux input dialog is present after validation");
        let action = match dialog.kind {
            HostTmuxInputDialogKind::RenameSession { target } => TmuxActionKind::RenameSession {
                target,
                name: value,
            },
            HostTmuxInputDialogKind::RenameWindow { target } => TmuxActionKind::RenameWindow {
                target,
                name: value,
            },
            HostTmuxInputDialogKind::SendPaneCommand { target } => {
                TmuxActionKind::SendPaneCommand {
                    target,
                    command: value,
                }
            }
        };
        self.ime_marked_text = None;
        self.clear_ime_selection();
        self.start_host_tmux_action(
            HostTmuxActionRequest {
                connection_id: dialog.connection_id,
                session_id: dialog.session_id,
                session_name: dialog.session_name,
                target_label: dialog.target_label,
                action,
            },
            cx,
        );
    }

    pub(super) fn start_host_tmux_action(
        &mut self,
        request: HostTmuxActionRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            self.push_host_tmux_toast(
                self.i18n.t("sidebar.host_tmux.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let (command, _os_type) =
            match self.host_tmux_action_command(&request.connection_id, request.action.clone()) {
                Ok(command) => command,
                Err(error) => {
                    self.push_host_tmux_toast(error, TerminalNoticeVariant::Error);
                    cx.notify();
                    return;
                }
            };
        let (tx, rx) = std::sync::mpsc::channel();
        let delivery_request = request.clone();
        self.connection_monitor.host_tmux_action_running = Some(request);
        self.connection_monitor.host_tmux_action_rx = Some(rx);
        self.connection_monitor.host_tmux_action_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_TMUX_ACTION_TIMEOUT,
                    HOST_TMUX_ACTION_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostTmuxActionDelivery {
                request: delivery_request,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::workspace) fn poll_host_tmux_snapshot_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_tmux_snapshot_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_tmux_snapshot_rx.take() else {
            self.connection_monitor.host_tmux_snapshot_polling = false;
            self.connection_monitor.host_tmux_snapshot_running = None;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.finish_host_tmux_snapshot(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_tmux_snapshot_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let feedback = self
                    .connection_monitor
                    .host_tmux_snapshot_running
                    .as_ref()
                    .map(|request| request.feedback)
                    .unwrap_or(HostSnapshotFeedback::Silent);
                self.connection_monitor.host_tmux_snapshot_polling = false;
                self.connection_monitor.host_tmux_snapshot_running = None;
                let reason = self.i18n.t("sidebar.host_tmux.toast.unknown_error");
                self.connection_monitor.host_tmux_last_error = Some(reason.clone());
                if feedback.should_toast() {
                    self.push_host_tmux_toast(
                        self.i18n_replace(
                            "sidebar.host_tmux.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn poll_host_tmux_action_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_tmux_action_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_tmux_action_rx.take() else {
            self.connection_monitor.host_tmux_action_polling = false;
            self.connection_monitor.host_tmux_action_running = None;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_tmux_action_polling = false;
                self.connection_monitor.host_tmux_action_running = None;
                self.finish_host_tmux_action(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_tmux_action_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_tmux_action_polling = false;
                self.connection_monitor.host_tmux_action_running = None;
                self.push_host_tmux_toast(
                    self.i18n.t("sidebar.host_tmux.toast.action_failed"),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
            }
        }
    }

    pub(super) fn finish_host_tmux_snapshot(
        &mut self,
        delivery: HostTmuxSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_tmux_snapshot_running
            .as_ref()
            .is_some_and(|running| running != &delivery.request)
        {
            cx.notify();
            return;
        }
        let feedback = delivery.request.feedback;
        self.connection_monitor.host_tmux_snapshot_polling = false;
        self.connection_monitor.host_tmux_snapshot_running = None;
        self.connection_monitor.host_tmux_snapshot_rx = None;
        match delivery.result {
            Ok(output) => {
                let snapshot =
                    tmux_capture_snapshot(&output.stdout, &output.stderr, output.exit_code);
                match &snapshot.status {
                    ResourceTmuxStatus::Available { .. } => {
                        let count = visible_tmux_session_rows(
                            &snapshot,
                            &self.connection_monitor.host_tmux_search_query,
                        )
                        .len();
                        self.connection_monitor.host_tmux_last_error = None;
                        if feedback.should_toast() {
                            self.push_host_tmux_toast(
                                self.i18n_replace(
                                    "sidebar.host_tmux.toast.snapshot_loaded",
                                    &[("count", count.to_string())],
                                ),
                                TerminalNoticeVariant::Success,
                            );
                        }
                    }
                    ResourceTmuxStatus::Unavailable => {
                        self.connection_monitor.host_tmux_last_error =
                            Some(self.i18n.t("sidebar.host_tmux.unavailable"));
                        if feedback.should_toast() {
                            self.push_host_tmux_toast(
                                self.i18n.t("sidebar.host_tmux.toast.unavailable"),
                                TerminalNoticeVariant::Warning,
                            );
                        }
                    }
                    ResourceTmuxStatus::Error { message } => {
                        self.connection_monitor.host_tmux_last_error = Some(message.clone());
                        if feedback.should_toast() {
                            self.push_host_tmux_toast(
                                self.i18n_replace(
                                    "sidebar.host_tmux.toast.snapshot_failed",
                                    &[("reason", message.clone())],
                                ),
                                TerminalNoticeVariant::Error,
                            );
                        }
                    }
                    ResourceTmuxStatus::Unknown => {}
                }
                self.connection_monitor.host_tmux_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_tmux_snapshot = Some(snapshot);
            }
            Err(error) => {
                self.connection_monitor.host_tmux_last_error = Some(error.clone());
                self.connection_monitor.host_tmux_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_tmux_snapshot = Some(ResourceTmuxSnapshot {
                    status: ResourceTmuxStatus::Error {
                        message: error.clone(),
                    },
                    sessions: Vec::new(),
                    windows: Vec::new(),
                    panes: Vec::new(),
                });
                if feedback.should_toast() {
                    self.push_host_tmux_toast(
                        self.i18n_replace(
                            "sidebar.host_tmux.toast.snapshot_failed",
                            &[("reason", error)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
            }
        }
        cx.notify();
    }

    pub(super) fn finish_host_tmux_action(
        &mut self,
        delivery: HostTmuxActionDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery.result {
            Ok(output) => {
                match interpret_tmux_action_output(&output.stdout, &output.stderr, output.exit_code)
                {
                    HostToolActionOutcome::Succeeded { message } => {
                        self.push_host_tmux_toast(message, TerminalNoticeVariant::Success);
                    }
                    HostToolActionOutcome::Failed { message } => {
                        self.push_host_tmux_toast(message, TerminalNoticeVariant::Error);
                    }
                }
            }
            Err(error) => {
                self.push_host_tmux_toast(error, TerminalNoticeVariant::Error);
            }
        }
        self.request_host_tmux_snapshot(
            delivery.request.connection_id,
            HostSnapshotFeedback::Silent,
            cx,
        );
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
        let confirm = self.connection_monitor.host_tmux_pending_confirm.as_ref()?;
        let request = &confirm.request;
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
                confirm.presence.phase(),
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
        let dialog = self.connection_monitor.host_tmux_input_dialog.as_ref()?;
        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::HostTmuxDialogInput;
        let title = self.i18n.t(host_tmux_input_title_key(&dialog.kind));
        let description = self.i18n_replace(
            host_tmux_input_description_key(&dialog.kind),
            &[
                ("name", dialog.session_name.clone()),
                ("target", dialog.target_label.clone()),
            ],
        );
        let submit_label = self.i18n.t(host_tmux_input_submit_key(&dialog.kind));
        let submit_disabled = dialog.value.trim().is_empty()
            || self.connection_monitor.host_tmux_action_running.is_some();
        let workspace = cx.entity();

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.connection_monitor.host_tmux_input_dialog = None;
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
                                text_input(
                                    &self.tokens,
                                    TextInputView {
                                        value: &dialog.value,
                                        placeholder: self
                                            .i18n
                                            .t(host_tmux_input_placeholder_key(&dialog.kind)),
                                        focused: dialog.focused,
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
                                        if let Some(dialog) =
                                            this.connection_monitor.host_tmux_input_dialog.as_mut()
                                        {
                                            dialog.focused = true;
                                        }
                                        this.ime_marked_text = None;
                                        this.new_connection_caret_visible = true;
                                        window.focus(&this.focus_handle, cx);
                                        this.begin_ime_selection_from_mouse_down(
                                            target, event, window, cx,
                                        );
                                        cx.stop_propagation();
                                    }),
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
                                        this.connection_monitor.host_tmux_input_dialog = None;
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

fn host_tmux_confirm_description_key(action: &TmuxActionKind) -> &'static str {
    match action {
        TmuxActionKind::KillSession { .. } => "sidebar.host_tmux.confirm.kill_session_desc",
        TmuxActionKind::KillWindow { .. } => "sidebar.host_tmux.confirm.kill_window_desc",
        TmuxActionKind::KillPane { .. } => "sidebar.host_tmux.confirm.kill_pane_desc",
        TmuxActionKind::RenameSession { .. }
        | TmuxActionKind::RenameWindow { .. }
        | TmuxActionKind::SendPaneCommand { .. } => "sidebar.host_tmux.confirm.action_desc",
    }
}

fn host_tmux_confirm_label_key(action: &TmuxActionKind) -> &'static str {
    match action {
        TmuxActionKind::KillSession { .. } => "sidebar.host_tmux.actions.kill_session",
        TmuxActionKind::KillWindow { .. } => "sidebar.host_tmux.actions.kill_window",
        TmuxActionKind::KillPane { .. } => "sidebar.host_tmux.actions.kill_pane",
        TmuxActionKind::RenameSession { .. } => "sidebar.host_tmux.actions.rename_session",
        TmuxActionKind::RenameWindow { .. } => "sidebar.host_tmux.actions.rename_window",
        TmuxActionKind::SendPaneCommand { .. } => "sidebar.host_tmux.actions.send_command",
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
