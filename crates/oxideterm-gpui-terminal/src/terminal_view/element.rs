use std::{
    collections::{HashMap, VecDeque, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    ops::Range,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, ContentMask, CursorStyle, Element, ElementId, Entity, FocusHandle,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine,
    SharedString, Style, TextRun, Window, fill, point, px, relative, rgb, rgba, size,
};
use oxideterm_terminal::{
    TerminalColor, TerminalCommandMark, TerminalCursorShape, TerminalSearchMatch, TerminalSnapshot,
};
use oxideterm_terminal_semantic::{
    CompiledSemanticScheme, SemanticLineRole, SemanticScheme, SemanticShellDialect,
    compiled_builtin_scheme,
};
use oxideterm_terminal_unicode::{TerminalVisualLine, visual_line_for_row_if_bidi};
use parking_lot::Mutex;
use unicode_width::UnicodeWidthChar;

use crate::app::{TerminalInputHandler, TerminalPane, TerminalRenderedImage, TerminalRowTimestamp};
use crate::command_facts::TransientCommandHighlight;
use crate::terminal_ui::*;
use crate::terminal_view::highlight::{TerminalHighlightLayout, terminal_highlights_for_rows};
use crate::terminal_view::links::*;
use crate::terminal_view::selection::TerminalSelection;
use crate::terminal_view::semantic::{
    append_terminal_semantics_for_rows, semantic_line_role_for_rows,
};

mod layout;
mod paint;
mod style;

const TERMINAL_ROW_LAYOUT_CACHE_CAPACITY: usize = 512;
const TERMINAL_HIGHLIGHT_CACHE_CAPACITY: usize = 128;
const TERMINAL_LINK_CACHE_CAPACITY: usize = 512;
const TRANSIENT_COMMAND_HIGHLIGHT_ALPHA: u32 = 0x52;

pub(crate) use layout::*;
#[cfg(test)]
pub(crate) use paint::ghost_text_grid_segments;
use paint::*;
pub(crate) use style::*;

pub(crate) struct TerminalElement {
    snapshot: TerminalSnapshot,
    rendered_images: Vec<TerminalRenderedImage>,
    selection: Option<TerminalSelection>,
    metrics: TerminalMetrics,
    theme: TerminalUiTheme,
    cursor_visible: bool,
    marked_text: Option<String>,
    ghost_text: Option<String>,
    search_query: Option<String>,
    search_matches: Arc<[TerminalSearchMatch]>,
    search_matches_precomputed: bool,
    selected_search_match: Option<usize>,
    command_marks: Arc<[TerminalCommandMark]>,
    selected_command_mark_id: Option<String>,
    hovered_command_mark_id: Option<String>,
    highlight_rules: Arc<[TerminalHighlightRule]>,
    highlight_rules_signature: u64,
    transient_command_highlight: Option<TransientCommandHighlight>,
    transient_command_highlight_signature: u64,
    semantic_coloring: bool,
    semantic_scheme: Arc<CompiledSemanticScheme>,
    semantic_shell: SemanticShellDialect,
    semantic_style_signature: u64,
    hovered_link: Option<TerminalLinkRange>,
    detect_file_paths_as_links: bool,
    bidi_enabled: bool,
    input: Option<TerminalElementInput>,
    transparent_background: bool,
    row_timestamps: Option<Arc<HashMap<u64, TerminalRowTimestamp>>>,
    layout_cache: Option<Arc<Mutex<TerminalLayoutCache>>>,
    performance_metrics_enabled: bool,
    viewport_rows: usize,
    scrollbar_display_offset: f32,
    scroll_y_offset: Pixels,
    command_mark_gutter_width: f32,
}

#[derive(Clone)]
pub(crate) struct TerminalElementInput {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) view: Entity<TerminalPane>,
    pub(crate) last_viewport_bounds: Option<Bounds<Pixels>>,
    pub(crate) last_viewport_scale_factor_bits: Option<u32>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TerminalElementLayout {
    pub(crate) backgrounds: Vec<TerminalRect>,
    pub(crate) highlight_backgrounds: Vec<TerminalRect>,
    pub(crate) highlight_underlines: Vec<TerminalRect>,
    pub(crate) highlight_outlines: Vec<TerminalRect>,
    pub(crate) search_matches: Vec<TerminalRect>,
    pub(crate) command_mark_overlays: Vec<TerminalCommandMarkOverlay>,
    pub(crate) selections: Vec<TerminalRect>,
    pub(crate) images: Vec<TerminalImageLayout>,
    pub(crate) text_runs: Vec<BatchedTextRun>,
    pub(crate) timestamp_runs: Vec<BatchedTextRun>,
    pub(crate) marked_text: Option<BatchedTextRun>,
    pub(crate) ghost_text: Option<BatchedTextRun>,
    pub(crate) ime_cursor_bounds: Option<Bounds<Pixels>>,
    pub(crate) cursor: Option<TerminalCursor>,
    pub(crate) scrollbar: Option<TerminalScrollbar>,
}

#[derive(Clone)]
pub(crate) struct TerminalRect {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) cells: usize,
    pub(crate) color: Hsla,
}

#[derive(Clone)]
pub(crate) struct BatchedTextRun {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) text: SharedString,
    pub(crate) cells: usize,
    pub(crate) style: TextRun,
    shaped: Option<Arc<OnceLock<ShapedLine>>>,
}

#[derive(Clone)]
pub(crate) struct TerminalImageLayout {
    pub(crate) image: TerminalRenderedImage,
}

#[derive(Clone)]
pub(crate) struct TerminalCommandMarkOverlay {
    pub(crate) start_row: usize,
    pub(crate) end_row: usize,
    pub(crate) has_top: bool,
    pub(crate) has_bottom: bool,
    pub(crate) stale: bool,
    pub(crate) selected: bool,
    pub(crate) hovered: bool,
    pub(crate) running: bool,
    pub(crate) exit_code: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalCursor {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) shape: TerminalCursorShape,
}

#[derive(Clone, Copy)]
pub(crate) struct TerminalScrollbar {
    pub(crate) top: f32,
    pub(crate) height: f32,
}

#[derive(Clone)]
struct TerminalRowLayout {
    backgrounds: Vec<TerminalRowRect>,
    selections: Vec<TerminalRowRect>,
    text_runs: Vec<TerminalRowTextRun>,
    cursor: Option<TerminalRowCursor>,
}

#[derive(Clone)]
struct TerminalRowRect {
    col: usize,
    cells: usize,
    color: Hsla,
}

#[derive(Clone)]
struct TerminalRowTextRun {
    col: usize,
    text: SharedString,
    cells: usize,
    style: TextRun,
    shaped: Arc<OnceLock<ShapedLine>>,
}

struct PendingTerminalRowTextRun {
    col: usize,
    text: String,
    cells: usize,
    style: TextRun,
}

impl From<PendingTerminalRowTextRun> for TerminalRowTextRun {
    fn from(run: PendingTerminalRowTextRun) -> Self {
        Self {
            col: run.col,
            text: SharedString::from(run.text),
            cells: run.cells,
            style: run.style,
            shaped: Arc::new(OnceLock::new()),
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
    foregrounds: HashMap<(usize, usize), Hsla>,
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
pub(crate) struct TerminalRowLayoutCacheKey {
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
    entries: HashMap<K, V>,
    insertion_order: VecDeque<K>,
    capacity: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TerminalLayoutPerformance {
    pub(crate) layout_micros: u64,
    pub(crate) paint_micros: u64,
    pub(crate) cache_hit_percent: u8,
}

impl<K, V> RecentCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            entries: HashMap::with_capacity(capacity),
            insertion_order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn get_or_insert_with(&mut self, key: K, build: impl FnOnce() -> V) -> (V, bool) {
        if let Some(value) = self.entries.get(&key) {
            return (value.clone(), true);
        }

        if self.entries.len() >= self.capacity
            && let Some(oldest_key) = self.insertion_order.pop_front()
        {
            // Streaming output continuously misses these caches. Insertion-order eviction keeps
            // that hot path constant-time while retaining several viewports of recent rows.
            self.entries.remove(&oldest_key);
        }

        let value = build();
        self.entries.insert(key.clone(), value.clone());
        self.insertion_order.push_back(key);
        (value, false)
    }
}

pub(crate) struct TerminalLayoutCache {
    rows: RecentCache<TerminalRowLayoutCacheKey, Arc<TerminalRowLayout>>,
    highlights: RecentCache<TerminalLogicalHighlightCacheKey, Arc<TerminalLogicalHighlightLayout>>,
    links: RecentCache<TerminalRowLinkCacheKey, Arc<TerminalRowLinkLayout>>,
    performance: TerminalLayoutPerformance,
    cache_hits: u64,
    cache_misses: u64,
}

impl Default for TerminalLayoutCache {
    fn default() -> Self {
        Self {
            rows: RecentCache::new(TERMINAL_ROW_LAYOUT_CACHE_CAPACITY),
            highlights: RecentCache::new(TERMINAL_HIGHLIGHT_CACHE_CAPACITY),
            links: RecentCache::new(TERMINAL_LINK_CACHE_CAPACITY),
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
        collect_performance_metrics: bool,
        build: impl FnOnce() -> TerminalRowLayout,
    ) -> Arc<TerminalRowLayout> {
        let (layout, hit) = self.rows.get_or_insert_with(key, || Arc::new(build()));
        self.record_cache_access(hit, collect_performance_metrics);
        layout
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
        let accesses = self.cache_hits.saturating_add(self.cache_misses);
        self.performance.cache_hit_percent = if accesses == 0 {
            0
        } else {
            ((self.cache_hits.saturating_mul(100) / accesses).min(100)) as u8
        };
    }

    fn record_layout_duration(&mut self, duration: Duration) {
        self.performance.layout_micros = duration_micros(duration);
    }

    fn record_paint_duration(&mut self, duration: Duration) {
        self.performance.paint_micros = duration_micros(duration);
    }

    pub(crate) fn performance(&self) -> TerminalLayoutPerformance {
        self.performance
    }
}

fn duration_micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

impl TerminalElement {
    #[allow(dead_code)]
    pub(crate) fn new(
        snapshot: TerminalSnapshot,
        selection: Option<TerminalSelection>,
        metrics: TerminalMetrics,
        cursor_visible: bool,
        marked_text: Option<String>,
        search_query: Option<String>,
        search_matches: impl Into<Arc<[TerminalSearchMatch]>>,
        selected_search_match: Option<usize>,
        hovered_link: Option<TerminalLinkRange>,
        input: Option<TerminalElementInput>,
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

    pub(crate) fn new_with_images(
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
        input: Option<TerminalElementInput>,
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

    pub(crate) fn new_with_images_and_bidi(
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
        input: Option<TerminalElementInput>,
    ) -> Self {
        let viewport_rows = snapshot.rows;
        let scrollbar_display_offset = snapshot.display_offset as f32;
        let highlight_rules = Arc::from(Vec::<TerminalHighlightRule>::new());
        let semantic_coloring = false;
        let semantic_scheme = Arc::new(compiled_builtin_scheme(SemanticScheme::Balanced).clone());
        let semantic_shell = SemanticShellDialect::Auto;
        let highlight_rules_signature = terminal_highlight_rules_signature(&highlight_rules);
        let semantic_style_signature = terminal_semantic_style_signature(
            semantic_coloring,
            &theme,
            &semantic_scheme,
            semantic_shell,
        );
        Self {
            snapshot,
            rendered_images,
            selection,
            metrics,
            theme,
            cursor_visible,
            marked_text,
            search_query,
            search_matches: search_matches.into(),
            search_matches_precomputed: false,
            selected_search_match,
            command_marks: Arc::from([]),
            selected_command_mark_id: None,
            hovered_command_mark_id: None,
            highlight_rules,
            highlight_rules_signature,
            transient_command_highlight: None,
            transient_command_highlight_signature: 0,
            semantic_coloring,
            semantic_scheme,
            semantic_shell,
            semantic_style_signature,
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
            scroll_y_offset: px(0.0),
            command_mark_gutter_width: 0.0,
        }
    }

    pub(crate) fn highlight_rules(
        mut self,
        rules: impl Into<Arc<[TerminalHighlightRule]>>,
    ) -> Self {
        self.highlight_rules = rules.into();
        self.highlight_rules_signature = terminal_highlight_rules_signature(&self.highlight_rules);
        self
    }

    pub(crate) fn transient_command_highlight(
        mut self,
        highlight: Option<TransientCommandHighlight>,
    ) -> Self {
        self.transient_command_highlight = highlight;
        self.transient_command_highlight_signature = terminal_transient_command_highlight_signature(
            self.transient_command_highlight.as_ref(),
        );
        self
    }

    pub(crate) fn semantic_coloring(mut self, enabled: bool) -> Self {
        self.semantic_coloring = enabled;
        self.refresh_semantic_style_signature();
        self
    }

    pub(crate) fn semantic_scheme(mut self, scheme: Arc<CompiledSemanticScheme>) -> Self {
        self.semantic_scheme = scheme;
        self.refresh_semantic_style_signature();
        self
    }

    pub(crate) fn semantic_shell(mut self, shell: SemanticShellDialect) -> Self {
        self.semantic_shell = shell;
        self.refresh_semantic_style_signature();
        self
    }

    fn refresh_semantic_style_signature(&mut self) {
        self.semantic_style_signature = terminal_semantic_style_signature(
            self.semantic_coloring,
            &self.theme,
            &self.semantic_scheme,
            self.semantic_shell,
        );
    }

    pub(crate) fn detect_file_paths_as_links(mut self, enabled: bool) -> Self {
        self.detect_file_paths_as_links = enabled;
        self
    }

    pub(crate) fn command_marks(
        mut self,
        marks: impl Into<Arc<[TerminalCommandMark]>>,
        selected_command_mark_id: Option<String>,
        hovered_command_mark_id: Option<String>,
    ) -> Self {
        self.command_marks = marks.into();
        self.selected_command_mark_id = selected_command_mark_id;
        self.hovered_command_mark_id = hovered_command_mark_id;
        self
    }

    pub(crate) fn transparent_background(mut self, transparent_background: bool) -> Self {
        self.transparent_background = transparent_background;
        self
    }

    pub(crate) fn ghost_text(mut self, ghost_text: Option<String>) -> Self {
        self.ghost_text = ghost_text;
        self
    }

    pub(crate) fn viewport_rows(mut self, viewport_rows: usize) -> Self {
        self.viewport_rows = viewport_rows;
        self
    }

    pub(crate) fn scrollbar_display_offset(mut self, display_offset_rows: f32) -> Self {
        self.scrollbar_display_offset = display_offset_rows;
        self
    }

    pub(crate) fn scroll_y_offset(mut self, scroll_y_offset: Pixels) -> Self {
        self.scroll_y_offset = scroll_y_offset;
        self
    }

    pub(crate) fn command_mark_gutter_width(mut self, width: f32) -> Self {
        self.command_mark_gutter_width = width.max(0.0);
        self
    }

    pub(crate) fn row_timestamps(
        mut self,
        row_timestamps: Option<Arc<HashMap<u64, TerminalRowTimestamp>>>,
    ) -> Self {
        self.row_timestamps = row_timestamps;
        self
    }

    pub(crate) fn precomputed_search_matches(mut self) -> Self {
        // An empty result is still a computed result. This flag prevents the
        // visible-row fallback search from rescanning on every repaint.
        self.search_matches_precomputed = true;
        self
    }

    pub(crate) fn layout_cache(mut self, cache: Arc<Mutex<TerminalLayoutCache>>) -> Self {
        self.layout_cache = Some(cache);
        self
    }

    pub(crate) fn performance_metrics_enabled(mut self, enabled: bool) -> Self {
        // Timing and cache accounting stay off during normal rendering so diagnostics do not
        // become a permanent cost in the terminal hot path.
        self.performance_metrics_enabled = enabled;
        self
    }

    #[allow(dead_code)]
    pub(crate) fn layout(&self) -> TerminalElementLayout {
        self.layout_for_rows(0..self.painted_row_limit(), None)
    }

    pub(crate) fn layout_for_bounds(&self, bounds: Bounds<Pixels>) -> TerminalElementLayout {
        self.layout_for_rows(self.visible_rows_for_bounds(bounds), None)
    }

    fn painted_row_limit(&self) -> usize {
        let overscan_rows = if f32::from(self.scroll_y_offset).abs() > f32::EPSILON {
            1
        } else {
            0
        };
        self.viewport_rows
            .saturating_add(overscan_rows)
            .min(self.snapshot.lines.len())
    }

    fn visible_rows_for_bounds(&self, bounds: Bounds<Pixels>) -> Range<usize> {
        let mut rows = terminal_visible_rows_for_limit(bounds, &self.metrics, self.viewport_rows);
        if f32::from(self.scroll_y_offset).abs() > f32::EPSILON {
            rows.end = rows.end.saturating_add(1).min(self.painted_row_limit());
        }
        rows
    }

    fn layout_for_rows(
        &self,
        visible_rows: Range<usize>,
        mut cache: Option<&mut TerminalLayoutCache>,
    ) -> TerminalElementLayout {
        let mut backgrounds = Vec::new();
        let semantic_roles = self.semantic_roles_for_rows(visible_rows.clone());
        let logical_lines = self.logical_lines_for_rows(visible_rows.clone());
        let highlight_layout = if let Some(cache) = cache.as_deref_mut() {
            self.cached_highlight_layout_for_rows(&logical_lines, &semantic_roles, cache)
        } else {
            self.highlight_layout_for_logical_lines(&logical_lines)
        };
        let search_matches = map_rects_to_visual(
            &self.snapshot,
            self.bidi_enabled,
            if !self.search_matches_precomputed && self.search_matches.is_empty() {
                search_match_rects_for_rows(
                    &self.snapshot,
                    self.search_query.as_deref(),
                    visible_rows.clone(),
                )
            } else {
                visible_search_match_rects(
                    &self.search_matches,
                    self.snapshot.display_offset,
                    visible_rows.clone(),
                    self.selected_search_match,
                )
            },
        );
        let command_mark_overlays = command_mark_overlays_for_rows(
            &self.snapshot,
            &self.command_marks,
            self.selected_command_mark_id.as_deref(),
            self.hovered_command_mark_id.as_deref(),
        );
        let mut selections = Vec::new();
        let mut images = self
            .rendered_images
            .iter()
            .filter(|image| {
                image.snapshot.row < self.painted_row_limit()
                    && image.snapshot.row + image.snapshot.rows > visible_rows.start
                    && image.snapshot.row < visible_rows.end
            })
            .cloned()
            .map(|image| TerminalImageLayout { image })
            .collect::<Vec<_>>();
        images.sort_by_key(|image| (image.image.snapshot.z_index, image.image.snapshot.id.0));
        let mut text_runs = Vec::new();
        let mut timestamp_runs = Vec::new();
        let mut cursor = None;
        let scrollbar = terminal_scrollbar_for_viewport_display_offset(
            &self.snapshot,
            &self.metrics,
            self.viewport_rows,
            self.scrollbar_display_offset,
        );
        let terminal_background = terminal_background(&self.theme);
        let cursor_row_visible = visible_rows.contains(&self.snapshot.cursor_row);
        let ime_cursor_bounds = cursor_row_visible
            .then(|| ime_cursor_bounds_for_snapshot(&self.snapshot, &self.metrics))
            .flatten();
        let link_ranges = if let Some(cache) = cache.as_deref_mut() {
            self.cached_link_ranges_for_rows(visible_rows.clone(), cache)
        } else {
            display_link_ranges_for_rows_with_path_detection(
                &self.snapshot,
                visible_rows.clone(),
                self.detect_file_paths_as_links,
            )
        };

        for row_index in visible_rows {
            let Some(row) = self.snapshot.lines.get(row_index) else {
                continue;
            };
            let row_layout = if let Some(cache) = cache.as_deref_mut() {
                let logical_line = logical_lines
                    .line_for_row(row_index)
                    .expect("visible snapshot rows must have logical line metadata");
                let semantic_role = semantic_roles.get(&logical_line.range).copied();
                let key = self.row_layout_cache_key_with_logical_line(
                    row_index,
                    logical_line,
                    semantic_role,
                );
                cache.get_or_insert_row_with(key, self.performance_metrics_enabled, || {
                    self.row_layout(
                        row_index,
                        row,
                        &highlight_layout,
                        &link_ranges,
                        terminal_background,
                    )
                })
            } else {
                Arc::new(self.row_layout(
                    row_index,
                    row,
                    &highlight_layout,
                    &link_ranges,
                    terminal_background,
                ))
            };
            append_cached_row_layout(
                row_index,
                &row_layout,
                &mut backgrounds,
                &mut selections,
                &mut text_runs,
                &mut cursor,
            );
            if let Some(timestamp_run) = self.timestamp_run_for_row(row_index, row.line_id) {
                timestamp_runs.push(timestamp_run);
            }
        }

        TerminalElementLayout {
            backgrounds,
            highlight_backgrounds: map_rects_to_visual(
                &self.snapshot,
                self.bidi_enabled,
                highlight_layout.backgrounds,
            ),
            highlight_underlines: map_rects_to_visual(
                &self.snapshot,
                self.bidi_enabled,
                highlight_layout.underlines,
            ),
            highlight_outlines: map_rects_to_visual(
                &self.snapshot,
                self.bidi_enabled,
                highlight_layout.outlines,
            ),
            search_matches,
            command_mark_overlays,
            selections,
            images,
            text_runs,
            timestamp_runs,
            marked_text: self.marked_text.as_ref().and_then(|text| {
                ime_cursor_bounds?;
                let marked_col = self
                    .snapshot
                    .lines
                    .get(self.snapshot.cursor_row)
                    .and_then(|row| visual_line_for_row_with_bidi(row, self.bidi_enabled))
                    .map(|line| line.visual_col_for_logical_col(self.snapshot.cursor_col))
                    .unwrap_or(self.snapshot.cursor_col);
                Some(BatchedTextRun {
                    row: self.snapshot.cursor_row,
                    col: marked_col,
                    text: SharedString::from(text.clone()),
                    cells: text.encode_utf16().count().max(1),
                    style: marked_text_run(text, &self.metrics),
                    shaped: None,
                })
            }),
            ghost_text: self.ghost_text_run(cursor_row_visible),
            ime_cursor_bounds,
            cursor,
            scrollbar,
        }
    }

    fn timestamp_run_for_row(&self, row_index: usize, line_id: u64) -> Option<BatchedTextRun> {
        let label = self.row_timestamps.as_ref()?.get(&line_id)?.label.clone();
        Some(BatchedTextRun {
            row: row_index,
            col: 0,
            cells: TERMINAL_TIMESTAMP_LABEL_CELLS,
            style: timestamp_text_run(&label, &self.theme, &self.metrics),
            text: SharedString::from(label),
            shaped: None,
        })
    }

    fn cached_layout_for_bounds(&self, bounds: Bounds<Pixels>) -> Arc<TerminalElementLayout> {
        let Some(cache) = &self.layout_cache else {
            return Arc::new(self.layout_for_bounds(bounds));
        };
        let visible_rows = self.visible_rows_for_bounds(bounds);
        let mut cache = cache.lock();
        Arc::new(self.layout_for_rows(visible_rows, Some(&mut cache)))
    }

    fn cached_highlight_layout_for_rows(
        &self,
        logical_lines: &TerminalLogicalLineIndex,
        semantic_roles: &HashMap<Range<usize>, SemanticLineRole>,
        cache: &mut TerminalLayoutCache,
    ) -> TerminalHighlightLayout {
        let mut layout = TerminalHighlightLayout::empty();

        for logical_line in &logical_lines.lines {
            let semantic_role = semantic_roles.get(&logical_line.range).copied();
            let key =
                self.logical_highlight_cache_key_with_logical_line(logical_line, semantic_role);
            let relative_layout =
                cache.get_or_insert_highlight_with(key, self.performance_metrics_enabled, || {
                    let line_layout = self.highlight_layout_for_rows(logical_line.range.clone());
                    relative_highlight_layout(logical_line.range.start, line_layout)
                });
            append_relative_highlight_layout(
                logical_line.range.start,
                &relative_layout,
                &mut layout,
            );
        }

        layout
    }

    fn highlight_layout_for_logical_lines(
        &self,
        logical_lines: &TerminalLogicalLineIndex,
    ) -> TerminalHighlightLayout {
        let mut layout = TerminalHighlightLayout::empty();
        for logical_line in &logical_lines.lines {
            let mut line_layout = self.highlight_layout_for_rows(logical_line.range.clone());
            layout.backgrounds.append(&mut line_layout.backgrounds);
            layout.underlines.append(&mut line_layout.underlines);
            layout.outlines.append(&mut line_layout.outlines);
            layout.foregrounds.extend(line_layout.foregrounds);
        }
        layout
    }

    fn highlight_layout_for_rows(&self, rows: Range<usize>) -> TerminalHighlightLayout {
        let transient = self.transient_command_highlight.as_ref().map(|highlight| {
            (
                highlight,
                rgba((self.theme.tokens.ui.warning << 8) | TRANSIENT_COMMAND_HIGHLIGHT_ALPHA)
                    .into(),
            )
        });
        let mut layout = terminal_highlights_for_rows(
            &self.snapshot,
            &self.highlight_rules,
            transient,
            rows.clone(),
        );
        if self.semantic_coloring {
            append_terminal_semantics_for_rows(
                &self.snapshot,
                &self.command_marks,
                rows,
                &self.theme,
                &self.semantic_scheme,
                self.semantic_shell,
                &mut layout,
            );
        }
        layout
    }

    fn cached_link_ranges_for_rows(
        &self,
        visible_rows: Range<usize>,
        cache: &mut TerminalLayoutCache,
    ) -> Vec<TerminalLinkRange> {
        let mut ranges = Vec::new();
        for row_index in visible_rows {
            if self.snapshot.lines.get(row_index).is_none() {
                continue;
            }
            let key = self.row_link_cache_key(row_index);
            let row_layout =
                cache.get_or_insert_links_with(key, self.performance_metrics_enabled, || {
                    relative_link_layout(display_link_ranges_for_rows_with_path_detection(
                        &self.snapshot,
                        row_index..row_index + 1,
                        self.detect_file_paths_as_links,
                    ))
                });
            ranges.extend(row_layout.ranges.iter().map(|range| TerminalLinkRange {
                row: row_index,
                start_col: range.start_col,
                end_col: range.end_col,
                target: range.target.clone(),
                kind: range.kind,
            }));
        }
        ranges
    }

    fn row_layout(
        &self,
        row_index: usize,
        row: &oxideterm_terminal::TerminalRow,
        highlight_layout: &TerminalHighlightLayout,
        link_ranges: &[TerminalLinkRange],
        terminal_background: Hsla,
    ) -> TerminalRowLayout {
        // Visible link ranges are ordered by row. Restrict cell styling to this row so output in
        // other rows cannot multiply every character lookup in a link-dense terminal.
        let row_links_start = link_ranges.partition_point(|range| range.row < row_index);
        let row_links_end = row_links_start
            + link_ranges[row_links_start..].partition_point(|range| range.row == row_index);
        let link_ranges = &link_ranges[row_links_start..row_links_end];
        let mut backgrounds = Vec::new();
        let mut selections = Vec::new();
        let mut text_runs = Vec::new();
        let mut cursor = None;
        let mut current_background: Option<TerminalRowRect> = None;
        let mut current_selection: Option<TerminalRowRect> = None;
        let mut current_run: Option<PendingTerminalRowTextRun> = None;
        let visual_line = visual_line_for_row_with_bidi(row, self.bidi_enabled);

        for (col_index, cell) in row.cells.iter().enumerate() {
            let paint_col = visual_line
                .as_ref()
                .map(|line| line.visual_col_for_logical_col(col_index))
                .unwrap_or(col_index);
            if self.cursor_visible
                && cell.cursor
                && self.snapshot.cursor_shape != TerminalCursorShape::Hidden
            {
                cursor = Some(TerminalRowCursor {
                    col: paint_col,
                    shape: self.snapshot.cursor_shape,
                });
            }

            let block_cursor = self.cursor_visible
                && cell.cursor
                && self.snapshot.cursor_shape == TerminalCursorShape::Block;
            let fg = if block_cursor {
                to_hsla(terminal_color_from_hex(self.theme.background))
            } else if let Some(highlight_fg) =
                highlight_layout.foreground_for_cell(row_index, col_index)
            {
                highlight_fg
            } else {
                to_hsla(resolve_terminal_foreground(cell.fg, &self.theme))
            };
            let bg = if block_cursor {
                to_hsla(terminal_color_from_hex(self.theme.header_foreground))
            } else {
                to_hsla(resolve_terminal_background(cell.bg, &self.theme))
            };
            let cell_width = if cell.wide { 2 } else { 1 };

            if bg != terminal_background {
                extend_or_push_row_rect(
                    &mut current_background,
                    &mut backgrounds,
                    paint_col,
                    cell_width,
                    bg,
                );
            } else if let Some(rect) = current_background.take() {
                backgrounds.push(rect);
            }

            if self.selection.is_some_and(|selection| {
                selection.contains_viewport_cell(row_index, col_index, self.snapshot.display_offset)
            }) {
                extend_or_push_row_rect(
                    &mut current_selection,
                    &mut selections,
                    paint_col,
                    cell_width,
                    to_hsla(TerminalColor::rgb(0x2d, 0x4f, 0x7f)),
                );
            } else if let Some(rect) = current_selection.take() {
                selections.push(rect);
            }

            if visual_line.is_some() {
                continue;
            }

            if cell.ch != ' '
                || !cell.zerowidth().is_empty()
                || (self.cursor_visible && cell.cursor)
            {
                let link = !block_cursor
                    && (cell.hyperlink().is_some() || is_link_stylable_cell(cell))
                    && link_should_be_styled(
                        link_ranges,
                        self.hovered_link.as_ref(),
                        row_index,
                        col_index,
                    );
                let style = text_run_for_cell(cell, fg, link, &self.metrics);
                if cell.zerowidth().is_empty() && powerline_separator(cell.ch).is_some() {
                    if let Some(run) = current_run.take() {
                        text_runs.push(run);
                    }
                    text_runs.push(PendingTerminalRowTextRun {
                        col: col_index,
                        text: cell_text(cell),
                        cells: cell_width,
                        style,
                    });
                    continue;
                }
                if let Some(run) = &mut current_run
                    && run.col + run.cells == col_index
                    && text_run_style_matches(&run.style, &style)
                {
                    // Append directly to the existing run so ordinary rows do not
                    // allocate a temporary String for every visible cell.
                    push_cell_text(&mut run.text, cell);
                    run.cells += cell_width;
                    run.style.len += cell_text_len(cell);
                    continue;
                }

                if let Some(run) = current_run.take() {
                    text_runs.push(run);
                }
                current_run = Some(PendingTerminalRowTextRun {
                    col: col_index,
                    text: cell_text(cell),
                    cells: cell_width,
                    style,
                });
            } else if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
        }

        if let Some(visual_line) = visual_line.as_ref() {
            push_visual_text_runs(
                row_index,
                row,
                visual_line,
                link_ranges,
                self.hovered_link.as_ref(),
                &self.metrics,
                self.cursor_visible,
                self.snapshot.cursor_shape,
                &self.theme,
                highlight_layout,
                &mut text_runs,
            );
        }

        if let Some(rect) = current_background.take() {
            backgrounds.push(rect);
        }
        if let Some(rect) = current_selection.take() {
            selections.push(rect);
        }
        if let Some(run) = current_run.take() {
            text_runs.push(run);
        }

        TerminalRowLayout {
            backgrounds,
            selections,
            text_runs: text_runs.into_iter().map(Into::into).collect(),
            cursor,
        }
    }

    fn row_layout_cache_key_with_logical_line(
        &self,
        row_index: usize,
        logical_line: &TerminalLogicalLine,
        semantic_role: Option<SemanticLineRole>,
    ) -> TerminalRowLayoutCacheKey {
        let mut hasher = DefaultHasher::new();
        self.snapshot.cols.hash(&mut hasher);
        if let Some(row) = self.snapshot.lines.get(row_index) {
            row.line_id.hash(&mut hasher);
            row.signature.hash(&mut hasher);
            let has_cursor = row.cells.iter().any(|cell| cell.cursor);
            has_cursor.hash(&mut hasher);
            if has_cursor {
                self.snapshot.cursor_shape.hash(&mut hasher);
                self.cursor_visible.hash(&mut hasher);
            }
        }
        logical_line.signature.hash(&mut hasher);
        f32::from(self.metrics.font_size)
            .to_bits()
            .hash(&mut hasher);
        f32::from(self.metrics.cell_width)
            .to_bits()
            .hash(&mut hasher);
        f32::from(self.metrics.line_height)
            .to_bits()
            .hash(&mut hasher);
        self.metrics.font.hash(&mut hasher);
        self.theme.background.hash(&mut hasher);
        self.theme.foreground.hash(&mut hasher);
        self.theme.header_foreground.hash(&mut hasher);
        self.hash_semantic_layout(logical_line.range.clone(), semantic_role, &mut hasher);
        self.bidi_enabled.hash(&mut hasher);
        self.detect_file_paths_as_links.hash(&mut hasher);
        if let Some(hovered_link) = self
            .hovered_link
            .as_ref()
            .filter(|hovered_link| hovered_link.row == row_index)
        {
            hovered_link.hash(&mut hasher);
        }
        hash_selection_for_row(
            self.selection,
            row_index,
            self.snapshot.display_offset,
            self.snapshot.cols,
            &mut hasher,
        );
        self.highlight_rules_signature.hash(&mut hasher);
        self.transient_command_highlight_signature.hash(&mut hasher);
        TerminalRowLayoutCacheKey {
            signature: hasher.finish(),
        }
    }

    fn logical_highlight_cache_key_with_logical_line(
        &self,
        logical_line: &TerminalLogicalLine,
        semantic_role: Option<SemanticLineRole>,
    ) -> TerminalLogicalHighlightCacheKey {
        let mut hasher = DefaultHasher::new();
        self.snapshot.cols.hash(&mut hasher);
        self.highlight_rules_signature.hash(&mut hasher);
        self.transient_command_highlight_signature.hash(&mut hasher);
        self.hash_semantic_layout(logical_line.range.clone(), semantic_role, &mut hasher);
        logical_line.signature.hash(&mut hasher);
        TerminalLogicalHighlightCacheKey {
            signature: hasher.finish(),
        }
    }

    fn hash_semantic_layout(
        &self,
        rows: Range<usize>,
        semantic_role: Option<SemanticLineRole>,
        hasher: &mut impl Hasher,
    ) {
        self.semantic_style_signature.hash(hasher);
        if !self.semantic_coloring {
            return;
        }
        semantic_role
            .unwrap_or_else(|| {
                semantic_line_role_for_rows(&self.snapshot, &self.command_marks, rows)
            })
            .hash(hasher);
    }

    fn logical_lines_for_rows(&self, visible_rows: Range<usize>) -> TerminalLogicalLineIndex {
        let visible_rows = visible_rows.start.min(self.snapshot.lines.len())
            ..visible_rows.end.min(self.snapshot.lines.len());
        let mut line_for_visible_row = vec![0; visible_rows.len()];
        let mut lines = Vec::new();
        let mut row_index = visible_rows.start;

        while row_index < visible_rows.end {
            let Some(range) = logical_line_range_for_row(&self.snapshot, row_index) else {
                break;
            };
            let mut hasher = DefaultHasher::new();
            range.len().hash(&mut hasher);
            for row in self.snapshot.lines.get(range.clone()).unwrap_or(&[]) {
                row.line_id.hash(&mut hasher);
                row.signature.hash(&mut hasher);
            }
            let line_index = lines.len();
            let mapped_start = range.start.max(visible_rows.start);
            let mapped_end = range.end.min(visible_rows.end);
            for mapped_row in mapped_start..mapped_end {
                line_for_visible_row[mapped_row - visible_rows.start] = line_index;
            }
            lines.push(TerminalLogicalLine {
                range: range.clone(),
                signature: hasher.finish(),
            });
            row_index = range.end.max(row_index + 1);
        }

        TerminalLogicalLineIndex {
            visible_rows,
            line_for_visible_row,
            lines,
        }
    }

    fn semantic_roles_for_rows(
        &self,
        visible_rows: Range<usize>,
    ) -> HashMap<Range<usize>, SemanticLineRole> {
        if !self.semantic_coloring {
            return HashMap::new();
        }
        let mut roles = HashMap::new();
        for row_index in visible_rows {
            let Some(rows) = logical_line_range_for_row(&self.snapshot, row_index) else {
                continue;
            };
            // Wrapped rows share one semantic role, so the command marks are scanned once per
            // logical line and the result is reused by highlights and row layout keys.
            roles.entry(rows.clone()).or_insert_with(|| {
                semantic_line_role_for_rows(&self.snapshot, &self.command_marks, rows)
            });
        }
        roles
    }

    fn row_link_cache_key(&self, row_index: usize) -> TerminalRowLinkCacheKey {
        let mut hasher = DefaultHasher::new();
        if let Some(row) = self.snapshot.lines.get(row_index) {
            row.line_id.hash(&mut hasher);
            row.signature.hash(&mut hasher);
        }
        self.detect_file_paths_as_links.hash(&mut hasher);
        TerminalRowLinkCacheKey {
            signature: hasher.finish(),
        }
    }

    fn ghost_text_run(&self, cursor_row_visible: bool) -> Option<BatchedTextRun> {
        if self.marked_text.is_some()
            || !cursor_row_visible
            || self.snapshot.cursor_shape == TerminalCursorShape::Hidden
        {
            return None;
        }

        let text = self.ghost_text.as_deref().filter(|text| !text.is_empty())?;
        let row = self.snapshot.lines.get(self.snapshot.cursor_row)?;
        let visual_line = visual_line_for_row_with_bidi(row, self.bidi_enabled);
        let col = visual_line
            .as_ref()
            .map(|line| line.visual_col_for_logical_col(self.snapshot.cursor_col))
            .unwrap_or(self.snapshot.cursor_col);
        let remaining_cells = self.snapshot.cols.saturating_sub(col);
        if remaining_cells == 0 {
            return None;
        }

        let (visible_text, visible_cells) = ghost_text_prefix_for_cells(text, remaining_cells);
        if visible_text.is_empty() {
            return None;
        }

        Some(BatchedTextRun {
            row: self.snapshot.cursor_row,
            col,
            cells: visible_cells,
            style: ghost_text_run(&visible_text, &self.theme, &self.metrics),
            text: SharedString::from(visible_text),
            shaped: None,
        })
    }
}

fn ghost_text_prefix_for_cells(text: &str, max_cells: usize) -> (String, usize) {
    // Ghost text is painted on the terminal grid, so clipping must use terminal
    // cell width rather than Rust char count. Otherwise CJK hints can overlap
    // the following columns while the layout believes they still fit.
    let mut prefix = String::new();
    let mut cells = 0;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if width == 0 {
            prefix.push(ch);
            continue;
        }
        if cells + width > max_cells {
            break;
        }
        prefix.push(ch);
        cells += width;
    }
    (prefix, cells)
}

fn visual_line_for_row_with_bidi(
    row: &oxideterm_terminal::TerminalRow,
    bidi_enabled: bool,
) -> Option<TerminalVisualLine> {
    // Ordinary rows use their logical cell order without allocating a visual map.
    if bidi_enabled {
        visual_line_for_row_if_bidi(row)
    } else {
        None
    }
}

fn append_cached_row_layout(
    row_index: usize,
    row_layout: &TerminalRowLayout,
    backgrounds: &mut Vec<TerminalRect>,
    selections: &mut Vec<TerminalRect>,
    text_runs: &mut Vec<BatchedTextRun>,
    cursor: &mut Option<TerminalCursor>,
) {
    backgrounds.extend(row_layout.backgrounds.iter().map(|rect| TerminalRect {
        row: row_index,
        col: rect.col,
        cells: rect.cells,
        color: rect.color,
    }));
    selections.extend(row_layout.selections.iter().map(|rect| TerminalRect {
        row: row_index,
        col: rect.col,
        cells: rect.cells,
        color: rect.color,
    }));
    text_runs.extend(row_layout.text_runs.iter().map(|run| BatchedTextRun {
        row: row_index,
        col: run.col,
        text: run.text.clone(),
        cells: run.cells,
        style: run.style.clone(),
        shaped: Some(run.shaped.clone()),
    }));
    if let Some(row_cursor) = row_layout.cursor {
        *cursor = Some(TerminalCursor {
            row: row_index,
            col: row_cursor.col,
            shape: row_cursor.shape,
        });
    }
}

fn extend_or_push_row_rect(
    current: &mut Option<TerminalRowRect>,
    rects: &mut Vec<TerminalRowRect>,
    col: usize,
    cells: usize,
    color: Hsla,
) {
    if let Some(rect) = current
        && rect.col + rect.cells == col
        && rect.color == color
    {
        rect.cells += cells;
        return;
    }

    if let Some(rect) = current.take() {
        rects.push(rect);
    }
    *current = Some(TerminalRowRect { col, cells, color });
}

fn logical_line_range_for_row(
    snapshot: &TerminalSnapshot,
    row_index: usize,
) -> Option<Range<usize>> {
    if row_index >= snapshot.lines.len() {
        return None;
    }

    let mut start = row_index;
    while start > 0 && snapshot.lines.get(start).is_some_and(|line| line.wrapped) {
        start -= 1;
    }

    let mut end = row_index + 1;
    while end < snapshot.lines.len() && snapshot.lines.get(end).is_some_and(|line| line.wrapped) {
        end += 1;
    }

    Some(start..end)
}

fn relative_highlight_layout(
    start_row: usize,
    layout: TerminalHighlightLayout,
) -> TerminalLogicalHighlightLayout {
    TerminalLogicalHighlightLayout {
        backgrounds: relative_highlight_rects(start_row, layout.backgrounds),
        underlines: relative_highlight_rects(start_row, layout.underlines),
        outlines: relative_highlight_rects(start_row, layout.outlines),
        foregrounds: layout
            .foregrounds
            .into_iter()
            .filter_map(|((row, col), color)| {
                row.checked_sub(start_row)
                    .map(|row_offset| ((row_offset, col), color))
            })
            .collect(),
    }
}

fn relative_highlight_rects(
    start_row: usize,
    rects: Vec<TerminalRect>,
) -> Vec<TerminalRowOffsetRect> {
    rects
        .into_iter()
        .filter_map(|rect| {
            rect.row
                .checked_sub(start_row)
                .map(|row_offset| TerminalRowOffsetRect {
                    row_offset,
                    col: rect.col,
                    cells: rect.cells,
                    color: rect.color,
                })
        })
        .collect()
}

fn append_relative_highlight_layout(
    start_row: usize,
    relative: &TerminalLogicalHighlightLayout,
    layout: &mut TerminalHighlightLayout,
) {
    layout.backgrounds.extend(
        relative
            .backgrounds
            .iter()
            .map(|rect| absolute_highlight_rect(start_row, rect)),
    );
    layout.underlines.extend(
        relative
            .underlines
            .iter()
            .map(|rect| absolute_highlight_rect(start_row, rect)),
    );
    layout.outlines.extend(
        relative
            .outlines
            .iter()
            .map(|rect| absolute_highlight_rect(start_row, rect)),
    );
    layout.foregrounds.extend(
        relative
            .foregrounds
            .iter()
            .map(|((row_offset, col), color)| ((start_row + row_offset, *col), *color)),
    );
}

fn absolute_highlight_rect(start_row: usize, rect: &TerminalRowOffsetRect) -> TerminalRect {
    TerminalRect {
        row: start_row + rect.row_offset,
        col: rect.col,
        cells: rect.cells,
        color: rect.color,
    }
}

fn relative_link_layout(ranges: Vec<TerminalLinkRange>) -> TerminalRowLinkLayout {
    TerminalRowLinkLayout {
        ranges: ranges
            .into_iter()
            .map(|range| TerminalRelativeLinkRange {
                start_col: range.start_col,
                end_col: range.end_col,
                target: range.target,
                kind: range.kind,
            })
            .collect(),
    }
}

fn hash_selection_for_row(
    selection: Option<TerminalSelection>,
    row: usize,
    display_offset: usize,
    cols: usize,
    hasher: &mut impl Hasher,
) {
    let Some(selection) = selection else {
        0u8.hash(hasher);
        return;
    };
    let line = row as i32 - display_offset as i32;
    let selected_span =
        if selection.mode == crate::terminal_view::selection::TerminalSelectionMode::Block {
            let row_start = selection.anchor.line.min(selection.head.line);
            let row_end = selection.anchor.line.max(selection.head.line);
            (line >= row_start && line <= row_end).then(|| {
                (
                    selection.anchor.col.min(selection.head.col),
                    selection.anchor.col.max(selection.head.col),
                )
            })
        } else {
            let (start, end) = selection.normalized();
            if line < start.line || line > end.line {
                None
            } else {
                Some((
                    if line == start.line { start.col } else { 0 },
                    if line == end.line {
                        end.col
                    } else {
                        cols.saturating_sub(1)
                    },
                ))
            }
        };
    let Some((start_col, end_col)) = selected_span else {
        0u8.hash(hasher);
        return;
    };
    1u8.hash(hasher);
    start_col.hash(hasher);
    end_col.hash(hasher);
    match selection.mode {
        crate::terminal_view::selection::TerminalSelectionMode::Simple => 0u8,
        crate::terminal_view::selection::TerminalSelectionMode::Block => 1,
        crate::terminal_view::selection::TerminalSelectionMode::Semantic => 2,
        crate::terminal_view::selection::TerminalSelectionMode::Lines => 3,
    }
    .hash(hasher);
}

fn resolve_terminal_foreground(color: TerminalColor, theme: &TerminalUiTheme) -> TerminalColor {
    if color == terminal_color_from_hex(OXIDETERM_TERMINAL_FOREGROUND) {
        terminal_color_from_hex(theme.foreground)
    } else {
        color
    }
}

fn resolve_terminal_background(color: TerminalColor, theme: &TerminalUiTheme) -> TerminalColor {
    if color == terminal_color_from_hex(OXIDETERM_TERMINAL_BACKGROUND) {
        terminal_color_from_hex(theme.background)
    } else {
        color
    }
}

fn hash_highlight_rules(rules: &[TerminalHighlightRule], hasher: &mut impl Hasher) {
    rules.len().hash(hasher);
    for rule in rules {
        rule.id.hash(hasher);
        rule.pattern.hash(hasher);
        rule.is_regex.hash(hasher);
        rule.case_sensitive.hash(hasher);
        rule.foreground.hash(hasher);
        rule.background.hash(hasher);
        match rule.render_mode {
            TerminalHighlightRenderMode::Background => 0u8,
            TerminalHighlightRenderMode::Underline => 1,
            TerminalHighlightRenderMode::Outline => 2,
        }
        .hash(hasher);
        match rule.match_scope {
            crate::terminal_ui::TerminalHighlightMatchScope::Match => 0u8,
            crate::terminal_ui::TerminalHighlightMatchScope::LogicalLine => 1,
        }
        .hash(hasher);
        rule.preserve_background.hash(hasher);
        rule.enabled.hash(hasher);
        rule.priority.hash(hasher);
    }
}

fn terminal_highlight_rules_signature(rules: &[TerminalHighlightRule]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_highlight_rules(rules, &mut hasher);
    hasher.finish()
}

fn terminal_transient_command_highlight_signature(
    highlight: Option<&TransientCommandHighlight>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    highlight.hash(&mut hasher);
    hasher.finish()
}

fn terminal_semantic_style_signature(
    enabled: bool,
    theme: &TerminalUiTheme,
    scheme: &CompiledSemanticScheme,
    shell: SemanticShellDialect,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    enabled.hash(&mut hasher);
    if enabled {
        let terminal = theme.tokens.terminal;
        terminal.red.hash(&mut hasher);
        terminal.bright_red.hash(&mut hasher);
        terminal.green.hash(&mut hasher);
        terminal.bright_green.hash(&mut hasher);
        terminal.yellow.hash(&mut hasher);
        terminal.bright_yellow.hash(&mut hasher);
        terminal.blue.hash(&mut hasher);
        terminal.bright_blue.hash(&mut hasher);
        terminal.magenta.hash(&mut hasher);
        terminal.bright_magenta.hash(&mut hasher);
        terminal.cyan.hash(&mut hasher);
        terminal.bright_cyan.hash(&mut hasher);
        terminal.bright_black.hash(&mut hasher);
        scheme.signature().hash(&mut hasher);
        shell.hash(&mut hasher);
    }
    hasher.finish()
}

fn command_mark_overlays_for_rows(
    snapshot: &TerminalSnapshot,
    marks: &[TerminalCommandMark],
    selected_command_mark_id: Option<&str>,
    hovered_command_mark_id: Option<&str>,
) -> Vec<TerminalCommandMarkOverlay> {
    let viewport_start = snapshot
        .scrollback_lines
        .saturating_sub(snapshot.display_offset);
    let viewport_end = viewport_start.saturating_add(snapshot.rows.saturating_sub(1));
    let mut overlays = Vec::new();

    for mark in marks {
        let start_line = mark.start_line;
        let end_line = mark.end_line.unwrap_or_else(|| {
            snapshot_prompt_block_start_line(snapshot, snapshot_absolute_cursor_line(snapshot))
                .saturating_sub(1)
                .max(mark.start_line)
        });
        if end_line < start_line || end_line < viewport_start || start_line > viewport_end {
            continue;
        }

        let selected = selected_command_mark_id == Some(mark.command_id.as_str());
        let hovered = !selected && hovered_command_mark_id == Some(mark.command_id.as_str());
        let visible_start_line = start_line.max(viewport_start);
        let visible_end_line = end_line.min(viewport_end);
        overlays.push(TerminalCommandMarkOverlay {
            start_row: visible_start_line.saturating_sub(viewport_start),
            end_row: visible_end_line.saturating_sub(viewport_start),
            has_top: start_line >= viewport_start,
            has_bottom: end_line <= viewport_end,
            stale: mark.stale,
            selected,
            hovered,
            running: !mark.is_closed,
            exit_code: mark.exit_code,
        });
    }

    // Passive command edges should stay behind hover/selection fills when ranges overlap.
    overlays.sort_by_key(|overlay| (overlay.selected, overlay.hovered));
    overlays
}

fn snapshot_absolute_cursor_line(snapshot: &TerminalSnapshot) -> usize {
    snapshot
        .scrollback_lines
        .saturating_add(snapshot.cursor_row)
        .saturating_sub(snapshot.display_offset)
}

fn snapshot_prompt_block_start_line(snapshot: &TerminalSnapshot, command_line: usize) -> usize {
    if !snapshot_line_text(snapshot, command_line).is_some_and(is_likely_prompt_input_line) {
        return command_line;
    }

    let mut start_line = command_line;
    let min_line = command_line.saturating_sub(3);
    for line in (min_line..command_line).rev() {
        if !snapshot_line_text(snapshot, line).is_some_and(is_likely_prompt_preamble_line) {
            break;
        }
        start_line = line;
    }
    start_line
}

fn snapshot_line_text(snapshot: &TerminalSnapshot, absolute_line: usize) -> Option<String> {
    let viewport_start = snapshot
        .scrollback_lines
        .saturating_sub(snapshot.display_offset);
    let row = absolute_line.checked_sub(viewport_start)?;
    snapshot.lines.get(row).map(|line| line.text())
}

fn is_likely_prompt_input_line(text: String) -> bool {
    let trimmed = text.trim();
    trimmed.is_empty()
        || trimmed.chars().next().is_some_and(|ch| {
            matches!(
                ch,
                '❯' | '➜' | 'λ' | '>' | '$' | '#' | '%' | '❮' | '›' | '»'
            )
        })
}

fn is_likely_prompt_preamble_line(text: String) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let has_private_use_glyph = trimmed
        .chars()
        .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch));
    let has_powerline_glyph = trimmed
        .chars()
        .any(|ch| matches!(ch, '' | '' | '' | ''));
    let has_ruler = has_repeated_ruler(trimmed);
    let has_clock = has_clock_like_text(trimmed);
    let has_prompt_context = trimmed.contains('@')
        || trimmed.contains('~')
        || trimmed.contains('/')
        || trimmed.contains('$');

    has_powerline_glyph
        || (has_private_use_glyph && (has_clock || has_ruler || has_prompt_context))
        || (has_ruler && (has_clock || has_prompt_context))
}

fn has_repeated_ruler(text: &str) -> bool {
    let mut count = 0;
    for ch in text.chars() {
        if matches!(ch, '·' | '•' | '∙' | '.') {
            count += 1;
            if count >= 6 {
                return true;
            }
        } else {
            count = 0;
        }
    }
    false
}

fn has_clock_like_text(text: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_digit() && ch != ':')
        .any(|part| {
            let pieces = part.split(':').collect::<Vec<_>>();
            match pieces.as_slice() {
                [hour, minute] | [hour, minute, ..] => {
                    (1..=2).contains(&hour.len()) && minute.len() == 2
                }
                _ => false,
            }
        })
}

fn push_visual_text_runs(
    source_row_index: usize,
    row: &oxideterm_terminal::TerminalRow,
    visual_line: &TerminalVisualLine,
    link_ranges: &[TerminalLinkRange],
    hovered_link: Option<&TerminalLinkRange>,
    metrics: &TerminalMetrics,
    cursor_visible: bool,
    cursor_shape: TerminalCursorShape,
    theme: &TerminalUiTheme,
    highlight_layout: &TerminalHighlightLayout,
    text_runs: &mut Vec<PendingTerminalRowTextRun>,
) {
    let mut current_run: Option<PendingTerminalRowTextRun> = None;
    for cluster in &visual_line.clusters {
        let Some(cell) = row.cells.get(cluster.logical_col) else {
            continue;
        };
        if cell.ch == ' ' && cell.zerowidth().is_empty() {
            if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
            continue;
        }

        let block_cursor =
            cursor_visible && cell.cursor && cursor_shape == TerminalCursorShape::Block;
        let fg = if block_cursor {
            to_hsla(terminal_color_from_hex(theme.background))
        } else if let Some(highlight_fg) =
            highlight_layout.foreground_for_cell(source_row_index, cluster.logical_col)
        {
            highlight_fg
        } else {
            to_hsla(cell.fg)
        };
        let link = !block_cursor
            && (cell.hyperlink().is_some() || is_link_stylable_cell(cell))
            && link_should_be_styled(
                link_ranges,
                hovered_link,
                source_row_index,
                cluster.logical_col,
            );
        let style = text_run_for_cell(cell, fg, link, metrics);
        if cell.zerowidth().is_empty() && powerline_separator(cell.ch).is_some() {
            if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
            text_runs.push(PendingTerminalRowTextRun {
                col: cluster.visual_col,
                text: cluster.text.clone(),
                cells: cluster.cells,
                style,
            });
            continue;
        }

        if let Some(run) = &mut current_run {
            if run.col + run.cells == cluster.visual_col
                && text_run_style_matches(&run.style, &style)
            {
                run.text.push_str(&cluster.text);
                run.cells += cluster.cells;
                run.style.len += cluster.text.len();
                continue;
            }
        }

        if let Some(run) = current_run.take() {
            text_runs.push(run);
        }
        current_run = Some(PendingTerminalRowTextRun {
            col: cluster.visual_col,
            text: cluster.text.clone(),
            cells: cluster.cells,
            style,
        });
    }

    if let Some(run) = current_run.take() {
        text_runs.push(run);
    }
}

fn map_rects_to_visual(
    snapshot: &TerminalSnapshot,
    bidi_enabled: bool,
    rects: Vec<TerminalRect>,
) -> Vec<TerminalRect> {
    let mut mapped = Vec::with_capacity(rects.len());
    for rect in rects {
        let Some(row) = snapshot.lines.get(rect.row) else {
            continue;
        };
        let Some(visual_line) = visual_line_for_row_with_bidi(row, bidi_enabled) else {
            mapped.push(rect);
            continue;
        };

        for range in visual_line.visual_rects_for_logical_range(rect.col..rect.col + rect.cells) {
            mapped.push(TerminalRect {
                row: rect.row,
                col: range.start,
                cells: range.end.saturating_sub(range.start),
                color: rect.color,
            });
        }
    }
    mapped
}

fn cell_text(cell: &oxideterm_terminal::TerminalCell) -> String {
    let mut text = String::with_capacity(cell_text_len(cell));
    push_cell_text(&mut text, cell);
    text
}

fn push_cell_text(text: &mut String, cell: &oxideterm_terminal::TerminalCell) {
    text.push(cell.ch);
    text.push_str(cell.zerowidth());
}

fn cell_text_len(cell: &oxideterm_terminal::TerminalCell) -> usize {
    cell.ch.len_utf8() + cell.zerowidth().len()
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
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
            cache.lock().record_layout_duration(started.elapsed());
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
            let view = input.view.clone();
            let scale_factor = window.scale_factor();
            let viewport_changed = input.last_viewport_bounds != Some(bounds)
                || input.last_viewport_scale_factor_bits != Some(scale_factor.to_bits());
            if viewport_changed {
                // Crossing the entity boundary schedules work for the next frame, so stable
                // terminal paints must not enqueue an update that will immediately return.
                window.on_next_frame(move |_window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.apply_viewport_bounds(bounds, scale_factor, cx);
                    });
                });
            }
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
        let origin = timestamp_origin + point(px(grid_gutter_width), px(0.0));
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
                    for run in &layout.timestamp_runs {
                        paint_text_run(run, timestamp_origin, &self.metrics, window, cx);
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
                for rect in &layout.backgrounds {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for rect in &layout.highlight_backgrounds {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for image in layout
                    .images
                    .iter()
                    .filter(|image| image.image.snapshot.z_index < 0)
                {
                    paint_terminal_image(image, origin, &self.metrics, window);
                }
                for rect in &layout.search_matches {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for overlay in &layout.command_mark_overlays {
                    paint_command_mark_overlay(
                        overlay,
                        origin,
                        self.snapshot.cols,
                        &self.metrics,
                        self.command_mark_gutter_width,
                        window,
                    );
                }
                for rect in &layout.selections {
                    paint_terminal_rect(rect, origin, &self.metrics, window);
                }
                for run in &layout.text_runs {
                    paint_text_run(run, origin, &self.metrics, window, cx);
                }
                if let Some(ghost_text) = &layout.ghost_text {
                    paint_ghost_text_run(ghost_text, origin, &self.metrics, window, cx);
                }
                if let Some(marked_text) = &layout.marked_text {
                    paint_text_run(marked_text, origin, &self.metrics, window, cx);
                }
                for image in layout
                    .images
                    .iter()
                    .filter(|image| image.image.snapshot.z_index >= 0)
                {
                    paint_terminal_image(image, origin, &self.metrics, window);
                }
                for rect in &layout.highlight_underlines {
                    paint_terminal_underline(rect, origin, &self.metrics, window);
                }
                for rect in &layout.highlight_outlines {
                    paint_terminal_outline(rect, origin, &self.metrics, window);
                }
            },
        );
        if let Some(input) = &self.input {
            let content_bounds = terminal_content_bounds_for_rows(
                viewport_origin,
                self.viewport_rows,
                self.snapshot.cols,
                &self.metrics,
            );
            window.handle_input(
                &input.focus_handle,
                TerminalInputHandler {
                    view: input.view.clone(),
                    content_bounds,
                },
                cx,
            );
        }
        if layout.marked_text.is_none()
            && let Some(cursor) = layout.cursor
        {
            window.with_content_mask(
                Some(ContentMask {
                    bounds: viewport_mask_bounds,
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
        if let Some(scrollbar) = layout.scrollbar {
            paint_scrollbar(
                scrollbar,
                viewport_origin,
                grid_viewport_width,
                self.viewport_rows,
                &self.metrics,
                window,
            );
        }
        if let (Some(started), Some(cache)) = (paint_started, &self.layout_cache) {
            cache.lock().record_paint_duration(started.elapsed());
        }
    }
}

#[cfg(test)]
mod cache_tests {
    use std::sync::Arc;

    use gpui::px;
    use oxideterm_terminal::{
        TerminalCell, TerminalColor, TerminalCursorShape, TerminalRow, TerminalSnapshot,
    };

    use super::*;

    fn test_metrics() -> TerminalMetrics {
        TerminalMetrics {
            font: terminal_font_with_family_and_cjk(TERMINAL_FONT, None, TERMINAL_FONT_LIGATURES),
            font_size: px(14.0),
            cell_width: px(8.0),
            line_height: px(10.0),
        }
    }

    fn row_with_text_and_cursor(absolute_line: i64, text: &str, cursor_col: usize) -> TerminalRow {
        let mut cells = text
            .chars()
            .enumerate()
            .map(|(col, ch)| TerminalCell {
                ch,
                wide: false,
                fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
                bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
                style_origin: Default::default(),
                attrs: Default::default(),
                extra: None,
                cursor: col == cursor_col,
            })
            .collect::<Vec<_>>();
        cells.resize_with(cursor_col.saturating_add(1), || TerminalCell {
            ch: ' ',
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: Default::default(),
            attrs: Default::default(),
            extra: None,
            cursor: false,
        });
        if let Some(cursor_cell) = cells.get_mut(cursor_col) {
            cursor_cell.cursor = true;
        }
        let mut row = TerminalRow {
            line_id: absolute_line.max(0) as u64,
            source_id: 0,
            absolute_line,
            cells: Arc::new(cells),
            wrapped: false,
            active_input: true,
            signature: 0,
        };
        row.refresh_signature();
        row
    }

    fn snapshot(display_offset: usize, lines: Vec<TerminalRow>) -> TerminalSnapshot {
        TerminalSnapshot {
            generation: 1,
            cols: 1,
            rows: lines.len(),
            cursor_col: 0,
            cursor_row: 0,
            cursor_shape: TerminalCursorShape::Block,
            display_offset,
            scrollback_lines: display_offset,
            lines,
            images: Vec::new(),
        }
    }

    fn element(snapshot: TerminalSnapshot) -> TerminalElement {
        TerminalElement::new(
            snapshot,
            None,
            test_metrics(),
            true,
            None,
            None,
            Vec::new(),
            None,
            None,
            None,
        )
    }

    #[test]
    fn stable_line_identity_keeps_layout_keys_across_grid_movement() {
        let mut original_row = row_with_text_and_cursor(4, "stable", 5);
        original_row.line_id = 42;
        let mut moved_row = original_row.clone();
        moved_row.absolute_line = 3;
        let original = element(snapshot(0, vec![original_row]));
        let moved = element(snapshot(0, vec![moved_row]));
        let original_lines = original.logical_lines_for_rows(0..1);
        let moved_lines = moved.logical_lines_for_rows(0..1);
        let original_line = original_lines.line_for_row(0).expect("original line");
        let moved_line = moved_lines.line_for_row(0).expect("moved line");

        assert_eq!(
            original.row_layout_cache_key_with_logical_line(0, original_line, None),
            moved.row_layout_cache_key_with_logical_line(0, moved_line, None)
        );
        assert_eq!(
            original.logical_highlight_cache_key_with_logical_line(original_line, None),
            moved.logical_highlight_cache_key_with_logical_line(moved_line, None)
        );
        assert_eq!(original.row_link_cache_key(0), moved.row_link_cache_key(0));
    }

    #[test]
    fn transparent_vim_row_contains_only_current_text_and_cursor() {
        let mut snapshot = snapshot(0, vec![row_with_text_and_cursor(0, "846", 2)]);
        snapshot.cols = 3;
        snapshot.cursor_col = 2;
        snapshot.cursor_shape = TerminalCursorShape::Bar;
        let layout = element(snapshot).transparent_background(true).layout();

        assert!(layout.backgrounds.is_empty());
        assert_eq!(layout.text_runs.len(), 1);
        assert_eq!(layout.text_runs[0].text, "846");
        assert_eq!(layout.text_runs[0].col, 0);
        assert_eq!(layout.text_runs[0].cells, 3);
        assert_eq!(
            layout
                .cursor
                .map(|cursor| (cursor.row, cursor.col, cursor.shape)),
            Some((0, 2, TerminalCursorShape::Bar))
        );
    }
}
