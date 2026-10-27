use gpui::{
    AnyElement, App, Bounds, CursorStyle, DispatchPhase, HitboxBehavior, Hsla, IntoElement,
    MouseButton, Pixels, Styled, TextAlign, TextLayout, TextRun, Window, canvas, point, size,
};
use std::ops::Range;

pub fn selectable_text_hit_target(
    layout: TextLayout,
    on_mouse_down: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    canvas(
        move |_bounds, window, _cx| {
            let layout_bounds = layout.bounds();
            let line_height = layout.line_height();
            let text_align = window.text_style().text_align;
            let mut line_y = layout_bounds.origin.y;
            let mut hitboxes = Vec::new();

            // TextLayout::bounds can be stretched by its flex parent. Build one
            // hitbox per visual line from shaped glyph advances instead, matching
            // the geometry used by GPUI's text painter.
            for line in layout.line_layouts() {
                let unwrapped = &line.unwrapped_layout;
                let mut line_start_x = Pixels::ZERO;
                let line_end_positions = line
                    .wrap_boundaries()
                    .iter()
                    .map(|boundary| {
                        unwrapped.runs[boundary.run_ix].glyphs[boundary.glyph_ix]
                            .position
                            .x
                    })
                    .chain(std::iter::once(unwrapped.width));

                for line_end_x in line_end_positions {
                    let visual_line_width = (line_end_x - line_start_x).max(Pixels::ZERO);
                    let aligned_x = match text_align {
                        TextAlign::Left => layout_bounds.origin.x,
                        TextAlign::Center => {
                            layout_bounds.origin.x
                                + (layout_bounds.size.width - visual_line_width) / 2.0
                        }
                        TextAlign::Right => layout_bounds.right() - visual_line_width,
                    };
                    if visual_line_width > Pixels::ZERO && line_height > Pixels::ZERO {
                        hitboxes.push(window.insert_hitbox(
                            Bounds::new(
                                point(aligned_x, line_y),
                                size(visual_line_width, line_height),
                            ),
                            HitboxBehavior::Normal,
                        ));
                    }
                    line_y += line_height;
                    line_start_x = line_end_x;
                }
            }
            hitboxes
        },
        move |_bounds, hitboxes, window, _cx| {
            for hitbox in &hitboxes {
                window.set_cursor_style(CursorStyle::IBeam, hitbox);
            }
            window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.button == MouseButton::Left
                    && hitboxes.iter().any(|hitbox| hitbox.is_hovered(window))
                {
                    on_mouse_down(event, window, cx);
                }
            });
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

pub fn selected_text_runs(
    runs: &[TextRun],
    selection_range: Range<usize>,
    selection_bg: Hsla,
) -> Vec<TextRun> {
    let selection_start = selection_range.start;
    let selection_end = selection_range.end;
    if selection_start >= selection_end {
        return runs.to_vec();
    }

    let mut split_runs = Vec::with_capacity(runs.len() + 2);
    let mut cursor = 0usize;
    for run in runs {
        let run_start = cursor;
        let run_end = cursor.saturating_add(run.len);
        cursor = run_end;
        if run.len == 0 {
            continue;
        }
        let cuts = [
            run_start,
            selection_start.clamp(run_start, run_end),
            selection_end.clamp(run_start, run_end),
            run_end,
        ];
        for pair in cuts.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start >= end {
                continue;
            }
            let mut part = run.clone();
            part.len = end - start;
            if start >= selection_start && end <= selection_end {
                part.background_color = Some(selection_bg);
            }
            split_runs.push(part);
        }
    }
    split_runs
}
