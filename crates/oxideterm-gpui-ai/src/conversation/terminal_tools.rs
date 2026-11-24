use gpui::App;
use oxideterm_workspace::{PaneId, TerminalSessionId};

use super::{
    context::{AiRuntimeCommandRecord, AiTerminalPaneContext},
    terminal_watch::AiTerminalWatchSample,
};

#[cfg(not(target_family = "wasm"))]
pub use super::native::{pane::NativeTerminalHost, tools::*};

pub enum AiTerminalUnavailable {
    HostClosed,
    OwnerClosed,
    Recovering,
}

pub enum AiTransportControl<'a> {
    State,
    Serial(&'a serde_json::Value),
    Telnet(&'a serde_json::Value),
}

/// Resolve the session against the current owner on every call; an ID does not grant tool authority.
pub trait AiTerminalHost {
    fn context(
        &mut self,
        session: TerminalSessionId,
        pane: PaneId,
        active: bool,
        label: String,
        cx: &mut App,
    ) -> Result<AiTerminalPaneContext, AiTerminalUnavailable>;
    fn records(
        &mut self,
        session: TerminalSessionId,
        cx: &mut App,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiTerminalUnavailable>;
    fn selection(
        &mut self,
        session: TerminalSessionId,
        cx: &mut App,
    ) -> Result<Option<String>, AiTerminalUnavailable>;
    fn observe(
        &mut self,
        session: TerminalSessionId,
        run_command: bool,
        command: Option<&str>,
        cx: &mut App,
    ) -> AiTerminalWatchSample;
    fn send_input(
        &mut self,
        session: TerminalSessionId,
        bytes: &[u8],
        cx: &mut App,
    ) -> Result<bool, AiTerminalUnavailable>;
    fn control(
        &mut self,
        session: TerminalSessionId,
        request: AiTransportControl<'_>,
        cx: &mut App,
    ) -> Result<serde_json::Value, AiTerminalControlError>;
    fn apply_inline(
        &mut self,
        session: TerminalSessionId,
        command: &str,
        execute: bool,
        cx: &mut App,
    ) -> Result<(), AiTerminalUnavailable>;
}

pub enum AiTerminalControlError {
    Unavailable(AiTerminalUnavailable),
    Rejected(String),
}
