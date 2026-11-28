use std::ops::Deref;

use crate::{TerminalSearchMatch, TerminalSearchRange};
use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::Line,
    term::{
        Term,
        cell::{Cell, Flags},
    },
};

const CHUNK_ROWS: i32 = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSearchOptions {
    pub case_sensitive: bool,
    pub regex: bool,
}

impl Default for TerminalSearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            regex: false,
        }
    }
}

impl TerminalSearchOptions {
    pub fn compile(self, query: &str) -> Result<regex::Regex, regex::Error> {
        let pattern = if self.regex {
            query.to_owned()
        } else {
            regex::escape(query)
        };
        regex::RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .build()
    }
}

/// A compiled query is shared by refreshes until its text or options change.
#[derive(Clone)]
pub struct TerminalSearchQuery {
    query: String,
    pattern: Option<regex::Regex>,
    across_lines: bool,
    context_required: bool,
}

impl TerminalSearchQuery {
    pub fn new(query: &str, options: TerminalSearchOptions) -> Result<Self, regex::Error> {
        let across_lines = options.regex || query.contains('\n');
        let pattern = if options.regex || !options.case_sensitive || across_lines {
            Some(options.compile(query)?)
        } else {
            None
        };
        Ok(Self {
            query: query.into(),
            pattern,
            across_lines,
            context_required: options.regex,
        })
    }
}

struct SearchRow {
    identity: usize,
    start: usize,
    len: usize,
    text_index: usize,
    wrapped: bool,
    history: bool,
    // ASCII columns are implicit; only width/UTF-8 deviations need coordinates.
    adjustments: Option<Box<ColumnMap>>,
}

#[derive(PartialEq, Eq)]
enum ColumnMap {
    Compact(Vec<[u32; 3]>),
    Wide(Vec<[usize; 3]>),
}

impl Default for ColumnMap {
    fn default() -> Self {
        Self::Compact(Vec::new())
    }
}

impl ColumnMap {
    fn push(&mut self, (byte, col, end): (usize, usize, usize)) {
        match self {
            Self::Compact(entries) => {
                if let (Ok(byte), Ok(col), Ok(end)) =
                    (u32::try_from(byte), u32::try_from(col), u32::try_from(end))
                {
                    entries.push([byte, col, end]);
                    return;
                }
                // Combining sequences can make a row's UTF-8 offsets exceed its
                // column count. Promote only that row without truncating offsets.
                let mut wide = Vec::with_capacity(entries.len() + 1);
                wide.extend(
                    entries
                        .iter()
                        .map(|entry| entry.map(|value| value as usize)),
                );
                wide.push([byte, col, end]);
                *self = Self::Wide(wide);
            }
            Self::Wide(entries) => entries.push([byte, col, end]),
        }
    }

    fn shrink_to_fit(&mut self) {
        match self {
            Self::Compact(entries) => entries.shrink_to_fit(),
            Self::Wide(entries) => entries.shrink_to_fit(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Compact(entries) => entries.is_empty(),
            Self::Wide(entries) => entries.is_empty(),
        }
    }

    fn preceding(&self, byte: usize) -> Option<(usize, usize, usize)> {
        let [start, col, end] = match self {
            Self::Compact(entries) => {
                let index = entries.partition_point(|entry| entry[0] as usize <= byte);
                entries
                    .get(index.checked_sub(1)?)?
                    .map(|value| value as usize)
            }
            Self::Wide(entries) => {
                let index = entries.partition_point(|entry| entry[0] <= byte);
                *entries.get(index.checked_sub(1)?)?
            }
        };
        Some((start, col, end))
    }
}

impl SearchRow {
    fn from_grid(
        row: &alacritty_terminal::grid::Row<Cell>,
        cols: usize,
        across_lines: bool,
        text: &mut String,
    ) -> Self {
        let start = text.len();
        let wrapped = row
            .last()
            .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE));
        let cols = if across_lines && !wrapped {
            row.occupied_len().min(cols)
        } else {
            cols
        };
        if let Some(prefix) = row.ascii_prefix() {
            let cols = cols.min(row.len());
            text.reserve(cols);
            text.push_str(&prefix[..prefix.len().min(cols)]);
            if cols > prefix.len() {
                text.extend(std::iter::repeat_n(
                    char::from(*prefix.as_bytes().last().unwrap()),
                    cols - prefix.len(),
                ));
            }
            return Self {
                identity: row.identity(),
                start,
                len: text.len() - start,
                text_index: 0,
                wrapped,
                history: false,
                adjustments: None,
            };
        }
        text.reserve(cols);
        let mut adjustments = ColumnMap::default();
        let mut next_col = 0;
        for (col, cell) in row.iter().take(cols).enumerate() {
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let ch = if cell.c == '\0' { ' ' } else { cell.c };
            let end_col = (col
                + if cell.flags.contains(Flags::WIDE_CHAR) {
                    2
                } else {
                    1
                })
            .min(cols);
            if !ch.is_ascii() || col != next_col || end_col != col + 1 {
                adjustments.push((text.len() - start, col, end_col));
            }
            text.push(ch);
            for ch in cell.zerowidth().into_iter().flatten() {
                adjustments.push((text.len() - start, col, end_col));
                text.push(*ch);
            }
            next_col = end_col;
        }
        adjustments.shrink_to_fit();
        Self {
            identity: row.identity(),
            start,
            len: text.len() - start,
            text_index: 0,
            wrapped,
            history: false,
            adjustments: (!adjustments.is_empty()).then(|| Box::new(adjustments)),
        }
    }

    fn text<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.start + self.len]
    }

    fn cell_at(&self, byte: usize, text: &str) -> (usize, usize) {
        let Some((start, col, end_col)) = self
            .adjustments
            .as_ref()
            .and_then(|map| map.preceding(byte))
        else {
            return (byte, byte + 1);
        };
        if byte == start {
            return (col, end_col);
        }
        let char_len = self.text(text)[start..].chars().next().unwrap().len_utf8();
        let col = end_col + byte - start - char_len;
        (col, col + 1)
    }
}

/// Results retain row identities so output arriving during a scan can be reconciled.
pub struct TerminalSearchResult {
    rows: Vec<SearchRow>,
    // Each matched logical line (or complete cross-line subject) owns its text once.
    text: Vec<Box<str>>,
    matches: Vec<TerminalSearchMatch>,
    cols: usize,
    across_lines: bool,
    context_required: bool,
}

impl TerminalSearchResult {
    fn scan_text(
        &mut self,
        text: &mut String,
        rows: &mut Vec<SearchRow>,
        query: &TerminalSearchQuery,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) {
        let base = self.rows.len();
        let previous_matches = self.matches.len();
        let mut add_match = |start: usize, end: usize| {
            if start == end {
                return;
            }
            let first = rows.partition_point(|row| row.start + row.len <= start);
            let mut ranges = Vec::new();
            for (index, row) in rows.iter().enumerate().skip(first) {
                if row.start >= end {
                    break;
                }
                let from = start.saturating_sub(row.start);
                let to = end.saturating_sub(row.start).min(row.len);
                if from >= to {
                    continue;
                }
                let last = to - row.text(text)[..to].chars().next_back().unwrap().len_utf8();
                ranges.push(TerminalSearchRange {
                    line: (base + index) as i32,
                    start_col: row.cell_at(from, text).0,
                    end_col: row.cell_at(last, text).1,
                });
            }
            if let Some(first) = ranges.first() {
                self.matches.push(TerminalSearchMatch {
                    line: first.line,
                    start_col: first.start_col,
                    end_col: first.end_col,
                    ranges,
                });
            }
        };
        if let Some(pattern) = &query.pattern {
            for hit in pattern.find_iter(text) {
                if is_cancelled() {
                    break;
                }
                add_match(hit.start(), hit.end());
            }
        } else {
            for (start, matched) in text.match_indices(&query.query) {
                if is_cancelled() {
                    break;
                }
                add_match(start, start + matched.len());
            }
        }
        if self.matches.len() != previous_matches {
            for row in rows.iter_mut() {
                row.text_index = self.text.len();
            }
            self.rows.append(rows);
            self.text.push(std::mem::take(text).into_boxed_str());
        }
    }
}

/// Captures bounded groups of rows. The host can yield between groups without
/// holding the grid while matching text or waiting for another frame.
pub struct ScrollbackScan {
    result: TerminalSearchResult,
    query: TerminalSearchQuery,
    next: i32,
    bottom: i32,
    text: String,
    pending: Vec<SearchRow>,
    chunk_text: String,
}

impl ScrollbackScan {
    pub fn new<T: EventListener>(term: &Term<T>, cols: usize, query: &TerminalSearchQuery) -> Self {
        let bottom = term.screen_lines() as i32;
        Self {
            result: TerminalSearchResult {
                rows: Vec::new(),
                text: Vec::new(),
                matches: Vec::new(),
                cols,
                across_lines: query.across_lines,
                context_required: query.context_required,
            },
            query: query.clone(),
            next: -(term.history_size() as i32),
            bottom,
            text: String::new(),
            pending: Vec::new(),
            chunk_text: String::new(),
        }
    }

    /// Returns true after the last group. Release a native grid lock before evaluating the query.
    pub fn advance<T: EventListener, G: Deref<Target = Term<T>>>(
        &mut self,
        grid: impl FnOnce() -> G,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> bool {
        let start = self.next;
        if start >= self.bottom || self.query.query.is_empty() || is_cancelled() {
            return true;
        }
        let end = start.saturating_add(CHUNK_ROWS).min(self.bottom);
        self.chunk_text.clear();
        let rows = {
            let term = grid();
            let current_top = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
            (start..end)
                .filter(|line| *line >= current_top && *line < term.screen_lines() as i32)
                .map(|line| {
                    let mut row = SearchRow::from_grid(
                        &term.grid()[Line(line)],
                        self.result.cols,
                        self.query.across_lines,
                        &mut self.chunk_text,
                    );
                    row.history = line < 0;
                    row
                })
                .collect::<Vec<_>>()
        };
        let text = &mut self.text;
        let pending_rows = &mut self.pending;
        for mut row in rows {
            let row_text = row.text(&self.chunk_text);
            row.start = text.len();
            text.push_str(row_text);
            let wrapped = row.wrapped;
            pending_rows.push(row);
            if !wrapped {
                if self.query.across_lines {
                    text.push('\n');
                } else {
                    self.result
                        .scan_text(text, pending_rows, &self.query, is_cancelled);
                    pending_rows.clear();
                    text.clear();
                }
            }
        }

        self.next = end;
        end == self.bottom
    }

    pub fn finish(
        mut self,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> TerminalSearchResult {
        if !is_cancelled() && !self.pending.is_empty() {
            if self.text.ends_with('\n') {
                self.text.pop();
            }
            self.result
                .scan_text(&mut self.text, &mut self.pending, &self.query, is_cancelled);
        }
        self.result
    }
}

impl TerminalSearchResult {
    pub fn resolve<T: EventListener, G: Deref<Target = Term<T>>>(
        self,
        grid: impl FnOnce() -> G,
    ) -> Vec<TerminalSearchMatch> {
        let result = self;
        if result.matches.is_empty() {
            return Vec::new();
        }
        let row_count = result.rows.len();
        let context_required = result.context_required;
        let context_bounds = |first: usize, last: usize| {
            if context_required {
                (first.saturating_sub(1), (last + 1).min(row_count - 1))
            } else {
                (first, last)
            }
        };
        let mut needed = vec![false; result.rows.len()];
        for hit in &result.matches {
            let first = hit.ranges.first().unwrap().line as usize;
            let last = hit.ranges.last().unwrap().line as usize;
            let (first, last) = context_bounds(first, last);
            needed[first..=last].fill(true);
        }
        let mut positions: std::collections::HashMap<_, Option<i32>> = result
            .rows
            .iter()
            .zip(&needed)
            .filter(|(_, needed)| **needed)
            .map(|(row, _)| (row.identity, None))
            .collect();
        let term = grid();
        if term.columns() != result.cols {
            return Vec::new();
        }
        let top = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
        let bottom = term.screen_lines() as i32;
        for line in top..bottom {
            if let Some(position) = positions.get_mut(&term.grid()[Line(line)].identity()) {
                *position = Some(line);
            }
        }
        let lines = result
            .rows
            .iter()
            .zip(needed)
            .map(|(row, needed)| {
                if !needed {
                    return None;
                }
                let line = positions.get(&row.identity).copied().flatten()?;
                // History rows cannot be edited by terminal input; identity and column count
                // suffice there. Only screen rows require a fresh content comparison.
                if row.history && line < 0 {
                    return Some(line);
                }
                let mut current_text = String::new();
                let current = SearchRow::from_grid(
                    &term.grid()[Line(line)],
                    result.cols,
                    result.across_lines,
                    &mut current_text,
                );
                (current.text(&current_text) == row.text(&result.text[row.text_index])
                    && current.wrapped == row.wrapped
                    && current.adjustments == row.adjustments)
                    .then_some(line)
            })
            .collect::<Vec<_>>();
        // All positions and boundary checks below refer to this captured revision.
        // Constructing, sorting and dropping potentially large result sets needs no grid lock.
        drop(term);
        let mut matches: Vec<_> = result
            .matches
            .into_iter()
            .filter_map(|mut hit| {
                let first = hit.ranges.first()?.line as usize;
                let last = hit.ranges.last()?.line as usize;
                let (first, last) = context_bounds(first, last);
                let first_line = lines[first]?;
                // Anchors and word boundaries also depend on neighboring rows and buffer edges.
                if context_required
                    && ((first == 0 && first_line != top)
                        || (last + 1 == row_count && lines[last] != Some(bottom - 1)))
                {
                    return None;
                }
                // Cross-line matches must still refer to consecutive, unchanged rows.
                if (first..=last)
                    .any(|index| lines[index] != Some(first_line + (index - first) as i32))
                {
                    return None;
                }
                for range in &mut hit.ranges {
                    range.line = lines[range.line as usize]?;
                }
                hit.line = hit.ranges[0].line;
                Some(hit)
            })
            .collect();
        matches.sort_by_key(|hit| (hit.line, hit.start_col, hit.end_col));
        matches.dedup();
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn search_columns_preserve_offsets_beyond_u32() {
        let mut map = ColumnMap::default();
        map.push((0, 0, 2));
        map.push((u32::MAX as usize + 1, 2, 3));
        assert_eq!(map.preceding(0), Some((0, 0, 2)));
        assert_eq!(map.preceding(u32::MAX as usize), Some((0, 0, 2)));
        assert_eq!(
            map.preceding(u32::MAX as usize + 1),
            Some((4_294_967_296, 2, 3))
        );
    }

    #[test]
    fn search_clamps_requested_columns_to_the_stored_row() {
        let mut grid = alacritty_terminal::grid::Grid::<Cell>::new(2, 80, 10);
        grid[Line(0)]
            .cell_mut(alacritty_terminal::index::Column(0))
            .c = 'a';
        grid.scroll_up(&(Line(0)..Line(2)), 1);
        let mut text = String::new();
        SearchRow::from_grid(&grid[Line(-1)], 160, false, &mut text);
        assert_eq!(text, format!("a{}", " ".repeat(79)));
    }
}
