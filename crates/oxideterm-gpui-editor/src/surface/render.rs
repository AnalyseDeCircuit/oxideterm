// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use gpui::{
    Context, Div, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Render, ScrollWheelEvent, SharedString, Styled, Window, div, px, rgb, rgba,
};
use oxideterm_editor_core::{BufferOffset, Selection};
use oxideterm_editor_syntax::SyntaxScope;

use super::{
    EditorBoundsProbe, TextEditorView, colored_text,
    coords::{
        byte_column_for_visual_column, selection_columns_for_line, visual_column_for_byte_column,
    },
    wrap::DisplayRow,
};

impl Render for TextEditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let display_rows = self.display_rows();
        let visible = self
            .viewport
            .visible_rows(display_rows.len(), self.metrics.line_height);
        let view = cx.entity();

        let mut rows = div()
            .absolute()
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .w_full()
            .top(px(visible.top_spacer_px as f32 - self.viewport.scroll_y_px))
            .h(px(display_rows.len() as f32 * self.metrics.line_height));

        for display_index in visible.range.clone() {
            let Some(row) = display_rows.get(display_index).copied() else {
                continue;
            };
            rows = rows.child(self.render_row(row, cx));
        }

        rows = rows.child(div().h(px(visible.bottom_spacer_px as f32)));

        let body = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                this.handle_scroll(event, cx);
            }))
            .child(EditorBoundsProbe::new(
                rows,
                view.clone(),
                self.focus_handle.clone(),
                move |bounds, window, app| {
                    let _ =
                        view.update(app, |this, cx| this.set_viewport_bounds(bounds, window, cx));
                },
            ));

        div()
            .id("oxideterm-gpui-editor")
            .size_full()
            .track_focus(&self.focus_handle)
            .font_family(SharedString::from(self.appearance.font_family.clone()))
            .text_size(px(self.metrics.font_size))
            .line_height(px(self.metrics.line_height))
            .text_color(rgb(self.appearance.text_hex))
            .bg(rgb(self.appearance.background_hex))
            .border_1()
            .border_color(rgb(self.appearance.border_hex))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, _cx| {
                    window.focus(&this.focus_handle);
                }),
            )
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_key(event, window, cx);
            }))
            .child(body)
    }
}

impl TextEditorView {
    fn render_row(&self, display_row: DisplayRow, cx: &mut Context<Self>) -> Div {
        let line = display_row.line;
        let line_text = self.buffer.line_text(line).unwrap_or_default().to_string();
        let line_start = self
            .buffer
            .line_start_offset(line)
            .unwrap_or(BufferOffset::ZERO)
            .0;
        let line_end = self
            .buffer
            .line_end_offset(line)
            .unwrap_or(BufferOffset::ZERO)
            .0;
        let cursor_position = self
            .buffer
            .offset_to_line_col(self.cursor.selection().head)
            .ok()
            .filter(|position| position.line == line);
        let is_current_line = cursor_position.is_some();
        let cursor_visual_column = cursor_position
            .map(|position| visual_column_for_byte_column(&line_text, position.column))
            .unwrap_or(0);
        let show_cursor = cursor_position.is_some()
            && cursor_visual_column >= display_row.start_col
            && cursor_visual_column <= display_row.end_col;
        let cursor_column = cursor_visual_column.saturating_sub(display_row.start_col);
        let line_height = self.metrics.line_height;
        let gutter_width = self.metrics.gutter_width;
        let content_left =
            gutter_width + self.metrics.content_padding_x - self.viewport.scroll_x_px;
        let row_display = display_row;
        let byte_start = byte_column_for_visual_column(&line_text, display_row.start_col);
        let byte_end = byte_column_for_visual_column(&line_text, display_row.end_col);
        let segment_text = line_text[byte_start..byte_end].to_string();
        let segment_range = (line_start + byte_start)..(line_start + byte_end).min(line_end);
        let selection_rects = self.selection_rects_for_line(&segment_text, segment_range.clone());
        let find_rects = self.find_rects_for_line(&segment_text, segment_range.clone());
        let bracket_rects = self.bracket_rects_for_line(&segment_text, segment_range.clone());
        let marked_text = self.marked_text.as_ref().and_then(|marked| {
            (marked.range.start.0 >= segment_range.start
                && marked.range.start.0 <= segment_range.end)
                .then_some(marked.text.as_str())
        });

        let mut row = div()
            .relative()
            .h(px(line_height))
            .w_full()
            .flex()
            .items_center()
            .bg(if is_current_line {
                rgba((self.appearance.current_line_hex << 8) | 0x99)
            } else {
                rgba((self.appearance.background_hex << 8) | 0x00)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    let raw_column =
                        row_display.start_col + this.visual_column_for_window_x(event.position.x);
                    this.place_cursor_on_line(row_display.line, raw_column, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .h(px(line_height))
                    .w(px(gutter_width))
                    .flex()
                    .items_center()
                    .justify_end()
                    .pr(px(self.metrics.gutter_padding_x))
                    .bg(rgb(self.appearance.gutter_background_hex))
                    .text_color(rgb(self.appearance.muted_text_hex))
                    .child(if display_row.is_first {
                        (line + 1).to_string()
                    } else {
                        String::new()
                    }),
            );

        for (start_col, end_col) in find_rects {
            let left = content_left + start_col as f32 * self.metrics.char_width;
            let width = (end_col.saturating_sub(start_col).max(1) as f32) * self.metrics.char_width;
            row = row.child(
                div()
                    .absolute()
                    .top(px(line_height * 0.16))
                    .left(px(left))
                    .w(px(width))
                    .h(px(line_height * 0.68))
                    .bg(rgba((self.appearance.current_line_hex << 8) | 0xaa)),
            );
        }

        for (start_col, end_col) in selection_rects {
            let left = content_left + start_col as f32 * self.metrics.char_width;
            let width = (end_col.saturating_sub(start_col).max(1) as f32) * self.metrics.char_width;
            row = row.child(
                div()
                    .absolute()
                    .top(px(line_height * 0.12))
                    .left(px(left))
                    .w(px(width))
                    .h(px(line_height * 0.76))
                    .bg(rgba((self.appearance.selection_hex << 8) | 0xcc)),
            );
        }

        for (start_col, end_col) in bracket_rects {
            let left = content_left + start_col as f32 * self.metrics.char_width;
            let width = (end_col.saturating_sub(start_col).max(1) as f32) * self.metrics.char_width;
            row = row.child(
                div()
                    .absolute()
                    .top(px(line_height * 0.12))
                    .left(px(left))
                    .w(px(width))
                    .h(px(line_height * 0.76))
                    .border_1()
                    .border_color(rgb(self.appearance.accent_hex)),
            );
        }

        row.child(
            div()
                .absolute()
                .top_0()
                .left(px(content_left))
                .h(px(line_height))
                .flex()
                .items_center()
                .child(self.render_line_text(
                    &segment_text,
                    segment_range,
                    cursor_column,
                    show_cursor,
                    marked_text,
                )),
        )
    }

    fn render_line_text(
        &self,
        line_text: &str,
        line_range: Range<usize>,
        cursor_column: usize,
        show_cursor: bool,
        marked_text: Option<&str>,
    ) -> Div {
        let byte_column = byte_column_for_visual_column(line_text, cursor_column);
        let mut row = div().flex().items_center();
        let mut cursor_drawn = false;

        for chunk in self.highlighted_line_chunks(line_text, line_range) {
            if show_cursor && !cursor_drawn && byte_column <= chunk.end {
                let split = byte_column.saturating_sub(chunk.start);
                let split = split.min(chunk.text.len());
                let (before, after) = chunk.text.split_at(split);
                row = row.child(colored_text(before, chunk.color));
                row = row.child(self.render_cursor());
                if let Some(marked_text) = marked_text {
                    row = row.child(
                        div()
                            .underline()
                            .text_color(rgb(self.appearance.accent_hex))
                            .child(marked_text.to_string()),
                    );
                }
                row = row.child(colored_text(after, chunk.color));
                cursor_drawn = true;
            } else {
                row = row.child(colored_text(&chunk.text, chunk.color));
            }
        }

        if show_cursor && !cursor_drawn {
            row = row.child(self.render_cursor());
            if let Some(marked_text) = marked_text {
                row = row.child(
                    div()
                        .underline()
                        .text_color(rgb(self.appearance.accent_hex))
                        .child(marked_text.to_string()),
                );
            }
        }
        row
    }

    fn selection_rects_for_line(
        &self,
        line_text: &str,
        line_range: Range<usize>,
    ) -> Vec<(usize, usize)> {
        self.all_selections()
            .into_iter()
            .filter_map(|selection| {
                selection_columns_for_line(selection, line_text, line_range.clone())
            })
            .collect()
    }

    fn find_rects_for_line(
        &self,
        line_text: &str,
        line_range: Range<usize>,
    ) -> Vec<(usize, usize)> {
        self.find_matches
            .iter()
            .filter_map(|hit| {
                selection_columns_for_line(
                    Selection::new(hit.range.start, hit.range.end),
                    line_text,
                    line_range.clone(),
                )
            })
            .collect()
    }

    fn bracket_rects_for_line(
        &self,
        line_text: &str,
        line_range: Range<usize>,
    ) -> Vec<(usize, usize)> {
        let Some(pair) = self.matching_bracket_pair() else {
            return Vec::new();
        };
        [pair.open, pair.close]
            .into_iter()
            .filter_map(|offset| {
                selection_columns_for_line(
                    Selection::new(offset, BufferOffset(offset.0.saturating_add(1))),
                    line_text,
                    line_range.clone(),
                )
            })
            .collect()
    }

    fn render_cursor(&self) -> Div {
        div()
            .w(px(1.0))
            .h(px(self.metrics.line_height * 0.78))
            .bg(rgb(self.appearance.accent_hex))
    }

    fn highlighted_line_chunks(&self, line_text: &str, line_range: Range<usize>) -> Vec<LineChunk> {
        if line_text.is_empty() {
            return vec![LineChunk {
                start: 0,
                end: 1,
                text: " ".to_string(),
                color: self.appearance.text_hex,
            }];
        }

        let mut chunks = Vec::new();
        let mut cursor = 0;
        for span in self.highlight_spans.iter().filter(|span| {
            span.range.start.0 < line_range.end && span.range.end.0 > line_range.start
        }) {
            let start = span.range.start.0.max(line_range.start) - line_range.start;
            let end = span.range.end.0.min(line_range.end) - line_range.start;
            if start > cursor {
                push_chunk(
                    &mut chunks,
                    line_text,
                    cursor,
                    start,
                    self.appearance.text_hex,
                );
            }
            push_chunk(
                &mut chunks,
                line_text,
                start,
                end,
                self.syntax_color(span.scope),
            );
            cursor = cursor.max(end);
        }
        if cursor < line_text.len() {
            push_chunk(
                &mut chunks,
                line_text,
                cursor,
                line_text.len(),
                self.appearance.text_hex,
            );
        }
        chunks
    }

    fn syntax_color(&self, scope: SyntaxScope) -> u32 {
        match scope {
            SyntaxScope::Attribute => self.appearance.syntax_attribute_hex,
            SyntaxScope::Comment => self.appearance.syntax_comment_hex,
            SyntaxScope::Constant => self.appearance.syntax_constant_hex,
            SyntaxScope::Function => self.appearance.syntax_function_hex,
            SyntaxScope::Keyword => self.appearance.syntax_keyword_hex,
            SyntaxScope::Namespace | SyntaxScope::Type => self.appearance.syntax_type_hex,
            SyntaxScope::Number => self.appearance.syntax_number_hex,
            SyntaxScope::Operator | SyntaxScope::Punctuation => self.appearance.muted_text_hex,
            SyntaxScope::Property | SyntaxScope::Variable => self.appearance.syntax_variable_hex,
            SyntaxScope::String => self.appearance.syntax_string_hex,
        }
    }
}

struct LineChunk {
    start: usize,
    end: usize,
    text: String,
    color: u32,
}

fn push_chunk(chunks: &mut Vec<LineChunk>, line_text: &str, start: usize, end: usize, color: u32) {
    if start >= end || start >= line_text.len() {
        return;
    }
    let end = end.min(line_text.len());
    if !line_text.is_char_boundary(start) || !line_text.is_char_boundary(end) {
        return;
    }
    chunks.push(LineChunk {
        start,
        end,
        text: line_text[start..end].to_string(),
        color,
    });
}
