use super::*;

use gpui::Div;
use oxideterm_gpui_ui::select::{
    select_event_boundary, select_option_action, select_option_highlighted,
};
use oxideterm_gpui_ui::text_input::{TextInputView, text_input_anchor_probe};

const HOST_TOOLS_CONNECTION_ROW_HEIGHT: f32 = 32.0;
const HOST_TOOLS_SELECTOR_OPTION_HEIGHT: f32 = 36.0;
const HOST_TOOLS_SELECTOR_MENU_PADDING_Y: f32 = 8.0;
const HOST_TOOLS_SELECTOR_VISIBLE_OPTIONS: usize = 4;
const HOST_TOOLS_SELECTOR_GAP: f32 = 8.0;

impl ContextSidebarTool {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Monitor => "sidebar.panels.host_monitor",
            Self::Gpu => "sidebar.panels.gpu",
            Self::Processes => "sidebar.panels.processes",
            Self::Services => "sidebar.panels.services",
            Self::Logs => "sidebar.panels.logs",
            Self::Tmux => "sidebar.panels.tmux",
            Self::Docker => "sidebar.panels.docker",
            Self::Ports => "sidebar.panels.ports",
            Self::Schedules => "sidebar.panels.schedules",
            Self::Filesystems => "sidebar.panels.filesystems",
            Self::Packages => "sidebar.panels.packages",
        }
    }
}

impl ContextSidebarTool {
    pub fn monitoring_enabled(self, settings: &oxideterm_settings::HostToolsSettings) -> bool {
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

    pub fn set_monitoring_enabled(
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
    pub fn render_connection_switcher(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        is_running: bool,
        tokens: &ThemeTokens,
        mono_font_family: SharedString,
        selectable_text: &HostToolsText,
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
        let focus_visible =
            browser_focus_visible(selector_focus_origin.is_some(), selector_focus_origin);
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
            .child(render_lucide_icon(
                "lucide/server.svg",
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
                    .child(selectable_text.render_label(
                        HostToolsTextRole::PlainDocument,
                        "host-tools-connection-endpoint",
                        connection.connection_id.as_str(),
                        monitor_connection_label(connection),
                        theme.text,
                        cx,
                    )),
            );
        if can_switch {
            trigger = trigger.child(div().flex_none().opacity(0.75).child(render_lucide_icon(
                "lucide/chevron-down.svg",
                14.0,
                rgb(theme.text_muted),
            )));
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
                    .child(div().mr_2().child(render_lucide_icon(
                        "lucide/server.svg",
                        14.0,
                        rgb(tokens.ui.text_muted),
                    ))),
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
mod docker;
mod filesystems;
mod gpu;
mod logs;
pub(crate) mod monitor;
mod packages;
mod ports;
mod process;
mod scheduled_tasks;
mod services;
pub(crate) mod tmux;
