use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, Corners, PathBuilder, Pixels, Point, RenderImage, SharedString, Window, fill,
    point, px, rgb, rgba, size,
};
use oxideterm_terminal::{TerminalCursorShape, TerminalImageData};

use crate::terminal_ui::*;
use crate::terminal_view::element::{
    BatchedTextRun, TerminalCommandMarkOverlay, TerminalCursor, TerminalImageLayout, TerminalRect,
    TerminalScrollbar,
};
use crate::terminal_view::element::{
    PowerlineDirection, PowerlineShape, PowerlineWeight, powerline_separator,
};

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
    window: &mut Window,
) {
    let x = 0.0;
    let y = overlay.start_row as f32 * metrics.line_height_f32();
    let width = cols as f32 * metrics.cell_width_f32();
    let height =
        (overlay.end_row.saturating_sub(overlay.start_row) + 1) as f32 * metrics.line_height_f32();
    let accent = if overlay.stale {
        rgba(0x94a3b8b8)
    } else {
        rgba(0x12cfd0ff)
    };
    let fill_color = if overlay.stale {
        rgba(0x94a3b80a)
    } else {
        rgba(0x12cfd009)
    };
    let bounds = Bounds::new(origin + point(px(x), px(y)), size(px(width), px(height)));
    window.paint_quad(fill(bounds, fill_color));
    window.paint_quad(fill(
        Bounds::new(origin + point(px(x), px(y)), size(px(1.0), px(height))),
        accent,
    ));
    window.paint_quad(fill(
        Bounds::new(
            origin + point(px((width - 1.0).max(0.0)), px(y)),
            size(px(1.0), px(height)),
        ),
        accent,
    ));
    if overlay.has_top {
        window.paint_quad(fill(
            Bounds::new(origin + point(px(x), px(y)), size(px(width), px(1.0))),
            accent,
        ));
    }
    if overlay.has_bottom {
        window.paint_quad(fill(
            Bounds::new(
                origin + point(px(x), px((y + height - 1.0).max(y))),
                size(px(width), px(1.0)),
            ),
            accent,
        ));
    }
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
    let data = image.image.snapshot.data.as_ref();
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
    let _ = window
        .text_system()
        .shape_line(
            SharedString::from(run.text.clone()),
            metrics.font_size,
            std::slice::from_ref(&run.style),
            Some(metrics.cell_width),
        )
        .paint(position, metrics.line_height, window, cx);
}

fn paint_powerline_separators(
    run: &BatchedTextRun,
    origin: gpui::Point<Pixels>,
    metrics: &TerminalMetrics,
    window: &mut Window,
) -> bool {
    let chars = run.text.chars().collect::<Vec<_>>();
    if chars.is_empty()
        || chars.len() != run.cells
        || !chars.iter().all(|ch| powerline_separator(*ch).is_some())
    {
        return false;
    }

    for (offset, ch) in chars.into_iter().enumerate() {
        let bounds = Bounds::new(
            origin
                + point(
                    px((run.col + offset) as f32 * metrics.cell_width_f32()),
                    px(run.row as f32 * metrics.line_height_f32()),
                ),
            size(metrics.cell_width, metrics.line_height),
        );
        let Some(separator) = powerline_separator(ch) else {
            return false;
        };
        match (separator.shape, separator.weight) {
            (PowerlineShape::Triangle, PowerlineWeight::Filled) => {
                let Some(points) = powerline_separator_points(ch, bounds) else {
                    return false;
                };
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&points, true);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.style.color);
                }
            }
            (PowerlineShape::Triangle, PowerlineWeight::Thin) => {
                let Some(points) = powerline_separator_points(ch, bounds) else {
                    return false;
                };
                let mut builder = PathBuilder::stroke(px(1.4));
                builder.move_to(points[0]);
                builder.line_to(points[2]);
                builder.line_to(points[1]);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.style.color);
                }
            }
            (PowerlineShape::HalfCircle, PowerlineWeight::Filled) => {
                let mut builder = PathBuilder::fill();
                add_half_circle_path(&mut builder, bounds, separator.direction);
                builder.close();
                if let Ok(path) = builder.build() {
                    window.paint_path(path, run.style.color);
                }
            }
            (PowerlineShape::HalfCircle, PowerlineWeight::Thin) => {
                let mut builder = PathBuilder::stroke(px(1.4));
                add_half_circle_stroke(&mut builder, bounds, separator.direction);
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
) -> Option<[Point<Pixels>; 3]> {
    let separator = powerline_separator(ch)?;
    if separator.shape != PowerlineShape::Triangle {
        return None;
    }
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);
    let middle_y = top + f32::from(bounds.size.height) / 2.0;
    let overscan = 0.5;

    Some(match separator.direction {
        PowerlineDirection::Right => [
            point(px(left - overscan), px(top - overscan)),
            point(px(left - overscan), px(bottom + overscan)),
            point(px(right + overscan), px(middle_y)),
        ],
        PowerlineDirection::Left => [
            point(px(right + overscan), px(top - overscan)),
            point(px(right + overscan), px(bottom + overscan)),
            point(px(left - overscan), px(middle_y)),
        ],
    })
}

fn add_half_circle_path(
    builder: &mut PathBuilder,
    bounds: Bounds<Pixels>,
    direction: PowerlineDirection,
) {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);
    let overscan = 0.5;

    match direction {
        PowerlineDirection::Right => {
            let top_left = point(px(left - overscan), px(top - overscan));
            let bottom_left = point(px(left - overscan), px(bottom + overscan));
            builder.move_to(top_left);
            builder.line_to(bottom_left);
            builder.cubic_bezier_to(
                top_left,
                point(px(right + overscan), px(bottom + overscan)),
                point(px(right + overscan), px(top - overscan)),
            );
        }
        PowerlineDirection::Left => {
            let top_right = point(px(right + overscan), px(top - overscan));
            let bottom_right = point(px(right + overscan), px(bottom + overscan));
            builder.move_to(top_right);
            builder.line_to(bottom_right);
            builder.cubic_bezier_to(
                top_right,
                point(px(left - overscan), px(bottom + overscan)),
                point(px(left - overscan), px(top - overscan)),
            );
        }
    }
}

fn add_half_circle_stroke(
    builder: &mut PathBuilder,
    bounds: Bounds<Pixels>,
    direction: PowerlineDirection,
) {
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);
    let right = left + f32::from(bounds.size.width);
    let bottom = top + f32::from(bounds.size.height);
    let overscan = 0.5;

    match direction {
        PowerlineDirection::Right => {
            let top_left = point(px(left - overscan), px(top - overscan));
            let bottom_left = point(px(left - overscan), px(bottom + overscan));
            builder.move_to(top_left);
            builder.cubic_bezier_to(
                bottom_left,
                point(px(right + overscan), px(top - overscan)),
                point(px(right + overscan), px(bottom + overscan)),
            );
        }
        PowerlineDirection::Left => {
            let top_right = point(px(right + overscan), px(top - overscan));
            let bottom_right = point(px(right + overscan), px(bottom + overscan));
            builder.move_to(top_right);
            builder.cubic_bezier_to(
                bottom_right,
                point(px(left - overscan), px(top - overscan)),
                point(px(left - overscan), px(bottom + overscan)),
            );
        }
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
