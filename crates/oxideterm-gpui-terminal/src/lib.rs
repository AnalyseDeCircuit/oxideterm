mod app;
mod background_cache;
mod command_facts;
mod modem_worker;
mod privilege_prompt;
pub mod terminal_ui;
mod terminal_view;
mod trzsz_worker;

pub use app::{
    SharedTerminalSession, TerminalContextAction, TerminalCursorAnchor,
    TerminalCwdShellIntegrationStatus, TerminalInputInterceptor, TerminalInputInterceptorResult,
    TerminalPane, TerminalPaneEvent, TerminalSearchStatus, TerminalSerialStatus,
    TerminalWorkingDirectorySource,
};
pub use background_cache::BackgroundImageRenderCache;
pub use command_facts::{
    TerminalAiCommandRecord, TerminalAutosuggestCommandRecord, TerminalAutosuggestInputState,
    TerminalCommandFact, TerminalCommandFactStatus,
};
pub use oxideterm_terminal::TerminalOutputProcessor;
pub use oxideterm_terminal_recording::{TerminalRecordingState, TerminalRecordingStatus};
pub use privilege_prompt::{
    PrivilegePromptConfidence, PrivilegePromptMatch, PrivilegePromptSnapshot,
    detect_custom_privilege_prompt, detect_privilege_prompt,
};
pub use terminal_ui::{
    TerminalBackgroundFit, TerminalBackgroundPreferences, TerminalCommandSelectionLabels,
    TerminalHighlightRenderMode, TerminalHighlightRule, TerminalModemLabels, TerminalNotice,
    TerminalNoticeVariant, TerminalPasteLabels, TerminalSerialControlLabels, TerminalTrzszLabels,
    TerminalUiPreferences, TerminalUiTheme,
};
