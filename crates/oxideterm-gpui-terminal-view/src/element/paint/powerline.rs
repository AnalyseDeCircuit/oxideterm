use super::*;
use crate::element::{PowerlineDirection, PowerlineShape, PowerlineWeight, powerline_separator};

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

pub(super) fn paint_powerline_separators(
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
            (
                PowerlineShape::SlantedLower | PowerlineShape::SlantedUpper,
                PowerlineWeight::Filled,
            ) => {
                let Some(points) = powerline_slanted_points(separator, bounds, paint_metrics)
                else {
                    return false;
                };
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&points, true);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.color);
                }
            }
            (
                PowerlineShape::SlantedLower | PowerlineShape::SlantedUpper,
                PowerlineWeight::Thin,
            ) => {
                let [a, b] = powerline_diagonal_line(separator, bounds, paint_metrics);
                let mut builder = PathBuilder::stroke(paint_metrics.thin_stroke_width);
                builder.move_to(a);
                builder.line_to(b);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.color);
                }
            }
            (PowerlineShape::Trapezoid, PowerlineWeight::Filled) => {
                let [upper, lower] =
                    powerline_trapezoid_polygons(separator.direction, bounds, paint_metrics);
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&upper, true);
                builder.add_polygon(&lower, true);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.color);
                }
            }
            (PowerlineShape::InverseTriangle, PowerlineWeight::Filled) => {
                let [upper, lower] =
                    powerline_inverse_triangle_polygons(separator.direction, bounds, paint_metrics);
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&upper, true);
                builder.add_polygon(&lower, true);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.color);
                }
            }
            (PowerlineShape::PixelatedSmall, PowerlineWeight::Filled) => {
                paint_powerline_pixelated(bounds, separator.direction, false, run.color, window);
            }
            (PowerlineShape::PixelatedBig, PowerlineWeight::Filled) => {
                paint_powerline_pixelated(bounds, separator.direction, true, run.color, window);
            }
            (
                PowerlineShape::Trapezoid
                | PowerlineShape::InverseTriangle
                | PowerlineShape::PixelatedSmall
                | PowerlineShape::PixelatedBig,
                PowerlineWeight::Thin,
            ) => return false,
        }
    }

    true
}

fn powerline_slanted_points(
    separator: crate::element::PowerlineSeparator,
    bounds: Bounds<Pixels>,
    metrics: PowerlinePaintMetrics,
) -> Option<[Point<Pixels>; 3]> {
    let extents = powerline_shape_extents(bounds, separator.direction, separator.weight, metrics);
    match separator.shape {
        PowerlineShape::SlantedLower => Some([
            point(px(extents.flat_x), px(extents.top)),
            point(px(extents.flat_x), px(extents.bottom)),
            point(px(extents.tip_x), px(extents.bottom)),
        ]),
        PowerlineShape::SlantedUpper => Some([
            point(px(extents.flat_x), px(extents.top)),
            point(px(extents.tip_x), px(extents.top)),
            point(px(extents.flat_x), px(extents.bottom)),
        ]),
        _ => None,
    }
}

fn powerline_diagonal_line(
    separator: crate::element::PowerlineSeparator,
    bounds: Bounds<Pixels>,
    metrics: PowerlinePaintMetrics,
) -> [Point<Pixels>; 2] {
    let extents = powerline_shape_extents(bounds, separator.direction, separator.weight, metrics);
    match separator.shape {
        PowerlineShape::SlantedLower => [
            point(px(extents.flat_x), px(extents.top)),
            point(px(extents.tip_x), px(extents.bottom)),
        ],
        PowerlineShape::SlantedUpper => [
            point(px(extents.flat_x), px(extents.bottom)),
            point(px(extents.tip_x), px(extents.top)),
        ],
        _ => [
            point(px(extents.flat_x), px(extents.top)),
            point(px(extents.tip_x), px(extents.bottom)),
        ],
    }
}

// Proportions measured from Nerd Fonts `nf-ple-trapezoid_top_bottom` (U+E0D2 / U+E0D4).
const POWERLINE_TRAPEZOID_INNER_X_FRACTION: f32 = 0.32;
const POWERLINE_TRAPEZOID_UPPER_Y_FRACTION: f32 = 0.44;
const POWERLINE_TRAPEZOID_LOWER_Y_FRACTION: f32 = 0.56;

fn powerline_trapezoid_polygons(
    direction: PowerlineDirection,
    bounds: Bounds<Pixels>,
    metrics: PowerlinePaintMetrics,
) -> [[Point<Pixels>; 4]; 2] {
    let extents = powerline_shape_extents(bounds, direction, PowerlineWeight::Filled, metrics);
    let left = f32::from(bounds.origin.x);
    let width = f32::from(bounds.size.width);
    let height = extents.bottom - extents.top;
    let inner_x = match direction {
        PowerlineDirection::Right => left + width * POWERLINE_TRAPEZOID_INNER_X_FRACTION,
        PowerlineDirection::Left => left + width * (1.0 - POWERLINE_TRAPEZOID_INNER_X_FRACTION),
    };
    let upper_y = extents.top + height * POWERLINE_TRAPEZOID_UPPER_Y_FRACTION;
    let lower_y = extents.top + height * POWERLINE_TRAPEZOID_LOWER_Y_FRACTION;
    let upper = [
        point(px(extents.flat_x), px(extents.top)),
        point(px(extents.tip_x), px(extents.top)),
        point(px(inner_x), px(upper_y)),
        point(px(extents.flat_x), px(upper_y)),
    ];
    let lower = [
        point(px(extents.flat_x), px(lower_y)),
        point(px(inner_x), px(lower_y)),
        point(px(extents.tip_x), px(extents.bottom)),
        point(px(extents.flat_x), px(extents.bottom)),
    ];
    [upper, lower]
}

fn powerline_inverse_triangle_polygons(
    direction: PowerlineDirection,
    bounds: Bounds<Pixels>,
    metrics: PowerlinePaintMetrics,
) -> [[Point<Pixels>; 3]; 2] {
    let extents = powerline_shape_extents(bounds, direction, PowerlineWeight::Filled, metrics);
    let upper = [
        point(px(extents.flat_x), px(extents.top)),
        point(px(extents.tip_x), px(extents.top)),
        point(px(extents.flat_x), px(extents.middle_y)),
    ];
    let lower = [
        point(px(extents.flat_x), px(extents.middle_y)),
        point(px(extents.tip_x), px(extents.bottom)),
        point(px(extents.flat_x), px(extents.bottom)),
    ];
    [upper, lower]
}

fn paint_powerline_pixelated(
    bounds: Bounds<Pixels>,
    direction: PowerlineDirection,
    big: bool,
    color: Hsla,
    window: &mut Window,
) {
    // 0 = empty, 1 = faint trailing pixel, 2 = solid pixel (anchored on the left for Right direction).
    const SMALL_ROWS: [[u8; 6]; 10] = [
        [2, 2, 2, 2, 1, 0],
        [2, 2, 2, 1, 2, 1],
        [2, 2, 2, 2, 0, 0],
        [2, 2, 1, 2, 1, 0],
        [2, 2, 2, 0, 2, 0],
        [2, 2, 2, 2, 1, 0],
        [2, 2, 1, 2, 0, 1],
        [2, 1, 2, 1, 1, 0],
        [2, 2, 2, 2, 0, 0],
        [2, 2, 2, 1, 1, 0],
    ];
    const BIG_ROWS: [[u8; 4]; 6] = [
        [2, 2, 2, 0],
        [2, 2, 1, 1],
        [2, 2, 2, 0],
        [2, 1, 2, 0],
        [2, 2, 1, 2],
        [2, 2, 2, 0],
    ];

    let scale = window.scale_factor();
    let left_px = (f32::from(bounds.left()) * scale).round();
    let top_px = (f32::from(bounds.top()) * scale).round();
    let right_px = (f32::from(bounds.right()) * scale).round();
    let bottom_px = (f32::from(bounds.bottom()) * scale).round();
    let total_w = right_px - left_px;
    let total_h = bottom_px - top_px;
    if total_w <= 0.0 || total_h <= 0.0 {
        return;
    }

    let (cols, rows): (usize, usize) = if big { (4, 6) } else { (6, 10) };
    let gutter = if total_w / cols as f32 >= 3.0 {
        1.0
    } else {
        0.0
    };

    for row in 0..rows {
        let y0 = (top_px + total_h * row as f32 / rows as f32).round();
        let y1 = ((top_px + total_h * (row + 1) as f32 / rows as f32).round() - gutter)
            .max(y0 + 1.0)
            .min(bottom_px);
        for col in 0..cols {
            let level = if big {
                BIG_ROWS[row][col]
            } else {
                SMALL_ROWS[row][col]
            };
            if level == 0 {
                continue;
            }
            let visual_col = match direction {
                PowerlineDirection::Right => col,
                PowerlineDirection::Left => cols - 1 - col,
            };
            let x0 = (left_px + total_w * visual_col as f32 / cols as f32).round();
            let x1 = ((left_px + total_w * (visual_col + 1) as f32 / cols as f32).round() - gutter)
                .max(x0 + 1.0)
                .min(right_px);
            if x1 > x0 && y1 > y0 {
                let mut quad_color = color;
                if level == 1 {
                    quad_color.a *= 0.45;
                }
                let rect = Bounds::from_corners(
                    point(px(x0 / scale), px(y0 / scale)),
                    point(px(x1 / scale), px(y1 / scale)),
                );
                window.paint_quad(fill(rect, quad_color));
            }
        }
    }
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

pub(super) fn powerline_background_path(
    ch: char,
    bounds: Bounds<Pixels>,
) -> Option<gpui::Path<Pixels>> {
    let separator = powerline_separator(ch)?;
    if !separator.supports_background_cutout() {
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
        PowerlineShape::SlantedLower => {
            let [flat_top, _, tip_bottom] = powerline_slanted_points(separator, bounds, metrics)?;
            builder.add_polygon(&[flat_top, point(outer_x, flat_top.y), tip_bottom], true);
        }
        PowerlineShape::SlantedUpper => {
            let [_, tip_top, flat_bottom] = powerline_slanted_points(separator, bounds, metrics)?;
            builder.add_polygon(&[tip_top, point(outer_x, flat_bottom.y), flat_bottom], true);
        }
        PowerlineShape::Trapezoid => {
            let [upper, lower] = powerline_trapezoid_polygons(separator.direction, bounds, metrics);
            builder.add_polygon(
                &[upper[1], lower[2], lower[1], lower[0], upper[3], upper[2]],
                true,
            );
        }
        PowerlineShape::InverseTriangle => {
            let [upper, lower] =
                powerline_inverse_triangle_polygons(separator.direction, bounds, metrics);
            builder.add_polygon(&[upper[1], lower[1], upper[2]], true);
        }
        PowerlineShape::PixelatedSmall | PowerlineShape::PixelatedBig => return None,
    }
    builder.build().ok()
}

#[cfg(test)]
mod powerline_background_tests {
    use super::*;

    #[test]
    fn powerline_cutouts_leave_the_glyph_interior_unpainted() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(8.0), px(16.0)));
        let sample_covered = |path: &gpui::Path<Pixels>, x: f32, y: f32| {
            let sample = point(px(10.0 + x * 8.0), px(20.0 + y * 16.0));
            path.vertices.chunks_exact(3).any(|triangle| {
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
            })
        };

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
                assert_eq!(
                    sample_covered(&path, x, y),
                    painted,
                    "{ch}: sample ({x}, {y})"
                );
            }
        }

        for (ch, samples) in [
            ('\u{e0b8}', [(0.2, 0.8, false), (0.8, 0.2, true)]),
            ('\u{e0ba}', [(0.8, 0.8, false), (0.2, 0.2, true)]),
            ('\u{e0bc}', [(0.2, 0.2, false), (0.8, 0.8, true)]),
            ('\u{e0be}', [(0.8, 0.2, false), (0.2, 0.8, true)]),
            ('\u{e0d2}', [(0.2, 0.15, false), (0.85, 0.5, true)]),
            ('\u{e0d4}', [(0.8, 0.15, false), (0.15, 0.5, true)]),
            ('\u{e0d6}', [(0.2, 0.1, false), (0.75, 0.5, true)]),
            ('\u{e0d7}', [(0.8, 0.1, false), (0.25, 0.5, true)]),
        ] {
            let path = powerline_background_path(ch, bounds).expect("extended cutout path");
            for (x, y, painted) in samples {
                assert_eq!(
                    sample_covered(&path, x, y),
                    painted,
                    "U+{:04X}: sample ({x}, {y})",
                    ch as u32
                );
            }
        }

        for ch in ['\u{e0c4}', '\u{e0c5}', '\u{e0c6}', '\u{e0c7}'] {
            assert!(powerline_separator(ch).is_some());
            assert!(powerline_background_path(ch, bounds).is_none());
        }
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
