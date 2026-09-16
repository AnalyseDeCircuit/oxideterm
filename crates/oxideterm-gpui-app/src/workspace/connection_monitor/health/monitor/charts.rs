use super::*;
use gpui::{Div, PathBuilder, canvas, point};
use oxideterm_connection_monitor::{format_rate, gpu_memory_percent, gpu_utilization_percent};

pub(super) fn capacity_percent(used: u64, total: u64) -> Option<f64> {
    (total > 0).then(|| (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

pub(super) fn monitor_usage(
    kind: MonitorMetricKind,
    metrics: &ResourceMetrics,
) -> Option<Option<f64>> {
    let capacity = |used, total| match (used, total) {
        (Some(used), Some(total)) => capacity_percent(used, total),
        _ => None,
    };
    Some(match kind {
        MonitorMetricKind::Cpu => metrics.cpu_percent,
        MonitorMetricKind::Memory => metrics
            .memory_percent
            .or_else(|| capacity(metrics.memory_used, metrics.memory_total)),
        MonitorMetricKind::Swap => metrics
            .swap_percent
            .or_else(|| capacity(metrics.swap_used, metrics.swap_total)),
        MonitorMetricKind::Disk => metrics
            .disk_percent
            .or_else(|| capacity(metrics.disk_used, metrics.disk_total)),
        MonitorMetricKind::Gpu => gpu_utilization_percent(metrics),
        MonitorMetricKind::GpuMemory => gpu_memory_percent(metrics),
        _ => return None,
    })
}

pub(super) fn render_usage(
    icon: LucideIcon,
    label: String,
    value: String,
    percent: Option<f64>,
    render: &CompactMonitorRenderContext,
) -> Div {
    let percent = percent
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 100.0));
    let mut tokens = render.tokens;
    tokens.ui.accent = match percent {
        Some(value) if value >= 90.0 => tokens.ui.error,
        Some(value) if value >= 70.0 => tokens.ui.warning,
        _ => tokens.ui.accent,
    };
    let caption = percent
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "—".into());
    div()
        .w_full()
        .min_w_0()
        .h(px(86.0))
        .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
        .py(px(6.0))
        .child(
            div()
                .size_full()
                .min_w_0()
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(6.0))
                .px(px(render.tokens.spacing.two))
                .rounded(px(render.tokens.radii.md))
                .border_1()
                .border_color(rgba((render.tokens.ui.border << 8) | 0x80))
                .bg(rgba((render.tokens.ui.bg_panel << 8) | 0x40))
                .text_size(px(render.tokens.metrics.ui_text_xs))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(WorkspaceApp::render_lucide_icon(
                            icon,
                            13.0,
                            rgb(render.tokens.ui.text_muted),
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(render.tokens.ui.text))
                                .child(label),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_family(render.mono_font_family.clone())
                                .text_color(rgb(tokens.ui.accent))
                                .child(caption),
                        ),
                )
                .child(
                    oxideterm_gpui_ui::progress::progress(
                        &tokens,
                        percent.map(|value| value as f32),
                        false,
                    )
                    .h(px(5.0))
                    .border_0()
                    .bg(rgba((render.tokens.ui.text_muted << 8) | 0x26)),
                )
                .child(
                    div()
                        .truncate()
                        .font_family(render.mono_font_family.clone())
                        .text_color(rgb(render.tokens.ui.text_muted))
                        .child(value),
                ),
        )
}

// Timestamp-based coordinates preserve uneven sampling intervals. Missing samples
// remain gaps instead of inventing zero usage or connecting across unavailable data.
fn trend_points(samples: &[(u64, Option<f64>)], maximum: f64) -> Vec<Option<(f32, f32)>> {
    let start = samples.first().map(|sample| sample.0).unwrap_or(0);
    let end = samples.last().map(|sample| sample.0).unwrap_or(start);
    let duration = end.saturating_sub(start).max(1) as f64;
    samples
        .iter()
        .map(|&(timestamp, value)| {
            value
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(|value| {
                    (
                        (timestamp.saturating_sub(start) as f64 / duration).clamp(0.0, 1.0) as f32,
                        (value / maximum.max(1.0)).clamp(0.0, 1.0) as f32,
                    )
                })
        })
        .collect()
}

pub(super) fn render_trend(render: &CompactMonitorRenderContext, cpu: bool) -> Div {
    let first: Vec<_> = render
        .history
        .iter()
        .map(|sample| {
            (
                sample.timestamp_ms,
                if cpu {
                    sample.cpu_percent
                } else {
                    sample.rx_bytes_per_sec.map(|value| value as f64)
                },
            )
        })
        .collect();
    let second: Vec<_> = if cpu {
        Vec::new()
    } else {
        render
            .history
            .iter()
            .map(|sample| {
                (
                    sample.timestamp_ms,
                    sample.tx_bytes_per_sec.map(|value| value as f64),
                )
            })
            .collect()
    };
    let maximum = if cpu {
        100.0
    } else {
        first
            .iter()
            .chain(&second)
            .filter_map(|(_, value)| *value)
            .fold(1.0_f64, f64::max)
    };
    let series = [
        trend_points(&first, maximum),
        trend_points(&second, maximum),
    ];
    let colors = [render.tokens.ui.accent, render.tokens.ui.accent_secondary];
    let grid_color = rgba((render.tokens.ui.text_muted << 8) | 0x26);
    let time_label = |timestamp: u64| {
        i64::try_from(timestamp)
            .ok()
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|time| {
                time.with_timezone(&chrono::Local)
                    .format("%H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "—".into())
    };
    let start = render
        .history
        .first()
        .map(|sample| time_label(sample.timestamp_ms))
        .unwrap_or_else(|| "—".into());
    let end = render
        .history
        .last()
        .map(|sample| time_label(sample.timestamp_ms))
        .unwrap_or_else(|| "—".into());
    div()
        .w_full()
        .min_w_0()
        .h(px(90.0))
        .px(px(
            COMPACT_MONITOR_ROW_SIDE_PADDING + render.tokens.spacing.two
        ))
        .pb(px(6.0))
        .flex()
        .flex_col()
        .gap(px(3.0))
        .text_size(px(render.tokens.metrics.ui_text_xs))
        .text_color(rgb(render.tokens.ui.text_muted))
        .child(div().text_align(gpui::TextAlign::Right).child(if cpu {
            "100%".into()
        } else {
            format_rate(maximum as u64)
        }))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let width = f32::from(bounds.size.width);
                    let height = f32::from(bounds.size.height);
                    let position = |x: f32, y: f32| {
                        point(
                            bounds.origin.x + px(x * width),
                            bounds.origin.y + px((1.0 - y) * height),
                        )
                    };
                    let mut grid = PathBuilder::stroke(px(1.0));
                    for fraction in [0.0, 0.5, 1.0] {
                        grid.move_to(position(0.0, fraction));
                        grid.line_to(position(1.0, fraction));
                    }
                    if let Ok(path) = grid.build() {
                        window.paint_path(path, grid_color);
                    }
                    for (points, color) in series.iter().zip(colors) {
                        let mut path = PathBuilder::stroke(px(1.5));
                        let mut connected = false;
                        for point in points {
                            if let Some((x, y)) = point {
                                let point = position(*x, *y);
                                if connected {
                                    path.line_to(point);
                                } else {
                                    path.move_to(point);
                                }
                                connected = true;
                            } else {
                                connected = false;
                            }
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, rgb(color));
                        }
                    }
                },
            )
            .w_full()
            .flex_1(),
        )
        .child(div().flex().justify_between().child(start).child(end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_coordinates_preserve_time_gaps_and_missing_values() {
        assert_eq!(
            trend_points(
                &[
                    (100, Some(0.0)),
                    (125, Some(50.0)),
                    (150, None),
                    (200, Some(200.0))
                ],
                100.0
            ),
            vec![Some((0.0, 0.0)), Some((0.25, 0.5)), None, Some((1.0, 1.0))]
        );
        assert_eq!(
            trend_points(&[(100, Some(f64::NAN)), (100, Some(10.0))], 0.0),
            vec![None, Some((0.0, 1.0))]
        );
        assert_eq!(capacity_percent(25, 100), Some(25.0));
        assert_eq!(capacity_percent(0, 0), None);
    }

    #[test]
    fn usage_preserves_reported_filesystem_percentage() {
        let mut metrics = ResourceMetrics::empty(0, MetricsSource::Full);
        metrics.disk_used = Some(20);
        metrics.disk_total = Some(100);
        // Filesystems may exclude reserved blocks when reporting utilization.
        metrics.disk_percent = Some(25.0);
        assert_eq!(
            monitor_usage(MonitorMetricKind::Disk, &metrics),
            Some(Some(25.0))
        );
        metrics.disk_percent = None;
        assert_eq!(
            monitor_usage(MonitorMetricKind::Disk, &metrics),
            Some(Some(20.0))
        );
    }
}
