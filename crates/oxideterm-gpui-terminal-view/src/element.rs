use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    hash::{BuildHasher, Hash, Hasher},
    ops::Range,
    rc::Rc,
    sync::{Arc, LazyLock, OnceLock, Weak},
    time::Duration,
};

use foldhash::{
    HashMap as FastHashMap, HashMapExt, HashSet as FastHashSet, HashSetExt,
    quality::{FoldHasher, RandomState as CacheKeyHasherState},
};
use gpui::{
    App, Bounds, ContentMask, CursorStyle, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, Instant, IntoElement, LayoutId, LineLayout, LinePaintCache, Pixels,
    SharedString, Style, TextRun, Window, fill, point, px, relative, rgb, rgba, size,
};
use oxideterm_terminal_emulator::{
    TerminalCell, TerminalCursorShape, TerminalSearchMatch, TerminalSnapshot,
};
use oxideterm_terminal_semantic::{
    CompiledSemanticScheme, SemanticLineRole, SemanticScheme, SemanticShellDialect,
    compiled_builtin_scheme,
};
use oxideterm_terminal_unicode::{TerminalVisualLine, visual_line_for_row_if_bidi};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use zeroize::Zeroizing;

use crate::highlight::{
    TerminalHighlightForeground, TerminalHighlightLayout, logical_line_overlaps_transient_output,
    terminal_highlights_for_rows,
};
use crate::links::*;
use crate::selection::TerminalSelection;
use crate::semantic::{append_terminal_semantics_with_palette, semantic_line_role_for_rows};
use crate::{
    CommandMarkAnnotation, TerminalElementInput, TerminalRenderedImage, TerminalRowTimestampStore,
    TransientCommandHighlight,
};
use crate::{metrics::*, theme::*};

mod cell_drawing;
mod frame;
mod rows;
use frame::{
    terminal_highlight_rules_signature, terminal_semantic_classification_signature,
    terminal_semantic_style_signature_from_palette, terminal_transient_command_highlight_signature,
};
use rows::{append_cached_row_layout, hash_row_content, map_rects_to_visual};
mod layout;
mod paint;

// Render cache key signatures stay process-local while all frames share one seed.
type CacheKeyHasher = FoldHasher<'static>;

static CACHE_KEY_HASHER_STATE: LazyLock<CacheKeyHasherState> =
    LazyLock::new(CacheKeyHasherState::default);

fn new_cache_key_hasher() -> CacheKeyHasher {
    CACHE_KEY_HASHER_STATE.build_hasher()
}

const TERMINAL_ROW_LAYOUT_CACHE_CAPACITY: usize = 512;
const TERMINAL_HIGHLIGHT_CACHE_CAPACITY: usize = 128;
const TERMINAL_SEMANTIC_SPAN_CACHE_BUDGET: usize = 65_536;
const TERMINAL_LINK_CACHE_CAPACITY: usize = 512;
const TERMINAL_TIMESTAMP_CACHE_CAPACITY: usize = 256;
const TRANSIENT_COMMAND_HIGHLIGHT_ALPHA: u32 = 0x52;

pub use crate::style::*;
pub use crate::style::{
    canvas_ghost_text_run as ghost_text_run, canvas_marked_text_run as marked_text_run,
    canvas_text_run_for_cell as text_run_for_cell, canvas_timestamp_text_run as timestamp_text_run,
};
pub use layout::*;
use paint::*;
pub use paint::{TerminalGhostTextSegment, ghost_text_grid_segments};

pub struct TerminalElement<H> {
    snapshot: TerminalSnapshot,
    rendered_images: Vec<TerminalRenderedImage>,
    selection: Option<TerminalSelection>,
    metrics: TerminalMetrics,
    theme: TerminalUiTheme,
    transition_colors: Option<TerminalTransitionColors>,
    dilation_settle_row: Option<usize>,
    cursor_visible: bool,
    marked_text: Option<String>,
    marked_text_caret_utf16: Option<usize>,
    ghost_text: Option<String>,
    search_query: Option<String>,
    search_matches: Arc<[TerminalSearchMatch]>,
    search_matches_precomputed: bool,
    selection_highlight_query: Option<Arc<Zeroizing<String>>>,
    selected_search_match: Option<usize>,
    command_marks: Arc<[CommandMarkAnnotation]>,
    selected_command_mark_id: Option<String>,
    hovered_command_mark_id: Option<String>,
    highlight_rules: Arc<[TerminalHighlightRule]>,
    highlight_rules_signature: u64,
    transient_command_highlight: Option<TransientCommandHighlight>,
    transient_command_highlight_signature: u64,
    semantic_coloring: bool,
    semantic_scheme: Option<Arc<CompiledSemanticScheme>>,
    semantic_shell: SemanticShellDialect,
    semantic_classification_signature: u64,
    semantic_style_signature: u64,
    hovered_link: Option<TerminalLinkRange>,
    detect_file_paths_as_links: bool,
    bidi_enabled: bool,
    input: Option<TerminalElementInput<H>>,
    transparent_background: bool,
    row_timestamps: Option<Arc<TerminalRowTimestampStore>>,
    layout_cache: Option<Rc<RefCell<TerminalLayoutCache>>>,
    performance_metrics_enabled: bool,
    viewport_rows: usize,
    scrollbar_display_offset: f32,
    scrollbar_visible: bool,
    scrollbar_active: bool,
    scroll_y_offset: Pixels,
    scroll_x_offset: Pixels,
    command_mark_gutter_width: f32,
    command_mark_decorations: bool,
    collapsed_commands: HashSet<String>,
    fold_rotations: HashMap<String, f32>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct TerminalElementLayout {
    pub backgrounds: Vec<TerminalRect>,
    pub highlight_backgrounds: Vec<TerminalRect>,
    pub highlight_underlines: Vec<TerminalRect>,
    pub highlight_outlines: Vec<TerminalRect>,
    pub search_matches: Vec<TerminalRect>,
    pub search_scrollbar_markers: Vec<(f32, bool)>,
    pub selection_matches: Vec<TerminalRect>,
    pub command_mark_overlays: Vec<TerminalCommandMarkOverlay>,
    pub selections: Vec<TerminalRect>,
    pub images: Vec<TerminalImageLayout>,
    pub text_runs: Vec<BatchedTextRun>,
    pub timestamp_runs: Vec<BatchedTextRun>,
    pub marked_text: Option<BatchedTextRun>,
    pub ghost_text: Option<BatchedTextRun>,
    pub ime_cursor_bounds: Option<Bounds<Pixels>>,
    pub cursor: Option<TerminalCursor>,
    pub scrollbar: Option<TerminalScrollbar>,
    cached_rows: Vec<CachedTerminalRowLayout>,
}

impl TerminalElementLayout {
    fn glyph_edge_exclusions(
        &self,
        rows: usize,
        cols: usize,
        command_mark_decorations: bool,
    ) -> Vec<Vec<bool>> {
        // Mark cells whose backdrop differs from the pane's solid fill. Keep this
        // outside the cached text layout so selection and search don't reshape text.
        let mut exclusions = vec![Vec::new(); rows];
        let mut exclude = |row: usize, col: usize, cells: usize| {
            if let Some(excluded) = exclusions.get_mut(row) {
                excluded.resize(cols, false);
                excluded[col.min(cols)..col.saturating_add(cells).min(cols)].fill(true);
            }
        };
        for rect in self
            .backgrounds
            .iter()
            .chain(&self.highlight_backgrounds)
            .chain(&self.selection_matches)
            .chain(&self.search_matches)
            .chain(&self.selections)
        {
            exclude(rect.row, rect.col, rect.cells);
        }
        for cached_row in &self.cached_rows {
            for rect in cached_row
                .layout
                .backgrounds
                .iter()
                .chain(&cached_row.layout.selections)
            {
                exclude(cached_row.row, rect.col, rect.cells);
            }
        }
        if command_mark_decorations {
            for overlay in &self.command_mark_overlays {
                if overlay.selected || overlay.hovered {
                    for row in overlay.start_row..=overlay.end_row {
                        exclude(row, 0, cols);
                    }
                }
            }
        }
        exclusions
    }
}

#[derive(Clone)]
pub struct TerminalRect {
    pub row: usize,
    pub col: usize,
    pub cells: usize,
    pub color: Hsla,
    pub powerline_cutout: Option<char>,
}

#[derive(Clone)]
pub struct BatchedTextRun {
    pub row: usize,
    pub col: usize,
    pub text: SharedString,
    pub cells: usize,
    pub style: TextRun,
    pub custom_underline: Option<CustomUnderline>,
    cache: Option<Arc<TerminalTextRunCache>>,
}

#[derive(Default)]
struct TerminalTextRunCache {
    double_cell: bool,
    layout: OnceLock<Option<Arc<LineLayout>>>,
    paint: LinePaintCache,
}

#[derive(Clone)]
pub struct TerminalImageLayout {
    pub image: TerminalRenderedImage,
}

#[derive(Clone)]
pub struct TerminalCommandMarkOverlay {
    pub start_row: usize,
    pub end_row: usize,
    pub has_top: bool,
    pub has_bottom: bool,
    pub stale: bool,
    pub selected: bool,
    pub hovered: bool,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub collapsed: bool,
    pub foldable: bool,
    pub fold_rotation: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalCursor {
    pub row: usize,
    pub col: usize,
    pub shape: TerminalCursorShape,
}

#[derive(Clone, Copy)]
pub struct TerminalScrollbar {
    pub top: f32,
    pub height: f32,
}

#[derive(Clone, Copy)]
pub struct TerminalHorizontalScrollbar {
    pub left: f32,
    pub width: f32,
    pub max_scroll: f32,
}

#[derive(Clone)]
struct TerminalRowLayout {
    backgrounds: Vec<TerminalRowRect>,
    selections: Vec<TerminalRowRect>,
    text_runs: Vec<TerminalRowTextRun>,
    cursor: Option<TerminalRowCursor>,
}

#[derive(Clone)]
struct CachedTerminalRowLayout {
    row: usize,
    layout: Arc<TerminalRowLayout>,
}

#[derive(Clone)]
struct TerminalRowRect {
    col: usize,
    cells: usize,
    color: Hsla,
    transition_key: TerminalTransitionColorKey,
    powerline_cutout: Option<char>,
}

#[derive(Clone)]
struct TerminalRowTextRun {
    col: usize,
    text: SharedString,
    cells: usize,
    style: TextRun,
    custom_underline: Option<CustomUnderline>,
    transition_key: TerminalTransitionColorKey,
    cache: Arc<TerminalTextRunCache>,
}

struct PendingTerminalRowTextRun {
    simple_han: bool,
    col: usize,
    text: String,
    cells: usize,
    style: TextRun,
    custom_underline: Option<CustomUnderline>,
    transition_key: TerminalTransitionColorKey,
}

impl PendingTerminalRowTextRun {
    fn finish(self, previous: Option<&TerminalRowTextRun>) -> TerminalRowTextRun {
        let double_cell = self.simple_han && self.cells > 2;
        let cache = previous
            .filter(|run| {
                // `TerminalTextRunCache.layout` depends only on glyph shaping inputs;
                // `DecorationRun` colors are supplied at paint time and keyed inside `LinePaintCache`.
                run.col == self.col
                    && run.cache.double_cell == double_cell
                    && run.cells == self.cells
                    && run.text.as_ref() == self.text
                    && run.style.len == self.style.len
                    && run.style.font == self.style.font
                    && run.style.letter_spacing == self.style.letter_spacing
                    && run.style.underline == self.style.underline
                    && run.style.strikethrough == self.style.strikethrough
            })
            .map(|run| run.cache.clone())
            .unwrap_or_else(|| {
                Arc::new(TerminalTextRunCache {
                    double_cell,
                    ..Default::default()
                })
            });
        TerminalRowTextRun {
            col: self.col,
            text: SharedString::from(self.text),
            cells: self.cells,
            style: self.style,
            custom_underline: self.custom_underline,
            transition_key: self.transition_key,
            cache,
        }
    }
}

#[derive(Clone, Copy)]
struct TerminalRowCursor {
    col: usize,
    shape: TerminalCursorShape,
}

struct TerminalLogicalHighlightLayout {
    backgrounds: Vec<TerminalRowOffsetRect>,
    underlines: Vec<TerminalRowOffsetRect>,
    outlines: Vec<TerminalRowOffsetRect>,
    foregrounds: FastHashMap<(usize, usize), TerminalHighlightForeground>,
}

struct TerminalRowOffsetRect {
    row_offset: usize,
    col: usize,
    cells: usize,
    color: Hsla,
}

struct TerminalRowLinkLayout {
    ranges: Vec<TerminalRelativeLinkRange>,
}

struct TerminalRelativeLinkRange {
    start_col: usize,
    end_col: usize,
    target: SharedString,
    kind: TerminalLinkKind,
}

// Wrapped rows share highlight and semantic metadata. Build that metadata once per frame instead
// of walking and hashing the complete logical line again for every visible physical row.
struct TerminalLogicalLine {
    range: Range<usize>,
    signature: u64,
}

struct TerminalLogicalLineIndex {
    visible_rows: Range<usize>,
    line_for_visible_row: Vec<usize>,
    lines: Vec<TerminalLogicalLine>,
}

impl TerminalLogicalLineIndex {
    fn line_for_row(&self, row: usize) -> Option<&TerminalLogicalLine> {
        let row_offset = row.checked_sub(self.visible_rows.start)?;
        let line_index = *self.line_for_visible_row.get(row_offset)?;
        self.lines.get(line_index)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct TerminalRowLayoutCacheKey {
    signature: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct TerminalLogicalHighlightCacheKey {
    signature: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct TerminalRowLinkCacheKey {
    signature: u64,
}

struct RecentCache<K, V> {
    entries: FastHashMap<K, V>,
    insertion_order: VecDeque<K>,
    capacity: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalLayoutPerformance {
    #[cfg(feature = "bench")]
    pub semantic_roles_micros: u64,
    #[cfg(feature = "bench")]
    pub links_micros: u64,
    #[cfg(feature = "bench")]
    pub highlights_micros: u64,
    #[cfg(feature = "bench")]
    pub semantic_command_lines: usize,
    pub layout_micros: u64,
    pub paint_micros: u64,
    pub cache_hit_percent: u8,
}

impl<K, V> RecentCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            entries: FastHashMap::with_capacity(capacity),
            insertion_order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn get(&self, key: &K) -> Option<V> {
        self.entries.get(key).cloned()
    }

    fn pop_oldest(&mut self) -> Option<V> {
        let key = self.insertion_order.pop_front()?;
        self.entries.remove(&key)
    }

    fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
        while self.entries.len() > capacity {
            let _ = self.pop_oldest();
        }
    }

    fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some(previous) = self.entries.get_mut(&key) {
            return Some(std::mem::replace(previous, value));
        }
        // Insertion-order eviction keeps streaming misses constant-time.
        let evicted = if self.entries.len() >= self.capacity {
            self.pop_oldest()
        } else {
            None
        };
        self.insertion_order.push_back(key.clone());
        self.entries.insert(key, value);
        evicted
    }

    fn get_or_insert_with(&mut self, key: K, build: impl FnOnce() -> V) -> (V, bool) {
        if let Some(value) = self.get(&key) {
            return (value, true);
        }
        let value = build();
        self.insert(key, value.clone());
        (value, false)
    }

    fn clear(&mut self) {
        self.entries = FastHashMap::new();
        self.insertion_order = VecDeque::new();
    }
}

pub struct TerminalLayoutCache {
    last_frame: Option<(u64, Arc<TerminalElementLayout>)>,
    rows: RecentCache<TerminalRowLayoutCacheKey, Arc<TerminalRowLayout>>,
    recent_row_keys: RecentCache<u64, TerminalRowLayoutCacheKey>,
    reuse_metrics: Option<[u32; 3]>,
    semantic_roles: RecentCache<(usize, usize, bool), SemanticLineRole>,
    semantic_marks: Option<Weak<[CommandMarkAnnotation]>>,
    semantic_spans: RecentCache<
        (u64, SemanticLineRole, u64),
        Arc<Vec<oxideterm_terminal_semantic::SemanticSpan>>,
    >,
    semantic_span_count: usize,
    highlights: RecentCache<TerminalLogicalHighlightCacheKey, Arc<TerminalLogicalHighlightLayout>>,
    links: RecentCache<TerminalRowLinkCacheKey, Arc<TerminalRowLinkLayout>>,
    timestamps: RecentCache<(SharedString, u64), Arc<TerminalTextRunCache>>,
    performance: TerminalLayoutPerformance,
    cache_hits: u64,
    cache_misses: u64,
}

impl Default for TerminalLayoutCache {
    fn default() -> Self {
        Self {
            last_frame: None,
            rows: RecentCache::new(TERMINAL_ROW_LAYOUT_CACHE_CAPACITY),
            recent_row_keys: RecentCache::new(TERMINAL_ROW_LAYOUT_CACHE_CAPACITY),
            reuse_metrics: None,
            semantic_roles: RecentCache::new(TERMINAL_ROW_LAYOUT_CACHE_CAPACITY),
            semantic_marks: None,
            semantic_spans: RecentCache::new(TERMINAL_HIGHLIGHT_CACHE_CAPACITY),
            semantic_span_count: 0,
            highlights: RecentCache::new(TERMINAL_HIGHLIGHT_CACHE_CAPACITY),
            links: RecentCache::new(TERMINAL_LINK_CACHE_CAPACITY),
            timestamps: RecentCache::new(TERMINAL_TIMESTAMP_CACHE_CAPACITY),
            performance: TerminalLayoutPerformance::default(),
            cache_hits: 0,
            cache_misses: 0,
        }
    }
}

impl TerminalLayoutCache {
    fn get_or_insert_row_with(
        &mut self,
        key: TerminalRowLayoutCacheKey,
        row_id: u64,
        collect_performance_metrics: bool,
        build: impl FnOnce(Option<&TerminalRowLayout>) -> TerminalRowLayout,
    ) -> Arc<TerminalRowLayout> {
        if let Some(layout) = self.rows.entries.get(&key).cloned() {
            self.record_cache_access(true, collect_performance_metrics);
            return layout;
        }
        // Keep only keys here: old text and glyph resources remain owned by the bounded row cache.
        let previous = (row_id != 0)
            .then(|| self.recent_row_keys.entries.get(&row_id))
            .flatten()
            .and_then(|key| self.rows.entries.get(key))
            .cloned();
        let (layout, _) = self
            .rows
            .get_or_insert_with(key.clone(), || Arc::new(build(previous.as_deref())));
        if row_id != 0 {
            if let Some(previous_key) = self.recent_row_keys.entries.get_mut(&row_id) {
                *previous_key = key;
            } else {
                self.recent_row_keys.get_or_insert_with(row_id, || key);
            }
        }
        self.record_cache_access(false, collect_performance_metrics);
        layout
    }

    fn semantic_spans_for(
        &mut self,
        key: (u64, SemanticLineRole, u64),
        build: impl FnOnce() -> Vec<oxideterm_terminal_semantic::SemanticSpan>,
    ) -> Arc<Vec<oxideterm_terminal_semantic::SemanticSpan>> {
        if let Some(spans) = self.semantic_spans.get(&key) {
            return spans;
        }
        let spans = Arc::new(build());
        if spans.len() > TERMINAL_SEMANTIC_SPAN_CACHE_BUDGET {
            return spans;
        }
        while self.semantic_span_count + spans.len() > TERMINAL_SEMANTIC_SPAN_CACHE_BUDGET {
            let evicted = self
                .semantic_spans
                .pop_oldest()
                .expect("span budget belongs to cached entries");
            self.semantic_span_count -= evicted.len();
        }
        if let Some(evicted) = self.semantic_spans.insert(key, spans.clone()) {
            self.semantic_span_count -= evicted.len();
        }
        self.semantic_span_count += spans.len();
        spans
    }

    fn get_or_insert_highlight_with(
        &mut self,
        key: TerminalLogicalHighlightCacheKey,
        collect_performance_metrics: bool,
        build: impl FnOnce() -> TerminalLogicalHighlightLayout,
    ) -> Arc<TerminalLogicalHighlightLayout> {
        let (layout, hit) = self
            .highlights
            .get_or_insert_with(key, || Arc::new(build()));
        self.record_cache_access(hit, collect_performance_metrics);
        layout
    }

    fn get_or_insert_links_with(
        &mut self,
        key: TerminalRowLinkCacheKey,
        collect_performance_metrics: bool,
        build: impl FnOnce() -> TerminalRowLinkLayout,
    ) -> Arc<TerminalRowLinkLayout> {
        let (layout, hit) = self.links.get_or_insert_with(key, || Arc::new(build()));
        self.record_cache_access(hit, collect_performance_metrics);
        layout
    }

    fn record_cache_access(&mut self, hit: bool, collect_performance_metrics: bool) {
        if !collect_performance_metrics {
            return;
        }
        if hit {
            self.cache_hits = self.cache_hits.saturating_add(1);
        } else {
            self.cache_misses = self.cache_misses.saturating_add(1);
        }
        self.refresh_cache_hit_percent();
    }

    fn record_cache_hits(&mut self, hits: u64, collect_performance_metrics: bool) {
        if !collect_performance_metrics {
            return;
        }
        self.cache_hits = self.cache_hits.saturating_add(hits);
        self.refresh_cache_hit_percent();
    }

    fn refresh_cache_hit_percent(&mut self) {
        let accesses = self.cache_hits.saturating_add(self.cache_misses);
        self.performance.cache_hit_percent = self
            .cache_hits
            .saturating_mul(100)
            .checked_div(accesses)
            .unwrap_or(0)
            .min(100) as u8;
    }

    fn record_layout_duration(&mut self, duration: Duration) {
        self.performance.layout_micros = duration_micros(duration);
    }

    fn record_paint_duration(&mut self, duration: Duration) {
        self.performance.paint_micros = duration_micros(duration);
    }

    pub fn performance(&self) -> TerminalLayoutPerformance {
        self.performance
    }

    pub fn clear(&mut self) {
        self.last_frame = None;
        self.rows.clear();
        self.recent_row_keys.clear();
        self.reuse_metrics = None;
        self.semantic_roles.clear();
        self.semantic_marks = None;
        self.highlights.clear();
        self.semantic_spans.clear();
        self.semantic_span_count = 0;
        self.links.clear();
        self.timestamps.clear();
        self.performance = TerminalLayoutPerformance::default();
        self.cache_hits = 0;
        self.cache_misses = 0;
    }
}

fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

impl<H: 'static> TerminalElement<H> {
    #[allow(dead_code)]
    pub fn new(
        snapshot: TerminalSnapshot,
        selection: Option<TerminalSelection>,
        metrics: TerminalMetrics,
        cursor_visible: bool,
        marked_text: Option<String>,
        search_query: Option<String>,
        search_matches: impl Into<Arc<[TerminalSearchMatch]>>,
        selected_search_match: Option<usize>,
        hovered_link: Option<TerminalLinkRange>,
        input: Option<TerminalElementInput<H>>,
    ) -> Self {
        Self::new_with_images(
            snapshot,
            Vec::new(),
            selection,
            metrics,
            TerminalUiTheme::default(),
            cursor_visible,
            marked_text,
            search_query,
            search_matches,
            selected_search_match,
            hovered_link,
            input,
        )
    }

    pub fn new_with_images(
        snapshot: TerminalSnapshot,
        rendered_images: Vec<TerminalRenderedImage>,
        selection: Option<TerminalSelection>,
        metrics: TerminalMetrics,
        theme: TerminalUiTheme,
        cursor_visible: bool,
        marked_text: Option<String>,
        search_query: Option<String>,
        search_matches: impl Into<Arc<[TerminalSearchMatch]>>,
        selected_search_match: Option<usize>,
        hovered_link: Option<TerminalLinkRange>,
        input: Option<TerminalElementInput<H>>,
    ) -> Self {
        Self::new_with_images_and_bidi(
            snapshot,
            rendered_images,
            selection,
            metrics,
            theme,
            cursor_visible,
            marked_text,
            search_query,
            search_matches,
            selected_search_match,
            hovered_link,
            true,
            input,
        )
    }

    pub fn new_with_images_and_bidi(
        snapshot: TerminalSnapshot,
        rendered_images: Vec<TerminalRenderedImage>,
        selection: Option<TerminalSelection>,
        metrics: TerminalMetrics,
        theme: TerminalUiTheme,
        cursor_visible: bool,
        marked_text: Option<String>,
        search_query: Option<String>,
        search_matches: impl Into<Arc<[TerminalSearchMatch]>>,
        selected_search_match: Option<usize>,
        hovered_link: Option<TerminalLinkRange>,
        bidi_enabled: bool,
        input: Option<TerminalElementInput<H>>,
    ) -> Self {
        let viewport_rows = snapshot.rows;
        let scrollbar_display_offset = snapshot.display_offset as f32;
        Self {
            snapshot,
            rendered_images,
            selection,
            metrics,
            theme,
            transition_colors: None,
            dilation_settle_row: None,
            cursor_visible,
            marked_text,
            marked_text_caret_utf16: None,
            search_query,
            search_matches: search_matches.into(),
            search_matches_precomputed: false,
            selection_highlight_query: None,
            selected_search_match,
            command_marks: Arc::from([]),
            selected_command_mark_id: None,
            hovered_command_mark_id: None,
            highlight_rules: Arc::from([]),
            highlight_rules_signature: 0,
            transient_command_highlight: None,
            transient_command_highlight_signature: 0,
            semantic_coloring: false,
            semantic_scheme: None,
            semantic_shell: SemanticShellDialect::Auto,
            semantic_classification_signature: 0,
            semantic_style_signature: 0,
            hovered_link,
            detect_file_paths_as_links: true,
            bidi_enabled,
            input,
            transparent_background: false,
            row_timestamps: None,
            ghost_text: None,
            layout_cache: None,
            performance_metrics_enabled: false,
            viewport_rows,
            scrollbar_display_offset,
            scrollbar_visible: false,
            scrollbar_active: false,
            scroll_y_offset: px(0.0),
            scroll_x_offset: px(0.0),
            command_mark_gutter_width: 0.0,
            command_mark_decorations: true,
            collapsed_commands: HashSet::new(),
            fold_rotations: HashMap::new(),
        }
    }

    pub fn theme_transition_colors(
        mut self,
        transition_colors: Option<TerminalTransitionColors>,
    ) -> Self {
        self.transition_colors = transition_colors;
        self.refresh_semantic_style_signature();
        self
    }

    pub fn dilation_settle_row(mut self, row: Option<usize>) -> Self {
        self.dilation_settle_row = row;
        self
    }

    pub fn highlight_rules(mut self, rules: impl Into<Arc<[TerminalHighlightRule]>>) -> Self {
        self.highlight_rules = rules.into();
        self.highlight_rules_signature = terminal_highlight_rules_signature(&self.highlight_rules);
        self
    }

    pub fn transient_command_highlight(
        mut self,
        highlight: Option<TransientCommandHighlight>,
    ) -> Self {
        self.transient_command_highlight = highlight;
        self.transient_command_highlight_signature = terminal_transient_command_highlight_signature(
            self.transient_command_highlight.as_ref(),
        );
        self
    }

    pub fn semantic_configuration(
        mut self,
        enabled: bool,
        scheme: Arc<CompiledSemanticScheme>,
        shell: SemanticShellDialect,
    ) -> Self {
        // Production rendering supplies all semantic inputs together, so hash the final state once
        // instead of rebuilding the same cache signature after three builder calls.
        self.semantic_coloring = enabled;
        self.semantic_scheme = Some(scheme);
        self.semantic_shell = shell;
        self.refresh_semantic_style_signature();
        self
    }

    fn refresh_semantic_style_signature(&mut self) {
        let semantic_scheme = self
            .semantic_scheme
            .as_deref()
            .unwrap_or_else(|| compiled_builtin_scheme(SemanticScheme::Balanced));
        self.semantic_classification_signature = terminal_semantic_classification_signature(
            self.semantic_coloring,
            semantic_scheme,
            self.semantic_shell,
        );
        let terminal_palette = self
            .transition_colors
            .as_ref()
            .map_or(self.theme.tokens.terminal, |transition| {
                transition.target_terminal
            });
        self.semantic_style_signature = terminal_semantic_style_signature_from_palette(
            self.semantic_coloring,
            terminal_palette,
            semantic_scheme,
            self.semantic_shell,
        );
    }

    fn semantic_scheme(&self) -> &CompiledSemanticScheme {
        self.semantic_scheme
            .as_deref()
            .unwrap_or_else(|| compiled_builtin_scheme(SemanticScheme::Balanced))
    }

    /// Places the composition caret inside the marked text; `None` keeps it at the end.
    pub fn marked_text_caret(mut self, caret_utf16: Option<usize>) -> Self {
        self.marked_text_caret_utf16 = caret_utf16;
        self
    }

    pub fn detect_file_paths_as_links(mut self, enabled: bool) -> Self {
        self.detect_file_paths_as_links = enabled;
        self
    }

    pub fn command_annotations(
        mut self,
        marks: impl Into<Arc<[CommandMarkAnnotation]>>,
        selected_command_mark_id: Option<String>,
        hovered_command_mark_id: Option<String>,
    ) -> Self {
        self.command_marks = marks.into();
        self.selected_command_mark_id = selected_command_mark_id;
        self.hovered_command_mark_id = hovered_command_mark_id;
        self
    }

    pub fn transparent_background(mut self, transparent_background: bool) -> Self {
        self.transparent_background = transparent_background;
        self
    }

    pub fn command_folding(
        mut self,
        collapsed: HashSet<String>,
        decorations: bool,
        fold_rotations: HashMap<String, f32>,
    ) -> Self {
        self.collapsed_commands = collapsed;
        self.command_mark_decorations = decorations;
        self.fold_rotations = fold_rotations;
        self
    }

    pub fn ghost_text(mut self, ghost_text: Option<String>) -> Self {
        self.ghost_text = ghost_text;
        self
    }

    pub fn viewport_rows(mut self, viewport_rows: usize) -> Self {
        self.viewport_rows = viewport_rows;
        self
    }

    pub fn scrollbar_active(mut self, active: bool) -> Self {
        self.scrollbar_active = active;
        self
    }

    pub fn scrollbar_visible(mut self, visible: bool) -> Self {
        self.scrollbar_visible = visible;
        self
    }

    pub fn scrollbar_display_offset(mut self, display_offset_rows: f32) -> Self {
        self.scrollbar_display_offset = display_offset_rows;
        self
    }

    pub fn scroll_y_offset(mut self, scroll_y_offset: Pixels) -> Self {
        self.scroll_y_offset = scroll_y_offset;
        self
    }

    pub fn scroll_x_offset(mut self, scroll_x_offset: Pixels) -> Self {
        self.scroll_x_offset = scroll_x_offset.max(px(0.0));
        self
    }

    pub fn command_mark_gutter_width(mut self, width: f32) -> Self {
        self.command_mark_gutter_width = width.max(0.0);
        self
    }

    pub fn row_timestamps(
        mut self,
        row_timestamps: Option<Arc<TerminalRowTimestampStore>>,
    ) -> Self {
        self.row_timestamps = row_timestamps;
        self
    }

    pub fn selection_highlight_query(mut self, query: Option<Arc<Zeroizing<String>>>) -> Self {
        self.selection_highlight_query = query;
        self
    }

    pub fn precomputed_search_matches(mut self) -> Self {
        // An empty result is still a computed result. This flag prevents the
        // visible-row fallback search from rescanning on every repaint.
        self.search_matches_precomputed = true;
        self
    }

    pub fn layout_cache(mut self, cache: Rc<RefCell<TerminalLayoutCache>>) -> Self {
        self.layout_cache = Some(cache);
        self
    }

    pub fn performance_metrics_enabled(mut self, enabled: bool) -> Self {
        // Timing and cache accounting stay off during normal rendering so diagnostics do not
        // become a permanent cost in the terminal hot path.
        self.performance_metrics_enabled = enabled;
        self
    }

    #[allow(dead_code)]
    pub fn layout(&self) -> TerminalElementLayout {
        self.layout_for_rows(0..self.painted_row_limit(), None)
    }

    pub fn layout_for_bounds(&self, bounds: Bounds<Pixels>) -> TerminalElementLayout {
        self.layout_for_rows(self.visible_rows_for_bounds(bounds), None)
    }
}

impl<H: crate::TerminalCanvasHost> IntoElement for TerminalElement<H> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<H: crate::TerminalCanvasHost> Element for TerminalElement<H> {
    type RequestLayoutState = ();
    type PrepaintState = Arc<TerminalElementLayout>;

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
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let started = self.performance_metrics_enabled.then(Instant::now);
        let layout = self.cached_layout_for_bounds(bounds);
        if let (Some(started), Some(cache)) = (started, &self.layout_cache) {
            cache.borrow_mut().record_layout_duration(started.elapsed());
        }
        layout
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let paint_started = self.performance_metrics_enabled.then(Instant::now);
        if let Some(input) = &self.input {
            input.sync_viewport(bounds, window);
        }
        if self.hovered_link.is_some() {
            window.set_window_cursor_style(CursorStyle::PointingHand);
        }

        if !self.transparent_background {
            window.paint_quad(fill(bounds, rgb(self.theme.background)));
        }
        let timestamp_gutter_width =
            terminal_timestamp_gutter_width(&self.metrics, self.row_timestamps.is_some());
        let viewport_timestamp_origin =
            bounds.origin + point(px(TERMINAL_CONTENT_PADDING), px(TERMINAL_CONTENT_PADDING));
        let timestamp_origin = viewport_timestamp_origin + point(px(0.0), self.scroll_y_offset);
        // The timestamp gutter and the terminal grid use separate origins so
        // timestamps remain a paint-only overlay and never affect text runs.
        let grid_gutter_width = timestamp_gutter_width + self.command_mark_gutter_width;
        let viewport_origin = viewport_timestamp_origin + point(px(grid_gutter_width), px(0.0));
        let origin =
            timestamp_origin + point(px(grid_gutter_width) - self.scroll_x_offset, px(0.0));
        let grid_viewport_width = px((f32::from(bounds.size.width) - grid_gutter_width).max(0.0));
        let viewport_mask_bounds = Bounds::new(
            viewport_timestamp_origin,
            size(
                px((f32::from(bounds.size.width) - TERMINAL_CONTENT_PADDING).max(0.0)),
                px(self.viewport_rows as f32 * self.metrics.line_height_f32()),
            ),
        );

        window.with_content_mask(
            Some(ContentMask {
                bounds: viewport_mask_bounds,
            }),
            |window| {
                if self.row_timestamps.is_some() {
                    if !layout.timestamp_runs.is_empty() {
                        window.paint_layer(viewport_mask_bounds, |window| {
                            paint_text_runs_in_layer(
                                &layout.timestamp_runs,
                                timestamp_origin,
                                &self.metrics,
                                gpui::GlyphEdgeEnhancement::None,
                                None,
                                window,
                                cx,
                            );
                        });
                    }
                    let divider_x = viewport_timestamp_origin.x
                        + px((TERMINAL_TIMESTAMP_LABEL_CELLS as f32
                            + TERMINAL_TIMESTAMP_GUTTER_GAP_CELLS / 2.0)
                            * self.metrics.cell_width_f32());
                    let divider_bounds = Bounds::new(
                        point(divider_x, viewport_timestamp_origin.y),
                        size(
                            px(1.0),
                            px(self.viewport_rows as f32 * self.metrics.line_height_f32()),
                        ),
                    );
                    window.paint_quad(fill(
                        divider_bounds,
                        rgba((self.theme.foreground << 8) | 0x2e),
                    ));
                }
            },
        );
        let grid_mask_bounds = Bounds::new(
            viewport_origin,
            size(
                grid_viewport_width.max(px(0.0)),
                px(self.viewport_rows as f32 * self.metrics.line_height_f32()),
            ),
        );
        window.with_content_mask(
            Some(ContentMask {
                bounds: grid_mask_bounds,
            }),
            |window| {
                for image in layout
                    .images
                    .iter()
                    .filter(|image| image.image.snapshot.z_index < -1_073_741_824)
                {
                    paint_terminal_image(image, origin, &self.metrics, window, cx);
                }
                let live_terminal_background = terminal_background(&self.theme);
                let transition_colors = self.transition_colors.as_ref();
                for rect in &layout.backgrounds {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for cached_row in &layout.cached_rows {
                    for rect in &cached_row.layout.backgrounds {
                        paint_terminal_row_rect(
                            cached_row.row,
                            rect,
                            transition_colors,
                            live_terminal_background,
                            origin,
                            &self.metrics,
                            window,
                        );
                    }
                }
                for rect in &layout.highlight_backgrounds {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for image in layout
                    .images
                    .iter()
                    .filter(|image| (-1_073_741_824..0).contains(&image.image.snapshot.z_index))
                {
                    paint_terminal_image(image, origin, &self.metrics, window, cx);
                }
                for rect in &layout.selection_matches {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for rect in &layout.search_matches {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for overlay in &layout.command_mark_overlays {
                    paint_command_mark_overlay(
                        overlay,
                        viewport_origin + point(px(0.0), self.scroll_y_offset),
                        grid_viewport_width,
                        &self.metrics,
                        self.command_mark_decorations,
                        window,
                    );
                }
                for rect in &layout.selections {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for cached_row in &layout.cached_rows {
                    for rect in &cached_row.layout.selections {
                        paint_terminal_row_rect(
                            cached_row.row,
                            rect,
                            transition_colors,
                            live_terminal_background,
                            origin,
                            &self.metrics,
                            window,
                        );
                    }
                }
                let glyph_edge_mode = if cfg!(target_os = "macos")
                    && !self.transparent_background
                    && layout.images.is_empty()
                {
                    let bg_l = terminal_background(&self.theme).l;
                    if bg_l >= 0.8 {
                        gpui::GlyphEdgeEnhancement::LightBackground
                    } else if bg_l <= 0.25 {
                        gpui::GlyphEdgeEnhancement::DarkBackground
                    } else {
                        gpui::GlyphEdgeEnhancement::None
                    }
                } else {
                    gpui::GlyphEdgeEnhancement::None
                };
                let glyph_edge_exclusions = (glyph_edge_mode != gpui::GlyphEdgeEnhancement::None)
                    .then(|| {
                        layout.glyph_edge_exclusions(
                            self.snapshot.lines.len(),
                            self.snapshot.cols,
                            self.command_mark_decorations,
                        )
                    });
                if !layout.text_runs.is_empty() || !layout.cached_rows.is_empty() {
                    // Establish one viewport-scoped layer for all visible terminal rows so GPUI
                    // can batch glyphs, underlines, and box-drawing quads across rows instead of
                    // splitting GPU draw batches at every row boundary.
                    window.paint_layer(grid_mask_bounds, |window| {
                        paint_text_runs_in_layer(
                            &layout.text_runs,
                            origin,
                            &self.metrics,
                            glyph_edge_mode,
                            glyph_edge_exclusions.as_deref(),
                            window,
                            cx,
                        );
                        for cached_row in &layout.cached_rows {
                            paint_cached_text_runs_in_layer(
                                cached_row.row,
                                &cached_row.layout.text_runs,
                                origin,
                                &self.metrics,
                                glyph_edge_mode,
                                glyph_edge_exclusions.as_deref(),
                                transition_colors,
                                self.dilation_settle_row,
                                live_terminal_background,
                                window,
                                cx,
                            );
                        }
                    });
                }
                if let Some(ghost_text) = &layout.ghost_text {
                    paint_grid_text_run(ghost_text, origin, &self.metrics, window, cx);
                }
                if let Some(marked_text) = &layout.marked_text {
                    paint_grid_text_run(marked_text, origin, &self.metrics, window, cx);
                }
                for image in layout
                    .images
                    .iter()
                    .filter(|image| image.image.snapshot.z_index >= 0)
                {
                    paint_terminal_image(image, origin, &self.metrics, window, cx);
                }
                for rect in &layout.highlight_underlines {
                    paint_terminal_underline(rect, origin, &self.metrics, window);
                }
                for rect in &layout.highlight_outlines {
                    paint_terminal_outline(rect, origin, &self.metrics, window);
                }
            },
        );
        // Gutter glyphs sit to the left of the grid mask. Keep text clipped to
        // the grid and paint the gutter separately within the full viewport.
        if self.command_mark_gutter_width > 0.0 {
            window.with_content_mask(
                Some(ContentMask {
                    bounds: viewport_mask_bounds,
                }),
                |window| {
                    for overlay in &layout.command_mark_overlays {
                        paint_command_gutter(
                            overlay,
                            origin,
                            &self.metrics,
                            self.command_mark_gutter_width,
                            &self.theme,
                            window,
                        );
                    }
                },
            );
        }
        if let Some(input) = &self.input {
            let input_origin = viewport_origin - point(self.scroll_x_offset, px(0.0));
            let content_bounds = terminal_content_bounds_for_rows(
                input_origin,
                self.viewport_rows,
                self.snapshot.cols,
                &self.metrics,
            );
            input.register_input(content_bounds, window, cx);
        }
        if let Some(cursor) = layout.cursor {
            window.with_content_mask(
                Some(ContentMask {
                    bounds: grid_mask_bounds,
                }),
                |window| {
                    paint_cursor(
                        cursor,
                        origin,
                        &self.metrics,
                        self.theme.header_foreground,
                        window,
                    );
                },
            );
        }
        if self.scrollbar_visible
            && let Some(scrollbar) = layout.scrollbar
        {
            paint_scrollbar(
                scrollbar,
                viewport_origin,
                grid_viewport_width,
                self.theme.foreground,
                self.scrollbar_active,
                window,
            );
        }
        // Search locations remain visible independently of the transient scroll thumb.
        let marker_x = terminal_scrollbar_x_for_viewport(grid_viewport_width);
        let marker_height = SEARCH_SCROLLBAR_MARKER_HEIGHT
            .min(self.viewport_rows as f32 * self.metrics.line_height_f32());
        for &(top, selected) in &layout.search_scrollbar_markers {
            let color = if selected {
                self.theme.tokens.ui.accent
            } else {
                self.theme.tokens.ui.warning
            };
            window.paint_quad(fill(
                Bounds::new(
                    viewport_origin + point(marker_x, px(top)),
                    size(px(SCROLLBAR_WIDTH), px(marker_height)),
                ),
                rgb(color),
            ));
        }
        if let Some(horizontal_scrollbar) = terminal_horizontal_scrollbar_for_viewport(
            f32::from(grid_viewport_width),
            timestamp_gutter_width,
            f32::from(self.scroll_x_offset),
        ) {
            paint_horizontal_scrollbar(
                horizontal_scrollbar,
                viewport_origin,
                grid_viewport_width,
                bounds.size.height,
                window,
            );
        }
        if let (Some(started), Some(cache)) = (paint_started, &self.layout_cache) {
            cache.borrow_mut().record_paint_duration(started.elapsed());
        }
    }
}

#[cfg(test)]
mod tests;
