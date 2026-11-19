use super::*;

pub(in crate::workspace) use oxideterm_gpui_ui::separator::{
    SIDEBAR_RESIZE_HOTZONE_WIDTH, sidebar_resize_hotzone_chrome,
};
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

pub(in crate::workspace) fn sidebar_resize_hotzone_origin(seam: f32) -> f32 {
    seam - SIDEBAR_RESIZE_HOTZONE_WIDTH
}

fn workspace_sidebar_resize_cursor() -> impl IntoElement {
    // The handle is above terminal cursors, but its override must end at the
    // hitbox rather than depend on a mouse-leave arriving during capture.
    gpui::canvas(
        |bounds, window, _| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
        |_, hitbox, window, _| {
            window.set_cursor_style_override(gpui::CursorStyle::ResizeColumn, &hitbox);
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_animated_sidebar_region(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded_width = self.sidebar_panel_width();
        self.sidebar_motion.retarget(if self.sidebar_collapsed {
            0.0
        } else {
            expanded_width
        });
        let content = div()
            .flex_none()
            .w(px(expanded_width))
            .h_full()
            .child(self.render_sidebar_region(window, cx));
        self.sidebar_motion.animate_drawer(
            &self.tokens,
            "workspace-left-sidebar-motion",
            expanded_width,
            oxideterm_gpui_ui::motion::DrawerEdge::Left,
            content.into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_animated_context_sidebar_frame(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded_width = self.ai.state.read(cx).chat_ui().sidebar_width;
        self.context_sidebar_motion
            .retarget(if self.context_sidebar_visible() {
                expanded_width
            } else {
                0.0
            });
        let content = div()
            .flex_none()
            .w(px(expanded_width))
            .h_full()
            .child(self.render_context_right_sidebar_frame(cx));
        self.context_sidebar_motion.animate_drawer(
            &self.tokens,
            "workspace-right-sidebar-motion",
            expanded_width,
            oxideterm_gpui_ui::motion::DrawerEdge::Right,
            content.into_any_element(),
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
        context_sidebar_frame_chrome(self.ai.state.read(cx).chat_ui().sidebar_width)
            .child(self.render_context_right_sidebar_region(cx))
            .child(oxideterm_gpui_ui::surface::panel_edge_highlight(
                &self.tokens,
                self.workspace_sidebar_background(self.tokens.ui.shell_bg())
                    .a,
                0.0,
            ))
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
            ContextSidebarPanel::HostTools => {
                let tool = self.host_tools.read(cx).active_tool();
                (
                    tool.label_key(),
                    "host-tools",
                    super::tools::context_sidebar_tool_icon(tool),
                )
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
                            .px_3()
                            // Match the center tab bar's chrome opacity instead
                            // of inheriting the more transparent sidebar body.
                            .bg(self.workspace_chrome_background(theme.shell_bg()))
                            // The context-sidebar titlebar is fixed chrome.
                            // Give it its own hitbox so wheel/drag events
                            // cannot fall through to a scrollable tool body.
                            .occlude()
                            .border_b_1()
                            .border_color(self.workspace_chrome_divider())
                            .child(self.render_context_sidebar_panel_title(
                                title_key, title_role, icon, cx,
                            ))
                            .when(
                                self.active_context_sidebar_panel == ContextSidebarPanel::HostTools,
                                |header| header.child(self.render_host_tool_monitoring_toggle(cx)),
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
                            .bg(self.workspace_sidebar_background(theme.shell_bg()))
                            .child({
                                let content = match self.active_context_sidebar_panel {
                                    ContextSidebarPanel::Assistant => {
                                        self.ai.transcript.clone().into_any_element()
                                    }
                                    ContextSidebarPanel::HostTools => {
                                        self.render_host_tools_context_panel(cx)
                                    }
                                };
                                // Content refreshes and virtual-list remounts are not panel
                                // entrances. The frame owns explicit open/close motion.
                                div().size_full().child(content)
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
        let activity_width = self.tokens.metrics.activity_bar_width;
        let hotzone = sidebar_resize_hotzone_chrome(
            "workspace-left-sidebar-resize-hotzone",
            if self.sidebar_resizing {
                rgb(theme.accent)
            } else {
                self.workspace_divider_color(theme.border, 0x40)
            },
            true,
        )
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
        .child(workspace_sidebar_resize_cursor());
        self.sidebar_motion.animate(
            &self.tokens,
            "left-sidebar-edge-motion",
            hotzone,
            move |edge, width| edge.left(px(sidebar_resize_hotzone_origin(activity_width + width))),
        )
    }

    pub(in crate::workspace) fn render_context_right_sidebar_resize_hotzone(
        &mut self,
        top_offset: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let activity_width = self.tokens.metrics.activity_bar_width;
        let hotzone = sidebar_resize_hotzone_chrome(
            "context-right-sidebar-resize-hotzone",
            if self.ai.state.read(cx).chat_ui().sidebar_resizing {
                rgb(theme.accent)
            } else {
                self.workspace_chrome_divider()
            },
            false,
        )
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
        .child(workspace_sidebar_resize_cursor());
        self.context_sidebar_motion.animate(
            &self.tokens,
            "right-sidebar-edge-motion",
            hotzone,
            move |edge, width| {
                edge.right(px(sidebar_resize_hotzone_origin(activity_width + width)))
            },
        )
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
                .gap(px(self.tokens.spacing.two))
                .child(Self::render_lucide_icon(
                    icon,
                    self.tokens.metrics.tab_icon_size,
                    rgb(theme.text_secondary),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(self.tokens.metrics.titlebar_label_font_size))
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
        let panel_section = self.effective_sidebar_panel_section();
        let transition_gen = self
            .sidebar_section_transition
            .filter(|(section, _)| *section == panel_section)
            .map(|(_, generation)| generation);
        let sidebar_content = self.render_sidebar_content(window, cx);
        let body_content = {
            let body = div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
                .child(sidebar_content);
            if let Some(transition_gen) = transition_gen {
                oxideterm_gpui_ui::motion::slide_fade_in_x(
                    &self.tokens,
                    ("sidebar-section-body", transition_gen as usize),
                    body,
                    -10.0,
                    oxideterm_gpui_ui::motion::MotionDuration::Micro,
                )
            } else {
                body.into_any_element()
            }
        };
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            // The overlaid resize hotzone owns the seam without reserving an
            // untinted layout border between the sidebar and main content.
            .child(self.render_sidebar_header(cx))
            .when(
                panel_section == SidebarSection::Sessions
                    && self
                        .disclosure_motions
                        .retained("session-search", self.session_search_open),
                |panel| {
                    panel.child(
                        self.disclosure_motions.render(
                            "session-search",
                            &self.tokens,
                            div()
                                .flex_none()
                                .overflow_hidden()
                                .child(self.render_session_search_input(cx)),
                            Some(36.0),
                        ),
                    )
                },
            )
            .when(
                panel_section == SidebarSection::Connections
                    && self.disclosure_motions.retained(
                        "session-manager-search",
                        self.connections
                            .session_manager
                            .read(cx)
                            .sidebar_search_open,
                    ),
                |panel| {
                    panel.child(
                        self.disclosure_motions.render(
                            "session-manager-search",
                            &self.tokens,
                            div()
                                .flex_none()
                                .overflow_hidden()
                                .child(self.render_session_manager_search_input(cx)),
                            Some(36.0),
                        ),
                    )
                },
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Match the activity bar and context sidebar base surface
                    // so the full-height navigation body does not read as a card.
                    .bg(self.workspace_sidebar_background(theme.shell_bg()))
                    .child(body_content),
            )
            .when(panel_section == SidebarSection::Sessions, |sidebar| {
                sidebar.child(self.render_active_sessions_footer(cx))
            })
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
            SidebarSection::Connections => "sidebar.panels.session_manager",
            _ => "sidebar.panels.sessions",
        };
        let title = plugin_panel_title.unwrap_or_else(|| self.i18n.t(title_key));
        let transition_gen = self
            .sidebar_section_transition
            .filter(|(section, _)| *section == panel_section)
            .map(|(_, generation)| generation);
        let title_inner = div()
            .flex()
            .items_center()
            .truncate()
            .text_size(px(self.tokens.metrics.sidebar_title_font_size))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(theme.text_secondary))
            // A title inside the native drag region must never start
            // read-only text selection or retain pointer ownership.
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "sidebar-header-title",
                title_key,
                title,
                theme.text_muted,
                cx,
            ));
        let title_element = if let Some(transition_gen) = transition_gen {
            oxideterm_gpui_ui::motion::fade_in(
                &self.tokens,
                ("sidebar-header-title", transition_gen as usize),
                title_inner,
                oxideterm_gpui_ui::motion::MotionDuration::Micro,
            )
        } else {
            title_inner.into_any_element()
        };
        let mut header = div()
            // Align sidebar titles with the neighboring workspace tab bar.
            .h(px(self.tokens.metrics.tabbar_height))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            // Use the same image-background opacity as the adjacent tab bar
            // without stacking it over the sidebar body's translucent tint.
            .bg(self.workspace_chrome_background(theme.shell_bg()))
            .border_b_1()
            .border_color(self.workspace_chrome_divider())
            .px_3()
            .child(self.render_window_drag_content_region(
                "sidebar-header-title-drag-region",
                title_element,
                cx,
            ));
        if panel_section == SidebarSection::Sessions {
            let view_icon = match self.active_session_sidebar_view_mode {
                ActiveSessionSidebarViewMode::Tree => LucideIcon::Folder,
                ActiveSessionSidebarViewMode::Host => LucideIcon::Server,
            };
            let actions = div()
                .flex()
                .items_center()
                .child(self.render_session_search_button(cx))
                .child(self.render_session_sort_button(cx))
                .child(self.render_sidebar_action(
                    view_icon,
                    SidebarActionKind::ToggleSessionView,
                    cx,
                ))
                .child(self.render_sidebar_action(
                    LucideIcon::Plus,
                    SidebarActionKind::NewConnection,
                    cx,
                ));
            header = header.child(if let Some(transition_gen) = transition_gen {
                oxideterm_gpui_ui::motion::fade_in(
                    &self.tokens,
                    ("sidebar-header-actions", transition_gen as usize),
                    actions,
                    oxideterm_gpui_ui::motion::MotionDuration::Micro,
                )
            } else {
                actions.into_any_element()
            });
        }
        if panel_section == SidebarSection::Connections {
            let workspace = cx.entity();
            // Match the tab form's sort trigger, which reflects the current
            // sort direction instead of a static glyph.
            let sort_direction = self.connections.session_manager.read(cx).sort_direction;
            let sort_action = self.render_sidebar_action(
                sort_direction.icon(),
                SidebarActionKind::ToggleSessionSort,
                cx,
            );
            let actions = div()
                .flex()
                .items_center()
                .child(self.connections.session_manager_sidebar_search_button())
                .child(
                    div()
                        .id("sidebar-session-manager-sort")
                        .child(select_anchor_probe(
                            SelectAnchorId::SessionManagerSidebarSort,
                            sort_action,
                            move |anchor, _window, cx| {
                                let _ = workspace.update(cx, |this, cx| {
                                    this.update_select_anchor(anchor, cx);
                                });
                            },
                        )),
                )
                .child(self.render_sidebar_action(
                    LucideIcon::ExternalLink,
                    SidebarActionKind::OpenSessionManagerTab,
                    cx,
                ))
                .child(self.render_sidebar_action(
                    LucideIcon::Plus,
                    SidebarActionKind::NewConnection,
                    cx,
                ));
            header = header.child(if let Some(transition_gen) = transition_gen {
                oxideterm_gpui_ui::motion::fade_in(
                    &self.tokens,
                    ("sidebar-header-actions", transition_gen as usize),
                    actions,
                    oxideterm_gpui_ui::motion::MotionDuration::Micro,
                )
            } else {
                actions.into_any_element()
            });
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
                ActiveSessionSidebarViewMode::Tree => self.i18n.t("sidebar.tooltips.switch_host"),
                ActiveSessionSidebarViewMode::Host => self.i18n.t("sidebar.tooltips.switch_tree"),
            },
            SidebarActionKind::NewConnection => self.i18n.t("sidebar.tooltips.new_connection"),
            SidebarActionKind::OpenSessionManagerTab => self.i18n.t("sidebar.tooltips.open_in_tab"),
            SidebarActionKind::ToggleSessionSort => self.i18n.t("sidebar.tooltips.sort"),
        };

        let host_view_active = action == SidebarActionKind::ToggleSessionView
            && self.active_session_sidebar_view_mode == ActiveSessionSidebarViewMode::Host;
        let sort_active = action == SidebarActionKind::ToggleSessionSort
            && self.connections.session_manager.read(cx).sidebar_sort_open;
        let active = host_view_active || sort_active;

        div()
            .ml_1()
            .child(self.workspace_tooltip_icon_button(
                icon,
                self.tokens.metrics.sidebar_action_icon_size,
                rgb(if active { theme.text } else { theme.text_muted }),
                IconButtonOptions {
                    has_background: active,
                    background: active.then_some(rgb(theme.bg_active)),
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
                        SidebarActionKind::OpenSessionManagerTab => {
                            // Jump to the tab form: move the sidebar body off the
                            // session manager so the two views stay mutually exclusive.
                            this.set_sidebar_section(SidebarSection::Sessions, cx);
                            this.open_session_manager_tab(window, cx);
                        }
                        SidebarActionKind::ToggleSessionSort => {
                            this.connections.session_manager.update(cx, |manager, cx| {
                                manager.set_sidebar_sort_open(!manager.sidebar_sort_open, cx);
                            });
                            cx.notify();
                        }
                    }
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_session_manager_search_input(
        &self,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        self.connections.session_manager_sidebar_search()
    }

    pub(in crate::workspace) fn render_sidebar_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel_section = self.effective_sidebar_panel_section();
        if matches!(
            panel_section,
            SidebarSection::Sessions | SidebarSection::Connections
        ) {
            let navigator = if panel_section == SidebarSection::Sessions {
                self.render_active_sessions_sidebar_content(cx)
            } else {
                self.render_session_manager_sidebar_surface(window, cx)
            };
            if self.embedded_sftp_node_id.is_none() {
                return navigator;
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
            // Embedded SFTP is a subordinate surface of the visible navigator
            // panel (Active Sessions or the Session Manager sidebar form). The
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
                        .child(navigator),
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

        let audit_entry = entry.clone();
        let show_audit = entry.kind == WorkspaceNotificationKind::Security
            || matches!(
                entry.severity,
                WorkspaceNotificationSeverity::Error | WorkspaceNotificationSeverity::Critical
            );

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
                    .when(show_audit, |row| {
                        row.child(self.workspace_toolbar_action_button(
                            self.i18n.t("event_log.actions.audit_view"),
                            None,
                            oxideterm_gpui_ui::button::ToolbarButtonOptions::default(),
                            cx.listener(move |this, _, window, cx| {
                                this.open_notification_audit(&audit_entry, window, cx);
                                cx.stop_propagation();
                            }),
                        ))
                    })
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

    pub(in crate::workspace) fn render_sidebar_empty_state(
        &self,
        icon: LucideIcon,
        title_id: &'static str,
        title: String,
        subtitle: Option<(&'static str, String)>,
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
                        icon,
                        self.tokens.metrics.empty_sidebar_icon_size,
                        rgba((theme.text_muted << 8) | 0x4d),
                    )))
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                            .text_color(rgb(theme.text_muted))
                            // Empty-state guidance is interface chrome, not
                            // user data that should start a text selection.
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                title_id,
                                (),
                                title,
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .when_some(subtitle, |empty, (subtitle_id, subtitle)| {
                        empty.child(
                            div()
                                .mt_1()
                                .w_full()
                                .text_center()
                                .text_size(px(self.tokens.metrics.empty_sidebar_subtitle_font_size))
                                .text_color(rgb(theme.text_muted))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::NonSelectable,
                                    subtitle_id,
                                    (),
                                    subtitle,
                                    theme.text_muted,
                                    cx,
                                )),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_empty_sessions_sidebar_content(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_sidebar_empty_state(
            LucideIcon::Server,
            "sessions-sidebar-empty-title",
            self.i18n.t("sessions.tree.no_sessions"),
            Some((
                "sessions-sidebar-empty-subtitle",
                self.i18n.t("sessions.tree.click_to_add"),
            )),
            cx,
        )
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
    OpenSessionManagerTab,
    ToggleSessionSort,
}

#[cfg(test)]
mod sidebar_resize_region_tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        Context, CursorStyle, IntoElement, Modifiers, MouseButton, ParentElement, Point, Render,
        Styled, TestAppContext, Window, canvas, div, px, size,
    };

    struct SidebarSurfaceView(Entity<WorkspaceApp>);

    impl Render for SidebarSurfaceView {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.update(cx, |workspace, cx| {
                div()
                    .size_full()
                    .flex()
                    .child(workspace.render_activity_bar(cx))
                    .child(workspace.render_sidebar_region(window, cx))
            })
        }
    }

    fn isolated_sidebar_settings(cx: &mut TestAppContext) -> Option<SettingsStore> {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_SIDEBAR_SEAM_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Portable storage isolates the real workspace from user settings and plugins.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            fs::hard_link(&executable, &child).unwrap();
            fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return None;
        };
        let settings_path = default_settings_path();
        assert!(settings_path.starts_with(PathBuf::from(fixture_dir)));
        let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        cx.executor().allow_parking();
        Some(settings)
    }

    #[gpui::test]
    fn sidebar_tints_reach_the_seams_without_exposing_wallpaper(cx: &mut TestAppContext) {
        let Some(mut settings) = isolated_sidebar_settings(cx) else {
            return;
        };
        settings.save().unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            SidebarSurfaceView(cx.new(|cx| {
                let mut workspace = WorkspaceApp::new(window, cx, None, None).unwrap();
                workspace.active_background.enabled = true;
                workspace.active_background.image = Some("seam-fixture.png".into());
                workspace.active_background.scope = oxideterm_settings::BackgroundScope::Window;
                workspace.render_policy.allow_background_images = true;
                // Dark shell chrome uses the panel surface, distinct from the canvas.
                workspace.tokens.ui.bg = 0x050607;
                workspace.tokens.ui.bg_panel = 0x102030;
                workspace.tokens.metrics.sidebar_vibrancy_alpha = 0.5;
                workspace.sidebar_width = 300.0;
                workspace
            }))
        });
        cx.simulate_resize(size(px(800.0), px(600.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let activity_width = view.read_with(cx, |view, cx| {
            view.0.read(cx).tokens.metrics.activity_bar_width
        });
        cx.update(|window, _| {
            let tint: gpui::Hsla = rgba(0x10203080).into();
            let scale = window.scale_factor();
            let quads = window.painted_quads();
            for (left, right) in [(0.0, activity_width), (activity_width, 300.0)] {
                let body = quads
                    .iter()
                    .filter(|quad| {
                        quad.background.as_solid() == Some(tint)
                            && (quad.bounds.left().0 / scale - left).abs() < 0.01
                    })
                    .max_by(|a, b| {
                        a.bounds
                            .size
                            .height
                            .partial_cmp(&b.bounds.size.height)
                            .unwrap()
                    })
                    .expect("sidebar body tint");
                assert_eq!(
                    body.bounds.right().0 / scale,
                    right,
                    "the body must cover the border slot, not expose a bright wallpaper strip"
                );
            }
        });
    }

    #[gpui::test]
    fn sidebar_resize_cursor_follows_the_handle_after_release(cx: &mut TestAppContext) {
        let Some(mut settings) = isolated_sidebar_settings(cx) else {
            return;
        };
        let config = settings.settings_mut();
        config.appearance.animation_speed = oxideterm_settings::AnimationSpeed::Off;
        config.ai.enabled = true;
        config.sidebar_ui.collapsed = false;
        config.sidebar_ui.ai_sidebar_collapsed = false;
        config.sidebar_ui.width = 280;
        config.sidebar_ui.ai_sidebar_width = 300;
        settings.save().unwrap();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            super::super::window_shell::WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        let viewport_width = 1200.0;
        cx.simulate_resize(size(px(viewport_width), px(700.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        for right in [false, true] {
            let seam = |cx: &gpui::VisualTestContext| {
                workspace.read_with(cx, |workspace, cx| {
                    if right {
                        viewport_width
                            - workspace.tokens.metrics.activity_bar_width
                            - workspace.ai.state.read(cx).chat_ui().sidebar_width
                    } else {
                        workspace.sidebar_width
                    }
                })
            };
            let handle = |cx: &gpui::VisualTestContext| {
                gpui::point(px(seam(cx) + if right { 3.0 } else { -3.0 }), px(180.0))
            };
            let modifiers = Modifiers::default();
            let before = seam(cx);
            let grip = handle(cx);
            cx.simulate_mouse_move(grip, None, modifiers);
            assert_eq!(
                cx.update(|window, _| window.cursor_style_for_test()),
                CursorStyle::ResizeColumn
            );
            cx.simulate_mouse_down(grip, MouseButton::Left, modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let moved = gpui::point(grip.x + px(if right { -48.0 } else { 48.0 }), grip.y);
            cx.simulate_mouse_move(moved, Some(MouseButton::Left), modifiers);
            assert!((seam(cx) - before).abs() > 30.0);
            assert_eq!(
                cx.update(|window, _| window.cursor_style_for_test()),
                CursorStyle::ResizeColumn
            );

            // Release without sending a final move to the captured resize overlay.
            let outside = gpui::point(moved.x, px(-12.0));
            cx.simulate_mouse_up(outside, MouseButton::Left, modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.update(|window, _| window.cursor_style_for_test()),
                CursorStyle::Arrow,
                "sidebar {right} held the cursor after releasing outside the window"
            );
            let released = seam(cx);
            cx.simulate_mouse_move(gpui::point(px(600.0), px(180.0)), None, modifiers);
            assert_eq!(seam(cx), released);

            let grip = handle(cx);
            cx.simulate_mouse_move(grip, None, modifiers);
            assert_eq!(
                cx.update(|window, _| window.cursor_style_for_test()),
                CursorStyle::ResizeColumn
            );
            cx.simulate_mouse_down(grip, MouseButton::Left, modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            // The initial press moves the seam to the pointer.
            cx.simulate_mouse_up(handle(cx), MouseButton::Left, modifiers);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(
                cx.update(|window, _| window.cursor_style_for_test()),
                CursorStyle::ResizeColumn,
                "the cursor should still show resize while over the released handle"
            );
            cx.simulate_mouse_move(gpui::point(px(600.0), px(180.0)), None, modifiers);
            assert_eq!(
                cx.update(|window, _| window.cursor_style_for_test()),
                CursorStyle::Arrow
            );

            cx.update(|window, _| window.activate_window());
            let grip = handle(cx);
            cx.simulate_mouse_move(grip, None, modifiers);
            cx.simulate_mouse_down(grip, MouseButton::Left, modifiers);
            let resizing = |cx: &gpui::VisualTestContext| {
                workspace.read_with(cx, |workspace, cx| {
                    workspace.sidebar_resizing
                        || workspace.ai.state.read(cx).chat_ui().sidebar_resizing
                })
            };
            assert!(resizing(cx));
            cx.deactivate_window();
            assert!(
                !resizing(cx),
                "sidebar {right} kept the drag after focus loss"
            );
        }
    }

    struct TestContextSidebarChrome {
        total_width: f32,
        resize_started: Rc<Cell<bool>>,
        resize_moved: Rc<Cell<bool>>,
        resizing: bool,
    }

    struct TestLeftSidebarChrome {
        total_width: f32,
        resize_started: Rc<Cell<bool>>,
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
                    sidebar_resize_hotzone_chrome("left-hotzone-element", rgba(0x000000ff), true)
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
                        .child(workspace_sidebar_resize_cursor()),
                )
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
                                                ),
                                        ),
                                ),
                        ),
                )
                .child(
                    sidebar_resize_hotzone_chrome(
                        "context-hotzone-element",
                        rgba(0x000000ff),
                        false,
                    )
                    .left(px(seam))
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
            "left hotzone ends before terminal content",
            right_edge(&hotzone),
            right_edge(&frame),
        );

        cx.simulate_mouse_down(
            Point::new(
                frame.origin.x + frame.size.width + px(0.5),
                frame.origin.y + px(20.0),
            ),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            !resize_started.get(),
            "column zero must not start sidebar resize"
        );
        cx.simulate_mouse_move(
            Point::new(
                frame.origin.x + frame.size.width - px(3.0),
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
                frame.origin.x + frame.size.width - px(3.0),
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

        assert_close(
            "hotzone origin",
            f32::from(hotzone.origin.x) - f32::from(frame.origin.x),
            0.0,
        );
        assert_close(
            "hotzone width",
            f32::from(hotzone.size.width),
            SIDEBAR_RESIZE_HOTZONE_WIDTH,
        );

        cx.simulate_mouse_down(
            Point::new(frame.origin.x - px(0.5), frame.origin.y + px(20.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            !resize_started.get(),
            "terminal edge must not start sidebar resize"
        );
        cx.simulate_mouse_move(
            Point::new(frame.origin.x + px(3.0), frame.origin.y + px(20.0)),
            None,
            Modifiers::default(),
        );
        assert_eq!(
            cx.update(|window, _cx| window.cursor_style_for_test()),
            CursorStyle::ResizeColumn,
            "hovering the context-sidebar hotzone should apply the column-resize cursor"
        );
        cx.simulate_mouse_down(
            Point::new(frame.origin.x + px(3.0), frame.origin.y + px(20.0)),
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
