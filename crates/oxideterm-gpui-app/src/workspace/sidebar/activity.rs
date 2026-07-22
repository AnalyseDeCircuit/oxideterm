use super::*;
use gpui::StatefulInteractiveElement;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_activity_bar(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let top_items_before_plugins = [
            (SidebarSection::Sessions, LucideIcon::Link2),
            (SidebarSection::Connections, LucideIcon::LayoutList),
            (SidebarSection::Runtime, LucideIcon::Gauge),
        ];
        let top_items_after_plugins = [
            (SidebarSection::CloudSync, LucideIcon::Cloud),
            (SidebarSection::Assistant, LucideIcon::Sparkles),
            (SidebarSection::HostTools, LucideIcon::Wrench),
        ];
        let mut bottom_items = vec![
            (SidebarSection::Workspace, LucideIcon::Square),
            (SidebarSection::Files, LucideIcon::FolderOpen),
            (SidebarSection::Notifications, LucideIcon::Bell),
            (SidebarSection::Settings, LucideIcon::Settings),
        ];
        if !cfg!(target_os = "windows") {
            // WSL Graphics now lives under Connections on Windows, not as an
            // independent activity-bar tool.
            bottom_items.insert(2, (SidebarSection::Monitor, LucideIcon::Monitor));
        }
        let mut bar = div()
            .w(px(self.tokens.metrics.activity_bar_width))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .border_r_1()
            .border_color(rgb(theme.border));

        bar = bar.child(
            div()
                .relative()
                .flex_none()
                .w_full()
                .h(px(self.tokens.metrics.tabbar_height))
                .flex()
                .items_center()
                .justify_center()
                // Paint top chrome directly over the window background so it
                // matches the adjacent sidebar and workspace tab bars exactly.
                .bg(self.workspace_chrome_background(theme.bg))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .id("activity-sidebar-toggle")
                        .size(px(self.tokens.metrics.activity_icon_size))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.md))
                        .cursor_pointer()
                        // Keep the primary-sidebar toggle feedback aligned with its context-sidebar counterpart.
                        .hover(move |button| button.bg(rgb(theme.bg_hover)))
                        .child(Self::render_lucide_icon(
                            if self.sidebar_collapsed {
                                LucideIcon::PanelLeft
                            } else {
                                LucideIcon::PanelLeftClose
                            },
                            self.tokens.metrics.sidebar_collapse_icon_size,
                            rgb(theme.text_muted),
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
                ),
        );

        let mut primary_items = div()
            .id("activity-primary-items")
            .min_h_0()
            .flex_1()
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center();
        for (section, icon) in top_items_before_plugins {
            primary_items = primary_items.child(self.render_activity_icon(section, icon, cx));
        }
        primary_items = primary_items.child(self.render_activity_icon(
            SidebarSection::Extensions,
            LucideIcon::Puzzle,
            cx,
        ));
        let plugin_activity_items = self
            .native_plugin_runtime
            .registry
            .contributions()
            .runtime_activity_bar_items();
        // Tauri inserts plugin-provided sidebar panels as independent activity
        // buttons immediately after the built-in Plugin Manager tab button.
        for panel in self
            .native_plugin_runtime
            .registry
            .contributions()
            .runtime_sidebar_panels()
        {
            primary_items =
                primary_items.child(self.render_plugin_sidebar_activity_icon(panel, cx));
        }
        for item in plugin_activity_items
            .iter()
            .filter(|item| item.position == "top")
            .cloned()
        {
            primary_items = primary_items.child(self.render_plugin_activity_action_icon(item, cx));
        }
        for (section, icon) in top_items_after_plugins {
            primary_items = primary_items.child(self.render_activity_icon(section, icon, cx));
        }
        // App lock is a global action rather than a selectable panel. Keep it
        // directly below Host Tools as the final primary activity action.
        if self.settings_store.settings().sidebar_ui.show_app_lock_icon {
            primary_items = primary_items.child(self.render_app_lock_activity_icon(cx));
        }

        let mut bottom = div().relative().flex().flex_col().items_center().child(
            div()
                .w(px(self.tokens.metrics.divider_width))
                .h(px(self.tokens.metrics.divider_height))
                .bg(rgb(theme.divider)),
        );
        for item in plugin_activity_items
            .into_iter()
            .filter(|item| item.position == "bottom")
        {
            bottom = bottom.child(self.render_plugin_activity_action_icon(item, cx));
        }
        bottom = bottom.children(
            bottom_items
                .into_iter()
                .map(|(section, icon)| self.render_activity_icon(section, icon, cx)),
        );
        if let Some(popover) = self.render_detached_local_terminals_popover(cx) {
            bottom = bottom.child(popover);
        }

        bar.child(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .pb_2()
                .flex()
                .flex_col()
                .items_center()
                // The content column keeps the lighter sidebar tint without
                // placing another translucent layer beneath the top chrome.
                .bg(self.workspace_sidebar_background(theme.bg))
                .child(primary_items)
                .child(bottom),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_activity_icon(
        &self,
        section: SidebarSection,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = match section {
            SidebarSection::Terminal => false,
            SidebarSection::Runtime => self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::Runtime),
            SidebarSection::Activity => self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::ConnectionMonitor),
            SidebarSection::Network => {
                self.active_tab()
                    .is_some_and(|tab| tab.kind == TabKind::Runtime)
                    && self.active_connection_runtime_section == ConnectionRuntimeSection::Topology
            }
            SidebarSection::Files => self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::FileManager),
            SidebarSection::Monitor if cfg!(target_os = "macos") => self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::Launcher),
            SidebarSection::Notifications => self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::NotificationCenter),
            SidebarSection::Extensions => self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::PluginManager),
            SidebarSection::CloudSync => self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::CloudSync),
            SidebarSection::Settings => self
                .active_tab()
                .is_some_and(|tab| tab.kind == TabKind::Settings),
            SidebarSection::Assistant => self.ai_sidebar_visible(),
            SidebarSection::HostTools => {
                self.context_sidebar_visible()
                    && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools
            }
            _ => self.active_sidebar_section == section,
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
        } else if section == SidebarSection::Workspace {
            self.visible_local_terminal_session_count()
                .saturating_add(self.detached_local_terminals.len())
                .min(u32::MAX as usize) as u32
        } else {
            0
        };
        let badge_is_error = section == SidebarSection::Notifications
            && ((!self.notification_center.notifications.dnd_enabled
                && self.notification_center.notifications.unread_critical_count > 0)
                || (!self.notification_center.event_log.dnd_enabled
                    && self.notification_center.event_log.unread_errors > 0));
        let badge_color = if badge_is_error {
            theme.error
        } else if section == SidebarSection::Workspace && !self.detached_local_terminals.is_empty()
        {
            theme.warning
        } else {
            theme.accent
        };
        let badge_text_color = if badge_color == theme.accent {
            theme.accent_text
        } else {
            theme.bg
        };

        // Activity entries use the same static selected-card treatment as the
        // settings navigation while retaining the shared icon-button states.
        let button = oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                icon,
                self.tokens.metrics.activity_icon_glyph_size,
                rgb(if active { theme.accent } else { theme.text }),
            ),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: self.tokens.metrics.activity_icon_size,
                radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                has_background: active,
                background: active.then(|| self.settings_panel_background(theme.bg_panel)),
                border: active.then(|| rgb(theme.border)),
                hover_background: Some(rgb(theme.bg_hover)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                    self.tokens.metrics.activity_icon_size,
                )
            },
        );
        let button = if active {
            oxideterm_gpui_ui::theme_card_surface_shadow(button, &self.tokens)
        } else {
            button
        };

        button
            .id(("activity-icon", section as u64))
            .relative()
            .mb(px(self.tokens.metrics.activity_icon_gap))
            .when(badge_count > 0, |icon_el| {
                icon_el.child(
                    div()
                        .absolute()
                        // Tauri badges sit outside the `h-9 w-9` Button
                        // (`-top-1 -right-1`) so the active button chrome and
                        // numeric pill do not visually collide.
                        .right(px(-4.0))
                        .top(px(-4.0))
                        .min_w(px(14.0))
                        .h(px(14.0))
                        .px(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgb(badge_color))
                        .text_color(rgb(badge_text_color))
                        .text_size(px(9.0))
                        .child(if badge_count > 99 {
                            "99+".to_string()
                        } else {
                            badge_count.to_string()
                        }),
                )
            })
            .on_mouse_move(cx.listener({
                let tooltip = tooltip;
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
                        this.open_connection_runtime_tab(
                            ConnectionRuntimeSection::Overview,
                            window,
                            cx,
                        );
                    } else if section == SidebarSection::Runtime {
                        this.open_connection_runtime_tab(
                            ConnectionRuntimeSection::Overview,
                            window,
                            cx,
                        );
                    } else if section == SidebarSection::Activity {
                        this.open_connection_monitor_tab(window, cx);
                    } else if section == SidebarSection::Network {
                        this.open_topology_tab(window, cx);
                    } else if section == SidebarSection::Workspace {
                        // Tauri treats the bottom square as a local-terminal action.
                        if !this.detached_local_terminals.is_empty() {
                            this.detached_local_terminals_popover_open =
                                !this.detached_local_terminals_popover_open;
                            cx.notify();
                        } else {
                            let _ = this.create_local_terminal_tab(window, cx);
                        }
                    } else if section == SidebarSection::Files {
                        this.open_file_manager_tab(window, cx);
                    } else if section == SidebarSection::Monitor && cfg!(target_os = "macos") {
                        this.open_launcher_tab(window, cx);
                    } else if section == SidebarSection::Notifications {
                        this.open_notification_center_tab(window, cx);
                    } else if section == SidebarSection::Assistant {
                        let _ = this.toggle_ai_sidebar(cx);
                    } else if section == SidebarSection::HostTools {
                        let _ =
                            this.toggle_context_sidebar_panel(ContextSidebarPanel::HostTools, cx);
                    } else if section == SidebarSection::CloudSync {
                        this.open_cloud_sync_tab(window, cx);
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

    pub(in crate::workspace) fn activity_icon_tooltip(&self, section: SidebarSection) -> String {
        match section {
            SidebarSection::Sessions => self.i18n.t("sidebar.panels.sessions"),
            SidebarSection::Connections => self.i18n.t("sidebar.panels.open_session_manager"),
            SidebarSection::Sftp => self.i18n.t("sidebar.panels.sftp"),
            SidebarSection::Forwards => self.i18n.t("forwards.table.title"),
            SidebarSection::Terminal => self.i18n.t("sidebar.panels.runtime_overview"),
            SidebarSection::Runtime => self.i18n.t("sidebar.panels.runtime"),
            SidebarSection::Activity => self.i18n.t("sidebar.panels.connection_monitor"),
            SidebarSection::Network => self.i18n.t("sidebar.panels.connection_matrix"),
            SidebarSection::Extensions => self.i18n.t("sidebar.panels.plugins"),
            SidebarSection::CloudSync => self.i18n.t("plugin.cloud_sync.panel_title"),
            SidebarSection::Assistant => self.i18n.t("sidebar.panels.ai"),
            SidebarSection::HostTools => self.i18n.t("sidebar.panels.host_tools"),
            SidebarSection::Automation => self.i18n.t("sidebar.panels.activity"),
            SidebarSection::Workspace => self.i18n.t("sidebar.actions.new_local_terminal"),
            SidebarSection::Files => self.i18n.t("sidebar.panels.files"),
            SidebarSection::Monitor if cfg!(target_os = "macos") => {
                self.i18n.t("launcher.tabTitle")
            }
            SidebarSection::Monitor => self.i18n.t("sidebar.panels.connection_monitor"),
            SidebarSection::Notifications => self.i18n.t("sidebar.panels.notifications"),
            SidebarSection::Settings => self.i18n.t("sidebar.tooltips.settings"),
        }
    }

    pub(in crate::workspace) fn render_plugin_sidebar_activity_icon(
        &self,
        panel: plugin_host::NativePluginRuntimeSidebarPanelContribution,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selection = plugin_ui::NativePluginSidebarPanelSelection {
            plugin_id: panel.plugin_id.clone(),
            panel_id: panel.panel_id.clone(),
        };
        let active = self.active_sidebar_section == SidebarSection::Extensions
            && self
                .native_plugin_manager
                .active_sidebar_panel
                .as_ref()
                .is_some_and(|active_panel| active_panel == &selection);
        let tooltip = panel.title.clone();
        let tooltip_id = format!("activity-plugin-{}-{}", panel.plugin_id, panel.panel_id);
        let tooltip_id_for_move = tooltip_id.clone();
        let icon = LucideIcon::from_plugin_name(&panel.icon);

        let button = oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                icon,
                self.tokens.metrics.activity_icon_glyph_size,
                rgb(if active { theme.accent } else { theme.text }),
            ),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: self.tokens.metrics.activity_icon_size,
                radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                has_background: active,
                background: active.then(|| self.settings_panel_background(theme.bg_panel)),
                border: active.then(|| rgb(theme.border)),
                hover_background: Some(rgb(theme.bg_hover)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                    self.tokens.metrics.activity_icon_size,
                )
            },
        );
        let button = if active {
            oxideterm_gpui_ui::theme_card_surface_shadow(button, &self.tokens)
        } else {
            button
        };

        button
            .id((
                "activity-plugin-icon",
                native_plugin_sidebar_activity_id(&panel),
            ))
            .relative()
            .mb(px(self.tokens.metrics.activity_icon_gap))
            .on_mouse_move(cx.listener({
                let tooltip = tooltip;
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
                cx.listener(move |this, _event, _window, cx| {
                    // Mirrors Tauri's `sidebarActiveSection = "plugin:<id>:<panel>"`
                    // path: choosing a plugin panel switches only the sidebar
                    // content, while Plugin Manager remains a separate tab.
                    this.native_plugin_manager.active_sidebar_panel = Some(selection.clone());
                    this.active_sidebar_section = SidebarSection::Extensions;
                    this.active_surface = ActiveSurface::Terminal;
                    this.persist_sidebar_settings();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_plugin_activity_action_icon(
        &self,
        item: plugin_host::NativePluginRuntimeActivityBarItemContribution,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let tooltip_id = format!("activity-plugin-action-{}-{}", item.plugin_id, item.item_id);
        let tooltip_id_for_move = tooltip_id.clone();
        let tooltip = item.title.clone();
        let plugin_id = item.plugin_id.clone();
        let command = item.command.clone();
        let icon = LucideIcon::from_plugin_name(&item.icon);
        let button = oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                icon,
                self.tokens.metrics.activity_icon_glyph_size,
                rgb(theme.text),
            ),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: self.tokens.metrics.activity_icon_size,
                radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                hover_background: Some(rgb(theme.bg_hover)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                    self.tokens.metrics.activity_icon_size,
                )
            },
        );

        button
            .id((
                "activity-plugin-action",
                native_plugin_activity_bar_item_id(&item),
            ))
            .relative()
            .mb(px(self.tokens.metrics.activity_icon_gap))
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        tooltip_id_for_move.clone(),
                        tooltip.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                }),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip(&tooltip_id, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    // Action items invoke only the manifest-declared command
                    // associated with their host-owned runtime registration.
                    this.dispatch_native_plugin_command(plugin_id.clone(), command.clone(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_lucide_icon(
        icon: LucideIcon,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    pub(in crate::workspace) fn render_animated_chevron(
        &self,
        id: impl Into<gpui::ElementId>,
        expanded: bool,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        // A single right-facing asset keeps state changes continuous instead
        // of swapping two unrelated SVG trees between renders.
        oxideterm_gpui_ui::motion::animated_chevron(
            &self.tokens,
            id,
            svg()
                .path(LucideIcon::ChevronRight.path())
                .size(px(size))
                .text_color(color),
            expanded,
        )
    }

    pub(in crate::workspace) fn render_loading_icon(
        &self,
        id: impl Into<gpui::ElementId>,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        // Spinner frames are requested only while callers render a genuine
        // loading state; static status icons continue using render_lucide_icon.
        oxideterm_gpui_ui::motion::animated_spinner(
            &self.tokens,
            id,
            svg()
                .path(LucideIcon::LoaderCircle.path())
                .size(px(size))
                .text_color(color),
        )
    }
}

pub(in crate::workspace) fn native_plugin_sidebar_activity_id(
    panel: &plugin_host::NativePluginRuntimeSidebarPanelContribution,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    panel.registration_id.hash(&mut hasher);
    panel.plugin_id.hash(&mut hasher);
    panel.panel_id.hash(&mut hasher);
    hasher.finish()
}

pub(in crate::workspace) fn native_plugin_activity_bar_item_id(
    item: &plugin_host::NativePluginRuntimeActivityBarItemContribution,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    item.registration_id.hash(&mut hasher);
    item.plugin_id.hash(&mut hasher);
    item.item_id.hash(&mut hasher);
    hasher.finish()
}
