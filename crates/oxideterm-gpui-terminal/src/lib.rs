mod app;
mod background_cache;
mod command_facts;
pub mod terminal_ui;
mod terminal_view;
mod trzsz_worker;

pub use app::{SharedTerminalSession, TerminalPane};
pub use background_cache::BackgroundImageRenderCache;
pub use command_facts::{TerminalAiCommandRecord, TerminalCommandFact, TerminalCommandFactStatus};
pub use oxideterm_terminal_recording::{TerminalRecordingState, TerminalRecordingStatus};
pub use terminal_ui::{
    TerminalBackgroundFit, TerminalBackgroundPreferences, TerminalCommandSelectionLabels,
    TerminalHighlightRenderMode, TerminalHighlightRule, TerminalNotice, TerminalNoticeVariant,
    TerminalPasteLabels, TerminalTrzszLabels, TerminalUiPreferences, TerminalUiTheme,
};
