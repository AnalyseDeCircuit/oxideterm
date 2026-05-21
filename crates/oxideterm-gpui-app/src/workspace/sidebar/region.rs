impl WorkspaceApp {
    pub(super) fn render_sidebar_region(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    .bg(if self.sidebar_resizing {
                        rgb(theme.accent)
                    } else {
                        rgba(theme.bg << 8)
                    })
                    .hover(|handle| handle.bg(rgba((theme.accent << 8) | 0x80)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event, _window, cx| {
                            this.start_sidebar_resize(event, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_ai_right_sidebar_region(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .relative()
            .w(px(self.ai_sidebar_width))
            .flex_none()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(theme.bg))
            .border_l_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .child(
                div()
                    .w_full()
                    .h(px(42.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x4d))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Sparkles,
                                16.0,
                                rgb(theme.accent),
                            ))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "ai-sidebar-title",
                                        "label",
                                        self.i18n.t("sidebar.panels.ai"),
                                        theme.text,
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id("ai-sidebar-collapse")
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
                                        "ai-sidebar-collapse",
                                        label.clone(),
                                        f32::from(event.position.x) + 12.0,
                                        f32::from(event.position.y) + 16.0,
                                        cx,
                                    );
                                }
                            }))
                            .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                                if !*hovered {
                                    this.clear_workspace_tooltip("ai-sidebar-collapse", cx);
                                }
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    let _ = this.toggle_ai_sidebar(cx);
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
                    .overflow_hidden()
                    .child(self.render_ai_sidebar_content(cx)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(self.tokens.metrics.sidebar_resize_handle_width))
                    .cursor(CursorStyle::ResizeColumn)
                    .bg(if self.ai_sidebar_resizing {
                        rgb(theme.accent)
                    } else {
                        rgba(theme.bg << 8)
                    })
                    .hover(|handle| handle.bg(rgba((theme.accent << 8) | 0x80)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            this.start_ai_sidebar_resize(event, window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let title_key = match self.active_sidebar_section {
            SidebarSection::Connections => "sidebar.panels.saved_connections",
            SidebarSection::Extensions => "sidebar.panels.plugins",
            SidebarSection::CloudSync => "plugin.cloud_sync.panel_title",
            SidebarSection::Notifications => "sidebar.panels.event_log",
            _ => "sidebar.panels.sessions",
        };
        let mut header = div()
            .h(px(self.tokens.metrics.sidebar_header_height))
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "sidebar-header-title",
                        title_key,
                        self.i18n.t(title_key).to_uppercase(),
                        theme.text_muted,
                        cx,
                    )),
            );
        if matches!(
            self.active_sidebar_section,
            SidebarSection::Sessions | SidebarSection::Notifications
        ) {
            header = header
                .child(self.render_sidebar_action(LucideIcon::Folder, SidebarActionKind::None, cx))
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
            SidebarActionKind::NewConnection => self.i18n.t("sidebar.tooltips.new_connection"),
            SidebarActionKind::AutoRoute => self.i18n.t("sidebar.tooltips.auto_route"),
            SidebarActionKind::None => self.i18n.t("sidebar.panels.sftp"),
        };
        let tooltip_id = format!("sidebar-action-{:?}", action);
        let tooltip_id_for_move = tooltip_id.clone();
        let mut button = div()
            .id((gpui::ElementId::from("sidebar-action"), tooltip_id.clone()))
            .size(px(self.tokens.metrics.sidebar_action_size))
            .ml_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.sidebar_action_icon_size,
                rgb(theme.text),
            ))
            .on_mouse_move(cx.listener({
                let label = label.clone();
                move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        tooltip_id_for_move.clone(),
                        label.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                }
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip(&tooltip_id, cx);
                }
            }));

        button = match action {
            SidebarActionKind::NewConnection => button.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.open_new_connection_form(window, cx);
                    cx.stop_propagation();
                }),
            ),
            SidebarActionKind::AutoRoute => button.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.open_auto_route_modal(window, cx);
                    cx.stop_propagation();
                }),
            ),
            SidebarActionKind::None => button,
        };

        button.into_any_element()
    }

    pub(super) fn render_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.active_sidebar_section == SidebarSection::Connections {
            return self.render_saved_connections_sidebar_content(cx);
        }
        if self.active_sidebar_section == SidebarSection::Sessions {
            return self.render_active_sessions_sidebar_content(cx);
        }
        if self.active_sidebar_section == SidebarSection::Extensions {
            return self.render_plugin_sidebar_placeholder();
        }
        if self.active_sidebar_section == SidebarSection::CloudSync {
            return self.render_cloud_sync_sidebar_content(cx);
        }
        self.render_empty_sessions_sidebar_content(cx)
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
        div()
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
            })
            .on_mouse_down(
                MouseButton::Left,
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
            .collect::<Vec<_>>();
        let rows = if filtered.is_empty() {
            vec![
                div()
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
                    ))
                    .into_any_element(),
            ]
        } else {
            filtered
                .into_iter()
                .map(|entry| self.render_notification_row(entry, cx))
                .collect::<Vec<_>>()
        };

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
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("notifications-sidebar-scroll"),
                    )
                    .px_2()
                    .py_2()
                    .children(rows),
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
                cx.listener(|this, _event, _window, cx| {
                    this.notification_center.notifications.dnd_enabled = !this.notification_center.notifications.dnd_enabled;
                    cx.notify();
                }),
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::ListTree,
                self.notification_center.notifications.filter.status != WorkspaceNotificationStatusFilter::All,
                "Cycle status filter",
                cx.listener(|this, _event, _window, cx| {
                    this.cycle_notification_status_filter();
                    cx.notify();
                }),
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::AlertCircle,
                self.notification_center.notifications.filter.severity != WorkspaceNotificationSeverityFilter::All,
                "Cycle severity filter",
                cx.listener(|this, _event, _window, cx| {
                    this.cycle_notification_severity_filter();
                    cx.notify();
                }),
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::Hash,
                self.notification_center.notifications.filter.kind != WorkspaceNotificationKindFilter::All,
                "Cycle kind filter",
                cx.listener(|this, _event, _window, cx| {
                    this.cycle_notification_kind_filter();
                    cx.notify();
                }),
            ))
            .child(div().flex_1())
            .child(self.render_activity_icon_button(
                LucideIcon::Check,
                false,
                "Mark all read",
                cx.listener(|this, _event, _window, cx| {
                    this.mark_all_notifications_read();
                    cx.notify();
                }),
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::Trash2,
                false,
                "Clear notifications",
                cx.listener(|this, _event, _window, cx| {
                    this.clear_notifications();
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    pub(super) fn render_event_log_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let filtered = self.notification_center.event_log.entries
            .iter()
            .filter(|entry| self.event_log_entry_matches_filter(entry))
            .collect::<Vec<_>>();
        let event_log_scroll = self.selectable_text_scroll_handle("event-log-sidebar-scroll");
        if !filtered.is_empty() {
            self.schedule_browser_scroll_to_bottom_if_sticky(event_log_scroll.clone(), cx);
        }
        let rows = if filtered.is_empty() {
            vec![
                div()
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
                    ))
                    .into_any_element(),
            ]
        } else {
            filtered
                .into_iter()
                .map(|entry| self.render_event_log_row(entry, cx))
                .collect::<Vec<_>>()
        };

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
                    .selectable_overflow_y_scrollbar(&event_log_scroll)
                    .px_2()
                    .py_2()
                    .children(rows),
            )
            .into_any_element()
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
                cx.listener(|this, _event, _window, cx| {
                    this.notification_center.event_log.dnd_enabled = !this.notification_center.event_log.dnd_enabled;
                    cx.notify();
                }),
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::ListTree,
                self.notification_center.event_log.filter.severity != WorkspaceEventSeverityFilter::All,
                "Cycle severity filter",
                cx.listener(|this, _event, _window, cx| {
                    this.cycle_event_log_severity_filter();
                    cx.notify();
                }),
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::Search,
                self.notification_center.event_log.filter.category != WorkspaceEventCategoryFilter::All,
                "Cycle category filter",
                cx.listener(|this, _event, _window, cx| {
                    this.cycle_event_log_category_filter();
                    cx.notify();
                }),
            ))
            .child(self.render_activity_icon_button(
                LucideIcon::Trash2,
                false,
                "Clear event log",
                cx.listener(|this, _event, _window, cx| {
                    this.clear_event_log();
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    fn render_activity_icon_button(
        &self,
        icon: LucideIcon,
        active: bool,
        _label: &'static str,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .bg(if active {
                rgb(theme.bg_active)
            } else {
                rgb(theme.bg)
            })
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                12.0,
                if active {
                    rgb(theme.text_heading)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .on_mouse_down(MouseButton::Left, listener)
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
        let mut meta = format!("#{} | {} | {}", entry.id, category, entry.source);
        if let Some(node_id) = &entry.node_id {
            meta.push_str(" | ");
            meta.push_str(node_id);
        }
        if let Some(connection_id) = &entry.connection_id {
            meta.push_str(" | ");
            meta.push_str(connection_id);
        }

        div()
            .w_full()
            .mb_2()
            .p_2()
            .rounded(px(self.tokens.radii.md))
            .bg(rgb(theme.bg))
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
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.render_selectable_text_scoped(
                                        "event-log-title",
                                        entry.id,
                                        self.resolve_event_log_title(entry),
                                        theme.text_heading,
                                        cx,
                                    )),
                            )
                            .when_some(self.resolve_event_log_detail(entry), |body, detail| {
                                body.child(
                                    div()
                                        .mt_1()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.render_selectable_text_scoped(
                                            "event-log-detail",
                                            entry.id,
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
                                        "event-log-meta",
                                        entry.id,
                                        format!("{timestamp} | {meta}"),
                                        theme.text_muted,
                                        cx,
                                    )),
                            ),
                    ),
            )
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SidebarActionKind {
    None,
    AutoRoute,
    NewConnection,
}
