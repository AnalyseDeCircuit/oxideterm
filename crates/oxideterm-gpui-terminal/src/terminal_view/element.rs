use std::ops::Range;

use gpui::{
    App, Bounds, ContentMask, CursorStyle, Element, ElementId, Entity, FocusHandle,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Style, TextRun,
    Window, fill, point, px, relative, rgb,
};
use oxideterm_terminal::{
    TerminalColor, TerminalCursorShape, TerminalSearchMatch, TerminalSnapshot,
};

use crate::app::{TerminalInputHandler, TerminalPane};
use crate::terminal_ui::*;
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
    selection: Option<TerminalSelection>,
    metrics: TerminalMetrics,
    cursor_visible: bool,
    marked_text: Option<String>,
    search_query: Option<String>,
    search_matches: Vec<TerminalSearchMatch>,
    selected_search_match: Option<usize>,
    hovered_link: Option<TerminalLinkRange>,
    input: Option<TerminalElementInput>,
}

#[derive(Clone)]
pub(crate) struct TerminalElementInput {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) view: Entity<TerminalPane>,
}

#[allow(dead_code)]
pub(crate) struct TerminalElementLayout {
    pub(crate) backgrounds: Vec<TerminalRect>,
    pub(crate) search_matches: Vec<TerminalRect>,
    pub(crate) selections: Vec<TerminalRect>,
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
        Self {
            snapshot,
            selection,
            metrics,
            cursor_visible,
            marked_text,
            search_query,
            search_matches,
            selected_search_match,
            hovered_link,
            input,
        }
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
        let search_matches = if self.search_matches.is_empty() {
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
        };
        let mut selections = Vec::new();
        let mut text_runs = Vec::new();
        let mut cursor = None;
        let scrollbar = terminal_scrollbar(&self.snapshot, &self.metrics);
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

            for (col_index, cell) in row.cells.iter().enumerate() {
                if self.cursor_visible
                    && cell.cursor
                    && self.snapshot.cursor_shape != TerminalCursorShape::Hidden
                {
                    cursor = Some(TerminalCursor {
                        row: row_index,
                        col: col_index,
                        shape: self.snapshot.cursor_shape,
                    });
                }

                let block_cursor = self.cursor_visible
                    && cell.cursor
                    && self.snapshot.cursor_shape == TerminalCursorShape::Block;
                let fg = if block_cursor {
                    to_hsla(TerminalColor::rgb(0x0d, 0x0f, 0x12))
                } else {
                    to_hsla(cell.fg)
                };
                let bg = if block_cursor {
                    to_hsla(TerminalColor::rgb(0x52, 0x8b, 0xff))
                } else {
                    to_hsla(cell.bg)
                };
                let cell_width = if cell.wide { 2 } else { 1 };

                if bg != terminal_background() {
                    extend_or_push_rect(
                        &mut current_background,
                        &mut backgrounds,
                        row_index,
                        col_index,
                        cell_width,
                        bg,
                    );
                } else if let Some(rect) = current_background.take() {
                    backgrounds.push(rect);
                }

                if self
                    .selection
                    .is_some_and(|selection| selection.contains(row_index, col_index))
                {
                    extend_or_push_rect(
                        &mut current_selection,
                        &mut selections,
                        row_index,
                        col_index,
                        cell_width,
                        to_hsla(TerminalColor::rgb(0x2d, 0x4f, 0x7f)),
                    );
                } else if let Some(rect) = current_selection.take() {
                    selections.push(rect);
                }

                if cell.ch != ' ' || (self.cursor_visible && cell.cursor) {
                    let link = !block_cursor
                        && (cell.hyperlink.is_some() || is_link_stylable_cell(cell))
                        && link_ranges_contain(&link_ranges, row_index, col_index);
                    let style = text_run_for_cell(cell, fg, link, &self.metrics);
                    if powerline_separator(cell.ch).is_some() {
                        if let Some(run) = current_run.take() {
                            text_runs.push(run);
                        }
                        text_runs.push(BatchedTextRun {
                            row: row_index,
                            col: col_index,
                            text: cell.ch.to_string(),
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
                            run.text.push(cell.ch);
                            run.cells += cell_width;
                            run.style.len += cell.ch.len_utf8();
                            continue;
                        }
                    }

                    if let Some(run) = current_run.take() {
                        text_runs.push(run);
                    }
                    current_run = Some(BatchedTextRun {
                        row: row_index,
                        col: col_index,
                        text: cell.ch.to_string(),
                        cells: cell_width,
                        style,
                    });
                } else if let Some(run) = current_run.take() {
                    text_runs.push(run);
                }
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
            search_matches,
            selections,
            text_runs,
            marked_text: self.marked_text.as_ref().and_then(|text| {
                ime_cursor_bounds?;
                Some(BatchedTextRun {
                    row: self.snapshot.cursor_row,
                    col: self.snapshot.cursor_col,
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

        window.paint_quad(fill(bounds, rgb(OXIDETERM_TERMINAL_BACKGROUND)));
        let origin =
            bounds.origin + point(px(TERMINAL_CONTENT_PADDING), px(TERMINAL_CONTENT_PADDING));

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for rect in &layout.backgrounds {
                paint_terminal_rect(rect, origin, &self.metrics, window);
            }
            for rect in &layout.search_matches {
                paint_terminal_rect(rect, origin, &self.metrics, window);
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
                paint_cursor(cursor, origin, &self.metrics, window);
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
