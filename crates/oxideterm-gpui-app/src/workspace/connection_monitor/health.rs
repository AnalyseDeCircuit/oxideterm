use super::*;

use gpui::Div;
use oxideterm_gpui_ui::select::{
    select_event_boundary, select_option_action, select_option_highlighted,
};
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};

use crate::workspace::selectable_text::{SelectableTextRenderState, selectable_document_group_id};

const HOST_TOOLS_CONNECTION_ROW_HEIGHT: f32 = 32.0;
const HOST_TOOLS_SELECTOR_OPTION_HEIGHT: f32 = 36.0;
const HOST_TOOLS_SELECTOR_MENU_PADDING_Y: f32 = 8.0;
const HOST_TOOLS_SELECTOR_VISIBLE_OPTIONS: usize = 4;
const HOST_TOOLS_SELECTOR_GAP: f32 = 8.0;
const HOST_TOOLS_TAB_STRIP_HEIGHT: f32 = 48.0;
const HOST_TOOLS_TAB_SCROLLBAR_HEIGHT: f32 = 3.0;
const HOST_TOOLS_TAB_SCROLLBAR_BOTTOM_INSET: f32 = 5.0;
const HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET: f32 = 12.0;
const HOST_TOOLS_TAB_SCROLLBAR_MIN_THUMB_WIDTH: f32 = 32.0;
const HOST_TOOLS_TAB_SCROLLBAR_RADIUS: f32 = 2.0;
const HOST_TOOLS_TAB_SCROLLBAR_ALPHA: u32 = 0x66;
const HOST_TOOLS_TAB_SCROLLBAR_DRAG_HEIGHT: f32 = 12.0;
const HOST_TOOLS_MONITOR_TOGGLE_WIDTH: f32 = 36.0;

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

pub(in crate::workspace) fn host_tools_tab_index(tool: ContextSidebarTool) -> usize {
    // Keep indices aligned with the stable render order in the scroll strip.
    match tool {
        ContextSidebarTool::Monitor => 0,
        ContextSidebarTool::Gpu => 1,
        ContextSidebarTool::Processes => 2,
        ContextSidebarTool::Services => 3,
        ContextSidebarTool::Logs => 4,
        ContextSidebarTool::Tmux => 5,
        ContextSidebarTool::Docker => 6,
        ContextSidebarTool::Ports => 7,
        ContextSidebarTool::Schedules => 8,
        ContextSidebarTool::Filesystems => 9,
        ContextSidebarTool::Packages => 10,
    }
}

impl ContextSidebarTool {
    pub(super) fn monitoring_enabled(
        self,
        settings: &oxideterm_settings::HostToolsSettings,
    ) -> bool {
        match self {
            Self::Monitor => settings.monitor_enabled,
            Self::Gpu => settings.gpu_enabled,
            Self::Processes => settings.processes_enabled,
            Self::Services => settings.services_enabled,
            Self::Logs => settings.logs_enabled,
            Self::Tmux => settings.tmux_enabled,
            Self::Docker => settings.docker_enabled,
            Self::Ports => settings.ports_enabled,
            Self::Schedules => settings.schedules_enabled,
            Self::Filesystems => settings.filesystems_enabled,
            Self::Packages => settings.packages_enabled,
        }
    }

    fn set_monitoring_enabled(
        self,
        settings: &mut oxideterm_settings::HostToolsSettings,
        enabled: bool,
    ) {
        // Keep persistence mapping next to the read mapping so new Host Tools cannot drift.
        match self {
            Self::Monitor => settings.monitor_enabled = enabled,
            Self::Gpu => settings.gpu_enabled = enabled,
            Self::Processes => settings.processes_enabled = enabled,
            Self::Services => settings.services_enabled = enabled,
            Self::Logs => settings.logs_enabled = enabled,
            Self::Tmux => settings.tmux_enabled = enabled,
            Self::Docker => settings.docker_enabled = enabled,
            Self::Ports => settings.ports_enabled = enabled,
            Self::Schedules => settings.schedules_enabled = enabled,
            Self::Filesystems => settings.filesystems_enabled = enabled,
            Self::Packages => settings.packages_enabled = enabled,
        }
    }
}

impl HostToolsEntity {
    fn render_connection_switcher(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        is_running: bool,
        tokens: &ThemeTokens,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(connection) = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .or_else(|| connections.first())
        else {
            return div().into_any_element();
        };

        let theme = tokens.ui;
        let selected_index = monitor_connection_selected_index(connections, selected_id);
        let can_switch = monitor_connection_can_switch(connections);
        let selector_focus_origin = self.selector_focus_origin();
        let focus_visible = browser_behavior::browser_focus_visible(
            selector_focus_origin.is_some(),
            selector_focus_origin,
        );
        // The selector owns its pointer state and connection transition. The
        // selectable text snapshot is a transient workspace text service and
        // is never retained by HostToolsEntity.
        let selector_bottom_margin = if can_switch && self.selector_open() {
            let visible_options = connections
                .len()
                .max(1)
                .min(HOST_TOOLS_SELECTOR_VISIBLE_OPTIONS) as f32;
            HOST_TOOLS_SELECTOR_MENU_PADDING_Y
                + (visible_options * HOST_TOOLS_SELECTOR_OPTION_HEIGHT)
                + (HOST_TOOLS_SELECTOR_GAP * 2.0)
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
            .rounded(px(tokens.radii.md))
            .when(can_switch, |row| row.cursor_pointer())
            .when(
                can_switch && (self.selector_open() || focus_visible),
                |row| row.bg(rgba((theme.bg_panel << 8) | MONITOR_TINT_ALPHA)),
            )
            .when(can_switch, |row| {
                row.hover(|hovered| hovered.bg(rgba((theme.bg_panel << 8) | MONITOR_TINT_ALPHA)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            host_tools.toggle_selector_from_pointer(selected_index, cx);
                            cx.stop_propagation();
                        }),
                    )
            })
            .child(WorkspaceApp::render_lucide_icon(
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
                    .font_family(mono_font_family.clone())
                    .text_color(rgb(theme.text))
                    .child(selectable_text.render_display_text_with_role_in_group(
                        SelectableTextRole::PlainDocument,
                        selectable_document_group_id(),
                        "host-tools-connection-endpoint",
                        connection.connection_id.as_str(),
                        0,
                        monitor_connection_label(connection),
                        theme.text,
                        cx,
                    )),
            );
        if can_switch {
            trigger = trigger.child(div().flex_none().opacity(0.75).child(
                WorkspaceApp::render_lucide_icon(
                    LucideIcon::ChevronDown,
                    14.0,
                    rgb(theme.text_muted),
                ),
            ));
        }

        let mut wrapper = div()
            .relative()
            .mb(px(selector_bottom_margin))
            .child(trigger);
        if can_switch && self.selector_open() {
            let highlighted = self.selector_highlighted_index().unwrap_or(selected_index);
            let mut popup = select_event_boundary(
                div()
                    .absolute()
                    .top(px(
                        HOST_TOOLS_CONNECTION_ROW_HEIGHT + HOST_TOOLS_SELECTOR_GAP
                    ))
                    .left_0()
                    .right_0()
                    .overflow_hidden()
                    .max_h(px(HOST_TOOLS_SELECTOR_MENU_PADDING_Y
                        + (HOST_TOOLS_SELECTOR_VISIBLE_OPTIONS as f32
                            * HOST_TOOLS_SELECTOR_OPTION_HEIGHT)))
                    .rounded(px(tokens.radii.md))
                    .border_1()
                    .border_color(rgb(tokens.ui.border))
                    .bg(rgb(tokens.ui.bg_panel))
                    .p_1()
                    .shadow_lg(),
            );
            for (index, connection) in connections.iter().enumerate() {
                let connection_id = connection.connection_id.clone();
                let selected = connection.connection_id == selected_id;
                let highlighted = highlighted == index;
                popup = popup.child(select_option_action(
                    select_option_highlighted(
                        tokens,
                        monitor_connection_label(connection),
                        selected,
                        highlighted,
                    )
                    .font_family(mono_font_family.clone())
                    .on_mouse_move(cx.listener(move |host_tools, _event, _window, cx| {
                        host_tools.highlight_selector_index(index, cx);
                    }))
                    .child(div().mr_2().child(
                        WorkspaceApp::render_lucide_icon(
                            LucideIcon::Server,
                            14.0,
                            rgb(tokens.ui.text_muted),
                        ),
                    )),
                    false,
                    false,
                    cx.listener(move |host_tools, _event, _window, cx| {
                        host_tools.select_connection_for_active_tool(
                            connection_id.clone(),
                            None,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ));
            }
            wrapper = wrapper.child(popup);
        }
        wrapper.into_any_element()
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
#[path = "health/gpu.rs"]
mod gpu;
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
    fn host_tool_monitoring_enabled(&self, tool: ContextSidebarTool) -> bool {
        tool.monitoring_enabled(&self.settings_store.settings().host_tools)
    }

    pub(in crate::workspace) fn set_host_tool_monitoring_enabled(
        &mut self,
        tool: ContextSidebarTool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            move |settings| tool.set_monitoring_enabled(&mut settings.host_tools, enabled),
            cx,
        );
        if !enabled && tool == ContextSidebarTool::Services {
            self.host_tools
                .update(cx, |host_tools, _cx| host_tools.pause_service_refreshes());
        }
        if enabled && self.host_tools.read(cx).active_tool() == tool {
            self.request_host_tool_snapshot_if_needed(tool, cx);
        }
    }

    fn request_host_tool_snapshot_if_needed(
        &mut self,
        tool: ContextSidebarTool,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            tool,
            ContextSidebarTool::Monitor
                | ContextSidebarTool::Gpu
                | ContextSidebarTool::Processes
                | ContextSidebarTool::Docker
        ) {
            return;
        }
        // Snapshot ownership stays in the Entity; the root only persists the
        // monitoring toggle that led to this request.
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
        });
    }

    pub(in crate::workspace) fn render_host_tools_context_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active_tool = self.host_tools.read(cx).active_tool();
        let enabled = self.host_tool_monitoring_enabled(active_tool);
        let content = if enabled {
            match active_tool {
                ContextSidebarTool::Monitor => self.render_host_tools_monitor_panel(cx),
                ContextSidebarTool::Gpu => {
                    let tokens = self.tokens;
                    let i18n = &self.i18n;
                    let mono_font_family =
                        settings_mono_font_family(self.settings_store.settings());
                    let selectable_text = self.selectable_text_render_state(cx);
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.render_host_gpu_panel(
                            &tokens,
                            i18n,
                            mono_font_family,
                            &selectable_text,
                            cx,
                        )
                    })
                }
                ContextSidebarTool::Processes => self.render_host_processes_panel(cx),
                ContextSidebarTool::Services => self.render_host_services_panel(cx),
                ContextSidebarTool::Logs => self.render_host_logs_panel(cx),
                ContextSidebarTool::Tmux => self.render_host_tmux_panel(cx),
                ContextSidebarTool::Docker => self.render_host_docker_panel(cx),
                ContextSidebarTool::Ports => self.render_host_ports_panel(cx),
                ContextSidebarTool::Schedules => self.render_host_schedules_panel(cx),
                ContextSidebarTool::Filesystems => self.render_host_filesystems_panel(cx),
                ContextSidebarTool::Packages => self.render_host_packages_panel(cx),
            }
        } else {
            self.render_host_tool_monitoring_disabled(active_tool, cx)
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
            .id("host-tools-monitor-context-panel")
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
                    // keeps monitor navigation and sampling inside it.
                    .child(self.render_host_monitor_panel(cx)),
            )
            .into_any_element()
    }

    fn render_host_tools_context_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_index = host_tools_tab_index(self.host_tools.read(cx).active_tool());
        let tab_scroll_handle = self.host_tools.read(cx).tab_scroll_handle();
        let selection_geometry =
            host_tools_tab_selection_geometry(&tab_scroll_handle, active_index);
        let selection_indicator = selection_geometry.map(|geometry| {
            self.render_host_tools_tab_selection_indicator(geometry, &tab_scroll_handle, cx)
        });
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
            .track_scroll(&tab_scroll_handle)
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
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Gpu,
                LucideIcon::Cpu,
                "sidebar.panels.gpu",
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

        let monitoring_enabled =
            self.host_tool_monitoring_enabled(self.host_tools.read(cx).active_tool());
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
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .right(px(HOST_TOOLS_MONITOR_TOGGLE_WIDTH))
                    .child(tabs),
            )
            .child(self.render_host_tool_monitoring_toggle(monitoring_enabled, cx))
            .child(self.render_host_tools_tab_scrollbar(cx))
            .into_any_element()
    }

    fn render_host_tool_monitoring_toggle(
        &self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tool = self.host_tools.read(cx).active_tool();
        let color = if enabled {
            MONITOR_EMERALD
        } else {
            self.tokens.ui.text_muted
        };
        div()
            .id("host-tool-monitoring-toggle")
            .absolute()
            .right_0()
            .top_0()
            .w(px(HOST_TOOLS_MONITOR_TOGGLE_WIDTH))
            .h(px(
                HOST_TOOLS_TAB_STRIP_HEIGHT - HOST_TOOLS_TAB_SCROLLBAR_HEIGHT
            ))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(rgb(color))
            .bg(self.context_sidebar_content_background(self.tokens.ui.bg))
            .hover(move |button| {
                if enabled {
                    button.bg(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA))
                } else {
                    button.bg(rgba((MONITOR_EMERALD_DARK << 8) | MONITOR_TINT_ALPHA))
                }
            })
            .child(Self::render_lucide_icon(
                LucideIcon::Power,
                14.0,
                rgb(color),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.set_host_tool_monitoring_enabled(tool, !enabled, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_tool_monitoring_disabled(
        &self,
        tool: ContextSidebarTool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = self.tokens.ui.text_muted;
        div()
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .text_align(gpui::TextAlign::Center)
            .text_color(rgb(color))
            .child(div().mb_2().opacity(0.3).child(Self::render_lucide_icon(
                LucideIcon::Power,
                24.0,
                rgb(color),
            )))
            .child(
                div()
                    .mb_3()
                    .text_size(px(14.0))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "host-tool-monitoring-disabled",
                        format!("{tool:?}"),
                        self.i18n.t("profiler.panel.disabled"),
                        color,
                        cx,
                    )),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "host-tool-monitoring-enable",
                        format!("{tool:?}"),
                        self.i18n.t("profiler.panel.enable"),
                        color,
                        cx,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_host_tool_monitoring_enabled(tool, true, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_host_tools_tab_selection_indicator(
        &self,
        target: HostToolsTabSelectionGeometry,
        tab_scroll_handle: &ScrollHandle,
        cx: &mut Context<Self>,
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
        let active_index = host_tools_tab_index(self.host_tools.read(cx).active_tool());
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
        let source_index = host_tools_tab_index(self.host_tools.read(cx).previous_tool());
        if motion.spatial
            && let Some(source) = host_tools_tab_selection_geometry(tab_scroll_handle, source_index)
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
        let tab_scroll_handle = self.host_tools.read(cx).tab_scroll_handle();
        let Some(geometry) = self.host_tools_tab_scrollbar_geometry(&tab_scroll_handle) else {
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

    fn host_tools_tab_scrollbar_geometry(
        &self,
        tab_scroll_handle: &ScrollHandle,
    ) -> Option<HostToolsTabScrollbarGeometry> {
        let viewport_bounds = tab_scroll_handle.bounds();
        let viewport_width = f32::from(viewport_bounds.size.width);
        let max_scroll = f32::from(tab_scroll_handle.max_offset().x);
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
        let scroll_x = self.current_host_tools_tab_scroll_x(tab_scroll_handle);
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

    pub(in crate::workspace) fn host_tools_tab_scrollbar_drag_active(
        &self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.host_tools.read(cx).tab_scrollbar_drag_active()
    }

    fn current_host_tools_tab_scroll_x(&self, tab_scroll_handle: &ScrollHandle) -> f32 {
        let max_scroll = f32::from(tab_scroll_handle.max_offset().x);
        f32::from(-tab_scroll_handle.offset().x).clamp(0.0, max_scroll)
    }

    fn set_host_tools_tab_scroll_x(&mut self, scroll_x: f32, cx: &mut Context<Self>) {
        let tab_scroll_handle = self.host_tools.read(cx).tab_scroll_handle();
        let max_scroll = f32::from(tab_scroll_handle.max_offset().x);
        let next_scroll_x = scroll_x.clamp(0.0, max_scroll);
        let current_scroll_x = self.current_host_tools_tab_scroll_x(&tab_scroll_handle);
        if (next_scroll_x - current_scroll_x).abs() < 0.01 {
            return;
        }
        tab_scroll_handle.set_offset(Point::new(px(-next_scroll_x), px(0.0)));
        cx.notify();
    }

    fn start_host_tools_tab_scrollbar_drag(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let tab_scroll_handle = self.host_tools.read(cx).tab_scroll_handle();
        let Some(geometry) = self.host_tools_tab_scrollbar_geometry(&tab_scroll_handle) else {
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
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_tab_scrollbar_drag(grab_offset_x, cx);
        });
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
        let Some(grab_offset_x) = self.host_tools.read(cx).tab_scrollbar_grab_offset() else {
            return;
        };
        if !event.dragging() {
            self.finish_host_tools_tab_scrollbar_drag(cx);
            return;
        }
        let tab_scroll_handle = self.host_tools.read(cx).tab_scroll_handle();
        let Some(geometry) = self.host_tools_tab_scrollbar_geometry(&tab_scroll_handle) else {
            self.finish_host_tools_tab_scrollbar_drag(cx);
            return;
        };
        let pointer_x = f32::from(event.position.x) - geometry.viewport_left;
        let track_left = HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET;
        let max_thumb_left = track_left + geometry.track_width - geometry.thumb_width;
        let thumb_left = (pointer_x - grab_offset_x).clamp(track_left, max_thumb_left);
        let ratio = (thumb_left - track_left) / (geometry.track_width - geometry.thumb_width);
        self.set_host_tools_tab_scroll_x(ratio * geometry.max_scroll, cx);
        cx.stop_propagation();
    }

    pub(in crate::workspace) fn finish_host_tools_tab_scrollbar_drag(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.finish_tab_scrollbar_drag(cx);
        });
    }

    fn handle_host_tools_tab_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_scroll_handle = self.host_tools.read(cx).tab_scroll_handle();
        let max_scroll = f32::from(tab_scroll_handle.max_offset().x);
        if max_scroll <= 1.0 {
            if tab_scroll_handle.offset().x != px(0.0) {
                tab_scroll_handle.set_offset(Point::new(px(0.0), px(0.0)));
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

        let current_scroll_x = self.current_host_tools_tab_scroll_x(&tab_scroll_handle);
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
        let active = self.host_tools.read(cx).active_tool() == tool;
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
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.select_sidebar_tool(tool, cx)
                    });
                    cx.stop_propagation();
                }),
            )
        })
        .into_any_element()
    }

    fn select_host_tools_connection(
        &mut self,
        connection_id: String,
        focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
        cx: &mut Context<Self>,
    ) {
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.select_connection_for_active_tool(connection_id, focus_origin, cx);
        });
    }

    pub(in crate::workspace) fn handle_connection_monitor_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let connections = self.monitor_connections(cx);
        if !monitor_connection_can_switch(&connections) {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.close_selector(true, cx);
            });
            return false;
        }
        let (selector_open, highlighted_index, focus_origin, selected_connection_id) = {
            let host_tools = self.host_tools.read(cx);
            (
                host_tools.selector_open(),
                host_tools.selector_highlighted_index(),
                host_tools.selector_focus_origin(),
                host_tools.selected_connection_id_owned(),
            )
        };
        let selected_id = selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let selected_index = monitor_connection_selected_index(&connections, selected_id);
        let current = highlighted_index.unwrap_or(selected_index);

        if selector_open {
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
                self.host_tools
                    .update(cx, |host_tools, cx| host_tools.focus_selector_trigger(cx));
                true
            }
            "enter" | "space" | " " | "arrowdown" | "down" if focus_origin.is_some() => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.open_selector_from_keyboard(selected_index, cx);
                });
                true
            }
            "escape" if focus_origin.is_some() => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.close_selector(true, cx);
                });
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
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.close_selector_to_keyboard_trigger(cx);
                });
                true
            }
            "tab" => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.close_selector(true, cx);
                });
                true
            }
            "arrowdown" | "down" => {
                let next_index = browser_behavior::browser_select_next_index(
                    current,
                    connections.len(),
                    browser_behavior::BrowserSelectKeyDirection::Next,
                );
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.highlight_selector_from_keyboard(next_index, cx);
                });
                true
            }
            "arrowup" | "up" => {
                let previous_index = browser_behavior::browser_select_next_index(
                    current,
                    connections.len(),
                    browser_behavior::BrowserSelectKeyDirection::Previous,
                );
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.highlight_selector_from_keyboard(previous_index, cx);
                });
                true
            }
            "home" => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.highlight_selector_from_keyboard(0, cx);
                });
                true
            }
            "end" => {
                let last_index = connections.len().saturating_sub(1);
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.highlight_selector_from_keyboard(last_index, cx);
                });
                true
            }
            "enter" | "space" | " " => {
                if let Some(connection) = connections.get(current.min(connections.len() - 1)) {
                    self.select_host_tools_connection(
                        connection.connection_id.clone(),
                        Some(browser_behavior::BrowserFocusOrigin::Keyboard),
                        cx,
                    );
                }
                true
            }
            _ => false,
        }
    }
}
