const AI_TOOL_CONDENSE_KEEP_RECENT: usize = 5;
const AI_TOOL_CONDENSE_SUMMARY_MAX_CHARS: usize = 300;
const AI_TOOL_MODEL_OUTPUT_MAX_CHARS: usize = 12_000;
const AI_TOOL_MODEL_ERROR_OUTPUT_MAX_CHARS: usize = 2_000;
const AI_TOOL_MODEL_SUMMARY_MAX_CHARS: usize = 1_000;
const AI_TOOL_MODEL_ERROR_MESSAGE_MAX_CHARS: usize = 1_000;

fn condense_ai_tool_messages(history: &mut [AiChatMessage]) {
    let tool_indices = history
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.role == AiChatRole::Tool).then_some(index))
        .collect::<Vec<_>>();
    if tool_indices.len() <= AI_TOOL_CONDENSE_KEEP_RECENT {
        return;
    }

    for index in tool_indices
        .iter()
        .take(tool_indices.len().saturating_sub(AI_TOOL_CONDENSE_KEEP_RECENT))
        .copied()
    {
        let message = &mut history[index];
        if message.content.starts_with("[condensed]") {
            continue;
        }
        let parsed = serde_json::from_str::<serde_json::Value>(&message.content).ok();
        let is_error = parsed.as_ref().is_some_and(|value| {
            value
                .get("error")
                .is_some_and(|error| !error.is_null() && error != &serde_json::Value::Bool(false))
        });
        if is_error {
            continue;
        }
        let tool_name = parsed
            .as_ref()
            .and_then(|value| value.get("meta"))
            .and_then(|meta| meta.get("toolName"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                parsed
                    .as_ref()
                    .and_then(|value| value.get("metadata"))
                    .and_then(|meta| meta.get("toolName"))
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or("tool");
        let lines = message
            .content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        let mut summary = if lines.len() <= 4 {
            lines.join("\n")
        } else {
            format!(
                "{}\n... ({} lines omitted)\n{}",
                lines[..2].join("\n"),
                lines.len().saturating_sub(4),
                lines[lines.len().saturating_sub(2)..].join("\n")
            )
        };
        if summary.chars().count() > AI_TOOL_CONDENSE_SUMMARY_MAX_CHARS {
            summary = summary
                .chars()
                .take(AI_TOOL_CONDENSE_SUMMARY_MAX_CHARS)
                .collect::<String>();
            summary.push_str("...");
        }
        message.content = format!("[condensed] {tool_name} -> ok:\n{summary}");
    }
}

fn ai_to_usable_budget_threshold(
    raw_window_ratio: f32,
    context_window: usize,
    system_budget: usize,
    response_reserve: usize,
) -> f32 {
    let prompt_budget =
        compute_ai_prompt_budget(context_window, response_reserve, system_budget, None);
    if prompt_budget.usable_prompt_budget == 0 {
        raw_window_ratio
    } else {
        (context_window as f32 * raw_window_ratio) / prompt_budget.usable_prompt_budget as f32
    }
}

fn ai_tool_result_model_content(result: &AiExecutedToolResult) -> String {
    // Match Tauri's model-facing formatter: keep UI-only payload fields out of
    // the next LLM turn while preserving compact execution context.
    let envelope = ai_tool_result_envelope_or_legacy(result);
    let ok = envelope
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(result.success);
    let summary_source = envelope
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| result.output.as_str());
    let output_source = envelope
        .get("output")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if result.output.is_empty() {
                summary_source
            } else {
                result.output.as_str()
            }
        });
    let (summary, summary_truncated) =
        truncate_ai_tool_result_for_model(summary_source, AI_TOOL_MODEL_SUMMARY_MAX_CHARS);
    let (output, output_truncated) = truncate_ai_tool_result_for_model(
        output_source,
        if ok {
            AI_TOOL_MODEL_OUTPUT_MAX_CHARS
        } else {
            AI_TOOL_MODEL_ERROR_OUTPUT_MAX_CHARS
        },
    );
    let (error, error_truncated) = ai_tool_result_model_error(&envelope);
    let envelope_meta_truncated = envelope
        .pointer("/meta/truncated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let execution_truncated = envelope
        .pointer("/execution/truncated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let truncated = envelope_meta_truncated
        || execution_truncated
        || summary_truncated
        || output_truncated
        || error_truncated;

    let mut payload = serde_json::Map::new();
    payload.insert("ok".to_string(), serde_json::json!(ok));
    payload.insert("summary".to_string(), serde_json::json!(summary));
    payload.insert("output".to_string(), serde_json::json!(output));
    payload.insert("truncated".to_string(), serde_json::json!(truncated));

    if let Some(execution) = envelope.get("execution") {
        ai_insert_execution_shortcuts_for_model(&mut payload, execution);
        payload.insert("execution".to_string(), execution.clone());
    }
    if let Some(error) = error {
        payload.insert("error".to_string(), error);
    }
    for key in ["recoverable", "waitingForInput"] {
        if let Some(value) = envelope.get(key) {
            payload.insert(key.to_string(), value.clone());
        }
    }
    for key in [
        "warnings",
        "observations",
        "targets",
        "nextActions",
    ] {
        ai_insert_non_empty_model_array(&mut payload, key, envelope.get(key));
    }
    for key in ["disambiguation", "outputPreview"] {
        if let Some(value) = envelope.get(key) {
            payload.insert(key.to_string(), value.clone());
        }
    }

    let mut meta = envelope
        .get("meta")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    meta.insert("truncated".to_string(), serde_json::json!(truncated));
    payload.insert("meta".to_string(), serde_json::Value::Object(meta));

    serde_json::Value::Object(payload).to_string()
}

fn ai_tool_result_envelope_or_legacy(result: &AiExecutedToolResult) -> serde_json::Value {
    if result.envelope.is_object() {
        return result.envelope.clone();
    }
    serde_json::json!({
        "ok": result.success,
        "summary": if result.output.is_empty() { result.error.as_deref().unwrap_or_default() } else { result.output.as_str() },
        "output": result.output,
        "error": result.error.as_ref().map(|message| {
            serde_json::json!({
                "code": "legacy_tool_error",
                "message": message,
                "recoverable": true,
            })
        }),
        "meta": {
            "toolName": &result.tool_name,
            "durationMs": result.duration_ms,
            "truncated": false,
        },
    })
}

fn ai_tool_result_model_error(envelope: &serde_json::Value) -> (Option<serde_json::Value>, bool) {
    let Some(error) = envelope.get("error").and_then(serde_json::Value::as_object) else {
        return (None, false);
    };
    let mut error = error.clone();
    let message = error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let (message, truncated) =
        truncate_ai_tool_result_for_model(message, AI_TOOL_MODEL_ERROR_MESSAGE_MAX_CHARS);
    error.insert("message".to_string(), serde_json::json!(message));
    (Some(serde_json::Value::Object(error)), truncated)
}

fn ai_insert_execution_shortcuts_for_model(
    payload: &mut serde_json::Map<String, serde_json::Value>,
    execution: &serde_json::Value,
) {
    let Some(execution) = execution.as_object() else {
        return;
    };
    for key in ["target", "cwd", "stderrSummary"] {
        if let Some(value) = execution.get(key) {
            payload.insert(key.to_string(), value.clone());
        }
    }
    if execution.contains_key("exitCode") {
        payload.insert(
            "exitCode".to_string(),
            execution
                .get("exitCode")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        );
    }
    if let Some(value) = execution.get("timedOut") {
        payload.insert("timedOut".to_string(), value.clone());
    }
}

fn ai_insert_non_empty_model_array(
    payload: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&serde_json::Value>,
) {
    if value
        .and_then(serde_json::Value::as_array)
        .is_some_and(|items| !items.is_empty())
    {
        if let Some(value) = value {
            payload.insert(key.to_string(), value.clone());
        }
    }
}

fn truncate_ai_tool_result_for_model(value: &str, max_chars: usize) -> (String, bool) {
    if value.chars().count() <= max_chars {
        return (value.to_string(), false);
    }
    let head = value.chars().take(max_chars).collect::<String>();
    let omitted = value.chars().count().saturating_sub(max_chars);
    (
        format!("{head}\n\n[truncated: {omitted} chars omitted]"),
        true,
    )
}

fn rejected_ai_tool_result(
    tool_call_id: String,
    tool_name: String,
    code: impl Into<String>,
    message: impl Into<String>,
) -> AiExecutedToolResult {
    let code = code.into();
    let message = message.into();
    let envelope = serde_json::json!({
        "ok": false,
        "summary": message,
        "output": message,
        "error": {
            "code": code,
            "message": message,
            "recoverable": true,
        },
        "meta": {
            "toolName": tool_name,
            "durationMs": 0,
            "verified": false,
            "truncated": false,
        }
    });
    AiExecutedToolResult {
        tool_call_id,
        tool_name,
        success: false,
        output: message.clone(),
        error: Some(message),
        duration_ms: 0,
        envelope,
    }
}

fn unavailable_ai_tool_result(tool_call_id: String, tool_name: String) -> AiExecutedToolResult {
    pre_execution_rejected_ai_tool_result(
        tool_call_id,
        tool_name,
        "tool_not_available",
        "Tool not available in current context.",
    )
}

fn pre_execution_rejected_ai_tool_result(
    tool_call_id: String,
    tool_name: String,
    _code: impl Into<String>,
    message: impl Into<String>,
) -> AiExecutedToolResult {
    let message = message.into();
    let envelope = serde_json::json!({
        "ok": false,
        "summary": message,
        "output": "",
        "error": {
            "code": "legacy_tool_error",
            "message": message,
            "recoverable": true,
        },
        "meta": {
            "toolName": tool_name,
            "durationMs": 0,
            "truncated": false,
        }
    });
    AiExecutedToolResult {
        tool_call_id,
        tool_name,
        success: false,
        output: String::new(),
        error: Some(message),
        duration_ms: 0,
        envelope,
    }
}

fn executed_summary(result: &AiExecutedToolResult) -> String {
    result
        .envelope
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| {
            if result.success {
                "Tool completed."
            } else {
                "Tool failed."
            }
        })
        .to_string()
}

fn ai_policy_risk_label(risk: oxideterm_ai::AiActionRisk) -> &'static str {
    match risk {
        oxideterm_ai::AiActionRisk::Read => "read",
        oxideterm_ai::AiActionRisk::Write => "write",
        oxideterm_ai::AiActionRisk::Execute => "execute",
        oxideterm_ai::AiActionRisk::Interactive => "interactive",
        oxideterm_ai::AiActionRisk::Destructive => "destructive",
        oxideterm_ai::AiActionRisk::Credential => "credential",
    }
}

fn ai_policy_decision_label(decision: oxideterm_ai::AiPolicyDecisionKind) -> &'static str {
    match decision {
        oxideterm_ai::AiPolicyDecisionKind::Allow => "allow",
        oxideterm_ai::AiPolicyDecisionKind::RequireApproval => "require_approval",
        oxideterm_ai::AiPolicyDecisionKind::Deny => "deny",
    }
}

fn ai_policy_safety_mode_label(mode: oxideterm_ai::AiPolicySafetyMode) -> &'static str {
    match mode {
        oxideterm_ai::AiPolicySafetyMode::Default => "default",
        oxideterm_ai::AiPolicySafetyMode::Bypass => "bypass",
    }
}

fn annotate_executed_ai_tool_result_policy(
    result: &mut AiExecutedToolResult,
    decision: &oxideterm_ai::AiPolicyDecision,
) {
    let Some(envelope) = result.envelope.as_object_mut() else {
        return;
    };
    let meta = envelope
        .entry("meta")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut();
    let Some(meta) = meta else {
        return;
    };
    let approval_mode = ai_policy_safety_mode_label(decision.approval_mode);
    if decision.approval_mode == oxideterm_ai::AiPolicySafetyMode::Bypass
        && decision.risk == oxideterm_ai::AiActionRisk::Destructive
        && decision.decision == oxideterm_ai::AiPolicyDecisionKind::Allow
    {
        meta.insert(
            "approvalMode".to_string(),
            serde_json::json!(approval_mode),
        );
    }
    if let Some(profile_id) = decision.profile_id.as_deref() {
        meta.insert("profileId".to_string(), serde_json::json!(profile_id));
    }
    let mut policy_decision = serde_json::json!({
        "decision": ai_policy_decision_label(decision.decision),
        "risk": ai_policy_risk_label(decision.risk),
        "reasonCode": decision.reason_code.as_str(),
        "matchedPolicyKey": decision.matched_policy_key.as_str(),
        "approvalMode": approval_mode,
    });
    if let Some(profile_id) = decision.profile_id.as_deref()
        && let Some(object) = policy_decision.as_object_mut()
    {
        object.insert("profileId".to_string(), serde_json::json!(profile_id));
    }
    meta.insert("policyDecision".to_string(), policy_decision);
}

fn annotate_ai_run_command_execution_result(
    result: &mut AiExecutedToolResult,
    args: &serde_json::Value,
) {
    let Some(envelope) = result.envelope.as_object_mut() else {
        return;
    };
    let command = args
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let cwd = args
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let target = envelope
        .get("targets")
        .and_then(serde_json::Value::as_array)
        .and_then(|targets| targets.first())
        .cloned();
    let target_kind = target
        .as_ref()
        .and_then(|target| target.get("kind"))
        .and_then(serde_json::Value::as_str);
    let data = envelope.get("data");
    let exit_code = data.and_then(|value| value.get("exitCode")).cloned();
    let timed_out = data
        .and_then(|value| value.get("timedOut"))
        .and_then(serde_json::Value::as_bool);
    let truncated = envelope
        .get("meta")
        .and_then(|meta| meta.get("truncated"))
        .and_then(serde_json::Value::as_bool);
    let stderr_summary = envelope
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(serde_json::Value::as_str)
        .and_then(ai_execution_stderr_summary);

    let mut execution = serde_json::Map::new();
    execution.insert(
        "kind".to_string(),
        serde_json::json!(if target_kind == Some("terminal-session") {
            "terminal"
        } else {
            "command"
        }),
    );
    if let Some(command) = command {
        execution.insert("command".to_string(), serde_json::json!(command));
    }
    if let Some(cwd) = cwd {
        execution.insert("cwd".to_string(), serde_json::json!(cwd));
    }
    if let Some(target) = target {
        let mut execution_target = serde_json::Map::new();
        if let Some(id) = target.get("id") {
            execution_target.insert("id".to_string(), id.clone());
        }
        if let Some(kind) = target.get("kind") {
            execution_target.insert("kind".to_string(), kind.clone());
        }
        if let Some(label) = target.get("label") {
            execution_target.insert("label".to_string(), label.clone());
        }
        if !execution_target.is_empty() {
            execution.insert(
                "target".to_string(),
                serde_json::Value::Object(execution_target),
            );
        }
    }
    if let Some(exit_code) = exit_code {
        execution.insert("exitCode".to_string(), exit_code);
    }
    if let Some(timed_out) = timed_out {
        execution.insert("timedOut".to_string(), serde_json::json!(timed_out));
    }
    if let Some(truncated) = truncated {
        execution.insert("truncated".to_string(), serde_json::json!(truncated));
    }
    if let Some(stderr_summary) = stderr_summary {
        execution.insert(
            "stderrSummary".to_string(),
            serde_json::json!(stderr_summary),
        );
    }
    envelope.insert(
        "execution".to_string(),
        serde_json::Value::Object(execution),
    );
}

fn ai_execution_stderr_summary(message: &str) -> Option<String> {
    let summary = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    if summary.is_empty() {
        None
    } else {
        Some(truncate_ai_execution_stderr_summary(&summary, 600))
    }
}

fn truncate_ai_execution_stderr_summary(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let head = value.chars().take(max_chars).collect::<String>();
    format!("{head}...[truncated]")
}

fn ai_terminal_input_payload(args: &serde_json::Value) -> String {
    let mut payload = args
        .get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    if args
        .get("append_enter")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        payload.push('\r');
    }
    payload
}

fn ai_terminal_screen_snapshot_json(
    snapshot: &oxideterm_terminal::TerminalSnapshot,
    is_alternate_buffer: bool,
) -> serde_json::Value {
    // Keep the payload shape close to Tauri's readScreen result while avoiding
    // renderer-only fields that are not useful to an AI tool.
    serde_json::json!({
        "lines": snapshot
            .lines
            .iter()
            .map(|row| row.text().trim_end().to_string())
            .collect::<Vec<_>>(),
        "cursorX": snapshot.cursor_col + 1,
        "cursorY": snapshot.cursor_row + 1,
        "rows": snapshot.rows,
        "cols": snapshot.cols,
        "isAlternateBuffer": is_alternate_buffer,
        "scrollbackLines": snapshot.scrollback_lines,
        "displayOffset": snapshot.display_offset,
    })
}

fn ai_terminal_readiness_json(target: &AiOrchestratorTarget) -> serde_json::Value {
    let ready = target.state == "connected";
    // Tauri stores readiness in a registry and updates `updatedAt` on every patch.
    // Native snapshots are computed on demand, so use the snapshot time while
    // preserving the same numeric field shape for AI tool consumers.
    let updated_at_ms = ai_now_ms();
    serde_json::json!({
        "sessionId": target.refs.get("sessionId").cloned().unwrap_or_default(),
        "terminalType": target.metadata.get("terminalType").cloned().unwrap_or(serde_json::Value::Null),
        "writerReady": ready,
        "frontendOutputListenerReady": target.terminal_buffer.is_some(),
        "renderBufferReady": target.terminal_screen.is_some(),
        "backendBufferReady": target.terminal_buffer.is_some(),
        "updatedAt": updated_at_ms,
    })
}

fn terminal_delta_output(before: &str, after: &str) -> String {
    if after.starts_with(before) {
        let delta = after[before.len()..].trim();
        if !delta.is_empty() {
            return delta.to_string();
        }
    }
    after
        .chars()
        .rev()
        .take(1000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn looks_waiting_for_input(value: &str) -> bool {
    let tail = value
        .chars()
        .rev()
        .take(1000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>()
        .to_ascii_lowercase();
    let prompt_line = tail
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    ["password", "passphrase", "sudo", "验证码", "口令", "密码"]
        .iter()
        .any(|needle| prompt_line.contains(needle))
}

fn settings_with_json_patch(
    settings: &PersistedSettings,
    section: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<PersistedSettings, String> {
    let mut root = serde_json::to_value(settings).map_err(|error| error.to_string())?;
    let Some(section_value) = root.get_mut(section) else {
        return Err(format!("No settings section named {section}."));
    };
    let Some(section_object) = section_value.as_object_mut() else {
        return Err(format!("Settings section {section} cannot be updated."));
    };
    section_object.insert(key.to_string(), value);
    serde_json::from_value(root).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> AiExecutedToolResult {
        AiExecutedToolResult {
            tool_call_id: "tool-1".to_string(),
            tool_name: "run_command".to_string(),
            success: true,
            output: "ok".to_string(),
            error: None,
            duration_ms: 7,
            envelope: serde_json::json!({
                "ok": true,
                "summary": "ok",
                "output": "ok",
                "meta": {
                    "toolName": "run_command",
                    "durationMs": 7,
                },
            }),
        }
    }

    fn sample_target() -> AiOrchestratorTarget {
        let mut refs = std::collections::BTreeMap::new();
        refs.insert("nodeId".to_string(), "prod-node-1".to_string());
        AiOrchestratorTarget {
            id: "ssh-node:prod-node-1".to_string(),
            kind: "ssh-node".to_string(),
            label: "prod.example.com".to_string(),
            state: "connected".to_string(),
            capabilities: vec!["filesystem.read".to_string()],
            refs,
            metadata: serde_json::json!({
                "host": "prod.example.com",
                "username": "deploy",
            }),
            terminal_buffer: None,
            terminal_screen: None,
            ssh_handle: None,
        }
    }

    #[test]
    fn target_query_matches_refs_and_metadata_values() {
        let target = sample_target();

        assert!(target_matches_ai_query(&target, "prod-node-1"));
        assert!(target_matches_ai_query(&target, "deploy"));
        assert!(!target_matches_ai_query(&target, "staging"));
    }

    #[test]
    fn target_query_stringifies_metadata_like_javascript_join() {
        assert_eq!(
            ai_js_query_string(&serde_json::json!({ "path": "/tmp/project" })),
            "[object Object]"
        );
        assert_eq!(
            ai_js_query_string(&serde_json::json!(["one", null, 3])),
            "one,,3"
        );
    }

    #[test]
    fn tool_result_target_uses_tauri_metadata_shape() {
        let target = sample_target();

        let value = tool_result_target_json(&target);

        assert_eq!(value.get("id"), Some(&serde_json::json!("ssh-node:prod-node-1")));
        assert!(value.get("refs").is_none());
        assert_eq!(
            value.pointer("/metadata/refs/nodeId"),
            Some(&serde_json::json!("prod-node-1"))
        );
        assert_eq!(
            value.pointer("/metadata/state"),
            Some(&serde_json::json!("connected"))
        );
        assert_eq!(
            value.pointer("/metadata/username"),
            Some(&serde_json::json!("deploy"))
        );
    }

    #[test]
    fn next_action_maps_to_tauri_tool_result_shape() {
        let action = serde_json::json!({
            "action": "list_targets",
            "args": { "view": "connections" },
            "reason": "Refresh targets."
        });

        let mapped = ai_next_action_json(&action).expect("next action should map");

        assert_eq!(mapped.get("tool"), Some(&serde_json::json!("list_targets")));
        assert_eq!(
            mapped.pointer("/args/view"),
            Some(&serde_json::json!("connections"))
        );
        assert_eq!(mapped.get("priority"), Some(&serde_json::json!("recommended")));
    }

    #[test]
    fn long_tool_output_uses_head_tail_preview_metadata() {
        let output = "a".repeat(30_000);

        let (preview, raw_output, output_preview, truncated) = prepare_ai_tool_output(&output);

        assert!(truncated);
        assert!(raw_output.is_some());
        assert!(preview.contains("showing head and tail"));
        assert_eq!(
            output_preview.get("strategy"),
            Some(&serde_json::json!("head_tail"))
        );
        assert_eq!(
            output_preview.get("rawOutputStored"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn run_command_execution_summary_matches_tauri_shape() {
        let mut result = sample_result();
        result.envelope = serde_json::json!({
            "ok": false,
            "summary": "Local command exited with 2.",
            "output": "failed",
            "data": { "exitCode": 2, "timedOut": false },
            "error": {
                "code": "local_command_failed",
                "message": "Exit code: 2",
                "recoverable": true
            },
            "targets": [{
                "id": "local-shell:default",
                "kind": "local-shell",
                "label": "Local shell",
                "metadata": { "state": "available", "refs": {} }
            }],
            "meta": { "toolName": "run_command", "durationMs": 7, "truncated": false }
        });

        annotate_ai_run_command_execution_result(
            &mut result,
            &serde_json::json!({
                "command": "cargo check",
                "cwd": "/tmp/project"
            }),
        );

        assert_eq!(
            result.envelope.pointer("/execution/kind"),
            Some(&serde_json::json!("command"))
        );
        assert_eq!(
            result.envelope.pointer("/execution/command"),
            Some(&serde_json::json!("cargo check"))
        );
        assert_eq!(
            result.envelope.pointer("/execution/cwd"),
            Some(&serde_json::json!("/tmp/project"))
        );
        assert_eq!(
            result.envelope.pointer("/execution/target/id"),
            Some(&serde_json::json!("local-shell:default"))
        );
        assert_eq!(
            result.envelope.pointer("/execution/exitCode"),
            Some(&serde_json::json!(2))
        );
        assert_eq!(
            result.envelope.pointer("/execution/timedOut"),
            Some(&serde_json::json!(false))
        );
        assert_eq!(
            result.envelope.pointer("/execution/stderrSummary"),
            Some(&serde_json::json!("Exit code: 2"))
        );
    }

    #[test]
    fn tool_result_model_content_omits_ui_only_payload_like_tauri() {
        let mut result = sample_result();
        result.envelope = serde_json::json!({
            "ok": true,
            "summary": "Listed targets.",
            "output": "2 targets",
            "data": [{ "id": "ssh-node:prod" }],
            "rawOutput": "full raw output",
            "targets": [{
                "id": "ssh-node:prod",
                "kind": "ssh-node",
                "label": "prod",
                "metadata": { "state": "connected" }
            }],
            "outputPreview": {
                "strategy": "full",
                "rawOutputStored": true
            },
            "meta": {
                "toolName": "list_targets",
                "durationMs": 9,
                "truncated": false
            }
        });

        let content = ai_tool_result_model_content(&result);
        let value = serde_json::from_str::<serde_json::Value>(&content).unwrap();

        assert!(value.get("data").is_none());
        assert!(value.get("rawOutput").is_none());
        assert_eq!(value.pointer("/targets/0/id"), Some(&serde_json::json!("ssh-node:prod")));
        assert_eq!(
            value.pointer("/outputPreview/rawOutputStored"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            value.pointer("/meta/truncated"),
            Some(&serde_json::json!(false))
        );
    }

    #[test]
    fn failed_tool_result_model_content_uses_tauri_error_output_limit() {
        let mut result = sample_result();
        result.success = false;
        result.output = "x".repeat(2_100);
        result.envelope = serde_json::json!({
            "ok": false,
            "summary": "Command failed.",
            "output": &result.output,
            "error": {
                "code": "local_command_failed",
                "message": "m".repeat(1_100),
                "recoverable": true
            },
            "execution": {
                "kind": "command",
                "exitCode": null,
                "timedOut": false,
                "truncated": false
            },
            "meta": {
                "toolName": "run_command",
                "durationMs": 7,
                "truncated": false
            }
        });

        let content = ai_tool_result_model_content(&result);
        let value = serde_json::from_str::<serde_json::Value>(&content).unwrap();

        assert_eq!(value.get("truncated"), Some(&serde_json::json!(true)));
        assert_eq!(value.get("exitCode"), Some(&serde_json::Value::Null));
        assert_eq!(value.get("timedOut"), Some(&serde_json::json!(false)));
        assert!(
            value
                .get("output")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|output| output.ends_with("[truncated: 100 chars omitted]"))
        );
        assert!(
            value
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|message| message.ends_with("[truncated: 100 chars omitted]"))
        );
    }

    #[test]
    fn ssh_command_cwd_wraps_like_tauri_remote_exec_cwd() {
        assert_eq!(
            ai_command_with_cwd("pwd", Some("/var/www/app")),
            "cd '/var/www/app' && pwd"
        );
        assert_eq!(
            ai_command_with_cwd("pwd", Some("/srv/it's ok")),
            "cd '/srv/it'\\''s ok' && pwd"
        );
        assert_eq!(ai_command_with_cwd("pwd", Some("~")), "cd ~ && pwd");
        assert_eq!(
            ai_command_with_cwd("pwd", Some("~/project dir")),
            "cd ~/'project dir' && pwd"
        );
        assert_eq!(ai_command_with_cwd("pwd", None), "pwd");
    }

    #[test]
    fn local_exec_timeout_caps_like_tauri_backend() {
        assert_eq!(ai_local_exec_timeout_secs(1), 1);
        assert_eq!(ai_local_exec_timeout_secs(60), 60);
        assert_eq!(ai_local_exec_timeout_secs(90), 60);
    }

    #[test]
    fn recall_preferences_memory_data_preserves_tauri_enabled_flag() {
        let memory = ai_memory_settings_json(false, "  - use compact output\n");

        assert_eq!(memory.get("enabled"), Some(&serde_json::json!(false)));
        assert_eq!(ai_memory_content(&memory), "  - use compact output\n");
        assert_eq!(ai_memory_trimmed_content(&memory), "- use compact output");
    }

    #[test]
    fn tool_verified_default_requires_success_without_error_like_tauri() {
        assert!(ai_tool_verified_default(true, None));
        assert!(!ai_tool_verified_default(false, None));
        assert!(!ai_tool_verified_default(true, Some("error")));
    }

    #[test]
    fn terminal_run_command_preflight_keeps_execute_risk_like_tauri() {
        assert_eq!(ai_run_command_preflight_risk(), "execute");
    }

    #[test]
    fn ssh_reconnect_failure_next_action_matches_tauri() {
        let actions = ai_ssh_reconnect_failed_next_actions();

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].get("action"),
            Some(&serde_json::json!("list_targets"))
        );
        assert_eq!(
            actions[0].get("reason"),
            Some(&serde_json::json!("Refresh target state before retrying."))
        );
    }

    #[test]
    fn live_state_guard_only_applies_to_runtime_targets() {
        let mut target = sample_target();
        assert!(target_requires_live_state(&target));

        target.kind = "settings".to_string();
        assert!(!target_requires_live_state(&target));

        target.kind = "local-shell".to_string();
        assert!(!target_requires_live_state(&target));
    }

    #[test]
    fn select_target_kind_filter_matches_tauri_validation() {
        assert_eq!(
            normalized_ai_select_target_kind(Some("ssh-node")),
            Some("ssh-node")
        );
        assert_eq!(
            normalized_ai_select_target_kind(Some("all")),
            Some("all")
        );
        assert_eq!(normalized_ai_select_target_kind(Some("bogus")), None);
        assert_eq!(normalized_ai_select_target_kind(None), None);
    }

    #[test]
    fn command_like_target_query_matches_tauri_case_sensitive_guard() {
        assert!(is_ai_command_like_query("ls -la"));
        assert!(is_ai_command_like_query("sudo systemctl status ssh"));
        assert!(!is_ai_command_like_query("LS"));
        assert!(!is_ai_command_like_query("-la"));
    }

    #[test]
    fn list_targets_invalid_view_defaults_to_connections_like_tauri() {
        assert_eq!(normalized_ai_target_view(Some("bogus")), "connections");
        assert_eq!(normalized_ai_target_view(None), "connections");
        assert_eq!(normalized_ai_target_view(Some("all")), "all");
    }

    #[test]
    fn target_query_trims_before_matching_like_tauri() {
        assert_eq!(normalized_ai_query(Some("  PROD  ")), "prod");
        assert_eq!(normalized_ai_query(Some("\n")), "");
        assert_eq!(normalized_ai_query(None), "");

        let target = sample_target();
        assert!(target_matches_ai_query(&target, &normalized_ai_query(Some("  prod-node-1  "))));
    }

    #[test]
    fn opened_local_terminal_target_uses_tauri_synthetic_shape() {
        let mut target = sample_target();
        target.id = "terminal-session:abc123".to_string();
        target.kind = "terminal-session".to_string();
        target.label = "Local terminal zsh".to_string();
        target.refs.insert("sessionId".to_string(), "abc123".to_string());
        target.refs.insert("tabId".to_string(), "tab-1".to_string());
        target.metadata = serde_json::json!({
            "terminalType": "local_terminal",
            "shell": { "label": "zsh" }
        });

        let opened = ai_opened_local_terminal_target(&target);
        let value = target_json(&opened);

        assert_eq!(value.pointer("/refs/sessionId"), Some(&serde_json::json!("abc123")));
        assert!(value.pointer("/refs/tabId").is_none());
        assert_eq!(
            value.pointer("/metadata/terminalType"),
            Some(&serde_json::json!("local_terminal"))
        );
        assert!(value.pointer("/metadata/shell").is_none());
    }

    #[test]
    fn resource_kind_validation_matches_tauri_executor_arg() {
        assert_eq!(normalized_ai_resource_kind(Some("file")), "file");
        assert_eq!(normalized_ai_resource_kind(Some("bogus")), "");
        assert_eq!(normalized_ai_resource_kind(None), "");
    }

    #[test]
    fn rag_query_arg_preserves_tauri_nullish_without_trim() {
        assert_eq!(
            ai_rag_query_arg(&serde_json::json!({ "query": "  keep spaces  ", "path": "fallback" })),
            "  keep spaces  "
        );
        assert_eq!(
            ai_rag_query_arg(&serde_json::json!({ "path": "  fallback path  " })),
            "  fallback path  "
        );
        assert_eq!(ai_rag_query_arg(&serde_json::json!({})), "");
    }

    #[test]
    fn transfer_directory_detection_accepts_tauri_separators() {
        assert!(ai_transfer_path_looks_directory("/tmp/project/"));
        assert!(ai_transfer_path_looks_directory(r"C:\Users\me\Downloads\"));
        assert!(!ai_transfer_path_looks_directory("/tmp/project/file.txt"));
    }

    #[test]
    fn active_context_matches_tab_session_or_node_refs() {
        let mut target = sample_target();
        target.refs.insert("tabId".to_string(), "7".to_string());
        target.refs.insert("sessionId".to_string(), "42".to_string());

        assert!(target_matches_active_context(
            &target,
            Some("7"),
            None,
            None
        ));
        assert!(target_matches_active_context(
            &target,
            None,
            Some("prod-node-1"),
            None
        ));
        assert!(target_matches_active_context(
            &target,
            None,
            None,
            Some("42")
        ));
        assert!(!target_matches_active_context(
            &target,
            Some("8"),
            Some("staging-node"),
            Some("43")
        ));
    }

    #[test]
    fn connection_state_error_count_uses_tauri_metadata_status() {
        let mut stale = sample_target();
        stale.state = "stale".to_string();
        stale.metadata = serde_json::json!({ "status": "link-down" });
        let mut error = sample_target();
        error.id = "ssh-node:error-node".to_string();
        error.state = "stale".to_string();
        error.metadata = serde_json::json!({ "status": "error" });

        let state = ai_connections_state(&[stale, error], "epoch-1");

        assert_eq!(state.pointer("/counts/linkDown"), Some(&serde_json::json!(2)));
        assert_eq!(state.pointer("/counts/error"), Some(&serde_json::json!(1)));
    }

    #[test]
    fn terminal_readiness_payload_uses_tauri_field_names() {
        let mut target = sample_target();
        target.kind = "terminal-session".to_string();
        target.refs.insert("sessionId".to_string(), "42".to_string());
        target.metadata = serde_json::json!({ "terminalType": "local_terminal" });
        target.terminal_buffer = Some("ready".to_string());
        target.terminal_screen = Some(serde_json::json!({ "lines": ["ready"] }));

        let readiness = ai_terminal_readiness_json(&target);

        assert_eq!(readiness.get("sessionId"), Some(&serde_json::json!("42")));
        assert_eq!(
            readiness.get("terminalType"),
            Some(&serde_json::json!("local_terminal"))
        );
        assert_eq!(readiness.get("writerReady"), Some(&serde_json::json!(true)));
        assert!(readiness.get("renderBufferReady").is_some());
        assert!(readiness
            .get("updatedAt")
            .and_then(serde_json::Value::as_i64)
            .is_some_and(|updated_at| updated_at > 0));
    }

    #[test]
    fn terminal_screen_payload_uses_tauri_cursor_and_buffer_shape() {
        let snapshot = oxideterm_terminal::TerminalSnapshot {
            cols: 80,
            rows: 24,
            cursor_col: 2,
            cursor_row: 3,
            cursor_shape: oxideterm_terminal::TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 10,
            lines: Vec::new(),
            images: Vec::new(),
        };

        let screen = ai_terminal_screen_snapshot_json(&snapshot, true);

        assert_eq!(screen.get("cursorX"), Some(&serde_json::json!(3)));
        assert_eq!(screen.get("cursorY"), Some(&serde_json::json!(4)));
        assert_eq!(screen.get("isAlternateBuffer"), Some(&serde_json::json!(true)));
    }

    #[test]
    fn waiting_for_input_uses_tail_prompt_line_like_tauri() {
        assert!(looks_waiting_for_input("ready\npassword: "));
        assert!(!looks_waiting_for_input("password accepted\nready$ "));
    }

    #[test]
    fn terminal_delta_fallback_matches_tauri_last_1000_chars() {
        let value = "a".repeat(1200);
        let output = terminal_delta_output("different", &value);

        assert_eq!(output.len(), 1000);
        assert!(!output.starts_with("[trimmed"));
    }

    #[test]
    fn terminal_target_short_ids_match_tauri_labels() {
        assert_eq!(ai_short_id("1234567890"), "12345678");
        assert_eq!(ai_short_id("42"), "42");
    }

    #[test]
    fn local_exec_output_pretruncates_like_tauri_backend() {
        let value = "a".repeat((64 * 1024) + 1);
        let truncated = truncate_ai_local_exec_output(&value);

        assert_eq!(truncated.len(), (64 * 1024) + "...(truncated)".len());
        assert!(truncated.ends_with("...(truncated)"));
    }

    #[test]
    fn resource_output_truncation_matches_tauri_suffix() {
        let truncated = truncate_for_model("abcdef".to_string(), 3);

        assert_eq!(truncated, "abc\n[truncated 3 chars]");
    }

    #[test]
    fn execution_stderr_summary_keeps_tauri_suffix() {
        let value = "a".repeat(601);
        let truncated = truncate_ai_execution_stderr_summary(&value, 600);

        assert!(truncated.ends_with("...[truncated]"));
    }

    #[test]
    fn dry_run_result_can_mark_success_as_unverified_like_tauri() {
        let result = AiActionResultLite {
            ok: true,
            summary: "Dry-run file write /tmp/a.".to_string(),
            output: "Dry-run only; file was not changed.".to_string(),
            data: serde_json::Value::Null,
            error_code: None,
            error_message: None,
            risk: "write",
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        }
        .with_verified(false);

        assert_eq!(result.verified, Some(false));
        assert!(result.data.is_null());
    }

    #[test]
    fn command_result_preserves_tauri_observations() {
        let result = AiActionResultLite {
            ok: true,
            summary: "Remote command output captured; exit code was not reported.".to_string(),
            output: "hello\n[exit_code: unknown]".to_string(),
            data: serde_json::json!({ "exitCode": serde_json::Value::Null }),
            error_code: None,
            error_message: None,
            risk: "execute",
            target: None,
            targets: Vec::new(),
            next_actions: Vec::new(),
            observations: Vec::new(),
            verified: None,
            state_version: None,
        }
        .with_observations(vec![
            "The remote command produced output, but the backend did not report an exit code."
                .to_string(),
        ]);

        assert_eq!(result.observations.len(), 1);
        assert!(result.observations[0].contains("did not report an exit code"));
    }

    #[test]
    fn bypass_destructive_policy_annotation_matches_tauri_envelope_shape() {
        let mut result = sample_result();
        let decision = oxideterm_ai::AiPolicyDecision {
            decision: oxideterm_ai::AiPolicyDecisionKind::Allow,
            risk: oxideterm_ai::AiActionRisk::Destructive,
            reason_code: "bypass_destructive_allowed".to_string(),
            reason_text_key: "ai.tool_use.policy_reason_bypass".to_string(),
            matched_policy_key: "run_command:dangerous".to_string(),
            approval_mode: oxideterm_ai::AiPolicySafetyMode::Bypass,
            profile_id: Some("profile-1".to_string()),
        };

        annotate_executed_ai_tool_result_policy(&mut result, &decision);

        assert_eq!(
            result.envelope.pointer("/meta/approvalMode"),
            Some(&serde_json::json!("bypass"))
        );
        assert_eq!(
            result.envelope.pointer("/meta/profileId"),
            Some(&serde_json::json!("profile-1"))
        );
        assert_eq!(
            result.envelope.pointer("/meta/policyDecision"),
            Some(&serde_json::json!({
                "decision": "allow",
                "risk": "destructive",
                "reasonCode": "bypass_destructive_allowed",
                "matchedPolicyKey": "run_command:dangerous",
                "approvalMode": "bypass",
                "profileId": "profile-1",
            }))
        );
    }

    #[test]
    fn default_policy_annotation_does_not_mark_bypass() {
        let mut result = sample_result();
        let decision = oxideterm_ai::AiPolicyDecision {
            decision: oxideterm_ai::AiPolicyDecisionKind::Allow,
            risk: oxideterm_ai::AiActionRisk::Read,
            reason_code: "read_only_auto_allowed".to_string(),
            reason_text_key: "ai.tool_use.policy_reason_read_only".to_string(),
            matched_policy_key: "list_targets".to_string(),
            approval_mode: oxideterm_ai::AiPolicySafetyMode::Default,
            profile_id: None,
        };

        annotate_executed_ai_tool_result_policy(&mut result, &decision);

        assert!(result.envelope.pointer("/meta/approvalMode").is_none());
        assert_eq!(
            result.envelope.pointer("/meta/policyDecision/approvalMode"),
            Some(&serde_json::json!("default"))
        );
        assert!(result
            .envelope
            .pointer("/meta/policyDecision/profileId")
            .is_none());
    }

    #[test]
    fn unavailable_tool_result_matches_tauri_pre_policy_rejection() {
        let result = unavailable_ai_tool_result(
            "call-1".to_string(),
            "mcp::external::tool".to_string(),
        );

        assert!(!result.success);
        assert_eq!(result.output, "");
        assert_eq!(
            result.error.as_deref(),
            Some("Tool not available in current context.")
        );
        assert_eq!(
            result.envelope.pointer("/error/code"),
            Some(&serde_json::json!("legacy_tool_error"))
        );
    }

    #[test]
    fn pre_execution_rejected_tool_result_keeps_model_output_empty_like_tauri() {
        let result = pre_execution_rejected_ai_tool_result(
            "call-1".to_string(),
            "run_command".to_string(),
            "user_rejected",
            "Tool call rejected by user.",
        );

        assert!(!result.success);
        assert_eq!(result.output, "");
        assert_eq!(
            result.error.as_deref(),
            Some("Tool call rejected by user.")
        );
        assert_eq!(result.envelope.get("output"), Some(&serde_json::json!("")));
        assert!(result.envelope.pointer("/meta/verified").is_none());
    }
}
