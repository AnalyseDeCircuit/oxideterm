//! Graph primitives shared by the standalone topology page and the runtime matrix.
use super::*;
use gpui::{PathBuilder, fill, point};
use oxideterm_topology::{
    TOPOLOGY_NODE_HEIGHT, TOPOLOGY_NODE_WIDTH, TopologyLayoutEdge, TopologyLayoutNode,
};

pub fn topology_canvas(
    theme: AppUiColors,
    has_background: bool,
    transform: TopologyTransform,
    edges: Vec<TopologyLayoutEdge>,
) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if !has_background {
                window.paint_quad(fill(bounds, rgb(theme.bg)));
            }
            let mut y = 0.0;
            while y <= f32::from(bounds.size.height) {
                let mut x = 0.0;
                while x <= f32::from(bounds.size.width) {
                    let dot_bounds = gpui::Bounds::new(
                        point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                        gpui::size(px(1.0), px(1.0)),
                    );
                    window.paint_quad(fill(
                        dot_bounds,
                        rgba((theme.text_muted << 8) | TOPOLOGY_BG_GRID_ALPHA),
                    ));
                    x += TOPOLOGY_BG_GRID_STEP;
                }
                y += TOPOLOGY_BG_GRID_STEP;
            }

            for edge in &edges {
                let start = point(
                    bounds.origin.x + px(topology_transform_x(edge.source_x, transform)),
                    bounds.origin.y
                        + px(topology_transform_y(
                            edge.source_y + TOPOLOGY_NODE_HEIGHT / 2.0,
                            transform,
                        )),
                );
                let end = point(
                    bounds.origin.x + px(topology_transform_x(edge.target_x, transform)),
                    bounds.origin.y
                        + px(topology_transform_y(
                            edge.target_y - TOPOLOGY_NODE_HEIGHT / 2.0,
                            transform,
                        )),
                );
                let delta_y = edge.target_y - edge.source_y;
                let control_a = point(
                    bounds.origin.x + px(topology_transform_x(edge.source_x, transform)),
                    bounds.origin.y
                        + px(topology_transform_y(
                            edge.source_y + delta_y * 0.4,
                            transform,
                        )),
                );
                let control_b = point(
                    bounds.origin.x + px(topology_transform_x(edge.target_x, transform)),
                    bounds.origin.y
                        + px(topology_transform_y(
                            edge.target_y - delta_y * 0.4,
                            transform,
                        )),
                );

                if edge.active {
                    let mut glow = PathBuilder::stroke(px(6.0 * transform.k));
                    glow.move_to(start);
                    glow.cubic_bezier_to(end, control_a, control_b);
                    if let Ok(path) = glow.build() {
                        window.paint_path(
                            path,
                            rgba(
                                (topology_view_status_color(edge.source_status) << 8)
                                    | TOPOLOGY_LINE_GLOW_ALPHA,
                            ),
                        );
                    }
                }

                let mut line =
                    PathBuilder::stroke(px(if edge.active { 2.5 } else { 1.5 } * transform.k));
                line.move_to(start);
                line.cubic_bezier_to(end, control_a, control_b);
                if let Ok(path) = line.build() {
                    window.paint_path(
                        path,
                        rgba(
                            (topology_view_status_color(edge.source_status) << 8)
                                | if edge.active {
                                    0xff
                                } else {
                                    TOPOLOGY_LINE_INACTIVE_ALPHA
                                },
                        ),
                    );
                }
            }
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

pub fn topology_node(
    tokens: &ThemeTokens,
    has_background: bool,
    node: &TopologyLayoutNode,
    transform: TopologyTransform,
    mono_font: SharedString,
) -> gpui::Div {
    let theme = tokens.ui;
    let status_color = topology_view_status_color(node.view_status);
    let is_down = node.view_status.is_down();
    let is_connecting = node.view_status.is_connecting();
    let scale = transform.k;
    let left = topology_transform_x(node.x, transform) - (TOPOLOGY_NODE_WIDTH * scale / 2.0);
    let top = topology_transform_y(node.y, transform) - (TOPOLOGY_NODE_HEIGHT * scale / 2.0);
    let connected_shadow = if node.view_status.is_connected() {
        vec![gpui::BoxShadow {
            inset: false,
            color: rgba((status_color << 8) | 0x30).into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(15.0),
            spread_radius: px(0.0),
        }]
    } else {
        Vec::new()
    };

    // Mirrors TopologyViewEnhanced NodeCard: fixed 140x50 glass panel with centered
    // status dot, semibold 11px name, and 9px mono host line.
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(TOPOLOGY_NODE_WIDTH * scale))
        .h(px(TOPOLOGY_NODE_HEIGHT * scale))
        .rounded(px(tokens.radii.lg * scale))
        .border_1()
        .border_color(if is_down {
            rgba((TOPOLOGY_FAILED << 8) | 0x66)
        } else {
            rgba((theme.border << 8) | TOPOLOGY_PANEL_BORDER_ALPHA_50)
        })
        .bg(if has_background {
            oxideterm_gpui_ui::color_for_background(
                theme.bg_card,
                true,
                oxideterm_gpui_ui::theme_glass_card_background_alpha(tokens),
            )
        } else {
            rgba((theme.bg_panel << 8) | TOPOLOGY_PANEL_BG_ALPHA_20)
        })
        .shadow(connected_shadow)
        .cursor_pointer()
        .hover(|style| {
            style
                .border_color(rgba((theme.accent << 8) | TOPOLOGY_PANEL_BORDER_ALPHA_50))
                .shadow(vec![gpui::BoxShadow {
                    inset: false,
                    color: rgba((theme.accent << 8) | 0x26).into(),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(20.0),
                    spread_radius: px(0.0),
                }])
        })
        .child(
            div()
                .size_full()
                .relative()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0 * scale))
                        .mb(px(2.0 * scale))
                        .child(
                            div()
                                .w(px(8.0 * scale))
                                .h(px(8.0 * scale))
                                .rounded_full()
                                .bg(rgb(status_color))
                                .when(is_down || is_connecting, |dot| {
                                    dot.shadow(vec![gpui::BoxShadow {
                                        inset: false,
                                        color: rgba((status_color << 8) | 0x66).into(),
                                        offset: point(px(0.0), px(0.0)),
                                        blur_radius: px(8.0),
                                        spread_radius: px(0.0),
                                    }])
                                }),
                        )
                        .child(
                            div()
                                .max_w(px(100.0 * scale))
                                .truncate()
                                .text_size(px(11.0 * scale))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.text))
                                .child(node.name.clone()),
                        ),
                )
                .child(
                    div()
                        .max_w(px(120.0 * scale))
                        .truncate()
                        .font_family(mono_font)
                        .text_size(px(9.0 * scale))
                        .text_color(rgba((theme.text_muted << 8) | TOPOLOGY_MUTED_TEXT_ALPHA_70))
                        .child(node.host.clone()),
                ),
        )
}
