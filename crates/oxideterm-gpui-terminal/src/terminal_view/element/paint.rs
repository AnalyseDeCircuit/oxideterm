use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, Corners, PathBuilder, Pixels, Point, RenderImage, Rgba, SharedString, TextAlign,
    Window, fill, point, px, rgb, rgba, size,
};
use oxideterm_terminal::{TerminalCursorShape, TerminalImageData};
use unicode_width::UnicodeWidthChar;

use crate::terminal_ui::*;
use crate::terminal_view::element::{
    BatchedTextRun, TerminalCommandMarkOverlay, TerminalCursor, TerminalImageLayout, TerminalRect,
    TerminalScrollbar,
};
use crate::terminal_view::element::{
    PowerlineDirection, PowerlineShape, PowerlineWeight, powerline_separator,
};

const POWERLINE_SEAM_OVERLAP_DEVICE_PIXELS: f32 = 0.5;
const POWERLINE_THIN_STROKE_DEVICE_PIXELS: f32 = 1.4;
const POWERLINE_HALF_CIRCLE_CONTROL_FACTOR: f32 = 4.0 / 3.0;

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
    pub(crate) end: Point<Pixels>,
    pub(crate) start_control: Point<Pixels>,
    pub(crate) end_control: Point<Pixels>,
}

pub(crate) fn paint_terminal_rect(
    rect: &TerminalRect,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    let bounds = Bounds::new(
        origin
            + point(
                px(rect.col as f32 * metrics.cell_width_f32()),
                px(rect.row as f32 * metrics.line_height_f32()),
            ),
        size(
            px(rect.cells as f32 * metrics.cell_width_f32()),
            metrics.line_height,
        ),
    );
    window.paint_quad(fill(bounds, rect.color));
}

pub(crate) fn paint_terminal_underline(
    rect: &TerminalRect,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    let bounds = Bounds::new(
        origin
            + point(
                px(rect.col as f32 * metrics.cell_width_f32()),
                px((rect.row + 1) as f32 * metrics.line_height_f32() - 2.0),
            ),
        size(px(rect.cells as f32 * metrics.cell_width_f32()), px(2.0)),
    );
    window.paint_quad(fill(bounds, rect.color));
}

pub(crate) fn paint_terminal_outline(
    rect: &TerminalRect,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    let x = rect.col as f32 * metrics.cell_width_f32();
    let y = rect.row as f32 * metrics.line_height_f32();
    let width = rect.cells as f32 * metrics.cell_width_f32();
    let height = metrics.line_height_f32();
    for bounds in [
        Bounds::new(origin + point(px(x), px(y)), size(px(width), px(1.0))),
        Bounds::new(
            origin + point(px(x), px(y + height - 1.0)),
            size(px(width), px(1.0)),
        ),
        Bounds::new(origin + point(px(x), px(y)), size(px(1.0), px(height))),
        Bounds::new(
            origin + point(px(x + width - 1.0), px(y)),
            size(px(1.0), px(height)),
        ),
    ] {
        window.paint_quad(fill(bounds, rect.color));
    }
}

pub(crate) fn paint_command_mark_overlay(
    overlay: &TerminalCommandMarkOverlay,
    origin: gpui::Point<Pixels>,
    cols: usize,
    metrics: &TerminalMetrics,
    command_mark_gutter_width: f32,
    window: &mut Window,
) {
    let x = 0.0;
    let y = overlay.start_row as f32 * metrics.line_height_f32();
    let width = cols as f32 * metrics.cell_width_f32();
    let height =
        (overlay.end_row.saturating_sub(overlay.start_row) + 1) as f32 * metrics.line_height_f32();
    let accent = command_mark_accent(overlay);
    let bounds = Bounds::new(origin + point(px(x), px(y)), size(px(width), px(height)));

    if let Some(fill_color) = command_mark_fill(overlay) {
        window.paint_quad(fill(bounds, fill_color));
    }

    if command_mark_gutter_width > 0.0 {
        let edge_width = if overlay.selected { 3.0 } else { 2.0 };
        let edge_x = command_mark_edge_x(command_mark_gutter_width, edge_width);
        window.paint_quad(fill(
            Bounds::new(
                origin + point(px(edge_x), px(y)),
                size(px(edge_width), px(height)),
            ),
            accent,
        ));
    }

    if overlay.selected && overlay.has_top {
        window.paint_quad(fill(
            Bounds::new(origin + point(px(x), px(y)), size(px(width), px(1.0))),
            accent,
        ));
    }
    if overlay.selected && overlay.has_bottom {
        window.paint_quad(fill(
            Bounds::new(
                origin + point(px(x), px((y + height - 1.0).max(y))),
                size(px(width), px(1.0)),
            ),
            accent,
        ));
    }
}

fn command_mark_edge_x(gutter_width: f32, edge_width: f32) -> f32 {
    if gutter_width <= edge_width {
        return 0.0;
    }
    -gutter_width + (gutter_width - edge_width) / 2.0
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
) {
    let bounds = Bounds::new(
        origin
            + point(
                px(image.image.snapshot.col as f32 * metrics.cell_width_f32()),
                px(image.image.snapshot.row as f32 * metrics.line_height_f32()),
            ),
        size(
            px(image.image.snapshot.cols as f32 * metrics.cell_width_f32()),
            px(image.image.snapshot.rows as f32 * metrics.line_height_f32()),
        ),
    );

    let Some(render_image) = &image.image.render_image else {
        window.paint_quad(fill(bounds, rgba(0x528bff29)));
        return;
    };
    let data = image.image.snapshot.data.as_deref();
    let frame_index =
        terminal_image_frame_index(render_image, data, image.image.animation_started_at);
    if terminal_image_should_request_frame(render_image, data, image.image.animation_started_at) {
        window.request_animation_frame();
    }
    let _ = window.paint_image(
        bounds,
        Corners::all(px(0.0)),
        render_image.clone(),
        frame_index,
        false,
    );
}

fn terminal_image_frame_index(
    render_image: &RenderImage,
    data: Option<&TerminalImageData>,
    started_at: Option<Instant>,
) -> usize {
    let frame_count = render_image.frame_count();
    if frame_count <= 1 {
        return 0;
    }

    let Some(data) = data else {
        return 0;
    };
    if !data.animation.running {
        return data.animation.current_frame.min(frame_count - 1);
    }
    let Some(started_at) = started_at else {
        return data.animation.current_frame.min(frame_count - 1);
    };
    let elapsed = started_at.elapsed();
    let mut cycle_duration = Duration::ZERO;
    let frame_delays = (0..frame_count)
        .map(|index| terminal_image_frame_delay(data, render_image, index))
        .inspect(|delay| {
            if let Some(delay) = delay {
                cycle_duration += *delay;
            }
        })
        .collect::<Vec<_>>();
    if cycle_duration.is_zero() {
        return data.animation.current_frame.min(frame_count - 1);
    }

    let elapsed_ms = elapsed.as_millis();
    if data.animation.loading && elapsed_ms >= cycle_duration.as_millis() {
        return last_displayable_frame(&frame_delays).unwrap_or(frame_count - 1);
    }
    if let Some(loop_limit) = data.animation.loop_limit {
        let total_duration = cycle_duration.as_millis() * u128::from(loop_limit);
        if elapsed_ms >= total_duration {
            return last_displayable_frame(&frame_delays).unwrap_or(frame_count - 1);
        }
    }

    let elapsed_in_cycle = elapsed_ms % cycle_duration.as_millis();
    let mut frame_end_ms = 0;
    for (index, delay) in frame_delays.iter().enumerate() {
        let Some(delay) = delay else {
            continue;
        };
        frame_end_ms += delay.as_millis();
        if elapsed_in_cycle < frame_end_ms {
            return index;
        }
    }
    frame_count - 1
}

fn terminal_image_frame_delay(
    data: &TerminalImageData,
    render_image: &RenderImage,
    frame_index: usize,
) -> Option<Duration> {
    if data
        .frames
        .get(frame_index)
        .is_some_and(|frame| frame.gapless)
    {
        return None;
    }
    let delay = Duration::from(render_image.delay(frame_index));
    (!delay.is_zero()).then_some(delay)
}

fn terminal_image_should_request_frame(
    render_image: &RenderImage,
    data: Option<&TerminalImageData>,
    started_at: Option<Instant>,
) -> bool {
    if render_image.frame_count() <= 1 {
        return false;
    }
    let Some(data) = data else {
        return false;
    };
    if !data.animation.running {
        return false;
    }
    if !data.animation.loading && data.animation.loop_limit.is_none() {
        return true;
    }
    let Some(started_at) = started_at else {
        return true;
    };
    let Some(cycle_duration) = terminal_image_cycle_duration(data, render_image) else {
        return false;
    };
    let elapsed_ms = started_at.elapsed().as_millis();
    if data.animation.loading {
        return elapsed_ms < cycle_duration.as_millis();
    }
    data.animation
        .loop_limit
        .is_none_or(|limit| elapsed_ms < cycle_duration.as_millis() * u128::from(limit))
}

fn terminal_image_cycle_duration(
    data: &TerminalImageData,
    render_image: &RenderImage,
) -> Option<Duration> {
    let cycle_duration = (0..render_image.frame_count())
        .filter_map(|index| terminal_image_frame_delay(data, render_image, index))
        .fold(Duration::ZERO, |total, delay| total + delay);
    (!cycle_duration.is_zero()).then_some(cycle_duration)
}

fn last_displayable_frame(frame_delays: &[Option<Duration>]) -> Option<usize> {
    frame_delays
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, delay)| delay.is_some().then_some(index))
}

pub(crate) fn paint_text_run(
    run: &BatchedTextRun,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    if paint_powerline_separators(run, origin, metrics, window) {
        return;
    }

    let position = origin
        + point(
            px(run.col as f32 * metrics.cell_width_f32()),
            px(run.row as f32 * metrics.line_height_f32()),
        );
    if let Some(shaped) = &run.shaped {
        // Stable terminal rows retain their shaped glyph layout with the row-layout cache.
        // Transient runs keep using GPUI's frame cache through the fallback below.
        let shaped = shaped.get_or_init(|| {
            window.text_system().shape_line(
                run.text.clone(),
                metrics.font_size,
                std::slice::from_ref(&run.style),
                Some(metrics.cell_width),
            )
        });
        let _ = shaped.paint_cached(position, metrics.line_height, window, cx);
        return;
    }

    let shaped = window.text_system().shape_line(
        run.text.clone(),
        metrics.font_size,
        std::slice::from_ref(&run.style),
        Some(metrics.cell_width),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TerminalGhostTextSegment {
    pub(crate) col_offset: usize,
    pub(crate) text: String,
    pub(crate) cells: usize,
    pub(crate) cell_stride: usize,
}

pub(crate) fn paint_ghost_text_run(
    run: &BatchedTextRun,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    for segment in ghost_text_grid_segments(&run.text) {
        let position = origin
            + point(
                px((run.col + segment.col_offset) as f32 * metrics.cell_width_f32()),
                px(run.row as f32 * metrics.line_height_f32()),
            );
        let mut style = run.style.clone();
        style.len = segment.text.len();
        let _ = window
            .text_system()
            .shape_line(
                SharedString::from(segment.text),
                metrics.font_size,
                &[style],
                Some(px(segment.cell_stride as f32 * metrics.cell_width_f32())),
            )
            .paint(
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

    for ch in text.chars() {
        let cell_stride = ch.width().unwrap_or(0);
        if cell_stride == 0 {
            current_text.push(ch);
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

        current_text.push(ch);
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
    run: &BatchedTextRun,
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
                    window.paint_path(path, run.style.color);
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
                    window.paint_path(path, run.style.color);
                }
            }
            (PowerlineShape::HalfCircle, PowerlineWeight::Filled) => {
                let Some(curve) = powerline_half_circle_curve(ch, bounds, paint_metrics) else {
                    return false;
                };
                let mut builder = PathBuilder::fill();
                builder.move_to(curve.start);
                builder.line_to(curve.end);
                builder.cubic_bezier_to(curve.start, curve.end_control, curve.start_control);
                builder.close();
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.style.color);
                }
            }
            (PowerlineShape::HalfCircle, PowerlineWeight::Thin) => {
                let Some(curve) = powerline_half_circle_curve(ch, bounds, paint_metrics) else {
                    return false;
                };
                let mut builder = PathBuilder::stroke(paint_metrics.thin_stroke_width);
                builder.move_to(curve.start);
                builder.cubic_bezier_to(curve.end, curve.start_control, curve.end_control);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.style.color);
                }
            }
        }
    }

    true
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
    let control_x =
        extents.flat_x + (extents.tip_x - extents.flat_x) * POWERLINE_HALF_CIRCLE_CONTROL_FACTOR;

    // A single cubic reaches the cell edge at its midpoint only when the controls are 4/3 of
    // the flat-edge-to-tip distance away. Using the tip itself as the controls makes a 3/4-width
    // curve, which is the narrow cap reported in issue #339.
    Some(PowerlineHalfCircleCurve {
        start: point(px(extents.flat_x), px(extents.top)),
        end: point(px(extents.flat_x), px(extents.bottom)),
        start_control: point(px(control_x), px(extents.top)),
        end_control: point(px(control_x), px(extents.bottom)),
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
    match cursor.shape {
        TerminalCursorShape::Block | TerminalCursorShape::Hidden => {}
        TerminalCursorShape::Underline => {
            let bounds = Bounds::new(
                origin
                    + point(
                        px(cursor.col as f32 * cell_width),
                        px((cursor.row + 1) as f32 * line_height - 2.0),
                    ),
                size(metrics.cell_width, px(2.0)),
            );
            window.paint_quad(fill(bounds, rgb(cursor_color)));
        }
        TerminalCursorShape::Bar => {
            let bounds = Bounds::new(
                origin
                    + point(
                        px(cursor.col as f32 * cell_width),
                        px(cursor.row as f32 * line_height),
                    ),
                size(px(2.0), metrics.line_height),
            );
            window.paint_quad(fill(bounds, rgb(cursor_color)));
        }
        TerminalCursorShape::Hollow => {
            let x = cursor.col as f32 * cell_width;
            let y = cursor.row as f32 * line_height;
            let color = rgb(cursor_color);
            for bounds in [
                Bounds::new(
                    origin + point(px(x), px(y)),
                    size(metrics.cell_width, px(1.0)),
                ),
                Bounds::new(
                    origin + point(px(x), px(y + line_height - 1.0)),
                    size(metrics.cell_width, px(1.0)),
                ),
                Bounds::new(
                    origin + point(px(x), px(y)),
                    size(px(1.0), metrics.line_height),
                ),
                Bounds::new(
                    origin + point(px(x + cell_width - 1.0), px(y)),
                    size(px(1.0), metrics.line_height),
                ),
            ] {
                window.paint_quad(fill(bounds, color));
            }
        }
    }
}

pub(crate) fn paint_scrollbar(
    scrollbar: TerminalScrollbar,
    origin: gpui::Point<Pixels>,
    viewport_width: Pixels,
    rows: usize,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    let x = terminal_scrollbar_x_for_viewport(viewport_width);
    let track = Bounds::new(
        origin + point(x, px(0.0)),
        size(
            px(SCROLLBAR_WIDTH),
            px(rows as f32 * metrics.line_height_f32()),
        ),
    );
    window.paint_quad(fill(track, rgba(0xffffff20)));

    let thumb = Bounds::new(
        origin + point(x, px(scrollbar.top)),
        size(px(SCROLLBAR_WIDTH), px(scrollbar.height)),
    );
    window.paint_quad(fill(thumb, rgba(0xffffff66)));
}
