mod app;
mod background_cache;
mod command_facts;
mod image_budget;
mod modem_worker;
mod privilege_prompt;
mod session_log;
pub mod terminal_ui;
mod terminal_view;
mod trzsz_worker;

pub use app::{
    SharedTerminalSession, TerminalBroadcastInputKind, TerminalContextAction, TerminalCursorAnchor,
    TerminalCwdShellIntegrationStatus, TerminalInputBroadcaster, TerminalInputInterceptor,
    TerminalInputInterceptorResult, TerminalPane, TerminalPaneEvent, TerminalSearchStatus,
    TerminalSerialAction, TerminalSerialStatus, TerminalTelnetAction,
    TerminalWorkingDirectorySource,
};
pub use background_cache::BackgroundImageRenderCache;
pub use command_facts::{
    TerminalAiCommandRecord, TerminalAutosuggestCommandRecord, TerminalAutosuggestInputState,
    TerminalCommandFact, TerminalCommandFactStatus,
};
pub use oxideterm_terminal::TerminalOutputProcessor;
pub use oxideterm_terminal_recording::{TerminalRecordingState, TerminalRecordingStatus};
pub use oxideterm_terminal_semantic::SemanticShellDialect;
pub use privilege_prompt::{
    PrivilegePromptConfidence, PrivilegePromptMatch, PrivilegePromptSnapshot,
    detect_custom_privilege_prompt, detect_privilege_prompt,
};
pub use session_log::{
    TerminalSessionLog, TerminalSessionLogContext, TerminalSessionLogOptions,
    TerminalSessionLogState, TerminalSessionLogStatus, prune_terminal_session_logs,
};
pub use terminal_ui::{
    TerminalBackgroundFit, TerminalBackgroundPreferences, TerminalCommandSelectionLabels,
    TerminalHighlightMatchScope, TerminalHighlightRenderMode, TerminalHighlightRule,
    TerminalHighlightRuleSetOverride, TerminalModemLabels, TerminalNotice, TerminalNoticeVariant,
    TerminalPasteLabels, TerminalSerialControlLabels, TerminalSessionLogLabels,
    TerminalTrzszLabels, TerminalUiPreferenceOverrides, TerminalUiPreferences, TerminalUiTheme,
    resolved_terminal_semantic_scheme,
};
