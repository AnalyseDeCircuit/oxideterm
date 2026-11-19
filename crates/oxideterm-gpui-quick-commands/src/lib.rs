//! Quick-command dock, manager and editor. Terminal execution and native input ownership stay with the host.

mod bridge;
mod buttons;
mod panel;
mod state;
mod store;
mod view;

pub use bridge::{
    QuickCommandsAccess, QuickCommandsBindings, QuickCommandsFrame, QuickCommandsIme,
    QuickCommandsNotice, QuickCommandsNoticeKind, QuickCommandsRequest,
    QuickCommandsSelectableText,
};
pub use panel::QuickCommandsPanelState;
pub use state::{
    QUICK_COMMAND_TEXTAREA_LINE_HEIGHT, QUICK_COMMAND_TEXTAREA_VERTICAL_PADDING,
    QuickCommandExecutionDraft, QuickCommandInput, QuickCommandKeyDirection, QuickCommandsState,
    TerminalQuickCommandsState, quick_command_input_uses_monospace,
    quick_command_space_inserts_literal,
};
pub use view::{QuickCommandsView, QuickCommandsViews};
