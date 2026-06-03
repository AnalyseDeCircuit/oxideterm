// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SVG generation for the supported Mermaid subset.

use oxideterm_theme::ThemeTokens;

use crate::mermaid::layout::{
    LaidOutDiagram, LaidOutDiagramKind, LaidOutGraph, LaidOutSequence, NodeBox, SubgraphBox,
};
use crate::mermaid::model::{
    GraphEdgeKind, GraphNodeShape, SequenceMessageKind, SequenceParticipantKind,
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
        LaidOutDiagramKind::Graph(graph) => render_graph(&mut svg, graph, tokens, opts),
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
}
