use super::*;

use gpui::Div;
use oxideterm_gpui_ui::select::{
    select_event_boundary, select_option_action, select_option_highlighted,
};
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};

const HOST_TOOLS_CONNECTION_ROW_HEIGHT: f32 = 32.0;
const SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT: f32 = 36.0;
const SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y: f32 = 8.0;
const SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS: usize = 4;
const SYSTEM_HEALTH_SELECTOR_GAP: f32 = 8.0;
const HOST_TOOLS_TAB_STRIP_HEIGHT: f32 = 48.0;
const HOST_TOOLS_TAB_SCROLLBAR_HEIGHT: f32 = 3.0;
const HOST_TOOLS_TAB_SCROLLBAR_BOTTOM_INSET: f32 = 5.0;
const HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET: f32 = 12.0;
const HOST_TOOLS_TAB_SCROLLBAR_MIN_THUMB_WIDTH: f32 = 32.0;
const HOST_TOOLS_TAB_SCROLLBAR_RADIUS: f32 = 2.0;
const HOST_TOOLS_TAB_SCROLLBAR_ALPHA: u32 = 0x66;
const HOST_TOOLS_TAB_SCROLLBAR_DRAG_HEIGHT: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct HostToolsTabSelectionGeometry {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy)]
struct HostToolsTabScrollbarGeometry {
    viewport_left: f32,
    track_width: f32,
    thumb_width: f32,
    thumb_left: f32,
    max_scroll: f32,
}

fn host_tools_tab_index(tool: ContextSidebarTool) -> usize {
    // Keep indices aligned with the stable render order in the scroll strip.
    match tool {
        ContextSidebarTool::Monitor => 0,
        ContextSidebarTool::Processes => 1,
        ContextSidebarTool::Services => 2,
        ContextSidebarTool::Logs => 3,
        ContextSidebarTool::Tmux => 4,
        ContextSidebarTool::Docker => 5,
        ContextSidebarTool::Ports => 6,
        ContextSidebarTool::Schedules => 7,
        ContextSidebarTool::Filesystems => 8,
        ContextSidebarTool::Packages => 9,
    }
}

fn host_tools_tab_selection_geometry_from_bounds(
    viewport: Bounds<Pixels>,
    scroll_offset: Point<Pixels>,
    item: Bounds<Pixels>,
) -> HostToolsTabSelectionGeometry {
    HostToolsTabSelectionGeometry {
        // The overlay is outside the scroll owner, so include its current
        // offset when converting measured content bounds into viewport space.
        left: f32::from(item.origin.x - viewport.origin.x + scroll_offset.x),
        top: f32::from(item.origin.y - viewport.origin.y + scroll_offset.y),
        width: f32::from(item.size.width),
        height: f32::from(item.size.height),
    }
}

fn host_tools_tab_selection_geometry(
    scroll_handle: &ScrollHandle,
    item_index: usize,
) -> Option<HostToolsTabSelectionGeometry> {
    let item = scroll_handle.bounds_for_item(item_index)?;
    Some(host_tools_tab_selection_geometry_from_bounds(
        scroll_handle.bounds(),
        scroll_handle.offset(),
        item,
    ))
}

fn interpolate_host_tools_tab_selection_geometry(
    source: HostToolsTabSelectionGeometry,
    target: HostToolsTabSelectionGeometry,
    progress: f32,
) -> HostToolsTabSelectionGeometry {
    HostToolsTabSelectionGeometry {
        left: oxideterm_gpui_ui::motion::lerp(source.left, target.left, progress),
        top: oxideterm_gpui_ui::motion::lerp(source.top, target.top, progress),
        // Width is measured and interpolated independently because localized
        // tab labels do not share a fixed cell width.
        width: oxideterm_gpui_ui::motion::lerp(source.width, target.width, progress),
        height: oxideterm_gpui_ui::motion::lerp(source.height, target.height, progress),
    }
}

// Each Host Tools module owns its complete UI and request lifecycle.
#[path = "health/docker.rs"]
mod docker;
#[path = "health/filesystems.rs"]
mod filesystems;
#[path = "health/logs.rs"]
mod logs;
#[path = "health/monitor.rs"]
mod monitor;
#[path = "health/packages.rs"]
mod packages;
#[path = "health/ports.rs"]
mod ports;
#[path = "health/process.rs"]
mod process;
#[path = "health/scheduled_tasks.rs"]
mod scheduled_tasks;
#[path = "health/services.rs"]
mod services;
#[path = "health/tmux.rs"]
mod tmux;

impl WorkspaceApp {
    // Keep Host Tools filter chips visually consistent across resource panels.
    fn host_tools_filter_chip(&self, active: bool) -> Div {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .h(px(self.tokens.metrics.ui_button_sm_height * 0.75))
            .px(px(self.tokens.spacing.two))
            .flex()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
    }

    pub(in crate::workspace) fn render_host_tools_context_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active_tool = self.active_context_sidebar_tool;
        let content = match active_tool {
            ContextSidebarTool::Monitor => self.render_host_tools_monitor_panel(cx),
            ContextSidebarTool::Processes => self.render_host_processes_panel(cx),
            ContextSidebarTool::Services => self.render_host_services_panel(cx),
            ContextSidebarTool::Logs => self.render_host_logs_panel(cx),
            ContextSidebarTool::Tmux => self.render_host_tmux_panel(cx),
            ContextSidebarTool::Docker => self.render_host_docker_panel(cx),
            ContextSidebarTool::Ports => self.render_host_ports_panel(cx),
            ContextSidebarTool::Schedules => self.render_host_schedules_panel(cx),
            ContextSidebarTool::Filesystems => self.render_host_filesystems_panel(cx),
            ContextSidebarTool::Packages => self.render_host_packages_panel(cx),
        };
        let content = oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            SharedString::from(format!("host-tools-page-{active_tool:?}")),
            div()
                .w_full()
                .min_w_0()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(content),
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        div()
            .id("host-tools-context-panel")
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_hidden()
            .bg(self.context_sidebar_content_background(theme.bg))
            .text_color(rgb(theme.text))
            .child(self.render_host_tools_context_tabs(cx))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    // Only the secondary tab strip may own horizontal scroll.
                    // Keep tool bodies clipped to the companion-sidebar width.
                    .overflow_hidden()
                    .child(content),
            )
            .into_any_element()
    }

    fn render_host_tools_monitor_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("system-health-context-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .occlude()
            .child(
                div()
                    .size_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .px_3()
                    .py_3()
                    // Host Tools owns the secondary navigation; monitoring
                    // keeps the existing health panel behavior inside it.
                    .child(self.render_system_health_panel(true, cx)),
            )
            .into_any_element()
    }

    fn render_host_tools_context_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_index = host_tools_tab_index(self.active_context_sidebar_tool);
        let selection_geometry =
            host_tools_tab_selection_geometry(&self.host_tools_tab_scroll_handle, active_index);
        let selection_indicator = selection_geometry
            .map(|geometry| self.render_host_tools_tab_selection_indicator(geometry));
        let selection_indicator_visible = selection_indicator.is_some();
        let mut tabs = div()
            .id("host-tools-tab-scroll-viewport")
            .size_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_3()
            .pt_2()
            .pb_3()
            // Match the main tabbar scroll model: the strip clips its own
            // children and maps wheel movement to horizontal offset, while the
            // thin visible thumb keeps hidden tab overflow discoverable.
            .occlude()
            .overflow_x_scroll()
            .track_scroll(&self.host_tools_tab_scroll_handle)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                this.handle_host_tools_tab_scroll(event, window, cx);
            }));

        tabs = tabs
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Monitor,
                LucideIcon::Activity,
                "sidebar.panels.host_monitor",
                true,
                selection_indicator_visible,
                cx,
            ))
            // These entries reserve the host-tools IA before their backends land.
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Processes,
                LucideIcon::ListChecks,
                "sidebar.panels.processes",
                true,
                selection_indicator_visible,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Services,
                LucideIcon::Wrench,
                "sidebar.panels.services",
                true,
                selection_indicator_visible,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Logs,
                LucideIcon::FileText,
                "sidebar.panels.logs",
                true,
                selection_indicator_visible,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Tmux,
                LucideIcon::Terminal,
                "sidebar.panels.tmux",
                true,
                selection_indicator_visible,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Docker,
                LucideIcon::Layers,
                "sidebar.panels.docker",
                true,
                selection_indicator_visible,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Ports,
                LucideIcon::Network,
                "sidebar.panels.ports",
                true,
                selection_indicator_visible,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Schedules,
                LucideIcon::Clock,
                "sidebar.panels.schedules",
                true,
                selection_indicator_visible,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Filesystems,
                LucideIcon::HardDrive,
                "sidebar.panels.filesystems",
                true,
                selection_indicator_visible,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Packages,
                LucideIcon::Archive,
                "sidebar.panels.packages",
                true,
                selection_indicator_visible,
                cx,
            ));

        div()
            .id("host-tools-tab-strip")
            .flex_none()
            .w_full()
            .h(px(HOST_TOOLS_TAB_STRIP_HEIGHT))
            .min_w_0()
            .relative()
            .overflow_hidden()
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
            // Keep the moving surface below every icon and label without
            // changing the tracked scroll owner's direct-child indices.
            .when_some(selection_indicator, |strip, indicator| {
                strip.child(indicator)
            })
            .child(tabs)
            .child(self.render_host_tools_tab_scrollbar(cx))
            .into_any_element()
    }

    fn render_host_tools_tab_selection_indicator(
        &self,
        target: HostToolsTabSelectionGeometry,
    ) -> AnyElement {
        let surface = div()
            .absolute()
            .left(px(target.left))
            .top(px(target.top))
            .w(px(target.width))
            .h(px(target.height))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel));
        // Host Tools keeps its compact geometry but shares the selected-card
        // chrome established by Settings navigation.
        let surface = oxideterm_gpui_ui::theme_card_surface_shadow(surface, &self.tokens);
        let active_index = host_tools_tab_index(self.active_context_sidebar_tool);
        let Some((generation, _)) = self.segmented_control_user_transition(
            selection_motion::HOST_TOOLS_SWITCHER_ID,
            active_index,
        ) else {
            return surface.into_any_element();
        };
        let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) else {
            return surface.into_any_element();
        };
        let animation_id = (
            gpui::ElementId::from(selection_motion::HOST_TOOLS_SWITCHER_ID),
            format!("selection-{generation}"),
        );
        let source_index =
            host_tools_tab_index(self.connection_monitor.previous_context_sidebar_tool);
        if motion.spatial
            && let Some(source) =
                host_tools_tab_selection_geometry(&self.host_tools_tab_scroll_handle, source_index)
        {
            return surface
                .with_animation(
                    animation_id,
                    Animation::new(motion.duration)
                        .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                    move |surface, progress| {
                        let geometry =
                            interpolate_host_tools_tab_selection_geometry(source, target, progress);
                        surface
                            .left(px(geometry.left))
                            .top(px(geometry.top))
                            .w(px(geometry.width))
                            .h(px(geometry.height))
                    },
                )
                .into_any_element();
        }

        surface
            .with_animation(
                animation_id,
                Animation::new(motion.duration)
                    .with_easing(oxideterm_gpui_ui::motion::ease_out_cubic),
                |surface, progress| surface.opacity(progress),
            )
            .into_any_element()
    }

    fn render_host_tools_tab_scrollbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(geometry) = self.host_tools_tab_scrollbar_geometry() else {
            return div().into_any_element();
        };

        // Tauri's tab-strip scrollbar uses a 3px thin thumb; the GPUI component
        // `Always` mode paints a 16px hit area, so this surface keeps the thin
        // visual while adding an invisible drag target around it.
        div()
            .id("host-tools-tab-thin-scrollbar")
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(HOST_TOOLS_TAB_SCROLLBAR_BOTTOM_INSET))
            .h(px(HOST_TOOLS_TAB_SCROLLBAR_DRAG_HEIGHT))
            .cursor(CursorStyle::OpenHand)
            // Own the thin track before individual Host Tools tabs see the press.
            .occlude()
            .bg(rgba(0x00000000))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.start_host_tools_tab_scrollbar_drag(event, cx);
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(geometry.thumb_left))
                    .bottom_0()
                    .w(px(geometry.thumb_width))
                    .h(px(HOST_TOOLS_TAB_SCROLLBAR_HEIGHT))
                    .rounded(px(HOST_TOOLS_TAB_SCROLLBAR_RADIUS))
                    .bg(rgba(
                        (self.tokens.ui.text_muted << 8) | HOST_TOOLS_TAB_SCROLLBAR_ALPHA,
                    )),
            )
            .into_any_element()
    }

    fn host_tools_tab_scrollbar_geometry(&self) -> Option<HostToolsTabScrollbarGeometry> {
        let viewport_bounds = self.host_tools_tab_scroll_handle.bounds();
        let viewport_width = f32::from(viewport_bounds.size.width);
        let max_scroll = f32::from(self.host_tools_tab_scroll_handle.max_offset().width);
        let track_width =
            (viewport_width - HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET * 2.0).max(0.0);
        if viewport_width <= 1.0 || max_scroll <= 1.0 || track_width <= 1.0 {
            return None;
        }

        let content_width = viewport_width + max_scroll;
        let min_thumb_width = HOST_TOOLS_TAB_SCROLLBAR_MIN_THUMB_WIDTH.min(track_width);
        let thumb_width = (viewport_width / content_width * track_width)
            .max(min_thumb_width)
            .min(track_width);
        if track_width - thumb_width <= 1.0 {
            return None;
        }
        let scroll_x = self.current_host_tools_tab_scroll_x();
        let thumb_left = HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET
            + (scroll_x / max_scroll * (track_width - thumb_width).max(0.0));
        Some(HostToolsTabScrollbarGeometry {
            viewport_left: f32::from(viewport_bounds.origin.x),
            track_width,
            thumb_width,
            thumb_left,
            max_scroll,
        })
    }

    pub(in crate::workspace) fn host_tools_tab_scrollbar_drag_active(&self) -> bool {
        // The root capture registry cannot access ConnectionMonitorState's private drag field.
        self.connection_monitor.tab_scrollbar_drag.is_some()
    }

    fn current_host_tools_tab_scroll_x(&self) -> f32 {
        let max_scroll = f32::from(self.host_tools_tab_scroll_handle.max_offset().width);
        f32::from(-self.host_tools_tab_scroll_handle.offset().x).clamp(0.0, max_scroll)
    }

    fn set_host_tools_tab_scroll_x(&mut self, scroll_x: f32, cx: &mut Context<Self>) {
        let max_scroll = f32::from(self.host_tools_tab_scroll_handle.max_offset().width);
        let next_scroll_x = scroll_x.clamp(0.0, max_scroll);
        let current_scroll_x = self.current_host_tools_tab_scroll_x();
        if (next_scroll_x - current_scroll_x).abs() < 0.01 {
            return;
        }
        self.host_tools_tab_scroll_handle
            .set_offset(Point::new(px(-next_scroll_x), px(0.0)));
        cx.notify();
    }

    fn start_host_tools_tab_scrollbar_drag(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(geometry) = self.host_tools_tab_scrollbar_geometry() else {
            return;
        };
        let pointer_x = f32::from(event.position.x) - geometry.viewport_left;
        let track_left = HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET;
        let track_right = track_left + geometry.track_width;
        let thumb_right = geometry.thumb_left + geometry.thumb_width;
        let grab_offset_x = if pointer_x >= geometry.thumb_left && pointer_x <= thumb_right {
            pointer_x - geometry.thumb_left
        } else {
            geometry.thumb_width / 2.0
        };
        self.connection_monitor.tab_scrollbar_drag =
            Some(HostToolsTabScrollbarDragState { grab_offset_x });
        let thumb_left =
            (pointer_x - grab_offset_x).clamp(track_left, track_right - geometry.thumb_width);
        let ratio = (thumb_left - track_left) / (geometry.track_width - geometry.thumb_width);
        self.set_host_tools_tab_scroll_x(ratio * geometry.max_scroll, cx);
        cx.stop_propagation();
    }

    pub(in crate::workspace) fn update_host_tools_tab_scrollbar_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.connection_monitor.tab_scrollbar_drag else {
            return;
        };
        if !event.dragging() {
            self.finish_host_tools_tab_scrollbar_drag(cx);
            return;
        }
        let Some(geometry) = self.host_tools_tab_scrollbar_geometry() else {
            self.finish_host_tools_tab_scrollbar_drag(cx);
            return;
        };
        let pointer_x = f32::from(event.position.x) - geometry.viewport_left;
        let track_left = HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET;
        let max_thumb_left = track_left + geometry.track_width - geometry.thumb_width;
        let thumb_left = (pointer_x - drag.grab_offset_x).clamp(track_left, max_thumb_left);
        let ratio = (thumb_left - track_left) / (geometry.track_width - geometry.thumb_width);
        self.set_host_tools_tab_scroll_x(ratio * geometry.max_scroll, cx);
        cx.stop_propagation();
    }

    pub(in crate::workspace) fn finish_host_tools_tab_scrollbar_drag(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.tab_scrollbar_drag.take().is_some() {
            cx.notify();
        }
    }

    fn handle_host_tools_tab_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let max_scroll = f32::from(self.host_tools_tab_scroll_handle.max_offset().width);
        if max_scroll <= 1.0 {
            if self.host_tools_tab_scroll_handle.offset().x != px(0.0) {
                self.host_tools_tab_scroll_handle
                    .set_offset(Point::new(px(0.0), px(0.0)));
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        let delta = event.delta.pixel_delta(px(HOST_TOOLS_TAB_STRIP_HEIGHT));
        let delta_x = f32::from(delta.x);
        let delta_y = f32::from(delta.y);
        let scroll_delta = if delta_y != 0.0 { delta_y } else { delta_x };
        if scroll_delta == 0.0 {
            return;
        }

        let current_scroll_x = self.current_host_tools_tab_scroll_x();
        let next_scroll_x = (current_scroll_x - scroll_delta).clamp(0.0, max_scroll);
        if (next_scroll_x - current_scroll_x).abs() < 0.01 {
            cx.stop_propagation();
            return;
        }

        self.set_host_tools_tab_scroll_x(next_scroll_x, cx);
        cx.stop_propagation();
    }

    fn render_host_tools_context_tab(
        &self,
        tool: ContextSidebarTool,
        icon: LucideIcon,
        label_key: &'static str,
        enabled: bool,
        selection_indicator_visible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_context_sidebar_tool == tool;
        let tab = div()
            .h(px(28.0))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .rounded(px(self.tokens.radii.md))
            .cursor(if enabled {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .opacity(if enabled { 1.0 } else { 0.45 })
            .bg(rgba(0x00000000))
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            });
        // Before tracked bounds exist, paint the same selected card directly
        // on the tab so the first frame does not fall back to older chrome.
        let tab = if active && !selection_indicator_visible {
            let tab = tab
                .border_1()
                .border_color(rgb(theme.border))
                .bg(self.settings_panel_background(theme.bg_panel));
            oxideterm_gpui_ui::theme_card_surface_shadow(tab, &self.tokens)
        } else {
            tab
        };
        tab.hover(move |tab| {
            if enabled && !active {
                tab.bg(rgb(theme.bg_hover))
            } else {
                tab
            }
        })
        .child(Self::render_lucide_icon(
            icon,
            13.0,
            if active {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            },
        ))
        .child(
            div()
                .text_size(px(12.0))
                .whitespace_nowrap()
                .truncate()
                .child(self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "host-tools-tab",
                    label_key,
                    self.i18n.t(label_key),
                    if active { theme.text } else { theme.text_muted },
                    cx,
                )),
        )
        .when(enabled, |tab| {
            tab.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.active_context_sidebar_tool != tool {
                        this.connection_monitor.previous_context_sidebar_tool =
                            this.active_context_sidebar_tool;
                        this.active_context_sidebar_tool = tool;
                        this.begin_user_segmented_control_transition(
                            selection_motion::HOST_TOOLS_SWITCHER_ID,
                            host_tools_tab_index(tool),
                            cx,
                        );
                        if tool != ContextSidebarTool::Processes {
                            this.connection_monitor.host_process_search_focused = false;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        if tool != ContextSidebarTool::Docker {
                            this.connection_monitor.host_docker_search_focused = false;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        if tool != ContextSidebarTool::Services {
                            this.connection_monitor.host_service_search_focused = false;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        if tool != ContextSidebarTool::Logs {
                            this.connection_monitor.host_log_search_focused = false;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        if tool != ContextSidebarTool::Tmux {
                            this.connection_monitor.host_tmux_search_focused = false;
                            this.connection_monitor.host_tmux_pending_confirm = None;
                            this.connection_monitor.host_tmux_input_dialog = None;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        if tool != ContextSidebarTool::Ports {
                            this.connection_monitor.host_port_search_focused = false;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        if tool != ContextSidebarTool::Schedules {
                            this.connection_monitor.host_schedule_search_focused = false;
                            this.connection_monitor.host_schedule_pending_confirm = None;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        if tool != ContextSidebarTool::Filesystems {
                            this.connection_monitor.host_filesystem_search_focused = false;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        if tool != ContextSidebarTool::Packages {
                            this.connection_monitor.host_package_search_focused = false;
                            this.clear_ime_selection();
                            this.ime_marked_text = None;
                        }
                        // Switching Host Tools pages should eagerly attach
                        // the selected connection profiler. Waiting for the
                        // heartbeat made data appear only after another
                        // layout event, such as entering fullscreen.
                        this.refresh_connection_monitor_pool_stats();
                        this.sync_connection_monitor_selection(cx);
                        if tool == ContextSidebarTool::Logs {
                            this.request_host_logs_snapshot_for_selected_connection(cx);
                        }
                        if tool == ContextSidebarTool::Tmux {
                            this.request_host_tmux_snapshot_for_selected_connection(cx);
                        }
                        if tool == ContextSidebarTool::Ports {
                            this.request_host_ports_snapshot_for_selected_connection(cx);
                        }
                        if tool == ContextSidebarTool::Schedules {
                            this.request_host_schedules_snapshot_for_selected_connection(cx);
                        }
                        if tool == ContextSidebarTool::Filesystems {
                            this.request_host_filesystems_snapshot_for_selected_connection(cx);
                        }
                        if tool == ContextSidebarTool::Packages {
                            this.request_host_packages_snapshot_for_selected_connection(cx);
                        }
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
        })
        .into_any_element()
    }

    fn render_connection_switcher_row(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        is_running: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(connection) = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .or_else(|| connections.first())
        else {
            return div().into_any_element();
        };

        let theme = self.tokens.ui;
        let selected_index = monitor_connection_selected_index(connections, selected_id);
        let can_switch = monitor_connection_can_switch(connections);
        let focus_visible = browser_behavior::browser_focus_visible(
            self.connection_monitor.selector_focus_origin.is_some(),
            self.connection_monitor.selector_focus_origin,
        );
        // This is a host identity row first and a selector only when multiple
        // live hosts exist. Keeping it visually inline avoids the old form-field
        // dropdown sitting between the tabs and each Host Tools page.
        let selector_bottom_margin = if can_switch && self.connection_monitor.selector_open {
            let visible_options = connections
                .len()
                .max(1)
                .min(SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS)
                as f32;
            SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y
                + (visible_options * SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT)
                + (SYSTEM_HEALTH_SELECTOR_GAP * 2.0)
        } else {
            0.0
        };
        let mut trigger = div()
            .h(px(HOST_TOOLS_CONNECTION_ROW_HEIGHT))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .px_1()
            .rounded(px(self.tokens.radii.md))
            .when(can_switch, |row| row.cursor_pointer())
            .when(
                can_switch && (self.connection_monitor.selector_open || focus_visible),
                |row| row.bg(rgba((theme.bg_panel << 8) | MONITOR_TINT_ALPHA)),
            )
            .when(can_switch, |row| {
                row.hover(|hovered| hovered.bg(rgba((theme.bg_panel << 8) | MONITOR_TINT_ALPHA)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.connection_monitor.selector_focus_origin =
                                Some(browser_behavior::BrowserFocusOrigin::Pointer);
                            if this.connection_monitor.selector_open {
                                this.connection_monitor.selector_open = false;
                                this.connection_monitor.selector_highlighted_index = None;
                            } else {
                                let connections = this.monitor_connections();
                                let selected_id = this
                                    .connection_monitor
                                    .selected_connection_id
                                    .as_deref()
                                    .unwrap_or_else(|| {
                                        connections
                                            .first()
                                            .map(|connection| connection.connection_id.as_str())
                                            .unwrap_or_default()
                                    });
                                this.connection_monitor.selector_highlighted_index = Some(
                                    monitor_connection_selected_index(&connections, selected_id),
                                );
                                this.connection_monitor.selector_open = true;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            })
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                14.0,
                if is_running {
                    rgb(MONITOR_EMERALD)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .whitespace_nowrap()
                    .text_size(px(13.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(theme.text))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "host-tools-connection-endpoint",
                        connection.connection_id.as_str(),
                        monitor_connection_label(connection),
                        theme.text,
                        cx,
                    )),
            );
        if can_switch {
            trigger = trigger.child(div().flex_none().opacity(0.75).child(
                Self::render_lucide_icon(LucideIcon::ChevronDown, 14.0, rgb(theme.text_muted)),
            ));
        }

        let mut wrapper = div()
            .relative()
            .mb(px(selector_bottom_margin))
            .child(trigger);
        if can_switch && self.connection_monitor.selector_open {
            let highlighted = self
                .connection_monitor
                .selector_highlighted_index
                .unwrap_or(selected_index);
            let mut popup = select_event_boundary(
                div()
                    .absolute()
                    .top(px(
                        HOST_TOOLS_CONNECTION_ROW_HEIGHT + SYSTEM_HEALTH_SELECTOR_GAP
                    ))
                    .left_0()
                    .right_0()
                    .overflow_hidden()
                    .max_h(px(SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y
                        + (SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS as f32
                            * SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT)))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .p_1()
                    .shadow_lg(),
            );
            for (index, connection) in connections.iter().enumerate() {
                let connection_id = connection.connection_id.clone();
                let selected = connection.connection_id == selected_id;
                let highlighted = highlighted == index;
                popup = popup.child(select_option_action(
                    select_option_highlighted(
                        &self.tokens,
                        monitor_connection_label(connection),
                        selected,
                        highlighted,
                    )
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                        if this.connection_monitor.selector_highlighted_index != Some(index) {
                            this.connection_monitor.selector_highlighted_index = Some(index);
                            cx.notify();
                        }
                    }))
                    .child(div().mr_2().child(Self::render_lucide_icon(
                        LucideIcon::Server,
                        14.0,
                        rgb(self.tokens.ui.text_muted),
                    ))),
                    false,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.connection_monitor.selected_connection_id =
                            Some(connection_id.clone());
                        this.connection_monitor.selector_open = false;
                        this.connection_monitor.selector_highlighted_index = None;
                        this.connection_monitor.selector_focus_origin = None;
                        this.connection_monitor.host_tmux_pending_confirm = None;
                        this.connection_monitor.host_tmux_input_dialog = None;
                        this.connection_monitor.host_schedule_pending_confirm = None;
                        this.sync_connection_monitor_selection(cx);
                        if this.active_context_sidebar_tool == ContextSidebarTool::Logs {
                            this.request_host_logs_snapshot(
                                connection_id.clone(),
                                HostSnapshotFeedback::Silent,
                                cx,
                            );
                        }
                        if this.active_context_sidebar_tool == ContextSidebarTool::Tmux {
                            this.request_host_tmux_snapshot(
                                connection_id.clone(),
                                HostSnapshotFeedback::Silent,
                                cx,
                            );
                        }
                        if this.active_context_sidebar_tool == ContextSidebarTool::Ports {
                            this.request_host_ports_snapshot(
                                connection_id.clone(),
                                HostSnapshotFeedback::Silent,
                                cx,
                            );
                        }
                        if this.active_context_sidebar_tool == ContextSidebarTool::Schedules {
                            this.request_host_schedules_snapshot(
                                connection_id.clone(),
                                HostSnapshotFeedback::Silent,
                                cx,
                            );
                        }
                        if this.active_context_sidebar_tool == ContextSidebarTool::Filesystems {
                            this.request_host_filesystems_snapshot(
                                connection_id.clone(),
                                HostSnapshotFeedback::Silent,
                                cx,
                            );
                        }
                        if this.active_context_sidebar_tool == ContextSidebarTool::Packages {
                            this.request_host_packages_snapshot(
                                connection_id.clone(),
                                HostSnapshotFeedback::Silent,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }),
                ));
            }
            wrapper = wrapper.child(popup);
        }
        wrapper.into_any_element()
    }

    pub(in crate::workspace) fn handle_connection_monitor_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let connections = self.monitor_connections();
        if !monitor_connection_can_switch(&connections) {
            self.connection_monitor.selector_open = false;
            self.connection_monitor.selector_highlighted_index = None;
            self.connection_monitor.selector_focus_origin = None;
            return false;
        }
        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let selected_index = monitor_connection_selected_index(&connections, selected_id);
        let current = self
            .connection_monitor
            .selector_highlighted_index
            .unwrap_or(selected_index);

        if self.connection_monitor.selector_open {
            return self.handle_open_connection_monitor_select_key(
                event,
                &connections,
                current,
                cx,
            );
        }

        match event.keystroke.key.as_str() {
            "tab" => {
                // Tauri/Radix exposes the select trigger as a keyboard tab stop.
                // Native has no DOM focus chain, so the monitor page owns that
                // first trigger focus explicitly.
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "enter" | "space" | " " | "arrowdown" | "down"
                if self.connection_monitor.selector_focus_origin.is_some() =>
            {
                self.connection_monitor.selector_open = true;
                self.connection_monitor.selector_highlighted_index = Some(selected_index);
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "escape" if self.connection_monitor.selector_focus_origin.is_some() => {
                self.connection_monitor.selector_focus_origin = None;
                self.connection_monitor.selector_highlighted_index = None;
                cx.notify();
                true
            }
            _ => false,
        }
    }

    fn handle_open_connection_monitor_select_key(
        &mut self,
        event: &KeyDownEvent,
        connections: &[MonitorConnectionOption],
        current: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.connection_monitor.selector_open = false;
                self.connection_monitor.selector_highlighted_index = None;
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "tab" => {
                self.connection_monitor.selector_open = false;
                self.connection_monitor.selector_highlighted_index = None;
                self.connection_monitor.selector_focus_origin = None;
                cx.notify();
                true
            }
            "arrowdown" | "down" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(browser_behavior::browser_select_next_index(
                        current,
                        connections.len(),
                        browser_behavior::BrowserSelectKeyDirection::Next,
                    ));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "arrowup" | "up" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(browser_behavior::browser_select_next_index(
                        current,
                        connections.len(),
                        browser_behavior::BrowserSelectKeyDirection::Previous,
                    ));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "home" => {
                self.connection_monitor.selector_highlighted_index = Some(0);
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "end" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(connections.len().saturating_sub(1));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "enter" | "space" | " " => {
                if let Some(connection) = connections.get(current.min(connections.len() - 1)) {
                    self.connection_monitor.selected_connection_id =
                        Some(connection.connection_id.clone());
                    self.connection_monitor.selector_open = false;
                    self.connection_monitor.selector_highlighted_index = None;
                    self.connection_monitor.selector_focus_origin =
                        Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                    self.connection_monitor.host_tmux_pending_confirm = None;
                    self.connection_monitor.host_tmux_input_dialog = None;
                    self.connection_monitor.host_schedule_pending_confirm = None;
                    self.sync_connection_monitor_selection(cx);
                    if self.active_context_sidebar_tool == ContextSidebarTool::Logs {
                        self.request_host_logs_snapshot(
                            connection.connection_id.clone(),
                            HostSnapshotFeedback::Silent,
                            cx,
                        );
                    }
                    if self.active_context_sidebar_tool == ContextSidebarTool::Tmux {
                        self.request_host_tmux_snapshot(
                            connection.connection_id.clone(),
                            HostSnapshotFeedback::Silent,
                            cx,
                        );
                    }
                    if self.active_context_sidebar_tool == ContextSidebarTool::Ports {
                        self.request_host_ports_snapshot(
                            connection.connection_id.clone(),
                            HostSnapshotFeedback::Silent,
                            cx,
                        );
                    }
                    if self.active_context_sidebar_tool == ContextSidebarTool::Schedules {
                        self.request_host_schedules_snapshot(
                            connection.connection_id.clone(),
                            HostSnapshotFeedback::Silent,
                            cx,
                        );
                    }
                    if self.active_context_sidebar_tool == ContextSidebarTool::Filesystems {
                        self.request_host_filesystems_snapshot(
                            connection.connection_id.clone(),
                            HostSnapshotFeedback::Silent,
                            cx,
                        );
                    }
                    if self.active_context_sidebar_tool == ContextSidebarTool::Packages {
                        self.request_host_packages_snapshot(
                            connection.connection_id.clone(),
                            HostSnapshotFeedback::Silent,
                            cx,
                        );
                    }
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Size;

    #[test]
    fn host_tools_tab_indices_match_render_order() {
        let tools = [
            ContextSidebarTool::Monitor,
            ContextSidebarTool::Processes,
            ContextSidebarTool::Services,
            ContextSidebarTool::Logs,
            ContextSidebarTool::Tmux,
            ContextSidebarTool::Docker,
            ContextSidebarTool::Ports,
            ContextSidebarTool::Schedules,
            ContextSidebarTool::Filesystems,
            ContextSidebarTool::Packages,
        ];

        for (expected_index, tool) in tools.into_iter().enumerate() {
            assert_eq!(host_tools_tab_index(tool), expected_index);
        }
    }

    #[test]
    fn host_tools_tab_geometry_accounts_for_viewport_scroll() {
        let viewport = Bounds {
            origin: Point::new(px(100.0), px(20.0)),
            size: Size::new(px(320.0), px(48.0)),
        };
        let item = Bounds {
            origin: Point::new(px(148.0), px(28.0)),
            size: Size::new(px(76.0), px(28.0)),
        };

        assert_eq!(
            host_tools_tab_selection_geometry_from_bounds(
                viewport,
                Point::new(px(-36.0), px(0.0)),
                item,
            ),
            HostToolsTabSelectionGeometry {
                left: 12.0,
                top: 8.0,
                width: 76.0,
                height: 28.0,
            }
        );
    }

    #[test]
    fn host_tools_tab_geometry_interpolates_position_and_width() {
        let source = HostToolsTabSelectionGeometry {
            left: 12.0,
            top: 8.0,
            width: 100.0,
            height: 28.0,
        };
        let target = HostToolsTabSelectionGeometry {
            left: 124.0,
            top: 8.0,
            width: 76.0,
            height: 28.0,
        };

        assert_eq!(
            interpolate_host_tools_tab_selection_geometry(source, target, 0.5),
            HostToolsTabSelectionGeometry {
                left: 68.0,
                top: 8.0,
                width: 88.0,
                height: 28.0,
            }
        );
    }
}
