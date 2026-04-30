// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Authoritative command facts for terminal runtime metadata.
//!
//! This store is intentionally independent from frontend xterm decorations.
//! Phase 1 uses it as a shadow-written fact model while the existing frontend
//! command marks continue to own presentation and hit-testing.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::scroll_buffer::BufferLineIdentity;

const MAX_COMMAND_TEXT_LENGTH: usize = 16_384;
const MAX_CWD_LENGTH: usize = 4_096;
const MAX_SOURCE_LENGTH: usize = 128;
const MAX_LEDGER_CANDIDATES_PER_SESSION: usize = 200;
const MAX_CANDIDATE_PREVIEW_CHARS: usize = 2_048;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandFactSource {
    CommandBar,
    Ai,
    Broadcast,
    UserInputObserved,
    Heuristic,
    ShellIntegration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandFactClosedBy {
    NextCommand,
    ShellIntegration,
    TerminalReset,
    SessionLost,
    InterruptedMode,
    Timeout,
    Manual,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandFactConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandFactStatus {
    Open,
    Closed,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandFactAuthorityMode {
    Shadow,
    Candidate,
    AuthoritativeLedger,
}

impl Default for CommandFactAuthorityMode {
    fn default() -> Self {
        Self::Shadow
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandFactLedgerDiagnosticReason {
    NullCommand,
    Stale,
    Interrupted,
    NotClosed,
    GenerationMismatch,
    RuntimeEpochMismatch,
    LowConfidence,
    CommandSanitizeFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFact {
    pub fact_id: String,
    pub client_mark_id: Option<String>,
    pub correlation_id: Option<String>,
    pub session_id: String,
    pub node_id: Option<String>,
    pub source: CommandFactSource,
    pub submitted_by: Option<CommandFactSource>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub start_global_line: u64,
    pub command_global_line: u64,
    pub output_start_global_line: Option<u64>,
    pub end_global_line: Option<u64>,
    pub buffer_generation: u64,
    pub runtime_epoch: String,
    pub status: CommandFactStatus,
    pub confidence: CommandFactConfidence,
    pub closed_by: Option<CommandFactClosedBy>,
    pub exit_code: Option<i32>,
    pub created_at: u64,
    pub closed_at: Option<u64>,
    pub stale_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFactLedgerCandidate {
    pub candidate_id: String,
    pub fact_id: String,
    pub session_id: String,
    pub node_id: Option<String>,
    pub source: CommandFactSource,
    pub submitted_by: Option<CommandFactSource>,
    pub command: String,
    pub cwd: Option<String>,
    pub start_global_line: u64,
    pub command_global_line: u64,
    pub output_start_global_line: Option<u64>,
    pub end_global_line: u64,
    pub buffer_generation: u64,
    pub runtime_epoch: String,
    pub closed_by: Option<CommandFactClosedBy>,
    pub exit_code: Option<i32>,
    pub created_at: u64,
    pub closed_at: u64,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFactLedgerDiagnostic {
    pub fact_id: String,
    pub session_id: String,
    pub reason: CommandFactLedgerDiagnosticReason,
    pub message: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFactLedgerDiagnosticsSnapshot {
    pub authority_mode: CommandFactAuthorityMode,
    pub candidates: Vec<CommandFactLedgerCandidate>,
    pub diagnostics: Vec<CommandFactLedgerDiagnostic>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommandFactRequest {
    pub client_mark_id: Option<String>,
    pub correlation_id: Option<String>,
    pub node_id: Option<String>,
    pub source: CommandFactSource,
    pub submitted_by: Option<CommandFactSource>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub start_global_line: u64,
    pub command_global_line: u64,
    pub output_start_global_line: Option<u64>,
    pub runtime_epoch: Option<String>,
    pub confidence: Option<CommandFactConfidence>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommandFactResponse {
    pub fact_id: String,
    pub fact: CommandFact,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseCommandFactPatch {
    pub end_global_line: Option<u64>,
    pub closed_by: Option<CommandFactClosedBy>,
    pub exit_code: Option<i32>,
    pub status: Option<CommandFactStatus>,
    pub stale_reason: Option<String>,
    pub runtime_epoch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFactOutputResponse {
    pub text: String,
    pub truncated: bool,
    pub line_count: usize,
    pub stale: bool,
}

#[derive(Default)]
struct CommandFactStoreInner {
    facts: Vec<CommandFact>,
    client_index: HashMap<String, String>,
    // Phase3a intentionally keeps Rust facts in shadow/candidate mode.
    // The React/xterm presentation path and production ledger are still the
    // user-visible authority until the terminal domain model is fully migrated
    // for the future Rust-native renderer/runtime work.
    authority_mode: CommandFactAuthorityMode,
    ledger_candidates: Vec<CommandFactLedgerCandidate>,
    ledger_diagnostics: Vec<CommandFactLedgerDiagnostic>,
}

#[derive(Default)]
pub struct CommandFactStore {
    inner: RwLock<CommandFactStoreInner>,
}

impl CommandFactStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn create_fact(
        &self,
        session_id: &str,
        request: CreateCommandFactRequest,
        identity: BufferLineIdentity,
    ) -> CommandFact {
        let now = now_millis();
        let fact_id = Uuid::new_v4().to_string();
        let command = request
            .command
            .as_deref()
            .map(sanitize_optional_text)
            .filter(|value| !value.is_empty());
        let cwd = request
            .cwd
            .as_deref()
            .map(|value| sanitize_text(value, MAX_CWD_LENGTH))
            .filter(|value| !value.is_empty());
        let runtime_epoch = request
            .runtime_epoch
            .as_deref()
            .map(|value| sanitize_text(value, MAX_SOURCE_LENGTH))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "default".to_string());
        let start_global_line = request.start_global_line.max(identity.base_global_line);
        let command_global_line = request.command_global_line.max(start_global_line);
        let output_start_global_line = request
            .output_start_global_line
            .map(|line| line.max(command_global_line));

        let fact = CommandFact {
            fact_id: fact_id.clone(),
            client_mark_id: request.client_mark_id.clone(),
            correlation_id: request.correlation_id,
            session_id: session_id.to_string(),
            node_id: request.node_id,
            source: request.source,
            submitted_by: request.submitted_by,
            command,
            cwd,
            start_global_line,
            command_global_line,
            output_start_global_line,
            end_global_line: None,
            buffer_generation: identity.buffer_generation,
            runtime_epoch,
            status: CommandFactStatus::Open,
            confidence: request.confidence.unwrap_or(CommandFactConfidence::High),
            closed_by: None,
            exit_code: None,
            created_at: now,
            closed_at: None,
            stale_reason: None,
        };

        let mut inner = self.inner.write().await;
        let auto_closed = conservatively_close_previous_open_fact(&mut inner.facts, &fact, now);
        if let Some(auto_closed) = auto_closed {
            record_ledger_candidate_or_diagnostic(
                &mut inner,
                &auto_closed,
                Some(identity.buffer_generation),
                Some(auto_closed.runtime_epoch.clone()),
                None,
                now,
            );
        }
        if let Some(client_mark_id) = &fact.client_mark_id {
            inner
                .client_index
                .insert(client_mark_id.clone(), fact_id.clone());
        }
        inner.facts.push(fact.clone());
        fact
    }

    pub async fn close_fact(
        &self,
        fact_id: &str,
        patch: CloseCommandFactPatch,
        current_buffer_generation: Option<u64>,
        preview: Option<String>,
    ) -> Option<CommandFact> {
        let mut inner = self.inner.write().await;
        let now = now_millis();
        let current_runtime_epoch = patch
            .runtime_epoch
            .as_deref()
            .map(|value| sanitize_text(value, MAX_SOURCE_LENGTH))
            .filter(|value| !value.is_empty());
        let (fact, should_record_close) = {
            let fact = inner
                .facts
                .iter_mut()
                .find(|candidate| candidate.fact_id == fact_id)?;
            let should_record_close = fact.status == CommandFactStatus::Open;
            apply_close_patch(fact, patch, now);
            (fact.clone(), should_record_close)
        };
        if should_record_close {
            record_ledger_candidate_or_diagnostic(
                &mut inner,
                &fact,
                current_buffer_generation,
                current_runtime_epoch,
                preview,
                now,
            );
        }
        Some(fact)
    }

    pub async fn get_fact(&self, fact_id: &str) -> Option<CommandFact> {
        let inner = self.inner.read().await;
        inner
            .facts
            .iter()
            .find(|candidate| candidate.fact_id == fact_id)
            .cloned()
    }

    pub async fn query_facts(&self, global_start: u64, global_end: u64) -> Vec<CommandFact> {
        let (start, end) = if global_start <= global_end {
            (global_start, global_end)
        } else {
            (global_end, global_start)
        };
        let inner = self.inner.read().await;
        inner
            .facts
            .iter()
            .filter(|fact| {
                let fact_end = fact.end_global_line.unwrap_or(fact.start_global_line);
                fact.start_global_line <= end && fact_end >= start
            })
            .cloned()
            .collect()
    }

    pub async fn mark_open_facts_stale(
        &self,
        reason: &str,
        closed_by: CommandFactClosedBy,
    ) -> Vec<CommandFact> {
        let now = now_millis();
        let stale_reason = sanitize_text(reason, MAX_SOURCE_LENGTH);
        let mut inner = self.inner.write().await;
        let mut changed = Vec::new();
        for fact in &mut inner.facts {
            if fact.status != CommandFactStatus::Open {
                continue;
            }
            fact.status = CommandFactStatus::Stale;
            fact.closed_by = Some(closed_by.clone());
            fact.closed_at = Some(now);
            fact.stale_reason = Some(stale_reason.clone());
            changed.push(fact.clone());
        }
        changed
    }

    pub async fn ledger_diagnostics_snapshot(&self) -> CommandFactLedgerDiagnosticsSnapshot {
        let inner = self.inner.read().await;
        CommandFactLedgerDiagnosticsSnapshot {
            authority_mode: inner.authority_mode.clone(),
            candidates: inner.ledger_candidates.clone(),
            diagnostics: inner.ledger_diagnostics.clone(),
        }
    }
}

fn conservatively_close_previous_open_fact(
    facts: &mut [CommandFact],
    next_fact: &CommandFact,
    now: u64,
) -> Option<CommandFact> {
    if let Some(previous) = facts.iter_mut().rev().find(|fact| {
        fact.session_id == next_fact.session_id
            && fact.status == CommandFactStatus::Open
            && fact.buffer_generation == next_fact.buffer_generation
            && fact.runtime_epoch == next_fact.runtime_epoch
    }) {
        previous.status = CommandFactStatus::Closed;
        previous.closed_by = Some(CommandFactClosedBy::NextCommand);
        previous.end_global_line = Some(next_fact.start_global_line.saturating_sub(1));
        previous.closed_at = Some(now);
        return Some(previous.clone());
    }
    None
}

fn apply_close_patch(fact: &mut CommandFact, patch: CloseCommandFactPatch, now: u64) {
    let requested_status = patch.status.unwrap_or(CommandFactStatus::Closed);
    fact.status = requested_status;
    fact.end_global_line = patch
        .end_global_line
        .map(|line| line.max(fact.start_global_line))
        .or(fact.end_global_line);
    fact.closed_by = patch.closed_by.or_else(|| fact.closed_by.clone());
    fact.exit_code = patch.exit_code.or(fact.exit_code);
    fact.closed_at = Some(now);
    fact.stale_reason = patch
        .stale_reason
        .as_deref()
        .map(|value| sanitize_text(value, MAX_SOURCE_LENGTH))
        .or_else(|| fact.stale_reason.clone());
}

fn sanitize_optional_text(value: &str) -> String {
    sanitize_text(value, MAX_COMMAND_TEXT_LENGTH)
}

fn sanitize_text(value: &str, max_len: usize) -> String {
    value
        .chars()
        .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
        .take(max_len)
        .collect()
}

fn now_millis() -> u64 {
    Utc::now().timestamp_millis().max(0) as u64
}

fn record_ledger_candidate_or_diagnostic(
    inner: &mut CommandFactStoreInner,
    fact: &CommandFact,
    current_buffer_generation: Option<u64>,
    current_runtime_epoch: Option<String>,
    preview: Option<String>,
    now: u64,
) {
    match build_ledger_candidate(
        fact,
        current_buffer_generation,
        current_runtime_epoch,
        preview,
    ) {
        Ok(candidate) => push_capped(
            &mut inner.ledger_candidates,
            candidate,
            MAX_LEDGER_CANDIDATES_PER_SESSION,
        ),
        Err(reason) => {
            let diagnostic = CommandFactLedgerDiagnostic {
                fact_id: fact.fact_id.clone(),
                session_id: fact.session_id.clone(),
                message: ledger_diagnostic_message(&reason),
                reason,
                created_at: now,
            };
            push_capped(
                &mut inner.ledger_diagnostics,
                diagnostic,
                MAX_LEDGER_CANDIDATES_PER_SESSION,
            );
        }
    }
}

fn build_ledger_candidate(
    fact: &CommandFact,
    current_buffer_generation: Option<u64>,
    current_runtime_epoch: Option<String>,
    preview: Option<String>,
) -> Result<CommandFactLedgerCandidate, CommandFactLedgerDiagnosticReason> {
    let command = fact
        .command
        .as_deref()
        .map(sanitize_optional_text)
        .filter(|value| !value.is_empty())
        .ok_or(CommandFactLedgerDiagnosticReason::NullCommand)?;
    if fact.confidence != CommandFactConfidence::High {
        return Err(CommandFactLedgerDiagnosticReason::LowConfidence);
    }
    if fact.status == CommandFactStatus::Stale {
        return Err(CommandFactLedgerDiagnosticReason::Stale);
    }
    if fact.status != CommandFactStatus::Closed {
        return Err(CommandFactLedgerDiagnosticReason::NotClosed);
    }
    if matches!(
        fact.closed_by,
        Some(
            CommandFactClosedBy::InterruptedMode
                | CommandFactClosedBy::TerminalReset
                | CommandFactClosedBy::SessionLost
        )
    ) {
        return Err(CommandFactLedgerDiagnosticReason::Interrupted);
    }
    if let Some(generation) = current_buffer_generation {
        if generation != fact.buffer_generation {
            return Err(CommandFactLedgerDiagnosticReason::GenerationMismatch);
        }
    }
    if let Some(runtime_epoch) = current_runtime_epoch {
        if runtime_epoch != fact.runtime_epoch {
            return Err(CommandFactLedgerDiagnosticReason::RuntimeEpochMismatch);
        }
    }
    let end_global_line = fact
        .end_global_line
        .ok_or(CommandFactLedgerDiagnosticReason::NotClosed)?;
    let closed_at = fact
        .closed_at
        .ok_or(CommandFactLedgerDiagnosticReason::NotClosed)?;

    Ok(CommandFactLedgerCandidate {
        candidate_id: Uuid::new_v4().to_string(),
        fact_id: fact.fact_id.clone(),
        session_id: fact.session_id.clone(),
        node_id: fact.node_id.clone(),
        source: fact.source.clone(),
        submitted_by: fact.submitted_by.clone(),
        command,
        cwd: fact.cwd.clone(),
        start_global_line: fact.start_global_line,
        command_global_line: fact.command_global_line,
        output_start_global_line: fact.output_start_global_line,
        end_global_line,
        buffer_generation: fact.buffer_generation,
        runtime_epoch: fact.runtime_epoch.clone(),
        closed_by: fact.closed_by.clone(),
        exit_code: fact.exit_code,
        created_at: fact.created_at,
        closed_at,
        preview: preview.map(|value| sanitize_text(&value, MAX_CANDIDATE_PREVIEW_CHARS)),
    })
}

fn ledger_diagnostic_message(reason: &CommandFactLedgerDiagnosticReason) -> String {
    match reason {
        CommandFactLedgerDiagnosticReason::NullCommand => "command is empty".to_string(),
        CommandFactLedgerDiagnosticReason::Stale => "fact is stale".to_string(),
        CommandFactLedgerDiagnosticReason::Interrupted => "fact was interrupted".to_string(),
        CommandFactLedgerDiagnosticReason::NotClosed => "fact is not normally closed".to_string(),
        CommandFactLedgerDiagnosticReason::GenerationMismatch => {
            "buffer generation mismatch".to_string()
        }
        CommandFactLedgerDiagnosticReason::RuntimeEpochMismatch => {
            "runtime epoch mismatch".to_string()
        }
        CommandFactLedgerDiagnosticReason::LowConfidence => {
            "fact confidence is not high".to_string()
        }
        CommandFactLedgerDiagnosticReason::CommandSanitizeFailed => {
            "command sanitize failed".to_string()
        }
    }
}

fn push_capped<T>(items: &mut Vec<T>, item: T, limit: usize) {
    items.push(item);
    if items.len() > limit {
        let overflow = items.len() - limit;
        items.drain(0..overflow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> BufferLineIdentity {
        BufferLineIdentity {
            current_lines: 10,
            total_lines: 20,
            base_global_line: 10,
            buffer_generation: 2,
        }
    }

    fn request(client_mark_id: &str, line: u64) -> CreateCommandFactRequest {
        CreateCommandFactRequest {
            client_mark_id: Some(client_mark_id.to_string()),
            correlation_id: None,
            node_id: Some("node-1".to_string()),
            source: CommandFactSource::CommandBar,
            submitted_by: None,
            command: Some("ls -la".to_string()),
            cwd: Some("/tmp".to_string()),
            start_global_line: line,
            command_global_line: line,
            output_start_global_line: Some(line + 1),
            runtime_epoch: Some("epoch-1".to_string()),
            confidence: Some(CommandFactConfidence::High),
        }
    }

    #[tokio::test]
    async fn generated_fact_id_is_authoritative() {
        let store = CommandFactStore::new();
        let fact = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;

        assert_ne!(fact.fact_id, "client-1");
        assert_eq!(fact.client_mark_id.as_deref(), Some("client-1"));
        assert_eq!(fact.buffer_generation, 2);
    }

    #[tokio::test]
    async fn next_command_conservatively_closes_previous_open_fact() {
        let store = CommandFactStore::new();
        let first = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;
        store
            .create_fact("session-1", request("client-2", 15), identity())
            .await;

        let closed = store.get_fact(&first.fact_id).await.unwrap();
        assert_eq!(closed.status, CommandFactStatus::Closed);
        assert_eq!(closed.closed_by, Some(CommandFactClosedBy::NextCommand));
        assert_eq!(closed.end_global_line, Some(14));
    }

    #[tokio::test]
    async fn close_fact_can_mark_stale() {
        let store = CommandFactStore::new();
        let fact = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;
        let closed = store
            .close_fact(
                &fact.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(13),
                    closed_by: Some(CommandFactClosedBy::TerminalReset),
                    exit_code: None,
                    status: Some(CommandFactStatus::Stale),
                    stale_reason: Some("terminal_reset".to_string()),
                    runtime_epoch: Some("epoch-1".to_string()),
                },
                Some(identity().buffer_generation),
                None,
            )
            .await
            .unwrap();

        assert_eq!(closed.status, CommandFactStatus::Stale);
        assert_eq!(closed.end_global_line, Some(13));
        assert_eq!(closed.stale_reason.as_deref(), Some("terminal_reset"));
    }

    #[tokio::test]
    async fn range_query_intersects_closed_facts() {
        let store = CommandFactStore::new();
        let fact = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;
        store
            .close_fact(
                &fact.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(16),
                    closed_by: Some(CommandFactClosedBy::ShellIntegration),
                    exit_code: Some(0),
                    status: None,
                    stale_reason: None,
                    runtime_epoch: Some("epoch-1".to_string()),
                },
                Some(identity().buffer_generation),
                None,
            )
            .await;

        assert_eq!(store.query_facts(15, 20).await.len(), 1);
        assert_eq!(store.query_facts(17, 20).await.len(), 0);
    }

    #[tokio::test]
    async fn mark_open_facts_stale_only_changes_open_facts() {
        let store = CommandFactStore::new();
        let first = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;
        store
            .close_fact(
                &first.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(13),
                    closed_by: Some(CommandFactClosedBy::Manual),
                    exit_code: None,
                    status: None,
                    stale_reason: None,
                    runtime_epoch: Some("epoch-1".to_string()),
                },
                Some(identity().buffer_generation),
                None,
            )
            .await;
        let second = store
            .create_fact("session-1", request("client-2", 15), identity())
            .await;

        let changed = store
            .mark_open_facts_stale("clear_buffer", CommandFactClosedBy::TerminalReset)
            .await;

        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].fact_id, second.fact_id);
        assert_eq!(changed[0].status, CommandFactStatus::Stale);
    }

    #[tokio::test]
    async fn closed_high_confidence_fact_generates_ledger_candidate() {
        let store = CommandFactStore::new();
        let fact = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;

        store
            .close_fact(
                &fact.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(16),
                    closed_by: Some(CommandFactClosedBy::ShellIntegration),
                    exit_code: Some(0),
                    status: None,
                    stale_reason: None,
                    runtime_epoch: Some("epoch-1".to_string()),
                },
                Some(identity().buffer_generation),
                Some("line\noutput".to_string()),
            )
            .await;

        let snapshot = store.ledger_diagnostics_snapshot().await;
        assert_eq!(snapshot.authority_mode, CommandFactAuthorityMode::Shadow);
        assert_eq!(snapshot.candidates.len(), 1);
        assert_eq!(snapshot.diagnostics.len(), 0);
        assert_eq!(snapshot.candidates[0].fact_id, fact.fact_id);
        assert_eq!(snapshot.candidates[0].command, "ls -la");
        assert_eq!(
            snapshot.candidates[0].preview.as_deref(),
            Some("line\noutput")
        );
    }

    #[tokio::test]
    async fn closing_an_already_closed_fact_does_not_duplicate_candidate() {
        let store = CommandFactStore::new();
        let first = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;
        store
            .create_fact("session-1", request("client-2", 15), identity())
            .await;

        store
            .close_fact(
                &first.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(14),
                    closed_by: Some(CommandFactClosedBy::NextCommand),
                    exit_code: None,
                    status: None,
                    stale_reason: None,
                    runtime_epoch: Some("epoch-1".to_string()),
                },
                Some(identity().buffer_generation),
                None,
            )
            .await;

        let snapshot = store.ledger_diagnostics_snapshot().await;
        assert_eq!(snapshot.candidates.len(), 1);
        assert_eq!(snapshot.candidates[0].fact_id, first.fact_id);
    }

    #[tokio::test]
    async fn null_command_generates_diagnostic_only() {
        let store = CommandFactStore::new();
        let mut null_request = request("client-1", 12);
        null_request.command = None;
        let fact = store
            .create_fact("session-1", null_request, identity())
            .await;

        store
            .close_fact(
                &fact.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(13),
                    closed_by: Some(CommandFactClosedBy::ShellIntegration),
                    exit_code: Some(0),
                    status: None,
                    stale_reason: None,
                    runtime_epoch: Some("epoch-1".to_string()),
                },
                Some(identity().buffer_generation),
                None,
            )
            .await;

        let snapshot = store.ledger_diagnostics_snapshot().await;
        assert_eq!(snapshot.candidates.len(), 0);
        assert_eq!(snapshot.diagnostics.len(), 1);
        assert_eq!(
            snapshot.diagnostics[0].reason,
            CommandFactLedgerDiagnosticReason::NullCommand
        );
    }

    #[tokio::test]
    async fn stale_fact_generates_diagnostic_only() {
        let store = CommandFactStore::new();
        let fact = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;

        store
            .close_fact(
                &fact.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(13),
                    closed_by: Some(CommandFactClosedBy::TerminalReset),
                    exit_code: None,
                    status: Some(CommandFactStatus::Stale),
                    stale_reason: Some("terminal_reset".to_string()),
                    runtime_epoch: Some("epoch-1".to_string()),
                },
                Some(identity().buffer_generation),
                None,
            )
            .await;

        let snapshot = store.ledger_diagnostics_snapshot().await;
        assert_eq!(snapshot.candidates.len(), 0);
        assert_eq!(snapshot.diagnostics.len(), 1);
        assert_eq!(
            snapshot.diagnostics[0].reason,
            CommandFactLedgerDiagnosticReason::Stale
        );
    }

    #[tokio::test]
    async fn generation_mismatch_generates_diagnostic_only() {
        let store = CommandFactStore::new();
        let fact = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;

        store
            .close_fact(
                &fact.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(13),
                    closed_by: Some(CommandFactClosedBy::ShellIntegration),
                    exit_code: Some(0),
                    status: None,
                    stale_reason: None,
                    runtime_epoch: Some("epoch-1".to_string()),
                },
                Some(identity().buffer_generation + 1),
                None,
            )
            .await;

        let snapshot = store.ledger_diagnostics_snapshot().await;
        assert_eq!(snapshot.candidates.len(), 0);
        assert_eq!(snapshot.diagnostics.len(), 1);
        assert_eq!(
            snapshot.diagnostics[0].reason,
            CommandFactLedgerDiagnosticReason::GenerationMismatch
        );
    }

    #[tokio::test]
    async fn runtime_epoch_mismatch_generates_diagnostic_only() {
        let store = CommandFactStore::new();
        let fact = store
            .create_fact("session-1", request("client-1", 12), identity())
            .await;

        store
            .close_fact(
                &fact.fact_id,
                CloseCommandFactPatch {
                    end_global_line: Some(13),
                    closed_by: Some(CommandFactClosedBy::ShellIntegration),
                    exit_code: Some(0),
                    status: None,
                    stale_reason: None,
                    runtime_epoch: Some("other-epoch".to_string()),
                },
                Some(identity().buffer_generation),
                None,
            )
            .await;

        let snapshot = store.ledger_diagnostics_snapshot().await;
        assert_eq!(snapshot.candidates.len(), 0);
        assert_eq!(snapshot.diagnostics.len(), 1);
        assert_eq!(
            snapshot.diagnostics[0].reason,
            CommandFactLedgerDiagnosticReason::RuntimeEpochMismatch
        );
    }

    #[tokio::test]
    async fn ledger_candidates_are_capped_and_preview_is_truncated() {
        let store = CommandFactStore::new();
        let long_preview = "x".repeat(MAX_CANDIDATE_PREVIEW_CHARS + 100);

        for index in 0..205 {
            let fact = store
                .create_fact(
                    "session-1",
                    request(&format!("client-{index}"), 12 + index),
                    identity(),
                )
                .await;
            store
                .close_fact(
                    &fact.fact_id,
                    CloseCommandFactPatch {
                        end_global_line: Some(13 + index),
                        closed_by: Some(CommandFactClosedBy::ShellIntegration),
                        exit_code: Some(0),
                        status: None,
                        stale_reason: None,
                        runtime_epoch: Some("epoch-1".to_string()),
                    },
                    Some(identity().buffer_generation),
                    Some(long_preview.clone()),
                )
                .await;
        }

        let snapshot = store.ledger_diagnostics_snapshot().await;
        assert_eq!(snapshot.candidates.len(), MAX_LEDGER_CANDIDATES_PER_SESSION);
        assert!(snapshot.candidates.iter().all(|candidate| {
            candidate
                .preview
                .as_ref()
                .is_some_and(|preview| preview.chars().count() == MAX_CANDIDATE_PREVIEW_CHARS)
        }));
    }
}
