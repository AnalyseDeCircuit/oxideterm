use crate::conversation::{
    context::{AiRuntimeCommandRecord, AiTerminalPaneContext, ai_local_os_label},
    terminal_tools::{
        AiTerminalControlError, AiTerminalHost, AiTerminalUnavailable, AiTransportControl,
    },
    terminal_watch::AiTerminalWatchSample,
};
use gpui::{App, Entity};
use oxideterm_gpui_terminal::TerminalPane;
use oxideterm_terminal::TerminalCommandMarkDetectionSource;
use oxideterm_workspace::{PaneId, TerminalSessionId};

pub fn ai_terminal_environment(
    kind: oxideterm_terminal::TerminalSessionKind,
    local_os: &str,
) -> String {
    use oxideterm_terminal::TerminalSessionKind;
    // Telnet and serial panes share the local tab kind but run on another target.
    match kind {
        TerminalSessionKind::LocalPty => format!("Local ({local_os}; shell unverified)"),
        TerminalSessionKind::SshPty => "SSH (remote OS and shell unverified)".to_string(),
        TerminalSessionKind::Mosh => "Mosh (remote OS and shell unverified)".to_string(),
        TerminalSessionKind::Telnet => "Telnet (remote OS and shell unverified)".to_string(),
        TerminalSessionKind::Serial => {
            "Serial device (OS and command interface unverified)".to_string()
        }
        TerminalSessionKind::Ble => "BLE device (OS and command interface unverified)".to_string(),
    }
}

fn ai_ledger_source_from_terminal_source(source: TerminalCommandMarkDetectionSource) -> String {
    match source {
        TerminalCommandMarkDetectionSource::Ai => "ai.terminal_input",
        TerminalCommandMarkDetectionSource::Broadcast => "broadcast",
        TerminalCommandMarkDetectionSource::ShellIntegration => "shell_integration",
        TerminalCommandMarkDetectionSource::CommandBar => "command_bar",
        TerminalCommandMarkDetectionSource::QuickCommand => "quick_command",
        TerminalCommandMarkDetectionSource::UserInputObserved => "user.terminal_input",
        TerminalCommandMarkDetectionSource::Heuristic => "user_promoted",
    }
    .to_string()
}

fn ai_ledger_status_from_terminal_status(
    status: oxideterm_gpui_terminal::TerminalCommandFactStatus,
) -> String {
    match status {
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Open => "running",
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed => "completed",
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Stale => "stale",
    }
    .to_string()
}

pub fn capture_command_records(
    input: impl IntoIterator<Item = oxideterm_gpui_terminal::TerminalAiCommandRecord>,
) -> Vec<AiRuntimeCommandRecord> {
    // History is captured from the pane, never from secret-capable tool arguments.
    let mut records = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for record in input {
        if !seen.insert(record.command_id.clone()) {
            continue;
        }
        let source = ai_ledger_source_from_terminal_source(record.source);
        let status = ai_ledger_status_from_terminal_status(record.status);
        records.push(AiRuntimeCommandRecord {
            command_id: record.command_id,
            command: record.command,
            cwd: None,
            source,
            status,
            exit_code: record.exit_code.map(i64::from),
            started_at: record.started_at as i64,
            finished_at: record.finished_at.map(|value| value as i64),
            approval_mode: None,
            risk: "execute".to_string(),
        });
    }
    records
}

fn terminal_ai_inline_os_context(kind: oxideterm_terminal::TerminalSessionKind) -> String {
    let local_os = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "Unknown"
    };
    match kind {
        oxideterm_terminal::TerminalSessionKind::SshPty => {
            format!("SSH terminal (remote OS unknown, local: {local_os})")
        }
        oxideterm_terminal::TerminalSessionKind::Mosh => {
            format!("Mosh terminal (remote OS unknown, local: {local_os})")
        }
        oxideterm_terminal::TerminalSessionKind::LocalPty => {
            format!("Local terminal on {local_os}")
        }
        oxideterm_terminal::TerminalSessionKind::Telnet => "Telnet (remote OS unknown)".into(),
        oxideterm_terminal::TerminalSessionKind::Serial => {
            "Serial device (command interface unknown)".into()
        }
        oxideterm_terminal::TerminalSessionKind::Ble => {
            "BLE device (command interface unknown)".into()
        }
    }
}

impl crate::state::AiWorkspaceEntity {
    pub fn submit_inline_for_terminal(
        &mut self,
        regenerate: bool,
        kind: oxideterm_terminal::TerminalSessionKind,
        settings: &oxideterm_settings::AiSettings,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut gpui::Context<Self>,
    ) {
        self.submit_inline_prompt(
            regenerate,
            terminal_ai_inline_os_context(kind),
            settings,
            i18n,
            cx,
        );
    }
}

pub fn apply_inline_command(
    pane: Option<&gpui::Entity<oxideterm_gpui_terminal::TerminalPane>>,
    command: zeroize::Zeroizing<String>,
    execute: bool,
    cx: &mut gpui::App,
) {
    let Some(pane) = pane else { return };
    apply_command(pane, &command, execute, cx);
}

impl crate::state::AiWorkspaceEntity {
    pub fn open_inline_for_pane(
        &mut self,
        target: (
            oxideterm_workspace::PaneId,
            oxideterm_workspace::TerminalSessionId,
        ),
        pane: Option<&gpui::Entity<oxideterm_gpui_terminal::TerminalPane>>,
        budget: usize,
        cx: &mut gpui::Context<Self>,
    ) {
        // The desktop caller has already resolved this specific pane/session pair.
        let mut host = NativeTerminalHost(|id, _: &mut App| {
            pane.filter(|_| id == target.1)
                .cloned()
                .ok_or(AiTerminalUnavailable::OwnerClosed)
        });
        self.open_inline_for_session(target, &mut host, budget, cx);
    }
}

/// The resolver belongs to the workspace's session/authority registry, not to an AI panel.
pub struct NativeTerminalHost<F>(pub F);

impl<F> AiTerminalHost for NativeTerminalHost<F>
where
    F: FnMut(TerminalSessionId, &mut App) -> Result<Entity<TerminalPane>, AiTerminalUnavailable>,
{
    fn context(
        &mut self,
        session: TerminalSessionId,
        pane: PaneId,
        active: bool,
        label: String,
        cx: &mut App,
    ) -> Result<AiTerminalPaneContext, AiTerminalUnavailable> {
        let handle = (self.0)(session, cx)?;
        let terminal = handle.read(cx);
        Ok(AiTerminalPaneContext {
            pane,
            session: Some(session),
            active,
            buffer: terminal.visible_text_snapshot(),
            environment: ai_terminal_environment(terminal.session_kind(), ai_local_os_label()),
            label,
        })
    }

    fn records(
        &mut self,
        session: TerminalSessionId,
        cx: &mut App,
    ) -> Result<Vec<AiRuntimeCommandRecord>, AiTerminalUnavailable> {
        Ok(capture_command_records(
            (self.0)(session, cx)?.read(cx).ai_command_records(),
        ))
    }

    fn selection(
        &mut self,
        session: TerminalSessionId,
        cx: &mut App,
    ) -> Result<Option<String>, AiTerminalUnavailable> {
        Ok((self.0)(session, cx)?.read(cx).selected_text_snapshot())
    }

    fn observe(
        &mut self,
        session: TerminalSessionId,
        run_command: bool,
        command: Option<&str>,
        cx: &mut App,
    ) -> AiTerminalWatchSample {
        match (self.0)(session, cx) {
            Ok(pane) => super::tools::capture_terminal_watch(pane.read(cx), run_command, command),
            Err(AiTerminalUnavailable::HostClosed) => AiTerminalWatchSample::HostClosed,
            Err(AiTerminalUnavailable::OwnerClosed) => AiTerminalWatchSample::OwnerClosed,
            Err(AiTerminalUnavailable::Recovering) => AiTerminalWatchSample::Recovering,
        }
    }

    fn send_input(
        &mut self,
        session: TerminalSessionId,
        bytes: &[u8],
        cx: &mut App,
    ) -> Result<bool, AiTerminalUnavailable> {
        Ok(send_input(&(self.0)(session, cx)?, bytes, cx))
    }

    fn control(
        &mut self,
        session: TerminalSessionId,
        request: AiTransportControl<'_>,
        cx: &mut App,
    ) -> Result<serde_json::Value, AiTerminalControlError> {
        let pane = (self.0)(session, cx).map_err(AiTerminalControlError::Unavailable)?;
        match request {
            AiTransportControl::State => super::tools::transport_state(&pane, cx),
            AiTransportControl::Serial(args) => {
                super::tools::serial_control(&pane, session, args, cx)
            }
            AiTransportControl::Telnet(args) => {
                super::tools::telnet_control(&pane, session, args, cx)
            }
        }
        .map_err(AiTerminalControlError::Rejected)
    }

    fn apply_inline(
        &mut self,
        session: TerminalSessionId,
        command: &str,
        execute: bool,
        cx: &mut App,
    ) -> Result<(), AiTerminalUnavailable> {
        apply_command(&(self.0)(session, cx)?, command, execute, cx);
        Ok(())
    }
}

pub(super) fn send_input(pane: &Entity<TerminalPane>, bytes: &[u8], cx: &mut App) -> bool {
    if !pane.read(cx).ai_accepts_input() {
        return false;
    }
    pane.update(cx, |pane, cx| pane.send_ai_input_bytes(bytes, cx));
    true
}

fn apply_command(pane: &Entity<TerminalPane>, command: &str, execute: bool, cx: &mut App) {
    pane.update(cx, |pane, cx| {
        if execute {
            pane.begin_command_mark(
                command,
                oxideterm_terminal::TerminalCommandMarkDetectionSource::Ai,
                cx,
            );
            pane.send_command_line(command, cx);
        } else {
            pane.send_ai_input_bytes(command.as_bytes(), cx);
        }
    });
}
