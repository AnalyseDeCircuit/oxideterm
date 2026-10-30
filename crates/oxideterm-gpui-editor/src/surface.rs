// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    sync::{Arc, OnceLock},
    time::Duration,
};

use gpui::{
    AnyElement, App, Bounds, Context, DispatchPhase, Div, Element, ElementId, ElementInputHandler,
    Entity, FocusHandle, Focusable, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, ScrollWheelEvent,
    SharedString, Task, TextRun, Window, div, point, prelude::*, px, rgb,
};
use oxideterm_editor_core::{
    BufferOffset, Cursor, EditTransaction, FindMatches, LineCol, Selection, TextBuffer, TextEdit,
    TextRange,
};
use oxideterm_editor_syntax::{
    BracketIndex, BracketPair, HighlightCache, LanguageId, StructureCache, SyntaxEdit,
    SyntaxSession,
};
use oxideterm_theme::ThemeTokens;

use crate::{
    EditorAppearance, EditorMetrics, EditorSettings, EditorViewport, metrics::editor_code_font,
};

mod commands;
mod coords;
mod fold;
mod input;
mod render;
mod scroll;
mod search;
mod search_bar;
mod segments;
pub use scroll::{EditorScrollAnchor, EditorScrollOrigin, EditorViewportChanged};
use search_bar::EditorSearchBar;
pub use search_bar::EditorSearchLabels;
mod syntax_task;
mod wrap;

pub use commands::{EditorCommand, EditorKeybindings, EditorShortcut};
use coords::{byte_column_for_visual_column, visual_column_for_byte_column};
use wrap::DisplayRow;

pub type SaveCallback =
    Box<dyn FnMut(Arc<str>, &mut Window, &mut Context<TextEditorView>) -> Result<(), String>>;
pub type SharedSaveCallback = Box<
    dyn FnMut(
        oxideterm_editor_core::SharedText,
        &mut Window,
        &mut Context<TextEditorView>,
    ) -> Result<(), String>,
>;
pub type ModifiedWordClickCallback =
    Box<dyn FnMut(String, &mut Window, &mut Context<TextEditorView>) -> Result<(), String>>;

pub const LARGE_FILE_THRESHOLD: usize = 10 * 1024 * 1024;

fn normalize_editor_text(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    }
}

const EDITOR_CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);
const EDITOR_SELECTION_SCROLL_INTERVAL: Duration = Duration::from_millis(30);

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
        let drag_view = self.view.downgrade();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture
                && let Some(view) = drag_view.upgrade()
                && view.read(cx).selection_drag.is_some()
            {
                view.update(cx, |this, cx| {
                    if event.pressed_button == Some(MouseButton::Left) {
                        this.drag_selection_to_point(event.position, window, cx);
                    } else {
                        this.finish_selection_drag(cx);
                    }
                });
            }
        });
        let drag_view = self.view.downgrade();
        // Drag selection belongs to this editor until release, even outside its viewport.
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
            if phase == DispatchPhase::Capture
                && event.button == MouseButton::Left
                && let Some(view) = drag_view.upgrade()
                && view.read(cx).selection_drag.is_some()
            {
                view.update(cx, |this, cx| {
                    this.finish_selection_drag(cx);
                });
            }
        });
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

#[derive(Clone, Debug, PartialEq)]
struct DisplayRowsCache {
    buffer_version: u64,
    wrap_width: Option<f32>,
    fold_revision: u64,
    max_width_columns: usize,
    rows: Arc<wrap::DisplayRows>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct HighlightChunkCacheKey {
    pub buffer_version: u64,
    pub line: usize,
    pub range_start: usize,
    pub range_end: usize,
}

#[derive(Clone, Debug)]
pub(super) struct LineChunkSpec {
    pub start: usize,
    pub end: usize,
    pub color: u32,
    pub text: gpui::SharedString,
    pub layout: OnceLock<Arc<gpui::LineLayout>>,
}

#[derive(Clone, Debug, Default)]
struct HighlightChunkCache {
    entries: HashMap<HighlightChunkCacheKey, CachedLineChunks>,
    recency: BTreeMap<u64, HighlightChunkCacheKey>,
    clock: u64,
    frame_start: u64,
    retained_bytes: usize,
    font_generation: usize,
}

#[derive(Clone, Debug)]
struct CachedLineChunks {
    chunks: Arc<Vec<LineChunkSpec>>,
    bytes: usize,
    last_used: u64,
}

impl CachedLineChunks {
    fn estimate_bytes(chunks: &Arc<Vec<LineChunkSpec>>) -> usize {
        let metadata = std::mem::size_of::<(HighlightChunkCacheKey, Self)>()
            + std::mem::size_of::<(u64, HighlightChunkCacheKey)>()
            + std::mem::size_of::<Vec<LineChunkSpec>>()
            + 2 * std::mem::size_of::<usize>();
        metadata
            + chunks.capacity() * std::mem::size_of::<LineChunkSpec>()
            + chunks
                .iter()
                .map(|chunk| {
                    // Charge shared layouts to each retaining row conservatively;
                    // allocator bookkeeping is not part of this payload estimate.
                    chunk.text.len()
                        + chunk.layout.get().map_or(0, |layout| {
                            std::mem::size_of::<gpui::LineLayout>()
                                + 2 * std::mem::size_of::<usize>()
                                + layout.runs.capacity() * std::mem::size_of::<gpui::ShapedRun>()
                                + layout
                                    .runs
                                    .iter()
                                    .map(|run| {
                                        run.glyphs.capacity()
                                            * std::mem::size_of::<gpui::ShapedGlyph>()
                                    })
                                    .sum::<usize>()
                        })
                })
                .sum::<usize>()
    }
}

impl HighlightChunkCache {
    // Bound retained text and native layouts, while keeping the current working
    // set pinned until it leaves the rendered window to avoid redraw churn.
    const BYTE_BUDGET: usize = 1024 * 1024;

    fn begin_frame(&mut self) {
        self.frame_start = self.clock;
    }

    fn get(&mut self, key: &HighlightChunkCacheKey) -> Option<Arc<Vec<LineChunkSpec>>> {
        let entry = self.entries.get_mut(key)?;
        self.recency.remove(&entry.last_used);
        self.clock += 1;
        entry.last_used = self.clock;
        self.recency.insert(entry.last_used, *key);
        Some(entry.chunks.clone())
    }

    fn insert(
        &mut self,
        key: HighlightChunkCacheKey,
        chunks: Arc<Vec<LineChunkSpec>>,
    ) -> Arc<Vec<LineChunkSpec>> {
        if let Some(previous) = self.entries.remove(&key) {
            self.recency.remove(&previous.last_used);
            self.retained_bytes -= previous.bytes;
        }
        self.clock += 1;
        let bytes = CachedLineChunks::estimate_bytes(&chunks);
        self.retained_bytes += bytes;
        self.entries.insert(
            key,
            CachedLineChunks {
                chunks: chunks.clone(),
                bytes,
                last_used: self.clock,
            },
        );
        self.recency.insert(self.clock, key);
        chunks
    }

    fn finish_frame(&mut self) {
        // Glyph layouts are filled lazily while building rows, after insertion.
        for (_, key) in self.recency.range(self.frame_start + 1..) {
            let entry = self
                .entries
                .get_mut(key)
                .expect("cached row owns recency entry");
            let bytes = CachedLineChunks::estimate_bytes(&entry.chunks);
            self.retained_bytes = self.retained_bytes - entry.bytes + bytes;
            entry.bytes = bytes;
        }
        while self.retained_bytes > Self::BYTE_BUDGET {
            let Some((&last_used, &key)) = self.recency.first_key_value() else {
                break;
            };
            if last_used > self.frame_start {
                break;
            }
            self.recency.pop_first();
            let entry = self
                .entries
                .remove(&key)
                .expect("recency entry owns cached row");
            self.retained_bytes -= entry.bytes;
        }
        if self.entries.capacity() > self.entries.len().saturating_mul(4) {
            self.entries.shrink_to(self.entries.len().saturating_mul(2));
        }
    }

    fn clear(&mut self) {
        self.entries = HashMap::new();
        self.recency.clear();
        self.clock = 0;
        self.frame_start = 0;
        self.retained_bytes = 0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SelectionDrag {
    anchor: BufferOffset,
    position: Option<Point<Pixels>>,
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
    scroll_origin: EditorScrollOrigin,
    last_published_scroll: Option<(u64, f32, f32, f32)>,
    pending_layout_anchor: Option<EditorScrollAnchor>,
    last_revealed_caret: Option<(u64, BufferOffset)>,
    metrics: EditorMetrics,
    configured_line_height: f32,
    text_system: Arc<gpui::TextSystem>,
    appearance: EditorAppearance,
    read_only: bool,
    preserve_line_endings: bool,
    on_save: Option<SharedSaveCallback>,
    on_modified_word_click: Option<ModifiedWordClickCallback>,
    save_status: EditorSaveStatus,
    language: Option<LanguageId>,
    language_path: Option<std::path::PathBuf>,
    language_fallback: Option<LanguageId>,
    plugin_grammar: Option<Arc<oxideterm_editor_syntax::PluginGrammar>>,
    _language_subscription: gpui::Subscription,
    language_notice_tokens: ThemeTokens,
    syntax: Option<SyntaxSession>,
    syntax_version: Option<u64>,
    syntax_generation: Arc<std::sync::atomic::AtomicU64>,
    syntax_executor: gpui::BackgroundExecutor,
    syntax_task: Option<Task<()>>,
    render_cache_active: bool,
    pending_syntax: Option<syntax_task::SyntaxRequest>,
    highlight_spans: HighlightCache,
    structure_cache: StructureCache,
    bracket_index: BracketIndex,
    content_bounds: Option<Bounds<Pixels>>,
    marked_text: Option<MarkedText>,
    secondary_selections: Vec<Selection>,
    settings: EditorSettings,
    find_query: String,
    find_regex: bool,
    find_regex_error: bool,
    find_pattern_cache: RefCell<Option<search::FindPatternCache>>,
    find_matches: FindMatches,
    find_work: search::FindWork,
    active_find_index: Option<usize>,
    search_labels: Option<EditorSearchLabels>,
    search_bar: Option<EditorSearchBar>,
    folded_ranges: Vec<FoldRange>,
    fold_revision: u64,
    display_rows_cache: RefCell<Option<DisplayRowsCache>>,
    highlight_chunk_cache: RefCell<HighlightChunkCache>,
    segment_cache: RefCell<segments::SegmentCache>,
    wrap_measurements: RefCell<wrap::WrapMeasurements>,
    selection_drag: Option<SelectionDrag>,
    selection_scroll_task: Option<Task<()>>,
    transparent_background: bool,
    presentation: EditorPresentation,
    border_visible: bool,
    context_menu: Option<EditorContextMenu>,
    context_menu_labels: EditorContextMenuLabels,
    caret_visible: bool,
    caret_blink_focused: bool,
    caret_blink_generation: u64,
    caret_blink_task: Option<Task<()>>,
}

impl TextEditorView {
    pub fn new(text: impl Into<Arc<str>>, tokens: &ThemeTokens, cx: &mut Context<Self>) -> Self {
        Self::new_with_line_endings(text, false, tokens, cx)
    }

    /// Text transformations must preserve pasted CR/LF bytes instead of normalizing them.
    pub fn new_verbatim(
        text: impl Into<Arc<str>>,
        tokens: &ThemeTokens,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_line_endings(text, true, tokens, cx)
    }

    fn new_with_line_endings(
        text: impl Into<Arc<str>>,
        preserve_line_endings: bool,
        tokens: &ThemeTokens,
        cx: &mut Context<Self>,
    ) -> Self {
        let text: Arc<str> = text.into();
        let text = if !preserve_line_endings && text.contains('\r') {
            normalize_editor_text(text.to_string()).into()
        } else {
            text
        };
        let buffer = TextBuffer::new(text);
        Self::from_buffer(buffer, preserve_line_endings, tokens, cx)
    }

    pub fn from_shared_text(
        text: oxideterm_editor_core::SharedText,
        tokens: &ThemeTokens,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = if text.chunks().any(|chunk| chunk.contains('\r')) {
            TextBuffer::new(text.with_text(|text| normalize_editor_text(text.to_owned())))
        } else {
            TextBuffer::from_shared_text(text)
        };
        Self::from_buffer(buffer, false, tokens, cx)
    }

    fn from_buffer(
        buffer: TextBuffer,
        preserve_line_endings: bool,
        tokens: &ThemeTokens,
        cx: &mut Context<Self>,
    ) -> Self {
        let metrics = EditorMetrics::from_theme(tokens);
        let settings = EditorSettings::default();
        let language_subscription =
            cx.observe_global::<crate::EditorLanguagePlugins>(|this, cx| {
                let detected = this.language_path.as_deref().map(|path| {
                    this.buffer
                        .with_line_text(0, |source| crate::detect_language(Some(path), source, cx))
                        .flatten()
                        .or_else(|| this.language_fallback.clone())
                });
                let language_changed = detected
                    .as_ref()
                    .is_some_and(|language| *language != this.language);
                if let Some(language) = detected {
                    this.language = language;
                }
                let next = this.language.as_ref().and_then(|id| {
                    cx.try_global::<crate::EditorLanguagePlugins>()
                        .and_then(|plugins| plugins.grammars.get(id))
                        .cloned()
                });
                let changed = match (&this.plugin_grammar, &next) {
                    (Some(old), Some(new)) => !Arc::ptr_eq(old, new),
                    (None, None) => false,
                    _ => true,
                };
                if language_changed || changed {
                    this.request_syntax(None, true, cx);
                    this.refresh_foldable_ranges();
                }
                cx.notify();
            });
        Self {
            buffer,
            cursor: Cursor::new(BufferOffset::ZERO),
            focus_handle: cx.focus_handle(),
            viewport: EditorViewport::new(metrics.overscan_rows),
            scroll_origin: EditorScrollOrigin::Layout,
            last_published_scroll: None,
            pending_layout_anchor: None,
            last_revealed_caret: None,
            configured_line_height: metrics.line_height,
            text_system: cx.text_system().clone(),
            metrics,
            appearance: EditorAppearance::from_theme(tokens),
            read_only: false,
            preserve_line_endings,
            on_save: None,
            on_modified_word_click: None,
            save_status: EditorSaveStatus::Clean,
            syntax: None,
            syntax_version: None,
            syntax_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            syntax_executor: cx.background_executor().clone(),
            syntax_task: None,
            render_cache_active: true,
            pending_syntax: None,
            language: None,
            language_path: None,
            language_fallback: None,
            plugin_grammar: None,
            _language_subscription: language_subscription,
            language_notice_tokens: *tokens,
            highlight_spans: HighlightCache::default(),
            structure_cache: StructureCache::default(),
            bracket_index: BracketIndex::default(),
            content_bounds: None,
            marked_text: None,
            secondary_selections: Vec::new(),
            settings,
            find_query: String::new(),
            find_regex: false,
            find_regex_error: false,
            find_pattern_cache: RefCell::new(None),
            find_matches: FindMatches::default(),
            find_work: search::FindWork::default(),
            active_find_index: None,
            search_labels: None,
            search_bar: None,
            folded_ranges: Vec::new(),
            fold_revision: 0,
            display_rows_cache: RefCell::new(None),
            highlight_chunk_cache: RefCell::new(HighlightChunkCache::default()),
            segment_cache: RefCell::new(segments::SegmentCache::default()),
            wrap_measurements: RefCell::new(wrap::WrapMeasurements::default()),
            selection_drag: None,
            selection_scroll_task: None,
            transparent_background: false,
            presentation: EditorPresentation::Document,
            border_visible: true,
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
            self.finish_selection_drag(cx);
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
        // Use the owning GPUI scheduler so tests can control time and local task wakeups.
        let executor = cx.background_executor().clone();
        self.caret_blink_task = Some(cx.spawn(async move |editor, cx| {
            loop {
                executor.timer(EDITOR_CARET_BLINK_INTERVAL).await;
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
        let text = if self.preserve_line_endings {
            text
        } else {
            normalize_editor_text(text)
        };
        if self.buffer.text() == text {
            return;
        }
        let range = TextRange::new(BufferOffset::ZERO, BufferOffset(self.buffer.len()));
        if self
            .buffer
            .apply_transaction(EditTransaction::single(TextEdit::new(range, text)))
            .is_ok()
        {
            self.finish_selection_drag(cx);
            self.cursor
                .set_selection(Selection::caret(BufferOffset::ZERO));
            self.secondary_selections.clear();
            self.marked_text = None;
            self.save_status = EditorSaveStatus::Dirty;
            self.request_syntax(None, true, cx);
            self.clear_folds_after_buffer_change();
            self.refresh_find_matches(cx);
            if self.render_cache_active {
                self.viewport
                    .clamp(self.document_row_count(), self.metrics.line_height);
            }
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

    pub fn set_on_save(&mut self, mut on_save: SaveCallback) {
        self.on_save = Some(Box::new(move |text, window, cx| {
            on_save(text.into_contiguous(), window, cx)
        }));
    }

    pub fn set_on_save_shared(&mut self, on_save: SharedSaveCallback) {
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
        self.viewport.scroll_past_end = presentation == EditorPresentation::Document;
        self.display_rows_cache.borrow_mut().take();
        self.viewport
            .clamp(self.document_row_count(), self.metrics.line_height);
        cx.notify();
    }

    pub fn set_border_visible(&mut self, visible: bool) {
        self.border_visible = visible;
    }

    pub fn set_transparent_background(
        &mut self,
        transparent_background: bool,
        cx: &mut Context<Self>,
    ) {
        if self.transparent_background == transparent_background {
            return;
        }
        // The owning surface decides whether the editor participates in its
        // background material; editor state and text rendering stay unchanged.
        self.transparent_background = transparent_background;
        cx.notify();
    }

    /// Returns the measured line box used by compact editor hosts.
    pub fn line_height(&self) -> f32 {
        self.metrics.line_height
    }

    pub fn set_placeholder(&mut self, placeholder: Option<String>, cx: &mut Context<Self>) {
        if self.settings.placeholder == placeholder {
            return;
        }
        self.settings.placeholder = placeholder;
        cx.notify();
    }

    pub fn set_settings(&mut self, settings: EditorSettings, cx: &mut Context<Self>) {
        let tab_changed = self.settings.tab_size != settings.tab_size;
        self.settings = settings;
        if tab_changed {
            self.request_syntax(None, false, cx);
        } else if self.render_cache_active && self.syntax_task.is_none() {
            self.refresh_foldable_ranges();
        }
        if self.render_cache_active {
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
        }
        self.refresh_find_matches(cx);
        cx.notify();
    }

    pub fn apply_ide_runtime_settings(
        &mut self,
        tokens: &ThemeTokens,
        font_family: String,
        font_weight: f32,
        font_fallback_family: Option<String>,
        font_size: f32,
        line_height: f32,
        word_wrap: bool,
        background_active: bool,
        cx: &mut Context<Self>,
    ) {
        self.apply_runtime_settings_with_fallback(
            tokens,
            font_family,
            font_fallback_family,
            font_size,
            line_height,
            word_wrap,
            background_active,
            cx,
        );
        self.appearance.font_weight = font_weight.clamp(100.0, 900.0);
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
        self.apply_runtime_settings_with_fallback(
            tokens,
            font_family,
            None,
            font_size,
            line_height,
            word_wrap,
            background_active,
            cx,
        );
    }

    fn apply_runtime_settings_with_fallback(
        &mut self,
        tokens: &ThemeTokens,
        font_family: String,
        font_fallback_family: Option<String>,
        font_size: f32,
        line_height: f32,
        word_wrap: bool,
        background_active: bool,
        cx: &mut Context<Self>,
    ) {
        if self.render_cache_active {
            self.pending_layout_anchor = Some(self.scroll_anchor());
        }
        self.appearance = EditorAppearance::from_theme(tokens);
        self.language_notice_tokens = *tokens;
        self.apply_search_theme(tokens, cx);
        // Embedded editors can follow the typography of their owning surface.
        self.appearance.font_family = font_family;
        self.appearance.font_fallback_family = font_fallback_family;
        self.metrics =
            EditorMetrics::from_theme_with_editor_typography(tokens, font_size, line_height);
        self.configured_line_height = self.metrics.line_height;
        self.display_rows_cache.borrow_mut().take();
        self.set_transparent_background(background_active, cx);
        self.highlight_chunk_cache.borrow_mut().clear();
        // Tauri wires Settings.ide.wordWrap into CodeMirror's lineWrapping
        // compartment. Keep that as editor settings, not a one-off render flag.
        self.settings.soft_wrap = word_wrap;
        if self.render_cache_active {
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
        }
        cx.notify();
    }

    pub fn set_language(&mut self, language: Option<LanguageId>, cx: &mut Context<Self>) {
        self.language_path = None;
        self.language_fallback = None;
        self.language = language;
        self.request_syntax(None, true, cx);
        self.refresh_foldable_ranges();
        cx.notify();
    }

    pub fn set_language_from_path(
        &mut self,
        path: impl AsRef<std::path::Path>,
        fallback: Option<LanguageId>,
        cx: &mut Context<Self>,
    ) {
        let path = path.as_ref().to_path_buf();
        let language = self
            .buffer
            .with_line_text(0, |source| crate::detect_language(Some(&path), source, cx))
            .flatten()
            .or_else(|| fallback.clone());
        self.set_language(language, cx);
        self.language_path = Some(path);
        self.language_fallback = fallback;
    }

    pub fn is_large_file(&self) -> bool {
        self.buffer.len() > LARGE_FILE_THRESHOLD
    }

    pub fn insert_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let text = text.into();
        let text = if self.preserve_line_endings {
            text
        } else {
            normalize_editor_text(text)
        };
        self.replace_all_selections_with_caret(text, cx);
    }

    /// Exposes undo to embedding surfaces without bypassing editor history bookkeeping.
    pub fn undo_external(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.undo(cx);
    }

    /// Exposes redo to embedding surfaces without bypassing editor history bookkeeping.
    pub fn redo_external(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.redo(cx);
    }

    /// Wraps the primary selection with Markdown-compatible delimiters.
    ///
    /// This public editing command keeps formatting toolbars on the same transaction, undo,
    /// syntax, and input-method path as keyboard edits instead of rebuilding the whole buffer.
    pub fn wrap_primary_selection_external(
        &mut self,
        prefix: &str,
        suffix: &str,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let selection = self.cursor.selection();
        let range = selection.range();
        let selected = self.buffer.slice(range).unwrap_or_default();
        let surrounding = range.start.0 >= prefix.len()
            && self
                .buffer
                .slice(TextRange::new(
                    BufferOffset(range.start.0 - prefix.len()),
                    range.start,
                ))
                .is_ok_and(|text| text == prefix)
            && self
                .buffer
                .slice(TextRange::new(
                    range.end,
                    BufferOffset(range.end.0 + suffix.len()),
                ))
                .is_ok_and(|text| text == suffix);
        let (range, replacement, selection_start) = if surrounding {
            (
                TextRange::new(
                    BufferOffset(range.start.0 - prefix.len()),
                    BufferOffset(range.end.0 + suffix.len()),
                ),
                selected.clone(),
                range.start.0 - prefix.len(),
            )
        } else {
            (
                range,
                wrapped_selection_text(&selected, prefix, suffix),
                range.start.0 + prefix.len(),
            )
        };
        let selection_end = selection_start + selected.len();
        self.replace_range_with_caret(range, replacement, cx);
        self.cursor.set_selection(if selected.is_empty() {
            Selection::caret(BufferOffset(selection_start))
        } else {
            Selection::new(BufferOffset(selection_start), BufferOffset(selection_end))
        });
        cx.notify();
    }

    /// Prefixes every selected line, or the caret line, using one undoable transaction.
    pub fn prefix_selected_lines_external(&mut self, prefix: &str, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let selection = self.cursor.selection();
        let range = selection.range();
        let first = self
            .buffer
            .offset_to_line_col(range.start)
            .expect("valid selection");
        let end = self
            .buffer
            .offset_to_line_col(range.end)
            .expect("valid selection");
        let last_line = if range.end > range.start && end.column == 0 {
            end.line - 1
        } else {
            end.line
        };
        let base = self.buffer.line_start_offset(first.line).unwrap().0;
        let end = self
            .buffer
            .line_start_offset(last_line + 1)
            .map_or(self.buffer.len(), |offset| offset.0);
        let text = self
            .buffer
            .slice(TextRange::new(BufferOffset(base), BufferOffset(end)))
            .expect("valid line range");
        let relative = Selection::new(
            BufferOffset(selection.anchor.0 - base),
            BufferOffset(selection.head.0 - base),
        );
        let (line_start, line_end, replacement, adjusted_selection) =
            prefixed_line_replacement(&text, relative, prefix);
        self.replace_range_with_caret(
            TextRange::new(
                BufferOffset(base + line_start),
                BufferOffset(base + line_end),
            ),
            replacement,
            cx,
        );
        // Keep Markdown markers outside the restored selection so the next
        // input replaces only the original content.
        self.cursor.set_selection(Selection::new(
            BufferOffset(base + adjusted_selection.anchor.0),
            BufferOffset(base + adjusted_selection.head.0),
        ));
        cx.notify();
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
        let row_edit = self.prepare_row_edit(range, &replacement);
        let caret = BufferOffset(range.start.0 + replacement.len());
        let syntax_edit = self
            .language
            .as_ref()
            .filter(|_| self.syntax_task.is_none() && !self.is_large_file())
            .and_then(|_| SyntaxEdit::from_buffer(&self.buffer, range, &replacement).ok());
        if self
            .buffer
            .apply_transaction(EditTransaction::single(TextEdit::new(range, replacement)))
            .is_ok()
        {
            self.request_syntax(syntax_edit, false, cx);
            self.cursor.set_selection(Selection::caret(caret));
            self.secondary_selections.clear();
            self.marked_text = None;
            self.save_status = EditorSaveStatus::Dirty;
            self.clear_folds_after_buffer_change();
            self.restore_rows_after_edit(row_edit);
            self.refresh_find_matches(cx);
            if self.render_cache_active {
                self.viewport
                    .clamp(self.document_row_count(), self.metrics.line_height);
            }
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
            self.request_syntax(None, true, cx);
            self.clear_folds_after_buffer_change();
            self.refresh_find_matches(cx);
            if self.render_cache_active {
                self.viewport
                    .clamp(self.document_row_count(), self.metrics.line_height);
            }
            cx.notify();
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut on_save) = self.on_save.take() else {
            self.save_status = EditorSaveStatus::Failed("save callback is not configured".into());
            cx.notify();
            return;
        };
        let result = on_save(self.buffer.shared_text(), window, cx);
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

    fn handle_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        self.scroll_origin = EditorScrollOrigin::User;
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
        self.scroll_origin = EditorScrollOrigin::User;
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
        let pending_layout = self.pending_layout_anchor.take();
        let anchor = pending_layout.or_else(|| {
            self.content_bounds
                .filter(|old| old.size != bounds.size)
                .map(|_| self.scroll_anchor())
        });
        self.content_bounds = Some(bounds);
        let width_changed = self.viewport.set_width(f32::from(bounds.size.width));
        let height_changed = self.viewport.set_height(f32::from(bounds.size.height));
        if width_changed || height_changed || pending_layout.is_some() {
            if let Some(anchor) = anchor {
                self.restore_scroll_anchor(anchor);
            }
            self.viewport
                .clamp_horizontal(self.max_horizontal_scroll_px());
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
        self.publish_viewport(cx);
    }

    fn measure_code_metrics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let font_generation = window.text_system().font_generation();
        {
            let mut cache = self.highlight_chunk_cache.borrow_mut();
            if cache.font_generation != font_generation {
                cache.clear();
                cache.font_generation = font_generation;
                self.display_rows_cache.borrow_mut().take();
            }
        }
        // GPUI snaps every row's explicit height before layout. Use that same
        // height for scrolling and hit testing to avoid drift at the final row.
        // Preserve the configured value when moving between display scales.
        let line_height = f32::from(window.pixel_snap(px(self.configured_line_height)));
        let line_height_changed = self.metrics.line_height != line_height;
        self.metrics.line_height = line_height;
        // CodeMirror measures actual font advances through the browser layout
        // engine. GPUI needs the same explicit measurement; the old 0.62 ratio
        // is only a startup fallback before the first render has a Window.
        if self.metrics.measure_code_cell_width(
            window,
            &self.appearance.font_family,
            self.appearance.font_fallback_family.as_deref(),
            self.appearance.font_weight,
        ) || line_height_changed
        {
            self.highlight_chunk_cache.borrow_mut().clear();
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
        if matches!(&*self.display_rows(), wrap::DisplayRows::Unwrapped(_))
            && let Some(segments) = self.line_segments(display_row.line)
        {
            let x = f32::from(point.x - self.content_bounds?.origin.x)
                - self.visible_gutter_width()
                - self.visible_content_padding_x()
                + self.viewport.scroll_x_px;
            let index = segments.at_x(x);
            let origin = segments.starts[index];
            let text = self.segment_text(display_row.line, origin.byte, segments.end(index));
            let byte = origin.byte
                + self.closest_grapheme_byte_for_x(&text, (x - origin.x).max(0.0), window);
            return self
                .buffer
                .line_col_to_offset(LineCol::new(display_row.line, byte))
                .ok();
        }
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
        let word = self.buffer.word_at(offset);
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
        self.selection_scroll_task = None;
        self.selection_drag = Some(SelectionDrag {
            anchor,
            position: None,
        });
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
        if !self.focus_handle.is_focused(window) {
            self.finish_selection_drag(cx);
            return;
        }
        let Some(drag) = self.selection_drag.as_mut() else {
            return;
        };
        drag.position = Some(point);
        if self.update_drag_selection(point, window) {
            cx.notify();
        }
        if self.selection_scroll_delta(point) == 0.0 {
            self.selection_scroll_task = None;
        } else if self.selection_scroll_task.is_none() {
            let executor = cx.background_executor().clone();
            // The editor owns this loop; release, blur, or dropping the view cancels it.
            self.selection_scroll_task = Some(cx.spawn_in(window, async move |editor, cx| {
                loop {
                    executor.timer(EDITOR_SELECTION_SCROLL_INTERVAL).await;
                    let keep_scrolling = editor
                        .update_in(cx, |this, window, cx| {
                            if !window.is_window_active() || !this.focus_handle.is_focused(window) {
                                this.finish_selection_drag(cx);
                                return false;
                            }
                            if this.scroll_drag_selection(window, cx) {
                                return true;
                            }
                            this.selection_scroll_task = None;
                            false
                        })
                        .unwrap_or(false);
                    if !keep_scrolling {
                        break;
                    }
                }
            }));
        }
    }

    fn update_drag_selection(&mut self, point: Point<Pixels>, window: &mut Window) -> bool {
        let Some(drag) = self.selection_drag else {
            return false;
        };
        let Some(bounds) = self.content_bounds else {
            return false;
        };
        let y = f32::from(point.y - bounds.top())
            .clamp(0.0, (f32::from(bounds.size.height) - 1.0).max(0.0));
        let point = gpui::point(point.x, bounds.top() + px(y));
        let head = self
            .offset_for_window_point(point, window)
            .unwrap_or(BufferOffset(self.buffer.len()));
        let selection = Selection::new(drag.anchor, head);
        if selection == self.cursor.selection() {
            return false;
        }
        self.cursor.set_selection(selection);
        true
    }

    fn selection_scroll_delta(&self, point: Point<Pixels>) -> f32 {
        let Some(bounds) = self.content_bounds else {
            return 0.0;
        };
        let height = f32::from(bounds.size.height);
        if height <= 0.0 {
            return 0.0;
        }
        let edge = self.metrics.line_height.min(height / 4.0).max(1.0);
        let y = f32::from(point.y - bounds.top());
        let distance = if y < edge {
            y - edge
        } else if y > height - edge {
            y - (height - edge)
        } else {
            return 0.0;
        };
        (distance / edge).clamp(-3.0, 3.0) * self.metrics.line_height
    }

    fn scroll_drag_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(point) = self.selection_drag.and_then(|drag| drag.position) else {
            return false;
        };
        let dy = self.selection_scroll_delta(point);
        let scrolled = self.viewport.scroll_by(
            0.0,
            dy,
            self.max_horizontal_scroll_px(),
            self.document_row_count(),
            self.metrics.line_height,
        );
        if scrolled {
            self.scroll_origin = EditorScrollOrigin::User;
            self.update_drag_selection(point, window);
            cx.notify();
        }
        scrolled
    }

    fn finish_selection_drag(&mut self, cx: &mut Context<Self>) {
        self.selection_scroll_task = None;
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
        if matches!(&*self.display_rows(), wrap::DisplayRows::Unwrapped(_))
            && let Some(segments) = self.line_segments(position.line)
        {
            let origin = segments.starts[segments.at_byte(position.column)];
            let prefix = self.segment_text(position.line, origin.byte, position.column);
            let x = origin.x
                + f32::from(
                    self.shape_coordinate_line(&prefix, window.text_system())
                        .width(),
                );
            return Bounds {
                origin: bounds.origin
                    + point(
                        px(
                            self.visible_gutter_width() + self.visible_content_padding_x()
                                - self.viewport.scroll_x_px
                                + x,
                        ),
                        px(position.line as f32 * self.metrics.line_height
                            - self.vertical_scroll_y_px()),
                    ),
                size: gpui::size(px(1.0), px(self.metrics.line_height)),
            };
        }
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
        let caret_x = f32::from(
            self.shape_coordinate_line(segment_text, window.text_system())
                .width(),
        );
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

    fn shape_coordinate_line(
        &self,
        text: &str,
        text_system: &gpui::WindowTextSystem,
    ) -> gpui::ShapedLine {
        let text = SharedString::from(text.to_string());
        let run = TextRun {
            len: text.len(),
            font: editor_code_font(
                &self.appearance.font_family,
                self.appearance.font_fallback_family.as_deref(),
                self.appearance.font_weight,
            ),
            color: rgb(self.appearance.text_hex).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
            letter_spacing: None,
        };
        text_system.shape_line(text, px(self.metrics.font_size), &[run], None)
    }

    fn closest_grapheme_byte_for_x(&self, text: &str, x: f32, window: &mut Window) -> usize {
        use unicode_segmentation::UnicodeSegmentation;

        let shaped = self.shape_coordinate_line(text, window.text_system());
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
        self.bracket_index.pair_at(head).cloned()
    }
}

impl Focusable for TextEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn wrapped_selection_text(selected: &str, prefix: &str, suffix: &str) -> String {
    format!("{prefix}{selected}{suffix}")
}

fn prefixed_line_replacement(
    text: &str,
    selection: Selection,
    prefix: &str,
) -> (usize, usize, String, Selection) {
    let selected_range = selection.range();
    let selection_start = selected_range.start.0;
    let selection_end = selected_range.end.0;
    let line_start = text[..selection_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let last_selected = if selection_end > selection_start
        && text.as_bytes().get(selection_end - 1) == Some(&b'\n')
    {
        selection_end - 1
    } else {
        selection_end
    };
    let line_end = text[last_selected..]
        .find('\n')
        .map_or(text.len(), |index| last_selected + index);
    let lines: Vec<_> = text[line_start..line_end].split('\n').collect();
    let toggle_off = !prefix.starts_with('#')
        && lines
            .iter()
            .all(|line| line.trim_start().starts_with(prefix));
    let mut replacements = Vec::new();
    let mut position = line_start;
    let replacement = lines
        .into_iter()
        .map(|line| {
            let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
            let body = &line[indent..];
            let removed = if prefix.starts_with('#') {
                let hashes = body.bytes().take_while(|byte| *byte == b'#').count();
                if (1..=6).contains(&hashes) && body.as_bytes().get(hashes) == Some(&b' ') {
                    hashes + 1
                } else {
                    0
                }
            } else if prefix == "> " {
                if body.starts_with("> ") { 2 } else { 0 }
            } else {
                ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ "]
                    .into_iter()
                    .find(|marker| body.starts_with(marker))
                    .map(str::len)
                    .unwrap_or_else(|| {
                        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
                        if digits > 0 && body.get(digits..digits + 2) == Some(". ") {
                            digits + 2
                        } else {
                            0
                        }
                    })
            };
            let marker = if toggle_off { "" } else { prefix };
            replacements.push((position + indent, position + indent + removed, marker.len()));
            position += line.len() + 1;
            format!("{}{marker}{}", &line[..indent], &body[removed..])
        })
        .collect::<Vec<_>>()
        .join("\n");
    let adjusted_offset = |offset: BufferOffset| {
        let mut shift = 0isize;
        for &(start, end, added) in &replacements {
            if offset.0 < start {
                break;
            }
            if offset.0 <= end {
                return BufferOffset(start.saturating_add_signed(shift) + added);
            }
            shift += added as isize - (end - start) as isize;
        }
        BufferOffset(offset.0.saturating_add_signed(shift))
    };
    let adjusted_selection = Selection::new(
        adjusted_offset(selection.anchor),
        adjusted_offset(selection.head),
    );
    (line_start, line_end, replacement, adjusted_selection)
}

fn colored_text(text: &str, color: u32) -> Div {
    div().text_color(rgb(color)).child(text.to_string())
}

#[cfg(test)]
mod tests {
    use gpui::AppContext;
    use std::sync::Arc;

    use super::{
        HighlightChunkCache, HighlightChunkCacheKey, LineChunkSpec, prefixed_line_replacement,
    };
    use oxideterm_editor_core::{BufferOffset, Selection};

    #[gpui::test]
    fn long_input_keeps_caret_visible_without_overriding_manual_horizontal_scroll(
        cx: &mut gpui::TestAppContext,
    ) {
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let editor = super::TextEditorView::new("", &oxideterm_theme::default_tokens(), cx);
            window.focus(&editor.focus_handle, cx);
            editor
        });
        cx.simulate_resize(gpui::size(gpui::px(420.0), gpui::px(126.0)));
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        let text = format!("echo {}", "1234567890".repeat(30));
        cx.simulate_input(&text);
        cx.update(|window, app| {
            window.draw(app).clear(app);
            editor.update(app, |editor, _| {
                let bounds = editor.content_bounds.unwrap();
                let caret =
                    editor.bounds_for_byte_offset(editor.cursor.selection().head, bounds, window);
                assert!(caret.right() <= bounds.right(), "typed caret is clipped");
                assert!(caret.left() >= bounds.left() + gpui::px(editor.visible_gutter_width()));
                assert_eq!(editor.buffer.text(), text);
            });
        });
        let track_start = editor.read_with(cx, |editor, _| {
            let bounds = editor.content_bounds.unwrap();
            gpui::point(
                bounds.left() + gpui::px(editor.visible_gutter_width() + 1.0),
                bounds.bottom() - gpui::px(5.0),
            )
        });
        cx.simulate_click(track_start, gpui::Modifiers::default());
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.viewport.scroll_x_px, 0.0);
            assert_eq!(editor.cursor.selection().head, BufferOffset(text.len()));
        });
        cx.simulate_input("x");
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        editor.read_with(cx, |editor, _| assert!(editor.viewport.scroll_x_px > 0.0));
        cx.simulate_input("\n");
        cx.update(|window, app| {
            window.draw(app).clear(app);
        });
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.viewport.scroll_x_px, 0.0);
            assert_eq!(editor.buffer.text(), format!("{text}x\n"));
        });
    }

    #[gpui::test]
    fn fractional_line_height_allows_the_final_row_at_the_viewport_top(
        cx: &mut gpui::TestAppContext,
    ) {
        let (editor, cx) = cx.add_window_view(|_, cx| {
            super::TextEditorView::new(
                vec!["Last row must remain visible"; 201].join("\n"),
                &oxideterm_theme::default_tokens(),
                cx,
            )
        });
        cx.update(|window, app| {
            editor.update(app, |editor, cx| {
                editor.apply_runtime_settings(
                    &oxideterm_theme::default_tokens(),
                    "monospace".into(),
                    14.0,
                    1.2,
                    false,
                    false,
                    cx,
                );
                for (scale, row_height, scroll_end) in [
                    (1.0, 17.0, 3400.0),
                    (2.0, 17.0, 3400.0),
                    (1.25, 16.8, 3360.0),
                ] {
                    window.set_scale_factor(scale);
                    editor.measure_code_metrics(window, cx);
                    assert!((editor.metrics.line_height - row_height).abs() < 0.001);
                    editor.viewport.set_height(180.0);
                    editor
                        .viewport
                        .scroll_by(0.0, 10000.0, 0.0, 201, editor.metrics.line_height);
                    assert!((editor.viewport.scroll_y_px - scroll_end).abs() < 0.001);
                }
            });
        });
    }

    #[gpui::test]
    fn editor_caret_blink_uses_scheduled_time_and_stops_when_released(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::{EDITOR_CARET_BLINK_INTERVAL, TextEditorView};
        let editor =
            cx.new(|cx| TextEditorView::new("text", &oxideterm_theme::default_tokens(), cx));
        editor.update(cx, |editor, cx| editor.sync_caret_blink_focus(true, cx));
        cx.run_until_parked();
        cx.executor().advance_clock(EDITOR_CARET_BLINK_INTERVAL);
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| assert!(!editor.caret_visible));
        editor.update(cx, |editor, cx| editor.sync_caret_blink_focus(false, cx));
        cx.executor().advance_clock(EDITOR_CARET_BLINK_INTERVAL * 2);
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(editor.caret_visible);
            assert!(editor.caret_blink_task.is_none());
        });
        editor.update(cx, |editor, cx| editor.sync_caret_blink_focus(true, cx));
        cx.run_until_parked();
        let weak = editor.downgrade();
        drop(editor);
        cx.update(|_| {});
        cx.executor().advance_clock(EDITOR_CARET_BLINK_INTERVAL * 2);
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
    }

    #[gpui::test]
    fn language_plugin_install_update_disable_preserves_document_history(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::{LanguageId, TextEditorView};
        use oxideterm_editor_syntax::{PluginGrammarSource, SyntaxScope};
        use sha2::{Digest, Sha256};
        let directory = tempfile::tempdir().unwrap();
        zip::ZipArchive::new(std::io::Cursor::new(include_bytes!(
            "../../oxideterm-editor-syntax/tests/fixtures/elixir.zip"
        )))
        .unwrap()
        .extract(directory.path())
        .unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.path().join("plugin.json")).unwrap())
                .unwrap();
        let declared = &manifest["contributes"]["language"];
        let language = LanguageId::from_plugin_key("custom-elixir").unwrap();
        let definition = crate::EditorLanguageDefinition {
            plugin_id: "com.example.custom-language".into(),
            definition: oxideterm_plugin_manifest::NativePluginLanguageDefinition {
                id: "custom-elixir".into(),
                display_name: Some("Custom Elixir".into()),
                grammar_name: Some("elixir".into()),
                extensions: vec!["custom.expr".into()],
                ..Default::default()
            },
        };
        let mut grammar = PluginGrammarSource {
            language: language.clone(),
            grammar_name: "elixir".into(),
            parser: directory.path().join("parser.wasm"),
            highlights: directory.path().join("highlights.scm"),
            parser_sha256: declared["parserSha256"].as_str().unwrap().into(),
            highlights_sha256: declared["highlightsSha256"].as_str().unwrap().into(),
            injections: Vec::new(),
        };
        let source = "defmodule Demo do\n  def value, do: 42\nend\n";
        let editor =
            cx.new(|cx| TextEditorView::new(source, &oxideterm_theme::default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            editor.set_language_from_path("sample.custom.expr", None, cx);
            assert_eq!(editor.language, None);
            editor.insert_text("# edited\n", cx);
            assert!(editor.syntax.is_none());
        });
        cx.update(|cx| {
            crate::EditorLanguagePlugins::update_languages(Vec::new(), vec![definition.clone()], cx)
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.language, Some(language.clone()));
            assert!(editor.syntax.is_none());
            crate::EditorLanguagePlugins::set_labels(
                "Missing {{language}}".into(),
                "Failed".into(),
                "Manage".into(),
                "Dismiss".into(),
                cx,
            );
            assert!(
                crate::render_language_plugin_notice(
                    language.clone(),
                    &oxideterm_theme::default_tokens(),
                    cx
                )
                .is_some()
            );
        });
        cx.update(|cx| crate::EditorLanguagePlugins::update(vec![grammar.clone()], cx));
        cx.run_until_parked();
        editor.update(cx, |editor, _| {
            let syntax = editor
                .syntax
                .as_ref()
                .expect("installed grammar parsed the open document");
            let text = editor.buffer.text();
            assert!(
                syntax
                    .highlight_spans(&text)
                    .iter()
                    .any(|span| span.scope == SyntaxScope::Number
                        && &text[span.range.start.0..span.range.end.0] == "42")
            );
        });
        let query = "(integer) @string\n";
        std::fs::write(&grammar.highlights, query).unwrap();
        grammar.highlights_sha256 = format!("{:x}", Sha256::digest(query.as_bytes()));
        cx.update(|cx| crate::EditorLanguagePlugins::update(vec![grammar.clone()], cx));
        cx.run_until_parked();
        editor.update(cx, |editor, _| {
            let text = editor.buffer.text();
            let spans = editor.syntax.as_ref().unwrap().highlight_spans(&text);
            assert!(spans.iter().any(|span| span.scope == SyntaxScope::String
                && &text[span.range.start.0..span.range.end.0] == "42"));
            assert!(!spans.iter().any(|span| span.scope == SyntaxScope::Number));
        });
        cx.update(|cx| crate::EditorLanguagePlugins::update(Vec::new(), cx));
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert!(editor.syntax.is_none());
            assert_eq!(editor.language, Some(language.clone()));
            assert_eq!(editor.buffer.text(), format!("# edited\n{source}"));
            editor.undo(cx);
            assert_eq!(editor.buffer.text(), source);
        });
        cx.update(|cx| crate::EditorLanguagePlugins::update(vec![grammar], cx));
        cx.run_until_parked();
        editor.update(cx, |editor, _| {
            assert!(!editor.syntax.as_ref().unwrap().root_has_error());
            assert_eq!(editor.buffer.text(), source);
        });
        cx.update(|cx| crate::EditorLanguagePlugins::update_languages(Vec::new(), Vec::new(), cx));
        cx.run_until_parked();
        editor.update(cx, |editor, _| {
            assert_eq!(editor.language, None);
            assert!(editor.syntax.is_none());
            assert_eq!(editor.buffer.text(), source);
        });
    }

    #[gpui::test]
    fn folds_and_guides_follow_newlines_and_history(cx: &mut gpui::TestAppContext) {
        use super::{BufferOffset, LanguageId, Selection, TextEditorView};
        let source = "fn first() {\n    call();\n}\nfn second() {\n    call();\n}\n";
        let editor =
            cx.new(|cx| TextEditorView::new(source, &oxideterm_theme::default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            crate::grammar_fixture::install_rust(cx);
            editor.set_language(Some(LanguageId::Rust), cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            editor
                .cursor
                .set_selection(Selection::caret(BufferOffset(0)));
            editor.insert_text("// 🙂\n", cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.structure_cache.fold_lines().collect::<Vec<_>>(),
                [(1, 3), (4, 6)]
            );
            assert_eq!(editor.structure_cache.columns_for_line(2), [0]);
            assert_eq!(editor.structure_cache.columns_for_line(5), [0]);
            editor.undo(cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer.text(), source);
            assert_eq!(
                editor.structure_cache.fold_lines().collect::<Vec<_>>(),
                [(0, 2), (3, 5)]
            );
            editor.redo(cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.structure_cache.fold_lines().collect::<Vec<_>>(),
                [(1, 3), (4, 6)]
            );
            editor.set_language(None, cx);
            assert!(editor.structure_cache.fold_lines().next().is_none());
            assert!(editor.structure_cache.columns_for_line(2).is_empty());
        });
    }

    #[gpui::test]
    fn highlight_cache_tracks_typing_and_history(cx: &mut gpui::TestAppContext) {
        use super::{BufferOffset, LanguageId, Selection, TextEditorView, TextRange};
        use oxideterm_editor_syntax::SyntaxScope;
        let source = "fn first() { let value = foo; }\nfn second() {}\n";
        let start = source.find("foo").unwrap();
        let editor =
            cx.new(|cx| TextEditorView::new(source, &oxideterm_theme::default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            crate::grammar_fixture::install_rust(cx);
            editor.set_language(Some(LanguageId::Rust), cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            editor
                .cursor
                .set_selection(Selection::new(BufferOffset(start), BufferOffset(start + 3)));
            editor.insert_text("Foo", cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.buffer.text(),
                "fn first() { let value = Foo; }\nfn second() {}\n"
            );
            assert!(
                editor
                    .highlight_spans
                    .spans_in_range(start..start + 3)
                    .any(|span| span.range
                        == TextRange::new(BufferOffset(start), BufferOffset(start + 3))
                        && span.scope == SyntaxScope::Type)
            );
            editor.undo(cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer.text(), source);
            assert!(
                !editor
                    .highlight_spans
                    .spans_in_range(start..start + 3)
                    .any(|span| span.scope == SyntaxScope::Type)
            );
            editor.redo(cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert!(
                editor
                    .highlight_spans
                    .spans_in_range(start..start + 3)
                    .any(|span| span.scope == SyntaxScope::Type)
            );
            editor.set_language(None, cx);
            assert!(editor.highlight_spans.is_empty());
        });
    }

    #[gpui::test]
    fn disabling_syntax_and_search_clears_presented_metadata(cx: &mut gpui::TestAppContext) {
        use super::{BufferOffset, LanguageId, Selection, TextEditorView};
        let editor = cx.new(|cx| {
            TextEditorView::new(
                "fn main() {\n    let value = 1;\n}\n",
                &oxideterm_theme::default_tokens(),
                cx,
            )
        });
        editor.update(cx, |editor, cx| {
            crate::grammar_fixture::install_rust(cx);
            editor.set_language(Some(LanguageId::Rust), cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            editor.set_find_query("value", cx);
            assert!(
                editor
                    .highlight_spans
                    .spans_in_range(0..usize::MAX)
                    .any(|s| s.range.start.0 == 0 && s.range.end.0 == 2)
            );
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|m| (m.range.start.0, m.range.end.0))
                    .collect::<Vec<_>>(),
                [(20, 25)]
            );
            assert_eq!(editor.structure_cache.columns_for_line(1), [0]);
            editor.set_language(None, cx);
            editor.set_find_query("", cx);
            editor
                .cursor
                .set_selection(Selection::caret(BufferOffset(0)));
            editor.insert_text("z", cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, _cx| {
            assert!(editor.highlight_spans.is_empty());
            assert!(editor.bracket_index.is_empty());
            assert!(editor.structure_cache.columns_for_line(1).is_empty());
            assert!(editor.find_matches.is_empty());
            assert_eq!(editor.buffer.line_text(0).as_deref(), Some("zfn main() {"));
        });
    }

    #[gpui::test]
    fn selected_line_commands_keep_global_unicode_offsets_and_history(
        cx: &mut gpui::TestAppContext,
    ) {
        let editor = cx.new(|cx| {
            super::TextEditorView::new(
                "before\n中🙂\nafter\n",
                &oxideterm_theme::default_tokens(),
                cx,
            )
        });
        editor.update(cx, |editor, cx| {
            editor
                .cursor
                .set_selection(Selection::new(BufferOffset(15), BufferOffset(7)));
            editor.prefix_selected_lines_external("> ", cx);
            assert_eq!(editor.buffer.text(), "before\n> 中🙂\nafter\n");
            assert_eq!(
                editor.cursor.selection(),
                Selection::new(BufferOffset(17), BufferOffset(9))
            );
            editor.undo_external(cx);
            assert_eq!(editor.buffer.text(), "before\n中🙂\nafter\n");
            editor
                .cursor
                .set_selection(Selection::new(BufferOffset(7), BufferOffset(14)));
            editor.wrap_primary_selection_external("**", "**", cx);
            assert_eq!(editor.buffer.text(), "before\n**中🙂**\nafter\n");
            editor.wrap_primary_selection_external("**", "**", cx);
            assert_eq!(editor.buffer.text(), "before\n中🙂\nafter\n");
        });
    }

    #[test]
    fn line_prefix_preserves_content_selection_after_inserted_markers() {
        for (source, selection, prefix, expected_range, expected_text, expected_selection) in [
            (
                "alpha\nbeta\ngamma",
                Selection::new(BufferOffset(1), BufferOffset(9)),
                "- ",
                (0, 10),
                "- alpha\n- beta",
                Selection::new(BufferOffset(3), BufferOffset(13)),
            ),
            (
                "title",
                Selection::caret(BufferOffset::ZERO),
                "## ",
                (0, 5),
                "## title",
                Selection::caret(BufferOffset(3)),
            ),
        ] {
            let (start, end, replacement, adjusted_selection) =
                prefixed_line_replacement(source, selection, prefix);
            assert_eq!((start, end), expected_range, "{source}");
            assert_eq!(replacement, expected_text, "{source}");
            assert_eq!(adjusted_selection, expected_selection, "{source}");
        }
    }

    #[test]
    fn markdown_prefix_replaces_heading_and_excludes_next_line_boundary() {
        let source = "# Title\nbody";
        let (start, end, replacement, _) = prefixed_line_replacement(
            source,
            Selection::new(BufferOffset(0), BufferOffset(8)),
            "## ",
        );
        assert_eq!((start, end), (0, 7));
        assert_eq!(replacement, "## Title");
        assert_eq!(
            prefixed_line_replacement("## Title", Selection::caret(BufferOffset(8)), "## ").2,
            "## Title"
        );
        assert_eq!(
            prefixed_line_replacement(
                "- first\n- second",
                Selection::new(BufferOffset(0), BufferOffset(16)),
                "- "
            )
            .2,
            "first\nsecond"
        );
    }

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
            layout: Default::default(),
        }]);

        let inserted = cache.insert(key, chunks.clone());
        let cached = cache.get(&key).expect("highlight chunks should be cached");

        assert!(Arc::ptr_eq(&inserted, &cached));
        assert!(Arc::ptr_eq(&chunks, &cached));
    }

    #[test]
    fn highlight_chunk_cache_evicts_cold_rows_by_bytes_and_preserves_visible_layouts() {
        let mut cache = HighlightChunkCache::default();
        for line in 0..3 {
            cache.begin_frame();
            if line == 2 {
                cache.get(&cache_key(0)).unwrap();
            }
            let text = "x".repeat(HighlightChunkCache::BYTE_BUDGET / 3);
            cache.insert(
                cache_key(line),
                Arc::new(vec![LineChunkSpec {
                    start: 0,
                    end: text.len(),
                    color: 0xff00ff,
                    text: text.into(),
                    layout: Default::default(),
                }]),
            );
            cache.finish_frame();
        }
        assert!(cache.get(&cache_key(1)).is_none());
        let visible = cache.get(&cache_key(2)).unwrap();
        assert!(cache.retained_bytes <= HighlightChunkCache::BYTE_BUDGET);

        // Native layout allocations appear after cache insertion. Even an oversized
        // visible row must survive this frame, then become eligible once offscreen.
        visible[0]
            .layout
            .set(Arc::new(gpui::LineLayout {
                runs: vec![gpui::ShapedRun {
                    font_id: gpui::FontId(0),
                    glyphs: Vec::with_capacity(
                        HighlightChunkCache::BYTE_BUDGET / std::mem::size_of::<gpui::ShapedGlyph>()
                            + 1,
                    ),
                }],
                ..Default::default()
            }))
            .unwrap();
        cache.finish_frame();
        assert!(cache.retained_bytes > HighlightChunkCache::BYTE_BUDGET);
        assert!(Arc::ptr_eq(&visible, &cache.get(&cache_key(2)).unwrap()));
        cache.begin_frame();
        let recent = cache.get(&cache_key(0)).unwrap();
        cache.finish_frame();
        assert!(cache.get(&cache_key(2)).is_none());
        assert!(Arc::ptr_eq(&recent, &cache.get(&cache_key(0)).unwrap()));
        assert!(cache.retained_bytes <= HighlightChunkCache::BYTE_BUDGET);
    }
}

#[cfg(test)]
mod line_ending_tests {
    use gpui::AppContext;

    use super::*;
    #[gpui::test]
    fn clipboard_crlf_and_cr_become_single_newlines(cx: &mut gpui::TestAppContext) {
        let editor = cx.new(|cx| TextEditorView::new("", &oxideterm_theme::default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                "first\r\nsecond\rthird\n".into(),
            ));
            editor.paste_from_clipboard(cx);
            assert_eq!(editor.buffer().text(), "first\nsecond\nthird\n");
        });
        let verbatim = cx.new(|cx| {
            TextEditorView::new_verbatim("prefix\r\n", &oxideterm_theme::default_tokens(), cx)
        });
        verbatim.update(cx, |editor, cx| {
            editor.move_cursor_to_document_end(cx);
            editor.paste_from_clipboard(cx);
            assert_eq!(editor.buffer().text(), "prefix\r\nfirst\r\nsecond\rthird\n");
            editor.replace_text_external("result\r\n", cx);
            editor.set_read_only(true);
            editor.paste_from_clipboard(cx);
            assert_eq!(editor.buffer().text(), "result\r\n");
        });
    }
}

#[cfg(test)]
mod performance;
