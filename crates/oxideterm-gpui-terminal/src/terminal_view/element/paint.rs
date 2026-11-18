use gpui::{
    App, Bounds, Corners, DecorationRun, Hsla, PathBuilder, Pixels, Point, Rgba, SharedString,
    TextAlign, TextRun, Window, fill, point, px, rgb, rgba, size,
};
use oxideterm_terminal::TerminalCursorShape;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::cell_drawing::{cell_drawing_svg, is_cell_drawing};
use crate::terminal_ui::*;
use crate::terminal_view::element::{
    BatchedTextRun, TerminalCommandMarkOverlay, TerminalCursor, TerminalHorizontalScrollbar,
    TerminalImageLayout, TerminalRect, TerminalRowRect, TerminalRowTextRun, TerminalScrollbar,
    TerminalTextRunCache,
};
use crate::terminal_view::element::{
    PowerlineDirection, PowerlineShape, PowerlineWeight, powerline_separator,
};

const POWERLINE_SEAM_OVERLAP_DEVICE_PIXELS: f32 = 0.5;
const POWERLINE_THIN_STROKE_DEVICE_PIXELS: f32 = 1.4;
const POWERLINE_HALF_CIRCLE_KAPPA: f32 = 0.552_284_8;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PowerlinePaintMetrics {
    pub(crate) seam_overlap: Pixels,
    pub(crate) thin_stroke_width: Pixels,
}

impl PowerlinePaintMetrics {
    pub(crate) fn for_scale_factor(scale_factor: f32) -> Self {
        debug_assert!(scale_factor > 0.0);

        // Keep the overlap and thin stroke visually stable across display scale factors.
        Self {
            seam_overlap: px(POWERLINE_SEAM_OVERLAP_DEVICE_PIXELS / scale_factor),
            thin_stroke_width: px(POWERLINE_THIN_STROKE_DEVICE_PIXELS / scale_factor),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PowerlineHalfCircleCurve {
    pub(crate) start: Point<Pixels>,
    pub(crate) tip: Point<Pixels>,
    pub(crate) end: Point<Pixels>,
    pub(crate) upper_flat_control: Point<Pixels>,
    pub(crate) upper_tip_control: Point<Pixels>,
    pub(crate) lower_tip_control: Point<Pixels>,
    pub(crate) lower_flat_control: Point<Pixels>,
}

pub(crate) fn paint_terminal_rect(
    rect: &TerminalRect,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    paint_terminal_rect_at(
        rect.row,
        rect.col,
        rect.cells,
        rect.color,
        rect.powerline_cutout,
        origin,
        metrics,
        window,
    );
}

pub(crate) fn paint_terminal_row_rect(
    row: usize,
    rect: &TerminalRowRect,
    transition_colors: Option<&TerminalTransitionColors>,
    live_terminal_background: Hsla,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    let color = transition_colors.map_or(rect.color, |transition| {
        transition.resolve_row_color(rect.transition_key, rect.color, live_terminal_background)
    });
    paint_terminal_rect_at(
        row,
        rect.col,
        rect.cells,
        color,
        rect.powerline_cutout,
        origin,
        metrics,
        window,
    );
}

fn pixel_snap_bounds(bounds: Bounds<Pixels>, window: &Window) -> Bounds<Pixels> {
    Bounds::from_corners(
        window.pixel_snap_point(bounds.origin),
        window.pixel_snap_point(point(bounds.right(), bounds.bottom())),
    )
}

fn device_aligned_stroke(logical_px: f32, scale_factor: f32) -> Pixels {
    let scale = scale_factor.max(1.0);
    px((logical_px * scale).round().max(1.0) / scale)
}

fn paint_cell_outline(
    bounds: Bounds<Pixels>,
    stroke: Pixels,
    color: gpui::Hsla,
    window: &mut Window,
) {
    let width = bounds.size.width;
    let height = bounds.size.height;
    let stroke_x = stroke.min(width);
    let stroke_y = stroke.min(height);
    for edge in [
        Bounds::new(bounds.origin, size(width, stroke_y)),
        Bounds::new(
            point(
                bounds.left(),
                (bounds.bottom() - stroke_y).max(bounds.top()),
            ),
            size(width, stroke_y),
        ),
        Bounds::new(bounds.origin, size(stroke_x, height)),
        Bounds::new(
            point((bounds.right() - stroke_x).max(bounds.left()), bounds.top()),
            size(stroke_x, height),
        ),
    ] {
        window.paint_quad(fill(edge, color));
    }
}

fn paint_terminal_rect_at(
    row: usize,
    col: usize,
    cells: usize,
    color: Hsla,
    powerline_cutout: Option<char>,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    // Snap all cell background edges consistently so normal quads and Powerline
    // cutouts meet on the same device pixels without fractional seams.
    let bounds = pixel_snap_bounds(
        Bounds::new(
            origin
                + point(
                    px(col as f32 * metrics.cell_width_f32()),
                    px(row as f32 * metrics.line_height_f32()),
                ),
            size(
                px(cells as f32 * metrics.cell_width_f32()),
                metrics.line_height,
            ),
        ),
        window,
    );
    if let Some(ch) = powerline_cutout {
        if let Some(path) = powerline_background_path(ch, bounds) {
            window.paint_path(path, color);
        }
    } else {
        window.paint_quad(fill(bounds, color));
    }
}

fn underline_top_offset(metrics: &TerminalMetrics, thickness: Pixels) -> Pixels {
    let max_top = (metrics.line_height - thickness).max(px(0.0));
    if metrics.primary_ascent + metrics.primary_descent <= px(0.0) {
        return max_top;
    }
    gpui::underline_y_offset(
        metrics.line_height,
        metrics.primary_ascent,
        metrics.primary_descent,
    )
    .clamp(px(0.0), max_top)
}

pub(crate) fn paint_terminal_underline(
    rect: &TerminalRect,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    let thickness = device_aligned_stroke(
        (f32::from(metrics.font_size) / 14.0).clamp(1.0, 2.0),
        window.scale_factor(),
    );
    let y =
        px(rect.row as f32 * metrics.line_height_f32()) + underline_top_offset(metrics, thickness);
    let bounds = pixel_snap_bounds(
        Bounds::new(
            origin + point(px(rect.col as f32 * metrics.cell_width_f32()), y),
            size(px(rect.cells as f32 * metrics.cell_width_f32()), thickness),
        ),
        window,
    );
    window.paint_quad(fill(bounds, rect.color));
}

pub(crate) fn paint_terminal_outline(
    rect: &TerminalRect,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    let bounds = pixel_snap_bounds(
        Bounds::new(
            origin
                + point(
                    px(rect.col as f32 * metrics.cell_width_f32()),
                    px(rect.row as f32 * metrics.line_height_f32()),
                ),
            size(
                px(rect.cells as f32 * metrics.cell_width_f32()),
                metrics.line_height,
            ),
        ),
        window,
    );
    let stroke = device_aligned_stroke(1.0, window.scale_factor());
    paint_cell_outline(bounds, stroke, rect.color, window);
}

pub(crate) fn paint_command_mark_overlay(
    overlay: &TerminalCommandMarkOverlay,
    origin: gpui::Point<Pixels>,
    viewport_width: Pixels,
    metrics: &TerminalMetrics,
    decorations: bool,
    window: &mut Window,
) {
    let x = 0.0;
    let y = overlay.start_row as f32 * metrics.line_height_f32();
    // Command decorations fill the viewport, including the fractional trailing cell.
    let width = viewport_width;
    let height =
        (overlay.end_row.saturating_sub(overlay.start_row) + 1) as f32 * metrics.line_height_f32();
    let accent = command_mark_accent(overlay);
    let bounds = Bounds::new(origin + point(px(x), px(y)), size(width, px(height)));

    if decorations && let Some(fill_color) = command_mark_fill(overlay) {
        window.paint_quad(fill(bounds, fill_color));
    }

    if decorations && overlay.selected && overlay.has_top {
        window.paint_quad(fill(
            Bounds::new(origin + point(px(x), px(y)), size(width, px(1.0))),
            accent,
        ));
    }
    if decorations && overlay.selected && overlay.has_bottom {
        window.paint_quad(fill(
            Bounds::new(
                origin + point(px(x), px((y + height - 1.0).max(y))),
                size(width, px(1.0)),
            ),
            accent,
        ));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandGutterHead {
    Square,
    Dot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CommandGutterGeometry {
    stroke: Pixels,
    spine_x: f32,
    elbow_right_x: f32,
    head: Option<(CommandGutterHead, f32)>,
    square_half: f32,
    fold_rotation: f32,
    node_radius: f32,
    rail_top: f32,
    rail_bottom: f32,
    finished: bool,
    bend_radius: f32,
}

fn snap_stroke_center(coord: f32, stroke_px: f32, scale: f32) -> f32 {
    let scale = scale.max(1.0);
    let device_stroke = (stroke_px * scale).round().max(1.0) as i32;
    // Odd device-pixel strokes align to pixel centers (`n + 0.5`); even strokes align to edges (`n`).
    if device_stroke % 2 == 1 {
        ((coord * scale - 0.5).round() + 0.5) / scale
    } else {
        (coord * scale).round() / scale
    }
}

fn command_gutter_colors(
    overlay: &TerminalCommandMarkOverlay,
    theme: &TerminalUiTheme,
) -> (Rgba, Rgba) {
    let failed = overlay.exit_code.is_some_and(|code| code != 0) && !overlay.stale;
    let emphasized = overlay.hovered || overlay.selected || overlay.collapsed;
    let base = if overlay.stale {
        theme.foreground
    } else if failed {
        theme.tokens.ui.error
    } else if emphasized || overlay.running {
        theme.tokens.ui.accent
    } else {
        theme.foreground
    };

    let rail_alpha = if overlay.stale {
        0x30
    } else if emphasized {
        0xe0
    } else if failed {
        0x8c
    } else if overlay.running {
        0x99
    } else {
        0x48
    };
    let head_alpha = if overlay.stale {
        0x42
    } else if emphasized {
        0xf2
    } else if failed {
        0xb8
    } else if overlay.foldable || overlay.running {
        0xa8
    } else {
        0x6c
    };

    (
        rgba((base << 8) | rail_alpha),
        rgba((base << 8) | head_alpha),
    )
}

fn command_gutter_geometry(
    overlay: &TerminalCommandMarkOverlay,
    origin: Point<Pixels>,
    line_height: f32,
    width: f32,
    scale: f32,
) -> CommandGutterGeometry {
    let origin_x = f32::from(origin.x);
    let origin_y = f32::from(origin.y);
    let stroke = device_aligned_stroke(1.2, scale);
    let stroke_f = f32::from(stroke);
    // Offset slightly left of gutter center so chevrons and end bends leave breathing room before cell 0 prompts.
    let spine_x = snap_stroke_center(origin_x - width * 0.56, stroke_f, scale);
    let elbow_right_x = origin_x + (-width * 0.14).min(-1.2).max(-width * 0.35);

    let square_half = (width * 0.24).clamp(1.8, 2.5).min(line_height * 0.20);
    let fold_rotation = overlay.fold_rotation.clamp(0.0, 1.0);
    let node_radius = (width * 0.16).clamp(1.3, 2.0);

    let head = overlay.has_top.then(|| {
        let center_y = origin_y + (overlay.start_row as f32 + 0.5) * line_height;
        let kind = if overlay.foldable || overlay.collapsed {
            CommandGutterHead::Square
        } else {
            CommandGutterHead::Dot
        };
        (kind, center_y)
    });

    let rail_top = match head {
        Some((CommandGutterHead::Square, cy)) => {
            let theta = fold_rotation * std::f32::consts::FRAC_PI_4;
            let (sin_t, cos_t) = theta.sin_cos();
            let vertical_extent = square_half * (cos_t.abs() + sin_t.abs());
            cy + vertical_extent + 2.5
        }
        Some((CommandGutterHead::Dot, cy)) => cy + node_radius + 2.5,
        None => origin_y + overlay.start_row as f32 * line_height,
    };

    let finished = overlay.has_bottom && !overlay.running;
    let rail_bottom = if finished {
        snap_stroke_center(
            origin_y + (overlay.end_row as f32 + 0.5) * line_height,
            stroke_f,
            scale,
        )
    } else {
        origin_y + (overlay.end_row + 1) as f32 * line_height
    };

    let bend_span = (elbow_right_x - spine_x).max(0.0);
    let vertical_span = (rail_bottom - rail_top).max(0.0);
    let bend_radius = if finished && vertical_span > 1.0 {
        (bend_span * 0.72)
            .min(vertical_span * 0.45)
            .min(4.0)
            .max(0.0)
    } else {
        0.0
    };

    CommandGutterGeometry {
        stroke,
        spine_x,
        elbow_right_x,
        head,
        square_half,
        fold_rotation,
        node_radius,
        rail_top,
        rail_bottom,
        finished,
        bend_radius,
    }
}

pub(super) fn paint_command_gutter(
    overlay: &TerminalCommandMarkOverlay,
    origin: Point<Pixels>,
    metrics: &TerminalMetrics,
    width: f32,
    theme: &TerminalUiTheme,
    window: &mut Window,
) {
    let scale = window.scale_factor().max(1.0);
    let geom = command_gutter_geometry(overlay, origin, metrics.line_height_f32(), width, scale);
    let (rail_color, head_color) = command_gutter_colors(overlay, theme);

    if let Some((head, cy)) = geom.head {
        match head {
            CommandGutterHead::Square => {
                let half = geom.square_half;
                let theta = geom.fold_rotation * std::f32::consts::FRAC_PI_4;
                if theta <= 0.001 {
                    let size_px = half * 2.0;
                    let radius = (half * 0.35).min(0.8);
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(px(geom.spine_x - half), px(cy - half)),
                                size(px(size_px), px(size_px)),
                            ),
                            head_color,
                        )
                        .corner_radii(Corners::all(px(radius))),
                    );
                } else {
                    let (sin_t, cos_t) = theta.sin_cos();
                    let rotate = |dx: f32, dy: f32| {
                        point(
                            px(geom.spine_x + dx * cos_t - dy * sin_t),
                            px(cy + dx * sin_t + dy * cos_t),
                        )
                    };
                    let corners = [
                        rotate(-half, -half),
                        rotate(half, -half),
                        rotate(half, half),
                        rotate(-half, half),
                    ];
                    let mut builder = PathBuilder::fill();
                    builder.add_polygon(&corners, true);
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, head_color);
                    }
                }
            }
            CommandGutterHead::Dot => {
                let r = geom.node_radius;
                window.paint_quad(
                    fill(
                        Bounds::new(
                            point(px(geom.spine_x - r), px(cy - r)),
                            size(px(r * 2.0), px(r * 2.0)),
                        ),
                        head_color,
                    )
                    .corner_radii(Corners::all(px(r))),
                );
            }
        }
    }

    if geom.rail_bottom > geom.rail_top + 1.0 {
        let mut rail = PathBuilder::stroke(geom.stroke);
        rail.move_to(point(px(geom.spine_x), px(geom.rail_top)));
        if geom.finished {
            if geom.bend_radius >= 1.1 {
                rail.line_to(point(
                    px(geom.spine_x),
                    px(geom.rail_bottom - geom.bend_radius),
                ));
                rail.curve_to(
                    point(px(geom.spine_x + geom.bend_radius), px(geom.rail_bottom)),
                    point(px(geom.spine_x), px(geom.rail_bottom)),
                );
            } else {
                rail.line_to(point(px(geom.spine_x), px(geom.rail_bottom)));
            }
            if geom.elbow_right_x > geom.spine_x + 0.5 {
                rail.line_to(point(px(geom.elbow_right_x), px(geom.rail_bottom)));
            }
        } else {
            rail.line_to(point(px(geom.spine_x), px(geom.rail_bottom)));
        }
        if let Ok(path) = rail.build() {
            window.paint_path(path, rail_color);
        }
    }
}

fn command_mark_accent(overlay: &TerminalCommandMarkOverlay) -> Rgba {
    if overlay.stale {
        return rgba(0x94a3b8a8);
    }
    if overlay.running {
        return rgba(0x38bdf8d8);
    }
    match overlay.exit_code {
        Some(0) => rgba(0x22c55ed8),
        Some(_) => rgba(0xef4444e0),
        None => rgba(0xf59e0bd8),
    }
}

fn command_mark_fill(overlay: &TerminalCommandMarkOverlay) -> Option<Rgba> {
    if overlay.selected {
        return Some(if overlay.stale {
            rgba(0x94a3b812)
        } else if overlay.running {
            rgba(0x38bdf814)
        } else {
            match overlay.exit_code {
                Some(0) => rgba(0x22c55e10),
                Some(_) => rgba(0xef444414),
                None => rgba(0xf59e0b12),
            }
        });
    }

    overlay.hovered.then(|| {
        if overlay.stale {
            rgba(0x94a3b80a)
        } else {
            match overlay.exit_code {
                Some(0) => rgba(0x22c55e08),
                Some(_) => rgba(0xef44440a),
                _ if overlay.running => rgba(0x38bdf80a),
                None => rgba(0xf59e0b08),
            }
        }
    })
}

pub(crate) fn paint_terminal_image(
    image: &TerminalImageLayout,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    let mut bounds = Bounds::new(
        origin
            + point(
                px(image.image.snapshot.col as f32 * metrics.cell_width_f32()),
                px(
                    (image.image.snapshot.row as f32 - image.image.snapshot.clipped_rows as f32)
                        * metrics.line_height_f32(),
                ),
            ),
        size(
            px(image.image.snapshot.cols as f32 * metrics.cell_width_f32()),
            px(image.image.snapshot.rows as f32 * metrics.line_height_f32()),
        ),
    );

    let mut clip = bounds;
    let scale_factor = window.scale_factor();
    if let Some(sixel) = &image.image.snapshot.sixel {
        bounds.origin.x += px(sixel.column_offset as f32 * metrics.cell_width_f32());
        bounds.size = native_image_size(
            size(
                image.image.snapshot.pixel_width,
                image.image.snapshot.pixel_height,
            ),
            scale_factor,
        );
        clip = sixel_clip_bounds(
            bounds,
            size(image.image.snapshot.cols, image.image.snapshot.rows),
            point(sixel.clip_left, sixel.clip_top),
            point(sixel.clip_right, sixel.clip_bottom),
            size(px(metrics.cell_width_f32()), px(metrics.line_height_f32())),
        );
    }
    if let Some(kitty) = &image.image.snapshot.kitty {
        if let Some(region) = &kitty.unicode {
            clip.size = size(
                px(region.cols as f32 * metrics.cell_width_f32()),
                px(metrics.line_height_f32()),
            );
            bounds.origin.x -= px(region.col as f32 * metrics.cell_width_f32());
            bounds.origin.y -= px(region.row as f32 * metrics.line_height_f32());
        }
        bounds.origin.x += px(kitty.column_offset as f32 * metrics.cell_width_f32());
        bounds = kitty_image_bounds(
            bounds,
            size(
                image.image.snapshot.source_width,
                image.image.snapshot.source_height,
            ),
            point(kitty.offset_x, kitty.offset_y),
            kitty.scaled,
            scale_factor,
        );
        if kitty.unicode.is_none() {
            clip.origin.y += px(kitty.clip_top as f32 * metrics.line_height_f32());
            clip.size.height = px((image
                .image
                .snapshot
                .rows
                .saturating_sub(kitty.clip_top + kitty.clip_bottom))
                as f32
                * metrics.line_height_f32());
        }
    }
    let Some(render_image) = &image.image.render_image else {
        window.paint_quad(fill(bounds, rgba(0x528bff29)));
        return;
    };
    let frame_index = image.image.playback.frame;
    let next_frame = image.image.playback.next;
    if let Some(delay) = next_frame.filter(|_| window.is_window_active()) {
        window.request_animation_frame_after(delay, cx);
    }
    window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
        if let Some(sixel) = &image.image.snapshot.sixel
            && let Some([r, g, b]) = sixel.background_rgb()
        {
            window.paint_quad(fill(
                bounds,
                rgba((u32::from(r) << 24) | (u32::from(g) << 16) | (u32::from(b) << 8) | 255),
            ));
        }
        let _ = window.paint_image(
            bounds,
            bounds,
            Corners::all(px(0.0)),
            render_image.clone(),
            frame_index,
            false,
        );
    });
}

fn sixel_clip_bounds(
    mut pixels: Bounds<Pixels>,
    cells: gpui::Size<usize>,
    start: Point<usize>,
    end: Point<usize>,
    cell: gpui::Size<Pixels>,
) -> Bounds<Pixels> {
    let left = cell.width * start.x as f32;
    let top = cell.height * start.y as f32;
    let right = pixels
        .size
        .width
        .min(cell.width * cells.width.saturating_sub(end.x) as f32);
    let bottom = pixels
        .size
        .height
        .min(cell.height * cells.height.saturating_sub(end.y) as f32);
    pixels.origin += point(left, top);
    pixels.size = size((right - left).max(px(0.0)), (bottom - top).max(px(0.0)));
    pixels
}

fn native_image_size(source: gpui::Size<u32>, scale_factor: f32) -> gpui::Size<Pixels> {
    size(
        px(source.width as f32 / scale_factor),
        px(source.height as f32 / scale_factor),
    )
}

fn kitty_image_bounds(
    mut cells: Bounds<Pixels>,
    source: gpui::Size<u32>,
    offset: Point<u32>,
    scaled: bool,
    scale_factor: f32,
) -> Bounds<Pixels> {
    // Protocol geometry uses device pixels; GPUI painting uses logical pixels.
    cells.origin.x += px(offset.x as f32 / scale_factor);
    cells.origin.y += px(offset.y as f32 / scale_factor);
    if scaled {
        cells.size.width = (cells.size.width - px(offset.x as f32 / scale_factor)).max(px(0.0));
        cells.size.height = (cells.size.height - px(offset.y as f32 / scale_factor)).max(px(0.0));
        let ratio = (f32::from(cells.size.width) / source.width.max(1) as f32)
            .min(f32::from(cells.size.height) / source.height.max(1) as f32);
        let fitted = size(
            px(source.width as f32 * ratio),
            px(source.height as f32 * ratio),
        );
        cells.origin.x += (cells.size.width - fitted.width) / 2.0;
        cells.origin.y += (cells.size.height - fitted.height) / 2.0;
        cells.size = fitted;
    } else {
        cells.size = native_image_size(source, scale_factor);
    }
    cells
}

#[cfg(test)]
mod image_geometry_tests {
    use super::*;

    #[test]
    fn sixel_clips_partial_cell_rows_without_stretching() {
        let bounds = Bounds::new(point(px(4.0), px(8.0)), size(px(20.0), px(7.0)));
        assert_eq!(
            sixel_clip_bounds(
                bounds,
                size(2, 2),
                point(0, 0),
                point(0, 1),
                size(px(10.0), px(6.0))
            ),
            Bounds::new(point(px(4.0), px(8.0)), size(px(20.0), px(6.0)))
        );
        let narrow = Bounds::new(point(px(-4.0), px(8.0)), size(px(7.0), px(6.0)));
        assert_eq!(
            sixel_clip_bounds(
                narrow,
                size(2, 1),
                point(1, 0),
                point(0, 0),
                size(px(4.0), px(6.0))
            ),
            Bounds::new(point(px(0.0), px(8.0)), size(px(3.0), px(6.0)))
        );
        assert_eq!(
            sixel_clip_bounds(
                bounds,
                size(2, 2),
                point(0, 1),
                point(0, 0),
                size(px(10.0), px(6.0))
            ),
            Bounds::new(point(px(4.0), px(14.0)), size(px(20.0), px(1.0)))
        );
    }

    #[test]
    fn native_pixels_and_explicit_cell_boxes_have_distinct_sizes() {
        let cells = Bounds::new(point(px(20.0), px(40.0)), size(px(20.0), px(40.0)));
        assert_eq!(
            kitty_image_bounds(cells, size(8, 4), point(2, 4), false, 1.0),
            Bounds::new(point(px(22.0), px(44.0)), size(px(8.0), px(4.0)))
        );
        assert_eq!(
            kitty_image_bounds(cells, size(8, 4), point(2, 4), true, 1.0),
            Bounds::new(point(px(22.0), px(57.5)), size(px(18.0), px(9.0)))
        );
    }

    #[test]
    fn retina_native_images_fit_their_physical_cell_box() {
        let cells = Bounds::new(point(px(10.0), px(20.0)), size(px(20.0), px(20.0)));
        let image = kitty_image_bounds(cells, size(40, 40), point(0, 0), false, 2.0);
        assert_eq!(image.size, size(px(20.0), px(20.0)));
        let sixel = Bounds::new(cells.origin, native_image_size(size(40, 40), 2.0));
        assert_eq!(
            sixel_clip_bounds(
                sixel,
                size(2, 2),
                point(0, 0),
                point(0, 0),
                size(px(10.0), px(10.0))
            ),
            sixel
        );
        let offset = kitty_image_bounds(cells, size(40, 20), point(2, 4), false, 2.0);
        assert_eq!(
            offset,
            Bounds::new(point(px(11.0), px(22.0)), size(px(20.0), px(10.0)))
        );
        assert_eq!(
            kitty_image_bounds(cells, size(40, 20), point(2, 4), true, 2.0),
            Bounds::new(point(px(11.0), px(26.25)), size(px(19.0), px(9.5)))
        );
    }
}

pub(crate) fn paint_text_run(
    run: &BatchedTextRun,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    paint_text_run_with_layer(run, origin, metrics, true, window, cx);
}

struct TerminalPaintRun<'a> {
    row: usize,
    col: usize,
    text: &'a SharedString,
    cells: usize,
    style: &'a TextRun,
    color: Hsla,
    dilation_color_hint: Option<Hsla>,
    cache: Option<&'a TerminalTextRunCache>,
}

fn paint_text_run_with_layer(
    run: &BatchedTextRun,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    establish_layer: bool,
    window: &mut Window,
    cx: &mut App,
) {
    paint_text_run_parts(
        TerminalPaintRun {
            row: run.row,
            col: run.col,
            text: &run.text,
            cells: run.cells,
            style: &run.style,
            color: run.style.color,
            dilation_color_hint: None,
            cache: run.cache.as_deref(),
        },
        origin,
        metrics,
        establish_layer,
        window,
        cx,
    );
}

fn paint_text_run_parts(
    run: TerminalPaintRun<'_>,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    establish_layer: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if paint_cell_drawing_run(&run, origin, metrics, window, cx) {
        if run.style.underline.is_some() || run.style.strikethrough.is_some() {
            // Let the font's metrics position decorations, without drawing a
            // second copy of the cell graphic through the font fallback path.
            let spaces = SharedString::from(" ".repeat(run.cells));
            let mut style = run.style.clone();
            style.len = spaces.len();
            paint_text_run_parts(
                TerminalPaintRun {
                    row: run.row,
                    col: run.col,
                    text: &spaces,
                    cells: run.cells,
                    style: &style,
                    color: run.color,
                    dilation_color_hint: run.dilation_color_hint,
                    cache: run.cache,
                },
                origin,
                metrics,
                establish_layer,
                window,
                cx,
            );
        }
        return;
    }
    if paint_powerline_separators(&run, origin, metrics, window) {
        return;
    }

    let mut position = origin
        + point(
            px(run.col as f32 * metrics.cell_width_f32()),
            px(run.row as f32 * metrics.line_height_f32()),
        );
    if let Some(cache) = run.cache {
        // Stable terminal rows retain only glyph layout; one explicit decoration avoids the
        // large inline decoration capacity carried by `ShapedLine` for every cached run.
        let layout = cache.layout.get_or_init(|| {
            window.text_system().layout_line(
                run.text,
                metrics.font_size,
                std::slice::from_ref(run.style),
                Some(metrics.cell_width),
            )
        });
        position.y = aligned_run_position_y(
            position.y,
            metrics.primary_ascent,
            metrics.primary_descent,
            layout.ascent,
            layout.descent,
        );
        let decoration = DecorationRun {
            len: run.style.len as u32,
            color: run.color,
            background_color: run.style.background_color,
            underline: run.style.underline,
            strikethrough: run.style.strikethrough,
        };
        let decorations = std::slice::from_ref(&decoration);
        let _ = if run.dilation_color_hint.is_some() {
            if establish_layer {
                let row_bounds = Bounds::new(position, size(layout.width, metrics.line_height));
                window.paint_layer(row_bounds, |window| {
                    let _ = layout.paint_cached_in_current_layer_recolored(
                        position,
                        metrics.line_height,
                        decorations,
                        &cache.paint,
                        run.dilation_color_hint,
                        window,
                        cx,
                    );
                });
                Ok(())
            } else {
                layout.paint_cached_in_current_layer_recolored(
                    position,
                    metrics.line_height,
                    decorations,
                    &cache.paint,
                    run.dilation_color_hint,
                    window,
                    cx,
                )
            }
        } else if establish_layer {
            layout.paint_cached(
                position,
                metrics.line_height,
                decorations,
                &cache.paint,
                window,
                cx,
            )
        } else {
            layout.paint_cached_in_current_layer(
                position,
                metrics.line_height,
                decorations,
                &cache.paint,
                window,
                cx,
            )
        };
        return;
    }

    let adjusted_style = (run.color != run.style.color).then(|| TextRun {
        color: run.color,
        ..run.style.clone()
    });
    let style_slice = std::slice::from_ref(adjusted_style.as_ref().unwrap_or(run.style));
    let shaped = window.text_system().shape_line(
        run.text.clone(),
        metrics.font_size,
        style_slice,
        Some(metrics.cell_width),
    );
    position.y = aligned_run_position_y(
        position.y,
        metrics.primary_ascent,
        metrics.primary_descent,
        shaped.ascent,
        shaped.descent,
    );
    let _ = shaped.paint(
        position,
        metrics.line_height,
        TextAlign::Left,
        None,
        window,
        cx,
    );
}

struct CachedCellDrawing {
    name: SharedString,
    svg: String,
}

type CellDrawingCache =
    super::RecentCache<(char, u32, u32, u32), std::sync::Arc<CachedCellDrawing>>;

thread_local! {
    // Holds all 160 box/block and 255 active Braille characters across recent scales.
    static CELL_DRAWINGS: std::cell::RefCell<CellDrawingCache> =
        std::cell::RefCell::new(CellDrawingCache::new(1024));
}

fn paint_cell_drawing_run(
    run: &TerminalPaintRun<'_>,
    origin: Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
    cx: &App,
) -> bool {
    if !run.text.starts_with(is_cell_drawing)
        || run.text.chars().count() != run.cells
        || !run.text.chars().all(is_cell_drawing)
    {
        return false;
    }
    let scale = window.scale_factor();
    // Stroke weight is based on the nominal cell width so neighboring cells
    // with differently rounded edges still join with the same line thickness.
    let stroke = (metrics.cell_width.as_f32() * scale / 8.0).round().max(1.0) as u32;
    for (column, ch) in run.text.chars().enumerate() {
        if ch == '\u{2800}' {
            continue;
        }
        let raw = Bounds::new(
            origin
                + point(
                    metrics.cell_width * (run.col + column) as f32,
                    metrics.line_height * run.row as f32,
                ),
            size(metrics.cell_width, metrics.line_height),
        );
        let bounds = Bounds::from_corners(
            window.pixel_snap_point(raw.origin),
            window.pixel_snap_point(point(raw.right(), raw.bottom())),
        );
        let width = (bounds.size.width.as_f32() * scale).round() as u32;
        let height = (bounds.size.height.as_f32() * scale).round() as u32;
        if width == 0 || height == 0 {
            continue;
        }
        let drawing = CELL_DRAWINGS.with(|cache| {
            cache
                .borrow_mut()
                .get_or_insert_with((ch, width, height, stroke), || {
                    std::sync::Arc::new(CachedCellDrawing {
                        name: format!("oxideterm-cell-{:x}-{width}-{height}-{stroke}", ch as u32)
                            .into(),
                        svg: cell_drawing_svg(ch, width, height, stroke),
                    })
                })
                .0
        });
        if let Err(error) = window.paint_svg(
            bounds,
            drawing.name.clone(),
            Some(drawing.svg.as_bytes()),
            gpui::TransformationMatrix::default(),
            run.color,
            cx,
        ) {
            eprintln!("Failed to paint terminal cell drawing: {error}");
        }
    }
    true
}

pub(crate) fn paint_text_runs_by_row(
    runs: &[BatchedTextRun],
    origin: gpui::Point<Pixels>,
    terminal_cols: usize,
    metrics: &TerminalMetrics,
    glyph_edge_exclusions: Option<&[Vec<bool>]>,
    window: &mut Window,
    cx: &mut App,
) {
    for row_runs in runs.chunk_by(|left, right| left.row == right.row) {
        let row = row_runs[0].row;
        let row_bounds = Bounds::new(
            origin + point(px(0.0), px(row as f32 * metrics.line_height_f32())),
            size(
                px(terminal_cols as f32 * metrics.cell_width_f32()),
                metrics.line_height,
            ),
        );
        // The complete row bounds include glyph overhang that can extend beyond a run's cells.
        // One row-scoped layer still avoids a layer pair for every adjacent style run.
        window.paint_layer(row_bounds, |window| {
            for run in row_runs {
                let enhance =
                    enhance_run_glyph_edges(glyph_edge_exclusions, row, run.col, run.cells);
                window.with_enhanced_glyph_edges(enhance, |window| {
                    paint_text_run_with_layer(run, origin, metrics, false, window, cx);
                });
            }
        });
    }
}

pub(crate) fn paint_cached_text_runs_for_row(
    row: usize,
    runs: &[TerminalRowTextRun],
    origin: gpui::Point<Pixels>,
    terminal_cols: usize,
    metrics: &TerminalMetrics,
    glyph_edge_exclusions: Option<&[Vec<bool>]>,
    transition_colors: Option<&TerminalTransitionColors>,
    dilation_settle_row: Option<usize>,
    live_terminal_background: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    if runs.is_empty() {
        return;
    }
    let row_bounds = Bounds::new(
        origin + point(px(0.0), px(row as f32 * metrics.line_height_f32())),
        size(
            px(terminal_cols as f32 * metrics.cell_width_f32()),
            metrics.line_height,
        ),
    );
    window.paint_layer(row_bounds, |window| {
        for run in runs {
            let enhance = enhance_run_glyph_edges(glyph_edge_exclusions, row, run.col, run.cells);
            let live_color = transition_colors.map_or(run.style.color, |transition| {
                transition.resolve_row_color(
                    run.transition_key,
                    run.style.color,
                    live_terminal_background,
                )
            });
            let dilation_color_hint = if transition_colors.is_some()
                || dilation_settle_row.is_some_and(|settled_rows| row >= settled_rows)
            {
                Some(run.style.color)
            } else {
                None
            };
            window.with_enhanced_glyph_edges(enhance, |window| {
                paint_text_run_parts(
                    TerminalPaintRun {
                        row,
                        col: run.col,
                        text: &run.text,
                        cells: run.cells,
                        style: &run.style,
                        color: live_color,
                        dilation_color_hint,
                        cache: Some(&run.cache),
                    },
                    origin,
                    metrics,
                    false,
                    window,
                    cx,
                );
            });
        }
    });
}

fn enhance_run_glyph_edges(
    exclusions: Option<&[Vec<bool>]>,
    row: usize,
    col: usize,
    cells: usize,
) -> bool {
    exclusions
        .and_then(|rows| rows.get(row))
        .is_some_and(|excluded| {
            let end = col.saturating_add(cells).min(excluded.len());
            !excluded[col.min(end)..end].contains(&true)
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TerminalGhostTextSegment {
    pub(crate) col_offset: usize,
    pub(crate) text: String,
    pub(crate) cells: usize,
    pub(crate) cell_stride: usize,
}

/// Paints overlay text such as ghost text or an IME preedit with grapheme grid advances.
pub(crate) fn paint_grid_text_run(
    run: &BatchedTextRun,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(color) = run.style.background_color {
        // Preedit owns an opaque grid surface so application colors and glyphs cannot bleed through.
        paint_terminal_rect_at(
            run.row, run.col, run.cells, color, None, origin, metrics, window,
        );
    }
    for segment in ghost_text_grid_segments(&run.text) {
        let mut position = origin
            + point(
                px((run.col + segment.col_offset) as f32 * metrics.cell_width_f32()),
                px(run.row as f32 * metrics.line_height_f32()),
            );
        let mut style = run.style.clone();
        style.len = segment.text.len();
        let shaped = window.text_system().shape_line(
            SharedString::from(segment.text),
            metrics.font_size,
            &[style],
            Some(px(segment.cell_stride as f32 * metrics.cell_width_f32())),
        );
        position.y = aligned_run_position_y(
            position.y,
            metrics.primary_ascent,
            metrics.primary_descent,
            shaped.ascent,
            shaped.descent,
        );
        let _ = shaped.paint(
            position,
            metrics.line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

pub(crate) fn ghost_text_grid_segments(text: &str) -> Vec<TerminalGhostTextSegment> {
    // GPUI can force one glyph advance for a shaped line, but terminal ghost
    // text needs mixed advances: ASCII at one cell and CJK at two cells.
    // Segmenting keeps every run on the terminal grid without touching normal
    // terminal-buffer text rendering.
    let mut segments = Vec::new();
    let mut current_text = String::new();
    let mut current_cells = 0;
    let mut current_col_offset = 0;
    let mut current_cell_stride = None;
    let mut col_offset = 0;

    for grapheme in text.graphemes(true) {
        let cell_stride = grapheme.width();
        if cell_stride == 0 {
            current_text.push_str(grapheme);
            continue;
        }

        if current_cell_stride.is_none() {
            current_col_offset = col_offset;
            current_cell_stride = Some(cell_stride);
        } else if current_cell_stride != Some(cell_stride) {
            if !current_text.is_empty() {
                segments.push(TerminalGhostTextSegment {
                    col_offset: current_col_offset,
                    text: std::mem::take(&mut current_text),
                    cells: current_cells,
                    cell_stride: current_cell_stride.unwrap_or(1),
                });
            }
            current_col_offset = col_offset;
            current_cell_stride = Some(cell_stride);
            current_cells = 0;
        }

        current_text.push_str(grapheme);
        current_cells += cell_stride;
        col_offset += cell_stride;
    }

    if !current_text.is_empty() {
        segments.push(TerminalGhostTextSegment {
            col_offset: current_col_offset,
            text: current_text,
            cells: current_cells,
            cell_stride: current_cell_stride.unwrap_or(1),
        });
    }

    segments
}

fn paint_powerline_separators(
    run: &TerminalPaintRun<'_>,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) -> bool {
    let mut run_chars = run.text.chars();
    let Some(first) = run_chars.next() else {
        return false;
    };
    // Avoid collecting ordinary text runs that cannot contain Powerline separators.
    if powerline_separator(first).is_none() {
        return false;
    }

    let chars = std::iter::once(first).chain(run_chars);
    if chars.clone().count() != run.cells
        || !chars.clone().all(|ch| powerline_separator(ch).is_some())
    {
        return false;
    }

    // Separator runs are short, so validating twice is cheaper than allocating
    // a character buffer while still preventing partially painted mixed runs.
    let paint_metrics = PowerlinePaintMetrics::for_scale_factor(window.scale_factor());

    for (offset, ch) in chars.enumerate() {
        let raw_bounds = Bounds::new(
            origin
                + point(
                    px((run.col + offset) as f32 * metrics.cell_width_f32()),
                    px(run.row as f32 * metrics.line_height_f32()),
                ),
            size(metrics.cell_width, metrics.line_height),
        );
        // Background quads snap each edge independently, so custom paths must use the same cell
        // edges or fractional metrics can leave a one-device-pixel mismatch between directions.
        let bounds = Bounds::from_corners(
            window.pixel_snap_point(raw_bounds.origin),
            window.pixel_snap_point(point(raw_bounds.right(), raw_bounds.bottom())),
        );
        let Some(separator) = powerline_separator(ch) else {
            return false;
        };
        match (separator.shape, separator.weight) {
            (PowerlineShape::Triangle, PowerlineWeight::Filled) => {
                let Some(points) = powerline_separator_points(ch, bounds, paint_metrics) else {
                    return false;
                };
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&points, true);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.color);
                }
            }
            (PowerlineShape::Triangle, PowerlineWeight::Thin) => {
                let Some(points) = powerline_separator_points(ch, bounds, paint_metrics) else {
                    return false;
                };
                let mut builder = PathBuilder::stroke(paint_metrics.thin_stroke_width);
                builder.move_to(points[0]);
                builder.line_to(points[2]);
                builder.line_to(points[1]);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.color);
                }
            }
            (PowerlineShape::HalfCircle, PowerlineWeight::Filled) => {
                let Some(curve) = powerline_half_circle_curve(ch, bounds, paint_metrics) else {
                    return false;
                };
                let mut builder = PathBuilder::fill();
                builder.move_to(curve.start);
                builder.line_to(curve.end);
                push_half_circle_arc(&mut builder, &curve, true);
                builder.close();
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.color);
                }
            }
            (PowerlineShape::HalfCircle, PowerlineWeight::Thin) => {
                let Some(curve) = powerline_half_circle_curve(ch, bounds, paint_metrics) else {
                    return false;
                };
                let mut builder = PathBuilder::stroke(paint_metrics.thin_stroke_width);
                builder.move_to(curve.start);
                push_half_circle_arc(&mut builder, &curve, false);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.color);
                }
            }
        }
    }

    true
}

fn aligned_run_position_y(
    position_y: Pixels,
    primary_ascent: Pixels,
    primary_descent: Pixels,
    layout_ascent: Pixels,
    layout_descent: Pixels,
) -> Pixels {
    // GPUI centers each shaped run with its own max ascent/descent; cancel the
    // delta so fallback symbols (e.g., prompt icons or CJK) share one row baseline.
    if primary_ascent + primary_descent <= px(0.0) {
        return position_y;
    }
    position_y + ((primary_ascent - primary_descent) - (layout_ascent - layout_descent)) * 0.5
}

fn push_half_circle_arc(
    builder: &mut PathBuilder,
    curve: &PowerlineHalfCircleCurve,
    reverse: bool,
) {
    if reverse {
        builder.cubic_bezier_to(curve.tip, curve.lower_flat_control, curve.lower_tip_control);
        builder.cubic_bezier_to(
            curve.start,
            curve.upper_tip_control,
            curve.upper_flat_control,
        );
    } else {
        builder.cubic_bezier_to(curve.tip, curve.upper_flat_control, curve.upper_tip_control);
        builder.cubic_bezier_to(curve.end, curve.lower_tip_control, curve.lower_flat_control);
    }
}

fn powerline_background_path(ch: char, bounds: Bounds<Pixels>) -> Option<gpui::Path<Pixels>> {
    let separator = powerline_separator(ch)?;
    if separator.weight != PowerlineWeight::Filled {
        return None;
    }
    // The opaque complement stays inside this cell; the transparent glyph needs no seam overlap.
    let metrics = PowerlinePaintMetrics {
        seam_overlap: px(0.0),
        thin_stroke_width: px(0.0),
    };
    let outer_x = match separator.direction {
        PowerlineDirection::Right => bounds.right(),
        PowerlineDirection::Left => bounds.left(),
    };
    let mut builder = PathBuilder::fill();
    match separator.shape {
        PowerlineShape::Triangle => {
            let [top, bottom, tip] = powerline_separator_points(ch, bounds, metrics)?;
            builder.add_polygon(&[top, point(outer_x, top.y), tip], true);
            builder.add_polygon(&[tip, point(outer_x, bottom.y), bottom], true);
        }
        PowerlineShape::HalfCircle => {
            let curve = powerline_half_circle_curve(ch, bounds, metrics)?;
            builder.move_to(curve.start);
            builder.line_to(point(outer_x, curve.start.y));
            builder.line_to(point(outer_x, curve.end.y));
            builder.line_to(curve.end);
            push_half_circle_arc(&mut builder, &curve, true);
            builder.close();
        }
    }
    builder.build().ok()
}

#[cfg(test)]
mod powerline_background_tests {
    use super::*;

    #[test]
    fn powerline_cutouts_leave_the_glyph_interior_unpainted() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(8.0), px(16.0)));
        for (ch, left) in [
            ('\u{e0b0}', false),
            ('\u{e0b2}', true),
            ('\u{e0b4}', false),
            ('\u{e0b6}', true),
        ] {
            let path = powerline_background_path(ch, bounds).expect("filled separator path");
            // Sample the tessellated paint geometry, not a second implementation of the curve.
            for (x, y, painted) in [
                (0.1, 0.5, false),
                (0.7, 0.5, false),
                (0.9, 0.1, true),
                (0.9, 0.9, true),
            ] {
                let x = if left { 1.0 - x } else { x };
                let sample = point(px(10.0 + x * 8.0), px(20.0 + y * 16.0));
                let covered = path.vertices.chunks_exact(3).any(|triangle| {
                    let cross = |a: Point<Pixels>, b: Point<Pixels>| {
                        f32::from(b.x - a.x) * f32::from(sample.y - a.y)
                            - f32::from(b.y - a.y) * f32::from(sample.x - a.x)
                    };
                    let signs = [
                        cross(triangle[0].xy_position, triangle[1].xy_position),
                        cross(triangle[1].xy_position, triangle[2].xy_position),
                        cross(triangle[2].xy_position, triangle[0].xy_position),
                    ];
                    signs.iter().all(|s| *s >= 0.0) || signs.iter().all(|s| *s <= 0.0)
                });
                assert_eq!(covered, painted, "{ch}: sample ({x}, {y})");
            }
        }
    }

    #[test]
    fn aligned_run_position_y_keeps_fallback_runs_on_primary_baseline() {
        let primary_ascent = px(14.28);
        let primary_descent = px(4.20);
        let line_height = px(16.63);
        let origin_y = px(20.0);

        let baseline_from = |pos_y: Pixels, ascent: Pixels, descent: Pixels| {
            pos_y + (line_height - ascent - descent) * 0.5 + ascent
        };

        let adjusted_y = aligned_run_position_y(
            origin_y,
            primary_ascent,
            primary_descent,
            px(15.50),
            px(6.10),
        );
        let primary_baseline = baseline_from(origin_y, primary_ascent, primary_descent);
        let adjusted_baseline = baseline_from(adjusted_y, px(15.50), px(6.10));

        assert!((f32::from(adjusted_baseline) - f32::from(primary_baseline)).abs() < 0.001);
    }

    #[test]
    fn device_aligned_stroke_snaps_to_physical_pixels() {
        for (logical, scale, expected) in [
            (1.0, 1.0, 1.0),
            (1.0, 1.25, 0.8),
            (1.0, 1.5, 2.0 / 1.5),
            (2.0, 1.25, 2.4),
            (1.0, 2.0, 1.0),
        ] {
            let snapped = f32::from(device_aligned_stroke(logical, scale));
            assert!((snapped - expected).abs() < 0.001);
            let device_px = snapped * scale;
            assert!((device_px - device_px.round()).abs() < 0.001);
        }
    }

    #[test]
    fn underline_top_offset_tracks_primary_font_baseline_inside_cell() {
        let mut metrics = TerminalMetrics {
            font: terminal_font_with_family_and_cjk(
                TERMINAL_FONT,
                None,
                TERMINAL_FONT_LIGATURES,
                TERMINAL_FONT_WEIGHT,
            ),
            font_size: px(14.0),
            cell_width: px(8.4),
            letter_spacing: None,
            line_height: px(18.48),
            primary_ascent: px(14.28),
            primary_descent: px(4.20),
        };
        let thickness = px(1.0);
        let expected = gpui::underline_y_offset(
            metrics.line_height,
            metrics.primary_ascent,
            metrics.primary_descent,
        );
        assert_eq!(underline_top_offset(&metrics, thickness), expected);

        // Compact line heights clamp the underline inside the row instead of bleeding into row+1.
        metrics.line_height = px(16.63);
        let max_top = metrics.line_height - thickness;
        assert_eq!(underline_top_offset(&metrics, thickness), max_top);

        metrics.primary_ascent = px(0.0);
        metrics.primary_descent = px(0.0);
        assert_eq!(underline_top_offset(&metrics, thickness), max_top);
    }

    #[test]
    fn command_gutter_geometry_keeps_end_elbow_and_clearance_before_cell_zero() {
        let base_overlay = TerminalCommandMarkOverlay {
            start_row: 1,
            end_row: 4,
            has_top: true,
            has_bottom: true,
            stale: false,
            selected: false,
            hovered: false,
            running: false,
            exit_code: Some(0),
            collapsed: false,
            foldable: true,
            fold_rotation: 0.0,
        };
        let origin = point(px(24.0), px(10.0));
        let expanded = command_gutter_geometry(&base_overlay, origin, 20.0, 9.0, 2.0);
        assert_eq!(
            expanded.head.map(|(kind, _)| kind),
            Some(CommandGutterHead::Square)
        );
        assert_eq!(expanded.fold_rotation, 0.0);
        assert!(expanded.finished);
        assert!(expanded.elbow_right_x > expanded.spine_x);
        assert!(expanded.elbow_right_x <= f32::from(origin.x) - 1.2);
        assert!(expanded.bend_radius >= 1.1);
        assert!(expanded.rail_bottom > expanded.rail_top);

        let collapsed = command_gutter_geometry(
            &TerminalCommandMarkOverlay {
                collapsed: true,
                fold_rotation: 1.0,
                end_row: 2,
                ..base_overlay.clone()
            },
            origin,
            20.0,
            9.0,
            2.0,
        );
        assert_eq!(
            collapsed.head.map(|(kind, _)| kind),
            Some(CommandGutterHead::Square)
        );
        assert_eq!(collapsed.fold_rotation, 1.0);
        // Rotated 45-degree diamond projects sqrt(2) * half vertically, pushing rail_top slightly lower.
        assert!(collapsed.rail_top > expanded.rail_top);
        assert!(collapsed.finished);
        assert!(collapsed.rail_bottom > collapsed.rail_top);

        let running = command_gutter_geometry(
            &TerminalCommandMarkOverlay {
                running: true,
                has_bottom: false,
                foldable: false,
                ..base_overlay
            },
            origin,
            20.0,
            9.0,
            2.0,
        );
        assert_eq!(
            running.head.map(|(kind, _)| kind),
            Some(CommandGutterHead::Dot)
        );
        assert!(!running.finished);
        assert_eq!(running.bend_radius, 0.0);
    }
}

pub(crate) fn powerline_separator_points(
    ch: char,
    bounds: Bounds<Pixels>,
    metrics: PowerlinePaintMetrics,
) -> Option<[Point<Pixels>; 3]> {
    let separator = powerline_separator(ch)?;
    if separator.shape != PowerlineShape::Triangle {
        return None;
    }

    let extents = powerline_shape_extents(bounds, separator.direction, separator.weight, metrics);

    Some([
        point(px(extents.flat_x), px(extents.top)),
        point(px(extents.flat_x), px(extents.bottom)),
        point(px(extents.tip_x), px(extents.middle_y)),
    ])
}

pub(crate) fn powerline_half_circle_curve(
    ch: char,
    bounds: Bounds<Pixels>,
    metrics: PowerlinePaintMetrics,
) -> Option<PowerlineHalfCircleCurve> {
    let separator = powerline_separator(ch)?;
    if separator.shape != PowerlineShape::HalfCircle {
        return None;
    }

    let extents = powerline_shape_extents(bounds, separator.direction, separator.weight, metrics);
    let rx = extents.tip_x - extents.flat_x;
    let ry = (extents.bottom - extents.top) * 0.5;
    let cx = extents.flat_x + rx * POWERLINE_HALF_CIRCLE_KAPPA;
    let cy_offset = ry * POWERLINE_HALF_CIRCLE_KAPPA;

    // Two kappa quarter-ellipse segments avoid the flat horizontal top/bottom
    // shelf produced by a single cubic curve across narrow terminal cells.
    Some(PowerlineHalfCircleCurve {
        start: point(px(extents.flat_x), px(extents.top)),
        tip: point(px(extents.tip_x), px(extents.middle_y)),
        end: point(px(extents.flat_x), px(extents.bottom)),
        upper_flat_control: point(px(cx), px(extents.top)),
        upper_tip_control: point(px(extents.tip_x), px(extents.middle_y - cy_offset)),
        lower_tip_control: point(px(extents.tip_x), px(extents.middle_y + cy_offset)),
        lower_flat_control: point(px(cx), px(extents.bottom)),
    })
}

#[derive(Clone, Copy, Debug)]
struct PowerlineShapeExtents {
    flat_x: f32,
    tip_x: f32,
    top: f32,
    bottom: f32,
    middle_y: f32,
}

fn powerline_shape_extents(
    bounds: Bounds<Pixels>,
    direction: PowerlineDirection,
    weight: PowerlineWeight,
    metrics: PowerlinePaintMetrics,
) -> PowerlineShapeExtents {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);
    let seam_overlap = f32::from(metrics.seam_overlap);
    let thin_inset = f32::from(metrics.thin_stroke_width) / 2.0;

    let (flat_x, tip_x, shape_top, shape_bottom) = match (direction, weight) {
        (PowerlineDirection::Right, PowerlineWeight::Filled) => {
            (left - seam_overlap, right, top, bottom)
        }
        (PowerlineDirection::Left, PowerlineWeight::Filled) => {
            (right + seam_overlap, left, top, bottom)
        }
        (PowerlineDirection::Right, PowerlineWeight::Thin) => (
            left + thin_inset,
            right - thin_inset,
            top + thin_inset,
            bottom - thin_inset,
        ),
        (PowerlineDirection::Left, PowerlineWeight::Thin) => (
            right - thin_inset,
            left + thin_inset,
            top + thin_inset,
            bottom - thin_inset,
        ),
    };

    PowerlineShapeExtents {
        flat_x,
        tip_x,
        top: shape_top,
        bottom: shape_bottom,
        middle_y: (top + bottom) / 2.0,
    }
}

pub(crate) fn paint_cursor(
    cursor: TerminalCursor,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    cursor_color: u32,
    window: &mut Window,
) {
    let cell_width = metrics.cell_width_f32();
    let line_height = metrics.line_height_f32();
    let scale = window.scale_factor();
    let color: gpui::Hsla = rgb(cursor_color).into();
    let cell_bounds = pixel_snap_bounds(
        Bounds::new(
            origin
                + point(
                    px(cursor.col as f32 * cell_width),
                    px(cursor.row as f32 * line_height),
                ),
            size(metrics.cell_width, metrics.line_height),
        ),
        window,
    );
    match cursor.shape {
        TerminalCursorShape::Block | TerminalCursorShape::Hidden => {}
        TerminalCursorShape::Underline => {
            let stroke = device_aligned_stroke(2.0, scale).min(cell_bounds.size.height);
            let bounds = Bounds::new(
                point(
                    cell_bounds.left(),
                    (cell_bounds.bottom() - stroke).max(cell_bounds.top()),
                ),
                size(cell_bounds.size.width, stroke),
            );
            window.paint_quad(fill(bounds, color));
        }
        TerminalCursorShape::Bar => {
            let stroke = device_aligned_stroke(2.0, scale).min(cell_bounds.size.width);
            let bounds = Bounds::new(cell_bounds.origin, size(stroke, cell_bounds.size.height));
            window.paint_quad(fill(bounds, color));
        }
        TerminalCursorShape::Hollow => {
            let stroke = device_aligned_stroke(1.0, scale);
            paint_cell_outline(cell_bounds, stroke, color, window);
        }
    }
}

pub(crate) fn paint_scrollbar(
    scrollbar: TerminalScrollbar,
    origin: gpui::Point<Pixels>,
    viewport_width: Pixels,
    foreground: u32,
    active: bool,
    window: &mut Window,
) {
    let width = if active {
        SCROLLBAR_ACTIVE_WIDTH
    } else {
        SCROLLBAR_WIDTH
    };
    // Grow inward so the overlay stays anchored to the same viewport edge.
    let x = terminal_scrollbar_x_for_viewport(viewport_width) - px(width - SCROLLBAR_WIDTH);
    let thumb = Bounds::new(
        origin + point(x, px(scrollbar.top)),
        size(px(width), px(scrollbar.height)),
    );
    let alpha = if active { 0x99 } else { 0x66 };
    window.paint_quad(fill(thumb, rgba((foreground << 8) | alpha)).corner_radii(px(width / 2.0)));
}

pub(crate) fn paint_horizontal_scrollbar(
    scrollbar: TerminalHorizontalScrollbar,
    origin: gpui::Point<Pixels>,
    viewport_width: Pixels,
    viewport_height: Pixels,
    window: &mut Window,
) {
    let y = viewport_height - px(SCROLLBAR_WIDTH);
    let track = Bounds::new(
        origin + point(px(0.0), y),
        size(viewport_width.max(px(0.0)), px(SCROLLBAR_WIDTH)),
    );
    window.paint_quad(fill(track, rgba(0xffffff20)));

    let thumb = Bounds::new(
        origin + point(px(scrollbar.left), y),
        size(px(scrollbar.width), px(SCROLLBAR_WIDTH)),
    );
    window.paint_quad(fill(thumb, rgba(0xffffff66)));
}
