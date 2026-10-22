use super::*;

use gpui::Div;
use oxideterm_gpui_ui::select::{
    select_event_boundary, select_option_action, select_option_highlighted,
};
use oxideterm_gpui_ui::text_input::{TextInputView, text_input_anchor_probe};

use crate::workspace::selectable_text::{SelectableTextRenderState, selectable_document_group_id};

const HOST_TOOLS_CONNECTION_ROW_HEIGHT: f32 = 32.0;
const HOST_TOOLS_SELECTOR_OPTION_HEIGHT: f32 = 36.0;
const HOST_TOOLS_SELECTOR_MENU_PADDING_Y: f32 = 8.0;
const HOST_TOOLS_SELECTOR_VISIBLE_OPTIONS: usize = 4;
const HOST_TOOLS_SELECTOR_GAP: f32 = 8.0;

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
                .clamp(1, HOST_TOOLS_SELECTOR_VISIBLE_OPTIONS)
                as f32;
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
        if tool == ContextSidebarTool::Monitor {
            self.host_tools.update(cx, |host_tools, cx| {
                if let Some(connection_id) = host_tools.selected_connection_id_owned() {
                    host_tools.request_profiler_refresh(connection_id, cx);
                }
            });
            return;
        }
        if matches!(
            tool,
            ContextSidebarTool::Gpu | ContextSidebarTool::Processes | ContextSidebarTool::Docker
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
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
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
                    // Host selection and sampling stay in the shared monitor panel.
                    .child(self.render_host_monitor_panel(cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_host_tool_monitoring_toggle(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tool = self.host_tools.read(cx).active_tool();
        let enabled = self.host_tool_monitoring_enabled(tool);
        let color = if enabled {
            MONITOR_EMERALD
        } else {
            self.tokens.ui.text_muted
        };
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(LucideIcon::Power, 14.0, rgb(color)),
            oxideterm_gpui_ui::button::IconButtonOptions::opaque_toolbar(
                28.0,
                oxideterm_gpui_ui::button::ButtonRadius::Md,
            ),
        )
        .id("host-tool-monitoring-toggle")
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
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
