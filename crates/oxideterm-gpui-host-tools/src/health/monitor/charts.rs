use super::*;
use gpui::{PathBuilder, point};
use oxideterm_connection_monitor::MonitorCpuCoreCell;

pub(crate) fn usage_value_color(level: MonitorValueLevel, ui: &AppUiColors) -> Rgba {
    rgb(match level {
        MonitorValueLevel::Muted => ui.text_muted,
        MonitorValueLevel::Normal => ui.text,
        MonitorValueLevel::Warning => ui.warning,
        MonitorValueLevel::Critical => ui.error,
    })
}

fn usage_bar_fill_color(level: MonitorValueLevel, ui: &AppUiColors) -> Rgba {
    match level {
        MonitorValueLevel::Muted => rgba(0),
        MonitorValueLevel::Normal => rgba((ui.accent << 8) | 0xd9),
        MonitorValueLevel::Warning => rgb(ui.warning),
        MonitorValueLevel::Critical => rgb(ui.error),
    }
}

fn usage_bar_fill(percent: Option<f64>, level: MonitorValueLevel, ui: &AppUiColors) -> Div {
    div()
        .h_full()
        .rounded_full()
        .bg(usage_bar_fill_color(level, ui))
        .w(relative(
            (percent.unwrap_or_default() as f32 / 100.0).clamp(0.0, 1.0),
        ))
}

pub(crate) fn render_compact_usage_row(
    kind: &MonitorUsageKind,
    detail: Option<&str>,
    percent: Option<f64>,
    level: MonitorValueLevel,
    render: &CompactMonitorRenderContext,
) -> Div {
    let tokens = render.tokens;
    let ui = tokens.ui;
    let (label, chevron) = match kind {
        MonitorUsageKind::Cpu {
            core_count,
            expandable,
            expanded,
        } => {
            let cpu = render.i18n.t("profiler.panel.cpu");
            (
                core_count.map_or_else(|| cpu.clone(), |count| format!("{cpu} · {count}C")),
                expandable.then_some(if *expanded {
                    "lucide/chevron-down.svg"
                } else {
                    "lucide/chevron-right.svg"
                }),
            )
        }
        MonitorUsageKind::Memory => (render.i18n.t("profiler.panel.memory"), None),
        MonitorUsageKind::Swap => (render.i18n.t("profiler.panel.swap"), None),
        MonitorUsageKind::Disk => (render.i18n.t("profiler.panel.disk"), None),
        MonitorUsageKind::Mount { mount_point } => (mount_point.clone(), None),
        MonitorUsageKind::Gpu { label, .. } => (label.clone(), None),
    };
    let label = div()
        .min_w_0()
        .truncate()
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(ui.text))
        .when(matches!(kind, MonitorUsageKind::Mount { .. }), |label| {
            label.font_family(render.mono_font_family.clone())
        })
        .child(label);
    let percentage = percent.map_or_else(
        || "—".into(),
        |value| {
            if matches!(kind, MonitorUsageKind::Cpu { .. }) {
                format!("{value:.1}%")
            } else if value > 0.0 && value < 1.0 {
                "<1%".into()
            } else {
                format!("{value:.0}%")
            }
        },
    );
    div()
        .w_full()
        .min_w_0()
        .h(px(COMPACT_MONITOR_USAGE_ROW_HEIGHT))
        .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(5.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(label)
                        .when_some(chevron, |row, icon| {
                            row.child(div().flex_none().child(render_lucide_icon(
                                icon,
                                12.0,
                                rgb(ui.text_muted),
                            )))
                        }),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_baseline()
                        .gap_2()
                        .font_family(render.mono_font_family.clone())
                        .when_some(detail, |row, detail| {
                            row.child(
                                div()
                                    .text_size(px(tokens.metrics.ui_text_2xs))
                                    .text_color(rgb(ui.text_muted))
                                    .child(detail.to_owned()),
                            )
                        })
                        .child(
                            div()
                                .min_w(px(36.0))
                                .text_align(gpui::TextAlign::Right)
                                .text_size(px(tokens.metrics.ui_text_xs))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(usage_value_color(level, &ui))
                                .child(percentage),
                        ),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(4.0))
                .rounded_full()
                .overflow_hidden()
                .bg(rgba((ui.text_muted << 8) | 0x38))
                .child(usage_bar_fill(percent, level, &ui)),
        )
}

pub(crate) fn render_cpu_core_pair_row(
    left: &MonitorCpuCoreCell,
    right: Option<&MonitorCpuCoreCell>,
    render: &CompactMonitorRenderContext,
) -> Div {
    let tokens = render.tokens;
    let ui = tokens.ui;
    let cell = |core: &MonitorCpuCoreCell| {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(4.0))
            .font_family(render.mono_font_family.clone())
            .text_size(px(tokens.metrics.ui_text_2xs))
            .child(
                div()
                    .w(px(22.0))
                    .flex_none()
                    .truncate()
                    .text_color(rgb(ui.text_muted))
                    .child(format!("C{}", core.index)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(16.0))
                    .h(px(3.0))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(rgba((ui.text_muted << 8) | 0x34))
                    .child(usage_bar_fill(core.percent, core.level, &ui)),
            )
            .child(
                div()
                    .w(px(30.0))
                    .flex_none()
                    .text_align(gpui::TextAlign::Right)
                    .text_color(usage_value_color(core.level, &ui))
                    .child(
                        core.percent
                            .map_or_else(|| "—".into(), |value| format!("{value:.0}%")),
                    ),
            )
    };
    div()
        .w_full()
        .min_w_0()
        .h(px(COMPACT_MONITOR_CORE_PAIR_ROW_HEIGHT))
        .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
        .flex()
        .items_center()
        .gap(px(18.0))
        .child(cell(left))
        .child(right.map_or_else(|| div().flex_1().min_w_0(), cell))
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

pub(crate) fn render_sparkline(render: &CompactMonitorRenderContext, cpu: bool) -> Div {
    let tokens = render.tokens;
    let ui = tokens.ui;
    // Store `(points, base_hex, stroke_alpha, fill_alpha)` so curve and area tint share series polarity.
    let mut series: Vec<(Vec<Option<(f32, f32)>>, u32, u32, u32)> = Vec::new();
    let mut idle_network = false;
    if cpu {
        let samples: Vec<_> = render
            .history
            .iter()
            .map(|sample| (sample.timestamp_ms, sample.cpu_percent))
            .collect();
        let color_hex = match render.cpu_level {
            MonitorValueLevel::Critical => ui.error,
            MonitorValueLevel::Warning => ui.warning,
            _ => ui.accent,
        };
        series.push((trend_points(&samples, 100.0), color_hex, 0xd8, 0x2a));
    } else {
        let peak_rate = render
            .history
            .iter()
            .flat_map(|sample| [sample.rx_bytes_per_sec, sample.tx_bytes_per_sec])
            .flatten()
            .max()
            .unwrap_or_default();
        if peak_rate == 0 {
            idle_network = true;
        } else {
            for (receive, color_hex, stroke_alpha, fill_alpha) in [
                (true, ui.accent, 0xd8, 0x28),
                (false, ui.accent_secondary, 0xb8, 0x18),
            ] {
                let samples: Vec<_> = render
                    .history
                    .iter()
                    .map(|sample| {
                        let rate = if receive {
                            sample.rx_bytes_per_sec
                        } else {
                            sample.tx_bytes_per_sec
                        };
                        (sample.timestamp_ms, rate.map(|rate| rate as f64))
                    })
                    .collect();
                series.push((
                    trend_points(&samples, peak_rate as f64),
                    color_hex,
                    stroke_alpha,
                    fill_alpha,
                ));
            }
        }
    }
    div()
        .w_full()
        .min_w_0()
        .h(px(COMPACT_MONITOR_SPARKLINE_ROW_HEIGHT))
        .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
        .py(px(3.0))
        .flex()
        .child(
            div()
                .w_full()
                .flex_1()
                .rounded(px(tokens.radii.sm))
                .bg(rgba((ui.bg_sunken << 8) | 0x6b))
                .border_1()
                .border_color(rgba((ui.border << 8) | 0x45))
                .overflow_hidden()
                .px(px(2.0))
                .py(px(2.0))
                .flex()
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            let width = f32::from(bounds.size.width);
                            let usable_height = (f32::from(bounds.size.height) - 2.0).max(1.0);
                            let position = |x: f32, y: f32| {
                                point(
                                    bounds.origin.x + px(x * width),
                                    bounds.origin.y + px(1.0 + (1.0 - y) * usable_height),
                                )
                            };
                            let mut midline = PathBuilder::stroke(px(1.0));
                            midline.move_to(position(0.0, 0.5));
                            midline.line_to(position(1.0, 0.5));
                            if let Ok(path) = midline.build() {
                                window.paint_path(path, rgba((ui.border << 8) | 0x45));
                            }
                            if idle_network {
                                let mut baseline = PathBuilder::stroke(px(1.0));
                                baseline.move_to(position(0.0, 0.0));
                                baseline.line_to(position(1.0, 0.0));
                                if let Ok(path) = baseline.build() {
                                    window.paint_path(path, rgba((ui.text_muted << 8) | 0x48));
                                }
                            }
                            for (points, color_hex, stroke_alpha, fill_alpha) in &series {
                                let stroke_color = rgba((*color_hex << 8) | *stroke_alpha);
                                let fill_color = rgba((*color_hex << 8) | *fill_alpha);
                                let mut line = PathBuilder::stroke(px(1.5));
                                let mut segments: Vec<Vec<(f32, f32)>> = Vec::new();
                                for maybe_point in points {
                                    if let Some((x, y)) = *maybe_point {
                                        if let Some(current) = segments.last_mut() {
                                            current.push((x, y));
                                        } else {
                                            segments.push(vec![(x, y)]);
                                        }
                                    } else if segments.last().is_some_and(|seg| !seg.is_empty()) {
                                        segments.push(Vec::new());
                                    }
                                }
                                for segment in segments {
                                    if segment.is_empty() {
                                        continue;
                                    }
                                    if segment.len() >= 2 {
                                        let mut area = PathBuilder::fill();
                                        let first = segment[0];
                                        let last = segment[segment.len() - 1];
                                        area.move_to(position(first.0, 0.0));
                                        for &(x, y) in &segment {
                                            area.line_to(position(x, y));
                                        }
                                        area.line_to(position(last.0, 0.0));
                                        area.close();
                                        if let Ok(path) = area.build() {
                                            window.paint_path(path, fill_color);
                                        }
                                    }
                                    for (idx, &(x, y)) in segment.iter().enumerate() {
                                        let pt = position(x, y);
                                        if idx == 0 {
                                            line.move_to(pt);
                                        } else {
                                            line.line_to(pt);
                                        }
                                    }
                                }
                                if let Ok(path) = line.build() {
                                    window.paint_path(path, stroke_color);
                                }
                            }
                        },
                    )
                    .w_full()
                    .flex_1(),
                ),
        )
}
