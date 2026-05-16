use oxideterm_terminal::{TerminalCell, TerminalSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalPoint {
    pub(crate) row: usize,
    pub(crate) col: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalSelection {
    pub(crate) anchor: TerminalPoint,
    pub(crate) head: TerminalPoint,
    pub(crate) mode: TerminalSelectionMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalSelectionMode {
    Simple,
    Block,
    Semantic,
    Lines,
}

impl TerminalSelection {
    pub(crate) fn is_empty(self) -> bool {
        self.anchor == self.head && self.mode == TerminalSelectionMode::Simple
    }

    pub(crate) fn normalized(self) -> (TerminalPoint, TerminalPoint) {
        if (self.anchor.row, self.anchor.col) <= (self.head.row, self.head.col) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub(crate) fn contains(self, row: usize, col: usize) -> bool {
        if self.mode == TerminalSelectionMode::Block {
            let row_start = self.anchor.row.min(self.head.row);
            let row_end = self.anchor.row.max(self.head.row);
            let col_start = self.anchor.col.min(self.head.col);
            let col_end = self.anchor.col.max(self.head.col);
            return row >= row_start && row <= row_end && col >= col_start && col <= col_end;
        }

        let (start, end) = self.normalized();
        (row, col) >= (start.row, start.col) && (row, col) <= (end.row, end.col)
    }
}

pub(crate) fn word_selection_at_point(
    snapshot: &TerminalSnapshot,
    point: TerminalPoint,
) -> Option<TerminalSelection> {
    let row = snapshot.lines.get(point.row)?;
    let col = point.col.min(row.cells.len().saturating_sub(1));
    let cell = row.cells.get(col)?;
    if !is_word_selection_char(cell.ch) {
        return None;
    }

    let mut start = TerminalPoint {
        row: point.row,
        col,
    };
    while let Some(previous) = previous_logical_cell(snapshot, start)
        && is_word_selection_char(snapshot.lines[previous.row].cells[previous.col].ch)
    {
        start = previous;
    }

    let mut end = TerminalPoint {
        row: point.row,
        col,
    };
    while let Some(next) = next_logical_cell(snapshot, end)
        && is_word_selection_char(snapshot.lines[next.row].cells[next.col].ch)
    {
        end = next;
    }

    Some(TerminalSelection {
        anchor: start,
        head: end,
        mode: TerminalSelectionMode::Semantic,
    })
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
        anchor: TerminalPoint {
            row: start_row,
            col: 0,
        },
        head: TerminalPoint {
            row: end_row,
            col: end.min(snapshot.cols.saturating_sub(1)),
        },
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
    for row_index in start.row..=end.row {
        let row = snapshot.lines.get(row_index)?;
        let line_start = if row_index == start.row { start.col } else { 0 };
        let line_end = if row_index == end.row {
            (end.col + 1).min(snapshot.cols)
        } else {
            snapshot.cols
        };
        let selected = row
            .cells
            .iter()
            .skip(line_start)
            .take(line_end.saturating_sub(line_start))
            .map(cell_text)
            .collect::<String>();
        let continues_without_newline =
            row.wrapped && row_index < end.row && line_end >= snapshot.cols;

        if continues_without_newline {
            text.push_str(&selected);
        } else {
            text.push_str(selected.trim_end());
            if row_index < end.row {
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
    let row_start = selection.anchor.row.min(selection.head.row);
    let row_end = selection.anchor.row.max(selection.head.row);
    let col_start = selection.anchor.col.min(selection.head.col);
    let col_end = (selection.anchor.col.max(selection.head.col) + 1).min(snapshot.cols);
    let mut text = String::new();

    for row_index in row_start..=row_end {
        let row = snapshot.lines.get(row_index)?;
        let selected = row
            .cells
            .iter()
            .skip(col_start)
            .take(col_end.saturating_sub(col_start))
            .map(cell_text)
            .collect::<String>();
        text.push_str(selected.trim_end());
        if row_index < row_end {
            text.push('\n');
        }
    }

    Some(text)
}

pub(crate) fn cell_text(cell: &TerminalCell) -> String {
    let mut text = String::new();
    text.push(cell.ch);
    text.push_str(&cell.zerowidth);
    text
}

pub(crate) fn is_word_selection_char(ch: char) -> bool {
    !ch.is_whitespace()
        && !matches!(
            ch,
            '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | '|'
        )
}
