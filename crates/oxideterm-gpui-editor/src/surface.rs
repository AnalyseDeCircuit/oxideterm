// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{cell::RefCell, ops::Range};

use gpui::{
    AnyElement, App, Bounds, Context, Div, Element, ElementId, ElementInputHandler, Entity,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    ParentElement, Pixels, Point, ScrollWheelEvent, Window, div, point, prelude::*, px, rgb,
};
use oxideterm_editor_core::{
    BufferOffset, Cursor, EditTransaction, FindMatch, LineCol, Selection, TextBuffer, TextEdit,
    TextRange,
};
use oxideterm_editor_syntax::{BracketPair, HighlightSpan, LanguageId, SyntaxEdit, SyntaxSession};
use oxideterm_theme::ThemeTokens;

use crate::{EditorAppearance, EditorMetrics, EditorSettings, EditorViewport};

mod commands;
mod coords;
mod input;
mod render;
mod search;
mod wrap;

pub use commands::EditorCommand;
use coords::{byte_column_for_visual_column, visual_column_for_byte_column};
use wrap::DisplayRow;

pub type SaveCallback =
    Box<dyn FnMut(&str, &mut Window, &mut Context<TextEditorView>) -> Result<(), String>>;

type BoundsCallback = Box<dyn FnOnce(Bounds<Pixels>, &mut Window, &mut App)>;

struct EditorBoundsProbe {
    child: Option<AnyElement>,
    on_bounds: Option<BoundsCallback>,
    view: Entity<TextEditorView>,
    focus_handle: FocusHandle,
}

impl EditorBoundsProbe {
    fn new(
        child: impl IntoElement,
        view: Entity<TextEditorView>,
        focus_handle: FocusHandle,
        on_bounds: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            child: Some(child.into_any_element()),
            on_bounds: Some(Box::new(on_bounds)),
            view,
            focus_handle,
        }
    }
}

impl IntoElement for EditorBoundsProbe {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorBoundsProbe {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let layout_id = self
            .child
            .as_mut()
            .expect("editor bounds probe child should render once")
            .request_layout(window, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        if let Some(on_bounds) = self.on_bounds.take() {
            on_bounds(bounds, window, cx);
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
        window.handle_input(
            &self.focus_handle,
            ElementInputHandler::new(_bounds, self.view.clone()),
            cx,
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorSaveStatus {
    Clean,
    Dirty,
    Saved,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MarkedText {
    text: String,
    range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DisplayRowsCache {
    buffer_version: u64,
    wrap_column: Option<usize>,
    rows: Vec<DisplayRow>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectionDrag {
    anchor: BufferOffset,
}

/// GPUI editor view for local text buffers.
pub struct TextEditorView {
    buffer: TextBuffer,
    cursor: Cursor,
    focus_handle: FocusHandle,
    viewport: EditorViewport,
    metrics: EditorMetrics,
    appearance: EditorAppearance,
    read_only: bool,
    on_save: Option<SaveCallback>,
    save_status: EditorSaveStatus,
    syntax: Option<SyntaxSession>,
    highlight_spans: Vec<HighlightSpan>,
    highlight_line_spans: Vec<Range<usize>>,
    bracket_pairs: Vec<BracketPair>,
    content_bounds: Option<Bounds<Pixels>>,
    marked_text: Option<MarkedText>,
    secondary_selections: Vec<Selection>,
    settings: EditorSettings,
    find_query: String,
    find_matches: Vec<FindMatch>,
    active_find_index: Option<usize>,
    display_rows_cache: RefCell<Option<DisplayRowsCache>>,
    selection_drag: Option<SelectionDrag>,
    transparent_background: bool,
}

impl TextEditorView {
    pub fn new(text: impl Into<String>, tokens: &ThemeTokens, cx: &mut Context<Self>) -> Self {
        let metrics = EditorMetrics::from_theme(tokens);
        Self {
            buffer: TextBuffer::new(text),
            cursor: Cursor::new(BufferOffset::ZERO),
            focus_handle: cx.focus_handle(),
            viewport: EditorViewport::new(metrics.overscan_rows),
            metrics,
            appearance: EditorAppearance::from_theme(tokens),
            read_only: false,
            on_save: None,
            save_status: EditorSaveStatus::Clean,
            syntax: None,
            highlight_spans: Vec::new(),
            highlight_line_spans: Vec::new(),
            bracket_pairs: Vec::new(),
            content_bounds: None,
            marked_text: None,
            secondary_selections: Vec::new(),
            settings: EditorSettings::default(),
            find_query: String::new(),
            find_matches: Vec::new(),
            active_find_index: None,
            display_rows_cache: RefCell::new(None),
            selection_drag: None,
            transparent_background: false,
        }
    }

    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    pub fn save_status(&self) -> &EditorSaveStatus {
        &self.save_status
    }

    pub fn mark_saved_external(&mut self, cx: &mut Context<Self>) {
        self.buffer.mark_saved();
        self.save_status = EditorSaveStatus::Saved;
        cx.notify();
    }

    pub fn mark_save_failed_external(
        &mut self,
        message: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.save_status = EditorSaveStatus::Failed(message.into());
        cx.notify();
    }

    pub fn replace_text_external(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = text.into();
        if self.buffer.text() == text {
            return;
        }
        let range = TextRange::new(BufferOffset::ZERO, BufferOffset(self.buffer.len()));
        if self
            .buffer
            .apply_transaction(EditTransaction::single(TextEdit::new(range, text)))
            .is_ok()
        {
            self.cursor
                .set_selection(Selection::caret(BufferOffset::ZERO));
            self.secondary_selections.clear();
            self.marked_text = None;
            self.save_status = EditorSaveStatus::Dirty;
            self.reparse_syntax();
            self.refresh_find_matches();
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn set_on_save(&mut self, on_save: SaveCallback) {
        self.on_save = Some(on_save);
    }

    pub fn set_settings(&mut self, settings: EditorSettings, cx: &mut Context<Self>) {
        self.settings = settings;
        self.viewport
            .clamp(self.document_row_count(), self.metrics.line_height);
        self.refresh_find_matches();
        cx.notify();
    }

    pub fn apply_ide_runtime_settings(
        &mut self,
        tokens: &ThemeTokens,
        font_size: f32,
        line_height: f32,
        word_wrap: bool,
        background_active: bool,
        cx: &mut Context<Self>,
    ) {
        self.appearance = EditorAppearance::from_theme(tokens);
        self.metrics =
            EditorMetrics::from_theme_with_editor_typography(tokens, font_size, line_height);
        self.transparent_background = background_active;
        // Tauri wires Settings.ide.wordWrap into CodeMirror's lineWrapping
        // compartment. Keep that as editor settings, not a one-off render flag.
        self.settings.soft_wrap = word_wrap;
        self.viewport
            .clamp(self.document_row_count(), self.metrics.line_height);
        cx.notify();
    }

    pub fn set_language(&mut self, language: Option<LanguageId>, cx: &mut Context<Self>) {
        self.syntax = language.and_then(|language| {
            self.buffer
                .with_text(|text| SyntaxSession::parse(language, text).ok())
        });
        self.refresh_highlights();
        cx.notify();
    }

    pub fn insert_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.replace_all_selections_with_caret(text, cx);
    }

    pub fn delete_backward(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let ranges = self
            .all_selections()
            .into_iter()
            .map(|selection| {
                if selection.is_caret() {
                    TextRange::new(
                        self.buffer.previous_grapheme_offset(selection.head),
                        selection.head,
                    )
                } else {
                    selection.range()
                }
            })
            .collect();
        self.replace_ranges_with_caret(ranges, "", cx);
    }

    pub fn delete_forward(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let ranges = self
            .all_selections()
            .into_iter()
            .map(|selection| {
                if selection.is_caret() {
                    TextRange::new(
                        selection.head,
                        self.buffer.next_grapheme_offset(selection.head),
                    )
                } else {
                    selection.range()
                }
            })
            .collect();
        self.replace_ranges_with_caret(ranges, "", cx);
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.cursor.set_selection(Selection::new(
            BufferOffset::ZERO,
            BufferOffset(self.buffer.len()),
        ));
        self.secondary_selections.clear();
        cx.notify();
    }

    pub fn add_cursor_at(&mut self, offset: BufferOffset, cx: &mut Context<Self>) {
        let selection = Selection::caret(offset);
        if self.buffer.offset_to_line_col(offset).is_ok()
            && !self.secondary_selections.contains(&selection)
            && self.cursor.selection() != selection
        {
            self.secondary_selections.push(selection);
            self.secondary_selections.sort_by_key(|selection| {
                let range = selection.range();
                (range.start.0, range.end.0)
            });
            cx.notify();
        }
    }

    pub fn clear_secondary_cursors(&mut self, cx: &mut Context<Self>) {
        if !self.secondary_selections.is_empty() {
            self.secondary_selections.clear();
            cx.notify();
        }
    }

    fn replace_range_with_caret(
        &mut self,
        range: TextRange,
        replacement: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let replacement = replacement.into();
        if range.is_empty() && replacement.is_empty() {
            return;
        }
        let caret = BufferOffset(range.start.0 + replacement.len());
        let syntax_edit = self.syntax.as_ref().map(|_| {
            self.buffer
                .with_text(|text| SyntaxEdit::replace(text, range, &replacement))
        });
        if self
            .buffer
            .apply_transaction(EditTransaction::single(TextEdit::new(range, replacement)))
            .is_ok()
        {
            self.apply_syntax_edit(syntax_edit);
            self.cursor.set_selection(Selection::caret(caret));
            self.secondary_selections.clear();
            self.marked_text = None;
            self.save_status = EditorSaveStatus::Dirty;
            self.refresh_find_matches();
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
    }

    fn replace_all_selections_with_caret(
        &mut self,
        replacement: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let selections = self.all_selections();
        let ranges = selections
            .iter()
            .map(|selection| selection.range())
            .collect::<Vec<_>>();
        self.replace_ranges_with_caret(ranges, replacement, cx);
    }

    fn replace_ranges_with_caret(
        &mut self,
        ranges: Vec<TextRange>,
        replacement: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let replacement = replacement.into();
        if ranges.len() <= 1 {
            let range = ranges
                .into_iter()
                .next()
                .unwrap_or_else(|| self.cursor.selection().range());
            self.replace_range_with_caret(range, replacement, cx);
            return;
        }
        let edits = ranges
            .iter()
            .filter(|range| !(range.is_empty() && replacement.is_empty()))
            .map(|range| TextEdit::new(*range, replacement.clone()))
            .collect::<Vec<_>>();
        if edits.is_empty() {
            return;
        }
        if self
            .buffer
            .apply_transaction(EditTransaction::new(edits))
            .is_ok()
        {
            let last = ranges
                .iter()
                .copied()
                .max_by_key(|range| range.start.0)
                .unwrap_or_else(|| self.cursor.selection().range());
            self.cursor.set_selection(Selection::caret(BufferOffset(
                last.start.0 + replacement.len(),
            )));
            self.secondary_selections.clear();
            self.marked_text = None;
            self.save_status = EditorSaveStatus::Dirty;
            self.reparse_syntax();
            self.refresh_find_matches();
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut on_save) = self.on_save.take() else {
            self.save_status = EditorSaveStatus::Failed("save callback is not configured".into());
            cx.notify();
            return;
        };
        let result = self.buffer.with_text(|text| on_save(text, window, cx));
        match result {
            Ok(()) => {
                self.buffer.mark_saved();
                self.save_status = EditorSaveStatus::Saved;
            }
            Err(message) => {
                self.save_status = EditorSaveStatus::Failed(message);
            }
        }
        self.on_save = Some(on_save);
        cx.notify();
    }

    fn apply_syntax_edit(&mut self, edit: Option<SyntaxEdit>) {
        if let (Some(syntax), Some(edit)) = (self.syntax.as_mut(), edit)
            && self
                .buffer
                .with_text(|text| syntax.apply_edit(text, edit))
                .is_err()
        {
            let language = syntax.language_id();
            self.syntax = self
                .buffer
                .with_text(|text| SyntaxSession::parse(language, text).ok());
        }
        self.refresh_highlights();
    }

    fn reparse_syntax(&mut self) {
        if let Some(syntax) = self.syntax.as_mut()
            && self.buffer.with_text(|text| syntax.reparse(text)).is_err()
        {
            let language = syntax.language_id();
            self.syntax = self
                .buffer
                .with_text(|text| SyntaxSession::parse(language, text).ok());
        }
        self.refresh_highlights();
    }

    fn refresh_highlights(&mut self) {
        self.highlight_spans = self.buffer.with_text(|text| {
            self.syntax
                .as_ref()
                .map(|syntax| syntax.highlight_spans(text))
                .unwrap_or_default()
        });
        self.highlight_spans
            .sort_by_key(|span| (span.range.start.0, span.range.end.0));
        self.highlight_line_spans = self.build_highlight_line_spans();
        self.bracket_pairs = self.buffer.with_text(|text| {
            self.syntax
                .as_ref()
                .map(|syntax| syntax.bracket_pairs(text))
                .unwrap_or_default()
        });
    }

    fn build_highlight_line_spans(&self) -> Vec<Range<usize>> {
        let mut ranges = Vec::with_capacity(self.buffer.line_count());
        let mut first_span = 0;
        let mut last_span = 0;

        for line in 0..self.buffer.line_count() {
            let Some(line_start) = self.buffer.line_start_offset(line).map(|offset| offset.0)
            else {
                ranges.push(0..0);
                continue;
            };
            let line_end = self
                .buffer
                .line_end_offset(line)
                .map(|offset| offset.0)
                .unwrap_or(line_start);

            while first_span < self.highlight_spans.len()
                && self.highlight_spans[first_span].range.end.0 <= line_start
            {
                first_span += 1;
            }
            last_span = last_span.max(first_span);
            while last_span < self.highlight_spans.len()
                && self.highlight_spans[last_span].range.start.0 < line_end
            {
                last_span += 1;
            }
            ranges.push(first_span..last_span);
        }

        ranges
    }

    fn handle_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(self.metrics.line_height));
        let dx = if event.modifiers.shift {
            -f32::from(delta.y)
        } else {
            -f32::from(delta.x)
        };
        let dy = if event.modifiers.shift {
            0.0
        } else {
            -f32::from(delta.y)
        };
        self.viewport
            .scroll_by(dx, dy, self.document_row_count(), self.metrics.line_height);
        cx.stop_propagation();
        cx.notify();
    }

    fn set_viewport_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Bounds are captured during the same frame's prepaint pass so the
        // editor does not render one-frame-stale virtual rows after resizing.
        self.content_bounds = Some(bounds);
        if self.viewport.set_height(f32::from(bounds.size.height)) {
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
    }

    fn measure_code_metrics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // CodeMirror measures actual font advances through the browser layout
        // engine. GPUI needs the same explicit measurement; the old 0.62 ratio
        // is only a startup fallback before the first render has a Window.
        if self
            .metrics
            .measure_code_cell_width(window, &self.appearance.font_family)
        {
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
    }

    fn offset_for_window_point(&self, point: Point<Pixels>) -> Option<BufferOffset> {
        let display_row = self.display_row_for_window_y(point.y)?;
        let column = display_row.start_col + self.visual_column_for_window_x(point.x);
        let line_text = self.buffer.line_text(display_row.line).unwrap_or_default();
        let byte_column = byte_column_for_visual_column(&line_text, column);
        self.buffer
            .line_col_to_offset(LineCol::new(display_row.line, byte_column))
            .ok()
    }

    fn start_selection_drag(
        &mut self,
        anchor: BufferOffset,
        head: BufferOffset,
        cx: &mut Context<Self>,
    ) {
        self.selection_drag = Some(SelectionDrag { anchor });
        self.cursor.set_selection(Selection::new(anchor, head));
        self.secondary_selections.clear();
        self.marked_text = None;
        cx.notify();
    }

    fn drag_selection_to_point(&mut self, point: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.selection_drag else {
            return;
        };
        let Some(head) = self.offset_for_window_point(point) else {
            return;
        };
        self.cursor.set_selection(Selection::new(drag.anchor, head));
        cx.notify();
    }

    fn finish_selection_drag(&mut self, cx: &mut Context<Self>) {
        if self.selection_drag.take().is_some() {
            cx.notify();
        }
    }

    fn place_cursor_on_line(&mut self, line: usize, visual_column: usize, cx: &mut Context<Self>) {
        let Some(start) = self.buffer.line_start_offset(line) else {
            return;
        };
        let line_text = self.buffer.line_text(line).unwrap_or_default();
        let byte_column = byte_column_for_visual_column(&line_text, visual_column);
        if let Ok(offset) = self
            .buffer
            .line_col_to_offset(LineCol::new(line, byte_column))
        {
            self.cursor
                .set_selection(Selection::caret(start.max(offset)));
            self.secondary_selections.clear();
            self.marked_text = None;
            cx.notify();
        }
    }

    fn visual_column_for_window_x(&self, x: Pixels) -> usize {
        let content_origin_x = self
            .content_bounds
            .map(|bounds| bounds.origin.x)
            .unwrap_or(px(0.0));
        let x = f32::from(x - content_origin_x)
            - self.metrics.gutter_width
            - self.metrics.content_padding_x
            + self.viewport.scroll_x_px;
        // The Phase 2 surface is explicitly monospace. Rounding places clicks
        // on the nearest caret slot instead of always biasing to the left edge.
        (x / self.metrics.char_width).round().max(0.0) as usize
    }

    fn bounds_for_byte_offset(
        &self,
        offset: BufferOffset,
        fallback_bounds: Bounds<Pixels>,
    ) -> Bounds<Pixels> {
        let bounds = self.content_bounds.unwrap_or(fallback_bounds);
        let position = self
            .buffer
            .offset_to_line_col(offset)
            .unwrap_or_else(|_| LineCol::new(0, 0));
        let line_text = self.buffer.line_text(position.line).unwrap_or_default();
        let visual_column = visual_column_for_byte_column(&line_text, position.column);
        Bounds {
            origin: bounds.origin
                + point(
                    px(self.metrics.gutter_width + self.metrics.content_padding_x
                        - self.viewport.scroll_x_px
                        + visual_column as f32 * self.metrics.char_width),
                    px(position.line as f32 * self.metrics.line_height - self.viewport.scroll_y_px),
                ),
            size: gpui::size(px(1.0), px(self.metrics.line_height)),
        }
    }

    fn all_selections(&self) -> Vec<Selection> {
        let mut selections = Vec::with_capacity(self.secondary_selections.len() + 1);
        selections.push(self.cursor.selection());
        selections.extend(self.secondary_selections.iter().copied());
        selections.sort_by_key(|selection| {
            let range = selection.range();
            (range.start.0, range.end.0)
        });
        selections.dedup();
        selections
    }

    fn matching_bracket_pair(&self) -> Option<BracketPair> {
        let head = self.cursor.selection().head.0;
        self.bracket_pairs
            .iter()
            .find(|pair| {
                pair.open.0 == head
                    || pair.close.0 == head
                    || pair.open.0.saturating_add(1) == head
                    || pair.close.0.saturating_add(1) == head
            })
            .cloned()
    }
}

impl Focusable for TextEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn colored_text(text: &str, color: u32) -> Div {
    div().text_color(rgb(color)).child(text.to_string())
}
