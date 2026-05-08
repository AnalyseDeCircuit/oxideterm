mod app;
mod background_cache;
pub mod terminal_ui;
mod terminal_view;
mod trzsz_worker;

pub use app::{SharedTerminalSession, TerminalPane};
pub use background_cache::BackgroundImageRenderCache;
pub use terminal_ui::{
    TerminalBackgroundFit, TerminalBackgroundPreferences, TerminalHighlightRenderMode,
    TerminalHighlightRule, TerminalNotice, TerminalNoticeVariant, TerminalPasteLabels,
    TerminalTrzszLabels, TerminalUiPreferences, TerminalUiTheme,
};
