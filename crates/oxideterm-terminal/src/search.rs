use alacritty_terminal::term::cell::{Cell, Flags};

use crate::{TerminalSearchMatch, TerminalSearchRange};

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
