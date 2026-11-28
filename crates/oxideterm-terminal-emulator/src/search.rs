pub mod scrollback;

use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::Line,
    term::{Term, cell::Flags},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSearchRange {
    pub line: i32,
    pub start_col: usize,
    pub end_col: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSearchMatch {
    pub line: i32,
    pub start_col: usize,
    pub end_col: usize,
    pub ranges: Vec<TerminalSearchRange>,
}

pub fn search_matches_from_term<T: EventListener>(
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
    let mut logical_text = zeroize::Zeroizing::new(String::new());
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

pub fn search_logical_line_matches(
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
            TerminalSearchMatch::from_cell_map(cell_map, start_index, end_index, max_cols)
        })
        .collect()
}

impl TerminalSearchMatch {
    /// Convert a character range in logical text to its physical grid spans.
    pub fn from_cell_map(
        cell_map: &[(i32, usize, usize)],
        start_index: usize,
        end_index: usize,
        max_cols: usize,
    ) -> Option<Self> {
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
        Some(Self {
            line: first.line,
            start_col: first.start_col,
            end_col: first.end_col,
            ranges,
        })
    }
}

pub fn append_grid_line_text<'a>(
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
