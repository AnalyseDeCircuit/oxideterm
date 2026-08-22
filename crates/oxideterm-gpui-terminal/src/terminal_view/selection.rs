use oxideterm_terminal::{TerminalCell, TerminalSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalPoint {
    pub(crate) row: usize,
    pub(crate) col: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalGridPoint {
    pub(crate) line: i32,
    pub(crate) col: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalSelection {
    pub(crate) anchor: TerminalGridPoint,
    pub(crate) head: TerminalGridPoint,
    pub(crate) mode: TerminalSelectionMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalSelectionMode {
    Simple,
    Block,
    Semantic,
    Lines,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalSelectionSnapshotRequest {
    pub(crate) display_offset: usize,
    pub(crate) rows: usize,
}

impl TerminalSelection {
    pub(crate) fn is_empty(self) -> bool {
        self.anchor == self.head && self.mode == TerminalSelectionMode::Simple
    }

    pub(crate) fn normalized(self) -> (TerminalGridPoint, TerminalGridPoint) {
        if (self.anchor.line, self.anchor.col) <= (self.head.line, self.head.col) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub(crate) fn contains_viewport_cell(
        self,
        row: usize,
        col: usize,
        display_offset: usize,
    ) -> bool {
        let line = grid_line_for_viewport_row(row, display_offset);
        if self.mode == TerminalSelectionMode::Block {
            let row_start = self.anchor.line.min(self.head.line);
            let row_end = self.anchor.line.max(self.head.line);
            let col_start = self.anchor.col.min(self.head.col);
            let col_end = self.anchor.col.max(self.head.col);
            return line >= row_start && line <= row_end && col >= col_start && col <= col_end;
        }

        let (start, end) = self.normalized();
        (line, col) >= (start.line, start.col) && (line, col) <= (end.line, end.col)
    }
}

pub(crate) fn grid_point_for_viewport_point(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<TerminalGridPoint> {
    (point.row < snapshot.rows).then_some(TerminalGridPoint {
        line: grid_line_for_viewport_row(point.row, snapshot.display_offset),
        col: point.col.min(snapshot.cols.saturating_sub(1)),
    })
}

fn grid_line_for_viewport_row(row: usize, display_offset: usize) -> i32 {
    row as i32 - display_offset as i32
}

fn snapshot_row_for_grid_line(snapshot: &TerminalSnapshot, line: i32) -> Option<usize> {
    let row = i64::from(line) + i64::try_from(snapshot.display_offset).ok()?;
    usize::try_from(row)
        .ok()
        .filter(|row| *row < snapshot.lines.len())
}

pub(crate) fn snapshot_request_for_selection(
    snapshot: &TerminalSnapshot,
    selection: TerminalSelection,
) -> Option<TerminalSelectionSnapshotRequest> {
    if selection.is_empty() {
        return None;
    }

    let (start, end) = selection.normalized();
    if snapshot_row_for_grid_line(snapshot, start.line).is_some()
        && snapshot_row_for_grid_line(snapshot, end.line).is_some()
    {
        return None;
    }

    let start_line = i64::from(start.line);
    let end_line = i64::from(end.line);
    let earliest_line = -i64::try_from(snapshot.scrollback_lines).ok()?;
    let latest_line = i64::try_from(snapshot.rows.saturating_sub(1)).ok()?;
    if start_line < earliest_line || end_line > latest_line {
        return None;
    }

    let display_offset = if start_line < 0 {
        usize::try_from(-start_line).ok()?
    } else {
        0
    };
    let first_line = -i64::try_from(display_offset).ok()?;
    let rows = usize::try_from(end_line - first_line + 1).ok()?;

    // Request only the rows covered by the selection instead of expanding every render snapshot
    // to the complete scrollback buffer.
    Some(TerminalSelectionSnapshotRequest {
        display_offset,
        rows,
    })
}

pub(crate) fn word_selection_at_point(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<TerminalSelection> {
    let row = snapshot.lines.get(point.row)?;
    let col = point.col.min(row.cells.len().saturating_sub(1));
    let cell = row.cells.get(col)?;
    if is_url_selection_char(cell.ch)
        && let Some(selection) = url_selection_at_point(
            snapshot,
            TerminalPoint {
                row: point.row,
                col,
            },
        )
    {
        return Some(selection);
    }
    if !is_shell_token_selection_char(cell.ch) {
        return None;
    }

    let mut start = TerminalPoint {
        row: point.row,
        col,
    };
    while let Some(previous) = previous_logical_cell(snapshot, start)
        && is_shell_token_selection_char(snapshot.lines[previous.row].cells[previous.col].ch)
    {
        start = previous;
    }

    let mut end = TerminalPoint {
        row: point.row,
        col,
    };
    while let Some(next) = next_logical_cell(snapshot, end)
        && is_shell_token_selection_char(snapshot.lines[next.row].cells[next.col].ch)
    {
        end = next;
    }

    Some(TerminalSelection {
        anchor: grid_point_for_viewport_point(snapshot, start)?,
        head: grid_point_for_viewport_point(snapshot, end)?,
        mode: TerminalSelectionMode::Semantic,
    })
}

pub(crate) fn matching_pair_selection_at_point(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<TerminalSelection> {
    let logical_cells = logical_line_cells(snapshot, point)?;
    let click_index = logical_cells
        .iter()
        .position(|(candidate, _)| *candidate == point)?;
    let mut stack = Vec::new();
    let mut quoted_by = None;
    let mut escaped = false;
    let mut best_pair = None;

    for (index, (_, ch)) in logical_cells.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if *ch == '\\' && quoted_by != Some('\'') {
            escaped = true;
            continue;
        }
        if matches!(*ch, '\'' | '"') {
            if quoted_by == Some(*ch) {
                quoted_by = None;
            } else if quoted_by.is_none() {
                quoted_by = Some(*ch);
            }
            continue;
        }
        if quoted_by.is_some() {
            continue;
        }

        if is_opening_pair(*ch) {
            stack.push((*ch, index));
            continue;
        }
        let Some(expected_opening) = matching_opening_pair(*ch) else {
            continue;
        };
        let Some((opening, opening_index)) = stack.pop() else {
            continue;
        };
        if opening != expected_opening {
            // A mismatched close makes the current nesting ambiguous.
            stack.clear();
            continue;
        }
        if opening_index <= click_index && click_index <= index {
            let pair_width = index.saturating_sub(opening_index);
            if best_pair.is_none_or(|(best_opening, best_closing): (usize, usize)| {
                pair_width < best_closing.saturating_sub(best_opening)
            }) {
                best_pair = Some((opening_index, index));
            }
        }
    }

    let (opening_index, closing_index) = best_pair?;
    let content_start = opening_index.checked_add(1)?;
    let content_end = closing_index.checked_sub(1)?;
    if content_start > content_end {
        return None;
    }

    Some(TerminalSelection {
        anchor: grid_point_for_viewport_point(snapshot, logical_cells[content_start].0)?,
        head: grid_point_for_viewport_point(snapshot, logical_cells[content_end].0)?,
        mode: TerminalSelectionMode::Semantic,
    })
}

fn logical_line_cells(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<Vec<(TerminalPoint, char)>> {
    snapshot.lines.get(point.row)?.cells.get(point.col)?;
    let mut start_row = point.row;
    while start_row > 0
        && snapshot
            .lines
            .get(start_row - 1)
            .is_some_and(|row| row.wrapped)
    {
        start_row -= 1;
    }

    let mut cells = Vec::new();
    let mut row_index = start_row;
    loop {
        let row = snapshot.lines.get(row_index)?;
        for (col, cell) in row.cells.iter().take(snapshot.cols).enumerate() {
            cells.push((
                TerminalPoint {
                    row: row_index,
                    col,
                },
                cell.ch,
            ));
        }
        if !row.wrapped || row_index + 1 >= snapshot.rows {
            break;
        }
        row_index += 1;
    }
    Some(cells)
}

fn is_opening_pair(ch: char) -> bool {
    matches!(ch, '(' | '[' | '{')
}

fn matching_opening_pair(ch: char) -> Option<char> {
    match ch {
        ')' => Some('('),
        ']' => Some('['),
        '}' => Some('{'),
        _ => None,
    }
}

fn url_selection_at_point(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<TerminalSelection> {
    let mut start = point;
    while let Some(previous) = previous_logical_cell(snapshot, start)
        && is_url_selection_char(snapshot.lines[previous.row].cells[previous.col].ch)
    {
        start = previous;
    }

    let mut end = point;
    while let Some(next) = next_logical_cell(snapshot, end)
        && is_url_selection_char(snapshot.lines[next.row].cells[next.col].ch)
    {
        end = next;
    }

    let candidate = text_for_logical_range(snapshot, start, end)?;
    if !candidate.contains("://") {
        return None;
    }

    while end != start && is_trailing_url_punctuation(snapshot.lines[end.row].cells[end.col].ch) {
        end = previous_logical_cell(snapshot, end)?;
    }

    Some(TerminalSelection {
        anchor: grid_point_for_viewport_point(snapshot, start)?,
        head: grid_point_for_viewport_point(snapshot, end)?,
        mode: TerminalSelectionMode::Semantic,
    })
}

fn text_for_logical_range(
    snapshot: &TerminalSnapshot,
    start: TerminalPoint,
    end: TerminalPoint,
) -> Option<String> {
    let mut point = start;
    let mut text = String::new();
    loop {
        text.push(snapshot.lines.get(point.row)?.cells.get(point.col)?.ch);
        if point == end {
            break;
        }
        point = next_logical_cell(snapshot, point)?;
    }
    Some(text)
}

pub(crate) fn previous_logical_cell(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<TerminalPoint> {
    if point.col > 0 {
        return Some(TerminalPoint {
            row: point.row,
            col: point.col - 1,
        });
    }

    if point.row > 0
        && snapshot
            .lines
            .get(point.row - 1)
            .is_some_and(|row| row.wrapped)
    {
        Some(TerminalPoint {
            row: point.row - 1,
            col: snapshot.cols.saturating_sub(1),
        })
    } else {
        None
    }
}

pub(crate) fn next_logical_cell(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<TerminalPoint> {
    if point.col + 1 < snapshot.cols {
        return Some(TerminalPoint {
            row: point.row,
            col: point.col + 1,
        });
    }

    snapshot
        .lines
        .get(point.row)
        .is_some_and(|row| row.wrapped && point.row + 1 < snapshot.rows)
        .then_some(TerminalPoint {
            row: point.row + 1,
            col: 0,
        })
}

pub(crate) fn line_selection_at_point(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<TerminalSelection> {
    if point.row >= snapshot.rows {
        return None;
    }
    let mut start_row = point.row;
    while start_row > 0
        && snapshot
            .lines
            .get(start_row - 1)
            .is_some_and(|row| row.wrapped)
    {
        start_row -= 1;
    }

    let mut end_row = point.row;
    while end_row + 1 < snapshot.rows && snapshot.lines.get(end_row).is_some_and(|row| row.wrapped)
    {
        end_row += 1;
    }

    let row = snapshot.lines.get(end_row)?;
    let end = row
        .cells
        .iter()
        .rposition(|cell| cell.ch != ' ')
        .unwrap_or_else(|| snapshot.cols.saturating_sub(1));

    Some(TerminalSelection {
        anchor: grid_point_for_viewport_point(
            snapshot,
            TerminalPoint {
                row: start_row,
                col: 0,
            },
        )?,
        head: grid_point_for_viewport_point(
            snapshot,
            TerminalPoint {
                row: end_row,
                col: end.min(snapshot.cols.saturating_sub(1)),
            },
        )?,
        mode: TerminalSelectionMode::Lines,
    })
}

pub(crate) fn selected_text_for_selection(
    snapshot: &TerminalSnapshot,
    selection: TerminalSelection,
) -> Option<String> {
    if selection.is_empty() {
        return None;
    }

    if selection.mode == TerminalSelectionMode::Block {
        return selected_text_for_block_selection(snapshot, selection);
    }

    let (start, end) = selection.normalized();
    let mut text = String::new();
    for line in start.line..=end.line {
        let row_index = snapshot_row_for_grid_line(snapshot, line)?;
        let row = snapshot.lines.get(row_index)?;
        let line_start = if line == start.line { start.col } else { 0 };
        let line_end = if line == end.line {
            (end.col + 1).min(snapshot.cols)
        } else {
            snapshot.cols
        };
        let selected = text_for_cell_range(&row.cells, line_start, line_end);
        let continues_without_newline = row.wrapped && line < end.line && line_end >= snapshot.cols;

        if continues_without_newline {
            text.push_str(&selected);
        } else {
            text.push_str(selected.trim_end());
            if line < end.line {
                text.push('\n');
            }
        }
    }

    if selection.mode == TerminalSelectionMode::Lines {
        text.push('\n');
    }

    Some(text)
}

pub(crate) fn selected_text_for_block_selection(
    snapshot: &TerminalSnapshot,
    selection: TerminalSelection,
) -> Option<String> {
    let row_start = selection.anchor.line.min(selection.head.line);
    let row_end = selection.anchor.line.max(selection.head.line);
    let col_start = selection.anchor.col.min(selection.head.col);
    let col_end = (selection.anchor.col.max(selection.head.col) + 1).min(snapshot.cols);
    let mut text = String::new();

    for line in row_start..=row_end {
        let row_index = snapshot_row_for_grid_line(snapshot, line)?;
        let row = snapshot.lines.get(row_index)?;
        let selected = text_for_cell_range(&row.cells, col_start, col_end);
        text.push_str(selected.trim_end());
        if line < row_end {
            text.push('\n');
        }
    }

    Some(text)
}

pub(crate) fn cell_text(cell: &TerminalCell) -> String {
    let mut text = String::new();
    text.push(cell.ch);
    text.push_str(cell.zerowidth());
    text
}

fn text_for_cell_range(cells: &[TerminalCell], start: usize, end: usize) -> String {
    let mut text = String::new();
    for index in start..end.min(cells.len()) {
        if index > 0 && cells[index - 1].wide {
            // The second grid cell of a wide glyph is layout state, not text.
            continue;
        }
        text.push_str(&cell_text(&cells[index]));
    }
    text
}

fn is_shell_token_selection_char(ch: char) -> bool {
    !ch.is_whitespace()
        && !matches!(
            ch,
            '"' | '\''
                | '`'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | ','
                | ';'
                | '|'
                | '&'
        )
}

fn is_url_selection_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            ':' | '/'
                | '?'
                | '#'
                | '['
                | ']'
                | '@'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
                | '-'
                | '.'
                | '_'
                | '~'
                | '%'
        )
}

fn is_trailing_url_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '>'
    )
}
