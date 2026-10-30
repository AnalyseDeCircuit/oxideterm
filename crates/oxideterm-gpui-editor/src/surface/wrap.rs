// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use gpui::Pixels;
use oxideterm_editor_core::TextRange;
use unicode_segmentation::UnicodeSegmentation;

use super::{DisplayRowsCache, TextEditorView, coords::grapheme_visual_width};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DisplayRow {
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub is_first: bool,
    pub is_folded_header: bool,
}

/// Ordinary rows have implicit line numbers and flags; retain only their widths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum DisplayRows {
    Unwrapped(Vec<u32>),
    Wrapped {
        widths: Vec<u32>,
        wraps: Vec<WrappedLine>,
    },
    Folded {
        base: Box<DisplayRows>,
        folds: Vec<FoldedRows>,
    },
    Explicit(Vec<DisplayRow>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FoldedRows {
    line: usize,
    end_line: usize,
    first_row: usize,
    hidden_before: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WrappedLine {
    line: usize,
    first_row: usize,
    ends: Box<[usize]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AdvanceRun {
    count: u32,
    columns: u32,
    advance: f32,
}

#[derive(Clone, Debug)]
enum LineMeasure {
    Uniform(AdvanceRun),
    Runs(Arc<[AdvanceRun]>),
    Uncached,
}

impl LineMeasure {
    fn runs(&self) -> &[AdvanceRun] {
        match self {
            Self::Uniform(run) => std::slice::from_ref(run),
            Self::Runs(runs) => runs,
            Self::Uncached => &[],
        }
    }

    fn heap_bytes(&self) -> usize {
        match self {
            Self::Runs(runs) => std::mem::size_of_val(&**runs),
            _ => 0,
        }
    }
}

#[derive(Default)]
pub(super) struct WrapMeasurements {
    version: u64,
    font: Option<(gpui::Font, f32, usize)>,
    lines: Vec<LineMeasure>,
    bytes: usize,
}

impl DisplayRows {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Unwrapped(widths) => widths.len(),
            Self::Wrapped { widths, wraps } => {
                widths.len()
                    + wraps
                        .last()
                        .map_or(0, |line| line.first_row - line.line + line.ends.len() - 1)
            }
            Self::Explicit(rows) => rows.len(),
            Self::Folded { base, folds } => {
                base.len()
                    - folds
                        .last()
                        .map_or(0, |fold| fold.hidden_before + fold.end_line - fold.line)
            }
        }
    }

    pub(super) fn get(&self, index: usize) -> Option<DisplayRow> {
        match self {
            Self::Unwrapped(widths) => widths.get(index).map(|&width| DisplayRow {
                line: index,
                start_col: 0,
                end_col: width as usize,
                is_first: true,
                is_folded_header: false,
            }),
            Self::Wrapped { widths, wraps } => {
                let previous = wraps
                    .partition_point(|line| line.first_row <= index)
                    .checked_sub(1)
                    .map(|i| &wraps[i]);
                let extra = if let Some(line) = previous {
                    let part = index - line.first_row;
                    if part < line.ends.len() {
                        return Some(DisplayRow {
                            line: line.line,
                            start_col: part.checked_sub(1).map_or(0, |i| line.ends[i]),
                            end_col: line.ends[part],
                            is_first: part == 0,
                            is_folded_header: false,
                        });
                    }
                    line.first_row - line.line + line.ends.len() - 1
                } else {
                    0
                };
                let line = index.checked_sub(extra)?;
                widths.get(line).map(|&width| DisplayRow {
                    line,
                    start_col: 0,
                    end_col: width as usize,
                    is_first: true,
                    is_folded_header: false,
                })
            }
            Self::Explicit(rows) => rows.get(index).copied(),
            Self::Folded { base, folds } => {
                let fold = folds
                    .partition_point(|fold| fold.first_row <= index)
                    .checked_sub(1)
                    .map(|i| &folds[i]);
                let hidden = fold.map_or(0, |fold| {
                    fold.hidden_before
                        + if index == fold.first_row {
                            0
                        } else {
                            fold.end_line - fold.line
                        }
                });
                let mut row = base.get(index.checked_add(hidden)?)?;
                row.is_folded_header = fold.is_some_and(|fold| fold.first_row == index);
                Some(row)
            }
        }
    }

    pub(super) fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = DisplayRow> + ExactSizeIterator + '_ {
        (0..self.len()).map(|index| self.get(index).expect("row index is in bounds"))
    }

    fn max_width(&self) -> usize {
        match self {
            Self::Unwrapped(widths) => widths.iter().copied().max().unwrap_or(0) as usize,
            Self::Folded { base, .. } => base.max_width(),
            Self::Wrapped { .. } => self
                .iter()
                .map(|row| row.end_col - row.start_col)
                .max()
                .unwrap_or(0),
            Self::Explicit(rows) => rows
                .iter()
                .map(|row| row.end_col.saturating_sub(row.start_col))
                .max()
                .unwrap_or(0),
        }
    }
}

pub(super) struct DisplayRowEdit {
    cache: DisplayRowsCache,
    first_line: usize,
    last_line: usize,
    newlines: usize,
}

impl TextEditorView {
    pub(super) fn prepare_row_edit(
        &self,
        range: TextRange,
        replacement: &str,
    ) -> Option<DisplayRowEdit> {
        let first_line = self.buffer.offset_to_line_col(range.start).ok()?.line;
        let last_line = self.buffer.offset_to_line_col(range.end).ok()?.line;
        let mut cached = self.display_rows_cache.borrow_mut();
        let cache = cached.as_ref()?;
        if !self.folded_ranges.is_empty()
            || cache.buffer_version != self.buffer.version()
            || cache.wrap_width != self.wrap_width()
        {
            return None;
        }
        Some(DisplayRowEdit {
            cache: cached.take()?,
            first_line,
            last_line,
            newlines: replacement.bytes().filter(|b| *b == b'\n').count(),
        })
    }

    pub(super) fn restore_rows_after_edit(&self, edit: Option<DisplayRowEdit>) {
        let Some(mut edit) = edit else {
            return;
        };
        if let Some(width) = self.wrap_width() {
            self.update_wrap_measurements(&edit);
            let Some((first_row, _, _)) =
                display_row_for_visual_column(&edit.cache.rows, edit.first_line, 0)
            else {
                return;
            };
            let Some((last_row, _, _)) =
                display_row_for_visual_column(&edit.cache.rows, edit.last_line, usize::MAX)
            else {
                return;
            };
            let last_row = last_row + 1;
            let mut new_widths = Vec::new();
            let mut new_wraps = Vec::new();
            let mut next_row = first_row;
            let mut new_max = 0;
            for line in edit.first_line..=edit.first_line + edit.newlines {
                let rows = self.wrapped_line_rows(line, width);
                new_widths.push(rows.last().unwrap().end_col as u32);
                new_max = new_max.max(
                    rows.iter()
                        .map(|row| row.end_col - row.start_col)
                        .max()
                        .unwrap_or(0),
                );
                if rows.len() > 1 {
                    new_wraps.push(WrappedLine {
                        line,
                        first_row: next_row,
                        ends: rows.iter().map(|row| row.end_col).collect(),
                    });
                }
                next_row += rows.len();
            }
            let DisplayRows::Wrapped { widths, wraps } = Arc::make_mut(&mut edit.cache.rows) else {
                return;
            };
            widths.splice(edit.first_line..=edit.last_line, new_widths);
            let first = wraps.partition_point(|line| line.line < edit.first_line);
            let end = wraps.partition_point(|line| line.line <= edit.last_line);
            let line_delta = edit.newlines as isize - (edit.last_line - edit.first_line) as isize;
            let row_delta = next_row as isize - last_row as isize;
            for line in &mut wraps[end..] {
                line.line = line.line.checked_add_signed(line_delta).unwrap();
                line.first_row = line.first_row.checked_add_signed(row_delta).unwrap();
            }
            wraps.splice(first..end, new_wraps);
            if widths.capacity() > widths.len().saturating_mul(2) {
                widths.shrink_to_fit();
            }
            if wraps.capacity() > wraps.len().saturating_mul(2) {
                wraps.shrink_to_fit();
            }
            edit.cache.max_width_columns = if new_max >= edit.cache.max_width_columns {
                new_max
            } else {
                edit.cache.rows.max_width()
            };
            edit.cache.buffer_version = self.buffer.version();
            edit.cache.fold_revision = self.fold_revision;
            *self.display_rows_cache.borrow_mut() = Some(edit.cache);
            return;
        }
        let DisplayRows::Unwrapped(widths) = Arc::make_mut(&mut edit.cache.rows) else {
            return;
        };
        let new_widths = (edit.first_line..edit.first_line + edit.newlines + 1)
            .map(|line| {
                u32::try_from(
                    self.buffer
                        .with_line_text(line, unwrapped_line_width)
                        .unwrap_or(0),
                )
            })
            .collect::<Result<Vec<_>, _>>();
        let Ok(new_widths) = new_widths else {
            // A wider row uses the existing explicit layout on the next rebuild.
            return;
        };
        let removed_widest = widths[edit.first_line..=edit.last_line]
            .contains(&(edit.cache.max_width_columns as u32));
        let new_max = new_widths.iter().copied().max().unwrap_or(0) as usize;
        widths.splice(edit.first_line..=edit.last_line, new_widths);
        if widths.capacity() > widths.len().saturating_mul(2) {
            widths.shrink_to(widths.len());
        }
        edit.cache.max_width_columns = if removed_widest && new_max < edit.cache.max_width_columns {
            widths.iter().copied().max().unwrap_or(0) as usize
        } else {
            edit.cache.max_width_columns.max(new_max)
        };
        edit.cache.buffer_version = self.buffer.version();
        edit.cache.fold_revision = self.fold_revision;
        *self.display_rows_cache.borrow_mut() = Some(edit.cache);
    }

    pub(super) fn display_row_for_window_y(&self, y: Pixels) -> Option<DisplayRow> {
        let bounds = self.content_bounds?;
        let relative_y = f32::from(y - bounds.origin.y) + self.vertical_scroll_y_px();
        let display_index = (relative_y / self.metrics.line_height).floor().max(0.0) as usize;
        self.display_rows().get(display_index)
    }

    pub(super) fn document_row_count(&self) -> usize {
        self.display_rows().len().max(1)
    }

    pub(super) fn display_rows(&self) -> Arc<DisplayRows> {
        let wrap_width = self.wrap_width();
        if wrap_width.is_none() {
            *self.wrap_measurements.borrow_mut() = WrapMeasurements::default();
        }
        let buffer_version = self.buffer.version();
        if let Some(cache) = self.display_rows_cache.borrow().as_ref()
            && cache.buffer_version == buffer_version
            && cache.wrap_width == wrap_width
            && cache.fold_revision == self.fold_revision
        {
            return cache.rows.clone();
        }

        let rows = Arc::new(self.compute_display_rows(wrap_width));
        let max_width_columns = rows.max_width();
        *self.display_rows_cache.borrow_mut() = Some(DisplayRowsCache {
            buffer_version,
            wrap_width,
            fold_revision: self.fold_revision,
            max_width_columns,
            rows: rows.clone(),
        });
        rows
    }

    pub(super) fn document_width_columns(&self) -> usize {
        // Populate the shared row cache once, then reuse its width summary on every scroll frame.
        let _ = self.display_rows();
        self.display_rows_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.max_width_columns)
            .unwrap_or(0)
    }

    fn compute_display_rows(&self, wrap_width: Option<f32>) -> DisplayRows {
        if !self.folded_ranges.is_empty()
            && let Some(rows) = self.compact_folded_rows(wrap_width)
        {
            return rows;
        }
        if let Some(width) = wrap_width {
            self.ensure_wrap_measurements();
            if self.folded_ranges.is_empty() {
                let mut widths = Vec::with_capacity(self.buffer.line_count());
                let mut wraps = Vec::new();
                let mut extra = 0;
                for line in 0..self.buffer.line_count() {
                    let rows = self.wrapped_line_rows(line, width);
                    widths.push(
                        u32::try_from(rows.last().unwrap().end_col)
                            .expect("wrapped document width"),
                    );
                    if rows.len() > 1 {
                        wraps.push(WrappedLine {
                            line,
                            first_row: line + extra,
                            ends: rows.iter().map(|row| row.end_col).collect(),
                        });
                        extra += rows.len() - 1;
                    }
                }
                return DisplayRows::Wrapped { widths, wraps };
            }
        }
        if wrap_width.is_none() && self.folded_ranges.is_empty() {
            let mut widths = Vec::with_capacity(self.buffer.line_count());
            for line in 0..self.buffer.line_count() {
                let width = self
                    .buffer
                    .with_line_text(line, unwrapped_line_width)
                    .unwrap_or(0);
                let Ok(width) = u32::try_from(width) else {
                    break;
                };
                widths.push(width);
            }
            if widths.len() == self.buffer.line_count() {
                return DisplayRows::Unwrapped(widths);
            }
        }
        // Share the renderer's font fallback and shaping rules. This temporary
        // cache belongs to one layout rebuild; scroll frames reuse DisplayRows.
        let mut rows = Vec::new();
        let mut line = 0;
        while line < self.buffer.line_count() {
            let folded = self
                .folded_ranges
                .iter()
                .find(|range| range.start_line == line)
                .copied();
            let line_wrap_width = if folded.is_some() { None } else { wrap_width };
            self.buffer
                .with_line_text(line, |text| {
                    if line_wrap_width.is_none() {
                        rows.push(DisplayRow {
                            line,
                            start_col: 0,
                            end_col: unwrapped_line_width(text),
                            is_first: true,
                            is_folded_header: folded.is_some(),
                        });
                    } else {
                        rows.extend(self.wrapped_line_rows(line, line_wrap_width.unwrap()));
                    }
                })
                .unwrap_or_else(|| {
                    append_display_rows_for_line(
                        &mut rows,
                        line,
                        std::iter::empty(),
                        line_wrap_width,
                        folded.is_some(),
                    );
                });
            line = folded
                .map(|range| range.end_line.saturating_add(1))
                .unwrap_or_else(|| line + 1);
        }
        DisplayRows::Explicit(rows)
    }

    fn compact_folded_rows(&self, wrap_width: Option<f32>) -> Option<DisplayRows> {
        if wrap_width.is_some() {
            self.ensure_wrap_measurements();
        }
        // Hidden physical lines keep implicit positions, not individual display-row records.
        let mut widths = vec![0; self.buffer.line_count()];
        let mut wraps = Vec::new();
        let mut folds = Vec::with_capacity(self.folded_ranges.len());
        let mut line = 0;
        let mut extra = 0;
        let mut hidden = 0;
        while line < self.buffer.line_count() {
            if let Some(fold) = self
                .folded_ranges
                .get(folds.len())
                .filter(|fold| fold.start_line == line)
            {
                widths[line] = u32::try_from(
                    self.buffer
                        .with_line_text(line, unwrapped_line_width)
                        .unwrap_or(0),
                )
                .ok()?;
                folds.push(FoldedRows {
                    line,
                    end_line: fold.end_line,
                    first_row: line + extra - hidden,
                    hidden_before: hidden,
                });
                hidden += fold.end_line - line;
                line = fold.end_line + 1;
            } else {
                if let Some(width) = wrap_width {
                    let rows = self.wrapped_line_rows(line, width);
                    widths[line] = u32::try_from(rows.last().unwrap().end_col).ok()?;
                    if rows.len() > 1 {
                        wraps.push(WrappedLine {
                            line,
                            first_row: line + extra,
                            ends: rows.iter().map(|row| row.end_col).collect(),
                        });
                        extra += rows.len() - 1;
                    }
                } else {
                    widths[line] = u32::try_from(
                        self.buffer
                            .with_line_text(line, unwrapped_line_width)
                            .unwrap_or(0),
                    )
                    .ok()?;
                }
                line += 1;
            }
        }
        let base = if wrap_width.is_some() {
            DisplayRows::Wrapped { widths, wraps }
        } else {
            DisplayRows::Unwrapped(widths)
        };
        Some(DisplayRows::Folded {
            base: Box::new(base),
            folds,
        })
    }

    fn measure_wrap_line(&self, line: usize) -> LineMeasure {
        self.measure_wrap_line_with_system(
            line,
            &gpui::WindowTextSystem::new(self.text_system.clone()),
        )
    }

    fn measure_wrap_line_with_system(
        &self,
        line: usize,
        system: &gpui::WindowTextSystem,
    ) -> LineMeasure {
        self.buffer
            .with_line_text(line, |text| {
                let shaped = self.shape_coordinate_line(text, system);
                let ordered = shaped
                    .runs
                    .iter()
                    .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.index))
                    .is_sorted();
                let mut glyphs = shaped
                    .runs
                    .iter()
                    .flat_map(|run| run.glyphs.iter())
                    .peekable();
                let mut runs: Vec<AdvanceRun> = Vec::new();
                let mut previous_x = 0.0;
                for (start, grapheme) in text.grapheme_indices(true) {
                    let end = start + grapheme.len();
                    let x = if ordered {
                        while glyphs.peek().is_some_and(|glyph| glyph.index < end) {
                            glyphs.next();
                        }
                        glyphs.peek().map_or(f32::from(shaped.width()), |glyph| {
                            f32::from(glyph.position.x)
                        })
                    } else {
                        f32::from(shaped.x_for_index(end))
                    };
                    let advance = x - previous_x;
                    previous_x = x;
                    let columns =
                        u32::try_from(grapheme_visual_width(grapheme)).expect("grapheme columns");
                    if let Some(last) = runs.last_mut()
                        && last.columns == columns
                        && last.advance == advance
                    {
                        last.count += 1;
                    } else {
                        runs.push(AdvanceRun {
                            count: 1,
                            columns,
                            advance,
                        });
                    }
                }
                if runs.len() <= 1 {
                    LineMeasure::Uniform(runs.first().copied().unwrap_or(AdvanceRun {
                        count: 0,
                        columns: 1,
                        advance: 0.0,
                    }))
                } else {
                    LineMeasure::Runs(runs.into())
                }
            })
            .unwrap()
    }

    fn ensure_wrap_measurements(&self) {
        let font = (
            crate::metrics::editor_code_font(
                &self.appearance.font_family,
                self.appearance.font_fallback_family.as_deref(),
                self.appearance.font_weight,
            ),
            self.metrics.font_size,
            self.text_system.font_generation(),
        );
        let mut cache = self.wrap_measurements.borrow_mut();
        if cache.version == self.buffer.version() && cache.font.as_ref() == Some(&font) {
            return;
        }
        cache.lines = Vec::with_capacity(self.buffer.line_count());
        cache.bytes = 0;
        let mut system = gpui::WindowTextSystem::new(self.text_system.clone());
        let mut measured_bytes = 0;
        for line in 0..self.buffer.line_count() {
            let measure = self.measure_wrap_line_with_system(line, &system);
            cache.retain_measure(measure);
            measured_bytes += self.buffer.line_end_offset(line).unwrap().0
                - self.buffer.line_start_offset(line).unwrap().0;
            if measured_bytes >= 64 * 1024 {
                // Reuse repeated lines without retaining a document of native glyphs.
                system = gpui::WindowTextSystem::new(self.text_system.clone());
                measured_bytes = 0;
            }
        }
        cache.version = self.buffer.version();
        cache.font = Some(font);
    }

    fn update_wrap_measurements(&self, edit: &DisplayRowEdit) {
        let mut cache = self.wrap_measurements.borrow_mut();
        if cache.font.is_none() || cache.version != edit.cache.buffer_version {
            return;
        }
        let removed: usize = cache.lines[edit.first_line..=edit.last_line]
            .iter()
            .map(LineMeasure::heap_bytes)
            .sum();
        cache.bytes -= removed;
        let new_lines: Vec<_> = (edit.first_line..=edit.first_line + edit.newlines)
            .map(|line| self.measure_wrap_line(line))
            .map(|measure| cache.admit_measure(measure))
            .collect();
        cache
            .lines
            .splice(edit.first_line..=edit.last_line, new_lines);
        if cache.lines.capacity() > cache.lines.len().saturating_mul(2) {
            cache.lines.shrink_to_fit();
        }
        cache.version = self.buffer.version();
    }

    fn wrapped_line_rows(&self, line: usize, width: f32) -> Vec<DisplayRow> {
        let cached = self.wrap_measurements.borrow().lines[line].clone();
        let measure = if matches!(cached, LineMeasure::Uncached) {
            self.measure_wrap_line(line)
        } else {
            cached
        };
        let advances = measure.runs().iter().flat_map(|run| {
            std::iter::repeat_n((run.columns as usize, run.advance), run.count as usize)
        });
        let mut rows = Vec::new();
        append_display_rows_for_line(&mut rows, line, advances, Some(width), false);
        rows
    }

    fn wrap_width(&self) -> Option<f32> {
        if self.is_large_file() || !self.settings.soft_wrap {
            return None;
        }
        let bounds = self.content_bounds?;
        let available_width = f32::from(bounds.size.width)
            - self.visible_gutter_width()
            - self.visible_content_padding_x() * 2.0;
        let available_width = available_width.max(self.metrics.char_width);
        Some(
            self.settings
                .soft_wrap_column
                .map_or(available_width, |limit| {
                    available_width.min(limit.max(8) as f32 * self.metrics.char_width)
                }),
        )
    }
}

impl WrapMeasurements {
    fn admit_measure(&mut self, measure: LineMeasure) -> LineMeasure {
        let bytes = measure.heap_bytes();
        // Keep variable advances bounded; uniform lines need no per-character storage.
        if self.bytes + bytes > 1024 * 1024 {
            return LineMeasure::Uncached;
        }
        self.bytes += bytes;
        measure
    }

    fn retain_measure(&mut self, measure: LineMeasure) {
        let measure = self.admit_measure(measure);
        self.lines.push(measure);
    }
}

fn unwrapped_line_width(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.graphemes(true).map(grapheme_visual_width).sum()
    }
}

pub(super) fn display_row_for_visual_column(
    rows: &DisplayRows,
    line: usize,
    visual_column: usize,
) -> Option<(usize, DisplayRow, usize)> {
    if let DisplayRows::Folded { base, folds } = rows {
        let previous = folds
            .partition_point(|fold| fold.line <= line)
            .checked_sub(1)
            .map(|i| &folds[i]);
        if previous.is_some_and(|fold| line > fold.line && line <= fold.end_line) {
            return None;
        }
        let hidden = previous.map_or(0, |fold| {
            fold.hidden_before
                + if line == fold.line {
                    0
                } else {
                    fold.end_line - fold.line
                }
        });
        let (index, _, column) = display_row_for_visual_column(base, line, visual_column)?;
        let index = index.checked_sub(hidden)?;
        return rows.get(index).map(|row| (index, row, column));
    }
    if matches!(rows, DisplayRows::Unwrapped(_)) {
        return rows.get(line).map(|row| (line, row, visual_column));
    }
    if let DisplayRows::Wrapped { wraps, .. } = rows {
        let next = wraps.partition_point(|wrapped| wrapped.line < line);
        let index = if let Some(wrapped) = wraps.get(next).filter(|wrapped| wrapped.line == line) {
            wrapped.first_row
                + wrapped
                    .ends
                    .partition_point(|&end| end <= visual_column)
                    .min(wrapped.ends.len() - 1)
        } else {
            let extra = next.checked_sub(1).map_or(0, |i| {
                wraps[i].first_row - wraps[i].line + wraps[i].ends.len() - 1
            });
            line + extra
        };
        return rows
            .get(index)
            .map(|row| (index, row, visual_column.saturating_sub(row.start_col)));
    }
    // Wrapped segments share their boundary column. Assign that caret slot to
    // the later segment, while the physical line ending remains on its last row.
    let index = rows
        .iter()
        .enumerate()
        .rfind(|(_, row)| {
            row.line == line && visual_column >= row.start_col && visual_column <= row.end_col
        })
        .map(|(index, _)| index)
        .or_else(|| rows.iter().rposition(|row| row.line == line))?;
    let row = rows.get(index)?;
    Some((index, row, visual_column.saturating_sub(row.start_col)))
}

fn append_display_rows_for_line(
    rows: &mut Vec<DisplayRow>,
    line: usize,
    grapheme_widths: impl IntoIterator<Item = (usize, f32)>,
    wrap_width: Option<f32>,
    is_folded_header: bool,
) {
    let mut start_col = 0;
    let mut end_col = 0;
    let mut row_width = 0.0;
    for (grapheme_width, advance) in grapheme_widths {
        if wrap_width.is_some_and(|width| end_col > start_col && row_width + advance > width) {
            rows.push(DisplayRow {
                line,
                start_col,
                end_col,
                is_first: start_col == 0,
                is_folded_header: false,
            });
            start_col = end_col;
            row_width = 0.0;
        }
        end_col += grapheme_width;
        row_width += advance;
    }
    // Every physical line owns at least one display row, including empty lines.
    rows.push(DisplayRow {
        line,
        start_col,
        end_col,
        is_first: start_col == 0,
        is_folded_header,
    });
}

#[cfg(test)]
mod tests {
    use super::{DisplayRow, append_display_rows_for_line};

    #[test]
    fn wrapping_never_splits_a_wide_grapheme() {
        let mut rows = Vec::new();
        append_display_rows_for_line(
            &mut rows,
            0,
            [(1, 1.0), (2, 2.0), (2, 2.0), (1, 1.0)],
            Some(4.0),
            false,
        );

        assert_eq!(
            rows,
            vec![
                DisplayRow {
                    line: 0,
                    start_col: 0,
                    end_col: 3,
                    is_first: true,
                    is_folded_header: false,
                },
                DisplayRow {
                    line: 0,
                    start_col: 3,
                    end_col: 6,
                    is_first: false,
                    is_folded_header: false,
                },
            ]
        );
    }
}

#[cfg(test)]
mod edit_layout_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    use oxideterm_editor_core::{BufferOffset, Selection};
    use oxideterm_theme::default_tokens;

    #[gpui::test]
    fn wrapping_reuses_unchanged_measurements_across_edits_and_widths(cx: &mut TestAppContext) {
        use gpui::{Bounds, point, px, size};
        let editor = cx.new(|cx| {
            TextEditorView::new(format!("{}\ntail", "中a".repeat(10)), &default_tokens(), cx)
        });
        editor.update(cx, |editor, cx| {
            editor.apply_runtime_settings(
                &default_tokens(),
                "monospace".into(),
                10.0,
                1.2,
                true,
                false,
                cx,
            );
            editor.settings.soft_wrap_column = Some(8);
            editor.content_bounds = Some(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(1000.0), px(500.0)),
            ));
            let old_rows = editor.display_rows();
            let LineMeasure::Runs(unchanged) = editor.wrap_measurements.borrow().lines[0].clone()
            else {
                panic!("mixed column widths require runs")
            };
            editor
                .cursor
                .set_selection(Selection::caret(BufferOffset(41)));
            editor.insert_text("z", cx);
            assert_eq!(
                editor
                    .display_rows()
                    .iter()
                    .map(|row| (row.line, row.start_col, row.end_col))
                    .collect::<Vec<_>>(),
                [(0, 0, 12), (0, 12, 24), (0, 24, 30), (1, 0, 5)]
            );
            editor.settings.soft_wrap_column = Some(16);
            assert_eq!(
                editor
                    .display_rows()
                    .iter()
                    .map(|row| (row.line, row.start_col, row.end_col))
                    .collect::<Vec<_>>(),
                [(0, 0, 24), (0, 24, 30), (1, 0, 5)]
            );
            let LineMeasure::Runs(retained) = editor.wrap_measurements.borrow().lines[0].clone()
            else {
                panic!("lost unchanged measurements")
            };
            assert!(Arc::ptr_eq(&unchanged, &retained));
            assert_eq!(
                old_rows.iter().map(|row| row.end_col).collect::<Vec<_>>(),
                [12, 24, 30, 4]
            );
            editor.undo(cx);
            assert_eq!(editor.buffer.line_text(1).as_deref(), Some("tail"));
            assert_eq!(
                editor
                    .display_rows()
                    .iter()
                    .map(|row| row.end_col)
                    .collect::<Vec<_>>(),
                [24, 30, 4]
            );
            editor
                .cursor
                .set_selection(Selection::caret(BufferOffset(editor.buffer.len())));
            editor.insert_text(&format!("\n{}", "中a".repeat(10)), cx);
            // Removing earlier physical lines must shift a later wrapped row's
            // document line and display index together, without remeasuring it.
            editor
                .cursor
                .set_selection(Selection::new(BufferOffset(0), BufferOffset(41)));
            editor.insert_text("", cx);
            assert_eq!(
                editor
                    .display_rows()
                    .iter()
                    .map(|row| (row.line, row.start_col, row.end_col))
                    .collect::<Vec<_>>(),
                [(0, 0, 4), (1, 0, 24), (1, 24, 30)]
            );
            editor.undo(cx);
            assert_eq!(
                editor
                    .display_rows()
                    .iter()
                    .map(|row| (row.line, row.start_col, row.end_col))
                    .collect::<Vec<_>>(),
                [(0, 0, 24), (0, 24, 30), (1, 0, 4), (2, 0, 24), (2, 24, 30)]
            );
        });
    }

    #[gpui::test]
    fn viewport_wrapping_uses_shaped_width_and_tracks_resize_and_font_changes(
        cx: &mut TestAppContext,
    ) {
        use gpui::{Bounds, point, px, size};
        let editor = cx.new(|cx| TextEditorView::new("中a".repeat(50), &default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            editor.apply_runtime_settings(
                &default_tokens(),
                "monospace".into(),
                10.0,
                1.2,
                true,
                false,
                cx,
            );
            editor.settings.soft_wrap = true;
            editor.settings.soft_wrap_column = None;
            let padding = editor.visible_gutter_width() + editor.visible_content_padding_x() * 2.0;
            // GPUI's headless font assigns 6 px to each BMP character at 10 px.
            // CJK visual columns still count as two, but cannot dictate wrapping.
            for (width, expected) in [
                (605.0, vec![(0, 150)]),
                (305.0, vec![(0, 75), (75, 150)]),
                (605.0, vec![(0, 150)]),
            ] {
                editor.content_bounds = Some(Bounds::new(
                    point(px(0.0), px(0.0)),
                    size(px(padding + width), px(500.0)),
                ));
                assert_eq!(
                    editor
                        .display_rows()
                        .iter()
                        .map(|row| (row.start_col, row.end_col))
                        .collect::<Vec<_>>(),
                    expected,
                );
            }
            editor.apply_runtime_settings(
                &default_tokens(),
                "monospace".into(),
                20.0,
                1.2,
                true,
                false,
                cx,
            );
            assert_eq!(
                editor
                    .display_rows()
                    .iter()
                    .map(|row| (row.start_col, row.end_col))
                    .collect::<Vec<_>>(),
                [(0, 75), (75, 150)],
            );
        });
    }

    #[gpui::test]
    fn compact_rows_transition_to_folding_and_wrapping(cx: &mut TestAppContext) {
        use gpui::{Bounds, point, px, size};
        use oxideterm_editor_syntax::LanguageId;
        let editor = cx.new(|cx| {
            TextEditorView::new("fn sample() {\n    call();\n}\nlast", &default_tokens(), cx)
        });
        editor.update(cx, |editor, cx| {
            crate::grammar_fixture::install_rust(cx);
            editor.set_language(Some(LanguageId::Rust), cx)
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            editor.settings.soft_wrap = false;
            let original = editor.display_rows();
            assert_eq!(
                original
                    .iter()
                    .map(|row| (row.line, row.end_col))
                    .collect::<Vec<_>>(),
                [(0, 13), (1, 11), (2, 1), (3, 4)]
            );
            let (index, row, column) = display_row_for_visual_column(&original, 0, 50).unwrap();
            assert_eq!((index, row.line, column), (0, 0, 50));
            assert!(editor.toggle_fold_at_line(0, cx));
            let folded = editor.display_rows();
            assert_eq!(
                folded.iter().collect::<Vec<_>>(),
                vec![
                    DisplayRow {
                        line: 0,
                        start_col: 0,
                        end_col: 13,
                        is_first: true,
                        is_folded_header: true,
                    },
                    DisplayRow {
                        line: 3,
                        start_col: 0,
                        end_col: 4,
                        is_first: true,
                        is_folded_header: false,
                    },
                ]
            );
            assert!(editor.toggle_fold_at_line(0, cx));
            assert_eq!(*editor.display_rows(), *original);
            editor.content_bounds = Some(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(1000.0), px(500.0)),
            ));
            editor.settings.soft_wrap = true;
            editor.settings.soft_wrap_column = Some(8);
            let wrapped = editor.display_rows();
            let (index, row, column) = display_row_for_visual_column(&wrapped, 0, 7).unwrap();
            assert_eq!((index, row.start_col, row.end_col, column), (0, 0, 8, 7));
            let (index, row, column) = display_row_for_visual_column(&wrapped, 0, 8).unwrap();
            assert_eq!((index, row.start_col, row.end_col, column), (1, 8, 13, 0));
            let (index, row, column) = display_row_for_visual_column(&wrapped, 0, 13).unwrap();
            assert_eq!((index, row.start_col, row.end_col, column), (1, 8, 13, 5));
        });
    }

    #[gpui::test]
    fn multiple_folds_keep_wrapped_rows_and_caret_coordinates(cx: &mut TestAppContext) {
        use gpui::{Bounds, point, px, size};
        let editor = cx.new(|cx| TextEditorView::new("fn first() {\n    hidden();\n}\n//abcdefghij\nfn second() {\n    hidden();\n}\nTAIL", &default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            crate::grammar_fixture::install_rust(cx);
            editor.set_language(Some(oxideterm_editor_syntax::LanguageId::Rust), cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            editor.content_bounds = Some(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(1000.0), px(500.0)),
            ));
            editor.settings.soft_wrap_column = Some(8);
            assert!(editor.toggle_fold_at_line(0, cx));
            assert!(editor.toggle_fold_at_line(4, cx));
            for wrapped in [false, true] {
                editor.settings.soft_wrap = wrapped;
                let rows = editor.display_rows();
                let expected = if wrapped {
                    vec![
                        (0, 0, 12, true),
                        (3, 0, 8, false),
                        (3, 8, 12, false),
                        (4, 0, 13, true),
                        (7, 0, 4, false),
                    ]
                } else {
                    vec![
                        (0, 0, 12, true),
                        (3, 0, 12, false),
                        (4, 0, 13, true),
                        (7, 0, 4, false),
                    ]
                };
                assert_eq!(
                    rows.iter()
                        .map(|row| (row.line, row.start_col, row.end_col, row.is_folded_header))
                        .collect::<Vec<_>>(),
                    expected
                );
                assert!(rows.get(expected.len()).is_none());
                for line in [1, 2, 5, 6] {
                    assert!(display_row_for_visual_column(&rows, line, 0).is_none());
                }
                let (index, row, column) = display_row_for_visual_column(&rows, 3, 8).unwrap();
                assert_eq!(
                    (index, row.line, column),
                    if wrapped { (2, 3, 0) } else { (1, 3, 8) }
                );
                let (index, row, column) = display_row_for_visual_column(&rows, 7, 4).unwrap();
                assert_eq!(
                    (index, row.line, column),
                    if wrapped { (4, 7, 4) } else { (3, 7, 4) }
                );
                assert_eq!(editor.document_width_columns(), 13);
            }
            assert!(editor.toggle_fold_at_line(0, cx));
            assert!(editor.toggle_fold_at_line(4, cx));
            editor.settings.soft_wrap = false;
            assert_eq!(
                editor
                    .display_rows()
                    .iter()
                    .map(|row| row.line)
                    .collect::<Vec<_>>(),
                (0..8).collect::<Vec<_>>()
            );
        });
    }

    #[gpui::test]
    fn unwrapped_edits_update_rows_without_changing_retained_layout(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| TextEditorView::new("a\n中🙂\nend", &default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            editor.settings.soft_wrap = false;
            let old_rows = editor.display_rows();
            editor.insert_text("long\n", cx);
            let widths = editor
                .display_rows()
                .iter()
                .map(|r| (r.line, r.end_col))
                .collect::<Vec<_>>();
            assert_eq!(widths, vec![(0, 4), (1, 1), (2, 4), (3, 3)]);
            assert_eq!(old_rows.len(), 3);
            assert_eq!(old_rows.get(1).unwrap().end_col, 4);
            editor
                .cursor
                .set_selection(Selection::new(BufferOffset(0), BufferOffset(5)));
            editor.insert_text("", cx);
            assert_eq!(*editor.display_rows(), *old_rows);
            editor
                .cursor
                .set_selection(Selection::new(BufferOffset(2), BufferOffset(9)));
            editor.insert_text("x", cx);
            assert_eq!(editor.document_width_columns(), 3);
        });
    }
}
