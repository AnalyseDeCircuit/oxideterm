// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SVG generation for the supported Mermaid subset.

use std::f32::consts::PI;

use oxideterm_theme::ThemeTokens;

use crate::mermaid::layout::{
    GanttSectionRow, GanttTaskRow, LaidOutDiagram, LaidOutDiagramKind, LaidOutGantt, LaidOutGraph,
    LaidOutPie, LaidOutSequence, NodeBox, SubgraphBox,
};
use crate::mermaid::model::{
    GanttTaskStatus, GraphEdgeKind, GraphNodeShape, PieSlice, SequenceMessageKind,
    SequenceParticipantKind,
};
use crate::options::MarkdownOptions;

const SVG_SYSTEM_FONT_FALLBACKS: &[&str] = &[
    "system-ui",
    "-apple-system",
    "BlinkMacSystemFont",
    "Segoe UI",
    "PingFang SC",
    "Hiragino Sans GB",
    "Microsoft YaHei",
    "Noto Sans CJK SC",
    "Noto Sans CJK",
    "sans-serif",
];

const PIE_COLORS: &[u32] = &[
    0x22d3ee, 0xa78bfa, 0x34d399, 0xfb923c, 0xf472b6, 0x60a5fa, 0xfacc15, 0xf87171, 0x818cf8,
    0x2dd4bf, 0xfbbf24, 0xf97316,
];

const GANTT_BAR_HEIGHT: f32 = 18.0;
const GANTT_MILESTONE_SIZE: f32 = 14.0;
const GANTT_TICK_LABEL_OFFSET: f32 = 18.0;

#[derive(Clone, Debug)]
pub struct RenderedSvg {
    pub svg: String,
    pub width: f32,
    pub height: f32,
    pub pixel_width: f32,
    pub pixel_height: f32,
}

pub fn render(
    layout: &LaidOutDiagram,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) -> RenderedSvg {
    render_with_scale(layout, tokens, opts, 1.0)
}

pub fn render_with_scale(
    layout: &LaidOutDiagram,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
    raster_scale: f32,
) -> RenderedSvg {
    let mut svg = String::new();
    push_svg_header(
        &mut svg,
        layout.width,
        layout.height,
        raster_scale,
        tokens,
        opts,
    );
    match &layout.kind {
        LaidOutDiagramKind::Gantt(gantt) => render_gantt(&mut svg, gantt, tokens, opts),
        LaidOutDiagramKind::Graph(graph) => render_graph(&mut svg, graph, tokens, opts),
        LaidOutDiagramKind::Pie(pie) => render_pie(&mut svg, pie, tokens, opts),
        LaidOutDiagramKind::Sequence(sequence) => render_sequence(&mut svg, sequence, tokens, opts),
    }
    svg.push_str("</svg>");
    RenderedSvg {
        svg,
        width: layout.width,
        height: layout.height,
        pixel_width: layout.width * raster_scale,
        pixel_height: layout.height * raster_scale,
    }
}

fn push_svg_header(
    svg: &mut String,
    width: f32,
    height: f32,
    raster_scale: f32,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let line = hex(tokens.ui.border);
    let pixel_width = width * raster_scale;
    let pixel_height = height * raster_scale;
    let font_family = svg_font_family_stack(&opts.body_font_family);
    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{pixel_width:.0}" height="{pixel_height:.0}" viewBox="0 0 {width:.0} {height:.0}" role="img" font-family="{}" font-size="{:.1}">"#,
        escape_attr(&font_family),
        opts.base_font_size,
    ));
    svg.push_str("<defs>");
    svg.push_str(&format!(
        r#"<marker id="arrow" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto" markerUnits="strokeWidth"><path d="M0,0 L0,6 L9,3 z" fill="{line}"/></marker>"#
    ));
    svg.push_str("</defs>");
}

fn svg_font_family_stack(primary_font_family: &str) -> String {
    let mut families = Vec::new();
    for family in primary_font_family.split(',') {
        push_svg_font_family(&mut families, family.trim());
    }
    for fallback in SVG_SYSTEM_FONT_FALLBACKS {
        push_svg_font_family(&mut families, fallback);
    }
    families.join(", ")
}

fn push_svg_font_family(families: &mut Vec<String>, family: &str) {
    if family.is_empty() {
        return;
    }
    let normalized = family.trim_matches('"').trim_matches('\'').trim();
    if normalized.is_empty()
        || families.iter().any(|existing| {
            existing
                .trim_matches('"')
                .trim_matches('\'')
                .eq_ignore_ascii_case(normalized)
        })
    {
        return;
    }

    // SVG font-family accepts generic names unquoted; concrete names with
    // whitespace need quotes to survive XML parsing and rasterizer lookup.
    if normalized.contains(char::is_whitespace) {
        families.push(format!(r#""{}""#, normalized.replace('"', "")));
    } else {
        families.push(normalized.to_string());
    }
}

fn render_graph(
    svg: &mut String,
    graph: &LaidOutGraph,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    for subgraph in &graph.subgraphs {
        render_subgraph(svg, subgraph, tokens, opts);
    }

    for edge in &graph.diagram.edges {
        let Some(from) = graph.nodes.get(&edge.from) else {
            continue;
        };
        let Some(to) = graph.nodes.get(&edge.to) else {
            continue;
        };
        let (x1, y1, x2, y2) = edge_points(from, to);
        let path = orthogonal_edge_path(x1, y1, x2, y2);
        let stroke = hex(tokens.ui.border);
        let mut attrs = format!(r#"stroke="{stroke}" fill="none" stroke-width="1.8""#);
        match edge.kind {
            GraphEdgeKind::Arrow => attrs.push_str(r#" marker-end="url(#arrow)""#),
            GraphEdgeKind::Line => {}
            GraphEdgeKind::DottedArrow => {
                attrs.push_str(r#" stroke-dasharray="5 5" marker-end="url(#arrow)""#);
            }
            GraphEdgeKind::ThickArrow => {
                attrs.push_str(r#" stroke-width="3" marker-end="url(#arrow)""#);
            }
        }
        svg.push_str(&format!(r#"<path d="{path}" {attrs}/>"#));
        if let Some(label) = &edge.label {
            let mid_x = (x1 + x2) * 0.5;
            let mid_y = (y1 + y2) * 0.5 - 6.0;
            render_edge_label(svg, mid_x, mid_y, label, tokens, opts);
        }
    }

    for node in &graph.diagram.nodes {
        let Some(bounds) = graph.nodes.get(&node.id) else {
            continue;
        };
        render_graph_node(svg, bounds, node.shape, &node.label, tokens, opts);
    }
}

fn render_gantt(
    svg: &mut String,
    gantt: &LaidOutGantt,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let text = hex(tokens.ui.text);
    let muted = hex(tokens.ui.text_muted);
    let border = hex(tokens.ui.border);
    let chart_width = (gantt.max_day - gantt.min_day).max(1) as f32 * gantt.day_width;

    if let Some(title) = &gantt.diagram.title {
        svg.push_str(&format!(
            r#"<text x="{:.1}" y="28" text-anchor="middle" fill="{text}" font-size="{:.1}" font-weight="600">{}</text>"#,
            gantt.chart_x + chart_width * 0.5,
            opts.base_font_size * 1.05,
            escape_text(title)
        ));
    }

    render_gantt_axis(svg, gantt, chart_width, tokens, opts);

    for section in &gantt.section_rows {
        render_gantt_section(svg, gantt, section, tokens, opts);
    }
    for row in &gantt.task_rows {
        render_gantt_task(svg, gantt, row, tokens, opts);
    }

    let bottom = gantt
        .task_rows
        .last()
        .map(|row| row.y + 24.0)
        .unwrap_or(gantt.chart_y);
    svg.push_str(&format!(
        r#"<path d="M{:.1} {:.1} L{:.1} {:.1}" stroke="{border}" stroke-width="1"/>"#,
        gantt.chart_x,
        gantt.chart_y - 10.0,
        gantt.chart_x + chart_width,
        gantt.chart_y - 10.0
    ));
    svg.push_str(&format!(
        r#"<rect x="{:.1}" y="{:.1}" width="{chart_width:.1}" height="{:.1}" fill="none" stroke="{border}" stroke-opacity="0.6" stroke-width="1"/>"#,
        gantt.chart_x,
        gantt.chart_y - 10.0,
        bottom - gantt.chart_y + 34.0
    ));
    svg.push_str(&format!(r#"<g fill="{muted}"></g>"#));
}

fn render_gantt_axis(
    svg: &mut String,
    gantt: &LaidOutGantt,
    chart_width: f32,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let tick_step = gantt_tick_step(gantt.max_day - gantt.min_day);
    let grid_bottom = gantt
        .task_rows
        .last()
        .map(|row| row.y + 24.0)
        .unwrap_or(gantt.chart_y);
    let mut day = gantt.min_day;
    while day <= gantt.max_day {
        let x = gantt.chart_x + (day - gantt.min_day) as f32 * gantt.day_width;
        svg.push_str(&format!(
            r#"<path d="M{x:.1} {:.1} L{x:.1} {grid_bottom:.1}" stroke="{}" stroke-opacity="0.34" stroke-width="1"/>"#,
            gantt.chart_y - 10.0,
            hex(tokens.ui.divider)
        ));
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{:.1}" text-anchor="middle" fill="{}" font-size="{:.1}">{}</text>"#,
            gantt.chart_y - GANTT_TICK_LABEL_OFFSET,
            hex(tokens.ui.text_muted),
            opts.base_font_size * 0.72,
            escape_text(&format_gantt_day(day))
        ));
        day += tick_step;
    }
    svg.push_str(&format!(
        r#"<path d="M{:.1} {:.1} L{:.1} {:.1}" stroke="{}" stroke-opacity="0.34" stroke-width="1"/>"#,
        gantt.chart_x + chart_width,
        gantt.chart_y - 10.0,
        gantt.chart_x + chart_width,
        grid_bottom,
        hex(tokens.ui.divider)
    ));
}

fn render_gantt_section(
    svg: &mut String,
    gantt: &LaidOutGantt,
    row: &GanttSectionRow,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let Some(section) = gantt.diagram.sections.get(row.section_index) else {
        return;
    };
    svg.push_str(&format!(
        r#"<text x="{:.1}" y="{:.1}" fill="{}" font-size="{:.1}" font-weight="600">{}</text>"#,
        28.0,
        row.y,
        hex(tokens.ui.text_muted),
        opts.base_font_size * 0.86,
        escape_text(&section.label)
    ));
}

fn render_gantt_task(
    svg: &mut String,
    gantt: &LaidOutGantt,
    row: &GanttTaskRow,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let Some(task) = gantt
        .diagram
        .sections
        .get(row.section_index)
        .and_then(|section| section.tasks.get(row.task_index))
    else {
        return;
    };
    let label_y = row.y + 19.0;
    svg.push_str(&format!(
        r#"<text x="28" y="{label_y:.1}" fill="{}" font-size="{:.1}">{}</text>"#,
        hex(tokens.ui.text),
        opts.base_font_size * 0.86,
        escape_text(&task.label)
    ));

    let x = gantt.chart_x + (task.start_day - gantt.min_day) as f32 * gantt.day_width;
    let y = row.y + (30.0 - GANTT_BAR_HEIGHT) * 0.5;
    let width = ((task.end_day - task.start_day).max(1) as f32 * gantt.day_width).max(8.0);
    let color = hex(gantt_status_color(task.status, tokens));
    if task.status == GanttTaskStatus::Milestone {
        let cx = x + width * 0.5;
        let cy = y + GANTT_BAR_HEIGHT * 0.5;
        svg.push_str(&format!(
            r#"<polygon points="{cx:.1},{:.1} {:.1},{cy:.1} {cx:.1},{:.1} {:.1},{cy:.1}" fill="{color}" stroke="{}" stroke-width="1"/>"#,
            cy - GANTT_MILESTONE_SIZE * 0.5,
            cx + GANTT_MILESTONE_SIZE * 0.5,
            cy + GANTT_MILESTONE_SIZE * 0.5,
            cx - GANTT_MILESTONE_SIZE * 0.5,
            hex(tokens.ui.bg)
        ));
        return;
    }

    svg.push_str(&format!(
        r#"<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{GANTT_BAR_HEIGHT:.1}" rx="5" fill="{color}" fill-opacity="0.86" stroke="{}" stroke-width="1"/>"#,
        hex(tokens.ui.bg)
    ));
}

fn gantt_status_color(status: GanttTaskStatus, tokens: &ThemeTokens) -> u32 {
    match status {
        GanttTaskStatus::Normal => tokens.ui.info,
        GanttTaskStatus::Active => tokens.ui.accent,
        GanttTaskStatus::Done => tokens.ui.success,
        GanttTaskStatus::Critical => tokens.ui.warning,
        GanttTaskStatus::Milestone => tokens.terminal.bright_magenta,
    }
}

fn gantt_tick_step(span_days: i32) -> i32 {
    match span_days {
        0..=21 => 1,
        22..=75 => 7,
        76..=180 => 14,
        _ => 30,
    }
}

fn format_gantt_day(day: i32) -> String {
    let (year, month, day) = civil_from_days(day);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(day: i32) -> (i32, u32, u32) {
    // Inverse of the parser's civil-day transform; this keeps axis labels
    // deterministic without depending on locale or wall-clock time.
    let z = day + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + (m <= 2) as i32, m as u32, d as u32)
}

fn render_subgraph(
    svg: &mut String,
    subgraph: &SubgraphBox,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let fill = hex(tokens.ui.bg_panel);
    let stroke = hex(tokens.ui.border);
    svg.push_str(&format!(
        r#"<g data-subgraph-id="{}"><rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="8" fill="{fill}" fill-opacity="0.42" stroke="{stroke}" stroke-width="1" stroke-dasharray="6 5"/>"#,
        escape_attr(&subgraph.id),
        subgraph.x,
        subgraph.y,
        subgraph.width,
        subgraph.height
    ));
    svg.push_str(&format!(
        r#"<text x="{:.1}" y="{:.1}" fill="{}" font-size="{:.1}" font-weight="600">{}</text></g>"#,
        subgraph.x + 12.0,
        subgraph.y + opts.base_font_size + 8.0,
        hex(tokens.ui.text_muted),
        opts.base_font_size * 0.92,
        escape_text(&subgraph.label)
    ));
}

fn edge_points(from: &NodeBox, to: &NodeBox) -> (f32, f32, f32, f32) {
    let from_cx = from.x + from.width * 0.5;
    let from_cy = from.y + from.height * 0.5;
    let to_cx = to.x + to.width * 0.5;
    let to_cy = to.y + to.height * 0.5;
    if (to_cx - from_cx).abs() > (to_cy - from_cy).abs() {
        let from_side = if to_cx >= from_cx {
            from.width * 0.5
        } else {
            -from.width * 0.5
        };
        let to_side = if to_cx >= from_cx {
            -to.width * 0.5
        } else {
            to.width * 0.5
        };
        (from_cx + from_side, from_cy, to_cx + to_side, to_cy)
    } else {
        let from_side = if to_cy >= from_cy {
            from.height * 0.5
        } else {
            -from.height * 0.5
        };
        let to_side = if to_cy >= from_cy {
            -to.height * 0.5
        } else {
            to.height * 0.5
        };
        (from_cx, from_cy + from_side, to_cx, to_cy + to_side)
    }
}

fn orthogonal_edge_path(x1: f32, y1: f32, x2: f32, y2: f32) -> String {
    if (x2 - x1).abs() < 4.0 || (y2 - y1).abs() < 4.0 {
        return format!("M{x1:.1} {y1:.1} L{x2:.1} {y2:.1}");
    }
    if (x2 - x1).abs() >= (y2 - y1).abs() {
        let mid_x = (x1 + x2) * 0.5;
        format!("M{x1:.1} {y1:.1} L{mid_x:.1} {y1:.1} L{mid_x:.1} {y2:.1} L{x2:.1} {y2:.1}")
    } else {
        let mid_y = (y1 + y2) * 0.5;
        format!("M{x1:.1} {y1:.1} L{x1:.1} {mid_y:.1} L{x2:.1} {mid_y:.1} L{x2:.1} {y2:.1}")
    }
}

fn render_graph_node(
    svg: &mut String,
    bounds: &NodeBox,
    shape: GraphNodeShape,
    label: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let fill = hex(tokens.ui.bg_elevated);
    let stroke = hex(tokens.ui.border);
    match shape {
        GraphNodeShape::Rectangle | GraphNodeShape::Subroutine => svg.push_str(&format!(
            r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="5" fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>"#,
            bounds.x, bounds.y, bounds.width, bounds.height
        )),
        GraphNodeShape::Rounded | GraphNodeShape::Stadium => svg.push_str(&format!(
            r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="16" fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>"#,
            bounds.x, bounds.y, bounds.width, bounds.height
        )),
        GraphNodeShape::Circle => {
            let cx = bounds.x + bounds.width * 0.5;
            let cy = bounds.y + bounds.height * 0.5;
            let radius = bounds.height.max(bounds.width.min(bounds.height * 1.6)) * 0.5;
            svg.push_str(&format!(
                r#"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{:.1}" ry="{:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>"#,
                radius,
                bounds.height * 0.5
            ));
        }
        GraphNodeShape::Decision => {
            let cx = bounds.x + bounds.width * 0.5;
            let cy = bounds.y + bounds.height * 0.5;
            svg.push_str(&format!(
                r#"<polygon points="{cx:.1},{:.1} {:.1},{cy:.1} {cx:.1},{:.1} {:.1},{cy:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>"#,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
                bounds.x,
            ));
        }
        GraphNodeShape::Database => {
            let top = bounds.y + 8.0;
            let bottom = bounds.y + bounds.height - 8.0;
            svg.push_str(&format!(
                r#"<path d="M{:.1} {top:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {top:.1} L{:.1} {bottom:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {bottom:.1} Z" fill="{fill}" stroke="{stroke}" stroke-width="1.2"/><path d="M{:.1} {top:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {top:.1}" fill="none" stroke="{stroke}" stroke-width="1.2"/>"#,
                bounds.x,
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y,
                bounds.x + bounds.width,
                bounds.x + bounds.width,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
                bounds.x,
                bounds.y + bounds.height,
                bounds.x,
                bounds.x,
                bounds.x,
                bounds.y + 16.0,
                bounds.x + bounds.width,
                bounds.y + 16.0,
                bounds.x + bounds.width
            ));
        }
    }
    if shape == GraphNodeShape::Subroutine {
        svg.push_str(&format!(
            r#"<path d="M{:.1} {:.1} L{:.1} {:.1} M{:.1} {:.1} L{:.1} {:.1}" stroke="{stroke}" stroke-width="1"/>"#,
            bounds.x + 12.0,
            bounds.y,
            bounds.x + 12.0,
            bounds.y + bounds.height,
            bounds.x + bounds.width - 12.0,
            bounds.y,
            bounds.x + bounds.width - 12.0,
            bounds.y + bounds.height
        ));
    }
    render_centered_text(
        svg,
        bounds.x + bounds.width * 0.5,
        bounds.y + bounds.height * 0.5,
        label,
        tokens,
        opts,
    );
}

fn render_sequence(
    svg: &mut String,
    sequence: &LaidOutSequence,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let stroke = hex(tokens.ui.border);
    let text = hex(tokens.ui.text);
    let lifeline_top = 72.0;
    let lifeline_bottom = sequence.message_y.last().copied().unwrap_or(100.0) + 44.0;

    for participant in &sequence.diagram.participants {
        let Some(bounds) = sequence.participants.get(&participant.id) else {
            continue;
        };
        let box_x = bounds.center_x - bounds.label_width * 0.5;
        let fill = match participant.kind {
            SequenceParticipantKind::Participant => hex(tokens.ui.bg_elevated),
            SequenceParticipantKind::Actor => hex(tokens.ui.bg_panel),
        };
        svg.push_str(&format!(
            r#"<rect x="{box_x:.1}" y="24" width="{:.1}" height="34" rx="6" fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>"#,
            bounds.label_width
        ));
        render_centered_text(svg, bounds.center_x, 41.0, &participant.label, tokens, opts);
        svg.push_str(&format!(
            r#"<path d="M{:.1} {lifeline_top:.1} L{:.1} {lifeline_bottom:.1}" stroke="{stroke}" stroke-width="1" stroke-dasharray="5 5"/>"#,
            bounds.center_x, bounds.center_x
        ));
    }

    for (index, message) in sequence.diagram.messages.iter().enumerate() {
        let y = sequence.message_y[index];
        let Some(from) = sequence.participants.get(&message.from) else {
            continue;
        };
        let Some(to) = sequence.participants.get(&message.to) else {
            continue;
        };
        if (from.center_x - to.center_x).abs() < f32::EPSILON {
            let x = from.center_x;
            svg.push_str(&format!(
                r#"<path d="M{x:.1} {y:.1} C{:.1} {:.1} {:.1} {:.1} {x:.1} {:.1}" stroke="{stroke}" fill="none" marker-end="url(#arrow)"/>"#,
                x + 48.0,
                y,
                x + 48.0,
                y + 30.0,
                y + 30.0
            ));
            render_edge_label(svg, x + 54.0, y - 10.0, &message.label, tokens, opts);
            continue;
        }
        let dash = if message.kind == SequenceMessageKind::DashedArrow {
            r#" stroke-dasharray="5 5""#
        } else {
            ""
        };
        svg.push_str(&format!(
            r#"<path d="M{:.1} {y:.1} L{:.1} {y:.1}" stroke="{stroke}" stroke-width="1.6" fill="none"{dash} marker-end="url(#arrow)"/>"#,
            from.center_x, to.center_x
        ));
        render_edge_label(
            svg,
            (from.center_x + to.center_x) * 0.5,
            y - 11.0,
            &message.label,
            tokens,
            opts,
        );
    }

    svg.push_str(&format!(r#"<g fill="{text}"></g>"#));
}

fn render_pie(svg: &mut String, pie: &LaidOutPie, tokens: &ThemeTokens, opts: &MarkdownOptions) {
    let total = pie
        .diagram
        .slices
        .iter()
        .map(|slice| slice.value)
        .sum::<f64>() as f32;
    let stroke = hex(tokens.ui.bg);
    let text = hex(tokens.ui.text);
    let muted = hex(tokens.ui.text_muted);

    if let Some(title) = &pie.diagram.title {
        svg.push_str(&format!(
            r#"<text x="{:.1}" y="28" text-anchor="middle" fill="{text}" font-size="{:.1}" font-weight="600">{}</text>"#,
            pie.center_x,
            opts.base_font_size * 1.05,
            escape_text(title)
        ));
    }

    // Mermaid pie charts remain useful with a compact deterministic renderer:
    // draw slices from stable palette slots and keep exact values in the legend.
    let mut start_angle = -PI * 0.5;
    let positive_count = pie
        .diagram
        .slices
        .iter()
        .filter(|slice| slice.value > 0.0)
        .count();
    for (index, slice) in pie.diagram.slices.iter().enumerate() {
        if slice.value <= 0.0 {
            continue;
        }
        let color = hex(PIE_COLORS[index % PIE_COLORS.len()]);
        let fraction = (slice.value as f32 / total).clamp(0.0, 1.0);
        if positive_count == 1 || fraction >= 0.999 {
            svg.push_str(&format!(
                r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{color}" stroke="{stroke}" stroke-width="2"/>"#,
                pie.center_x, pie.center_y, pie.radius
            ));
            start_angle += PI * 2.0 * fraction;
            continue;
        }

        let sweep = PI * 2.0 * fraction;
        let end_angle = start_angle + sweep;
        let (start_x, start_y) = polar_point(pie.center_x, pie.center_y, pie.radius, start_angle);
        let (end_x, end_y) = polar_point(pie.center_x, pie.center_y, pie.radius, end_angle);
        let large_arc = if sweep > PI { 1 } else { 0 };
        svg.push_str(&format!(
            r#"<path d="M{:.1} {:.1} L{start_x:.1} {start_y:.1} A{:.1} {:.1} 0 {large_arc} 1 {end_x:.1} {end_y:.1} Z" fill="{color}" stroke="{stroke}" stroke-width="2"/>"#,
            pie.center_x, pie.center_y, pie.radius, pie.radius
        ));
        start_angle = end_angle;
    }

    for (index, slice) in pie.diagram.slices.iter().enumerate() {
        render_pie_legend_row(svg, pie, slice, index, total, tokens, opts);
    }

    svg.push_str(&format!(
        r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="none" stroke="{muted}" stroke-opacity="0.26" stroke-width="1"/>"#,
        pie.center_x, pie.center_y, pie.radius
    ));
}

fn render_pie_legend_row(
    svg: &mut String,
    pie: &LaidOutPie,
    slice: &PieSlice,
    index: usize,
    total: f32,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let y = pie.legend_y + index as f32 * 24.0;
    let color = hex(PIE_COLORS[index % PIE_COLORS.len()]);
    svg.push_str(&format!(
        r#"<rect x="{:.1}" y="{:.1}" width="12" height="12" rx="3" fill="{color}"/>"#,
        pie.legend_x,
        y - 10.0
    ));
    let value = if pie.diagram.show_data {
        format_pie_value(slice.value)
    } else {
        let percent = if total > 0.0 {
            slice.value as f32 / total * 100.0
        } else {
            0.0
        };
        format!("{percent:.1}%")
    };
    svg.push_str(&format!(
        r#"<text x="{:.1}" y="{:.1}" fill="{}" font-size="{:.1}">{}</text><text x="{:.1}" y="{:.1}" text-anchor="end" fill="{}" font-size="{:.1}">{}</text>"#,
        pie.legend_x + 20.0,
        y,
        hex(tokens.ui.text),
        opts.base_font_size * 0.92,
        escape_text(&slice.label),
        pie.legend_x + 188.0,
        y,
        hex(tokens.ui.text_muted),
        opts.base_font_size * 0.86,
        escape_text(&value)
    ));
}

fn polar_point(cx: f32, cy: f32, radius: f32, angle: f32) -> (f32, f32) {
    (cx + radius * angle.cos(), cy + radius * angle.sin())
}

fn format_pie_value(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        return format!("{value:.0}");
    }
    let formatted = format!("{value:.2}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn render_centered_text(
    svg: &mut String,
    x: f32,
    y: f32,
    label: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    svg.push_str(&format!(
        r#"<text x="{x:.1}" y="{y:.1}" text-anchor="middle" dominant-baseline="middle" fill="{}" font-size="{:.1}">{}</text>"#,
        hex(tokens.ui.text),
        opts.base_font_size,
        escape_text(label)
    ));
}

fn render_edge_label(
    svg: &mut String,
    x: f32,
    y: f32,
    label: &str,
    tokens: &ThemeTokens,
    opts: &MarkdownOptions,
) {
    let width = (label.chars().count() as f32 * opts.base_font_size * 0.58 + 14.0).max(24.0);
    let height = opts.base_font_size + 8.0;
    svg.push_str(&format!(
        r#"<rect x="{:.1}" y="{:.1}" width="{width:.1}" height="{height:.1}" rx="4" fill="{}" opacity="0.94"/>"#,
        x - width * 0.5,
        y - height * 0.5,
        hex(tokens.ui.bg_panel)
    ));
    render_centered_text(svg, x, y, label, tokens, opts);
}

fn hex(value: u32) -> String {
    format!("#{value:06x}")
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(value: &str) -> String {
    escape_text(value).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use oxideterm_theme::default_tokens;

    use crate::mermaid::{layout, parser};
    use crate::options::MarkdownOptions;

    use super::*;

    #[test]
    fn escapes_svg_text() {
        let tokens = default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);
        let diagram = parser::parse("graph TD\nA[<bad&value>] --> B[ok]").unwrap();
        let layout = layout::layout(diagram, &opts);
        let rendered = render(&layout, &tokens, &opts);

        assert!(rendered.svg.contains("&lt;bad&amp;value&gt;"));
        assert!(!rendered.svg.contains("<bad&value>"));
    }

    #[test]
    fn svg_font_family_uses_system_cjk_fallbacks() {
        let stack = svg_font_family_stack("SF Pro Text");

        assert!(stack.starts_with(r#""SF Pro Text", system-ui"#));
        assert!(stack.contains(r#""PingFang SC""#));
        assert!(stack.contains(r#""Microsoft YaHei""#));
        assert!(stack.ends_with("sans-serif"));
    }

    #[test]
    fn svg_font_family_deduplicates_primary_fonts() {
        let stack = svg_font_family_stack(r#""Segoe UI", PingFang SC"#);

        assert_eq!(stack.matches("Segoe UI").count(), 1);
        assert_eq!(stack.matches("PingFang SC").count(), 1);
    }

    #[test]
    fn renders_subgraph_and_orthogonal_paths() {
        let tokens = default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);
        let diagram = parser::parse("flowchart TD\nsubgraph cluster[Cluster]\nA --> B\nend")
            .expect("graph should parse");
        let layout = layout::layout(diagram, &opts);
        let rendered = render(&layout, &tokens, &opts);

        assert!(rendered.svg.contains("data-subgraph-id=\"cluster\""));
        assert!(rendered.svg.contains("Cluster"));
        assert!(rendered.svg.contains("<path d=\"M"));
    }

    #[test]
    fn renders_pie_chart_svg_with_legend_values() {
        let tokens = default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);
        let diagram = parser::parse("pie showData title Tickets\n\"Open\" : 4\n\"Closed\" : 6")
            .expect("pie chart should parse");
        let layout = layout::layout(diagram, &opts);
        let rendered = render(&layout, &tokens, &opts);

        assert!(rendered.svg.contains("Tickets"));
        assert!(rendered.svg.contains("Open"));
        assert!(rendered.svg.contains(">4<"));
        assert!(rendered.svg.contains("<path d=\"M") || rendered.svg.contains("<circle"));
    }

    #[test]
    fn renders_gantt_chart_svg_with_axis_and_task_bars() {
        let tokens = default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);
        let diagram = parser::parse(
            "gantt\n\
             title Release Plan\n\
             dateFormat YYYY-MM-DD\n\
             section Build\n\
             Compile :active, build, 2026-01-01, 3d\n\
             Ship :done, ship, after build, 2d",
        )
        .expect("gantt chart should parse");
        let layout = layout::layout(diagram, &opts);
        let rendered = render(&layout, &tokens, &opts);

        assert!(rendered.svg.contains("Release Plan"));
        assert!(rendered.svg.contains("Compile"));
        assert!(rendered.svg.contains("2026-01-01"));
        assert!(rendered.svg.contains("<rect"));
    }
}
