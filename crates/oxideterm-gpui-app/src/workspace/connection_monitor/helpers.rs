use super::*;

use oxideterm_topology::TopologyViewStatus;

use crate::workspace::selectable_text::{SelectableTextRenderState, selectable_document_group_id};

pub(super) fn host_tools_tooltip_icon_button(
    tokens: &ThemeTokens,
    icon: LucideIcon,
    icon_size: f32,
    icon_color: Rgba,
    options: oxideterm_gpui_ui::button::IconButtonOptions,
    tooltip: String,
    element_id_prefix: &'static str,
    flex_none: bool,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let actionable = !(options.disabled || options.loading);
    let tooltip_label = tooltip.clone();
    let tooltip_tokens = *tokens;

    // Host Tools owns these controls, so their tooltip and click lifecycle must
    // not depend on the workspace entity or its global overlay state.
    oxideterm_gpui_ui::button::icon_button(
        tokens,
        svg()
            .path(icon.path())
            .size(px(icon_size))
            .text_color(icon_color)
            .into_any_element(),
        options,
    )
    .id((gpui::ElementId::from(element_id_prefix), tooltip))
    .tooltip(move |_window, cx| {
        oxideterm_gpui_ui::tooltip::tooltip_view(tooltip_tokens, tooltip_label.clone(), None, cx)
    })
    .when(actionable, |button| {
        button.on_mouse_down(MouseButton::Left, listener)
    })
    .when(!actionable, |button| {
        // Disabled and loading buttons still consume the click so parent rows
        // cannot treat an unavailable action as a selection request.
        button.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
    })
    .when(flex_none, |button| button.flex_none())
    .into_any_element()
}

pub(super) fn monitor_center_state(
    app: &WorkspaceApp,
    icon: LucideIcon,
    color: u32,
    label: String,
    cx: &mut Context<WorkspaceApp>,
) -> AnyElement {
    let selectable_text = app.selectable_text_render_state(cx);
    host_tools_center_state(icon, color, label, &selectable_text, cx)
}

pub(super) fn host_tools_center_state(
    icon: LucideIcon,
    color: u32,
    label: String,
    selectable_text: &SelectableTextRenderState,
    cx: &mut App,
) -> AnyElement {
    let label_key = label.clone();
    div()
        .p_4()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .text_align(gpui::TextAlign::Center)
        .text_color(rgb(color))
        .child(
            div()
                .mb_2()
                .child(WorkspaceApp::render_lucide_icon(icon, 20.0, rgb(color))),
        )
        .child(div().text_size(px(14.0)).child(
            selectable_text.render_display_text_with_role_in_group(
                SelectableTextRole::PlainDocument,
                selectable_document_group_id(),
                "monitor-center-state",
                label_key,
                0,
                label,
                color,
                cx,
            ),
        ))
        .into_any_element()
}

pub(super) fn monitor_connection_label(connection: &MonitorConnectionOption) -> String {
    format!(
        "{}@{}:{}",
        connection.username, connection.host, connection.port
    )
}

pub(super) fn monitor_connection_can_switch(connections: &[MonitorConnectionOption]) -> bool {
    // A single Host Tools connection is already identified by the monitor and
    // process headers. Only expose switch affordances when another host exists.
    connections.len() > 1
}

pub(super) fn host_process_table_uses_separate_user_column(sidebar_width: f32) -> bool {
    // The default Host Tools sidebar is too narrow for Program/User/PID/CPU/Mem
    // plus action affordances. Merge Program and User until the user drags the
    // sidebar wide enough for a btop-like separate User column.
    sidebar_width >= HOST_PROCESS_SEPARATE_USER_COLUMN_MIN_WIDTH
}

pub(super) fn host_process_identity_header_label(
    i18n: &I18n,
    separate_user_column: bool,
) -> String {
    if separate_user_column {
        return i18n.t("sidebar.host_processes.sort.command");
    }

    format!(
        "{} / {}",
        i18n.t("sidebar.host_processes.sort.command"),
        i18n.t("sidebar.host_processes.sort.user")
    )
}

pub(super) fn monitor_connection_selected_index(
    connections: &[MonitorConnectionOption],
    selected_id: &str,
) -> usize {
    // Radix Select opens with the current value highlighted. Keep the lookup
    // shared between pointer-open rendering and keyboard-open behavior so the
    // monitor selector cannot drift by input modality.
    connections
        .iter()
        .position(|connection| connection.connection_id == selected_id)
        .unwrap_or(0)
}

pub(super) fn topology_transform_x(x: f32, transform: TopologyTransform) -> f32 {
    transform.x + x * transform.k
}

pub(super) fn topology_transform_y(y: f32, transform: TopologyTransform) -> f32 {
    transform.y + y * transform.k
}

pub(super) fn topology_view_status_color(status: TopologyViewStatus) -> u32 {
    match status {
        TopologyViewStatus::Connected => TOPOLOGY_CONNECTED,
        TopologyViewStatus::Connecting => TOPOLOGY_CONNECTING,
        TopologyViewStatus::Failed => TOPOLOGY_FAILED,
        TopologyViewStatus::Disconnected => TOPOLOGY_DISCONNECTED,
        TopologyViewStatus::Pending => TOPOLOGY_PENDING,
    }
}

pub(super) fn threshold_color(value: Option<f64>) -> u32 {
    monitor_value_level_color(percent_level(value), 0x94a3b8)
}

pub(super) fn monitor_value_level_color(level: MonitorValueLevel, muted_color: u32) -> u32 {
    match level {
        MonitorValueLevel::Muted => muted_color,
        MonitorValueLevel::Normal => MONITOR_EMERALD,
        MonitorValueLevel::Warning => MONITOR_AMBER,
        MonitorValueLevel::Critical => MONITOR_RED,
    }
}
