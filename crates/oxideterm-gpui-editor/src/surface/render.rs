// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use gpui::{
    Context, Div, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Render, ScrollWheelEvent, SharedString, Styled, Window, div, px,
    rgb, rgba,
};
use oxideterm_editor_core::{BufferOffset, Selection};
use oxideterm_editor_syntax::SyntaxScope;

use super::{
    EditorBoundsProbe, HighlightChunkCacheKey, LineChunkSpec, TextEditorView, colored_text,
    coords::{
        byte_column_for_visual_column, selection_columns_for_line, visual_column_for_byte_column,
    },
    wrap::DisplayRow,
};

// Tauri `useCodeMirrorEditor.ts` paints these with color-mix against
// `--theme-accent`: active line 7%, selection 20/25%, search match 25% with a
// 50% outline, and focused cursor width 2px.
const CM_ACTIVE_LINE_ACCENT_ALPHA: u32 = 0x12;
const CM_ACTIVE_GUTTER_ACCENT_ALPHA: u32 = 0xcc;
const CM_SELECTION_ACCENT_ALPHA: u32 = 0x40;
const CM_SEARCH_MATCH_ACCENT_ALPHA: u32 = 0x40;
const CM_SEARCH_MATCH_OUTLINE_ALPHA: u32 = 0x80;
const CM_INDENT_GUIDE_ALPHA: u32 = 0x26;
const CM_SPECIAL_CHAR_ALPHA: u32 = 0xcc;
const CM_PLACEHOLDER_ALPHA: u32 = 0x80;
const CM_FOLD_MARKER_ALPHA: u32 = 0xb3;
const CM_FOLD_TOKEN_ALPHA: u32 = 0x99;
const CM_SELECTION_RADIUS: f32 = 2.0;
const CM_CURSOR_WIDTH: f32 = 2.0;
const CM_INDENT_GUIDE_WIDTH: f32 = 1.0;
const CM_FOLD_ICON_WIDTH: f32 = 14.0;
const CM_TAB_MARKER: &str = "→";
const CM_CONTROL_CHAR_MARKER: &str = "�";
const CM_FOLDED_TOKEN: &str = " …";
const CM_FOLD_OPEN_ICON: &str = "▾";
const CM_FOLD_CLOSED_ICON: &str = "▸";

impl Render for TextEditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.measure_code_metrics(window, cx);
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

        let mut body_content = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                this.handle_scroll(event, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                this.drag_selection_to_point(event.position, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.finish_selection_drag(cx);
                }),
            )
            .child(rows);
        if let Some(placeholder) = self.render_placeholder() {
            body_content = body_content.child(placeholder);
        }

        // Capture the viewport container, not the absolute row stack. Otherwise
        // long files report their full document height as the visible height and
        // GPUI/Monaco-style virtual scrolling clamps to zero.
        let body = EditorBoundsProbe::new(
            body_content,
            view.clone(),
            self.focus_handle.clone(),
            move |bounds, window, app| {
                let _ = view.update(app, |this, cx| this.set_viewport_bounds(bounds, window, cx));
            },
        );

        div()
            .id("oxideterm-gpui-editor")
            .size_full()
            .track_focus(&self.focus_handle)
            .font_family(SharedString::from(self.appearance.font_family.clone()))
            .text_size(px(self.metrics.font_size))
            .line_height(px(self.metrics.line_height))
            .text_color(rgb(self.appearance.text_hex))
            .bg(self.editor_background(self.appearance.background_hex))
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
        self.buffer
            .with_line_text(display_row.line, |line_text| {
                self.render_row_with_text(display_row, line_text, cx)
            })
            .unwrap_or_else(|| div().h(px(self.metrics.line_height)).w_full())
    }

    fn render_row_with_text(
        &self,
        display_row: DisplayRow,
        line_text: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let line = display_row.line;
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
        let segment_text = &line_text[byte_start..byte_end];
        let segment_range = (line_start + byte_start)..(line_start + byte_end).min(line_end);
        let selection_rects = self.selection_rects_for_line(&segment_text, segment_range.clone());
        let find_rects = self.find_rects_for_line(line, &segment_text, segment_range.clone());
        let bracket_rects = self.bracket_rects_for_line(&segment_text, segment_range.clone());
        let indent_guides = self.indentation_marker_columns(&line_text, display_row);
        let foldable = display_row
            .is_first
            .then(|| self.foldable_range_starting_at(line))
            .flatten();
        let folded = display_row.is_folded_header;
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
                rgba((self.appearance.accent_hex << 8) | CM_ACTIVE_LINE_ACCENT_ALPHA)
            } else {
                rgba((self.appearance.background_hex << 8) | 0x00)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    if let Some(offset) = this.offset_for_window_point(event.position) {
                        if (event.modifiers.secondary() || event.modifiers.control)
                            && !event.modifiers.alt
                            && !event.modifiers.shift
                            && this.modified_word_click(offset, window, cx)
                        {
                            cx.stop_propagation();
                            return;
                        }
                        if event.modifiers.alt {
                            this.add_cursor_at(offset, cx);
                        } else {
                            let anchor = if event.modifiers.shift {
                                this.cursor.selection().anchor
                            } else {
                                offset
                            };
                            this.start_selection_drag(anchor, offset, cx);
                        }
                    } else {
                        let raw_column = row_display.start_col
                            + this.visual_column_for_window_x(event.position.x);
                        this.place_cursor_on_line(row_display.line, raw_column, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .child(self.render_gutter(
                display_row,
                line_height,
                gutter_width,
                is_current_line,
                foldable,
                folded,
                cx,
            ));

        for column in indent_guides {
            let left = content_left + column as f32 * self.metrics.char_width;
            row = row.child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(left))
                    .w(px(CM_INDENT_GUIDE_WIDTH))
                    .h(px(line_height))
                    .bg(rgba(
                        (self.appearance.muted_text_hex << 8) | CM_INDENT_GUIDE_ALPHA,
                    )),
            );
        }

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
                    .rounded(px(CM_SELECTION_RADIUS))
                    .bg(rgba(
                        (self.appearance.accent_hex << 8) | CM_SEARCH_MATCH_ACCENT_ALPHA,
                    ))
                    .border_1()
                    .border_color(rgba(
                        (self.appearance.accent_hex << 8) | CM_SEARCH_MATCH_OUTLINE_ALPHA,
                    )),
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
                    .rounded(px(CM_SELECTION_RADIUS))
                    .bg(rgba(
                        (self.appearance.accent_hex << 8) | CM_SELECTION_ACCENT_ALPHA,
                    )),
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
                    .rounded(px(CM_SELECTION_RADIUS))
                    .border_1()
                    .border_color(rgb(self.appearance.accent_hex)),
            );
        }

        if show_cursor {
            let left = content_left + cursor_column as f32 * self.metrics.char_width;
            row = row.child(self.render_cursor_at(left));
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
                    line,
                    &segment_text,
                    segment_range,
                    cursor_column,
                    show_cursor,
                    marked_text,
                    folded,
                )),
        )
    }

    fn render_gutter(
        &self,
        display_row: DisplayRow,
        line_height: f32,
        gutter_width: f32,
        is_current_line: bool,
        foldable: Option<super::FoldRange>,
        folded: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let line = display_row.line;
        let text_hex = if is_current_line && display_row.is_first {
            self.appearance.background_hex
        } else {
            self.appearance.muted_text_hex
        };
        let mut fold_icon = div()
            .w(px(CM_FOLD_ICON_WIDTH))
            .h(px(line_height))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgba((text_hex << 8) | CM_FOLD_MARKER_ALPHA));
        if foldable.is_some() {
            fold_icon = fold_icon
                .cursor_pointer()
                .child(if folded {
                    CM_FOLD_CLOSED_ICON
                } else {
                    CM_FOLD_OPEN_ICON
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                        this.toggle_fold_at_line(line, cx);
                        cx.stop_propagation();
                    }),
                );
        }

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
            .bg(if is_current_line && display_row.is_first {
                rgba((self.appearance.accent_hex << 8) | CM_ACTIVE_GUTTER_ACCENT_ALPHA)
            } else {
                self.editor_panel_background(self.appearance.gutter_background_hex)
            })
            .text_color(rgb(text_hex))
            .child(fold_icon)
            .child(if display_row.is_first {
                (line + 1).to_string()
            } else {
                String::new()
            })
    }

    fn render_line_text(
        &self,
        line: usize,
        line_text: &str,
        line_range: Range<usize>,
        cursor_column: usize,
        show_cursor: bool,
        marked_text: Option<&str>,
        folded: bool,
    ) -> Div {
        let byte_column = byte_column_for_visual_column(line_text, cursor_column);
        let mut row = div().flex().items_center();
        let mut cursor_drawn = false;

        if line_text.is_empty() {
            if show_cursor {
                if let Some(marked_text) = marked_text {
                    row = row.child(
                        div()
                            .underline()
                            .text_color(rgb(self.appearance.accent_hex))
                            .child(marked_text.to_string()),
                    );
                }
            }
            row = self.append_rendered_text(row, " ", self.appearance.text_hex);
            return self.append_fold_token(row, folded);
        }

        for chunk in self
            .highlighted_line_chunks(line, line_text, line_range)
            .iter()
        {
            let Some(chunk_text) = line_text.get(chunk.start..chunk.end) else {
                continue;
            };
            if show_cursor && marked_text.is_some() && !cursor_drawn && byte_column <= chunk.end {
                let split = byte_column.saturating_sub(chunk.start);
                let split = split.min(chunk_text.len());
                let (before, after) = chunk_text.split_at(split);
                row = self.append_rendered_text(row, before, chunk.color);
                if let Some(marked_text) = marked_text {
                    row = row.child(
                        div()
                            .underline()
                            .text_color(rgb(self.appearance.accent_hex))
                            .child(marked_text.to_string()),
                    );
                }
                row = self.append_rendered_text(row, after, chunk.color);
                cursor_drawn = true;
            } else {
                row = self.append_rendered_text(row, chunk_text, chunk.color);
            }
        }

        if show_cursor && marked_text.is_some() && !cursor_drawn {
            if let Some(marked_text) = marked_text {
                row = row.child(
                    div()
                        .underline()
                        .text_color(rgb(self.appearance.accent_hex))
                        .child(marked_text.to_string()),
                );
            }
        }
        self.append_fold_token(row, folded)
    }

    fn append_rendered_text(&self, mut row: Div, text: &str, color: u32) -> Div {
        if !self.settings.highlight_special_chars || !contains_special_char(text) {
            return row.child(colored_text(text, color));
        }

        let mut plain = String::new();
        for ch in text.chars() {
            if let Some(marker) = special_char_marker(ch) {
                if !plain.is_empty() {
                    row = row.child(colored_text(&plain, color));
                    plain.clear();
                }
                row = row.child(
                    div()
                        .text_color(rgba(
                            (self.appearance.muted_text_hex << 8) | CM_SPECIAL_CHAR_ALPHA,
                        ))
                        .child(marker.to_string()),
                );
            } else {
                plain.push(ch);
            }
        }
        if !plain.is_empty() {
            row = row.child(colored_text(&plain, color));
        }
        row
    }

    fn append_fold_token(&self, row: Div, folded: bool) -> Div {
        if !folded {
            return row;
        }
        row.child(
            div()
                .text_color(rgba(
                    (self.appearance.muted_text_hex << 8) | CM_FOLD_TOKEN_ALPHA,
                ))
                .child(CM_FOLDED_TOKEN.to_string()),
        )
    }

    fn indentation_marker_columns(&self, line_text: &str, display_row: DisplayRow) -> Vec<usize> {
        if !self.settings.indentation_markers {
            return Vec::new();
        }
        indentation_marker_columns(line_text, self.settings.tab_size, display_row.start_col)
            .into_iter()
            .filter(|column| *column >= display_row.start_col && *column < display_row.end_col)
            .map(|column| column - display_row.start_col)
            .collect()
    }

    fn render_placeholder(&self) -> Option<Div> {
        let placeholder = self.settings.placeholder.as_deref()?;
        if !self.buffer.is_empty() || placeholder.is_empty() {
            return None;
        }
        Some(
            div()
                .absolute()
                .top_0()
                .left(px(self.metrics.gutter_width
                    + self.metrics.content_padding_x
                    - self.viewport.scroll_x_px))
                .h(px(self.metrics.line_height))
                .flex()
                .items_center()
                .text_color(rgba(
                    (self.appearance.muted_text_hex << 8) | CM_PLACEHOLDER_ALPHA,
                ))
                .child(placeholder.to_string()),
        )
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
        line: usize,
        line_text: &str,
        line_range: Range<usize>,
    ) -> Vec<(usize, usize)> {
        let match_range = self.find_line_matches.get(line).cloned().unwrap_or(0..0);
        self.find_matches
            .get(match_range)
            .unwrap_or(&[])
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

    fn render_cursor_at(&self, left: f32) -> Div {
        // Match CodeMirror/browser editor semantics: the caret is painted over
        // the line box and must never reserve horizontal layout space.
        div()
            .absolute()
            .top(px(self.metrics.line_height * 0.11))
            .left(px(left))
            .w(px(CM_CURSOR_WIDTH))
            .h(px(self.metrics.line_height * 0.78))
            .bg(rgb(self.appearance.accent_hex))
    }

    fn editor_background(&self, color: u32) -> gpui::Rgba {
        if self.transparent_background {
            rgba((color << 8) | 0x00)
        } else {
            rgb(color)
        }
    }

    fn editor_panel_background(&self, color: u32) -> gpui::Rgba {
        if self.transparent_background {
            // Tauri `[data-bg-active]` leaves CodeMirror's main scroller
            // transparent and keeps chrome at theme-bg-panel/40.
            rgba((color << 8) | 0x66)
        } else {
            rgb(color)
        }
    }

    fn highlighted_line_chunks(
        &self,
        line: usize,
        line_text: &str,
        line_range: Range<usize>,
    ) -> std::sync::Arc<Vec<LineChunkSpec>> {
        let key = HighlightChunkCacheKey {
            buffer_version: self.buffer.version(),
            line,
            range_start: line_range.start,
            range_end: line_range.end,
        };
        if let Some(chunks) = self.highlight_chunk_cache.borrow().get(&key) {
            return chunks;
        }

        let chunks =
            std::sync::Arc::new(self.build_highlighted_line_chunks(line, line_text, line_range));
        self.highlight_chunk_cache.borrow_mut().insert(key, chunks)
    }

    fn build_highlighted_line_chunks(
        &self,
        line: usize,
        line_text: &str,
        line_range: Range<usize>,
    ) -> Vec<LineChunkSpec> {
        let mut chunks = Vec::new();
        let mut cursor = 0;
        let span_range = self
            .highlight_line_spans
            .get(line)
            .cloned()
            .unwrap_or(0..self.highlight_spans.len());
        for span in self.highlight_spans[span_range].iter().filter(|span| {
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

fn push_chunk(
    chunks: &mut Vec<LineChunkSpec>,
    line_text: &str,
    start: usize,
    end: usize,
    color: u32,
) {
    if start >= end || start >= line_text.len() {
        return;
    }
    let end = end.min(line_text.len());
    if !line_text.is_char_boundary(start) || !line_text.is_char_boundary(end) {
        return;
    }
    chunks.push(LineChunkSpec { start, end, color });
}

fn contains_special_char(text: &str) -> bool {
    text.chars().any(|ch| special_char_marker(ch).is_some())
}

fn special_char_marker(ch: char) -> Option<&'static str> {
    match ch {
        '\t' => Some(CM_TAB_MARKER),
        ch if ch.is_control() => Some(CM_CONTROL_CHAR_MARKER),
        _ => None,
    }
}

fn indentation_marker_columns(
    line_text: &str,
    tab_size: usize,
    first_visible_column: usize,
) -> Vec<usize> {
    let tab_size = tab_size.max(1);
    let mut visual_column = 0usize;
    let mut columns = Vec::new();
    for ch in line_text.chars() {
        match ch {
            ' ' => visual_column += 1,
            '\t' => visual_column += tab_size,
            _ => break,
        }
        if visual_column >= tab_size
            && visual_column % tab_size == 0
            && visual_column > first_visible_column
        {
            // CodeMirror's indentation markers are paint-only guides; the text
            // buffer and byte-offset mapping remain unchanged.
            columns.push(visual_column);
        }
    }
    columns
}

#[cfg(test)]
mod tests {
    use super::{contains_special_char, indentation_marker_columns, special_char_marker};

    #[test]
    fn special_char_markers_cover_tabs_and_controls_only() {
        assert!(contains_special_char("\t"));
        assert!(contains_special_char("\u{0007}"));
        assert_eq!(special_char_marker('\t'), Some("→"));
        assert_eq!(special_char_marker(' '), None);
        assert_eq!(special_char_marker('a'), None);
    }

    #[test]
    fn indentation_guides_follow_configured_tab_size() {
        assert_eq!(indentation_marker_columns("        code", 4, 0), vec![4, 8]);
        assert_eq!(indentation_marker_columns("\t  code", 4, 0), vec![4]);
        assert_eq!(indentation_marker_columns("  code", 2, 0), vec![2]);
    }

    #[test]
    fn indentation_guides_skip_columns_before_wrapped_segment() {
        assert_eq!(
            indentation_marker_columns("            code", 4, 4),
            vec![8, 12]
        );
    }
}
