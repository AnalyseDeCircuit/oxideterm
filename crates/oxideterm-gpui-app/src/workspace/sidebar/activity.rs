use gpui::StatefulInteractiveElement;

impl WorkspaceApp {
    pub(super) fn render_activity_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let top_items = [
            (SidebarSection::Sessions, LucideIcon::Link2),
            (SidebarSection::Connections, LucideIcon::LayoutList),
            (SidebarSection::Terminal, LucideIcon::Terminal),
            (SidebarSection::Activity, LucideIcon::Activity),
            (SidebarSection::Network, LucideIcon::Network),
            (SidebarSection::Extensions, LucideIcon::Puzzle),
            (SidebarSection::Assistant, LucideIcon::Sparkles),
        ];
        let bottom_items = [
            (SidebarSection::Workspace, LucideIcon::Square),
            (SidebarSection::Files, LucideIcon::FolderOpen),
            (SidebarSection::Monitor, LucideIcon::Monitor),
            (SidebarSection::Notifications, LucideIcon::Bell),
            (SidebarSection::Settings, LucideIcon::Settings),
        ];

        let mut bar = div()
            .w(px(self.tokens.metrics.activity_bar_width))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .py_2()
            .bg(rgb(theme.bg))
            .border_r_1()
            .border_color(rgb(theme.border));

        bar = bar
            .child(
                div()
                    .id("activity-sidebar-toggle")
                    .size(px(self.tokens.metrics.activity_icon_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        if self.sidebar_collapsed {
                            LucideIcon::PanelLeft
                        } else {
                            LucideIcon::PanelLeftClose
                        },
                        self.tokens.metrics.activity_icon_glyph_size,
                        rgb(theme.text_heading),
                    ))
                    .on_mouse_move(cx.listener({
                        let label = self.i18n.t(if self.sidebar_collapsed {
                            "sidebar.actions.expand"
                        } else {
                            "sidebar.actions.collapse"
                        });
                        move |this, event: &MouseMoveEvent, _window, cx| {
                            this.queue_workspace_tooltip(
                                "activity-sidebar-toggle",
                                label.clone(),
                                f32::from(event.position.x) + 12.0,
                                f32::from(event.position.y) + 16.0,
                                cx,
                            );
                        }
                    }))
                    .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            this.clear_workspace_tooltip("activity-sidebar-toggle", cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.toggle_sidebar(cx);
                        }),
                    ),
            )
            .child(
                div()
                    .w(px(self.tokens.metrics.divider_width))
                    .h(px(self.tokens.metrics.divider_height))
                    .my_1()
                    .bg(rgb(theme.divider)),
            );

        for (section, icon) in top_items {
            bar = bar.child(self.render_activity_icon(section, icon, cx));
        }

        bar.child(div().flex_1())
            .child(
                div()
                    .w(px(self.tokens.metrics.divider_width))
                    .h(px(self.tokens.metrics.divider_height))
                    .bg(rgb(theme.divider)),
            )
            .children(
                bottom_items
                    .into_iter()
                    .map(|(section, icon)| self.render_activity_icon(section, icon, cx)),
            )
            .into_any_element()
    }

    pub(super) fn render_activity_icon(
        &self,
        section: SidebarSection,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = if section == SidebarSection::Notifications {
            self.active_tab()
                .is_some_and(|tab| tab.kind == TabKind::NotificationCenter)
        } else if section == SidebarSection::Assistant {
            self.ai_sidebar_visible()
        } else {
            self.active_sidebar_section == section
        };
        let tooltip = self.activity_icon_tooltip(section);
        let tooltip_id = format!("activity-icon-{}", section.as_settings_key());
        let tooltip_id_for_move = tooltip_id.clone();
        let badge_count = if section == SidebarSection::Notifications {
            let notification_count = if self.notification_center.notifications.dnd_enabled {
                0
            } else {
                self.notification_center.notifications.unread_count
            };
            let event_count = if self.notification_center.event_log.dnd_enabled {
                0
            } else {
                self.notification_center.event_log.unread_count
            };
            notification_count.saturating_add(event_count)
        } else {
            0
        };
        let badge_is_error = section == SidebarSection::Notifications
            && ((!self.notification_center.notifications.dnd_enabled && self.notification_center.notifications.unread_critical_count > 0)
                || (!self.notification_center.event_log.dnd_enabled && self.notification_center.event_log.unread_errors > 0));
        div()
            .id(("activity-icon", section as u64))
            .relative()
            .size(px(self.tokens.metrics.activity_icon_size))
            .mb(px(self.tokens.spacing.icon_gap))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .bg(if active {
                rgb(theme.bg_active)
            } else {
                rgb(theme.bg)
            })
            .border_1()
            .border_color(if active {
                rgb(theme.border)
            } else {
                rgb(theme.bg)
            })
            .cursor_pointer()
            .when(active, |icon_el| {
                icon_el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(self.tokens.metrics.activity_indicator_inset))
                        .bottom(px(self.tokens.metrics.activity_indicator_inset))
                        .w(px(self.tokens.metrics.activity_indicator_width))
                        .rounded(px(self.tokens.radii.active_indicator))
                        .bg(rgb(theme.accent)),
                )
            })
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.activity_icon_glyph_size,
                if active {
                    rgb(theme.text_heading)
                } else {
                    rgb(theme.text)
                },
            ))
            .when(badge_count > 0, |icon_el| {
                icon_el.child(
                    div()
                        .absolute()
                        .right(px(1.0))
                        .top(px(1.0))
                        .min_w(px(14.0))
                        .h(px(14.0))
                        .px(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgb(if badge_is_error { 0xef4444 } else { theme.accent }))
                        .text_color(rgb(0xffffff))
                        .text_size(px(9.0))
                        .child(if badge_count > 99 {
                            "99+".to_string()
                        } else {
                            badge_count.to_string()
                        }),
                )
            })
            .on_mouse_move(cx.listener({
                let tooltip = tooltip.clone();
                move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        tooltip_id_for_move.clone(),
                        tooltip.clone(),
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
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if section == SidebarSection::Settings {
                        this.open_settings(window, cx);
                    } else if section == SidebarSection::Connections {
                        this.open_session_manager_tab(window, cx);
                    } else if section == SidebarSection::Terminal {
                        this.open_connection_pool_tab(window, cx);
                    } else if section == SidebarSection::Activity {
                        this.open_connection_monitor_tab(window, cx);
                    } else if section == SidebarSection::Network {
                        this.open_topology_tab(window, cx);
                    } else if section == SidebarSection::Workspace {
                        // Tauri treats the bottom square as a local-terminal action,
                        // not as a sidebar content section. Keep the current sidebar
                        // selection unchanged when it opens a terminal tab.
                        let _ = this.create_local_terminal_tab(window, cx);
                    } else if section == SidebarSection::Files {
                        this.open_file_manager_tab(window, cx);
                    } else if section == SidebarSection::Monitor && cfg!(target_os = "macos") {
                        this.open_launcher_tab(window, cx);
                    } else if section == SidebarSection::Monitor && cfg!(target_os = "windows") {
                        this.open_graphics_tab(window, cx);
                    } else if section == SidebarSection::Notifications {
                        this.open_notification_center_tab(window, cx);
                    } else if section == SidebarSection::Assistant {
                        let _ = this.toggle_ai_sidebar(cx);
                    } else if section == SidebarSection::Extensions {
                        this.open_plugin_manager_tab(window, cx);
                    } else {
                        this.active_surface = ActiveSurface::Terminal;
                        this.set_sidebar_section(section, cx);
                    }
                }),
            )
            .into_any_element()
    }

    fn activity_icon_tooltip(&self, section: SidebarSection) -> String {
        match section {
            SidebarSection::Sessions => self.i18n.t("sidebar.panels.sessions"),
            SidebarSection::Connections => self.i18n.t("sidebar.panels.open_session_manager"),
            SidebarSection::Terminal => self.i18n.t("sidebar.panels.connection_pool"),
            SidebarSection::Activity => self.i18n.t("sidebar.panels.connection_monitor"),
            SidebarSection::Network => self.i18n.t("sidebar.panels.connection_matrix"),
            SidebarSection::Extensions => self.i18n.t("sidebar.panels.plugins"),
            SidebarSection::Assistant => self.i18n.t("sidebar.panels.ai"),
            SidebarSection::Automation => self.i18n.t("sidebar.panels.activity"),
            SidebarSection::Workspace => self.i18n.t("sidebar.actions.new_local_terminal"),
            SidebarSection::Files => self.i18n.t("sidebar.panels.files"),
            SidebarSection::Monitor if cfg!(target_os = "macos") => {
                self.i18n.t("launcher.tabTitle")
            }
            SidebarSection::Monitor if cfg!(target_os = "windows") => {
                self.i18n.t("graphics.tab_title")
            }
            SidebarSection::Monitor => self.i18n.t("sidebar.panels.connection_monitor"),
            SidebarSection::Notifications => self.i18n.t("sidebar.panels.notifications"),
            SidebarSection::Settings => self.i18n.t("sidebar.tooltips.settings"),
        }
    }

    pub(super) fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }
}
