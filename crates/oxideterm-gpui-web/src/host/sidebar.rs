use super::{chrome::glyph, *};
use oxideterm_gpui_connections::session_manager::{
    SessionManagerOpenTarget, SessionSortField, SortDirection,
};
use oxideterm_gpui_shell::chrome::activity_button;
use oxideterm_gpui_ui::{
    WORKSPACE_STATUS_BAR_HEIGHT,
    button::{ButtonRadius, IconButtonOptions, icon_button},
    modal::popover_backdrop,
    motion::{
        MotionDuration, animated_chevron, exit_settle_duration, fade_in, popover_zoom_enter,
        slide_fade_in_x,
    },
    select::{
        select_option, select_option_action, select_option_highlighted, select_overlay_popup,
        select_overlay_popup_with_max_height, select_separator,
    },
    tree::{TreeGuideMetrics, tree_guide_child},
    typography::{
        MonospaceDatumOptions, MonospaceDatumTone, monospace_datum, tabular_nums_features,
    },
};
use oxideterm_settings::SessionSortOrder;
use oxideterm_theme::AppUiColors;

const PRIMARY_SIDEBAR_CONTENT_TOP_INSET: f32 = 4.0;
const SESSION_TREE_ICON_SIZE: f32 = 14.0;
const SESSION_TREE_CHILD_ICON_SIZE: f32 = 12.0;
const SESSION_TREE_NODE_HEIGHT: f32 = 28.0;
const SESSION_TREE_ITEM_HEIGHT: f32 = 26.0;
const SESSION_ROW_SELECTED_BG_ALPHA: u32 = 0x26;
const MONITOR_EMERALD: u32 = 0x34d399;

const CONTEXT_TOOLS: [ContextSidebarTool; 13] = [
    ContextSidebarTool::Monitor,
    ContextSidebarTool::Processes,
    ContextSidebarTool::Gpu,
    ContextSidebarTool::Docker,
    ContextSidebarTool::Container,
    ContextSidebarTool::Services,
    ContextSidebarTool::Logs,
    ContextSidebarTool::Ports,
    ContextSidebarTool::Tmux,
    ContextSidebarTool::Filesystems,
    ContextSidebarTool::Schedules,
    ContextSidebarTool::Packages,
    ContextSidebarTool::Redis,
];

const MONITORING_TOOL_COUNT: usize = 5;

fn mix_rgb(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |shift: u32| -> u32 {
        let ca = ((a >> shift) & 0xff) as f32;
        let cb = ((b >> shift) & 0xff) as f32;
        (ca + (cb - ca) * t).round().clamp(0.0, 255.0) as u32
    };
    (blend(16) << 16) | (blend(8) << 8) | blend(0)
}

fn session_item_surface(theme: AppUiColors, selected: bool) -> Div {
    if selected {
        div().bg(rgba((theme.accent << 8) | SESSION_ROW_SELECTED_BG_ALPHA))
    } else {
        div().hover(move |style| style.bg(rgb(theme.bg_hover)))
    }
}

fn tool_stack_height(count: usize, row_height: f32, separator_height: f32) -> f32 {
    let separators = if count > MONITORING_TOOL_COUNT {
        2.0
    } else {
        1.0
    };
    (count + 1) as f32 * row_height + separators * separator_height
}

fn visible_tool_count(available_height: f32, row_height: f32, separator_height: f32) -> usize {
    // Reserve More only for overflow, and the second divider only when both groups show.
    (0..=CONTEXT_TOOLS.len())
        .rev()
        .find(|&count| {
            let more_height = if count < CONTEXT_TOOLS.len() {
                row_height
            } else {
                0.0
            };
            tool_stack_height(count, row_height, separator_height) + more_height <= available_height
        })
        .unwrap_or(0)
}

fn overflow_tools(visible: usize) -> Vec<ContextSidebarTool> {
    CONTEXT_TOOLS[visible..].to_vec()
}

struct ActiveNodeGroup {
    node_id: String,
    title: String,
    subtitle: String,
    port: u32,
    local_group: bool,
    ble_group: bool,
    terminals: Vec<(TabId, PaneId, TerminalSessionId)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionActionVariant {
    Primary,
    Danger,
}

impl Workspace {
    pub(super) fn collapse(&mut self, side: RailSide, close: bool, cx: &mut Context<Self>) {
        let rail = self.rails.rail_mut(side);
        let generation = rail.set_collapsed(close, &self.tokens);
        if close && rail.mounted() {
            let delay = exit_settle_duration(&self.tokens, MotionDuration::Control);
            at_deadline(Instant::now() + delay, cx, move |h, _, cx| {
                if h.rails.rail_mut(side).finish_collapse(generation) {
                    cx.notify();
                }
            })
            .detach();
        }
    }

    fn separator(&self) -> Div {
        let m = self.tokens.metrics;
        div()
            .flex_none()
            .w(px(m.divider_width))
            .h(px(m.divider_height))
            .mt(px(2.0))
            .mb(px(m.activity_icon_gap + 2.0))
            .bg(self.chrome_divider())
    }

    fn host_tools_dimmed(&self) -> bool {
        let sidebar_open = !self.rails.right.collapsed()
            && self.rails.context_panel == ContextSidebarPanel::HostTools;
        if sidebar_open || self.context_tools_menu.is_some() {
            return false;
        }
        !self.layout.active_tab().is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::Workspace
                    | TabKind::LocalTerminal
                    | TabKind::SshTerminal
                    | TabKind::MoshTerminal
                    | TabKind::Sftp
                    | TabKind::Ide
                    | TabKind::Forwards
                    | TabKind::Runtime
                    | TabKind::ConnectionPool
                    | TabKind::Topology
            )
        })
    }

    fn rail_button(
        &self,
        icon: LucideIcon,
        id: impl Into<ElementId>,
        label: String,
        active: bool,
        dimmed: bool,
        count: u32,
        cx: &Context<Self>,
        activate: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        let t = &self.tokens;
        let critical = icon == LucideIcon::Bell && self.notifications.unread_critical_count > 0;
        let button = activity_button(
            t,
            icon,
            active,
            dimmed,
            rgb(t.ui.shell_active_bg()),
            count,
            if critical { t.ui.error } else { t.ui.accent },
        );
        self.bind_button(button, id, label, None, cx, activate)
    }

    fn page_button(
        &self,
        action: Action,
        icon: LucideIcon,
        kind: TabKind,
        key: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let active = self.layout.active_tab().is_some_and(|t| t.kind == kind);
        let count = if action == Action::Notifications && !self.notifications.dnd_enabled {
            self.notifications.unread_count
        } else {
            0
        };
        self.rail_button(
            icon,
            key,
            self.i18n.t(key),
            active,
            false,
            count,
            cx,
            move |h, w, cx| h.action(action, w, cx),
        )
    }

    fn drawer_button(
        &self,
        section: SidebarSection,
        icon: LucideIcon,
        key: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.rail_button(
            icon,
            key,
            self.i18n.t(key),
            self.rails.section == section && !self.rails.left.collapsed(),
            false,
            0,
            cx,
            move |h, w, cx| h.request(ShellRequest::Select(section), w, cx),
        )
    }

    pub(super) fn left_activity_rail(
        &self,
        env: &ShellEnvironment,
        cx: &Context<Self>,
    ) -> AnyElement {
        use SidebarSection::*;
        let mut primary = div()
            .id("activity-primary-items")
            .flex_1()
            .min_h_0()
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center();

        // Mirror desktop SidebarSection::PRIMARY_ITEMS order and group divider
        // (Sessions, Connections, Runtime | Extensions, CloudSync, Knowledge).
        for (index, (section, icon)) in SidebarSection::PRIMARY_ITEMS.into_iter().enumerate() {
            if index == 3 {
                primary = primary.child(self.separator());
            }
            primary = match section {
                Sessions => {
                    primary.child(self.drawer_button(section, icon, "sidebar.panels.sessions", cx))
                }
                Connections if self.demo => primary.child(self.drawer_button(
                    section,
                    icon,
                    "sidebar.panels.session_manager",
                    cx,
                )),
                Runtime if self.demo => primary.child(self.page_button(
                    Action::Runtime,
                    icon,
                    TabKind::Runtime,
                    "sidebar.panels.runtime",
                    cx,
                )),
                Extensions => primary.child(self.page_button(
                    Action::Plugins,
                    icon,
                    TabKind::PluginManager,
                    "sidebar.panels.plugins",
                    cx,
                )),
                CloudSync => primary.child(self.page_button(
                    Action::CloudSync,
                    icon,
                    TabKind::CloudSync,
                    "plugin.cloud_sync.panel_title",
                    cx,
                )),
                Knowledge if self.demo => primary.child(self.page_button(
                    Action::Knowledge,
                    icon,
                    TabKind::Knowledge,
                    "sidebar.panels.knowledge",
                    cx,
                )),
                _ => primary,
            };
        }

        let mut footer = div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .child(self.separator().mt_0());
        for (section, icon) in SidebarSection::FOOTER_ITEMS {
            footer = match section {
                Workspace if self.ready => footer.child(self.rail_button(
                    icon,
                    "sidebar.actions.new_local_terminal",
                    self.i18n.t("sidebar.actions.new_local_terminal"),
                    false,
                    false,
                    self.sessions.values().filter(|s| s.node == "local").count() as u32,
                    cx,
                    |h, w, cx| h.action(Action::Terminal, w, cx),
                )),
                Files if self.demo => footer.child(self.page_button(
                    Action::Files,
                    icon,
                    TabKind::FileManager,
                    "sidebar.panels.files",
                    cx,
                )),
                Notifications => footer.child(self.page_button(
                    Action::Notifications,
                    icon,
                    TabKind::NotificationCenter,
                    "sidebar.panels.notifications",
                    cx,
                )),
                Settings => footer.child(self.page_button(
                    Action::Settings,
                    icon,
                    TabKind::Settings,
                    "sidebar.tooltips.settings",
                    cx,
                )),
                _ => footer,
            };
        }
        let body = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .pb_2()
            .flex()
            .flex_col()
            .items_center()
            .bg(env.chrome_background)
            .child(primary)
            .child(footer);
        activity_rail(RailSide::Left, !self.rails.left.collapsed(), body, env, cx)
    }

    fn context_tool_count(&self, w: &Window) -> usize {
        let m = self.tokens.metrics;
        let available = f32::from(w.viewport_size().height)
            - m.titlebar_height
            - m.tabbar_height
            - PRIMARY_SIDEBAR_CONTENT_TOP_INSET;
        visible_tool_count(
            available,
            m.activity_icon_size + m.activity_icon_gap,
            m.divider_height + m.activity_icon_gap + 4.0,
        )
    }

    fn context_tool_active(&self, tool: ContextSidebarTool, cx: &App) -> bool {
        !self.rails.right.collapsed()
            && self.rails.context_panel == ContextSidebarPanel::HostTools
            && self
                .tools
                .monitor
                .as_ref()
                .is_some_and(|m| m.read(cx).active_tool(cx) == tool)
    }

    pub(super) fn right_activity_rail(
        &self,
        w: &Window,
        env: &ShellEnvironment,
        cx: &Context<Self>,
    ) -> AnyElement {
        let visible = !self.rails.right.collapsed();
        let dimmed = self.host_tools_dimmed();
        let count = self.context_tool_count(w);

        let mut body = div()
            .id("context-activity-items")
            .flex_1()
            .min_h_0()
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .flex()
            .flex_col()
            .items_center()
            .bg(env.chrome_background)
            .child(self.rail_button(
                LucideIcon::Sparkles,
                Action::Assistant.key(),
                self.i18n.t(Action::Assistant.key()),
                visible && self.rails.context_panel == ContextSidebarPanel::Assistant,
                false,
                0,
                cx,
                |h, w, cx| h.action(Action::Assistant, w, cx),
            ))
            .child(self.separator());

        for (index, tool) in CONTEXT_TOOLS.iter().take(count).copied().enumerate() {
            if index == MONITORING_TOOL_COUNT {
                body = body.child(self.separator());
            }
            let active = self.context_tool_active(tool, cx);
            body = body.child(self.rail_button(
                tool.icon(),
                tool.label_key(),
                self.i18n.t(tool.label_key()),
                active,
                dimmed,
                0,
                cx,
                move |h, w, cx| {
                    h.context_tools_menu = None;
                    h.action(Action::HostTool(tool), w, cx);
                },
            ));
        }

        let overflow = overflow_tools(count);
        if !overflow.is_empty() {
            let active = self.context_tools_menu.is_some()
                || overflow
                    .iter()
                    .any(|tool| self.context_tool_active(*tool, cx));
            body = body.child(self.rail_button(
                LucideIcon::MoreVertical,
                "context-tools-more",
                self.i18n.t("sidebar.panels.more_tools"),
                active,
                dimmed,
                0,
                cx,
                move |h, w, cx| {
                    if h.context_tools_menu.is_some() {
                        h.context_tools_menu = None;
                    } else {
                        let overflow = overflow_tools(h.context_tool_count(w));
                        if let Some(first) = overflow.first().copied() {
                            let current =
                                h.tools.monitor.as_ref().map(|m| m.read(cx).active_tool(cx));
                            let highlighted = current
                                .filter(|tool| overflow.contains(tool))
                                .unwrap_or(first);
                            h.popover_generation = h.popover_generation.wrapping_add(1);
                            h.context_tools_menu = Some(highlighted);
                        }
                    }
                    cx.notify();
                },
            ));
        }

        activity_rail(RailSide::Right, visible, body, env, cx)
    }

    pub(super) fn render_context_tools_menu(
        &mut self,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let highlighted = self.context_tools_menu?;
        if self.settings.read(cx).settings(cx).sidebar_ui.zen_mode {
            self.context_tools_menu = None;
            return None;
        }
        let count = self.context_tool_count(w);
        let overflow = overflow_tools(count);
        if overflow.is_empty() {
            self.context_tools_menu = None;
            return None;
        }
        let highlighted = if overflow.contains(&highlighted) {
            highlighted
        } else {
            overflow[0]
        };
        let max_height = (f32::from(w.viewport_size().height) - 24.0)
            .min(self.tokens.metrics.ui_select_max_height);
        let mut popup = select_overlay_popup_with_max_height(&self.tokens, 220.0, max_height)
            .id("context-tools-menu")
            .track_scroll(&self.context_tools_menu_scroll);
        for (index, tool) in overflow.into_iter().enumerate() {
            let option = select_option_action(
                select_option_highlighted(
                    &self.tokens,
                    self.i18n.t(tool.label_key()),
                    self.context_tool_active(tool, cx),
                    highlighted == tool,
                ),
                false,
                false,
                cx.listener(move |h, _, w, cx| {
                    h.context_tools_menu = None;
                    h.action(Action::HostTool(tool), w, cx);
                    cx.stop_propagation();
                }),
            );
            popup = popup.child(
                div()
                    .when(index > 0 && count + index == MONITORING_TOOL_COUNT, |row| {
                        row.child(select_separator(&self.tokens))
                    })
                    .child(option),
            );
        }
        let m = self.tokens.metrics;
        let top = m.titlebar_height
            + m.tabbar_height
            + PRIMARY_SIDEBAR_CONTENT_TOP_INSET
            + tool_stack_height(
                count,
                m.activity_icon_size + m.activity_icon_gap,
                m.divider_height + m.activity_icon_gap + 4.0,
            );
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|h, _, _, cx| {
                        h.context_tools_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|h, _, _, cx| {
                        h.context_tools_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(deferred(
                    anchored()
                        .anchor(Corner::TopRight)
                        .position(point(
                            w.viewport_size().width - px(m.activity_bar_width + 4.0),
                            px(top),
                        ))
                        .position_mode(AnchoredPositionMode::Window)
                        .snap_to_window_with_margin(px(8.0))
                        .child(popover_zoom_enter(
                            &self.tokens,
                            ("context-tools-menu-motion", self.popover_generation),
                            div().child(popup),
                            -6.0,
                            point(0.5, 0.0),
                        )),
                ))
                .into_any_element(),
        )
    }

    fn drawer(&self, header: Div, content: AnyElement, footer: Option<AnyElement>) -> AnyElement {
        let t = self.tokens;
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                header
                    .w_full()
                    .min_w_0()
                    .flex_none()
                    .h(px(t.metrics.tabbar_height))
                    .flex()
                    .items_center()
                    .bg(rgb(t.ui.shell_bg()))
                    .occlude()
                    .border_b_1()
                    .border_color(self.chrome_divider()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .bg(rgb(t.ui.shell_bg()))
                    .child(content),
            )
            .children(footer)
            .into_any_element()
    }

    pub(super) fn right_drawer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.tokens;
        let (panel_key, icon, key, content, toggle) = match self.rails.context_panel {
            ContextSidebarPanel::Assistant => (
                0usize,
                LucideIcon::Sparkles,
                Action::Assistant.key(),
                self.assistant.as_ref()?.clone().into_any_element(),
                None,
            ),
            ContextSidebarPanel::HostTools => {
                let monitor = self.tools.monitor.as_ref()?;
                let width = self.rails.right.width();
                if monitor.read(cx).width != width {
                    monitor.update(cx, |m, cx| {
                        m.width = width;
                        cx.notify();
                    });
                }
                let tool = monitor.read(cx).active_tool(cx);
                let enabled = monitor.read(cx).tool_enabled(tool);
                let toggle = (tool != ContextSidebarTool::Container).then(|| {
                    let color = if enabled {
                        MONITOR_EMERALD
                    } else {
                        t.ui.text_muted
                    };
                    let btn = icon_button(
                        &t,
                        glyph(LucideIcon::Power, 14.0, color),
                        IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Md),
                    );
                    self.bind_button(
                        btn,
                        "host-tool-monitoring-toggle",
                        self.i18n.t(if enabled {
                            "profiler.panel.disabled"
                        } else {
                            "profiler.panel.enable"
                        }),
                        None,
                        cx,
                        move |h, _, cx| {
                            if let Some(m) = &h.tools.monitor {
                                m.update(cx, |m, cx| m.set_tool_enabled(tool, !enabled, cx));
                            }
                            cx.notify();
                        },
                    )
                });
                let tool_idx = CONTEXT_TOOLS
                    .iter()
                    .position(|candidate| *candidate == tool)
                    .map_or(1, |idx| idx + 1);
                (
                    tool_idx,
                    tool.icon(),
                    tool.label_key(),
                    monitor.clone().into_any_element(),
                    toggle,
                )
            }
        };
        let header_title = fade_in(
            &t,
            ("right-sidebar-title", panel_key),
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(t.spacing.two))
                .child(glyph(icon, t.metrics.tab_icon_size, t.ui.text_secondary))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(t.metrics.titlebar_label_font_size))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(t.ui.text))
                        .child(self.i18n.t(key)),
                ),
            MotionDuration::Micro,
        );
        let header = div()
            .px_3()
            .gap(px(t.spacing.two))
            .child(header_title)
            .children(toggle);
        let animated_content = fade_in(
            &t,
            ("right-sidebar-body", panel_key),
            div().size_full().child(content),
            MotionDuration::Micro,
        );
        Some(self.drawer(header, animated_content, None))
    }

    fn sidebar_header_action(
        &self,
        id: impl Into<ElementId>,
        icon: LucideIcon,
        tooltip: String,
        active: bool,
        cx: &Context<Self>,
        activate: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        let t = &self.tokens;
        let btn = icon_button(
            t,
            glyph(
                icon,
                t.metrics.sidebar_action_icon_size,
                if active { t.ui.text } else { t.ui.text_muted },
            ),
            IconButtonOptions {
                has_background: active,
                background: active.then_some(rgb(t.ui.bg_active)),
                hover_background: Some(rgb(t.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(t.metrics.sidebar_action_size, ButtonRadius::Md)
            },
        );
        div()
            .ml(px(2.0))
            .child(self.bind_button(btn, id, tooltip, None, cx, activate))
            .into_any_element()
    }

    fn render_sidebar_search_bar(
        &self,
        id: &'static str,
        query: &str,
        placeholder_key: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let focused = self.sidebar_search_focused;
        let empty = query.is_empty();
        // Matches desktop `sidebar_search_row` + borderless `TextInputView`:
        // flat 36px strip under the header divider with no inner bordered box.
        div()
            .id(id)
            .w_full()
            .min_w_0()
            .flex_none()
            .h(px(36.0))
            .px_3()
            .flex()
            .items_center()
            .gap(px(t.spacing.one))
            .border_b_1()
            .border_color(self.chrome_divider())
            .bg(rgb(theme.shell_bg()))
            .cursor(CursorStyle::IBeam)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_size(px(t.metrics.sidebar_title_font_size))
                    .line_height(px(20.0))
                    .text_color(rgb(if empty { theme.text_muted } else { theme.text }))
                    .child(if empty {
                        div()
                            .flex()
                            .items_center()
                            .when(focused, |row| {
                                row.child(
                                    div().w(px(1.5)).h(px(14.0)).mr(px(1.0)).bg(rgb(theme.text)),
                                )
                            })
                            .child(self.i18n.t(placeholder_key))
                            .into_any_element()
                    } else {
                        div()
                            .flex()
                            .items_center()
                            .min_w_0()
                            .child(div().truncate().child(query.to_owned()))
                            .when(focused, |row| {
                                row.child(
                                    div()
                                        .flex_none()
                                        .w(px(1.5))
                                        .h(px(14.0))
                                        .ml(px(1.0))
                                        .bg(rgb(theme.text)),
                                )
                            })
                            .into_any_element()
                    }),
            )
            .when(!empty, |bar| {
                bar.child(
                    div()
                        .id(SharedString::from(format!("{id}-clear")))
                        .size(px(18.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(t.radii.sm))
                        .cursor_pointer()
                        .hover(move |b| b.bg(rgb(theme.bg_hover)))
                        .child(glyph(LucideIcon::X, 11.0, theme.text_muted))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|h, _, _, cx| {
                                if h.rails.section == SidebarSection::Connections {
                                    h.connections_search_query.clear();
                                } else {
                                    h.session_search_query.clear();
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|h, _, w, cx| {
                    h.sidebar_search_focused = true;
                    h.knowledge.active_input = None;
                    h.pages.plugin_search_focused = false;
                    w.focus(&h.focus, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_session_sort_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.session_sort_menu_open
            || self.rails.left.collapsed()
            || self.rails.section != SidebarSection::Sessions
        {
            return None;
        }
        let options = [
            (SessionSortOrder::Default, "sidebar.sort.default"),
            (
                SessionSortOrder::NameAscending,
                "sidebar.sort.name_ascending",
            ),
            (
                SessionSortOrder::NameDescending,
                "sidebar.sort.name_descending",
            ),
            (
                SessionSortOrder::ConnectedFirst,
                "sidebar.sort.connected_first",
            ),
        ];
        let mut popup = select_overlay_popup(&self.tokens, 220.0);
        for (order, label) in options {
            popup = popup.child(select_option_action(
                select_option(
                    &self.tokens,
                    self.i18n.t(label),
                    order == self.session_sort_order,
                ),
                false,
                false,
                cx.listener(move |h, _, _, cx| {
                    h.session_sort_order = order;
                    h.session_sort_menu_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        let m = self.tokens.metrics;
        let anchor_x = m.activity_bar_width + self.rails.left.width() - 42.0;
        let anchor_y = m.titlebar_height + m.tabbar_height + 4.0;
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|h, _, _, cx| {
                        h.session_sort_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|h, _, _, cx| {
                        h.session_sort_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(deferred(
                    anchored()
                        .anchor(Corner::TopRight)
                        .position(point(px(anchor_x), px(anchor_y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .snap_to_window_with_margin(px(8.0))
                        .child(popover_zoom_enter(
                            &self.tokens,
                            ("session-sort-menu-motion", self.popover_generation),
                            div().child(popup),
                            -6.0,
                            point(0.5, 0.0),
                        )),
                ))
                .into_any_element(),
        )
    }

    pub(super) fn render_connections_sort_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.connections_sort_menu_open
            || self.rails.left.collapsed()
            || self.rails.section != SidebarSection::Connections
        {
            return None;
        }
        let fields = [
            SessionSortField::Name,
            SessionSortField::Host,
            SessionSortField::Port,
            SessionSortField::Username,
            SessionSortField::AuthType,
            SessionSortField::Group,
            SessionSortField::LastUsed,
        ];
        let mut popup = select_overlay_popup(&self.tokens, 220.0);
        for field in fields {
            let active = self.connections_sort_field == field;
            let mut label = self.i18n.t(field.label_key());
            if active {
                label.push_str(match self.connections_sort_direction {
                    SortDirection::Asc => " ↑",
                    SortDirection::Desc => " ↓",
                });
            }
            popup = popup.child(select_option_action(
                select_option(&self.tokens, label, active),
                false,
                false,
                cx.listener(move |h, _, _, cx| {
                    if h.connections_sort_field == field {
                        h.connections_sort_direction = match h.connections_sort_direction {
                            SortDirection::Asc => SortDirection::Desc,
                            SortDirection::Desc => SortDirection::Asc,
                        };
                    } else {
                        h.connections_sort_field = field;
                        h.connections_sort_direction = SortDirection::Asc;
                    }
                    h.connections_sort_menu_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        let m = self.tokens.metrics;
        let anchor_x = m.activity_bar_width + self.rails.left.width() - 42.0;
        let anchor_y = m.titlebar_height + m.tabbar_height + 4.0;
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|h, _, _, cx| {
                        h.connections_sort_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|h, _, _, cx| {
                        h.connections_sort_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(deferred(
                    anchored()
                        .anchor(Corner::TopRight)
                        .position(point(px(anchor_x), px(anchor_y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .snap_to_window_with_margin(px(8.0))
                        .child(popover_zoom_enter(
                            &self.tokens,
                            ("connections-sort-menu-motion", self.popover_generation),
                            div().child(popup),
                            -6.0,
                            point(0.5, 0.0),
                        )),
                ))
                .into_any_element(),
        )
    }

    pub(super) fn render_connection_context_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let (node_id, pos) = self.connection_context_menu.clone()?;
        let is_ble = crate::ble::is_ble_node(&node_id);
        let connect_id = node_id.clone();
        let edit_id = node_id.clone();
        let delete_id = node_id.clone();
        let mut popup = select_overlay_popup(&self.tokens, 176.0).child(select_option_action(
            select_option(
                &self.tokens,
                self.i18n.t("sessionManager.actions.connect"),
                false,
            ),
            false,
            false,
            cx.listener(move |h, _, w, cx| {
                h.connection_context_menu = None;
                h.new_terminal(connect_id.clone(), None, w, cx);
                cx.stop_propagation();
            }),
        ));
        if !is_ble {
            popup = popup.child(select_option_action(
                select_option(
                    &self.tokens,
                    self.i18n.t("sessions.tree.actions.sftp"),
                    false,
                ),
                false,
                false,
                cx.listener(|h, _, w, cx| {
                    h.connection_context_menu = None;
                    h.action(Action::Sftp, w, cx);
                    cx.stop_propagation();
                }),
            ));
        }
        popup = popup
            .child(select_separator(&self.tokens))
            .child(select_option_action(
                select_option(
                    &self.tokens,
                    self.i18n.t("sessionManager.actions.edit"),
                    false,
                ),
                false,
                false,
                cx.listener(move |h, _, w, cx| {
                    h.connection_context_menu = None;
                    h.form(Some(&edit_id), None, w, cx);
                    cx.stop_propagation();
                }),
            ))
            .child(select_option_action(
                select_option(
                    &self.tokens,
                    self.i18n.t("sessionManager.actions.delete"),
                    false,
                ),
                false,
                false,
                cx.listener(move |h, _, _, cx| {
                    h.connection_context_menu = None;
                    h.catalog_data.items.retain(|r| {
                        r.target != SessionManagerOpenTarget::Connection(delete_id.clone())
                    });
                    h.ble_profiles.remove(&delete_id);
                    if h.selected_connection_id.as_deref() == Some(delete_id.as_str()) {
                        h.selected_connection_id = None;
                    }
                    h.catalog.update(cx, |v, cx| {
                        v.set_catalog_data(h.catalog_data.clone(), cx);
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|h, _, _, cx| {
                        h.connection_context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|h, _, _, cx| {
                        h.connection_context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(deferred(
                    anchored()
                        .anchor(Corner::TopLeft)
                        .position(pos)
                        .position_mode(AnchoredPositionMode::Window)
                        .snap_to_window_with_margin(px(8.0))
                        .child(popover_zoom_enter(
                            &self.tokens,
                            ("connection-context-menu-motion", self.popover_generation),
                            div().child(popup),
                            -4.0,
                            point(0.0, 0.0),
                        )),
                ))
                .into_any_element(),
        )
    }

    pub(super) fn left_drawer(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let section = self.rails.section;
        let transition_gen = self.rails.section_transition(section.panel());
        let title_key = match section {
            SidebarSection::Connections if self.demo => "sidebar.panels.connections_header",
            _ => "sidebar.panels.sessions_header",
        };
        let title_inner = div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(px(t.metrics.sidebar_title_font_size))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(t.ui.text_muted))
            .child(self.i18n.t(title_key));
        let title_element = if let Some(transition) = transition_gen {
            fade_in(
                &t,
                ("sidebar-header-title", transition as usize),
                title_inner,
                MotionDuration::Micro,
            )
        } else {
            title_inner.into_any_element()
        };
        let mut header = div().px(px(10.0)).gap(px(4.0)).child(title_element);

        if section == SidebarSection::Connections && self.demo {
            let sort_icon = self.connections_sort_direction.icon();
            let actions = div()
                .flex()
                .items_center()
                .child(self.sidebar_header_action(
                    "sidebar-connections-search",
                    LucideIcon::Search,
                    self.i18n.t("sidebar.search.title"),
                    self.connections_search_open,
                    cx,
                    |h, w, cx| {
                        h.connections_sort_menu_open = false;
                        h.connections_search_open = !h.connections_search_open;
                        h.begin_disclosure(
                            "connections-search".into(),
                            h.connections_search_open,
                            cx,
                        );
                        if h.connections_search_open {
                            h.sidebar_search_focused = true;
                            w.focus(&h.focus, cx);
                        } else {
                            h.connections_search_query.clear();
                            h.sidebar_search_focused = false;
                        }
                        cx.notify();
                    },
                ))
                .child(self.sidebar_header_action(
                    "sidebar-connections-sort",
                    sort_icon,
                    self.i18n.t("sidebar.tooltips.sort"),
                    self.connections_sort_menu_open,
                    cx,
                    |h, _, cx| {
                        h.connections_sort_menu_open = !h.connections_sort_menu_open;
                        if h.connections_sort_menu_open {
                            h.popover_generation = h.popover_generation.wrapping_add(1);
                        }
                        h.session_sort_menu_open = false;
                        cx.notify();
                    },
                ))
                .child(self.sidebar_header_action(
                    "sidebar-connections-open-tab",
                    LucideIcon::ExternalLink,
                    self.i18n.t("sidebar.tooltips.open_in_tab"),
                    false,
                    cx,
                    |h, w, cx| h.action(Action::Catalog, w, cx),
                ))
                .child(self.sidebar_header_action(
                    "sidebar-connections-new",
                    LucideIcon::Plus,
                    self.i18n.t("sidebar.tooltips.new_connection"),
                    false,
                    cx,
                    |h, w, cx| h.action(Action::NewConnection, w, cx),
                ));
            header = header.child(if let Some(transition) = transition_gen {
                fade_in(
                    &t,
                    ("sidebar-header-actions", transition as usize),
                    actions,
                    MotionDuration::Micro,
                )
            } else {
                actions.into_any_element()
            });
            let body_col = div()
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .when(
                    self.disclosure_motions
                        .retained("connections-search", self.connections_search_open),
                    |col| {
                        col.child(self.disclosure_motions.render(
                            "connections-search",
                            &t,
                            div().w_full().child(self.render_sidebar_search_bar(
                                "connections-search-input",
                                &self.connections_search_query,
                                "sessionManager.toolbar.search_placeholder",
                                cx,
                            )),
                            Some(36.0),
                        ))
                    },
                )
                .child(div().flex_1().min_h_0().child(self.connections_sidebar(cx)));
            let body = if let Some(transition) = transition_gen {
                slide_fade_in_x(
                    &t,
                    ("sidebar-section-body", transition as usize),
                    body_col,
                    -10.0,
                    MotionDuration::Micro,
                )
            } else {
                body_col.into_any_element()
            };
            return self.drawer(header, body, None);
        }

        let view_icon = if self.session_host_view {
            LucideIcon::Server
        } else {
            LucideIcon::Folder
        };
        let view_tooltip = self.i18n.t(if self.session_host_view {
            "sidebar.tooltips.switch_tree"
        } else {
            "sidebar.tooltips.switch_host"
        });
        let session_actions = div()
            .flex()
            .items_center()
            .child(self.sidebar_header_action(
                "sidebar-sessions-search",
                LucideIcon::Search,
                self.i18n.t("sidebar.search.title"),
                self.session_search_open,
                cx,
                |h, w, cx| {
                    h.session_sort_menu_open = false;
                    h.session_search_open = !h.session_search_open;
                    h.begin_disclosure("session-search".into(), h.session_search_open, cx);
                    if h.session_search_open {
                        h.sidebar_search_focused = true;
                        w.focus(&h.focus, cx);
                    } else {
                        h.session_search_query.clear();
                        h.sidebar_search_focused = false;
                    }
                    cx.notify();
                },
            ))
            .child(self.sidebar_header_action(
                "sidebar-sessions-sort",
                LucideIcon::ArrowDownAZ,
                self.i18n.t("sidebar.sort.title"),
                self.session_sort_menu_open,
                cx,
                |h, _, cx| {
                    h.session_sort_menu_open = !h.session_sort_menu_open;
                    if h.session_sort_menu_open {
                        h.popover_generation = h.popover_generation.wrapping_add(1);
                    }
                    h.connections_sort_menu_open = false;
                    cx.notify();
                },
            ))
            .child(self.sidebar_header_action(
                "sidebar-sessions-view",
                view_icon,
                view_tooltip,
                self.session_host_view,
                cx,
                |h, _, cx| {
                    h.session_host_view = !h.session_host_view;
                    cx.notify();
                },
            ))
            .when(self.demo, |actions| {
                actions.child(self.sidebar_header_action(
                    "sidebar-sessions-new",
                    LucideIcon::Plus,
                    self.i18n.t("sidebar.tooltips.new_connection"),
                    false,
                    cx,
                    |h, w, cx| h.action(Action::NewConnection, w, cx),
                ))
            });
        header = header.child(if let Some(transition) = transition_gen {
            fade_in(
                &t,
                ("sidebar-header-actions", transition as usize),
                session_actions,
                MotionDuration::Micro,
            )
        } else {
            session_actions.into_any_element()
        });

        let all_groups = self.active_session_groups();
        let active_count = all_groups
            .iter()
            .map(|g| if g.local_group { g.terminals.len() } else { 1 })
            .sum::<usize>();
        let query = self.session_search_query.trim().to_lowercase();
        let filtered_groups: Vec<ActiveNodeGroup> = if query.is_empty() {
            all_groups
        } else {
            all_groups
                .into_iter()
                .filter(|g| {
                    g.title.to_lowercase().contains(&query)
                        || g.subtitle.to_lowercase().contains(&query)
                        || g.node_id.to_lowercase().contains(&query)
                })
                .collect()
        };
        let content_col = div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .when(
                self.disclosure_motions
                    .retained("session-search", self.session_search_open),
                |col| {
                    col.child(self.disclosure_motions.render(
                        "session-search",
                        &t,
                        div().w_full().child(self.render_sidebar_search_bar(
                            "sessions-search-input",
                            &self.session_search_query,
                            "sidebar.search.placeholder",
                            cx,
                        )),
                        Some(36.0),
                    ))
                },
            )
            .child(div().flex_1().min_h_0().child(self.sessions_sidebar(
                &filtered_groups,
                !query.is_empty(),
                cx,
            )));
        let content = if let Some(transition) = transition_gen {
            slide_fade_in_x(
                &t,
                ("sidebar-section-body", transition as usize),
                content_col,
                -10.0,
                MotionDuration::Micro,
            )
        } else {
            content_col.into_any_element()
        };
        let footer = self.render_active_sessions_footer(active_count);
        self.drawer(header, content, Some(footer))
    }

    fn active_session_groups(&self) -> Vec<ActiveNodeGroup> {
        let mut by_node: Vec<ActiveNodeGroup> = Vec::new();
        for (_, tab) in self.layout.main_tabs() {
            let Some(root) = &tab.root_pane else {
                continue;
            };
            let mut panes = Vec::new();
            root.collect_pane_ids(&mut panes);
            for pane in panes {
                let Some(session_id) = root.session_id_for_pane(pane) else {
                    continue;
                };
                let Some(session) = self.sessions.get(&session_id) else {
                    continue;
                };
                if let Some(group) = by_node.iter_mut().find(|g| g.node_id == session.node) {
                    group.terminals.push((tab.id, pane, session_id));
                } else {
                    let local_group = session.node == "local";
                    let ble_group = crate::ble::is_ble_node(&session.node);
                    let profile = self.catalog_data.items.iter().find(|row| {
                        matches!(&row.target, SessionManagerOpenTarget::Connection(id) if id == &session.node)
                    });
                    let ble_profile = ble_group
                        .then(|| self.ble_profiles.get(&session.node))
                        .flatten();
                    let title = if local_group {
                        self.i18n
                            .t("modals.new_connection.transport_local_terminal")
                    } else if let Some(b) = ble_profile {
                        b.name.clone()
                    } else if let Some(p) = profile {
                        p.name.clone()
                    } else {
                        session.node.clone()
                    };
                    let subtitle = if let Some(b) = ble_profile {
                        b.device_name.clone()
                    } else {
                        profile.map(|p| p.subtitle.clone()).unwrap_or_default()
                    };
                    let port = if ble_group {
                        0
                    } else {
                        profile.map_or(22, |p| p.port)
                    };
                    by_node.push(ActiveNodeGroup {
                        node_id: session.node.clone(),
                        title,
                        subtitle,
                        port,
                        local_group,
                        ble_group,
                        terminals: vec![(tab.id, pane, session_id)],
                    });
                }
            }
        }
        match self.session_sort_order {
            SessionSortOrder::Default => {}
            SessionSortOrder::NameAscending => {
                by_node.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
            }
            SessionSortOrder::NameDescending => {
                by_node.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()));
            }
            SessionSortOrder::ConnectedFirst => {
                by_node.sort_by(|a, b| {
                    let a_conn = if a.ble_group {
                        a.terminals
                            .iter()
                            .any(|(_, _, sid)| self.ble_links.contains_key(sid))
                    } else {
                        self.ready
                    };
                    let b_conn = if b.ble_group {
                        b.terminals
                            .iter()
                            .any(|(_, _, sid)| self.ble_links.contains_key(sid))
                    } else {
                        self.ready
                    };
                    b_conn
                        .cmp(&a_conn)
                        .then_with(|| a.local_group.cmp(&b.local_group))
                        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                });
            }
        }
        by_node
    }

    fn sessions_sidebar(
        &self,
        groups: &[ActiveNodeGroup],
        filtering: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let active_terminal = self.active_session();
        let active_tab_kind = self.layout.active_tab().map(|tab| tab.kind.clone());

        if groups.is_empty() {
            if filtering {
                return div()
                    .p_4()
                    .text_size(px(t.metrics.sidebar_title_font_size))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sidebar.search.empty"))
                    .into_any_element();
            }
            return div()
                .id("sessions-sidebar-empty")
                .size_full()
                .px_3()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(glyph(
                    LucideIcon::Server,
                    t.metrics.empty_sidebar_icon_size,
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(px(t.metrics.empty_sidebar_title_font_size))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("sessions.tree.no_sessions")),
                )
                .into_any_element();
        }

        let mut list = div()
            .id("sessions-sidebar-list")
            .size_full()
            .overflow_y_scroll()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .px_1()
            .flex()
            .flex_col();

        for group in groups {
            let expanded = self.expanded_nodes.contains(&group.node_id);
            let owns_terminal = active_terminal.is_some_and(|active_id| {
                group.terminals.iter().any(|(_, _, sid)| *sid == active_id)
            });
            let owns_tool_tab = !group.local_group
                && !group.ble_group
                && matches!(
                    active_tab_kind,
                    Some(TabKind::Sftp | TabKind::Ide | TabKind::Forwards)
                );
            let owns_active_tab = owns_terminal || owns_tool_tab;
            let header = self.render_session_node_header(group, expanded, owns_active_tab, cx);

            let disc_key = format!("session-node:{}", group.node_id);
            let show_children = self.disclosure_motions.retained(&disc_key, expanded);
            let children = if show_children {
                self.render_session_node_children(
                    group,
                    active_terminal,
                    active_tab_kind.as_ref(),
                    cx,
                )
            } else {
                Vec::new()
            };
            let estimated_height = children.len() as f32 * SESSION_TREE_ITEM_HEIGHT;

            list = list.child(div().w_full().flex().flex_col().child(header).when(
                !children.is_empty(),
                |col| {
                    col.child(self.disclosure_motions.render(
                        &disc_key,
                        &t,
                        div().w_full().flex().flex_col().children(children),
                        Some(estimated_height),
                    ))
                },
            ));
        }

        list.into_any_element()
    }

    fn render_session_node_header(
        &self,
        group: &ActiveNodeGroup,
        expanded: bool,
        owns_active_tab: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let icon_color = if owns_active_tab {
            theme.accent
        } else {
            theme.text_muted
        };
        let leading_icon = if group.ble_group {
            LucideIcon::Bluetooth
        } else if group.local_group {
            LucideIcon::Terminal
        } else {
            LucideIcon::Server
        };
        let connected = if group.ble_group {
            group
                .terminals
                .iter()
                .any(|(_, _, sid)| self.ble_links.contains_key(sid))
        } else {
            self.ready
        };
        let node_id = group.node_id.clone();
        let chevron_id = SharedString::from(format!("session-node-chevron-{}", group.node_id));
        let terminal_count = group.terminals.len();
        let display_title = if self.session_host_view && !group.subtitle.is_empty() {
            group.subtitle.clone()
        } else {
            group.title.clone()
        };

        session_item_surface(theme, owns_active_tab)
            .id(SharedString::from(format!(
                "session-node-header-{}",
                group.node_id
            )))
            .relative()
            .w_full()
            .min_w_0()
            .h(px(SESSION_TREE_NODE_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .rounded(px(t.radii.sm))
            .cursor_pointer()
            .child(
                div()
                    .w(px(12.0))
                    .h(px(20.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(animated_chevron(
                        &t,
                        chevron_id,
                        svg()
                            .path(LucideIcon::ChevronRight.path())
                            .size(px(12.0))
                            .text_color(rgb(theme.text_muted)),
                        expanded,
                    )),
            )
            .child(
                div()
                    .ml(px(6.0))
                    .w(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(leading_icon, SESSION_TREE_ICON_SIZE, icon_color)),
            )
            .child(
                div()
                    .ml(px(6.0))
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(t.metrics.ui_text_xs))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(display_title),
            )
            .when(
                !group.local_group && !group.ble_group && group.port != 22,
                |row| {
                    row.child(
                        div().ml_2().flex_none().child(monospace_datum(
                            &t,
                            format!(":{}", group.port),
                            Some("Lilex".into()),
                            MonospaceDatumOptions::new(MonospaceDatumTone::Muted)
                                .text_size(t.metrics.ui_text_2xs),
                        )),
                    )
                },
            )
            .when(terminal_count > 0, |row| {
                row.child(
                    div()
                        .ml_2()
                        .flex_none()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(3.0))
                        .when(!group.local_group, |c| {
                            c.child(glyph(
                                LucideIcon::Terminal,
                                SESSION_TREE_CHILD_ICON_SIZE,
                                theme.text_muted,
                            ))
                        })
                        .child(monospace_datum(
                            &t,
                            terminal_count.to_string(),
                            Some("Lilex".into()),
                            MonospaceDatumOptions::new(MonospaceDatumTone::Muted)
                                .text_size(t.metrics.ui_text_2xs),
                        )),
                )
            })
            .child(
                div()
                    .ml(px(6.0))
                    .size(px(6.0))
                    .flex_none()
                    .rounded_full()
                    .bg(rgb(if connected {
                        theme.success
                    } else {
                        theme.text_muted
                    })),
            )
            .on_click(cx.listener(move |h, _, _, cx| {
                let now_expanded = if h.expanded_nodes.insert(node_id.clone()) {
                    true
                } else {
                    h.expanded_nodes.remove(&node_id);
                    false
                };
                h.begin_disclosure(format!("session-node:{node_id}"), now_expanded, cx);
                cx.stop_propagation();
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_session_node_children(
        &self,
        group: &ActiveNodeGroup,
        active_terminal: Option<TerminalSessionId>,
        active_tab_kind: Option<&TabKind>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let mut children = Vec::new();
        let has_terminals = !group.terminals.is_empty();

        for (index, &(tab_id, pane_id, session_id)) in group.terminals.iter().enumerate() {
            let active = active_terminal == Some(session_id);
            children.push(self.render_session_terminal_row(
                tab_id,
                pane_id,
                session_id,
                index + 1,
                active,
                false,
                cx,
            ));
        }

        if has_terminals {
            children.push(tree_guide_child(
                &self.tokens,
                TreeGuideMetrics::session_tree(),
                1,
                &[],
                false,
                div()
                    .w_full()
                    .h(px(7.0))
                    .pl(px(6.0))
                    .pr(px(8.0))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .h(px(1.0))
                            .bg(rgba((self.tokens.ui.border << 8) | 0x38)),
                    )
                    .into_any_element(),
            ));
        }

        let node_id = group.node_id.clone();
        if group.local_group {
            children.push(self.render_session_action_row(
                &group.node_id,
                "new-terminal",
                LucideIcon::Plus,
                self.i18n.t("sessions.tree.actions.new_terminal"),
                SessionActionVariant::Primary,
                false,
                true,
                cx.listener(move |h, _, w, cx| {
                    h.new_terminal(node_id.clone(), None, w, cx);
                    cx.stop_propagation();
                }),
            ));
            return children;
        }

        if group.ble_group {
            let disconnect_node = node_id;
            children.push(self.render_session_action_row(
                &group.node_id,
                "disconnect",
                LucideIcon::WifiOff,
                self.i18n.t("sessions.tree.actions.disconnect"),
                SessionActionVariant::Danger,
                false,
                true,
                cx.listener(move |h, _, _, cx| {
                    h.disconnect_node(&disconnect_node, cx);
                    cx.stop_propagation();
                }),
            ));
            return children;
        }

        // SSH node tools: New Terminal, SFTP, IDE, Port Forwarding, then Disconnect.
        let new_term_node = node_id.clone();
        children.push(self.render_session_action_row(
            &group.node_id,
            "new-terminal",
            LucideIcon::Plus,
            self.i18n.t("sessions.tree.actions.new_terminal"),
            SessionActionVariant::Primary,
            false,
            false,
            cx.listener(move |h, _, w, cx| {
                h.new_terminal(new_term_node.clone(), None, w, cx);
                cx.stop_propagation();
            }),
        ));

        if self.demo {
            children.push(self.render_session_action_row(
                &group.node_id,
                "sftp",
                LucideIcon::FolderOpen,
                self.i18n.t("sessions.tree.actions.sftp"),
                SessionActionVariant::Primary,
                active_tab_kind == Some(&TabKind::Sftp),
                false,
                cx.listener(|h, _, w, cx| {
                    h.action(Action::Sftp, w, cx);
                    cx.stop_propagation();
                }),
            ));
            children.push(self.render_session_action_row(
                &group.node_id,
                "ide",
                LucideIcon::Code2,
                "IDE".to_string(),
                SessionActionVariant::Primary,
                active_tab_kind == Some(&TabKind::Ide),
                false,
                cx.listener(|h, _, w, cx| {
                    h.action(Action::Ide, w, cx);
                    cx.stop_propagation();
                }),
            ));
            children.push(self.render_session_action_row(
                &group.node_id,
                "port-forwarding",
                LucideIcon::ArrowLeftRight,
                self.i18n.t("sessions.tree.actions.port_forwarding"),
                SessionActionVariant::Primary,
                active_tab_kind == Some(&TabKind::Forwards),
                false,
                cx.listener(|h, _, w, cx| {
                    h.action(Action::Forwards, w, cx);
                    cx.stop_propagation();
                }),
            ));
        }

        // Segment gap before destructive lifecycle action.
        children.push(tree_guide_child(
            &self.tokens,
            TreeGuideMetrics::session_tree(),
            1,
            &[],
            false,
            div().w_full().h(px(2.0)).into_any_element(),
        ));

        let disconnect_node = node_id.clone();
        children.push(self.render_session_action_row(
            &group.node_id,
            "disconnect",
            LucideIcon::WifiOff,
            self.i18n.t("sessions.tree.actions.disconnect"),
            SessionActionVariant::Danger,
            false,
            true,
            cx.listener(move |h, _, _, cx| {
                h.disconnect_node(&disconnect_node, cx);
                cx.stop_propagation();
            }),
        ));

        children
    }

    fn render_session_terminal_row(
        &self,
        tab_id: TabId,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        index: usize,
        active: bool,
        line_stops_here: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self
            .i18n
            .t("sessions.tree.terminal")
            .replace("{{number}}", &index.to_string());
        let icon_color = if active {
            theme.accent
        } else {
            theme.text_muted
        };

        tree_guide_child(
            &self.tokens,
            TreeGuideMetrics::session_tree(),
            1,
            &[],
            line_stops_here,
            session_item_surface(theme, active)
                .id(SharedString::from(format!(
                    "session-terminal-row-{}",
                    session_id.0
                )))
                .group("session-terminal-item")
                .relative()
                .w_full()
                .h(px(SESSION_TREE_ITEM_HEIGHT))
                .pl(px(6.0))
                .pr(px(8.0))
                .gap(px(6.0))
                .flex()
                .flex_row()
                .items_center()
                .rounded(px(self.tokens.radii.sm))
                .cursor_pointer()
                .child(
                    div()
                        .w(px(14.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(glyph(
                            LucideIcon::Terminal,
                            SESSION_TREE_CHILD_ICON_SIZE,
                            icon_color,
                        )),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(if active {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(rgb(theme.text))
                        .child(label),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "session-terminal-close-{}",
                            session_id.0
                        )))
                        .size(px(20.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.sm))
                        .opacity(0.0)
                        .group_hover("session-terminal-item", |btn| btn.opacity(0.75))
                        .hover(move |btn| btn.opacity(1.0).bg(rgb(theme.bg_hover)))
                        .child(glyph(LucideIcon::X, 12.0, theme.text_muted))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |h, _, w, cx| {
                                h.pane_request(
                                    ShellPaneRequest::ClosePane(
                                        tab_id,
                                        pane_id,
                                        PaneContent::Terminal(session_id),
                                    ),
                                    w,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |h, _, w, cx| {
                        h.layout.select_main_tab(Some(tab_id));
                        h.pane_request(
                            ShellPaneRequest::Focus(
                                tab_id,
                                pane_id,
                                PaneContent::Terminal(session_id),
                            ),
                            w,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_session_action_row(
        &self,
        owner_key: &str,
        action_key: &'static str,
        icon: LucideIcon,
        label: String,
        variant: SessionActionVariant,
        active: bool,
        line_stops_here: bool,
        on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let idle_label_color = mix_rgb(theme.text, theme.text_muted, 0.42);
        let idle_icon_color = theme.text_muted;
        let (hover_bg, hover_fg) = match variant {
            SessionActionVariant::Primary => (theme.bg_hover, theme.text),
            SessionActionVariant::Danger => {
                (mix_rgb(theme.bg_hover, theme.error, 0.14), theme.error)
            }
        };

        tree_guide_child(
            &self.tokens,
            TreeGuideMetrics::session_tree(),
            1,
            &[],
            line_stops_here,
            div()
                .id(SharedString::from(format!(
                    "session-action-{owner_key}-{action_key}"
                )))
                .group("session-action-row")
                .h(px(SESSION_TREE_ITEM_HEIGHT))
                .w_full()
                .pl(px(6.0))
                .pr(px(8.0))
                .gap(px(6.0))
                .flex()
                .flex_row()
                .items_center()
                .relative()
                .rounded(px(self.tokens.radii.sm))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(if active { theme.text } else { idle_label_color }))
                .cursor_pointer()
                .when(active, |row| {
                    row.bg(rgba((theme.accent << 8) | SESSION_ROW_SELECTED_BG_ALPHA))
                })
                .hover(move |row| row.bg(rgb(hover_bg)).text_color(rgb(hover_fg)))
                .child(
                    div()
                        .w(px(14.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path(icon.path())
                                .size(px(SESSION_TREE_CHILD_ICON_SIZE))
                                .text_color(rgb(if active {
                                    theme.accent
                                } else {
                                    idle_icon_color
                                }))
                                .group_hover("session-action-row", move |el| {
                                    el.text_color(rgb(hover_fg))
                                }),
                        ),
                )
                .child(div().min_w_0().flex_1().truncate().child(label))
                .on_mouse_down(MouseButton::Left, on_click)
                .into_any_element(),
        )
    }

    fn render_active_sessions_footer(&self, active_count: usize) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self
            .i18n
            .t("sidebar.session_footer_counts")
            .replace("{{active}}", &active_count.to_string())
            .replace("{{selected}}", "0");
        div()
            .w_full()
            .min_w_0()
            .flex_none()
            .h(px(WORKSPACE_STATUS_BAR_HEIGHT))
            .px(px(self.tokens.spacing.three))
            .py_0()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .border_t_1()
            .border_color(self.chrome_divider())
            .bg(rgb(theme.shell_bg()))
            .child(
                div()
                    .flex_none()
                    .size(px(6.0))
                    .rounded_full()
                    .opacity(if active_count > 0 { 1.0 } else { 0.55 })
                    .bg(rgb(if active_count > 0 {
                        theme.success
                    } else {
                        theme.text_muted
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_features(tabular_nums_features())
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .into_any_element()
    }

    fn connections_sidebar(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let query = self.connections_search_query.trim().to_lowercase();
        let mut items: Vec<_> = self
            .catalog_data
            .items
            .iter()
            .filter_map(|row| match &row.target {
                SessionManagerOpenTarget::Connection(id)
                    if query.is_empty()
                        || row.name.to_lowercase().contains(&query)
                        || row.subtitle.to_lowercase().contains(&query)
                        || row.host.to_lowercase().contains(&query) =>
                {
                    Some((id.clone(), row))
                }
                _ => None,
            })
            .collect();
        items.sort_by(|a, b| {
            let ord = match self.connections_sort_field {
                SessionSortField::Name => a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()),
                SessionSortField::Host => a.1.host.to_lowercase().cmp(&b.1.host.to_lowercase()),
                SessionSortField::Port => a.1.port.cmp(&b.1.port),
                SessionSortField::Username => {
                    a.1.username
                        .to_lowercase()
                        .cmp(&b.1.username.to_lowercase())
                }
                SessionSortField::AuthType => {
                    crate::ble::is_ble_node(&a.0).cmp(&crate::ble::is_ble_node(&b.0))
                }
                SessionSortField::Group => a.1.group.cmp(&b.1.group),
                SessionSortField::LastUsed => a.1.last_used.cmp(&b.1.last_used),
            };
            match self.connections_sort_direction {
                SortDirection::Asc => ord,
                SortDirection::Desc => ord.reverse(),
            }
        });

        if items.is_empty() {
            if !query.is_empty() {
                return div()
                    .p_4()
                    .text_size(px(t.metrics.sidebar_title_font_size))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sidebar.search.empty"))
                    .into_any_element();
            }
            return div()
                .id("connections-sidebar-empty")
                .size_full()
                .px_3()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(glyph(
                    LucideIcon::LayoutList,
                    t.metrics.empty_sidebar_icon_size,
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(px(t.metrics.empty_sidebar_title_font_size))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("layout.empty.recent_connections_empty")),
                )
                .into_any_element();
        }

        div()
            .id("connections-sidebar-list")
            .size_full()
            .overflow_y_scroll()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .px_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(items.into_iter().map(|(node_id, row)| {
                let is_ble = crate::ble::is_ble_node(&node_id);
                let is_connected = self.sessions.values().any(|s| s.node == node_id);
                let is_selected = self.selected_connection_id.as_deref() == Some(node_id.as_str());
                let semantic = if is_ble { theme.warning } else { theme.info };
                let icon_box = div()
                    .size(px(24.0))
                    .flex_none()
                    .rounded(px(t.radii.sm))
                    .bg(rgba((semantic << 8) | 0x26))
                    .when(!is_ble, |badge| {
                        badge
                            .border_1()
                            .border_color(rgba((theme.text << 8) | 0x1a))
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph(
                        if is_ble {
                            LucideIcon::Bluetooth
                        } else {
                            LucideIcon::Server
                        },
                        14.0,
                        semantic,
                    ));
                let click_id = node_id.clone();
                let ctx_id = node_id.clone();
                div()
                    .id(SharedString::from(format!(
                        "connections-sidebar-row-{node_id}"
                    )))
                    .relative()
                    .w_full()
                    .min_w_0()
                    .px_2()
                    .py(px(5.0))
                    .rounded(px(t.radii.sm))
                    .flex()
                    .items_center()
                    .gap(px(t.spacing.two))
                    .cursor_pointer()
                    .when(is_selected, |r| {
                        r.bg(rgba((theme.accent << 8) | 0x1a)).child(
                            div()
                                .absolute()
                                .left_0()
                                .top(px(5.0))
                                .bottom(px(5.0))
                                .w(px(2.0))
                                .rounded_full()
                                .bg(rgb(theme.accent)),
                        )
                    })
                    .when(!is_selected, |r| {
                        r.hover(move |r| r.bg(rgb(theme.bg_hover)))
                    })
                    .child(icon_box)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap_1()
                                    .when(is_connected, |title_row| {
                                        title_row.child(
                                            div()
                                                .flex_none()
                                                .size(px(6.0))
                                                .rounded_full()
                                                .bg(rgb(theme.success)),
                                        )
                                    })
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(t.metrics.ui_text_xs))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(row.name.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(t.metrics.ui_text_2xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(row.subtitle.clone()),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |h, e: &MouseDownEvent, w, cx| {
                            h.selected_connection_id = Some(click_id.clone());
                            if e.click_count >= 2 {
                                h.new_terminal(click_id.clone(), None, w, cx);
                            } else {
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |h, e: &MouseDownEvent, _, cx| {
                            h.selected_connection_id = Some(ctx_id.clone());
                            h.popover_generation = h.popover_generation.wrapping_add(1);
                            h.connection_context_menu = Some((ctx_id.clone(), e.position));
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            }))
            .into_any_element()
    }
}
