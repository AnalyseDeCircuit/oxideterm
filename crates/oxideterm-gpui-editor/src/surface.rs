// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{
    AnyElement, App, Bounds, Context, Div, Element, ElementId, ElementInputHandler, Entity,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    ParentElement, Pixels, ScrollWheelEvent, Window, div, point, prelude::*, px, rgb,
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
    bracket_pairs: Vec<BracketPair>,
    content_bounds: Option<Bounds<Pixels>>,
    marked_text: Option<MarkedText>,
    secondary_selections: Vec<Selection>,
    settings: EditorSettings,
    find_query: String,
    find_matches: Vec<FindMatch>,
    active_find_index: Option<usize>,
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
            bracket_pairs: Vec::new(),
            content_bounds: None,
            marked_text: None,
            secondary_selections: Vec::new(),
            settings: EditorSettings::default(),
            find_query: String::new(),
            find_matches: Vec::new(),
            active_find_index: None,
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

    pub fn set_language(&mut self, language: Option<LanguageId>, cx: &mut Context<Self>) {
        self.syntax =
            language.and_then(|language| SyntaxSession::parse(language, self.buffer.text()).ok());
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
        let syntax_edit = self
            .syntax
            .as_ref()
            .map(|_| SyntaxEdit::replace(self.buffer.text(), range, &replacement));
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
        match on_save(self.buffer.text(), window, cx) {
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
            && syntax.apply_edit(self.buffer.text(), edit).is_err()
        {
            let language = syntax.language_id();
            self.syntax = SyntaxSession::parse(language, self.buffer.text()).ok();
        }
        self.refresh_highlights();
    }

    fn reparse_syntax(&mut self) {
        if let Some(syntax) = self.syntax.as_mut()
            && syntax.reparse(self.buffer.text()).is_err()
        {
            let language = syntax.language_id();
            self.syntax = SyntaxSession::parse(language, self.buffer.text()).ok();
        }
        self.refresh_highlights();
    }

    fn refresh_highlights(&mut self) {
        self.highlight_spans = self
            .syntax
            .as_ref()
            .map(|syntax| syntax.highlight_spans(self.buffer.text()))
            .unwrap_or_default();
        self.bracket_pairs = self
            .syntax
            .as_ref()
            .map(|syntax| syntax.bracket_pairs(self.buffer.text()))
            .unwrap_or_default();
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

    fn place_cursor_on_line(&mut self, line: usize, visual_column: usize, cx: &mut Context<Self>) {
        let Some(start) = self.buffer.line_start_offset(line) else {
            return;
        };
        let line_text = self.buffer.line_text(line).unwrap_or_default();
        let byte_column = byte_column_for_visual_column(line_text, visual_column);
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
        let visual_column = visual_column_for_byte_column(line_text, position.column);
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
