// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, ops::Range};

use gpui::{
    Anchor, AnchoredPositionMode, App, Context, Div, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Render, ScrollWheelEvent,
    SharedString, Styled, Window, anchored, deferred, div, prelude::FluentBuilder, px, rgb, rgba,
};
use oxideterm_editor_core::{BufferOffset, Selection};
use oxideterm_editor_syntax::{BracketPair, SyntaxScope};

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
const CM_CONTEXT_MENU_WIDTH: f32 = 160.0;
const CM_CONTEXT_MENU_ITEM_HEIGHT: f32 = 28.0;
const CM_CONTEXT_MENU_PADDING_Y: f32 = 4.0;
const CM_CONTEXT_MENU_Z: usize = 60;
const CM_CONTROL_CHAR_MARKER: &str = "�";
const CM_FOLDED_TOKEN: &str = " …";
const CM_FOLD_OPEN_ICON: &str = "▾";
const CM_FOLD_CLOSED_ICON: &str = "▸";

struct RenderRowContext {
    selections: Vec<Selection>,
    matching_bracket_pair: Option<BracketPair>,
    indentation_columns_by_line: BTreeMap<usize, Vec<usize>>,
    primary_caret_display_index: Option<usize>,
}

impl Render for TextEditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.measure_code_metrics(window, cx);
        let display_rows = self.display_rows();
        let visible = self
            .viewport
            .visible_rows(display_rows.len(), self.metrics.line_height);
        let row_context =
            self.prepare_render_row_context(&display_rows, &display_rows[visible.range.clone()]);
        let view = cx.entity();

        let mut rows = div()
            .absolute()
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .w_full()
            .top(px(
                visible.top_spacer_px as f32 - self.vertical_scroll_y_px()
            ))
            .h(px(display_rows.len() as f32 * self.metrics.line_height));
        for display_index in visible.range.clone() {
            let Some(row) = display_rows.get(display_index).copied() else {
                continue;
            };
            rows = rows.child(self.render_row(display_index, row, &row_context, cx));
        }
        rows = rows.child(div().h(px(visible.bottom_spacer_px as f32)));

        let mut body_content = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.open_context_menu(event.position, cx);
                    cx.stop_propagation();
                }),
            )
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

        let mut root = div()
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
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    if this.context_menu.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_key(event, window, cx);
            }))
            .child(body);
        if let Some(menu) = self.context_menu {
            root = root.child(self.render_context_menu(menu, window, cx));
        }
        root
    }
}

impl TextEditorView {
    fn render_row(
        &self,
        display_index: usize,
        display_row: DisplayRow,
        row_context: &RenderRowContext,
        cx: &mut Context<Self>,
    ) -> Div {
        self.buffer
            .with_line_text(display_row.line, |line_text| {
                self.render_row_with_text(display_index, display_row, line_text, row_context, cx)
            })
            .unwrap_or_else(|| div().h(px(self.metrics.line_height)).w_full())
    }

    fn render_row_with_text(
        &self,
        display_index: usize,
        display_row: DisplayRow,
        line_text: &str,
        row_context: &RenderRowContext,
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
        let show_cursor = row_context.primary_caret_display_index == Some(display_index);
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
        let selection_rects = if row_context.selections.is_empty() {
            Vec::new()
        } else {
            self.selection_rects_for_line(
                &row_context.selections,
                &segment_text,
                segment_range.clone(),
            )
        };
        let find_rects = if self.find_matches.is_empty() {
            Vec::new()
        } else {
            self.find_rects_for_line(line, &segment_text, segment_range.clone())
        };
        let bracket_rects = row_context
            .matching_bracket_pair
            .as_ref()
            .map(|pair| self.bracket_rects_for_line(pair, &segment_text, segment_range.clone()))
            .unwrap_or_default();
        let indent_guides =
            self.indentation_marker_columns(display_row, &row_context.indentation_columns_by_line);
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
                    this.context_menu = None;
                    window.focus(&this.focus_handle, cx);
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
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    // Match browser editor behavior: opening the context menu
                    // over a selection must not collapse that selection first.
                    this.open_context_menu(event.position, cx);
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

    fn open_context_menu(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        self.context_menu = Some(super::EditorContextMenu {
            x: f32::from(position.x),
            y: f32::from(position.y),
        });
        cx.notify();
    }

    fn render_context_menu(
        &self,
        menu: super::EditorContextMenu,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let viewport = window.viewport_size();
        let x = menu
            .x
            .min(f32::from(viewport.width) - CM_CONTEXT_MENU_WIDTH - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(
                f32::from(viewport.height)
                    - CM_CONTEXT_MENU_ITEM_HEIGHT * 4.0
                    - CM_CONTEXT_MENU_PADDING_Y * 2.0
                    - 8.0,
            )
            .max(8.0);
        let has_selection = self.has_primary_or_secondary_selection();
        let can_edit = !self.read_only;
        let popup = div()
            .w(px(CM_CONTEXT_MENU_WIDTH))
            .py(px(CM_CONTEXT_MENU_PADDING_Y))
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(self.appearance.border_hex))
            .bg(rgb(self.appearance.gutter_background_hex))
            .shadow_lg()
            .child(self.render_context_menu_item(
                self.context_menu_labels.copy.clone(),
                has_selection,
                cx.listener(|this, _event, _window, cx| {
                    this.copy_selection_to_clipboard(cx);
                    this.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_context_menu_item(
                self.context_menu_labels.cut.clone(),
                has_selection && can_edit,
                cx.listener(|this, _event, _window, cx| {
                    this.cut_selection_to_clipboard(cx);
                    this.context_menu = None;
                    cx.stop_propagation();
                }),
            ))
            .child(self.render_context_menu_item(
                self.context_menu_labels.paste.clone(),
                can_edit,
                cx.listener(|this, _event, _window, cx| {
                    this.paste_from_clipboard(cx);
                    this.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_context_menu_item(
                self.context_menu_labels.select_all.clone(),
                !self.buffer.is_empty(),
                cx.listener(|this, _event, _window, cx| {
                    this.select_all(cx);
                    this.context_menu = None;
                    cx.stop_propagation();
                }),
            ))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .into_any_element();

        div()
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, _window, cx| {
                    this.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                deferred(
                    anchored()
                        .anchor(Anchor::TopLeft)
                        .position(gpui::point(px(x), px(y)))
                        .position_mode(AnchoredPositionMode::Window)
                        .child(popup),
                )
                .with_priority(CM_CONTEXT_MENU_Z),
            )
    }

    fn render_context_menu_item(
        &self,
        label: String,
        enabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        div()
            .h(px(CM_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .px_3()
            .text_size(px(12.0))
            .text_color(rgb(self.appearance.text_hex))
            .opacity(if enabled { 1.0 } else { 0.45 })
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(rgb(self.appearance.selection_hex)))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .child(div().truncate().child(label))
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
                row = self.append_rendered_chunk(row, chunk);
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

    fn append_rendered_chunk(&self, row: Div, chunk: &LineChunkSpec) -> Div {
        if !self.settings.highlight_special_chars || !contains_special_char(&chunk.text) {
            // Highlight chunks are cached across scroll frames, so retain the
            // shared text allocation instead of rebuilding a String per row.
            return row.child(colored_shared_text(chunk.text.clone(), chunk.color));
        }
        self.append_rendered_text(row, &chunk.text, chunk.color)
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

    fn prepare_render_row_context(
        &self,
        display_rows: &[DisplayRow],
        visible_rows: &[DisplayRow],
    ) -> RenderRowContext {
        let primary_caret_display_index = self
            .buffer
            .offset_to_line_col(self.cursor.selection().head)
            .ok()
            .and_then(|position| {
                let line_text = self.buffer.line_text(position.line).unwrap_or_default();
                let visual_column = visual_column_for_byte_column(&line_text, position.column);
                super::wrap::display_row_for_visual_column(
                    display_rows,
                    position.line,
                    visual_column,
                )
                .map(|(index, _, _)| index)
            });
        RenderRowContext {
            // Selection ordering and bracket matching depend on editor state,
            // not on the row, so compute them once for the current frame.
            selections: self.active_selections(),
            matching_bracket_pair: self.matching_bracket_pair(),
            indentation_columns_by_line: visible_indentation_columns(
                &self.indent_guide_index,
                visible_rows,
                self.settings.indentation_markers,
            ),
            primary_caret_display_index,
        }
    }

    fn indentation_marker_columns(
        &self,
        display_row: DisplayRow,
        columns_by_line: &BTreeMap<usize, Vec<usize>>,
    ) -> Vec<usize> {
        if !self.settings.indentation_markers {
            return Vec::new();
        }
        let Some(columns) = columns_by_line.get(&display_row.line) else {
            return Vec::new();
        };
        columns
            .iter()
            .copied()
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
        selections: &[Selection],
        line_text: &str,
        line_range: Range<usize>,
    ) -> Vec<(usize, usize)> {
        if selections.is_empty() {
            return Vec::new();
        }
        selections
            .iter()
            .copied()
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
        if match_range.is_empty() {
            return Vec::new();
        }
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
        pair: &BracketPair,
        line_text: &str,
        line_range: Range<usize>,
    ) -> Vec<(usize, usize)> {
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
            let Some(highlight_range) = visible_highlight_range(start, end, cursor) else {
                continue;
            };
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
                highlight_range.start,
                highlight_range.end,
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

fn visible_highlight_range(
    start: usize,
    end: usize,
    already_rendered_until: usize,
) -> Option<Range<usize>> {
    // Tree-sitter captures can overlap parent and child nodes. The renderer is
    // linear, so every later span must be clipped to text that is not already
    // emitted for this visual line segment.
    let start = start.max(already_rendered_until);
    (start < end).then_some(start..end)
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
    chunks.push(LineChunkSpec {
        start,
        end,
        color,
        text: SharedString::from(line_text[start..end].to_string()),
    });
}

fn colored_shared_text(text: SharedString, color: u32) -> Div {
    div().text_color(rgb(color)).child(text)
}

fn contains_special_char(text: &str) -> bool {
    text.chars().any(|ch| special_char_marker(ch).is_some())
}

fn special_char_marker(ch: char) -> Option<&'static str> {
    match ch {
        '\t' => None,
        ch if ch.is_control() => Some(CM_CONTROL_CHAR_MARKER),
        _ => None,
    }
}

fn visible_indentation_columns(
    index: &super::indent_index::IndentGuideIndex,
    visible_rows: &[DisplayRow],
    enabled: bool,
) -> BTreeMap<usize, Vec<usize>> {
    if !enabled {
        return BTreeMap::new();
    }

    let mut columns_by_line = BTreeMap::new();
    let mut previous_line = None;
    for row in visible_rows {
        if previous_line == Some(row.line) {
            continue;
        }
        previous_line = Some(row.line);
        let columns = index.columns_for_line(row.line);
        if !columns.is_empty() {
            columns_by_line.insert(row.line, columns);
        }
    }
    columns_by_line
}

#[cfg(test)]
mod tests {
    use super::{
        contains_special_char, special_char_marker, visible_highlight_range,
        visible_indentation_columns,
    };
    use oxideterm_editor_syntax::IndentGuide;

    use crate::surface::{indent_index::IndentGuideIndex, wrap::DisplayRow};

    fn display_row(line: usize, start_col: usize, end_col: usize) -> DisplayRow {
        DisplayRow {
            line,
            start_col,
            end_col,
            is_first: start_col == 0,
            is_folded_header: false,
        }
    }

    #[test]
    fn special_char_markers_ignore_tabs_but_cover_controls() {
        assert!(!contains_special_char("\t"));
        assert!(contains_special_char("\u{0007}"));
        assert_eq!(special_char_marker('\t'), None);
        assert_eq!(special_char_marker(' '), None);
        assert_eq!(special_char_marker('a'), None);
    }

    #[test]
    fn indentation_guides_follow_syntax_ranges() {
        let guides = vec![
            IndentGuide {
                start_line: 0,
                end_line: 4,
                column: 4,
            },
            IndentGuide {
                start_line: 1,
                end_line: 3,
                column: 8,
            },
        ];

        let rows = [display_row(0, 0, 120), display_row(2, 0, 120)];
        let columns = visible_indentation_columns(&IndentGuideIndex::new(guides), &rows, true);

        assert_eq!(columns.get(&2), Some(&vec![4, 8]));
        assert_eq!(columns.get(&0), None);
    }

    #[test]
    fn indentation_guides_skip_columns_before_wrapped_segment() {
        let guides = vec![IndentGuide {
            start_line: 0,
            end_line: 4,
            column: 8,
        }];

        let rows = [display_row(2, 4, 12)];
        let columns = visible_indentation_columns(&IndentGuideIndex::new(guides), &rows, true);

        assert_eq!(columns.get(&2), Some(&vec![8]));
    }

    #[test]
    fn disabled_indentation_guides_skip_index_queries() {
        let rows = [display_row(2, 0, 12)];
        let index = IndentGuideIndex::new(vec![IndentGuide {
            start_line: 0,
            end_line: 4,
            column: 8,
        }]);

        assert!(visible_indentation_columns(&index, &rows, false).is_empty());
    }

    #[test]
    fn overlapping_highlight_spans_are_clipped_to_unrendered_text() {
        assert_eq!(visible_highlight_range(2, 8, 0), Some(2..8));
        assert_eq!(visible_highlight_range(2, 8, 5), Some(5..8));
        assert_eq!(visible_highlight_range(2, 8, 8), None);
    }
}
