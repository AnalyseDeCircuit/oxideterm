use alacritty_terminal::{event::WindowSize, grid::Dimensions};

#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    pub cols: usize,
    pub rows: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

pub fn window_size(size: TerminalSize) -> WindowSize {
    WindowSize {
        num_lines: size.rows as u16,
        num_cols: size.cols as u16,
        cell_width: size.cell_width,
        cell_height: size.cell_height,
    }
}
