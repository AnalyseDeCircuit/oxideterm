use crate::{ShellEnvironment, ShellHost, ShellRequest};
use gpui::{AnyElement, CursorStyle, MouseButton, div, prelude::*, px, rgb, svg};
use oxideterm_gpui_ui::{
    button::{ButtonRadius, IconButtonOptions, icon_button},
    icons::LucideIcon,
    motion::{DrawerEdge, SidebarMotion},
    separator::{SIDEBAR_RESIZE_HOTZONE_WIDTH, sidebar_resize_hotzone_chrome},
};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidebarSection {
    Sessions,
    Connections,
    Forwards,
    Runtime,
    Terminal,
    Network,
    Extensions,
    CloudSync,
    Knowledge,
    Assistant,
    HostTools,
    Automation,
    Workspace,
    Files,
    Notifications,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextSidebarPanel {
    Assistant,
    HostTools,
}

impl SidebarSection {
    pub fn from_settings_key(key: &str) -> Self {
        match key {
            // The retired saved-connections sidebar now restores the active
            // session navigator while the full manager remains a workspace tab.
            "connections" | "saved" => Self::Sessions,
            // The session manager sidebar body persists under its own key.
            "session_manager" => Self::Connections,
            // Embedded SFTP now shares the active-sessions panel. Preserve the
            // old persisted key as a migration path instead of reopening a
            // retired standalone sidebar.
            "sftp" => Self::Sessions,
            "forwards" => Self::Forwards,
            "runtime" => Self::Runtime,
            "connection_pool" | "terminal" => Self::Terminal,
            // Retired health-page keys now restore the Host Tools replacement.
            "connection_monitor" | "activity" => Self::HostTools,
            "network" | "topology" => Self::Network,
            "extensions" => Self::Extensions,
            "cloud_sync" => Self::CloudSync,
            "knowledge" => Self::Knowledge,
            "ai" | "assistant" => Self::Assistant,
            "host_tools" => Self::HostTools,
            "automation" => Self::Automation,
            "workspace" => Self::Workspace,
            "files" => Self::Files,
            // The standalone monitor activity button was retired after Host Tools
            // became the cross-platform owner of connection monitoring.
            "monitor" => Self::HostTools,
            "notifications" => Self::Notifications,
            "settings" => Self::Settings,
            _ => Self::Sessions,
        }
    }

    pub fn as_settings_key(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            // The session manager sidebar renders the saved-connections tree
            // inline in the sidebar body; persist under a stable key so the
            // retired "saved"/"connections" migrations stay distinct.
            Self::Connections => "session_manager",
            Self::Forwards => "forwards",
            Self::Runtime => "runtime",
            Self::Terminal => "connection_pool",
            Self::Network => "topology",
            Self::Extensions => "extensions",
            Self::CloudSync => "cloud_sync",
            Self::Knowledge => "knowledge",
            Self::Assistant => "ai",
            Self::HostTools => "host_tools",
            Self::Automation => "automation",
            Self::Workspace => "workspace",
            Self::Files => "files",
            Self::Notifications => "notifications",
            Self::Settings => "settings",
        }
    }
}

impl SidebarSection {
    pub const PRIMARY_ITEMS: [(Self, LucideIcon); 6] = [
        (SidebarSection::Sessions, LucideIcon::Link2),
        (SidebarSection::Connections, LucideIcon::LayoutList),
        (SidebarSection::Runtime, LucideIcon::Gauge),
        (SidebarSection::Extensions, LucideIcon::Puzzle),
        (SidebarSection::CloudSync, LucideIcon::Cloud),
        (SidebarSection::Knowledge, LucideIcon::BookOpen),
    ];
    pub const FOOTER_ITEMS: [(Self, LucideIcon); 4] = [
        (SidebarSection::Workspace, LucideIcon::Square),
        (SidebarSection::Files, LucideIcon::FolderOpen),
        (SidebarSection::Notifications, LucideIcon::Bell),
        (SidebarSection::Settings, LucideIcon::Settings),
    ];
    pub fn panel(self) -> Self {
        match self {
            SidebarSection::Sessions
            | SidebarSection::Connections
            | SidebarSection::Forwards
            | SidebarSection::Extensions
            | SidebarSection::CloudSync => self,
            // Tauri separates activity-bar tab buttons from sidebar sections.
            // Keep tab-only entries from replacing the Sessions sidebar body.
            SidebarSection::Terminal
            | SidebarSection::Runtime
            | SidebarSection::Network
            | SidebarSection::Knowledge
            | SidebarSection::Assistant
            | SidebarSection::HostTools
            | SidebarSection::Automation
            | SidebarSection::Workspace
            | SidebarSection::Files
            | SidebarSection::Notifications
            | SidebarSection::Settings => SidebarSection::Sessions,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RailSide {
    Left,
    Right,
}

impl RailSide {
    // Desktop persists the left rail and panel together, but only the right panel.
    pub fn width_at(self, x: f32, viewport: f32, activity_width: f32) -> f32 {
        match self {
            Self::Left => x,
            Self::Right => viewport - activity_width - x,
        }
    }

    pub fn toggle_id(self) -> &'static str {
        match self {
            Self::Left => "activity-sidebar-toggle",
            Self::Right => "context-sidebar-toggle",
        }
    }
}

pub fn clamp_responsive_sidebar_width(width: f32, viewport: f32, min: f32, max: f32) -> f32 {
    let viewport = viewport.max(0.0);
    let min = min.max(viewport * 0.16);
    let max = max.max(viewport * 0.45).min(viewport.max(min)).max(min);
    width.clamp(min, max)
}

pub struct RailState {
    side: RailSide,
    width: f32,
    collapsed: bool,
    mounted: bool,
    resizing: bool,
    generation: u64,
    motion: SidebarMotion,
}

impl RailState {
    pub fn new(
        side: RailSide,
        width: f32,
        collapsed: bool,
        visible: bool,
        tokens: &ThemeTokens,
    ) -> Self {
        let mut rail = Self {
            side,
            width,
            collapsed,
            mounted: visible,
            resizing: false,
            generation: 0,
            motion: SidebarMotion::new(0.0),
        };
        rail.restore(width, collapsed, visible, tokens);
        rail
    }
    pub fn width(&self) -> f32 {
        self.width
    }
    pub fn panel_width(&self, tokens: &ThemeTokens) -> f32 {
        match self.side {
            RailSide::Left => (self.width - tokens.metrics.activity_bar_width).max(0.0),
            RailSide::Right => self.width,
        }
    }
    pub fn collapsed(&self) -> bool {
        self.collapsed
    }
    pub fn mounted(&self) -> bool {
        self.mounted
    }
    pub fn resizing(&self) -> bool {
        self.resizing
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn restore(&mut self, width: f32, collapsed: bool, visible: bool, tokens: &ThemeTokens) {
        self.width = width;
        self.collapsed = collapsed;
        self.reset_visibility(visible, tokens);
    }

    pub fn reset_visibility(&mut self, visible: bool, tokens: &ThemeTokens) {
        self.generation = self.generation.wrapping_add(1);
        self.mounted = visible;
        self.motion.settle(if visible {
            self.panel_width(tokens)
        } else {
            0.0
        });
    }

    pub fn set_collapsed(&mut self, collapsed: bool, tokens: &ThemeTokens) -> u64 {
        self.collapsed = collapsed;
        self.generation = self.generation.wrapping_add(1);
        self.motion.retarget(if collapsed {
            0.0
        } else {
            self.panel_width(tokens)
        });
        if !collapsed {
            self.mounted = true;
        } else if !tokens.motion.enabled || !tokens.motion.spatial_enabled {
            self.motion.settle(0.0);
            self.mounted = false;
        }
        self.generation
    }

    /// An intervening expand or drag invalidates the pending exit.
    pub fn finish_collapse(&mut self, generation: u64) -> bool {
        if self.collapsed && self.mounted && self.generation == generation {
            self.mounted = false;
            return true;
        }
        false
    }

    pub fn set_width(&mut self, width: f32, visible: bool, tokens: &ThemeTokens) -> bool {
        if (width - self.width).abs() < f32::EPSILON {
            return false;
        }
        self.width = width;
        self.motion.settle(if visible {
            self.panel_width(tokens)
        } else {
            0.0
        });
        true
    }

    pub fn begin_resize(&mut self, width: f32, tokens: &ThemeTokens) {
        self.generation = self.generation.wrapping_add(1);
        self.resizing = true;
        self.width = width;
        self.motion.settle(self.panel_width(tokens));
    }
    pub fn end_resize(&mut self) -> bool {
        std::mem::take(&mut self.resizing)
    }

    pub fn present<H: ShellHost>(
        &mut self,
        panel: Option<AnyElement>,
        visible: bool,
        env: &ShellEnvironment,
        top: f32,
        cx: &Context<H>,
    ) -> RailElements {
        let side = self.side;
        let left = side == RailSide::Left;
        let width = self.panel_width(&env.tokens);
        let drawer = panel.map(|panel| {
            self.motion.retarget(if visible { width } else { 0.0 });
            self.motion.animate_drawer(
                &env.tokens,
                if left {
                    "workspace-left-sidebar-motion"
                } else {
                    "workspace-right-sidebar-motion"
                },
                width,
                if left {
                    DrawerEdge::Left
                } else {
                    DrawerEdge::Right
                },
                div()
                    .flex_none()
                    .w(px(width))
                    .h_full()
                    .child(panel)
                    .into_any_element(),
            )
        });
        let handle = (visible && !env.zen_mode && !env.chrome_locked).then(|| {
            let id = if left {
                "workspace-left-sidebar-resize-hotzone"
            } else {
                "context-right-sidebar-resize-hotzone"
            };
            let line = if self.resizing {
                rgb(env.tokens.ui.accent)
            } else if left {
                env.left_divider
            } else {
                env.divider
            };
            let hotzone = sidebar_resize_hotzone_chrome(id, line, left)
                .top(px(top))
                .bottom_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |host, event: &gpui::MouseDownEvent, window, cx| {
                        host.request(
                            ShellRequest::BeginResize(side, event.position.x),
                            window,
                            cx,
                        );
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
                .child(workspace_sidebar_resize_cursor());
            let activity_width = env.tokens.metrics.activity_bar_width;
            self.motion.animate(
                &env.tokens,
                if left {
                    "left-sidebar-edge-motion"
                } else {
                    "right-sidebar-edge-motion"
                },
                hotzone,
                move |edge, width| {
                    let offset = px(sidebar_resize_hotzone_origin(activity_width + width));
                    if left {
                        edge.left(offset)
                    } else {
                        edge.right(offset)
                    }
                },
            )
        });
        RailElements { drawer, handle }
    }
}

#[derive(Default)]
pub struct RailElements {
    pub(crate) drawer: Option<AnyElement>,
    pub(crate) handle: Option<AnyElement>,
}

pub struct WorkspaceRails {
    pub left: RailState,
    pub right: RailState,
    pub section: SidebarSection,
    pub context_panel: ContextSidebarPanel,
    section_generation: u64,
    section_transition: Option<(SidebarSection, u64)>,
}

impl WorkspaceRails {
    pub fn new(
        left: RailState,
        right: RailState,
        section: SidebarSection,
        context_panel: ContextSidebarPanel,
    ) -> Self {
        Self {
            left,
            right,
            section,
            context_panel,
            section_generation: 0,
            section_transition: None,
        }
    }

    pub fn select_section(&mut self, section: SidebarSection, tokens: &ThemeTokens) -> Option<u64> {
        let previous = self.section.panel();
        self.section = section;
        let panel = section.panel();
        self.section_transition =
            (!self.left.collapsed && previous != panel && tokens.motion.enabled).then(|| {
                self.section_generation = self.section_generation.wrapping_add(1);
                (panel, self.section_generation)
            });
        self.section_transition.map(|(_, generation)| generation)
    }

    pub fn section_transition(&self, panel: SidebarSection) -> Option<u64> {
        self.section_transition
            .filter(|(current, _)| *current == panel)
            .map(|(_, generation)| generation)
    }

    pub fn finish_section_transition(&mut self, generation: u64) -> bool {
        if self
            .section_transition
            .is_some_and(|(_, current)| current == generation)
        {
            self.section_transition = None;
            return true;
        }
        false
    }

    pub fn resizing_side(&self) -> Option<RailSide> {
        if self.left.resizing {
            Some(RailSide::Left)
        } else if self.right.resizing {
            Some(RailSide::Right)
        } else {
            None
        }
    }
    pub fn rail_mut(&mut self, side: RailSide) -> &mut RailState {
        match side {
            RailSide::Left => &mut self.left,
            RailSide::Right => &mut self.right,
        }
    }

    pub fn set_zen_mode(&mut self, zen: bool, right_available: bool, tokens: &ThemeTokens) {
        for (rail, available) in [(&mut self.left, true), (&mut self.right, right_available)] {
            if zen {
                rail.reset_visibility(false, tokens);
            } else {
                // Remount at the hidden position; the next frame opens the drawer from zero.
                rail.generation = rail.generation.wrapping_add(1);
                rail.mounted = !rail.collapsed && available;
            }
        }
    }
}

pub fn sidebar_resize_hotzone_origin(seam: f32) -> f32 {
    seam - SIDEBAR_RESIZE_HOTZONE_WIDTH
}

pub fn workspace_sidebar_resize_cursor() -> impl IntoElement {
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

/// Keeps the captured drag above virtual lists and terminal cursor overrides.
pub fn rail_resize_capture<H: ShellHost>(cx: &Context<H>) -> AnyElement {
    div()
        .id("workspace-pointer-capture-overlay")
        .absolute()
        .inset_0()
        .cursor(CursorStyle::ResizeColumn)
        .occlude()
        .on_mouse_move(
            cx.listener(|host, event: &gpui::MouseMoveEvent, window, cx| {
                host.request(
                    if event.dragging() {
                        ShellRequest::DragResize(event.position.x)
                    } else {
                        ShellRequest::EndResize
                    },
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|host, _, window, cx| {
                host.request(ShellRequest::EndResize, window, cx);
                cx.stop_propagation();
            }),
        )
        .child(
            gpui::canvas(
                |_, _, _| (),
                |_, _, window, _| window.set_window_cursor_style(CursorStyle::ResizeColumn),
            )
            .absolute(),
        )
        .into_any_element()
}

pub fn activity_rail<H: ShellHost>(
    side: RailSide,
    visible: bool,
    body: impl IntoElement,
    env: &ShellEnvironment,
    cx: &Context<H>,
) -> AnyElement {
    let tokens = &env.tokens;
    let theme = tokens.ui;
    let left = side == RailSide::Left;
    let icon = match (left, visible) {
        (true, true) => LucideIcon::PanelLeftClose,
        (true, false) => LucideIcon::PanelLeft,
        (false, true) => LucideIcon::PanelRightClose,
        (false, false) => LucideIcon::PanelRight,
    };
    let glyph = svg()
        .path(icon.path())
        .size(px(tokens.metrics.sidebar_collapse_icon_size))
        .text_color(rgb(theme.text_muted));
    let button = if left {
        div()
            .size(px(tokens.metrics.activity_icon_size))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(tokens.radii.md))
            .cursor_pointer()
            .id(side.toggle_id())
            .hover(move |style| style.bg(rgb(theme.bg_hover)))
            .child(glyph)
    } else {
        icon_button(
            tokens,
            glyph.into_any_element(),
            IconButtonOptions {
                radius: ButtonRadius::Md,
                hover_background: Some(rgb(theme.bg_hover)),
                idle_opacity: 1.0,
                ..IconButtonOptions::compact(tokens.metrics.activity_icon_size)
            },
        )
        .id(side.toggle_id())
    };
    let label = env.i18n.t(if visible {
        "sidebar.actions.collapse"
    } else {
        "sidebar.actions.expand"
    });
    let button = button
        .on_mouse_move(
            cx.listener(move |host, event: &gpui::MouseMoveEvent, window, cx| {
                host.request(
                    ShellRequest::ShowToggleHint(side, label.clone(), event.position),
                    window,
                    cx,
                );
            }),
        )
        .on_hover(cx.listener(move |host, hovered: &bool, window, cx| {
            if !hovered {
                host.request(ShellRequest::HideToggleHint(side), window, cx);
            }
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |host, _, window, cx| {
                host.request(ShellRequest::Toggle(side), window, cx);
                if !left {
                    cx.stop_propagation();
                }
            }),
        );
    let header = div()
        .when(left, |row| row.relative())
        .flex_none()
        .w_full()
        .h(px(tokens.metrics.tabbar_height))
        .flex()
        .items_center()
        .justify_center()
        .bg(env.chrome_background)
        .border_b_1()
        .border_color(env.divider)
        .child(button);
    let bar = div()
        .w(px(tokens.metrics.activity_bar_width))
        .h_full()
        .flex()
        .flex_col()
        .items_center();
    if left {
        bar.child(header).child(body).into_any_element()
    } else {
        bar.id("context-activity-bar")
            .flex_none()
            .child(header)
            .child(body)
            .into_any_element()
    }
}
