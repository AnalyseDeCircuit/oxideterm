use std::sync::Arc;

use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::Line,
    sync::FairMutex,
    term::{
        Term,
        cell::{Cell, Flags},
    },
};

use crate::{LocalEventListener, TerminalSearchMatch, TerminalSearchRange};

const TERMINAL_SEARCH_LOCK_CHUNK_LINES: i32 = 32;

#[derive(Clone)]
pub struct TerminalSearchSource {
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    cols: usize,
}

impl TerminalSearchSource {
    pub(crate) fn new(term: Arc<FairMutex<Term<LocalEventListener>>>, cols: usize) -> Self {
        Self { term, cols }
    }

    pub fn search_matches(
        &self,
        query: &str,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> Vec<TerminalSearchMatch> {
        let query = query.trim();
        if query.is_empty() || is_cancelled() {
            return Vec::new();
        }
        let (top_line, bottom_line) = {
            let term = self.term.lock();
            (
                -(term.total_lines().saturating_sub(term.screen_lines()) as i32),
                term.screen_lines() as i32,
            )
        };
        let mut matches = Vec::new();
        let mut logical_text = String::new();
        let mut logical_map = Vec::new();
        let mut last_fragment_line: Option<i32> = None;
        let mut chunk_start = top_line;
        while chunk_start < bottom_line {
            if is_cancelled() {
                return Vec::new();
            }
            let chunk_end = (chunk_start + TERMINAL_SEARCH_LOCK_CHUNK_LINES).min(bottom_line);
            let fragments = {
                // Copy a bounded group while holding the emulator lock, then perform substring
                // matching without blocking terminal parsing or interactive input.
                let term = self.term.lock();
                let current_top = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
                let current_bottom = term.screen_lines() as i32;
                (chunk_start..chunk_end)
                    .filter(|line| *line >= current_top && *line < current_bottom)
                    .map(|line| {
                        let row = &term.grid()[Line(line)];
                        let mut text = String::new();
                        let mut map = Vec::new();
                        append_grid_line_text(row[..].iter(), line, self.cols, &mut text, &mut map);
                        let wrapped = row
                            .last()
                            .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE));
                        (line, text, map, wrapped)
                    })
                    .collect::<Vec<_>>()
            };
            for (line, text, map, wrapped) in fragments {
                if last_fragment_line.is_some_and(|previous| previous + 1 != line) {
                    logical_text.clear();
                    logical_map.clear();
                }
                last_fragment_line = Some(line);
                logical_text.push_str(&text);
                logical_map.extend(map);
                if wrapped && line + 1 < bottom_line {
                    continue;
                }
                matches.extend(search_logical_line_matches(
                    &logical_text,
                    &logical_map,
                    query,
                    self.cols,
                ));
                logical_text.clear();
                logical_map.clear();
            }
            chunk_start = chunk_end;
        }
        matches
    }
}

pub(crate) fn search_matches_from_term<T: EventListener>(
    term: &Term<T>,
    cols: usize,
    query: &str,
) -> Vec<TerminalSearchMatch> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let grid = term.grid();
    let top_line = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
    let bottom_line = term.screen_lines() as i32;
    let mut matches = Vec::new();
    let mut logical_text = String::new();
    let mut logical_map = Vec::new();

    for line in top_line..bottom_line {
        let row = &grid[Line(line)];
        append_grid_line_text(
            row[..].iter(),
            line,
            cols,
            &mut logical_text,
            &mut logical_map,
        );
        let wrapped = row
            .last()
            .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE));
        if wrapped && line + 1 < bottom_line {
            continue;
        }

        matches.extend(search_logical_line_matches(
            &logical_text,
            &logical_map,
            query,
            cols,
        ));
        logical_text.clear();
        logical_map.clear();
    }
    matches
}

pub(crate) fn viewport_row_for_grid_line(line: i32, display_offset: usize) -> Option<usize> {
    (line + display_offset as i32).try_into().ok()
}

#[cfg(test)]
pub(crate) fn search_line_matches(
    line: i32,
    text: &str,
    query: &str,
    max_cols: usize,
) -> Vec<TerminalSearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    text.match_indices(query)
        .filter_map(|(start_byte, matched)| {
            let start_col = text[..start_byte].chars().count();
            if start_col >= max_cols {
                return None;
            }

            let cells = matched
                .chars()
                .count()
                .min(max_cols.saturating_sub(start_col));
            (cells > 0).then_some(TerminalSearchMatch {
                line,
                start_col,
                end_col: start_col + cells,
                ranges: vec![TerminalSearchRange {
                    line,
                    start_col,
                    end_col: start_col + cells,
                }],
            })
        })
        .collect()
}

pub(crate) fn search_logical_line_matches(
    text: &str,
    cell_map: &[(i32, usize)],
    query: &str,
    max_cols: usize,
) -> Vec<TerminalSearchMatch> {
    if query.is_empty() || cell_map.is_empty() {
        return Vec::new();
    }

    text.match_indices(query)
        .filter_map(|(start_byte, matched)| {
            let start_index = text[..start_byte].chars().count();
            let end_index = start_index + matched.chars().count();
            ranges_for_match(cell_map, start_index, end_index, max_cols)
        })
        .collect()
}

fn ranges_for_match(
    cell_map: &[(i32, usize)],
    start_index: usize,
    end_index: usize,
    max_cols: usize,
) -> Option<TerminalSearchMatch> {
    if start_index >= end_index || start_index >= cell_map.len() {
        return None;
    }

    let mut ranges: Vec<TerminalSearchRange> = Vec::new();
    for &(line, col) in cell_map
        .iter()
        .skip(start_index)
        .take(end_index.saturating_sub(start_index))
    {
        if col >= max_cols {
            continue;
        }

        if let Some(range) = ranges.last_mut()
            && range.line == line
            && range.end_col == col
        {
            range.end_col = (col + 1).min(max_cols);
            continue;
        }

        ranges.push(TerminalSearchRange {
            line,
            start_col: col,
            end_col: (col + 1).min(max_cols),
        });
    }

    let first = ranges.first()?;
    Some(TerminalSearchMatch {
        line: first.line,
        start_col: first.start_col,
        end_col: first.end_col,
        ranges,
    })
}

pub(crate) fn append_grid_line_text<'a>(
    cells: impl Iterator<Item = &'a Cell>,
    line: i32,
    max_cols: usize,
    text: &mut String,
    cell_map: &mut Vec<(i32, usize)>,
) {
    for (col, cell) in cells.take(max_cols).enumerate() {
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }

        text.push(if cell.c == '\0' { ' ' } else { cell.c });
        cell_map.push((line, col));
        for ch in cell.zerowidth().into_iter().flatten() {
            text.push(*ch);
            cell_map.push((line, col));
        }
    }
}
