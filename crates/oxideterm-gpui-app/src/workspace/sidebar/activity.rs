use super::*;
use gpui::StatefulInteractiveElement;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_activity_bar(
        &mut self,
        env: &ShellEnvironment,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
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
        for (index, (section, icon)) in SidebarSection::PRIMARY_ITEMS.into_iter().enumerate() {
            if index == 3 {
                primary_items = primary_items.child(
                    div()
                        .flex_none()
                        .w(px(self.tokens.metrics.divider_width))
                        .h(px(self.tokens.metrics.divider_height))
                        .mt(px(2.0))
                        .mb(px(self.tokens.metrics.activity_icon_gap + 2.0))
                        .bg(self.workspace_chrome_divider()),
                );
            }
            primary_items = primary_items.child(self.render_activity_icon(section, icon, cx));
        }
        let plugin_activity_items = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .runtime_activity_bar_items();
        let plugin_sidebar_panels = self
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .runtime_sidebar_panels();
        // Keep built-in entries stationary as plugins are enabled or disabled.
        if plugin_sidebar_panels
            .iter()
            .any(|panel| panel.position == "top")
            || plugin_activity_items
                .iter()
                .any(|item| item.position == "top")
        {
            primary_items = primary_items.child(
                div()
                    .flex_none()
                    .w(px(self.tokens.metrics.divider_width))
                    .h(px(self.tokens.metrics.divider_height))
                    .mt(px(2.0))
                    .mb(px(self.tokens.metrics.activity_icon_gap + 2.0))
                    .bg(self.workspace_chrome_divider()),
            );
        }
        for panel in plugin_sidebar_panels
            .iter()
            .filter(|panel| panel.position == "top")
            .cloned()
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

        let mut bottom = div()
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w(px(self.tokens.metrics.divider_width))
                    .h(px(self.tokens.metrics.divider_height))
                    .mb(px(self.tokens.metrics.activity_icon_gap + 2.0))
                    .bg(self.workspace_chrome_divider()),
            );
        for panel in plugin_sidebar_panels
            .into_iter()
            .filter(|panel| panel.position == "bottom")
        {
            bottom = bottom.child(self.render_plugin_sidebar_activity_icon(panel, cx));
        }
        for item in plugin_activity_items
            .into_iter()
            .filter(|item| item.position == "bottom")
        {
            bottom = bottom.child(self.render_plugin_activity_action_icon(item, cx));
        }
        bottom = bottom.children(
            SidebarSection::FOOTER_ITEMS
                .into_iter()
                .map(|(section, icon)| self.render_activity_icon(section, icon, cx)),
        );
        if let Some(popover) = self.render_detached_local_terminals_popover(cx) {
            bottom = bottom.child(popover);
        }

        let body = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .pb_2()
            .flex()
            .flex_col()
            .items_center()
            // The content column keeps the lighter sidebar tint without
            // placing another translucent layer beneath the top chrome.
            .bg(self.workspace_sidebar_background(theme.shell_bg()))
            .child(primary_items)
            .child(bottom);
        oxideterm_gpui_shell::activity_rail(
            RailSide::Left,
            !self.rails.left.collapsed(),
            body,
            env,
            cx,
        )
    }

    pub(in crate::workspace) fn render_context_activity_bar(
        &self,
        window: &Window,
        env: &ShellEnvironment,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let panel_visible = self.context_sidebar_visible();
        let body = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(self.workspace_sidebar_background(theme.shell_bg()))
            .child(self.render_context_tool_buttons(window, cx))
            .when(
                self.settings_store.settings().sidebar_ui.show_app_lock_icon,
                |body| {
                    body.child(
                        div()
                            .flex_none()
                            .pt(px(self.tokens.spacing.two))
                            .pb(px(self.tokens.spacing.two))
                            .mb(px(self.tokens.metrics.activity_icon_gap))
                            .child(self.render_app_lock_button(
                                self.tokens.metrics.activity_icon_size,
                                cx,
                            )),
                    )
                },
            );
        oxideterm_gpui_shell::activity_rail(RailSide::Right, panel_visible, body, env, cx)
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
                .active_content_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Runtime),
            SidebarSection::Network => {
                self.active_content_tab(cx)
                    .is_some_and(|tab| tab.kind == TabKind::Runtime)
                    && self.host_tools.read(cx).active_runtime_section
                        == ConnectionRuntimeSection::Topology
            }
            SidebarSection::Files => self
                .active_content_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::FileManager),
            SidebarSection::Notifications => self
                .active_content_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::NotificationCenter),
            SidebarSection::Extensions => self
                .active_content_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::PluginManager),
            SidebarSection::CloudSync => self
                .active_content_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::CloudSync),
            SidebarSection::Knowledge => self
                .active_content_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Knowledge),
            SidebarSection::Settings => self
                .active_content_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Settings),
            SidebarSection::Assistant => self.ai_sidebar_visible(),
            SidebarSection::HostTools => {
                self.context_sidebar_visible()
                    && self.rails.context_panel == ContextSidebarPanel::HostTools
            }
            _ => self.rails.section == section,
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
            notification_count
        } else if section == SidebarSection::Workspace {
            self.visible_local_terminal_session_count(cx)
                .saturating_add(self.detached_local_terminals.len())
                .min(u32::MAX as usize) as u32
        } else {
            0
        };
        let badge_is_error = section == SidebarSection::Notifications
            && !self.notification_center.notifications.dnd_enabled
            && self.notification_center.notifications.unread_critical_count > 0;
        let badge_color = if badge_is_error {
            theme.error
        } else if section == SidebarSection::Workspace && !self.detached_local_terminals.is_empty()
        {
            theme.warning
        } else {
            theme.accent
        };
        let ai_disabled =
            section == SidebarSection::Assistant && !self.settings_store.settings().ai.enabled;
        oxideterm_gpui_shell::chrome::activity_button(
            &self.tokens,
            icon,
            active,
            ai_disabled,
            self.settings_panel_background(theme.shell_active_bg()),
            badge_count,
            badge_color,
        )
        .id(("activity-icon", section as u64))
        .on_mouse_move(cx.listener({
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
                this.request(ShellRequest::Select(section), window, cx);
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn activity_icon_tooltip(&self, section: SidebarSection) -> String {
        match section {
            SidebarSection::Sessions => self.i18n.t("sidebar.panels.sessions"),
            SidebarSection::Connections => self.i18n.t("sidebar.panels.session_manager"),
            SidebarSection::Forwards => self.i18n.t("forwards.table.title"),
            SidebarSection::Terminal => self.i18n.t("sidebar.panels.runtime_overview"),
            SidebarSection::Runtime => self.i18n.t("sidebar.panels.runtime"),
            SidebarSection::Network => self.i18n.t("sidebar.panels.connection_matrix"),
            SidebarSection::Extensions => self.i18n.t("sidebar.panels.plugins"),
            SidebarSection::CloudSync => self.i18n.t("plugin.cloud_sync.panel_title"),
            SidebarSection::Knowledge => self.i18n.t("sidebar.panels.knowledge"),
            SidebarSection::Assistant => {
                if self.settings_store.settings().ai.enabled {
                    self.i18n.t("sidebar.panels.ai")
                } else {
                    self.i18n.t("sidebar.tooltips.ai_disabled")
                }
            }
            SidebarSection::HostTools => self.i18n.t("sidebar.panels.host_tools"),
            SidebarSection::Automation => self.i18n.t("sidebar.panels.activity"),
            SidebarSection::Workspace => self.i18n.t("sidebar.actions.new_local_terminal"),
            SidebarSection::Files => self.i18n.t("sidebar.panels.files"),
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
        let active = self.rails.section == SidebarSection::Extensions
            && self
                .plugin_manager_state(cx)
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
                background: active.then(|| self.settings_panel_background(theme.shell_active_bg())),
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
                    let requested_panel_is_visible = !this.rails.left.collapsed()
                        && this.rails.section == SidebarSection::Extensions
                        && this
                            .plugin_manager_state(cx)
                            .active_sidebar_panel
                            .as_ref()
                            .is_some_and(|active_panel| active_panel == &selection);
                    if requested_panel_is_visible {
                        this.toggle_sidebar(cx);
                        cx.stop_propagation();
                        return;
                    }
                    // Mirrors Tauri's `sidebarActiveSection = "plugin:<id>:<panel>"`
                    // path: choosing a plugin panel switches only the sidebar
                    // content, while Plugin Manager remains a separate tab.
                    this.plugin_entity.update(cx, |plugins, _cx| {
                        plugins.select_sidebar_panel(selection.clone());
                    });
                    this.active_surface = ActiveSurface::Terminal;
                    this.set_sidebar_section(SidebarSection::Extensions, cx);
                    cx.stop_propagation();
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

impl ShellHost for WorkspaceApp {
    fn request(&mut self, request: ShellRequest, window: &mut Window, cx: &mut Context<Self>) {
        match request {
            ShellRequest::ExitZen => self.toggle_zen_mode(cx),
            ShellRequest::HoverZenStrip(hovered) => self.set_zen_exit_hot_zone_hovered(hovered, cx),
            ShellRequest::HoverZenPill(hovered) => self.set_zen_exit_pill_hovered(hovered, cx),
            ShellRequest::DragResize(x) => self.drag_rail_resize(x, window, cx),
            ShellRequest::EndResize => self.finish_rail_resize(cx),
            ShellRequest::Select(section) => {
                if section == SidebarSection::Settings {
                    self.open_settings(window, cx);
                } else if matches!(section, SidebarSection::Terminal | SidebarSection::Runtime) {
                    self.open_connection_runtime_tab(
                        ConnectionRuntimeSection::Overview,
                        window,
                        cx,
                    );
                } else if section == SidebarSection::Network {
                    self.open_topology_tab(window, cx);
                } else if section == SidebarSection::Workspace {
                    // Tauri treats the bottom square as a local-terminal action.
                    if !self.detached_local_terminals.is_empty() {
                        self.detached_local_terminals_popover_open =
                            !self.detached_local_terminals_popover_open;
                        cx.notify();
                    } else {
                        let _ = self.create_local_terminal_tab(window, cx);
                    }
                } else if section == SidebarSection::Files {
                    self.open_file_manager_tab(window, cx);
                } else if section == SidebarSection::Notifications {
                    self.open_notification_center_tab(window, cx);
                } else if section == SidebarSection::Assistant {
                    let _ = self.toggle_ai_sidebar(cx);
                } else if section == SidebarSection::Knowledge {
                    self.open_knowledge_workspace_tab(window, cx);
                } else if section == SidebarSection::HostTools {
                    let _ = self.toggle_context_sidebar_panel(ContextSidebarPanel::HostTools, cx);
                } else if section == SidebarSection::CloudSync {
                    self.open_cloud_sync_tab(window, cx);
                } else if section == SidebarSection::Extensions {
                    self.open_plugin_manager_tab(window, cx);
                } else if section == SidebarSection::Connections {
                    // While the Session Manager already lives in a tab the
                    // icon is inert; the tab's context menu owns switching
                    // back to the sidebar form.
                    if !self
                        .tabs(cx)
                        .iter()
                        .any(|tab| tab.kind == TabKind::SessionManager)
                    {
                        self.active_surface = ActiveSurface::Terminal;
                        self.toggle_sidebar_section(section, cx);
                    }
                } else {
                    self.active_surface = ActiveSurface::Terminal;
                    self.toggle_sidebar_section(section, cx);
                }
            }
            ShellRequest::Toggle(RailSide::Left) => self.toggle_sidebar(cx),
            ShellRequest::Toggle(RailSide::Right) => {
                self.clear_workspace_tooltip(RailSide::Right.toggle_id(), cx);
                self.toggle_context_sidebar_panel(self.rails.context_panel, cx);
            }
            ShellRequest::BeginResize(side, x) => self.start_rail_resize(side, x, window, cx),
            ShellRequest::ShowToggleHint(side, label, position) => {
                self.queue_workspace_tooltip(
                    side.toggle_id(),
                    label,
                    f32::from(position.x) + 12.0,
                    f32::from(position.y) + 16.0,
                    cx,
                );
            }
            ShellRequest::HideToggleHint(side) => {
                self.clear_workspace_tooltip(side.toggle_id(), cx)
            }
        }
    }
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
