use gpui::{
    App, Bounds, PathBuilder, Pixels, Point, SharedString, Window, fill, point, px, rgb, rgba, size,
};
use oxideterm_terminal::TerminalCursorShape;

use crate::terminal_ui::*;
use crate::terminal_view::element::{
    BatchedTextRun, TerminalCursor, TerminalRect, TerminalScrollbar,
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
            window.paint_quad(fill(bounds, rgb(0x528bff)));
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
            window.paint_quad(fill(bounds, rgb(0x528bff)));
        }
        TerminalCursorShape::Hollow => {
            let x = cursor.col as f32 * cell_width;
            let y = cursor.row as f32 * line_height;
            let color = rgb(0x528bff);
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
    cols: usize,
    rows: usize,
    metrics: &TerminalMetrics,
    window: &mut Window,
) {
    let x = cols as f32 * metrics.cell_width_f32() + SCROLLBAR_GAP;
    let track = Bounds::new(
        origin + point(px(x), px(0.0)),
        size(
            px(SCROLLBAR_WIDTH),
            px(rows as f32 * metrics.line_height_f32()),
        ),
    );
    window.paint_quad(fill(track, rgba(0xffffff20)));

    let thumb = Bounds::new(
        origin + point(px(x), px(scrollbar.top)),
        size(px(SCROLLBAR_WIDTH), px(scrollbar.height)),
    );
    window.paint_quad(fill(thumb, rgba(0xffffff66)));
}
