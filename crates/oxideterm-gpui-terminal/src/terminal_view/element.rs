use std::ops::Range;

use gpui::{
    App, Bounds, ContentMask, CursorStyle, Element, ElementId, Entity, FocusHandle,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Style, TextRun,
    Window, fill, point, px, relative, rgb,
};
use oxideterm_terminal::{
    TerminalColor, TerminalCommandMark, TerminalCursorShape, TerminalSearchMatch, TerminalSnapshot,
};
use oxideterm_terminal_unicode::{TerminalVisualLine, visual_line_for_row};

use crate::app::{TerminalInputHandler, TerminalPane, TerminalRenderedImage};
use crate::terminal_ui::*;
use crate::terminal_view::highlight::{TerminalHighlightLayout, terminal_highlights_for_rows};
use crate::terminal_view::links::*;
use crate::terminal_view::selection::TerminalSelection;

mod layout;
mod paint;
mod style;

pub(crate) use layout::*;
#[cfg(test)]
pub(crate) use paint::powerline_separator_points;
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
    search_query: Option<String>,
    search_matches: Vec<TerminalSearchMatch>,
    selected_search_match: Option<usize>,
    command_marks: Vec<TerminalCommandMark>,
    selected_command_mark_id: Option<String>,
    highlight_rules: Vec<TerminalHighlightRule>,
    hovered_link: Option<TerminalLinkRange>,
    bidi_enabled: bool,
    input: Option<TerminalElementInput>,
    transparent_background: bool,
}

#[derive(Clone)]
pub(crate) struct TerminalElementInput {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) view: Entity<TerminalPane>,
}

#[allow(dead_code)]
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
    pub(crate) marked_text: Option<BatchedTextRun>,
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
    pub(crate) text: String,
    pub(crate) cells: usize,
    pub(crate) style: TextRun,
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

impl TerminalElement {
    #[allow(dead_code)]
    pub(crate) fn new(
        snapshot: TerminalSnapshot,
        selection: Option<TerminalSelection>,
        metrics: TerminalMetrics,
        cursor_visible: bool,
        marked_text: Option<String>,
        search_query: Option<String>,
        search_matches: Vec<TerminalSearchMatch>,
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
        search_matches: Vec<TerminalSearchMatch>,
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
        search_matches: Vec<TerminalSearchMatch>,
        selected_search_match: Option<usize>,
        hovered_link: Option<TerminalLinkRange>,
        bidi_enabled: bool,
        input: Option<TerminalElementInput>,
    ) -> Self {
        Self {
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
            command_marks: Vec::new(),
            selected_command_mark_id: None,
            highlight_rules: Vec::new(),
            hovered_link,
            bidi_enabled,
            input,
            transparent_background: false,
        }
    }

    pub(crate) fn highlight_rules(mut self, rules: Vec<TerminalHighlightRule>) -> Self {
        self.highlight_rules = rules;
        self
    }

    pub(crate) fn command_marks(
        mut self,
        marks: Vec<TerminalCommandMark>,
        selected_command_mark_id: Option<String>,
    ) -> Self {
        self.command_marks = marks;
        self.selected_command_mark_id = selected_command_mark_id;
        self
    }

    pub(crate) fn transparent_background(mut self, transparent_background: bool) -> Self {
        self.transparent_background = transparent_background;
        self
    }

    #[allow(dead_code)]
    pub(crate) fn layout(&self) -> TerminalElementLayout {
        self.layout_for_rows(0..self.snapshot.rows)
    }

    pub(crate) fn layout_for_bounds(&self, bounds: Bounds<Pixels>) -> TerminalElementLayout {
        self.layout_for_rows(terminal_visible_rows(bounds, &self.snapshot, &self.metrics))
    }

    fn layout_for_rows(&self, visible_rows: Range<usize>) -> TerminalElementLayout {
        let mut backgrounds = Vec::new();
        let highlight_layout = terminal_highlights_for_rows(
            &self.snapshot,
            &self.highlight_rules,
            visible_rows.clone(),
        );
        let search_matches = map_rects_to_visual(
            &self.snapshot,
            self.bidi_enabled,
            if self.search_matches.is_empty() {
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
        );
        let mut selections = Vec::new();
        let mut images = self
            .rendered_images
            .iter()
            .filter(|image| {
                image.snapshot.row < self.snapshot.rows
                    && image.snapshot.row + image.snapshot.rows > visible_rows.start
                    && image.snapshot.row < visible_rows.end
            })
            .cloned()
            .map(|image| TerminalImageLayout { image })
            .collect::<Vec<_>>();
        images.sort_by_key(|image| (image.image.snapshot.z_index, image.image.snapshot.id.0));
        let mut text_runs = Vec::new();
        let mut cursor = None;
        let scrollbar = terminal_scrollbar(&self.snapshot, &self.metrics);
        let terminal_background = terminal_background(&self.theme);
        let cursor_row_visible = visible_rows.contains(&self.snapshot.cursor_row);
        let ime_cursor_bounds = cursor_row_visible
            .then(|| ime_cursor_bounds_for_snapshot(&self.snapshot, &self.metrics))
            .flatten();
        let link_ranges = display_link_ranges_for_rows(&self.snapshot, visible_rows.clone());
        let mut current_run: Option<BatchedTextRun> = None;

        for row_index in visible_rows {
            let Some(row) = self.snapshot.lines.get(row_index) else {
                continue;
            };
            let mut current_background: Option<TerminalRect> = None;
            let mut current_selection: Option<TerminalRect> = None;
            let visual_line = visual_line_for_row_with_bidi(row, self.bidi_enabled);

            for (col_index, cell) in row.cells.iter().enumerate() {
                let paint_col = if visual_line.has_bidi {
                    visual_line.visual_col_for_logical_col(col_index)
                } else {
                    col_index
                };
                if self.cursor_visible
                    && cell.cursor
                    && self.snapshot.cursor_shape != TerminalCursorShape::Hidden
                {
                    cursor = Some(TerminalCursor {
                        row: row_index,
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
                    to_hsla(cell.fg)
                };
                let bg = if block_cursor {
                    to_hsla(terminal_color_from_hex(self.theme.header_foreground))
                } else {
                    to_hsla(cell.bg)
                };
                let cell_width = if cell.wide { 2 } else { 1 };

                if bg != terminal_background {
                    extend_or_push_rect(
                        &mut current_background,
                        &mut backgrounds,
                        row_index,
                        paint_col,
                        cell_width,
                        bg,
                    );
                } else if let Some(rect) = current_background.take() {
                    backgrounds.push(rect);
                }

                if self.selection.is_some_and(|selection| {
                    selection.contains_viewport_cell(
                        row_index,
                        col_index,
                        self.snapshot.display_offset,
                    )
                }) {
                    extend_or_push_rect(
                        &mut current_selection,
                        &mut selections,
                        row_index,
                        paint_col,
                        cell_width,
                        to_hsla(TerminalColor::rgb(0x2d, 0x4f, 0x7f)),
                    );
                } else if let Some(rect) = current_selection.take() {
                    selections.push(rect);
                }

                if visual_line.has_bidi {
                    continue;
                }

                if cell.ch != ' '
                    || !cell.zerowidth.is_empty()
                    || (self.cursor_visible && cell.cursor)
                {
                    let link = !block_cursor
                        && (cell.hyperlink.is_some() || is_link_stylable_cell(cell))
                        && link_ranges_contain(&link_ranges, row_index, col_index);
                    let style = text_run_for_cell(cell, fg, link, &self.metrics);
                    let cell_text = cell_text(cell);
                    if cell.zerowidth.is_empty() && powerline_separator(cell.ch).is_some() {
                        if let Some(run) = current_run.take() {
                            text_runs.push(run);
                        }
                        text_runs.push(BatchedTextRun {
                            row: row_index,
                            col: col_index,
                            text: cell_text,
                            cells: cell_width,
                            style,
                        });
                        continue;
                    }
                    if let Some(run) = &mut current_run {
                        if run.row == row_index
                            && run.col + run.cells == col_index
                            && text_run_style_matches(&run.style, &style)
                        {
                            run.text.push_str(&cell_text);
                            run.cells += cell_width;
                            run.style.len += cell_text.len();
                            continue;
                        }
                    }

                    if let Some(run) = current_run.take() {
                        text_runs.push(run);
                    }
                    current_run = Some(BatchedTextRun {
                        row: row_index,
                        col: col_index,
                        text: cell_text,
                        cells: cell_width,
                        style,
                    });
                } else if let Some(run) = current_run.take() {
                    text_runs.push(run);
                }
            }

            if visual_line.has_bidi {
                if let Some(run) = current_run.take() {
                    text_runs.push(run);
                }
                push_visual_text_runs(
                    row_index,
                    row,
                    &visual_line,
                    &link_ranges,
                    &self.metrics,
                    self.cursor_visible,
                    self.snapshot.cursor_shape,
                    &self.theme,
                    &highlight_layout,
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
            marked_text: self.marked_text.as_ref().and_then(|text| {
                ime_cursor_bounds?;
                let marked_col = self
                    .snapshot
                    .lines
                    .get(self.snapshot.cursor_row)
                    .map(|row| visual_line_for_row_with_bidi(row, self.bidi_enabled))
                    .filter(|line| line.has_bidi)
                    .map(|line| line.visual_col_for_logical_col(self.snapshot.cursor_col))
                    .unwrap_or(self.snapshot.cursor_col);
                Some(BatchedTextRun {
                    row: self.snapshot.cursor_row,
                    col: marked_col,
                    text: text.clone(),
                    cells: text.encode_utf16().count().max(1),
                    style: marked_text_run(text, &self.metrics),
                })
            }),
            ime_cursor_bounds,
            cursor,
            scrollbar,
        }
    }
}

fn visual_line_for_row_with_bidi(
    row: &oxideterm_terminal::TerminalRow,
    bidi_enabled: bool,
) -> TerminalVisualLine {
    if bidi_enabled {
        visual_line_for_row(row)
    } else {
        TerminalVisualLine::identity(row)
    }
}

fn command_mark_overlays_for_rows(
    snapshot: &TerminalSnapshot,
    marks: &[TerminalCommandMark],
    selected_command_mark_id: Option<&str>,
) -> Vec<TerminalCommandMarkOverlay> {
    let Some(selected_id) = selected_command_mark_id else {
        return Vec::new();
    };
    let Some(mark) = marks.iter().find(|mark| mark.command_id == selected_id) else {
        return Vec::new();
    };
    let start_line = mark.start_line;
    let end_line = mark.end_line.unwrap_or_else(|| {
        snapshot_prompt_block_start_line(snapshot, snapshot_absolute_cursor_line(snapshot))
            .saturating_sub(1)
            .max(mark.start_line)
    });
    if end_line < start_line {
        return Vec::new();
    }

    let viewport_start = snapshot
        .scrollback_lines
        .saturating_sub(snapshot.display_offset);
    let viewport_end = viewport_start.saturating_add(snapshot.rows.saturating_sub(1));
    if end_line < viewport_start || start_line > viewport_end {
        return Vec::new();
    }

    let visible_start_line = start_line.max(viewport_start);
    let visible_end_line = end_line.min(viewport_end);
    vec![TerminalCommandMarkOverlay {
        start_row: visible_start_line.saturating_sub(viewport_start),
        end_row: visible_end_line.saturating_sub(viewport_start),
        has_top: start_line >= viewport_start,
        has_bottom: end_line <= viewport_end,
        stale: mark.stale,
    }]
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
    row_index: usize,
    row: &oxideterm_terminal::TerminalRow,
    visual_line: &TerminalVisualLine,
    link_ranges: &[TerminalLinkRange],
    metrics: &TerminalMetrics,
    cursor_visible: bool,
    cursor_shape: TerminalCursorShape,
    theme: &TerminalUiTheme,
    highlight_layout: &TerminalHighlightLayout,
    text_runs: &mut Vec<BatchedTextRun>,
) {
    let mut current_run: Option<BatchedTextRun> = None;
    for cluster in &visual_line.clusters {
        let Some(cell) = row.cells.get(cluster.logical_col) else {
            continue;
        };
        if cell.ch == ' ' && cell.zerowidth.is_empty() {
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
            highlight_layout.foreground_for_cell(row_index, cluster.logical_col)
        {
            highlight_fg
        } else {
            to_hsla(cell.fg)
        };
        let link = !block_cursor
            && (cell.hyperlink.is_some() || is_link_stylable_cell(cell))
            && link_ranges_contain(link_ranges, row_index, cluster.logical_col);
        let style = text_run_for_cell(cell, fg, link, metrics);
        if cell.zerowidth.is_empty() && powerline_separator(cell.ch).is_some() {
            if let Some(run) = current_run.take() {
                text_runs.push(run);
            }
            text_runs.push(BatchedTextRun {
                row: row_index,
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
        current_run = Some(BatchedTextRun {
            row: row_index,
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
        let visual_line = visual_line_for_row_with_bidi(row, bidi_enabled);
        if !visual_line.has_bidi {
            mapped.push(rect);
            continue;
        }

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
    if cell.zerowidth.is_empty() {
        cell.ch.to_string()
    } else {
        let mut text = String::with_capacity(cell.ch.len_utf8() + cell.zerowidth.len());
        text.push(cell.ch);
        text.push_str(&cell.zerowidth);
        text
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalElementLayout;

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
        self.layout_for_bounds(bounds)
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
        if let Some(input) = &self.input {
            let view = input.view.clone();
            let scale_factor = window.scale_factor();
            window.on_next_frame(move |_window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.apply_viewport_bounds(bounds, scale_factor, cx);
                });
            });
        }
        if self.hovered_link.is_some() {
            window.set_window_cursor_style(CursorStyle::PointingHand);
        }

        if !self.transparent_background {
            window.paint_quad(fill(bounds, rgb(self.theme.background)));
        }
        let origin =
            bounds.origin + point(px(TERMINAL_CONTENT_PADDING), px(TERMINAL_CONTENT_PADDING));

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
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
                    window,
                );
            }
            for rect in &layout.selections {
                paint_terminal_rect(rect, origin, &self.metrics, window);
            }
            for run in &layout.text_runs {
                paint_text_run(run, origin, &self.metrics, window, cx);
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
        });
        if let Some(input) = &self.input {
            let content_bounds = terminal_content_bounds(origin, &self.snapshot, &self.metrics);
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
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                paint_cursor(
                    cursor,
                    origin,
                    &self.metrics,
                    self.theme.header_foreground,
                    window,
                );
            });
        }
        if let Some(scrollbar) = layout.scrollbar {
            paint_scrollbar(
                scrollbar,
                origin,
                self.snapshot.cols,
                self.snapshot.rows,
                &self.metrics,
                window,
            );
        }
    }
}
