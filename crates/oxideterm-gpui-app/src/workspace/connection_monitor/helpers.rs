fn monitor_center_state(
    app: &WorkspaceApp,
    icon: LucideIcon,
    color: u32,
    label: String,
    cx: &mut Context<WorkspaceApp>,
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
            app.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "monitor-center-state",
                label_key,
                label,
                color,
                cx,
            ),
        ))
        .into_any_element()
}

fn monitor_connection_label(connection: &MonitorConnectionOption) -> String {
    format!(
        "{}@{}:{}",
        connection.username, connection.host, connection.port
    )
}

fn monitor_connection_selected_index(
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

fn topology_transform_x(x: f32, transform: TopologyTransform) -> f32 {
    transform.x + x * transform.k
}

fn topology_transform_y(y: f32, transform: TopologyTransform) -> f32 {
    transform.y + y * transform.k
}

fn topology_view_status_color(status: TopologyViewStatus) -> u32 {
    match status {
        TopologyViewStatus::Connected => TOPOLOGY_CONNECTED,
        TopologyViewStatus::Connecting => TOPOLOGY_CONNECTING,
        TopologyViewStatus::Failed => TOPOLOGY_FAILED,
        TopologyViewStatus::Disconnected => TOPOLOGY_DISCONNECTED,
        TopologyViewStatus::Pending => TOPOLOGY_PENDING,
    }
}

fn connection_pool_state_view(
    state: &ConnectionPoolEntryState,
    i18n: &I18n,
    tokens: &ThemeTokens,
) -> ConnectionPoolStateView {
    match state {
        ConnectionPoolEntryState::Connecting => ConnectionPoolStateView {
            label: i18n.t("connections.state.connecting"),
            color: CONNECTION_POOL_YELLOW_400,
        },
        ConnectionPoolEntryState::Active => ConnectionPoolStateView {
            label: i18n.t("connections.state.active"),
            color: CONNECTION_POOL_GREEN_400,
        },
        ConnectionPoolEntryState::Idle => ConnectionPoolStateView {
            label: i18n.t("connections.state.idle"),
            color: CONNECTION_POOL_AMBER_400,
        },
        ConnectionPoolEntryState::LinkDown => ConnectionPoolStateView {
            label: i18n.t("connections.monitor.link_down"),
            color: tokens.ui.text_muted,
        },
        ConnectionPoolEntryState::Reconnecting => ConnectionPoolStateView {
            label: i18n.t("connections.monitor.reconnecting"),
            color: tokens.ui.text_muted,
        },
        ConnectionPoolEntryState::Disconnecting => ConnectionPoolStateView {
            label: i18n.t("connections.state.disconnecting"),
            color: CONNECTION_POOL_ORANGE_400,
        },
        ConnectionPoolEntryState::Disconnected => ConnectionPoolStateView {
            label: i18n.t("connections.state.disconnected"),
            color: tokens.ui.text_muted,
        },
        ConnectionPoolEntryState::Error(error) => ConnectionPoolStateView {
            label: i18n
                .t("connections.state.error")
                .replace("{{error}}", error),
            color: CONNECTION_POOL_RED_400,
        },
    }
}

fn connection_pool_keep_alive_tooltip(
    i18n: &I18n,
    keep_alive: bool,
    global_never_timeout: bool,
    idle_timeout_min: u64,
) -> String {
    if global_never_timeout {
        return i18n.t("connections.keep_alive.global_never_tooltip");
    }
    if keep_alive {
        return i18n
            .t("connections.keep_alive.disable_tooltip")
            .replace("{{min}}", &idle_timeout_min.to_string());
    }
    i18n.t("connections.keep_alive.enable_tooltip")
}

fn threshold_color(value: Option<f64>) -> u32 {
    match value {
        None => 0x94a3b8,
        Some(value) if value < 70.0 => MONITOR_EMERALD,
        Some(value) if value < 90.0 => MONITOR_AMBER,
        Some(_) => MONITOR_RED,
    }
}

fn rtt_color(value: Option<u64>) -> u32 {
    match value {
        None => 0x94a3b8,
        Some(value) if value < 100 => MONITOR_EMERALD,
        Some(value) if value < 300 => MONITOR_AMBER,
        Some(_) => MONITOR_RED,
    }
}

fn metrics_source_label_key(source: MetricsSource) -> &'static str {
    match source {
        MetricsSource::Full => "profiler.panel.source_full",
        MetricsSource::Partial => "profiler.panel.source_partial",
        MetricsSource::RttOnly => "profiler.panel.source_rtt_only",
        MetricsSource::Failed => "profiler.panel.source_failed",
        MetricsSource::Unsupported => "profiler.panel.source_unsupported",
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_rate(bytes: u64) -> String {
    format!("{}/s", format_bytes(bytes))
}

fn render_sparkline(values: Vec<Option<f64>>, color: u32) -> AnyElement {
    if values.iter().filter_map(|value| *value).count() < 2 {
        return div().into_any_element();
    }

    div()
        .h(px(MONITOR_SPARKLINE_HEIGHT))
        .w_full()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let points = sparkline_polyline_points(
                        &values,
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                    );
                    if points.len() < 2 {
                        return;
                    }

                    let mut builder = PathBuilder::stroke(px(MONITOR_SPARKLINE_STROKE_WIDTH));
                    for (index, (x, y)) in points.into_iter().enumerate() {
                        let point = point(bounds.origin.x + px(x), bounds.origin.y + px(y));
                        if index == 0 {
                            builder.move_to(point);
                        } else {
                            builder.line_to(point);
                        }
                    }
                    if let Ok(path) = builder.build() {
                        window
                            .paint_path(path, rgba((color << 8) | MONITOR_SPARKLINE_STROKE_ALPHA));
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}

fn sparkline_polyline_points(values: &[Option<f64>], width: f32, height: f32) -> Vec<(f32, f32)> {
    let valid = values.iter().filter_map(|value| *value).collect::<Vec<_>>();
    if valid.len() < 2 {
        return Vec::new();
    }

    let width = width.max(1.0);
    let height = height.max(1.0);
    let max = valid.iter().copied().fold(1.0_f64, f64::max);
    let step = width / (valid.len().saturating_sub(1) as f32);
    valid
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let x = index as f32 * step;
            let y = height - ((value / max) as f32 * height * 0.85) - height * 0.05;
            (x, y)
        })
        .collect()
}
