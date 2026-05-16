use alacritty_terminal::vte::ansi::CursorShape as AlacCursorShape;

#[derive(Clone, Debug)]
pub struct TerminalCell {
    pub ch: char,
    pub zerowidth: String,
    pub wide: bool,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub attrs: TerminalAttrs,
    pub hyperlink: Option<String>,
    pub cursor: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl TerminalColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalAttrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub inverse: bool,
}

#[derive(Clone, Debug)]
pub struct TerminalRow {
    pub cells: Vec<TerminalCell>,
    pub wrapped: bool,
    pub active_input: bool,
}

impl TerminalRow {
    pub fn text(&self) -> String {
        let mut text = String::new();
        for cell in &self.cells {
            text.push(cell.ch);
            text.push_str(&cell.zerowidth);
        }
        text
    }
}

#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    pub cols: usize,
    pub rows: usize,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub cursor_shape: TerminalCursorShape,
    pub display_offset: usize,
    pub scrollback_lines: usize,
    pub lines: Vec<TerminalRow>,
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalCursorShape {
    #[default]
    Block,
    Underline,
    Bar,
    Hollow,
    Hidden,
}

impl From<AlacCursorShape> for TerminalCursorShape {
    fn from(value: AlacCursorShape) -> Self {
        match value {
            AlacCursorShape::Block => TerminalCursorShape::Block,
            AlacCursorShape::Underline => TerminalCursorShape::Underline,
            AlacCursorShape::Beam => TerminalCursorShape::Bar,
            AlacCursorShape::HollowBlock => TerminalCursorShape::Hollow,
            AlacCursorShape::Hidden => TerminalCursorShape::Hidden,
        }
    }
}
