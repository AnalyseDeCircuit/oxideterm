// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

pub const TERMINAL_TRIGGERS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTriggersSnapshot {
    pub version: u32,
    pub triggers: Vec<TerminalTrigger>,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTrigger {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    #[serde(rename = "match")]
    pub matcher: TerminalTriggerMatch,
    pub action: TerminalTriggerAction,
    pub timing: TerminalTriggerTiming,
    pub scope: TerminalTriggerScope,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTriggerMatch {
    pub pattern: String,
    pub mode: TerminalTriggerMatchMode,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
}

impl std::fmt::Debug for TerminalTriggerMatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TerminalTriggerMatch")
            .field("pattern", &"<redacted>")
            .field("mode", &self.mode)
            .field("case_sensitive", &self.case_sensitive)
            .field("whole_word", &self.whole_word)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalTriggerMatchMode {
    Literal,
    Regex,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTriggerTiming {
    pub dispatch: TerminalTriggerDispatch,
    #[serde(default)]
    pub delay_ms: u64,
    pub cooldown_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalTriggerDispatch {
    Immediate,
    AfterNextLineBreak,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalTriggerScope {
    AllTerminals,
    LocalTerminals,
    SavedConnections {
        connections: Vec<SavedConnectionRef>,
    },
}

/// A saved connection target. The kind prevents collisions across protocol stores.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionRef {
    pub kind: SavedConnectionKind,
    pub id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedConnectionKind {
    Ssh,
    Telnet,
    Mosh,
    Serial,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalTriggerAction {
    SendText { text: String, append_enter: bool },
    RunQuickCommand { quick_command_id: String },
    LaunchLocalProcess { process: LocalProcessSpec },
}

impl std::fmt::Debug for TerminalTriggerAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::SendText { .. } => "send_text",
            Self::RunQuickCommand { .. } => "run_quick_command",
            Self::LaunchLocalProcess { .. } => "launch_local_process",
        };
        formatter
            .debug_struct("TerminalTriggerAction")
            .field("type", &kind)
            .field("content", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum LocalProcessSpec {
    DirectProgram {
        executable: String,
        arguments: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_directory: Option<String>,
    },
    ExplicitShell {
        shell_executable: String,
        arguments: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_directory: Option<String>,
    },
}

impl std::fmt::Debug for LocalProcessSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mode = match self {
            Self::DirectProgram { .. } => "direct_program",
            Self::ExplicitShell { .. } => "explicit_shell",
        };
        formatter
            .debug_struct("LocalProcessSpec")
            .field("mode", &mode)
            .field("content", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_debug_redacts_configured_pattern() {
        let matcher = TerminalTriggerMatch {
            pattern: "secret-pattern-sentinel".to_string(),
            mode: TerminalTriggerMatchMode::Regex,
            case_sensitive: true,
            whole_word: false,
        };

        let rendered = format!("{matcher:?}");
        assert!(!rendered.contains("secret-pattern-sentinel"));
        assert!(rendered.contains("<redacted>"));
    }
}
