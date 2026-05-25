// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Terminal read-only host API response helpers.

use std::collections::HashMap;

use oxideterm_plugin_protocol as plugin_runtime;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginTerminalNodeSnapshot {
    pub buffer: String,
    pub selection: Option<String>,
    pub current_lines: usize,
}

pub fn native_plugin_terminal_search_response(
    request_id: String,
    terminal_nodes: &HashMap<String, NativePluginTerminalNodeSnapshot>,
    args: Value,
) -> plugin_runtime::PluginResponse {
    let Some(node_id) = args.get("nodeId").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_node_id",
                "Native plugin terminal.search requires args.nodeId",
            ),
        );
    };
    let Some(query) = args.get("query").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_terminal_search_query",
                "Native plugin terminal.search requires args.query",
            ),
        );
    };
    let options = args.get("options").unwrap_or(&Value::Null);
    let search_options = native_plugin_terminal_search_options(query, options);
    let Some(terminal) = terminal_nodes.get(node_id) else {
        return plugin_runtime::PluginResponse::ok(
            request_id,
            json!({ "matches": [], "total_matches": 0 }),
        );
    };
    let search = native_plugin_terminal_search_matches(&terminal.buffer, &search_options);
    plugin_runtime::PluginResponse::ok(
        request_id,
        json!({
            "matches": search.matches,
            "total_matches": search.total_matches,
            "truncated": search.truncated,
            "error": search.error,
        }),
    )
}

pub fn native_plugin_terminal_scroll_buffer_response(
    request_id: String,
    terminal_nodes: &HashMap<String, NativePluginTerminalNodeSnapshot>,
    args: Value,
) -> plugin_runtime::PluginResponse {
    let Some(node_id) = args.get("nodeId").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_node_id",
                "Native plugin terminal.getScrollBuffer requires args.nodeId",
            ),
        );
    };
    let start_line = args
        .get("startLine")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let count = args
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .min(1000) as usize;
    let Some(terminal) = terminal_nodes.get(node_id) else {
        return plugin_runtime::PluginResponse::ok(request_id, json!([]));
    };
    let lines = terminal
        .buffer
        .lines()
        .enumerate()
        .skip(start_line)
        .take(count)
        .map(|(line_number, text)| json!({ "text": text, "lineNumber": line_number }))
        .collect::<Vec<_>>();
    plugin_runtime::PluginResponse::ok(request_id, json!(lines))
}

pub fn native_plugin_terminal_buffer_size_response(
    request_id: String,
    terminal_nodes: &HashMap<String, NativePluginTerminalNodeSnapshot>,
    args: Value,
) -> plugin_runtime::PluginResponse {
    let Some(node_id) = args.get("nodeId").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_node_id",
                "Native plugin terminal.getBufferSize requires args.nodeId",
            ),
        );
    };
    let current_lines = terminal_nodes
        .get(node_id)
        .map(|terminal| terminal.current_lines)
        .unwrap_or_default();
    plugin_runtime::PluginResponse::ok(
        request_id,
        json!({
            "currentLines": current_lines,
            "totalLines": current_lines,
            "maxLines": current_lines,
        }),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativePluginTerminalSearchOptions {
    query: String,
    case_sensitive: bool,
    regex: bool,
    whole_word: bool,
    max_matches: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativePluginTerminalSearchResult {
    matches: Vec<Value>,
    total_matches: usize,
    truncated: bool,
    error: Option<String>,
}

fn native_plugin_terminal_search_options(
    query: &str,
    options: &Value,
) -> NativePluginTerminalSearchOptions {
    NativePluginTerminalSearchOptions {
        query: query.to_string(),
        case_sensitive: options
            .get("caseSensitive")
            .or_else(|| options.get("case_sensitive"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        regex: options
            .get("regex")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        whole_word: options
            .get("wholeWord")
            .or_else(|| options.get("whole_word"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // Tauri's Rust SearchOptions defaults max_matches to 100 when the
        // plugin does not specify one through the JS context factory.
        max_matches: options
            .get("maxMatches")
            .or_else(|| options.get("max_matches"))
            .and_then(Value::as_u64)
            .unwrap_or(100) as usize,
    }
}

fn native_plugin_terminal_search_matches(
    buffer: &str,
    options: &NativePluginTerminalSearchOptions,
) -> NativePluginTerminalSearchResult {
    if options.query.is_empty() {
        return NativePluginTerminalSearchResult {
            matches: Vec::new(),
            total_matches: 0,
            truncated: false,
            error: None,
        };
    }

    let pattern = if options.regex {
        options.query.clone()
    } else if options.whole_word {
        format!(r"\b{}\b", regex::escape(&options.query))
    } else {
        regex::escape(&options.query)
    };

    let regex = match regex::RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
    {
        Ok(regex) => regex,
        Err(error) => {
            return NativePluginTerminalSearchResult {
                matches: Vec::new(),
                total_matches: 0,
                truncated: false,
                error: Some(format!("Invalid regex: {error}")),
            };
        }
    };

    let limit = if options.max_matches == 0 {
        usize::MAX
    } else {
        options.max_matches
    };
    let mut matches = Vec::new();
    let mut total_matches = 0usize;
    for (line_number, line) in buffer.lines().enumerate() {
        for matched in regex.find_iter(line) {
            total_matches += 1;
            if matches.len() < limit {
                // Tauri returns backend `HistorySearchMatch`/`SearchMatch`
                // payloads with snake_case fields; pluginContextFactory passes
                // them through as unknown values without camel-case mapping.
                matches.push(json!({
                    "line_number": line_number,
                    "column_start": matched.start(),
                    "column_end": matched.end(),
                    "matched_text": matched.as_str(),
                    "line_content": line,
                }));
            }
        }
    }

    NativePluginTerminalSearchResult {
        truncated: total_matches > matches.len(),
        matches,
        total_matches,
        error: None,
    }
}
