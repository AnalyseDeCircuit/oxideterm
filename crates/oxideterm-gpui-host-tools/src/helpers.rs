use super::*;

use oxideterm_topology::TopologyViewStatus;

pub(crate) fn host_tools_row_id(
    kind: &'static str,
    connection_id: &str,
    resource_id: impl Into<SharedString>,
) -> gpui::ElementId {
    // Native tooltip state belongs to a resource, even when rows reorder or hosts switch.
    let connection = gpui::ElementId::from((gpui::ElementId::from(kind), connection_id.to_owned()));
    (connection, resource_id).into()
}

pub fn host_tools_tooltip_icon_button(
    tokens: &ThemeTokens,
    icon: &'static str,
    icon_size: f32,
    icon_color: Rgba,
    options: oxideterm_gpui_ui::button::IconButtonOptions,
    tooltip: String,
    element_id_prefix: &'static str,
    flex_none: bool,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    oxideterm_gpui_ui::button::icon_button_with_tooltip(
        tokens,
        icon,
        icon_size,
        icon_color,
        options,
        tooltip,
        element_id_prefix,
        flex_none,
        listener,
    )
}

pub fn host_tools_center_state(
    icon: &'static str,
    color: u32,
    label: String,
    selectable_text: &HostToolsText,
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
                .child(render_lucide_icon(icon, 20.0, rgb(color))),
        )
        .child(
            div()
                .text_size(px(14.0))
                .child(selectable_text.render_label(
                    HostToolsTextRole::PlainDocument,
                    "monitor-center-state",
                    label_key,
                    label,
                    color,
                    cx,
                )),
        )
        .into_any_element()
}

pub(crate) fn monitor_connection_label(connection: &MonitorConnectionOption) -> String {
    if let Some(label) = connection.label.as_ref() {
        return label.clone();
    }
    format!(
        "{}@{}:{}",
        connection.username, connection.host, connection.port
    )
}

pub fn monitor_connection_can_switch(connections: &[MonitorConnectionOption]) -> bool {
    // A single Host Tools connection is already identified by the monitor and
    // process headers. Only expose switch affordances when another host exists.
    connections.len() > 1
}

pub(crate) fn host_process_table_uses_separate_user_column(sidebar_width: f32) -> bool {
    // The default Host Tools sidebar is too narrow for Program/User/PID/CPU/Mem
    // plus action affordances. Merge Program and User until the user drags the
    // sidebar wide enough for a btop-like separate User column.
    sidebar_width >= HOST_PROCESS_SEPARATE_USER_COLUMN_MIN_WIDTH
}

pub(crate) fn host_process_identity_header_label(
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

pub fn monitor_connection_selected_index(
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

pub fn topology_transform_x(x: f32, transform: TopologyTransform) -> f32 {
    transform.x + x * transform.k
}

pub fn topology_transform_y(y: f32, transform: TopologyTransform) -> f32 {
    transform.y + y * transform.k
}

pub fn topology_view_status_color(status: TopologyViewStatus) -> u32 {
    match status {
        TopologyViewStatus::Connected => TOPOLOGY_CONNECTED,
        TopologyViewStatus::Connecting => TOPOLOGY_CONNECTING,
        TopologyViewStatus::Failed => TOPOLOGY_FAILED,
        TopologyViewStatus::Disconnected => TOPOLOGY_DISCONNECTED,
        TopologyViewStatus::Pending => TOPOLOGY_PENDING,
    }
}

pub(crate) fn threshold_color(value: Option<f64>) -> u32 {
    monitor_value_level_color(percent_level(value), 0x94a3b8)
}

pub(crate) fn monitor_value_level_color(level: MonitorValueLevel, muted_color: u32) -> u32 {
    match level {
        MonitorValueLevel::Muted => muted_color,
        MonitorValueLevel::Normal => MONITOR_EMERALD,
        MonitorValueLevel::Warning => MONITOR_AMBER,
        MonitorValueLevel::Critical => MONITOR_RED,
    }
}
