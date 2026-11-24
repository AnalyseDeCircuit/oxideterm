use alacritty_terminal::{
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionType},
    term::Term,
};

/// Inclusive grid coordinates shared by selection painting and clipboard reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSelectionRange {
    pub start_line: i32,
    pub start_col: usize,
    pub end_line: i32,
    pub end_col: usize,
    pub is_block: bool,
}

pub fn set_term_selection<T>(term: &mut Term<T>, range: Option<TerminalSelectionRange>) {
    term.selection = range.map(|range| {
        // The UI already expands word and line selections. Keep those exact
        // cells while delegating scroll, erase, and resize tracking to the grid.
        let kind = if range.is_block {
            SelectionType::Block
        } else {
            SelectionType::Simple
        };
        let mut selection = Selection::new(
            kind,
            Point::new(Line(range.start_line), Column(range.start_col)),
            Side::Left,
        );
        selection.update(
            Point::new(Line(range.end_line), Column(range.end_col)),
            Side::Right,
        );
        selection
    });
}

pub fn term_selection<T>(term: &Term<T>) -> Option<TerminalSelectionRange> {
    let range = term.selection.as_ref()?.to_range(term)?;
    Some(TerminalSelectionRange {
        start_line: range.start.line.0,
        start_col: range.start.column.0,
        end_line: range.end.line.0,
        end_col: range.end.column.0,
        is_block: range.is_block,
    })
}
