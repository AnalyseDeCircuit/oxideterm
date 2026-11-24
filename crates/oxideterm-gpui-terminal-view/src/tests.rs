use oxideterm_terminal_emulator::{
    TermMode, TerminalCell, TerminalColor, TerminalCursorShape, TerminalRow, TerminalSnapshot,
};

use crate::input::{KittyKeyEventType, configurable_key_escape_sequence};

#[path = "../tests/support/default_key_sequence.rs"]
mod default_key_sequence;
#[path = "../tests/support/snapshot.rs"]
mod snapshot;

mod input_tests;
mod link_tests;
mod selection_tests;
