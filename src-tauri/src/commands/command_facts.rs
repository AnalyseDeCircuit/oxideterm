// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Tauri commands for authoritative terminal command facts.

use std::sync::Arc;

use tauri::State;

use crate::session::{
    CloseCommandFactPatch, CommandFact, CommandFactLedgerDiagnosticsSnapshot,
    CommandFactOutputResponse, CreateCommandFactRequest, CreateCommandFactResponse, ScrollBuffer,
    SessionRegistry,
};

const DEFAULT_OUTPUT_MAX_LINES: usize = 400;
const DEFAULT_OUTPUT_MAX_CHARS: usize = 24_000;
const HARD_OUTPUT_MAX_LINES: usize = 5_000;
const HARD_OUTPUT_MAX_CHARS: usize = 512_000;

#[tauri::command]
pub async fn create_command_fact(
    session_id: String,
    request: CreateCommandFactRequest,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<CreateCommandFactResponse, String> {
    let (store, scroll_buffer) = registry
        .with_session(&session_id, |entry| {
            (entry.command_facts.clone(), entry.scroll_buffer.clone())
        })
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    let identity = scroll_buffer.identity().await;
    let fact = store.create_fact(&session_id, request, identity).await;
    Ok(CreateCommandFactResponse {
        fact_id: fact.fact_id.clone(),
        fact,
    })
}

#[tauri::command]
pub async fn close_command_fact(
    session_id: String,
    fact_id: String,
    patch: CloseCommandFactPatch,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<CommandFact, String> {
    let (store, scroll_buffer) = registry
        .with_session(&session_id, |entry| {
            (entry.command_facts.clone(), entry.scroll_buffer.clone())
        })
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    let identity = scroll_buffer.identity().await;
    store
        .close_fact(&fact_id, patch, Some(identity.buffer_generation), None)
        .await
        .ok_or_else(|| format!("Command fact {} not found", fact_id))
}

#[tauri::command]
pub async fn get_command_facts(
    session_id: String,
    global_start: u64,
    global_end: u64,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<Vec<CommandFact>, String> {
    let store = registry
        .with_session(&session_id, |entry| entry.command_facts.clone())
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    Ok(store.query_facts(global_start, global_end).await)
}

#[tauri::command]
pub async fn get_command_fact_output(
    session_id: String,
    fact_id: String,
    max_lines: Option<usize>,
    max_chars: Option<usize>,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<CommandFactOutputResponse, String> {
    let (store, scroll_buffer) = registry
        .with_session(&session_id, |entry| {
            (entry.command_facts.clone(), entry.scroll_buffer.clone())
        })
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    let fact = store
        .get_fact(&fact_id)
        .await
        .ok_or_else(|| format!("Command fact {} not found", fact_id))?;
    read_fact_output(scroll_buffer, fact, max_lines, max_chars).await
}

#[tauri::command]
pub async fn get_command_fact_ledger_diagnostics(
    session_id: String,
    registry: State<'_, Arc<SessionRegistry>>,
) -> Result<CommandFactLedgerDiagnosticsSnapshot, String> {
    let store = registry
        .with_session(&session_id, |entry| entry.command_facts.clone())
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    Ok(store.ledger_diagnostics_snapshot().await)
}

async fn read_fact_output(
    scroll_buffer: Arc<ScrollBuffer>,
    fact: CommandFact,
    max_lines: Option<usize>,
    max_chars: Option<usize>,
) -> Result<CommandFactOutputResponse, String> {
    let identity = scroll_buffer.identity().await;
    if fact.buffer_generation != identity.buffer_generation {
        return Ok(CommandFactOutputResponse {
            text: String::new(),
            truncated: false,
            line_count: 0,
            stale: true,
        });
    }

    let start_global_line = fact
        .output_start_global_line
        .unwrap_or_else(|| fact.command_global_line.saturating_add(1));
    let end_global_line = fact
        .end_global_line
        .unwrap_or(fact.command_global_line)
        .max(start_global_line);
    let Some(hot_start) = scroll_buffer
        .hot_index_for_global_line(start_global_line)
        .await
    else {
        return Ok(CommandFactOutputResponse {
            text: String::new(),
            truncated: false,
            line_count: 0,
            stale: true,
        });
    };

    let requested_lines = end_global_line.saturating_sub(start_global_line) + 1;
    let max_lines = max_lines
        .unwrap_or(DEFAULT_OUTPUT_MAX_LINES)
        .clamp(1, HARD_OUTPUT_MAX_LINES);
    let count = (requested_lines as usize).min(max_lines);
    let lines = scroll_buffer.get_range(hot_start, count).await;
    let max_chars = max_chars
        .unwrap_or(DEFAULT_OUTPUT_MAX_CHARS)
        .clamp(1, HARD_OUTPUT_MAX_CHARS);
    let mut text = String::new();
    let mut truncated = requested_lines as usize > lines.len();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        let line_text = &line.text;
        if text.len().saturating_add(line_text.len()) > max_chars {
            let remaining = max_chars.saturating_sub(text.len());
            text.push_str(
                &line_text[..line_text
                    .char_indices()
                    .map(|(idx, _)| idx)
                    .take_while(|idx| *idx <= remaining)
                    .last()
                    .unwrap_or(0)],
            );
            truncated = true;
            break;
        }
        text.push_str(line_text);
    }

    Ok(CommandFactOutputResponse {
        text,
        truncated,
        line_count: lines.len(),
        stale: false,
    })
}
