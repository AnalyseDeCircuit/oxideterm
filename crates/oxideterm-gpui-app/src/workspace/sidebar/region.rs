use super::*;

pub(in crate::workspace) const SIDEBAR_RESIZE_HOTZONE_PADDING: f32 = 4.0;
pub(in crate::workspace) const SIDEBAR_RESIZE_DIVIDER_WIDTH: f32 = 1.0;
pub(in crate::workspace) const SIDEBAR_RESIZE_HOTZONE_WIDTH: f32 =
    SIDEBAR_RESIZE_DIVIDER_WIDTH + SIDEBAR_RESIZE_HOTZONE_PADDING * 2.0;
const EMBEDDED_SFTP_SPLIT_HANDLE_SIZE: f32 = 7.0;
const EMBEDDED_SFTP_SPLIT_LINE_SIZE: f32 = 1.0;
const EMBEDDED_SFTP_SPLIT_HOVER_ALPHA: u32 = 0x1f;
const ACTIVITY_TOOLBAR_BUTTON_SIZE: f32 = 28.0;
const ACTIVITY_TOOLBAR_ICON_SIZE: f32 = 15.0;
const ACTIVITY_TOOLBAR_GROUP_PADDING: f32 = 2.0;
const ACTIVITY_EMPTY_STATE_ICON_SIZE: f32 = 20.0;
const ACTIVITY_TOOLBAR_ACTIVE_BACKGROUND_ALPHA: u32 = 0x1f;
const ACTIVITY_TOOLBAR_ACTIVE_BORDER_ALPHA: u32 = 0x52;

pub(in crate::workspace) fn context_sidebar_frame_chrome(
    total_width: f32,
) -> gpui::Stateful<gpui::Div> {
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

pub(in crate::workspace) fn context_sidebar_region_chrome() -> gpui::Div {
    div().relative().flex_1().min_w(px(0.0)).h_full().min_h_0()
}

pub(in crate::workspace) fn sidebar_resize_hotzone_chrome(
    element_id: &'static str,
    line_color: gpui::Rgba,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(element_id)
        .absolute()
        .w(px(SIDEBAR_RESIZE_HOTZONE_WIDTH))
        .cursor_col_resize()
        // Match browser split panes: the hit target straddles the seam while
        // remaining fully transparent except for its centered divider.
        .occlude()
        .bg(rgba(0x00000000))
        .child(
            div()
                .absolute()
                .left(px(SIDEBAR_RESIZE_HOTZONE_PADDING))
                .top_0()
                .bottom_0()
                .w(px(SIDEBAR_RESIZE_DIVIDER_WIDTH))
                // Keep the resize cursor when the pointer lands on the painted line itself.
                .cursor_col_resize()
                .bg(line_color),
        )
}

pub(in crate::workspace) fn sidebar_resize_hotzone_origin(seam: f32) -> f32 {
    seam - SIDEBAR_RESIZE_HOTZONE_PADDING
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_animated_sidebar_region(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded_width = self.sidebar_panel_width();
        let expanded = !self.sidebar_collapsed;
        let content = div()
            .flex_none()
            .w(px(expanded_width))
            .h_full()
            .child(self.render_sidebar_region(window, cx));
        oxideterm_gpui_ui::motion::horizontal_reveal(
            &self.tokens,
            "workspace-left-sidebar-motion",
            content,
            expanded_width,
            expanded,
        )
    }

    pub(in crate::workspace) fn render_animated_context_sidebar_frame(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded_width = self.ai_entity.read(cx).chat_ui().sidebar_width;
        let expanded = self.context_sidebar_visible();
        let content = div()
            .flex_none()
            .w(px(expanded_width))
            .h_full()
            .child(self.render_context_right_sidebar_frame(cx));
        oxideterm_gpui_ui::motion::horizontal_reveal(
            &self.tokens,
            "workspace-right-sidebar-motion",
            content,
            expanded_width,
            expanded,
        )
    }

    pub(in crate::workspace) fn render_sidebar_region(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .relative()
            .w(px(self.sidebar_panel_width()))
            .h_full()
            .child(self.render_sidebar(window, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_context_right_sidebar_frame(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        context_sidebar_frame_chrome(self.ai_entity.read(cx).chat_ui().sidebar_width)
            .child(self.render_context_right_sidebar_region(cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_context_right_sidebar_region(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (title_key, title_role, icon) = match self.active_context_sidebar_panel {
            ContextSidebarPanel::Assistant => {
                ("sidebar.panels.ai", "assistant", LucideIcon::Sparkles)
            }
            ContextSidebarPanel::HostTools => (
                "sidebar.panels.host_tools",
                "host-tools",
                LucideIcon::Wrench,
            ),
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
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex_none()
                            // The right sidebar header sits beside the main
                            // tabbar, so keep both chrome rows exactly aligned.
                            .h(px(self.tokens.metrics.tabbar_height))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .px_3()
                            // Match the center tab bar's chrome opacity instead
                            // of inheriting the more transparent sidebar body.
                            .bg(self.workspace_chrome_background(theme.bg))
                            // The context-sidebar titlebar is fixed chrome.
                            // Give it its own hitbox so wheel/drag events
                            // cannot fall through to a scrollable tool body.
                            .occlude()
                            .border_b_1()
                            .border_color(rgb(theme.border))
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
                                        self.tokens.metrics.sidebar_collapse_icon_size,
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
                            // Keep the sidebar tint below the titlebar so the
                            // translucent chrome is composited exactly once.
                            .bg(self.workspace_sidebar_background(theme.bg))
                            .child(match self.active_context_sidebar_panel {
                                ContextSidebarPanel::Assistant => {
                                    self.render_ai_sidebar_content(cx)
                                }
                                ContextSidebarPanel::HostTools => {
                                    self.render_host_tools_context_panel(cx)
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_left_sidebar_resize_hotzone(
        &mut self,
        top_offset: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let seam = self.tokens.metrics.activity_bar_width + self.sidebar_panel_width();
        sidebar_resize_hotzone_chrome(
            "workspace-left-sidebar-resize-hotzone",
            if self.sidebar_resizing {
                rgb(theme.accent)
            } else {
                rgba(0x00000000)
            },
        )
        .left(px(sidebar_resize_hotzone_origin(seam)))
        .top(px(top_offset))
        .bottom_0()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                this.start_sidebar_resize(event, window, cx);
                window.prevent_default();
                cx.stop_propagation();
            }),
        )
        .on_hover(cx.listener(|this, hovered, _window, cx| {
            if this.sidebar_resize_hotzone_hovered != *hovered {
                this.sidebar_resize_hotzone_hovered = *hovered;
                cx.notify();
            }
        }))
        .into_any_element()
    }

    pub(in crate::workspace) fn render_context_right_sidebar_resize_hotzone(
        &mut self,
        top_offset: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        sidebar_resize_hotzone_chrome(
            "context-right-sidebar-resize-hotzone",
            if self.ai_entity.read(cx).chat_ui().sidebar_resizing {
                rgb(theme.accent)
            } else {
                rgb(theme.border)
            },
        )
        .right(px(sidebar_resize_hotzone_origin(
            self.ai_entity.read(cx).chat_ui().sidebar_width,
        )))
        .top(px(top_offset))
        .bottom_0()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                this.start_ai_sidebar_resize(event, window, cx);
                window.prevent_default();
                cx.stop_propagation();
            }),
        )
        .on_hover(cx.listener(|this, hovered, _window, cx| {
            if this.sidebar_resize_hotzone_hovered != *hovered {
                this.sidebar_resize_hotzone_hovered = *hovered;
                cx.notify();
            }
        }))
        .into_any_element()
    }

    pub(in crate::workspace) fn render_context_sidebar_panel_title(
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

    pub(in crate::workspace) fn render_sidebar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(theme.border))
            .child(self.render_sidebar_header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    // Keep the body on the lighter sidebar tint while the
                    // fixed header independently matches workspace chrome.
                    .bg(self.workspace_sidebar_background(theme.bg_panel))
                    .child(self.render_sidebar_content(window, cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_sidebar_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let panel_section = self.effective_sidebar_panel_section();
        let plugin_panel_title = (panel_section == SidebarSection::Extensions)
            .then(|| {
                self.plugin_manager_state(cx)
                    .active_sidebar_panel
                    .as_ref()
                    .and_then(|selection| {
                        self.plugin_entity
                            .read(cx)
                            .registry()
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
            SidebarSection::Forwards => "forwards.table.title",
            SidebarSection::Extensions => "sidebar.panels.plugins",
            SidebarSection::CloudSync => "plugin.cloud_sync.panel_title",
            SidebarSection::Notifications => "sidebar.panels.event_log",
            _ => "sidebar.panels.sessions",
        };
        let title = plugin_panel_title.unwrap_or_else(|| self.i18n.t(title_key).to_uppercase());
        let mut header = div()
            // Align sidebar titles with the neighboring workspace tab bar.
            .h(px(self.tokens.metrics.tabbar_height))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            // Use the same image-background opacity as the adjacent tab bar
            // without stacking it over the sidebar body's translucent tint.
            .bg(self.workspace_chrome_background(theme.bg))
            .border_b_1()
            .border_color(rgb(theme.border))
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
                .child(self.render_sidebar_action(
                    LucideIcon::Plus,
                    SidebarActionKind::NewConnection,
                    cx,
                ));
        }
        header.into_any_element()
    }

    pub(in crate::workspace) fn render_sidebar_action(
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
                        SidebarActionKind::NewConnection => {
                            this.open_new_connection_form(window, cx)
                        }
                    }
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_sidebar_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel_section = self.effective_sidebar_panel_section();
        if panel_section == SidebarSection::Sessions {
            let sessions = self.render_active_sessions_sidebar_content(cx);
            if self.embedded_sftp_node_id.is_none() {
                return sessions;
            }
            let sftp = self.render_sftp_sidebar_surface(window, cx);
            let session_fraction = self
                .settings_store
                .settings()
                .sftp
                .sidebar_session_fraction
                .clamp(
                    EMBEDDED_SFTP_MIN_SESSION_FRACTION,
                    EMBEDDED_SFTP_MAX_SESSION_FRACTION,
                );
            let file_fraction = 1.0 - session_fraction;
            let split_line_top =
                (EMBEDDED_SFTP_SPLIT_HANDLE_SIZE - EMBEDDED_SFTP_SPLIT_LINE_SIZE) / 2.0;
            let split_line_color = if self.embedded_sftp_sidebar_resizing {
                rgb(self.tokens.ui.accent)
            } else {
                rgb(self.tokens.ui.border)
            };
            let split_hover_bg =
                rgba((self.tokens.ui.accent << 8) | EMBEDDED_SFTP_SPLIT_HOVER_ALPHA);
            // Embedded SFTP is a subordinate surface of Active Sessions. The
            // upper navigator and lower file browser scroll independently,
            // while the divider keeps them visually in one sidebar shell.
            return div()
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .flex_grow_0()
                        .flex_shrink_1()
                        .flex_basis(relative(session_fraction))
                        .min_h(px(0.0))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(sessions),
                )
                .child(
                    div()
                        .id("embedded-sftp-sidebar-splitter")
                        .flex_none()
                        .relative()
                        .h(px(EMBEDDED_SFTP_SPLIT_HANDLE_SIZE))
                        .w_full()
                        .cursor(CursorStyle::ResizeRow)
                        .hover(move |handle| handle.bg(split_hover_bg))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(px(split_line_top))
                                .h(px(EMBEDDED_SFTP_SPLIT_LINE_SIZE))
                                .bg(split_line_color),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                this.start_embedded_sftp_sidebar_resize(event, window, cx);
                                window.prevent_default();
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(
                    div()
                        .flex_grow_0()
                        .flex_shrink_1()
                        .flex_basis(relative(file_fraction))
                        .min_h(px(0.0))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(sftp),
                )
                .into_any_element();
        }
        if panel_section == SidebarSection::Extensions {
            return self.render_native_plugin_sidebar_content(cx);
        }
        if panel_section == SidebarSection::CloudSync {
            return self.render_cloud_sync_sidebar_content(cx);
        }
        if panel_section == SidebarSection::Forwards {
            // Tauri only persists these command-palette section keys here; it
            // does not reuse the Sessions empty state for their sidebar body.
            return self.render_blank_sidebar_content();
        }
        self.render_empty_sessions_sidebar_content(cx)
    }

    pub(in crate::workspace) fn render_blank_sidebar_content(&self) -> AnyElement {
        div().flex_1().w_full().into_any_element()
    }

    pub(in crate::workspace) fn render_notifications_center_content(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filtered = self
            .notification_center
            .notifications
            .entries
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
            .child(
                div()
                    .id("notifications-sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .when(row_count == 0, |content| {
                        content.child(self.render_activity_empty_state(
                            LucideIcon::Inbox,
                            self.i18n.t("event_log.notification_empty"),
                        ))
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
        self.activity_toolbar_shell()
            .flex()
            .items_center()
            .child(
                self.activity_toolbar_group()
                    .child(self.render_activity_icon_button(
                        LucideIcon::Bell,
                        self.notification_center.notifications.dnd_enabled,
                        |this, _event, _window, cx| {
                            this.notification_center.notifications.dnd_enabled =
                                !this.notification_center.notifications.dnd_enabled;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::ListTree,
                        self.notification_center.notifications.filter.status
                            != WorkspaceNotificationStatusFilter::All,
                        |this, _event, _window, cx| {
                            this.cycle_notification_status_filter();
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::AlertCircle,
                        self.notification_center.notifications.filter.severity
                            != WorkspaceNotificationSeverityFilter::All,
                        |this, _event, _window, cx| {
                            this.cycle_notification_severity_filter();
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::Hash,
                        self.notification_center.notifications.filter.kind
                            != WorkspaceNotificationKindFilter::All,
                        |this, _event, _window, cx| {
                            this.cycle_notification_kind_filter();
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .when(
                self.notification_center.notifications.dnd_enabled,
                |toolbar| {
                    toolbar.child(oxideterm_gpui_ui::status_pill(
                        &self.tokens,
                        self.i18n.t("event_log.dnd.on"),
                        oxideterm_gpui_ui::StatusPillOptions::new(
                            oxideterm_gpui_ui::StatusTone::Warning,
                        )
                        .compact(),
                    ))
                },
            )
            .child(div().flex_1())
            .child(
                self.activity_toolbar_group()
                    .child(self.render_activity_icon_button(
                        LucideIcon::Check,
                        false,
                        |this, _event, _window, cx| {
                            this.mark_all_notifications_read();
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::Trash2,
                        false,
                        |this, _event, _window, cx| {
                            this.clear_notifications();
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_event_log_center_content(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filtered = self
            .notification_center
            .event_log
            .entries
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
            .child(
                div()
                    .id("event-log-sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_hidden()
                    .when(row_count == 0, |content| {
                        content.child(self.render_activity_empty_state(
                            LucideIcon::History,
                            self.i18n.t("event_log.empty"),
                        ))
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

    pub(in crate::workspace) fn schedule_event_log_virtual_scroll_to_bottom_if_sticky(
        &self,
        handle: UniformListScrollHandle,
        last_index: usize,
        spec: TauriVirtualListSpec,
        cx: &mut Context<Self>,
    ) {
        if !tauri_virtual_list_is_near_bottom(&handle, px(EVENT_LOG_STICKY_BOTTOM_THRESHOLD_PX)) {
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
        self.activity_toolbar_shell()
            .flex()
            .items_center()
            .child(
                self.activity_toolbar_group().child(
                    div()
                        .px(px(self.tokens.spacing.one))
                        .flex()
                        .items_center()
                        .gap(px(self.tokens.spacing.two))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .child(self.render_count_chip(
                            LucideIcon::AlertCircle,
                            theme.error,
                            counts.2,
                            cx,
                        ))
                        .child(self.render_count_chip(
                            LucideIcon::AlertTriangle,
                            theme.warning,
                            counts.1,
                            cx,
                        ))
                        .child(self.render_count_chip(
                            LucideIcon::Info,
                            theme.accent,
                            counts.0,
                            cx,
                        )),
                ),
            )
            .child(div().flex_1())
            .when(self.notification_center.event_log.dnd_enabled, |toolbar| {
                toolbar.child(oxideterm_gpui_ui::status_pill(
                    &self.tokens,
                    self.i18n.t("event_log.dnd.on"),
                    oxideterm_gpui_ui::StatusPillOptions::new(
                        oxideterm_gpui_ui::StatusTone::Warning,
                    )
                    .compact(),
                ))
            })
            .child(
                self.activity_toolbar_group()
                    .child(self.render_activity_icon_button(
                        LucideIcon::Bell,
                        self.notification_center.event_log.dnd_enabled,
                        |this, _event, _window, cx| {
                            this.notification_center.event_log.dnd_enabled =
                                !this.notification_center.event_log.dnd_enabled;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::ListTree,
                        self.notification_center.event_log.filter.severity
                            != WorkspaceEventSeverityFilter::All,
                        |this, _event, _window, cx| {
                            this.cycle_event_log_severity_filter();
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::Search,
                        self.notification_center.event_log.filter.category
                            != WorkspaceEventCategoryFilter::All,
                        |this, _event, _window, cx| {
                            this.cycle_event_log_category_filter();
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::Trash2,
                        false,
                        |this, _event, _window, cx| {
                            this.clear_event_log();
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_activity_icon_button(
        &self,
        icon: LucideIcon,
        active: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let icon_color = if active {
            rgb(theme.accent)
        } else {
            rgb(theme.text_muted)
        };
        self.workspace_icon_action_button(
            icon,
            ACTIVITY_TOOLBAR_ICON_SIZE,
            icon_color,
            oxideterm_gpui_ui::button::IconButtonOptions {
                background: active
                    .then(|| rgba((theme.accent << 8) | ACTIVITY_TOOLBAR_ACTIVE_BACKGROUND_ALPHA)),
                border: active
                    .then(|| rgba((theme.accent << 8) | ACTIVITY_TOOLBAR_ACTIVE_BORDER_ALPHA)),
                hover_background: Some(rgb(theme.bg_hover)),
                ..oxideterm_gpui_ui::button::IconButtonOptions::opaque_toolbar(
                    ACTIVITY_TOOLBAR_BUTTON_SIZE,
                    oxideterm_gpui_ui::button::ButtonRadius::Sm,
                )
            },
            listener,
            cx,
        )
        .into_any_element()
    }

    fn activity_toolbar_shell(&self) -> gpui::Div {
        let theme = self.tokens.ui;
        // The full-page toolbar uses compact grouped controls so icon actions
        // remain subordinate to the notification content and page switcher.
        div()
            .h(px(ACTIVITY_TOOLBAR_BUTTON_SIZE + self.tokens.spacing.three))
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.three))
            .border_b_1()
            .border_color(rgb(theme.border))
    }

    fn activity_toolbar_group(&self) -> gpui::Div {
        oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::InsetGroup)
                .padding(oxideterm_gpui_ui::SurfacePadding::None)
                .has_background_image(self.background_surface_active("notification_center")),
        )
        .flex()
        .items_center()
        .gap(px(ACTIVITY_TOOLBAR_GROUP_PADDING))
        .p(px(ACTIVITY_TOOLBAR_GROUP_PADDING))
    }

    fn render_activity_empty_state(&self, icon: LucideIcon, label: String) -> AnyElement {
        let theme = self.tokens.ui;
        oxideterm_gpui_ui::empty_state(
            &self.tokens,
            Self::render_lucide_icon(icon, ACTIVITY_EMPTY_STATE_ICON_SIZE, rgb(theme.accent)),
            label,
            None,
            None,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_count_chip(
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

    pub(in crate::workspace) fn filtered_event_log_counts(&self) -> (usize, usize, usize) {
        let mut info = 0;
        let mut warn = 0;
        let mut error = 0;
        for entry in self
            .notification_center
            .event_log
            .entries
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

    pub(in crate::workspace) fn resolve_event_log_title(
        &self,
        entry: &WorkspaceEventLogEntry,
    ) -> String {
        resolve_event_log_text(&self.i18n, &entry.title).unwrap_or_else(|| entry.title.clone())
    }

    pub(in crate::workspace) fn resolve_event_log_detail(
        &self,
        entry: &WorkspaceEventLogEntry,
    ) -> Option<String> {
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

    pub(in crate::workspace) fn render_notification_row(
        &self,
        entry: &WorkspaceNotificationEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let id = entry.id;
        let (icon, accent) = match entry.severity {
            WorkspaceNotificationSeverity::Info => (LucideIcon::Info, theme.accent),
            WorkspaceNotificationSeverity::Warning => (LucideIcon::AlertTriangle, theme.warning),
            WorkspaceNotificationSeverity::Error => (LucideIcon::AlertCircle, theme.error),
            WorkspaceNotificationSeverity::Critical => (LucideIcon::Shield, theme.error),
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
                                            .child(self.render_display_text_with_role(
                                                SelectableTextRole::NonSelectable,
                                                "notification-title",
                                                entry.id,
                                                entry.title.clone(),
                                                theme.text_heading,
                                                cx,
                                            )),
                                    )
                                    .when(
                                        status_unread
                                            && !self.notification_center.notifications.dnd_enabled,
                                        |row| {
                                            row.child(
                                                div()
                                                    .size(px(6.0))
                                                    .rounded_full()
                                                    .bg(rgb(theme.accent)),
                                            )
                                        },
                                    ),
                            )
                            .when_some(entry.body.clone(), |body, detail| {
                                body.child(
                                    div()
                                        .mt_1()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.render_display_text_with_role(
                                            SelectableTextRole::NonSelectable,
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
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::NonSelectable,
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
                    if let Some(entry) = this
                        .notification_center
                        .notifications
                        .entries
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

    pub(in crate::workspace) fn render_event_log_row(
        &self,
        entry: &WorkspaceEventLogEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (icon, accent) = match entry.severity {
            WorkspaceEventSeverity::Info => (LucideIcon::Info, theme.accent),
            WorkspaceEventSeverity::Warn => (LucideIcon::AlertTriangle, theme.warning),
            WorkspaceEventSeverity::Error => (LucideIcon::AlertCircle, theme.error),
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

    pub(in crate::workspace) fn render_event_log_category_badge(
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

    pub(in crate::workspace) fn render_empty_sessions_sidebar_content(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

pub(in crate::workspace) fn notification_kind_label(
    kind: WorkspaceNotificationKind,
) -> &'static str {
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

pub(in crate::workspace) fn resolve_event_log_text(i18n: &I18n, raw: &str) -> Option<String> {
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

pub(in crate::workspace) fn notification_sidebar_row_signatures(
    entries: &[WorkspaceNotificationEntry],
) -> Vec<u64> {
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
pub(in crate::workspace) enum SidebarActionKind {
    ToggleSessionView,
    NewConnection,
}

#[cfg(test)]
mod sidebar_resize_region_tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        Context, CursorStyle, IntoElement, Modifiers, MouseButton, ParentElement, Point, Render,
        Styled, TestAppContext, Window, canvas, div, px, size,
    };

    struct TestContextSidebarChrome {
        total_width: f32,
        resize_started: Rc<Cell<bool>>,
        resize_moved: Rc<Cell<bool>>,
        resizing: bool,
    }

    struct TestLeftSidebarChrome {
        total_width: f32,
        resize_started: Rc<Cell<bool>>,
        hotzone_hovered: bool,
    }

    impl Render for TestLeftSidebarChrome {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let resize_started = self.resize_started.clone();
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .w(px(self.total_width))
                        .h_full()
                        .debug_selector(|| "left-frame".to_string())
                        // Simulate loaded sidebar content owning the full visible surface.
                        .child(div().absolute().size_full().occlude())
                        // Simulate a custom-painted child requesting a window-wide cursor.
                        .child(canvas(
                            |_, _, _| (),
                            |_, _, window, _| {
                                window.set_window_cursor_style(CursorStyle::Arrow);
                            },
                        )),
                )
                .child(
                    sidebar_resize_hotzone_chrome("left-hotzone-element", rgba(0x000000ff))
                        .left(px(sidebar_resize_hotzone_origin(self.total_width)))
                        .top_0()
                        .bottom_0()
                        .debug_selector(|| "left-hotzone".to_string())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, _event, _window, _cx| {
                                resize_started.set(true);
                            }),
                        )
                        .on_hover(cx.listener(|this, hovered, _window, cx| {
                            if this.hotzone_hovered != *hovered {
                                this.hotzone_hovered = *hovered;
                                cx.notify();
                            }
                        })),
                )
                .when(self.hotzone_hovered, |root| {
                    root.child(canvas(
                        |_, _, _| (),
                        |_, _, window, _| {
                            // The resize handle must beat earlier window-wide cursor requests.
                            window.set_window_cursor_style(CursorStyle::ResizeColumn);
                        },
                    ))
                })
        }
    }

    impl Render for TestContextSidebarChrome {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let resize_started = self.resize_started.clone();
            let resize_moved = self.resize_moved.clone();
            let seam = f32::from(window.viewport_size().width) - self.total_width;
            div()
                .relative()
                .size_full()
                .flex()
                .justify_end()
                .child(
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
                                        // Simulate loaded Host Tools content owning a blocking hitbox.
                                        .child(div().absolute().size_full().occlude())
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
                                                        .debug_selector(|| {
                                                            "context-title-drag".to_string()
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .size(px(28.0))
                                                        .debug_selector(|| {
                                                            "context-collapse".to_string()
                                                        }),
                                                ),
                                        ),
                                ),
                        ),
                )
                .child(
                    sidebar_resize_hotzone_chrome("context-hotzone-element", rgba(0x000000ff))
                        .left(px(sidebar_resize_hotzone_origin(seam)))
                        .top_0()
                        .bottom_0()
                        .debug_selector(|| "context-hotzone".to_string())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.resizing = true;
                                resize_started.set(true);
                                cx.notify();
                            }),
                        ),
                )
                .when(self.resizing, |root| {
                    root.child(
                        div()
                            .absolute()
                            .size_full()
                            .occlude()
                            .on_mouse_move(cx.listener(
                                move |this, event: &MouseMoveEvent, window, cx| {
                                    // Root capture owns movement after the pointer leaves the hotzone.
                                    this.total_width = (f32::from(window.viewport_size().width)
                                        - f32::from(event.position.x))
                                    .max(0.0);
                                    resize_moved.set(true);
                                    cx.notify();
                                },
                            )),
                    )
                })
        }
    }

    pub(in crate::workspace) fn right_edge(bounds: &gpui::Bounds<gpui::Pixels>) -> f32 {
        f32::from(bounds.origin.x) + f32::from(bounds.size.width)
    }

    pub(in crate::workspace) fn assert_close(label: &str, actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.5,
            "{label}: expected {expected}, got {actual}"
        );
    }

    #[gpui::test]
    pub(in crate::workspace) fn left_sidebar_resize_hotzone_overlays_loaded_content(
        cx: &mut TestAppContext,
    ) {
        let total_width = 280.0;
        let resize_started = Rc::new(Cell::new(false));
        let (_, cx) = cx.add_window_view(|_, _| TestLeftSidebarChrome {
            total_width,
            resize_started: resize_started.clone(),
            hotzone_hovered: false,
        });
        cx.simulate_resize(size(px(700.0), px(180.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let frame = cx.debug_bounds("left-frame").expect("left frame bounds");
        let hotzone = cx
            .debug_bounds("left-hotzone")
            .expect("left hotzone bounds");
        assert_close("left frame width", f32::from(frame.size.width), total_width);
        assert_close(
            "left hotzone width",
            f32::from(hotzone.size.width),
            SIDEBAR_RESIZE_HOTZONE_WIDTH,
        );
        assert_close(
            "left divider position",
            f32::from(hotzone.origin.x) + SIDEBAR_RESIZE_HOTZONE_PADDING,
            right_edge(&frame),
        );

        cx.simulate_mouse_move(
            Point::new(
                frame.origin.x + frame.size.width + px(3.0),
                frame.origin.y + px(20.0),
            ),
            None,
            Modifiers::default(),
        );
        assert_eq!(
            cx.update(|window, _cx| window.cursor_style_for_test()),
            CursorStyle::ResizeColumn,
            "hovering the left resize hotzone should apply the column-resize cursor"
        );
        cx.simulate_mouse_down(
            Point::new(
                frame.origin.x + frame.size.width + px(3.0),
                frame.origin.y + px(20.0),
            ),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            resize_started.get(),
            "left resize hotzone should receive mouse down above loaded content"
        );
    }

    #[gpui::test]
    pub(in crate::workspace) fn context_sidebar_resize_hotzone_has_no_gap_after_content_load(
        cx: &mut TestAppContext,
    ) {
        let total_width = 620.0;
        let resize_started = Rc::new(Cell::new(false));
        let resize_moved = Rc::new(Cell::new(false));

        let (_, cx) = cx.add_window_view(|_, _| TestContextSidebarChrome {
            total_width,
            resize_started: resize_started.clone(),
            resize_moved: resize_moved.clone(),
            resizing: false,
        });
        cx.simulate_resize(size(px(700.0), px(180.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let frame = cx.debug_bounds("context-frame").expect("frame bounds");
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
        assert_close("region width", f32::from(region.size.width), total_width);
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
            -SIDEBAR_RESIZE_HOTZONE_PADDING,
        );
        assert_close(
            "hotzone width",
            f32::from(hotzone.size.width),
            SIDEBAR_RESIZE_HOTZONE_WIDTH,
        );

        cx.simulate_mouse_move(
            Point::new(frame.origin.x - px(3.0), frame.origin.y + px(20.0)),
            None,
            Modifiers::default(),
        );
        assert_eq!(
            cx.update(|window, _cx| window.cursor_style_for_test()),
            CursorStyle::ResizeColumn,
            "hovering the context-sidebar hotzone should apply the column-resize cursor"
        );
        cx.simulate_mouse_down(
            Point::new(frame.origin.x - px(3.0), frame.origin.y + px(20.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            resize_started.get(),
            "frame-local resize hotzone should receive mouse down above loaded content"
        );
        cx.simulate_mouse_move(
            Point::new(frame.origin.x - px(40.0), frame.origin.y + px(20.0)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        assert!(
            resize_moved.get(),
            "root capture should continue the frame-local hotzone drag"
        );
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let resized_frame = cx
            .debug_bounds("context-frame")
            .expect("resized frame bounds");
        assert_close(
            "resized frame width",
            f32::from(resized_frame.size.width),
            660.0,
        );
    }
}
