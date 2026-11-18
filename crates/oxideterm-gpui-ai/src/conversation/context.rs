use std::collections::HashSet;

use crate::state::detect_ai_cli_agent_kind;

#[derive(Clone)]
pub struct AiRuntimeCommandRecord {
    pub command_id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub source: String,
    pub status: String,
    pub exit_code: Option<i64>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub approval_mode: Option<String>,
    pub risk: String,
}

impl std::fmt::Debug for AiRuntimeCommandRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Command text and working directories can contain credentials.
        formatter
            .debug_struct("AiRuntimeCommandRecord")
            .field("command_id", &self.command_id)
            .field("command", &"[redacted]")
            .field("cwd", &self.cwd.as_ref().map(|_| "[redacted]"))
            .field("source", &self.source)
            .field("status", &self.status)
            .field("exit_code", &self.exit_code)
            .field("started_at", &self.started_at)
            .field("finished_at", &self.finished_at)
            .field("approval_mode", &self.approval_mode)
            .field("risk", &self.risk)
            .finish()
    }
}

#[derive(Clone)]
struct AiCliAgentSession {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    pub live_terminal: bool,
    pub started_at: i64,
    pub updated_at: i64,
}

impl std::fmt::Debug for AiCliAgentSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Agent launch commands share the same secret-bearing boundary.
        formatter
            .debug_struct("AiCliAgentSession")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("label", &self.label)
            .field("status", &self.status)
            .field("live_terminal", &self.live_terminal)
            .field("started_at", &self.started_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

pub struct AiIdeContext {
    pub project_root: String,
    pub project_name: String,
    pub git_branch: Option<String>,
    pub active_file: Option<String>,
    pub active_language: Option<String>,
    pub is_dirty: bool,
    pub open_tab_count: usize,
    pub open_tab_paths: Vec<String>,
}

pub struct AiEnvironmentSnapshot<'a> {
    pub local_os: &'a str,
    pub active_tab: Option<&'a str>,
    pub cwd: Option<String>,
    pub terminal: Option<String>,
    pub ssh_endpoint: Option<(&'a str, &'a str, u16)>,
    pub tab_count: usize,
    pub terminal_count: usize,
    pub ide: Option<AiIdeContext>,
    pub sftp: Option<(String, Vec<String>)>,
}

pub fn build_environment_prompt(
    environment: AiEnvironmentSnapshot<'_>,
    command_records: Vec<AiRuntimeCommandRecord>,
) -> String {
    let mut parts = Vec::new();
    parts.push("## Environment".to_string());
    parts.push(format!(
        "- OxideTerm host OS (local application only): {}",
        environment.local_os
    ));
    if let Some(tab) = environment.active_tab {
        parts.push(format!("- Active page: {tab}"));
    }
    if let Some(cwd) = environment.cwd {
        parts.push(format!(
            "- Active terminal CWD (inferred; verify on the selected target): {cwd}"
        ));
    }
    if let Some(terminal) = environment.terminal {
        parts.push(format!("- Active terminal: {terminal}"));
        if let Some((user, host, port)) = environment.ssh_endpoint {
            parts.push(format!("- Active SSH endpoint: {user}@{host}:{port}"));
        }
    } else {
        parts.push("- Terminal: No active terminal".to_string());
    }
    parts.push(String::new());
    parts.push("## Runtime State".to_string());
    parts.push(format!("- Open tabs: {}", environment.tab_count));
    parts.push(format!(
        "- Runtime terminal sessions: {}",
        environment.terminal_count
    ));
    parts.push(
            "- Tabs, pane ids, and terminal session ids are memory-only and do not survive an app restart/reload."
                .to_string(),
        );

    if let Some(ide) = environment.ide {
        parts.push(String::new());
        parts.push("## IDE Context".to_string());
        parts.push(format!(
            "- Project: {} ({})",
            ide.project_name, ide.project_root
        ));
        if let Some(branch) = ide.git_branch {
            parts.push(format!("- Git: {branch}"));
        }
        if let Some(active_file) = ide.active_file {
            let language = ide.active_language.unwrap_or_else(|| "unknown".to_string());
            let dirty = if ide.is_dirty { " [unsaved]" } else { "" };
            parts.push(format!("- Editing: {active_file} ({language}){dirty}"));
        }
        if ide.open_tab_count > 1 {
            parts.push(format!(
                "- Open tabs ({}): {}",
                ide.open_tab_count,
                ide.open_tab_paths.join(", ")
            ));
        }
    }

    if let Some((remote_path, selected_files)) = environment.sftp {
        parts.push(String::new());
        parts.push("## File Browser Context".to_string());
        parts.push(format!("- CWD: {remote_path}"));
        if !selected_files.is_empty() {
            let shown = selected_files.iter().take(20).cloned().collect::<Vec<_>>();
            let suffix = if selected_files.len() > 20 {
                format!(" ... +{} more", selected_files.len() - 20)
            } else {
                String::new()
            };
            parts.push(format!(
                "- Selected ({}): [{}{}]",
                selected_files.len(),
                shown.join(", "),
                suffix
            ));
        }
    }

    if let Some(context_chips) = build_runtime_context_chips(command_records) {
        parts.push(String::new());
        parts.push(context_chips);
    }

    parts.join("\n")
}

pub fn build_runtime_context_chips(
    mut command_records: Vec<AiRuntimeCommandRecord>,
) -> Option<String> {
    let mut chips = Vec::new();
    command_records.sort_by(|left, right| {
        right
            .finished_at
            .unwrap_or(right.started_at)
            .cmp(&left.finished_at.unwrap_or(left.started_at))
    });
    for record in command_records.iter().take(5) {
        let kind = if record.status == "error" {
            "recent_error"
        } else {
            "recent_command"
        };
        let exit_suffix = record
            .exit_code
            .map(|code| format!(" (exit {code})"))
            .unwrap_or_default();
        chips.push(format!(
            "- {kind}:{} {}",
            exit_suffix,
            serde_json::json!({
                "status": record.status,
                "approvalMode": record.approval_mode,
                "source": record.source,
                "risk": record.risk,
                "commandChars": record.command.chars().count(),
                "hasCwd": record.cwd.is_some(),
            })
        ));
        if chips.len() >= 8 {
            break;
        }
    }
    if chips.len() < 8 {
        let mut sessions = cli_agent_sessions(&command_records);
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        for session in sessions.into_iter().take(3) {
            chips.push(format!(
                "- cli_agent: {} is {}{} {}",
                session.kind,
                session.status,
                session
                    .live_terminal
                    .then(|| " in a live terminal".to_string())
                    .unwrap_or_default(),
                serde_json::json!({
                    "kind": session.kind,
                    "status": session.status,
                    "label": session.label,
                })
            ));
            if chips.len() >= 8 {
                break;
            }
        }
    }
    if chips.is_empty() {
        return None;
    }
    Some(
            [
                "## Runtime Context Chips".to_string(),
                "These are non-authoritative runtime hints. Rediscover a current handle before acting on a live target.".to_string(),
            ]
            .into_iter()
            .chain(chips)
            .collect::<Vec<_>>()
            .join("\n"),
        )
}

fn cli_agent_sessions(records: &[AiRuntimeCommandRecord]) -> Vec<AiCliAgentSession> {
    let mut sessions = Vec::new();
    let mut seen = HashSet::new();
    for record in records {
        let Some(kind) = detect_ai_cli_agent_kind(&record.command) else {
            continue;
        };
        let id = format!("cli-agent:{kind}:live-terminal");
        if !seen.insert(id.clone()) {
            continue;
        }
        let status = match record.status.as_str() {
            "waiting_for_input" => "waiting_for_input",
            "error" => "failed",
            _ => "running",
        };
        sessions.push(AiCliAgentSession {
            id,
            kind: kind.clone(),
            label: format!("{kind} agent"),
            status: status.to_string(),
            live_terminal: true,
            started_at: record.started_at,
            updated_at: record.finished_at.unwrap_or(record.started_at),
        });
    }
    sessions
}

use oxideterm_ai::{AiReferenceMatch, ai_reference_context_block, extract_ai_error_context, infer_ai_cwd};
use oxideterm_workspace::{PaneId, TerminalSessionId};

#[derive(Clone, Copy)]
pub enum AiTerminalReferenceSource { ActiveBuffer, ActiveSelection, Pane(usize) }

pub fn build_reference_context(
    references: &[AiReferenceMatch], mut snapshot: impl FnMut(AiTerminalReferenceSource) -> Option<String>,
) -> Option<String> {
    use AiTerminalReferenceSource::*;
    let blocks = references.iter().filter_map(|reference| {
        let content = match reference.reference_type.as_str() {
            "buffer" => snapshot(ActiveBuffer),
            "selection" => snapshot(ActiveSelection),
            "error" => snapshot(ActiveBuffer).and_then(|text| extract_ai_error_context(&text)),
            "pane" => snapshot(Pane(reference.value.as_deref()?.parse::<usize>().ok()?.checked_sub(1)?)),
            "cwd" => snapshot(ActiveBuffer).and_then(|text| infer_ai_cwd(&text)),
            _ => None,
        }?;
        ai_reference_context_block(reference, &content)
    }).collect::<Vec<_>>();
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

pub fn build_ide_code_context(file: Option<&str>, language: Option<&str>, snippet: Option<&str>, first_line: usize) -> Option<String> {
    let (file, snippet) = (file?, snippet?);
    let language = language.unwrap_or("text");
    Some(format!("=== Code: {file} ({language}, lines {first_line}+) ===\n{snippet}"))
}

pub struct AiTerminalPaneContext {
    pub pane: PaneId,
    pub session: Option<TerminalSessionId>,
    pub active: bool,
    pub buffer: String,
    pub environment: String,
    pub label: String,
}

fn append_selection(parts: &mut Vec<String>, selection: Option<String>) {
    if let Some(selection) = selection {
        parts.push("=== SELECTED TEXT (Focus Area) ===".to_string());
        parts.push(selection);
        parts.push(String::new());
    }
}

pub fn build_single_terminal_context(selection: Option<String>, buffer: Option<&str>, budget: usize) -> Option<String> {
    let mut parts = Vec::new();
    append_selection(&mut parts, selection);
    if let Some(buffer) = buffer {
        let (buffer, lines) = limit_terminal_buffer(buffer, usize::MAX, budget);
        if !buffer.trim().is_empty() {
            parts.push(format!("=== Terminal Output (last {lines} lines) ==="));
            parts.push(buffer);
        }
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

pub fn build_split_terminal_context(selection: Option<String>, panes: impl Iterator<Item = AiTerminalPaneContext>) -> Option<String> {
    let mut parts = Vec::new();
    append_selection(&mut parts, selection);
    for pane in panes {
        let (buffer, line_count) = limit_terminal_buffer(&pane.buffer, 30, 4000);
        if buffer.trim().is_empty() { continue; }
        let label = if pane.active { "Active Pane" } else { "Pane" };
        let session = pane.session?;
        let environment = pane.environment;
        let target = oxideterm_ai::sanitize_for_ai(&pane.label);
        parts.push(format!("=== {label} #{} / session {} ({environment}; {target}) — last {line_count} lines ===", pane.pane.0, session.0));
        parts.push(buffer);
        parts.push(String::new());
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn limit_terminal_buffer(
        buffer: &str,
        max_lines: usize,
        max_chars: usize,
    ) -> (String, usize) {
        let max_lines = max_lines.max(1);
        let mut lines = buffer.lines().rev().take(max_lines).collect::<Vec<_>>();
        lines.reverse();
        let mut text = lines.join("\n");
        if max_chars > 0 && text.len() > max_chars {
            let start = text
                .char_indices()
                .map(|(index, _)| index)
                .find(|index| text.len().saturating_sub(*index) <= max_chars)
                .unwrap_or(0);
            text = text[start..].to_string();
        }
        let line_count = text.lines().count();
        (text, line_count)
    }


use oxideterm_terminal::TerminalCommandMarkDetectionSource;

pub fn ai_local_os_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

pub fn ai_terminal_environment(kind: oxideterm_terminal::TerminalSessionKind, local_os: &str) -> String {
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
        TerminalSessionKind::Ble => {
            "BLE device (OS and command interface unverified)".to_string()
        }
    }
}

fn ai_ledger_source_from_terminal_source(
    source: TerminalCommandMarkDetectionSource,
) -> String {
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
