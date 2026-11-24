use oxideterm_terminal_emulator::{
    TermMode, TerminalCell, TerminalColor, TerminalCursorShape, TerminalEmulator, TerminalRow,
    TerminalSize, TerminalSnapshot,
};

use crate::input::{KittyKeyEventType, configurable_key_escape_sequence};

#[path = "../tests/support/default_key_sequence.rs"]
mod default_key_sequence;
#[path = "../tests/support/snapshot.rs"]
mod snapshot;

mod input_tests;
mod link_tests;
mod selection_tests;

use crate::metrics;
#[path = "../tests/support/metrics.rs"]
mod geometry;
pub(crate) use geometry::*;
mod layout_tests;

pub(crate) fn emulator(cols: usize, rows: usize) -> TerminalEmulator {
    TerminalEmulator::new(
        TerminalSize {
            cols,
            rows,
            cell_width: 8,
            cell_height: 10,
        },
        Default::default(),
    )
}

pub(crate) fn viewport(
    terminal: &mut TerminalEmulator,
    offset: usize,
    rows: usize,
) -> TerminalSnapshot {
    // Scroll the real grid so wrapped-line context includes the offscreen members.
    let current = terminal.snapshot().display_offset;
    terminal.scroll_display(offset as i32 - current as i32);
    let mut snapshot = terminal.snapshot();
    snapshot.lines.truncate(rows);
    snapshot
}
