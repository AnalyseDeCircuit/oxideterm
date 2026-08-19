// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{cell::RefCell, collections::HashMap, ops::Range, sync::Arc, time::Duration};

use gpui::{
    AnyElement, App, Bounds, Context, Div, Element, ElementId, ElementInputHandler, Entity,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    ParentElement, Pixels, Point, ScrollWheelEvent, SharedString, Task, TextRun, Timer, Window,
    div, point, prelude::*, px, rgb,
};
use oxideterm_editor_core::{
    BufferOffset, Cursor, EditTransaction, FindMatch, LineCol, Selection, TextBuffer, TextEdit,
    TextRange, word_at,
};
use oxideterm_editor_syntax::{BracketPair, HighlightSpan, LanguageId, SyntaxEdit, SyntaxSession};
use oxideterm_theme::ThemeTokens;

use crate::{
    EditorAppearance, EditorMetrics, EditorSettings, EditorViewport, metrics::editor_code_font,
};

mod commands;
mod coords;
mod fold;
mod indent_index;
mod input;
mod render;
mod search;
mod wrap;

pub use commands::EditorCommand;
use coords::{byte_column_for_visual_column, visual_column_for_byte_column};
use indent_index::IndentGuideIndex;
use wrap::DisplayRow;

pub type SaveCallback =
    Box<dyn FnMut(&str, &mut Window, &mut Context<TextEditorView>) -> Result<(), String>>;
pub type ModifiedWordClickCallback =
    Box<dyn FnMut(String, &mut Window, &mut Context<TextEditorView>) -> Result<(), String>>;

const EDITOR_CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// Controls whether the editor owns a full document surface or sits inside an
/// existing input row whose surrounding component already provides chrome.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditorPresentation {
    #[default]
    Document,
    Inline,
}

fn content_padding_x_for_presentation(
    presentation: EditorPresentation,
    document_content_padding_x: f32,
) -> f32 {
    match presentation {
        EditorPresentation::Document => document_content_padding_x,
        // The surrounding input row owns the horizontal inset in inline mode.
        EditorPresentation::Inline => 0.0,
    }
}

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
pub struct EditorContextMenuLabels {
    pub copy: String,
    pub cut: String,
    pub paste: String,
    pub select_all: String,
}

impl Default for EditorContextMenuLabels {
    fn default() -> Self {
        Self {
            copy: "Copy".into(),
            cut: "Cut".into(),
            paste: "Paste".into(),
            select_all: "Select All".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EditorContextMenu {
    x: f32,
    y: f32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DisplayRowsCache {
    buffer_version: u64,
    wrap_column: Option<usize>,
    fold_revision: u64,
    max_width_columns: usize,
    rows: Arc<Vec<DisplayRow>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct HighlightChunkCacheKey {
    pub buffer_version: u64,
    pub line: usize,
    pub range_start: usize,
    pub range_end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LineChunkSpec {
    pub start: usize,
    pub end: usize,
    pub color: u32,
    pub text: gpui::SharedString,
}

#[derive(Clone, Debug, Default)]
struct HighlightChunkCache {
    entries: HashMap<HighlightChunkCacheKey, Arc<Vec<LineChunkSpec>>>,
}

impl HighlightChunkCache {
    // Keep roughly several large viewports of rendered rows. The cache is a
    // scroll hot-path helper, so clearing it wholesale is cheaper than managing
    // a per-entry LRU list in the render path.
    const MAX_ENTRIES: usize = 2048;

    fn get(&self, key: &HighlightChunkCacheKey) -> Option<Arc<Vec<LineChunkSpec>>> {
        self.entries.get(key).cloned()
    }

    fn insert(
        &mut self,
        key: HighlightChunkCacheKey,
        chunks: Arc<Vec<LineChunkSpec>>,
    ) -> Arc<Vec<LineChunkSpec>> {
        if self.entries.len() >= Self::MAX_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(key, chunks.clone());
        chunks
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectionDrag {
    anchor: BufferOffset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FoldRange {
    pub start_line: usize,
    pub end_line: usize,
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
    on_modified_word_click: Option<ModifiedWordClickCallback>,
    save_status: EditorSaveStatus,
    syntax: Option<SyntaxSession>,
    highlight_spans: Vec<HighlightSpan>,
    highlight_line_spans: Vec<Range<usize>>,
    // Cursor movement queries bracket matches on every frame. Index every
    // accepted caret slot so lookup does not rescan the document's pairs.
    bracket_pair_by_caret: HashMap<usize, BracketPair>,
    content_bounds: Option<Bounds<Pixels>>,
    marked_text: Option<MarkedText>,
    secondary_selections: Vec<Selection>,
    settings: EditorSettings,
    find_query: String,
    find_matches: Vec<FindMatch>,
    // Scroll rendering asks for highlights per visible row. Keep search hits
    // indexed by line so each row does not scan every match in a large file.
    find_line_matches: Vec<Range<usize>>,
    active_find_index: Option<usize>,
    foldable_ranges: Vec<FoldRange>,
    folded_ranges: Vec<FoldRange>,
    indent_guide_index: IndentGuideIndex,
    fold_revision: u64,
    display_rows_cache: RefCell<Option<DisplayRowsCache>>,
    highlight_chunk_cache: RefCell<HighlightChunkCache>,
    selection_drag: Option<SelectionDrag>,
    transparent_background: bool,
    presentation: EditorPresentation,
    context_menu: Option<EditorContextMenu>,
    context_menu_labels: EditorContextMenuLabels,
    caret_visible: bool,
    caret_blink_focused: bool,
    caret_blink_generation: u64,
    caret_blink_task: Option<Task<()>>,
}

impl TextEditorView {
    pub fn new(text: impl Into<String>, tokens: &ThemeTokens, cx: &mut Context<Self>) -> Self {
        let metrics = EditorMetrics::from_theme(tokens);
        let settings = EditorSettings::default();
        let buffer = TextBuffer::new(text);
        Self {
            buffer,
            cursor: Cursor::new(BufferOffset::ZERO),
            focus_handle: cx.focus_handle(),
            viewport: EditorViewport::new(metrics.overscan_rows),
            metrics,
            appearance: EditorAppearance::from_theme(tokens),
            read_only: false,
            on_save: None,
            on_modified_word_click: None,
            save_status: EditorSaveStatus::Clean,
            syntax: None,
            highlight_spans: Vec::new(),
            highlight_line_spans: Vec::new(),
            bracket_pair_by_caret: HashMap::new(),
            content_bounds: None,
            marked_text: None,
            secondary_selections: Vec::new(),
            settings,
            find_query: String::new(),
            find_matches: Vec::new(),
            find_line_matches: Vec::new(),
            active_find_index: None,
            foldable_ranges: Vec::new(),
            folded_ranges: Vec::new(),
            indent_guide_index: IndentGuideIndex::default(),
            fold_revision: 0,
            display_rows_cache: RefCell::new(None),
            highlight_chunk_cache: RefCell::new(HighlightChunkCache::default()),
            selection_drag: None,
            transparent_background: false,
            presentation: EditorPresentation::Document,
            context_menu: None,
            context_menu_labels: EditorContextMenuLabels::default(),
            caret_visible: true,
            caret_blink_focused: false,
            caret_blink_generation: 0,
            caret_blink_task: None,
        }
    }

    fn sync_caret_blink_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.caret_blink_focused == focused {
            return;
        }
        self.caret_blink_focused = focused;
        if focused {
            self.restart_caret_blink(cx);
        } else {
            // Dropping the task stops repainting as soon as this editor loses focus.
            self.caret_blink_generation = self.caret_blink_generation.wrapping_add(1);
            self.caret_blink_task = None;
            self.caret_visible = true;
        }
    }

    pub(super) fn activate_caret_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_blink_focused = true;
        self.restart_caret_blink(cx);
    }

    fn restart_caret_blink_if_focused(&mut self, cx: &mut Context<Self>) {
        if self.caret_blink_focused {
            self.restart_caret_blink(cx);
        }
    }

    fn restart_caret_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_blink_generation = self.caret_blink_generation.wrapping_add(1);
        self.caret_blink_task = None;
        self.caret_visible = true;
        let generation = self.caret_blink_generation;
        self.caret_blink_task = Some(cx.spawn(async move |editor, cx| {
            loop {
                Timer::after(EDITOR_CARET_BLINK_INTERVAL).await;
                let should_continue = editor
                    .update(cx, |editor, cx| {
                        if editor.caret_blink_generation != generation
                            || !editor.caret_blink_focused
                        {
                            return false;
                        }
                        editor.caret_visible = !editor.caret_visible;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
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
            self.clear_folds_after_buffer_change();
            self.refresh_find_matches();
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            self.restart_caret_blink_if_focused(cx);
            cx.notify();
        }
    }

    pub fn move_cursor_to_document_end(&mut self, cx: &mut Context<Self>) {
        // External draft insertion should leave the next typed character after
        // the inserted content instead of at the beginning of the document.
        self.cursor
            .set_selection(Selection::caret(BufferOffset(self.buffer.len())));
        self.secondary_selections.clear();
        self.marked_text = None;
        cx.notify();
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn set_on_save(&mut self, on_save: SaveCallback) {
        self.on_save = Some(on_save);
    }

    pub fn set_on_modified_word_click(&mut self, on_click: ModifiedWordClickCallback) {
        self.on_modified_word_click = Some(on_click);
    }

    pub fn set_context_menu_labels(&mut self, labels: EditorContextMenuLabels) {
        self.context_menu_labels = labels;
    }

    pub fn set_presentation(&mut self, presentation: EditorPresentation, cx: &mut Context<Self>) {
        if self.presentation == presentation {
            return;
        }
        // Inline mode removes only visual chrome. Buffer, cursor, undo, IME,
        // and selection ownership remain on this editor instance.
        self.presentation = presentation;
        self.display_rows_cache.borrow_mut().take();
        self.viewport
            .clamp(self.document_row_count(), self.metrics.line_height);
        cx.notify();
    }

    pub fn set_placeholder(&mut self, placeholder: Option<String>, cx: &mut Context<Self>) {
        if self.settings.placeholder == placeholder {
            return;
        }
        self.settings.placeholder = placeholder;
        cx.notify();
    }

    pub fn set_settings(&mut self, settings: EditorSettings, cx: &mut Context<Self>) {
        self.settings = settings;
        self.refresh_foldable_ranges();
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
        self.apply_runtime_settings(
            tokens,
            tokens.metrics.markdown_code_font_family.to_string(),
            font_size,
            line_height,
            word_wrap,
            background_active,
            cx,
        );
    }

    pub fn apply_runtime_settings(
        &mut self,
        tokens: &ThemeTokens,
        font_family: String,
        font_size: f32,
        line_height: f32,
        word_wrap: bool,
        background_active: bool,
        cx: &mut Context<Self>,
    ) {
        self.appearance = EditorAppearance::from_theme(tokens);
        // Embedded editors can follow the typography of their owning surface.
        self.appearance.font_family = font_family;
        self.metrics =
            EditorMetrics::from_theme_with_editor_typography(tokens, font_size, line_height);
        self.transparent_background = background_active;
        self.highlight_chunk_cache.borrow_mut().clear();
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
        self.refresh_foldable_ranges();
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

    pub fn reveal_line_column(&mut self, line: u32, column: u32, cx: &mut Context<Self>) {
        let line_index = line.saturating_sub(1) as usize;
        if line_index >= self.buffer.line_count() {
            return;
        }
        let unfolded = self.unfold_line_if_hidden(line_index);
        let line_text = self.buffer.line_text(line_index).unwrap_or_default();
        let byte_column =
            coords::floor_char_boundary(&line_text, column.saturating_sub(1) as usize);
        if let Ok(offset) = self
            .buffer
            .line_col_to_offset(LineCol::new(line_index, byte_column))
        {
            self.cursor.set_selection(Selection::caret(offset));
            self.secondary_selections.clear();
            self.marked_text = None;
        }
        let visual_column = visual_column_for_byte_column(&line_text, byte_column);
        let display_rows = self.display_rows();
        let display_index =
            wrap::display_row_for_visual_column(&display_rows, line_index, visual_column)
                .map(|(index, _, _)| index)
                .unwrap_or(line_index);
        self.reveal_display_row(display_index);
        if unfolded {
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
        }
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
            self.clear_folds_after_buffer_change();
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
            self.clear_folds_after_buffer_change();
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
                // The IDE save path is asynchronous, matching Tauri's
                // `saveFile`: dirty state clears only when the remote write
                // resolves successfully and the owner calls `mark_saved_external`.
                self.save_status = if self.buffer.is_dirty() {
                    EditorSaveStatus::Dirty
                } else {
                    EditorSaveStatus::Saved
                };
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
        let bracket_pairs = self.buffer.with_text(|text| {
            self.syntax
                .as_ref()
                .map(|syntax| syntax.bracket_pairs(text))
                .unwrap_or_default()
        });
        self.bracket_pair_by_caret = build_bracket_pair_index(&bracket_pairs);
        self.highlight_chunk_cache.borrow_mut().clear();
    }

    pub(super) fn refresh_indent_guides(&mut self) {
        let guides = self.buffer.with_text(|text| {
            self.syntax
                .as_ref()
                .map(|syntax| syntax.indent_guides(text, self.settings.tab_size))
                .unwrap_or_default()
        });
        self.indent_guide_index = IndentGuideIndex::new(guides);
    }

    fn active_selections(&self) -> Vec<Selection> {
        let primary = self.cursor.selection();
        let mut selections = Vec::new();
        if !primary.is_caret() {
            selections.push(primary);
        }
        selections.extend(
            self.secondary_selections
                .iter()
                .copied()
                .filter(|selection| !selection.is_caret()),
        );
        if selections.len() > 1 {
            selections.sort_by_key(|selection| {
                let range = selection.range();
                (range.start.0, range.end.0)
            });
            selections.dedup();
        }
        selections
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

    fn build_find_line_matches(&self) -> Vec<Range<usize>> {
        let mut ranges = Vec::with_capacity(self.buffer.line_count());
        let mut first_match = 0;
        let mut last_match = 0;

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

            while first_match < self.find_matches.len()
                && self.find_matches[first_match].range.end.0 <= line_start
            {
                first_match += 1;
            }
            last_match = last_match.max(first_match);
            while last_match < self.find_matches.len()
                && self.find_matches[last_match].range.start.0 < line_end
            {
                last_match += 1;
            }
            ranges.push(first_match..last_match);
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
        let max_scroll_x_px = self.max_horizontal_scroll_px();
        let scrolled = self.viewport.scroll_by(
            dx,
            dy,
            max_scroll_x_px,
            self.document_row_count(),
            self.metrics.line_height,
        );
        cx.stop_propagation();
        if scrolled {
            cx.notify();
        }
    }

    pub(super) fn horizontal_viewport_width_px(&self) -> f32 {
        // The gutter remains fixed while only the document content moves horizontally.
        (self.viewport.width_px - self.visible_gutter_width()).max(0.0)
    }

    pub(super) fn horizontal_document_width_px(&self) -> f32 {
        self.document_width_columns() as f32 * self.metrics.char_width
            + self.visible_content_padding_x() * 2.0
    }

    pub(super) fn max_horizontal_scroll_px(&self) -> f32 {
        (self.horizontal_document_width_px() - self.horizontal_viewport_width_px()).max(0.0)
    }

    pub(super) fn vertical_scroll_y_px(&self) -> f32 {
        self.viewport.scroll_y_px
    }

    pub(super) fn reveal_display_row(&mut self, display_index: usize) {
        self.viewport.reveal_line(
            display_index,
            self.document_row_count(),
            self.metrics.line_height,
        );
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
        let width_changed = self.viewport.set_width(f32::from(bounds.size.width));
        let height_changed = self.viewport.set_height(f32::from(bounds.size.height));
        if width_changed || height_changed {
            self.viewport
                .clamp_horizontal(self.max_horizontal_scroll_px());
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

    pub(super) fn visible_gutter_width(&self) -> f32 {
        if self.presentation == EditorPresentation::Inline {
            0.0
        } else {
            self.metrics.gutter_width
        }
    }

    pub(super) fn visible_content_padding_x(&self) -> f32 {
        content_padding_x_for_presentation(self.presentation, self.metrics.content_padding_x)
    }

    fn offset_for_window_point(
        &self,
        point: Point<Pixels>,
        window: &mut Window,
    ) -> Option<BufferOffset> {
        let display_row = self.display_row_for_window_y(point.y)?;
        let line_text = self.buffer.line_text(display_row.line).unwrap_or_default();
        let byte_start = byte_column_for_visual_column(&line_text, display_row.start_col);
        let byte_end = byte_column_for_visual_column(&line_text, display_row.end_col);
        let segment_text = line_text.get(byte_start..byte_end)?;
        let relative_x = f32::from(point.x)
            - self
                .content_bounds
                .map(|bounds| f32::from(bounds.origin.x))
                .unwrap_or_default()
            - self.visible_gutter_width()
            - self.visible_content_padding_x()
            + self.viewport.scroll_x_px;
        let local_byte =
            self.closest_grapheme_byte_for_x(segment_text, relative_x.max(0.0), window);
        let byte_column = byte_start + local_byte;
        self.buffer
            .line_col_to_offset(LineCol::new(display_row.line, byte_column))
            .ok()
    }

    fn modified_word_click(
        &mut self,
        offset: BufferOffset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let word = self.buffer.with_text(|text| word_at(text, offset));
        if word.is_empty() {
            return false;
        }
        let Some(mut on_click) = self.on_modified_word_click.take() else {
            return false;
        };
        let handled = on_click(word, window, cx).is_ok();
        self.on_modified_word_click = Some(on_click);
        handled
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

    fn drag_selection_to_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.selection_drag else {
            return;
        };
        let Some(head) = self.offset_for_window_point(point, window) else {
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
            - self.visible_gutter_width()
            - self.visible_content_padding_x()
            + self.viewport.scroll_x_px;
        // The Phase 2 surface is explicitly monospace. Rounding places clicks
        // on the nearest caret slot instead of always biasing to the left edge.
        (x / self.metrics.char_width).round().max(0.0) as usize
    }

    fn bounds_for_byte_offset(
        &self,
        offset: BufferOffset,
        fallback_bounds: Bounds<Pixels>,
        window: &mut Window,
    ) -> Bounds<Pixels> {
        let bounds = self.content_bounds.unwrap_or(fallback_bounds);
        let position = self
            .buffer
            .offset_to_line_col(offset)
            .unwrap_or_else(|_| LineCol::new(0, 0));
        let line_text = self.buffer.line_text(position.line).unwrap_or_default();
        let visual_column = visual_column_for_byte_column(&line_text, position.column);
        let display_rows = self.display_rows();
        let (display_index, display_row) =
            wrap::display_row_for_visual_column(&display_rows, position.line, visual_column)
                .map(|(index, row, _)| (index, row))
                .unwrap_or((
                    position.line,
                    DisplayRow {
                        line: position.line,
                        start_col: 0,
                        end_col: visual_column,
                        is_first: true,
                        is_folded_header: false,
                    },
                ));
        let byte_start = byte_column_for_visual_column(&line_text, display_row.start_col);
        let segment_text = line_text
            .get(byte_start..position.column)
            .unwrap_or_default();
        let caret_x = f32::from(self.shape_coordinate_line(segment_text, window).width());
        Bounds {
            origin: bounds.origin
                + point(
                    px(
                        self.visible_gutter_width() + self.visible_content_padding_x()
                            - self.viewport.scroll_x_px
                            + caret_x,
                    ),
                    px(display_index as f32 * self.metrics.line_height
                        - self.vertical_scroll_y_px()),
                ),
            size: gpui::size(px(1.0), px(self.metrics.line_height)),
        }
    }

    fn shape_coordinate_line(&self, text: &str, window: &mut Window) -> gpui::ShapedLine {
        let text = SharedString::from(text.to_string());
        let run = TextRun {
            len: text.len(),
            font: editor_code_font(&self.appearance.font_family),
            color: rgb(self.appearance.text_hex).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text, px(self.metrics.font_size), &[run], None)
    }

    fn closest_grapheme_byte_for_x(&self, text: &str, x: f32, window: &mut Window) -> usize {
        use unicode_segmentation::UnicodeSegmentation;

        let shaped = self.shape_coordinate_line(text, window);
        // Font shaping is authoritative for pointer hit testing, but only
        // grapheme boundaries are legal caret positions.
        text.grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
            .min_by(|left, right| {
                let left_distance = (f32::from(shaped.x_for_index(*left)) - x).abs();
                let right_distance = (f32::from(shaped.x_for_index(*right)) - x).abs();
                left_distance.total_cmp(&right_distance)
            })
            .unwrap_or_default()
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

    fn has_primary_or_secondary_selection(&self) -> bool {
        !self.cursor.selection().is_caret()
            || self
                .secondary_selections
                .iter()
                .any(|selection| !selection.is_caret())
    }

    fn matching_bracket_pair(&self) -> Option<BracketPair> {
        let head = self.cursor.selection().head.0;
        self.bracket_pair_by_caret.get(&head).cloned()
    }
}

fn build_bracket_pair_index(pairs: &[BracketPair]) -> HashMap<usize, BracketPair> {
    let mut index = HashMap::with_capacity(pairs.len().saturating_mul(4));
    for pair in pairs {
        for caret in [
            pair.open.0,
            pair.open.0.saturating_add(1),
            pair.close.0,
            pair.close.0.saturating_add(1),
        ] {
            // Preserve the syntax provider's first-match behavior when two
            // bracket pairs share the caret slot between adjacent tokens.
            index.entry(caret).or_insert_with(|| pair.clone());
        }
    }
    index
}

impl Focusable for TextEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn colored_text(text: &str, color: u32) -> Div {
    div().text_color(rgb(color)).child(text.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{HighlightChunkCache, HighlightChunkCacheKey, LineChunkSpec};

    fn cache_key(line: usize) -> HighlightChunkCacheKey {
        HighlightChunkCacheKey {
            buffer_version: 7,
            line,
            range_start: 0,
            range_end: 16,
        }
    }

    #[test]
    fn highlight_chunk_cache_reuses_arc_for_same_visible_row_segment() {
        let mut cache = HighlightChunkCache::default();
        let key = cache_key(3);
        let chunks = Arc::new(vec![LineChunkSpec {
            start: 0,
            end: 4,
            color: 0xff00ff,
            text: gpui::SharedString::from("test"),
        }]);

        let inserted = cache.insert(key, chunks.clone());
        let cached = cache.get(&key).expect("highlight chunks should be cached");

        assert!(Arc::ptr_eq(&inserted, &cached));
        assert!(Arc::ptr_eq(&chunks, &cached));
    }

    #[test]
    fn highlight_chunk_cache_clears_when_scroll_window_exceeds_limit() {
        let mut cache = HighlightChunkCache::default();
        for line in 0..HighlightChunkCache::MAX_ENTRIES {
            cache.insert(cache_key(line), Arc::new(Vec::new()));
        }

        cache.insert(
            cache_key(HighlightChunkCache::MAX_ENTRIES),
            Arc::new(Vec::new()),
        );

        assert!(cache.get(&cache_key(0)).is_none());
        assert!(
            cache
                .get(&cache_key(HighlightChunkCache::MAX_ENTRIES))
                .is_some()
        );
    }
}
