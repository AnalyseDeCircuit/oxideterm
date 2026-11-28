//! Editable command drafts, paced sends and per-pane ownership for the terminal command bar.
pub mod cwd;
mod panel;
mod state;
pub use oxideterm_terminal_encoding::command_sender::{
    TerminalSenderFrame, TerminalSenderInputMode, TerminalSenderPacing,
};
pub use panel::{ExpandedSender, SenderAction, SenderPanelFrame};
pub use state::*;
