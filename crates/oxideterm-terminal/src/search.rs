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

#[derive(Clone)]
pub struct TerminalSearchSource {
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    cols: usize,
}

struct SearchRow {
    identity: usize,
    start: usize,
    len: usize,
    text_index: usize,
    wrapped: bool,
    history: bool,
    // ASCII columns are implicit; only width/UTF-8 deviations need coordinates.
    adjustments: Box<[(usize, usize, usize)]>,
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
                adjustments: Box::default(),
            };
        }
        text.reserve(cols);
        let mut adjustments = Vec::new();
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
        Self {
            identity: row.identity(),
            start,
            len: text.len() - start,
            text_index: 0,
            wrapped,
            history: false,
            adjustments: adjustments.into_boxed_slice(),
        }
    }

    fn text<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.start + self.len]
    }

    fn cell_at(&self, byte: usize, text: &str) -> (usize, usize) {
        let index = self
            .adjustments
            .partition_point(|(start, _, _)| *start <= byte);
        if index == 0 {
            return (byte, byte + 1);
        }
        let (start, col, end_col) = self.adjustments[index - 1];
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

impl TerminalSearchSource {
    pub(crate) fn new(term: Arc<FairMutex<Term<LocalEventListener>>>, cols: usize) -> Self {
        Self { term, cols }
    }

    pub fn search_matches(
        &self,
        query: &str,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> Vec<TerminalSearchMatch> {
        self.search_matches_with_options(query, TerminalSearchOptions::default(), is_cancelled)
    }

    pub fn search_matches_with_options(
        &self,
        query: &str,
        options: TerminalSearchOptions,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> Vec<TerminalSearchMatch> {
        let Ok(query) = TerminalSearchQuery::new(query, options) else {
            return Vec::new();
        };
        let result = self.search(&query, is_cancelled);
        if is_cancelled() {
            return Vec::new();
        }
        self.resolve_search_result(result)
    }

    pub fn search(
        &self,
        query: &TerminalSearchQuery,
        is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    ) -> TerminalSearchResult {
        let mut result = TerminalSearchResult {
            rows: Vec::new(),
            text: Vec::new(),
            matches: Vec::new(),
            cols: self.cols,
            across_lines: query.across_lines,
            context_required: query.context_required,
        };
        if query.query.is_empty() || is_cancelled() {
            return result;
        }
        let (top, bottom) = {
            let term = self.term.lock();
            (
                -(term.total_lines().saturating_sub(term.screen_lines()) as i32),
                term.screen_lines() as i32,
            )
        };
        let mut text = String::new();
        let mut pending_rows = Vec::new();
        let mut chunk_text = String::new();
        let mut start = top;
        while start < bottom {
            if is_cancelled() {
                return result;
            }
            let end = (start + TERMINAL_SEARCH_LOCK_CHUNK_LINES).min(bottom);
            chunk_text.clear();
            let rows = {
                // Copy bounded chunks; regex evaluation never holds the emulator lock.
                let term = self.term.lock();
                let current_top = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
                (start..end)
                    .filter(|line| *line >= current_top && *line < term.screen_lines() as i32)
                    .map(|line| {
                        let mut row = SearchRow::from_grid(
                            &term.grid()[Line(line)],
                            self.cols,
                            query.across_lines,
                            &mut chunk_text,
                        );
                        row.history = line < 0;
                        row
                    })
                    .collect::<Vec<_>>()
            };
            for mut row in rows {
                let row_text = row.text(&chunk_text);
                row.start = text.len();
                text.push_str(row_text);
                let wrapped = row.wrapped;
                pending_rows.push(row);
                if !wrapped {
                    if query.across_lines {
                        text.push('\n');
                    } else {
                        result.scan_text(&mut text, &mut pending_rows, query, is_cancelled);
                        pending_rows.clear();
                        text.clear();
                    }
                }
            }
            start = end;
        }
        // The final synthetic separator is not part of the terminal contents.
        if text.ends_with('\n') {
            text.pop();
        }
        if !pending_rows.is_empty() {
            result.scan_text(&mut text, &mut pending_rows, query, is_cancelled);
        }
        result
    }

    pub fn resolve_search_result(&self, result: TerminalSearchResult) -> Vec<TerminalSearchMatch> {
        if result.cols != self.cols || result.matches.is_empty() {
            return Vec::new();
        }
        let term = self.term.lock();
        if term.columns() != result.cols {
            return Vec::new();
        }
        let top = -(term.total_lines().saturating_sub(term.screen_lines()) as i32);
        let bottom = term.screen_lines() as i32;
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
                    self.cols,
                    result.across_lines,
                    &mut current_text,
                );
                (current.text(&current_text) == row.text(&result.text[row.text_index])
                    && current.wrapped == row.wrapped
                    && current.adjustments == row.adjustments)
                    .then_some(line)
            })
            .collect::<Vec<_>>();
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
        append_grid_line_text(row.iter(), line, cols, &mut logical_text, &mut logical_map);
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

pub(crate) fn search_logical_line_matches(
    text: &str,
    cell_map: &[(i32, usize, usize)],
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

#[cfg(test)]
fn search_pattern_matches(
    text: &str,
    cell_map: &[(i32, usize, usize)],
    pattern: &regex::Regex,
    max_cols: usize,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Vec<TerminalSearchMatch> {
    let mut previous_byte = 0;
    let mut previous_char = 0;
    pattern
        .find_iter(text)
        .take_while(|_| !is_cancelled())
        .filter_map(|matched| {
            let start = previous_char + text[previous_byte..matched.start()].chars().count();
            let end = start + matched.as_str().chars().count();
            previous_byte = matched.end();
            previous_char = end;
            ranges_for_match(cell_map, start, end, max_cols)
        })
        .collect()
}

fn ranges_for_match(
    cell_map: &[(i32, usize, usize)],
    start_index: usize,
    end_index: usize,
    max_cols: usize,
) -> Option<TerminalSearchMatch> {
    if start_index >= end_index || start_index >= cell_map.len() {
        return None;
    }

    let mut ranges: Vec<TerminalSearchRange> = Vec::new();
    for &(line, col, end_col) in cell_map
        .iter()
        .skip(start_index)
        .take(end_index.saturating_sub(start_index))
    {
        if col >= max_cols {
            continue;
        }

        if let Some(range) = ranges.last_mut()
            && range.line == line
            && range.end_col >= col
        {
            range.end_col = range.end_col.max(end_col).min(max_cols);
            continue;
        }

        ranges.push(TerminalSearchRange {
            line,
            start_col: col,
            end_col: end_col.min(max_cols),
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
    cells: impl Iterator<Item = alacritty_terminal::term::cell::CellRef<'a>>,
    line: i32,
    max_cols: usize,
    text: &mut String,
    cell_map: &mut Vec<(i32, usize, usize)>,
) {
    for (col, cell) in cells.take(max_cols).enumerate() {
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }

        text.push(if cell.c == '\0' { ' ' } else { cell.c });
        let end_col = (col
            + if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            })
        .min(max_cols);
        cell_map.push((line, col, end_col));
        for ch in cell.zerowidth().into_iter().flatten() {
            text.push(*ch);
            cell_map.push((line, col, end_col));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_search_preserves_text_history_tails_and_chunk_boundaries() {
        let mut session =
            crate::TerminalSession::recording_playback(400, 3, Default::default(), 100);
        for line in 0..70 {
            let row = format!("row{line:02} {}", "abcdefghij".repeat(39));
            session.feed_recording_output(format!("\x1b[31m{row}\x1b[0m\r\n").as_bytes());
        }
        let source = session.search_source().unwrap();
        {
            let term = source.term.lock();
            let mut text = String::new();
            let copied = SearchRow::from_grid(&term.grid()[Line(-36)], 800, false, &mut text);
            assert_eq!(
                copied.text(&text),
                format!("row32 {}    ", "abcdefghij".repeat(39))
            );
        }
        for (query, regex, expected) in [
            ("row32", false, vec![(-36, 0, 5)]),
            (
                "ij    ",
                false,
                (0..70).map(|i| (i - 68, 394, 400)).collect(),
            ),
            ("ghij\nrow33", false, vec![(-36, 392, 396), (-35, 0, 5)]),
            (r"ghij\nrow33", true, vec![(-36, 392, 396), (-35, 0, 5)]),
        ] {
            let matches = source.search_matches_with_options(
                query,
                TerminalSearchOptions {
                    regex,
                    case_sensitive: true,
                },
                &|| false,
            );
            let actual: Vec<_> = matches
                .iter()
                .flat_map(|hit| &hit.ranges)
                .map(|range| (range.line, range.start_col, range.end_col))
                .collect();
            assert_eq!(actual, expected, "{query}");
        }
        let mut wrapped =
            crate::TerminalSession::recording_playback(80, 3, Default::default(), 100);
        wrapped.feed_recording_output(
            format!(
                "{}needle{}\r\none\r\ntwo\r\nthree",
                "a".repeat(76),
                "b".repeat(78)
            )
            .as_bytes(),
        );
        let matches = wrapped
            .search_source()
            .unwrap()
            .search_matches("needle", &|| false);
        let ranges: Vec<_> = matches
            .iter()
            .flat_map(|hit| &hit.ranges)
            .map(|range| (range.line, range.start_col, range.end_col))
            .collect();
        assert_eq!(ranges, vec![(-2, 76, 80), (-1, 0, 2)]);
    }

    #[test]
    fn search_ranges_cover_complete_wide_cells() {
        let mut cells = Vec::new();
        for ch in "a中文b".chars() {
            let mut cell = Cell::default();
            cell.c = ch;
            if matches!(ch, '中' | '文') {
                cell.flags.insert(Flags::WIDE_CHAR);
            }
            cells.push(cell);
            if matches!(ch, '中' | '文') {
                let mut spacer = Cell::default();
                spacer.flags.insert(Flags::WIDE_CHAR_SPACER);
                cells.push(spacer);
            }
        }
        let mut text = String::new();
        let mut map = Vec::new();
        append_grid_line_text(
            cells.iter().map(alacritty_terminal::grid::GridCell::view),
            0,
            6,
            &mut text,
            &mut map,
        );
        for (query, expected) in [("中", (1, 3)), ("中文", (1, 5)), ("文b", (3, 6))] {
            for options in [
                TerminalSearchOptions::default(),
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                TerminalSearchOptions {
                    case_sensitive: false,
                    ..Default::default()
                },
            ] {
                let hits = if options == TerminalSearchOptions::default() {
                    search_logical_line_matches(&text, &map, query, 6)
                } else {
                    search_pattern_matches(
                        &text,
                        &map,
                        &options.compile(query).unwrap(),
                        6,
                        &|| false,
                    )
                };
                assert_eq!(
                    hits.iter()
                        .flat_map(|hit| hit.ranges.iter())
                        .map(|range| (range.line, range.start_col, range.end_col))
                        .collect::<Vec<_>>(),
                    vec![(0, expected.0, expected.1)],
                    "{query}, {options:?}"
                );
            }
        }
    }

    #[test]
    fn search_modes_preserve_unicode_coordinates_and_cross_line_ranges() {
        let text = "猫 Foo foo\nbar";
        let map = vec![
            (0, 0, 2),
            (0, 2, 3),
            (0, 3, 4),
            (0, 4, 5),
            (0, 5, 6),
            (0, 6, 7),
            (0, 7, 8),
            (0, 8, 9),
            (0, 9, 10),
            (0, 80, 81),
            (1, 0, 1),
            (1, 1, 2),
            (1, 2, 3),
        ];
        for (query, options, expected) in [
            (
                "foo",
                TerminalSearchOptions::default(),
                vec![vec![(0, 7, 10)]],
            ),
            (
                "foo",
                TerminalSearchOptions {
                    case_sensitive: false,
                    ..Default::default()
                },
                vec![vec![(0, 3, 6)], vec![(0, 7, 10)]],
            ),
            (
                "F.o",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![vec![(0, 3, 6)]],
            ),
            ("F.o", TerminalSearchOptions::default(), vec![]),
            (
                "foo.bar",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![],
            ),
            (
                ".*",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![vec![(0, 0, 10)], vec![(1, 0, 3)]],
            ),
            (
                "(?s).*",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![vec![(0, 0, 10), (1, 0, 3)]],
            ),
            (
                r"foo\nbar",
                TerminalSearchOptions {
                    regex: true,
                    ..Default::default()
                },
                vec![vec![(0, 7, 10), (1, 0, 3)]],
            ),
            (
                "foo\nbar",
                TerminalSearchOptions::default(),
                vec![vec![(0, 7, 10), (1, 0, 3)]],
            ),
        ] {
            let pattern = options.compile(query).unwrap();
            let actual = search_pattern_matches(text, &map, &pattern, 80, &|| false);
            let ranges: Vec<Vec<_>> = actual
                .iter()
                .map(|hit| {
                    hit.ranges
                        .iter()
                        .map(|range| (range.line, range.start_col, range.end_col))
                        .collect()
                })
                .collect();
            assert_eq!(ranges, expected, "query={query:?}, options={options:?}");
        }
    }

    #[test]
    fn invalid_and_zero_width_patterns_do_not_produce_highlights() {
        let options = TerminalSearchOptions {
            regex: true,
            ..Default::default()
        };
        assert!(options.compile("[").is_err());
        assert_eq!(
            search_pattern_matches(
                "abc",
                &[(0, 0, 1), (0, 1, 2), (0, 2, 3)],
                &options.compile("^").unwrap(),
                80,
                &|| false
            ),
            vec![]
        );
    }
}
