const CONTEXT_SIDEBAR_RESIZE_HOTZONE_WIDTH: f32 = 12.0;
const CONTEXT_SIDEBAR_RESIZE_DIVIDER_WIDTH: f32 = 1.0;

fn context_sidebar_frame_chrome(total_width: f32) -> gpui::Stateful<gpui::Div> {
    div()
        .id("context-right-sidebar-frame")
        .relative()
        .flex_none()
        .w(px(total_width))
        .h_full()
        .min_w_0()
        .flex()
        .flex_row()
}

fn context_sidebar_region_chrome() -> gpui::Div {
    div()
        .relative()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .min_h_0()
}

fn context_sidebar_resize_hotzone_chrome() -> gpui::Stateful<gpui::Div> {
    div()
        .id("context-right-sidebar-resize-hotzone")
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(CONTEXT_SIDEBAR_RESIZE_HOTZONE_WIDTH))
        .cursor(CursorStyle::ResizeColumn)
        // This is the frame-owned hit target rendered after the content region.
        // Host Tools bodies can contain scroll/list hitboxes; keeping the resize
        // hotzone above them prevents loaded content from stealing edge drags.
        .occlude()
}

impl WorkspaceApp {
    pub(super) fn render_sidebar_region(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .relative()
            .w(px(self.sidebar_panel_width()))
            .h_full()
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(self.tokens.metrics.sidebar_resize_handle_width))
                    .cursor(CursorStyle::ResizeColumn)
                    // The handle is intentionally transparent while idle, so
                    // give GPUI a concrete top-level hitbox instead of relying
                    // on neighboring title/content regions to leave the edge.
                    .occlude()
                    .bg(if self.sidebar_resizing {
                        rgb(theme.accent)
                    } else {
                        rgba(theme.bg << 8)
                    })
                    .hover(|handle| handle.bg(rgba((theme.accent << 8) | 0x80)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event, window, cx| {
                            this.start_sidebar_resize(event, window, cx);
                            window.prevent_default();
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_context_right_sidebar_frame(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        context_sidebar_frame_chrome(self.ai_sidebar_width)
            // The frame owns the full right-sidebar width. The content starts
            // at the real seam with the terminal, while the resize affordance
            // is an overlay so it does not create a visible layout gap.
            .child(self.render_context_right_sidebar_region(cx))
            .child(self.render_context_right_sidebar_resize_hotzone(cx))
            .into_any_element()
    }

    pub(super) fn render_context_right_sidebar_region(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (title_key, title_role, icon) = match self.active_context_sidebar_panel {
            ContextSidebarPanel::Assistant => {
                ("sidebar.panels.ai", "assistant", LucideIcon::Sparkles)
            }
            ContextSidebarPanel::HostTools => {
                ("sidebar.panels.host_tools", "host-tools", LucideIcon::Wrench)
            }
        };
        context_sidebar_region_chrome()
            .child(
                div()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .bg(rgb(theme.bg))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex_none()
                            .h(px(42.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .px_3()
                            // The context-sidebar titlebar is fixed chrome.
                            // Give it its own hitbox so wheel/drag events
                            // cannot fall through to a scrollable tool body.
                            .occlude()
                            .border_b_1()
                            .border_color(rgba((theme.border << 8) | 0x4d))
                            // Keep the title and collapse button in one real
                            // horizontal flex row. The region width is owned by
                            // the parent frame, so this row must never infer a
                            // smaller hand-derived width from the title text.
                            .child(self.render_context_sidebar_panel_title(
                                title_key, title_role, icon, cx,
                            ))
                            .child(
                                div()
                                    .id("context-sidebar-collapse")
                                    .flex_none()
                                    .size(px(28.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .cursor_pointer()
                                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::PanelRightClose,
                                        16.0,
                                        rgb(theme.text_muted),
                                    ))
                                    .on_mouse_move(cx.listener({
                                        let label = self.i18n.t("sidebar.tooltips.collapse");
                                        move |this, event: &MouseMoveEvent, _window, cx| {
                                            this.queue_workspace_tooltip(
                                                "context-sidebar-collapse",
                                                label.clone(),
                                                f32::from(event.position.x) + 12.0,
                                                f32::from(event.position.y) + 16.0,
                                                cx,
                                            );
                                        }
                                    }))
                                    .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                                        if !*hovered {
                                            this.clear_workspace_tooltip(
                                                "context-sidebar-collapse",
                                                cx,
                                            );
                                        }
                                    }))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.collapse_context_sidebar(cx);
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(match self.active_context_sidebar_panel {
                                ContextSidebarPanel::Assistant => self.render_ai_sidebar_content(cx),
                                ContextSidebarPanel::HostTools => {
                                    self.render_host_tools_context_panel(cx)
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_context_right_sidebar_resize_hotzone(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        context_sidebar_resize_hotzone_chrome()
            .child(
                div()
                    .absolute()
                    // The visible divider is drawn at the actual frame seam.
                    // The wider invisible hotzone remains above the content
                    // for reliable edge dragging after Host Tools lists load.
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(CONTEXT_SIDEBAR_RESIZE_DIVIDER_WIDTH))
                    .bg(if self.ai_sidebar_resizing {
                        rgb(theme.accent)
                    } else {
                        rgba((theme.border << 8) | 0x80)
                    }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.start_ai_sidebar_resize(event, window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_context_sidebar_panel_title(
        &self,
        title_key: &'static str,
        title_role: &'static str,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.render_window_drag_content_region(
            "context-sidebar-titlebar-title",
            div()
                .w_full()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(Self::render_lucide_icon(icon, 16.0, rgb(theme.accent)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "context-sidebar-title",
                            title_role,
                            self.i18n.t(title_key),
                            theme.text,
                            cx,
                        )),
                )
                .into_any_element(),
            cx,
        )
    }

    pub(super) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border))
            .child(self.render_sidebar_header(cx))
            .child(self.render_sidebar_content(cx))
            .into_any_element()
    }

    pub(super) fn render_sidebar_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let panel_section = self.effective_sidebar_panel_section();
        let plugin_panel_title =
            (panel_section == SidebarSection::Extensions)
                .then(|| {
                    self.active_native_plugin_sidebar_panel
                        .as_ref()
                        .and_then(|selection| {
                            self.plugin_registry
                                .contributions()
                                .runtime_sidebar_panels()
                                .into_iter()
                                .find(|panel| {
                                    panel.plugin_id == selection.plugin_id
                                        && panel.panel_id == selection.panel_id
                                })
                                .map(|panel| panel.title)
                        })
                })
                .flatten();
        let title_key = match panel_section {
            SidebarSection::Connections => "sidebar.panels.saved_connections",
            SidebarSection::Sftp => "sidebar.panels.sftp",
            SidebarSection::Forwards => "forwards.table.title",
            SidebarSection::Extensions => "sidebar.panels.plugins",
            SidebarSection::CloudSync => "plugin.cloud_sync.panel_title",
            SidebarSection::Notifications => "sidebar.panels.event_log",
            _ => "sidebar.panels.sessions",
        };
        let title = plugin_panel_title
            .unwrap_or_else(|| self.i18n.t(title_key).to_uppercase());
        let mut header = div()
            .h(px(self.tokens.metrics.sidebar_header_height))
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .child(
                self.render_window_drag_content_region(
                    "sidebar-header-title-drag-region",
                    div()
                        .flex()
                        .items_center()
                        .truncate()
                        .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_muted))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "sidebar-header-title",
                            title_key,
                            title,
                            theme.text_muted,
                            cx,
                        ))
                    .into_any_element(),
                    cx,
                ),
            );
        if panel_section == SidebarSection::Sessions {
            let (view_icon, view_action) = match self.active_session_sidebar_view_mode {
                ActiveSessionSidebarViewMode::Tree => {
                    (LucideIcon::Folder, SidebarActionKind::ToggleSessionView)
                }
                ActiveSessionSidebarViewMode::Focus => {
                    (LucideIcon::ListChecks, SidebarActionKind::ToggleSessionView)
                }
            };
            header = header
                .child(self.render_sidebar_action(view_icon, view_action, cx))
                .child(self.render_sidebar_action(LucideIcon::Network, SidebarActionKind::AutoRoute, cx))
                .child(self.render_sidebar_action(LucideIcon::Plus, SidebarActionKind::NewConnection, cx));
        }
        header.into_any_element()
    }

    pub(super) fn render_sidebar_action(
        &self,
        icon: LucideIcon,
        action: SidebarActionKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = match action {
            SidebarActionKind::ToggleSessionView => match self.active_session_sidebar_view_mode {
                ActiveSessionSidebarViewMode::Tree => self.i18n.t("sidebar.tooltips.switch_focus"),
                ActiveSessionSidebarViewMode::Focus => self.i18n.t("sidebar.tooltips.switch_tree"),
            },
            SidebarActionKind::NewConnection => self.i18n.t("sidebar.tooltips.new_connection"),
            SidebarActionKind::AutoRoute => self.i18n.t("sidebar.tooltips.auto_route"),
        };

        let toggle_focus_active = action == SidebarActionKind::ToggleSessionView
            && self.active_session_sidebar_view_mode == ActiveSessionSidebarViewMode::Focus;

        // Tauri sidebar header actions are icon buttons with title tooltips.
        // The view-mode action is the old Folder/ListChecks toggle from
        // Sidebar.tsx; keep its active "secondary" chrome only in focus mode.
        div()
            .ml_1()
            .child(self.workspace_tooltip_icon_button(
                icon,
                self.tokens.metrics.sidebar_action_icon_size,
                rgb(theme.text),
                IconButtonOptions {
                    has_background: toggle_focus_active,
                    background: toggle_focus_active.then_some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_hover)),
                    ..IconButtonOptions::opaque_toolbar(
                        self.tokens.metrics.sidebar_action_size,
                        ButtonRadius::Md,
                    )
                },
                label,
                "sidebar-action",
                false,
                cx.listener(move |this, _event, window, cx| {
                    match action {
                        SidebarActionKind::ToggleSessionView => {
                            this.toggle_active_session_sidebar_view(cx)
                        }
                        SidebarActionKind::NewConnection => this.open_new_connection_form(window, cx),
                        SidebarActionKind::AutoRoute => this.open_auto_route_modal(window, cx),
                    }
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(super) fn render_sidebar_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let panel_section = self.effective_sidebar_panel_section();
        if panel_section == SidebarSection::Connections {
            return self.render_saved_connections_sidebar_content(cx);
        }
        if panel_section == SidebarSection::Sessions {
            return self.render_active_sessions_sidebar_content(cx);
        }
        if panel_section == SidebarSection::Extensions {
            return self.render_native_plugin_sidebar_content(cx);
        }
        if panel_section == SidebarSection::CloudSync {
            return self.render_cloud_sync_sidebar_content(cx);
        }
        if matches!(
            panel_section,
            SidebarSection::Sftp | SidebarSection::Forwards
        ) {
            // Tauri only persists these command-palette section keys here; it
            // does not reuse the Sessions empty state for their sidebar body.
            return self.render_blank_sidebar_content();
        }
        self.render_empty_sessions_sidebar_content(cx)
    }

    fn render_blank_sidebar_content(&self) -> AnyElement {
        div().flex_1().w_full().into_any_element()
    }

    pub(super) fn render_activity_tab_button(
        &self,
        view: WorkspaceActivityView,
        icon: LucideIcon,
        label: String,
        badge_count: u32,
        badge_error: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.notification_center.active_view == view;
        let tab = div()
            .h(px(28.0))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .bg(if active { rgb(theme.bg_active) } else { rgb(theme.bg) })
            .border_1()
            .border_color(if active { rgb(theme.border) } else { rgb(theme.bg) })
            .text_size(px(11.0))
            .text_color(if active {
                rgb(theme.text_heading)
            } else {
                rgb(theme.text_muted)
            })
            .child(Self::render_lucide_icon(
                icon,
                13.0,
                if active {
                    rgb(theme.text_heading)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(div().truncate().child(
                // Activity tabs are buttons; the label should bubble clicks exactly like Tauri select-none text.
                self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "activity-tab-label",
                    if view == WorkspaceActivityView::Notifications {
                        "notifications"
                    } else {
                        "event-log"
                    },
                    label,
                    if active {
                        theme.text_heading
                    } else {
                        theme.text_muted
                    },
                    cx,
                ),
            ))
            .when(badge_count > 0, |button| {
                button.child(
                    div()
                        .min_w(px(14.0))
                        .h(px(14.0))
                        .px(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgb(if badge_error { 0xef4444 } else { theme.accent }))
                        .text_color(rgb(0xffffff))
                        .text_size(px(9.0))
                        .child(if badge_count > 99 {
                            "99+".to_string()
                        } else {
                            badge_count.to_string()
                        }),
                )
            });

        // Tauri activity tabs are Button-like rows rather than plain labels.
        // Keep the custom tab chrome, but route pointer activation through the
        // shared clickable-row guard used by browser-style list controls.
        self.workspace_clickable_row_action(
            tab,
            false,
            cx.listener(move |this, _event, _window, cx| {
                this.notification_center.active_view = view;
                if view == WorkspaceActivityView::Notifications {
                    this.notification_center.notifications.unread_count = 0;
                    this.notification_center.notifications.unread_critical_count = 0;
                } else {
                    this.notification_center.event_log.unread_count = 0;
                    this.notification_center.event_log.unread_errors = 0;
                }
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(super) fn render_notifications_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let filtered = self.notification_center.notifications.entries
            .iter()
            .rev()
            .filter(|entry| self.notification_matches_filter(entry))
            .cloned()
            .collect::<Vec<_>>();
        let row_count = filtered.len();
        let signatures = notification_sidebar_row_signatures(&filtered);
        let notification_spec = TauriVirtualListSpec::new(
            px(NOTIFICATION_SIDEBAR_ROW_HEIGHT_ESTIMATE),
            NOTIFICATION_SIDEBAR_VIRTUAL_OVERSCAN,
        );
        {
            let mut cache = self.notification_sidebar_list_cache.borrow_mut();
            super::virtual_list::sync_tauri_variable_list_state_by_signatures(
                &self.notification_sidebar_list_state,
                &mut cache,
                "notifications-sidebar",
                &signatures,
                notification_spec,
            );
        }
        let notification_rows = Arc::new(filtered);
        let notification_list_state = self.notification_sidebar_list_state.clone();
        let workspace = cx.entity();

        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .child(self.render_notifications_toolbar(cx))
            .when(self.notification_center.notifications.dnd_enabled, |content| {
                content.child(
                    div()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .bg(rgba(0xf59e0b1a))
                        .px_3()
                        .py_2()
                        .text_size(px(11.0))
                        .text_color(rgb(0xfbbf24))
                        .child(self.render_selectable_display_text(
                            "notifications-dnd",
                            (),
                            "Do Not Disturb is on".to_string(),
                            0xfbbf24,
                            cx,
                        )),
                )
            })
            .child(
                div()
                    .id("notifications-sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .when(row_count == 0, |content| {
                        content.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(12.0))
                                .text_color(rgb(theme.text_muted))
                                .child(self.render_selectable_display_text(
                                    "notifications-empty",
                                    (),
                                    "No notifications".to_string(),
                                    theme.text_muted,
                                    cx,
                                )),
                        )
                    })
                    .when(row_count > 0, |content| {
                        content.child(tauri_virtual_list(
                            notification_list_state,
                            notification_spec,
                            move |index, _window, app| {
                                let rows = notification_rows.clone();
                                let Some(entry) = rows.get(index).cloned() else {
                                    return div().into_any_element();
                                };
                                workspace.update(app, |this, cx| {
                                    this.render_notification_row(&entry, cx)
                                })
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    fn render_notifications_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .px_2()
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(self.render_activity_icon_button(
                LucideIcon::Bell,
                self.notification_center.notifications.dnd_enabled,
                "Toggle notification DND",
                |this, _event, _window, cx| {
                    this.notification_center.notifications.dnd_enabled = !this.notification_center.notifications.dnd_enabled;
                    cx.notify();
                },
                cx,
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::ListTree,
                self.notification_center.notifications.filter.status != WorkspaceNotificationStatusFilter::All,
                "Cycle status filter",
                |this, _event, _window, cx| {
                    this.cycle_notification_status_filter();
                    cx.notify();
                },
                cx,
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::AlertCircle,
                self.notification_center.notifications.filter.severity != WorkspaceNotificationSeverityFilter::All,
                "Cycle severity filter",
                |this, _event, _window, cx| {
                    this.cycle_notification_severity_filter();
                    cx.notify();
                },
                cx,
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::Hash,
                self.notification_center.notifications.filter.kind != WorkspaceNotificationKindFilter::All,
                "Cycle kind filter",
                |this, _event, _window, cx| {
                    this.cycle_notification_kind_filter();
                    cx.notify();
                },
                cx,
            ))
            .child(div().flex_1())
            .child(self.render_activity_icon_button(
                LucideIcon::Check,
                false,
                "Mark all read",
                |this, _event, _window, cx| {
                    this.mark_all_notifications_read();
                    cx.notify();
                },
                cx,
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::Trash2,
                false,
                "Clear notifications",
                |this, _event, _window, cx| {
                    this.clear_notifications();
                    cx.notify();
                },
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_event_log_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let filtered = self.notification_center.event_log.entries
            .iter()
            .filter(|entry| self.event_log_entry_matches_filter(entry))
            .cloned()
            .collect::<Vec<_>>();
        let row_count = filtered.len();
        let event_log_scroll = self.event_log_sidebar_scroll_handle.clone();
        let event_log_spec = TauriVirtualListSpec::new(
            px(EVENT_LOG_SIDEBAR_ROW_HEIGHT),
            EVENT_LOG_SIDEBAR_VIRTUAL_OVERSCAN,
        );
        if row_count > 0 {
            self.schedule_event_log_virtual_scroll_to_bottom_if_sticky(
                event_log_scroll.clone(),
                row_count - 1,
                event_log_spec,
                cx,
            );
        }
        let event_log_rows = Arc::new(filtered);
        let workspace = cx.entity();

        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .child(self.render_event_log_toolbar(cx))
            .when(self.notification_center.event_log.dnd_enabled, |content| {
                content.child(
                    div()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .bg(rgba(0xf59e0b1a))
                        .px_3()
                        .py_2()
                        .text_size(px(11.0))
                        .text_color(rgb(0xfbbf24))
                        .child(self.render_selectable_display_text(
                            "event-log-dnd",
                            (),
                            self.i18n.t("event_log.dnd.on"),
                            0xfbbf24,
                            cx,
                        )),
                )
            })
            .child(
                div()
                    .id("event-log-sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_hidden()
                    .when(row_count == 0, |content| {
                        content.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(12.0))
                                .text_color(rgb(theme.text_muted))
                                .child(self.render_selectable_display_text(
                                    "event-log-empty",
                                    (),
                                    self.i18n.t("event_log.empty"),
                                    theme.text_muted,
                                    cx,
                                )),
                        )
                    })
                    .when(row_count > 0, |content| {
                        content.child(tauri_virtual_uniform_list(
                            "event-log-sidebar-virtual",
                            row_count,
                            event_log_scroll,
                            event_log_spec,
                            move |range, _window, app| {
                                let mut rendered = Vec::new();
                                let rows = event_log_rows.clone();
                                let _ = workspace.update(app, |this, cx| {
                                    for index in range {
                                        let Some(entry) = rows.get(index) else {
                                            continue;
                                        };
                                        rendered.push(this.render_event_log_row(entry, cx));
                                    }
                                });
                                rendered
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    fn schedule_event_log_virtual_scroll_to_bottom_if_sticky(
        &self,
        handle: UniformListScrollHandle,
        last_index: usize,
        spec: TauriVirtualListSpec,
        cx: &mut Context<Self>,
    ) {
        if !tauri_virtual_list_is_near_bottom(
            &handle,
            px(EVENT_LOG_STICKY_BOTTOM_THRESHOLD_PX),
        ) {
            return;
        }
        // Tauri defers the bottom scroll until after React commits the new row.
        // GPUI likewise needs a post-layout turn before the uniform-list extent
        // is current, otherwise the newest event can remain just below view.
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = weak.update(cx, move |_this, cx| {
                scroll_tauri_virtual_list_to_index(
                    &handle,
                    last_index,
                    spec,
                    TauriVirtualScrollAlign::End,
                );
                cx.notify();
            });
        })
        .detach();
    }

    fn render_event_log_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let counts = self.filtered_event_log_counts();
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .px_2()
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(10.0))
                    .child(self.render_count_chip(LucideIcon::AlertCircle, 0xef4444, counts.2, cx))
                    .child(self.render_count_chip(
                        LucideIcon::AlertTriangle,
                        0xf59e0b,
                        counts.1,
                        cx,
                    ))
                    .child(self.render_count_chip(LucideIcon::Info, theme.accent, counts.0, cx)),
            )
            .child(div().flex_1())
            .child(self.render_activity_icon_button(
                LucideIcon::Bell,
                self.notification_center.event_log.dnd_enabled,
                "Toggle event log DND",
                |this, _event, _window, cx| {
                    this.notification_center.event_log.dnd_enabled = !this.notification_center.event_log.dnd_enabled;
                    cx.notify();
                },
                cx,
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::ListTree,
                self.notification_center.event_log.filter.severity != WorkspaceEventSeverityFilter::All,
                "Cycle severity filter",
                |this, _event, _window, cx| {
                    this.cycle_event_log_severity_filter();
                    cx.notify();
                },
                cx,
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::Search,
                self.notification_center.event_log.filter.category != WorkspaceEventCategoryFilter::All,
                "Cycle category filter",
                |this, _event, _window, cx| {
                    this.cycle_event_log_category_filter();
                    cx.notify();
                },
                cx,
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::Trash2,
                false,
                "Clear event log",
                |this, _event, _window, cx| {
                    this.clear_event_log();
                    cx.notify();
                },
                cx,
            ))
            .into_any_element()
    }

    fn render_activity_icon_button(
        &self,
        icon: LucideIcon,
        active: bool,
        _label: &'static str,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let background = if active {
            rgb(theme.bg_active)
        } else {
            rgb(theme.bg)
        };
        let icon_color = if active {
            rgb(theme.text_heading)
        } else {
            rgb(theme.text_muted)
        };
        self.workspace_icon_action_button(
            icon,
            12.0,
            icon_color,
            oxideterm_gpui_ui::button::IconButtonOptions {
                background: Some(background),
                hover_background: Some(rgb(theme.bg_hover)),
                // Tauri activity toolbar icons are fully opaque in both normal
                // and active states; muting is represented by icon color.
                ..oxideterm_gpui_ui::button::IconButtonOptions::opaque_toolbar(
                    22.0,
                    oxideterm_gpui_ui::button::ButtonRadius::Sm,
                )
            },
            listener,
            cx,
        )
        .into_any_element()
    }

    fn render_count_chip(
        &self,
        icon: LucideIcon,
        color: u32,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .text_color(rgb(color))
            .child(Self::render_lucide_icon(icon, 11.0, rgb(color)))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "event-log-count-chip",
                (icon as u8, color),
                count.to_string(),
                color,
                cx,
            ))
            .into_any_element()
    }

    fn filtered_event_log_counts(&self) -> (usize, usize, usize) {
        let mut info = 0;
        let mut warn = 0;
        let mut error = 0;
        for entry in self.notification_center.event_log.entries
            .iter()
            .filter(|entry| self.event_log_entry_matches_filter(entry))
        {
            match entry.severity {
                WorkspaceEventSeverity::Info => info += 1,
                WorkspaceEventSeverity::Warn => warn += 1,
                WorkspaceEventSeverity::Error => error += 1,
            }
        }
        (info, warn, error)
    }

    fn resolve_event_log_title(&self, entry: &WorkspaceEventLogEntry) -> String {
        resolve_event_log_text(&self.i18n, &entry.title).unwrap_or_else(|| entry.title.clone())
    }

    fn resolve_event_log_detail(&self, entry: &WorkspaceEventLogEntry) -> Option<String> {
        let detail = entry.detail.as_ref()?;
        if let Some(resolved) = resolve_event_log_text(&self.i18n, detail) {
            return Some(resolved);
        }
        if entry.source == "reconnect_orchestrator" {
            let phase_key = format!("event_log.phase.{detail}");
            let translated = self.i18n.t(&phase_key);
            if translated != phase_key {
                return Some(translated);
            }
        }
        Some(detail.clone())
    }

    fn render_notification_row(
        &self,
        entry: &WorkspaceNotificationEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let id = entry.id;
        let (icon, accent) = match entry.severity {
            WorkspaceNotificationSeverity::Info => (LucideIcon::Info, theme.accent),
            WorkspaceNotificationSeverity::Warning => (LucideIcon::AlertTriangle, 0xf59e0b),
            WorkspaceNotificationSeverity::Error => (LucideIcon::AlertCircle, 0xef4444),
            WorkspaceNotificationSeverity::Critical => (LucideIcon::Shield, 0xef4444),
        };
        let kind = notification_kind_label(entry.kind);
        let status_unread = entry.status == WorkspaceNotificationStatus::Unread;
        let timestamp = entry
            .created_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let scope = match &entry.scope {
            WorkspaceNotificationScope::Global => "global".to_string(),
            WorkspaceNotificationScope::Node(node_id) => node_id.clone(),
            WorkspaceNotificationScope::Connection(connection_id) => connection_id.clone(),
        };

        div()
            .w_full()
            .mb_2()
            .p_2()
            .rounded(px(self.tokens.radii.md))
            .bg(if status_unread {
                rgb(theme.bg_hover)
            } else {
                rgb(theme.bg)
            })
            .border_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(div().mt(px(1.0)).child(Self::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(accent),
                    )))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .min_w(px(0.0))
                                            .flex_1()
                                            .truncate()
                                            .text_size(px(12.0))
                                            .font_weight(if status_unread {
                                                gpui::FontWeight::SEMIBOLD
                                            } else {
                                                gpui::FontWeight::NORMAL
                                            })
                                            .text_color(rgb(theme.text_heading))
                                            .child(self.render_selectable_text_scoped(
                                                "notification-title",
                                                entry.id,
                                                entry.title.clone(),
                                                theme.text_heading,
                                                cx,
                                            )),
                                    )
                                    .when(status_unread && !self.notification_center.notifications.dnd_enabled, |row| {
                                        row.child(
                                            div()
                                                .size(px(6.0))
                                                .rounded_full()
                                                .bg(rgb(theme.accent)),
                                        )
                                    }),
                            )
                            .when_some(entry.body.clone(), |body, detail| {
                                body.child(
                                    div()
                                        .mt_1()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.render_selectable_text_scoped(
                                            "notification-body",
                                            id,
                                            detail,
                                            theme.text_muted,
                                            cx,
                                        )),
                                )
                            })
                            .child(
                                div()
                                    .mt_1()
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_selectable_text_scoped(
                                        "notification-meta",
                                        id,
                                        format!("{timestamp} | {kind} | {scope}"),
                                        theme.text_muted,
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .cursor_pointer()
                            .hover(move |button| button.bg(rgb(theme.bg_hover)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::X,
                                12.0,
                                rgb(theme.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.dismiss_notification(id);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(entry) = this.notification_center.notifications.entries
                        .iter_mut()
                        .find(|entry| entry.id == id)
                    {
                        entry.status = WorkspaceNotificationStatus::Read;
                    }
                    this.recount_notifications();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_event_log_row(
        &self,
        entry: &WorkspaceEventLogEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (icon, accent) = match entry.severity {
            WorkspaceEventSeverity::Info => (LucideIcon::Info, theme.accent),
            WorkspaceEventSeverity::Warn => (LucideIcon::AlertTriangle, 0xf59e0b),
            WorkspaceEventSeverity::Error => (LucideIcon::AlertCircle, 0xef4444),
        };
        let category = match entry.category {
            WorkspaceEventCategory::Connection => "connection",
            WorkspaceEventCategory::Reconnect => "reconnect",
            WorkspaceEventCategory::Node => "node",
        };
        let timestamp = entry
            .timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let node_label = entry
            .node_id
            .as_ref()
            .or(entry.connection_id.as_ref())
            .cloned();

        div()
            .w_full()
            .h(px(EVENT_LOG_SIDEBAR_ROW_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px_3()
            .py_1()
            .overflow_hidden()
            .text_size(px(12.0))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .bg(rgb(theme.bg))
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w(px(60.0))
                    .flex_none()
                    .truncate()
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "event-log-timestamp",
                        entry.id,
                        timestamp,
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(Self::render_lucide_icon(icon, 14.0, rgb(accent)))
            .child(self.render_event_log_category_badge(category, cx))
            .when_some(node_label, |row, node| {
                row.child(
                    div()
                        .max_w(px(120.0))
                        .flex_none()
                        .truncate()
                        .text_color(rgb(theme.accent))
                        .child(self.render_selectable_text_scoped(
                            "event-log-node",
                            entry.id,
                            node,
                            theme.accent,
                            cx,
                        )),
                )
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_color(rgb(theme.text))
                    .child(self.render_selectable_text_scoped(
                        "event-log-title",
                        entry.id,
                        self.resolve_event_log_title(entry),
                        theme.text,
                        cx,
                    )),
            )
            .when_some(self.resolve_event_log_detail(entry), |row, detail| {
                row.child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .text_color(rgb(theme.text_muted))
                        .child(self.render_selectable_text_scoped(
                            "event-log-detail",
                            entry.id,
                            format!("- {detail}"),
                            theme.text_muted,
                            cx,
                        )),
                )
            })
            .into_any_element()
    }

    fn render_event_log_category_badge(
        &self,
        category: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (bg, text) = match category {
            "connection" => (0x10b981, 0x34d399),
            "reconnect" => (0xf59e0b, 0xfbbf24),
            _ => (0x3b82f6, 0x60a5fa),
        };
        div()
            .flex_none()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((bg << 8) | 0x26))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(text))
            .child(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "event-log-category",
                category,
                self.i18n.t(&format!("event_log.category.{category}")),
                text,
                cx,
            ))
            .into_any_element()
    }

    fn render_empty_sessions_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .px(px(self.tokens.metrics.empty_sidebar_padding_x))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .w_full()
                    .h(px(self.tokens.metrics.empty_sidebar_height))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(div().mb_3().child(Self::render_lucide_icon(
                        LucideIcon::Server,
                        self.tokens.metrics.empty_sidebar_icon_size,
                        rgba((theme.text_muted << 8) | 0x4d),
                    )))
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "sessions-sidebar-empty-title",
                                (),
                                self.i18n.t("sessions.tree.no_sessions"),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .mt_1()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_subtitle_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "sessions-sidebar-empty-subtitle",
                                (),
                                self.i18n.t("sessions.tree.click_to_add"),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }
}

fn notification_kind_label(kind: WorkspaceNotificationKind) -> &'static str {
    match kind {
        WorkspaceNotificationKind::Connection => "connection",
        WorkspaceNotificationKind::Security => "security",
        WorkspaceNotificationKind::Transfer => "transfer",
        WorkspaceNotificationKind::Update => "update",
        WorkspaceNotificationKind::Health => "health",
        WorkspaceNotificationKind::Plugin => "plugin",
        WorkspaceNotificationKind::Agent => "agent",
    }
}

fn resolve_event_log_text(i18n: &I18n, raw: &str) -> Option<String> {
    if !raw.starts_with("event_log.") {
        return None;
    }
    let (key, count) = raw
        .split_once(':')
        .map(|(key, value)| (key, value.parse::<usize>().ok()))
        .unwrap_or((raw, None));
    let mut translated = i18n.t(key);
    if translated == key {
        return Some(raw.to_string());
    }
    if let Some(count) = count {
        translated = translated.replace("{{count}}", &count.to_string());
    }
    Some(translated)
}

fn notification_sidebar_row_signatures(entries: &[WorkspaceNotificationEntry]) -> Vec<u64> {
    entries
        .iter()
        .map(|entry| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            // The variable-height list must be invalidated when visible text or
            // read state changes, mirroring React keys plus prop updates in
            // Tauri's NotificationsPanel.
            std::hash::Hash::hash(&entry.id, &mut hasher);
            std::hash::Hash::hash(&(entry.status as u8), &mut hasher);
            std::hash::Hash::hash(&(entry.kind as u8), &mut hasher);
            std::hash::Hash::hash(&(entry.severity as u8), &mut hasher);
            std::hash::Hash::hash(&entry.title, &mut hasher);
            std::hash::Hash::hash(&entry.body, &mut hasher);
            match &entry.scope {
                WorkspaceNotificationScope::Global => {
                    std::hash::Hash::hash(&0u8, &mut hasher);
                }
                WorkspaceNotificationScope::Node(node_id) => {
                    std::hash::Hash::hash(&1u8, &mut hasher);
                    std::hash::Hash::hash(node_id, &mut hasher);
                }
                WorkspaceNotificationScope::Connection(connection_id) => {
                    std::hash::Hash::hash(&2u8, &mut hasher);
                    std::hash::Hash::hash(connection_id, &mut hasher);
                }
            }
            std::hash::Hasher::finish(&hasher)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SidebarActionKind {
    ToggleSessionView,
    AutoRoute,
    NewConnection,
}

#[cfg(test)]
mod sidebar_resize_region_tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        Context, IntoElement, Modifiers, MouseButton, ParentElement, Point, Render, Styled,
        TestAppContext, Window, div, px, size,
    };

    struct TestContextSidebarChrome {
        total_width: f32,
        resize_started: Rc<Cell<bool>>,
    }

    impl Render for TestContextSidebarChrome {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let resize_started = self.resize_started.clone();
            context_sidebar_frame_chrome(self.total_width)
                .debug_selector(|| "context-frame".to_string())
                .child(
                    context_sidebar_region_chrome()
                        .debug_selector(|| "context-region".to_string())
                        .child(
                            div()
                                .size_full()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .w_full()
                                        .min_w(px(0.0))
                                        .flex_none()
                                        .h(px(42.0))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(8.0))
                                        .px_3()
                                        .debug_selector(|| "context-titlebar".to_string())
                                        .child(
                                            div()
                                                .h_full()
                                                .flex_1()
                                                .min_w(px(0.0))
                                                .debug_selector(|| "context-title-drag".to_string()),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .size(px(28.0))
                                                .debug_selector(|| "context-collapse".to_string()),
                                        ),
                                ),
                        ),
                )
                .child(
                    context_sidebar_resize_hotzone_chrome()
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(CONTEXT_SIDEBAR_RESIZE_DIVIDER_WIDTH))
                                .debug_selector(|| "context-divider".to_string()),
                        )
                        .debug_selector(|| "context-hotzone".to_string())
                        .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                            resize_started.set(true);
                            cx.stop_propagation();
                        }),
                )
        }
    }

    fn right_edge(bounds: &gpui::Bounds<gpui::Pixels>) -> f32 {
        f32::from(bounds.origin.x) + f32::from(bounds.size.width)
    }

    fn assert_close(label: &str, actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.5,
            "{label}: expected {expected}, got {actual}"
        );
    }

    #[test]
    fn context_sidebar_resize_hotzone_is_frame_overlay_owned() {
        // The resize affordance must remain wide enough for reliable dragging,
        // but it must not reserve layout width between the terminal and the
        // sidebar content. A frame-owned absolute overlay preserves both.
        assert!(CONTEXT_SIDEBAR_RESIZE_HOTZONE_WIDTH >= 8.0);
        assert_eq!(CONTEXT_SIDEBAR_RESIZE_DIVIDER_WIDTH, 1.0);
    }

    #[gpui::test]
    fn context_sidebar_region_fills_frame_without_layout_gutter(cx: &mut TestAppContext) {
        let total_width = 620.0;
        let resize_started = Rc::new(Cell::new(false));

        let (_, cx) = cx.add_window_view(|_, _| TestContextSidebarChrome {
            total_width,
            resize_started: resize_started.clone(),
        });
        cx.simulate_resize(size(px(700.0), px(180.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });

        let frame = cx.debug_bounds("context-frame").expect("frame bounds");
        let divider = cx.debug_bounds("context-divider").expect("divider bounds");
        let region = cx.debug_bounds("context-region").expect("region bounds");
        let titlebar = cx
            .debug_bounds("context-titlebar")
            .expect("titlebar bounds");
        let collapse = cx
            .debug_bounds("context-collapse")
            .expect("collapse bounds");
        let hotzone = cx.debug_bounds("context-hotzone").expect("hotzone bounds");

        assert_close("frame width", f32::from(frame.size.width), total_width);
        assert_close(
            "region origin",
            f32::from(region.origin.x) - f32::from(frame.origin.x),
            0.0,
        );
        assert_close(
            "divider origin",
            f32::from(divider.origin.x),
            f32::from(region.origin.x),
        );
        assert_close(
            "region width",
            f32::from(region.size.width),
            total_width,
        );
        assert_close(
            "titlebar width",
            f32::from(titlebar.size.width),
            f32::from(region.size.width),
        );

        // The collapse control should be at the right chrome edge, allowing for
        // the titlebar padding. This catches regressions where the titlebar row
        // shrinks to the intrinsic "OxideSens" title width.
        let right_padding = right_edge(&titlebar) - right_edge(&collapse);
        assert_close("collapse right padding", right_padding, 12.0);

        assert_close(
            "hotzone origin",
            f32::from(hotzone.origin.x) - f32::from(frame.origin.x),
            0.0,
        );
        assert_close(
            "hotzone width",
            f32::from(hotzone.size.width),
            CONTEXT_SIDEBAR_RESIZE_HOTZONE_WIDTH,
        );

        cx.simulate_mouse_down(
            Point::new(frame.origin.x + px(4.0), frame.origin.y + px(20.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            resize_started.get(),
            "resize hotzone should receive edge mouse down after content is rendered"
        );
    }
}
