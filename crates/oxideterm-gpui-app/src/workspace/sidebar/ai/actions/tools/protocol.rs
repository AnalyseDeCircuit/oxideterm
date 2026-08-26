pub(in crate::workspace) fn rejected_ai_tool_result(
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

pub(in crate::workspace) fn unavailable_ai_tool_result(
    tool_call_id: String,
    tool_name: String,
) -> AiExecutedToolResult {
    pre_execution_rejected_ai_tool_result(
        tool_call_id,
        tool_name,
        "tool_not_available",
        "Tool not available in current context.",
    )
}

pub(in crate::workspace) fn pre_execution_rejected_ai_tool_result(
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
        "output": "",
        "error": {
            "code": code,
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

pub(in crate::workspace) fn executed_summary(result: &AiExecutedToolResult) -> String {
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

pub(in crate::workspace) fn ai_policy_risk_label(risk: oxideterm_ai::AiActionRisk) -> &'static str {
    match risk {
        oxideterm_ai::AiActionRisk::Read => "read",
        oxideterm_ai::AiActionRisk::Write => "write",
        oxideterm_ai::AiActionRisk::Execute => "execute",
        oxideterm_ai::AiActionRisk::Interactive => "interactive",
        oxideterm_ai::AiActionRisk::Destructive => "destructive",
        oxideterm_ai::AiActionRisk::Credential => "credential",
    }
}

pub(in crate::workspace) fn ai_policy_decision_label(
    decision: oxideterm_ai::AiPolicyDecisionKind,
) -> &'static str {
    match decision {
        oxideterm_ai::AiPolicyDecisionKind::Allow => "allow",
        oxideterm_ai::AiPolicyDecisionKind::RequireApproval => "require_approval",
        oxideterm_ai::AiPolicyDecisionKind::Deny => "deny",
    }
}

pub(in crate::workspace) fn ai_policy_safety_mode_label(
    mode: oxideterm_ai::AiPolicySafetyMode,
) -> &'static str {
    match mode {
        oxideterm_ai::AiPolicySafetyMode::Default => "default",
        oxideterm_ai::AiPolicySafetyMode::ReadOnly => "read_only",
        oxideterm_ai::AiPolicySafetyMode::Bypass => "bypass",
    }
}

pub(in crate::workspace) fn annotate_executed_ai_tool_result_policy(
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
        meta.insert("approvalMode".to_string(), serde_json::json!(approval_mode));
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

pub(in crate::workspace) fn annotate_ai_run_command_execution_result(
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
    let execution_state = data
        .and_then(|value| value.get("executionState"))
        .and_then(serde_json::Value::as_str);
    let visible_in_terminal = data
        .and_then(|value| value.get("visibleInTerminal"))
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
    if let Some(execution_state) = execution_state {
        execution.insert("state".to_string(), serde_json::json!(execution_state));
    }
    if let Some(visible_in_terminal) = visible_in_terminal {
        execution.insert(
            "visibleInTerminal".to_string(),
            serde_json::json!(visible_in_terminal),
        );
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

pub(in crate::workspace) fn ai_execution_stderr_summary(message: &str) -> Option<String> {
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

pub(in crate::workspace) fn truncate_ai_execution_stderr_summary(
    value: &str,
    max_chars: usize,
) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let head = value.chars().take(max_chars).collect::<String>();
    format!("{head}...[truncated]")
}

pub(in crate::workspace) fn ai_terminal_input_payload(args: &serde_json::Value) -> Vec<u8> {
    if let Some(key) = args.get("key").and_then(serde_json::Value::as_str) {
        return ai_terminal_key_bytes(key).map_or_else(Vec::new, <[u8]>::to_vec);
    }
    let mut payload = args
        .get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .as_bytes()
        .to_vec();
    if args
        .get("append_enter")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        payload.push(b'\r');
    }
    payload
}

fn ai_terminal_key_bytes(key: &str) -> Option<&'static [u8]> {
    // These are terminal protocol bytes, not platform key events, so the same
    // payload remains valid for local, SSH, Telnet, and serial sessions.
    match key {
        "ctrl_c" => Some(b"\x03"),
        "ctrl_d" => Some(b"\x04"),
        "ctrl_z" => Some(b"\x1a"),
        "escape" => Some(b"\x1b"),
        "enter" => Some(b"\r"),
        "tab" => Some(b"\t"),
        "backspace" => Some(b"\x7f"),
        "up" => Some(b"\x1b[A"),
        "down" => Some(b"\x1b[B"),
        "right" => Some(b"\x1b[C"),
        "left" => Some(b"\x1b[D"),
        "home" => Some(b"\x1b[H"),
        "end" => Some(b"\x1b[F"),
        "page_up" => Some(b"\x1b[5~"),
        "page_down" => Some(b"\x1b[6~"),
        "delete" => Some(b"\x1b[3~"),
        _ => None,
    }
}

pub(in crate::workspace) fn ai_terminal_tui_state(
    screen: Option<&serde_json::Value>,
    buffer: &str,
) -> &'static str {
    if screen
        .and_then(|screen| screen.get("isAlternateBuffer"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return "alternate_screen";
    }
    if looks_waiting_for_input(buffer) {
        return "prompt";
    }
    "shell"
}

pub(in crate::workspace) fn ai_terminal_command_record_json(
    record: &oxideterm_gpui_terminal::TerminalAiCommandRecord,
) -> serde_json::Value {
    let status = match record.status {
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Open => "running",
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed => "completed",
        oxideterm_gpui_terminal::TerminalCommandFactStatus::Stale => "stale",
    };
    serde_json::json!({
        "commandId": record.command_id,
        "command": oxideterm_ai::sanitize_for_ai(&record.command),
        "status": status,
        "startedAt": record.started_at,
        "finishedAt": record.finished_at,
        "exitCode": record.exit_code,
    })
}

pub(in crate::workspace) fn ai_terminal_wait_match(
    args: &serde_json::Value,
    initial_buffer: &str,
    current_buffer: &str,
    initial_alternate_screen: bool,
    current_alternate_screen: bool,
    command_records: &[oxideterm_gpui_terminal::TerminalAiCommandRecord],
) -> Option<serde_json::Value> {
    let condition = args
        .get("condition")
        .and_then(serde_json::Value::as_str)?;
    match condition {
        "changed" if current_buffer != initial_buffer => {
            Some(serde_json::json!({ "condition": condition }))
        }
        "contains" => {
            let expected = args.get("text").and_then(serde_json::Value::as_str)?;
            let case_sensitive = args
                .get("case_sensitive")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let matched = if case_sensitive {
                current_buffer.contains(expected)
            } else {
                current_buffer
                    .to_lowercase()
                    .contains(&expected.to_lowercase())
            };
            matched.then(|| serde_json::json!({ "condition": condition }))
        }
        "prompt" if looks_waiting_for_input(current_buffer) => {
            Some(serde_json::json!({ "condition": condition }))
        }
        "tui_entered" if !initial_alternate_screen && current_alternate_screen => {
            Some(serde_json::json!({ "condition": condition }))
        }
        "tui_exited" if initial_alternate_screen && !current_alternate_screen => {
            Some(serde_json::json!({ "condition": condition }))
        }
        "command_completed" => {
            let command_id = args
                .get("command_id")
                .and_then(serde_json::Value::as_str)?;
            command_records
                .iter()
                .find(|record| {
                    record.command_id == command_id
                        && record.status
                            != oxideterm_gpui_terminal::TerminalCommandFactStatus::Open
                })
                .map(|record| {
                    serde_json::json!({
                        "condition": condition,
                        "command": ai_terminal_command_record_json(record),
                    })
                })
        }
        _ => None,
    }
}

pub(in crate::workspace) fn ai_terminal_screen_snapshot_json(
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


pub(in crate::workspace) fn terminal_delta_output(before: &str, after: &str) -> String {
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

pub(in crate::workspace) fn looks_waiting_for_input(value: &str) -> bool {
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

pub(in crate::workspace) fn settings_with_json_patch(
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

    pub(in crate::workspace) fn sample_result() -> AiExecutedToolResult {
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

    pub(in crate::workspace) fn sample_target() -> AiOrchestratorTarget {
        let mut refs = std::collections::BTreeMap::new();
        refs.insert("nodeId".to_string(), "prod-node-1".to_string());
        refs.insert(
            "connectionId".to_string(),
            "4cb736c8-579f-40de-9f71-4efe2c90e7ef".to_string(),
        );
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
        }
    }

    #[test]
    pub(in crate::workspace) fn target_query_uses_only_safe_ranking_fields() {
        let target = sample_target();

        assert!(target_matches_ai_query(&target, "prod.example.com"));
        assert!(target_matches_ai_query(
            &target,
            "4cb736c8-579f-40de-9f71-4efe2c90e7ef"
        ));
        assert!(!target_matches_ai_query(&target, "4cb736c8"));
        assert!(!target_matches_ai_query(&target, "prod-node-1"));
        assert!(!target_matches_ai_query(&target, "deploy"));
        assert!(!target_matches_ai_query(&target, "staging"));
    }

    #[test]
    pub(in crate::workspace) fn duplicate_labels_remain_explicitly_ambiguous() {
        let first = sample_target();
        let mut second = sample_target();
        second.id = "ssh-node:second-internal-owner".to_string();
        second
            .refs
            .insert("nodeId".to_string(), "second-internal-owner".to_string());
        let mut snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        snapshot.targets = vec![first, second];

        let result = snapshot.select_target(&serde_json::json!({
            "query": "prod.example.com",
            "intent": "connection",
            "kind": "ssh-node",
        }));

        assert!(!result.ok);
        assert_eq!(
            result.error_code.as_deref(),
            Some("target_disambiguation_required")
        );
        assert_eq!(result.targets.len(), 2);
    }

    #[test]
    pub(in crate::workspace) fn target_discovery_applies_a_hard_result_limit() {
        let mut snapshot = AiOrchestratorRuntimeSnapshot::background_result_projection();
        snapshot.targets = (0..=AI_TARGET_DISCOVERY_LIMIT)
            .map(|index| {
                let mut target = sample_target();
                target.id = format!("ssh-node:internal-{index}");
                target.label = format!("host-{index}.example.com");
                target
            })
            .collect();

        let result = snapshot.list_targets(&serde_json::json!({
            "view": "connections",
            "kind": "ssh-node",
        }));

        assert!(result.ok);
        assert_eq!(result.targets.len(), AI_TARGET_DISCOVERY_LIMIT);
    }


    #[test]
    pub(in crate::workspace) fn long_tool_output_uses_head_tail_preview_metadata() {
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
    pub(in crate::workspace) fn run_command_execution_summary_preserves_visibility_and_state() {
        let mut result = sample_result();
        result.envelope = serde_json::json!({
            "ok": true,
            "summary": "Command sent to terminal.",
            "output": "Command sent: uptime",
            "data": {
                "executionState": "sent",
                "visibleInTerminal": true
            },
            "targets": [{
                "id": "ssh-node:prod-node-1",
                "kind": "ssh-node",
                "label": "prod.example.com",
                "metadata": { "state": "connected", "refs": { "sessionId": "42" } }
            }],
            "meta": { "toolName": "run_command", "durationMs": 7, "truncated": false }
        });

        annotate_ai_run_command_execution_result(
            &mut result,
            &serde_json::json!({ "command": "uptime" }),
        );

        assert_eq!(
            result.envelope.pointer("/execution/state"),
            Some(&serde_json::json!("sent"))
        );
        assert_eq!(
            result.envelope.pointer("/execution/visibleInTerminal"),
            Some(&serde_json::json!(true))
        );
    }















    #[test]
    pub(in crate::workspace) fn model_result_projection_removes_internal_runtime_identifiers() {
        let value = serde_json::json!({
            "nodeId": "node-1",
            "session_id": "42",
            "runtimeEpoch": "epoch-1",
            "target": "terminal-session:42",
            "refs": { "tabId": "tab-1" },
            "stable": { "kind": "saved_connection", "id": "4e22e673-067e-46e2-8b9f-902d7b21af4c" },
        });
        let projected = ai_model_safe_runtime_value(&value);

        assert!(projected.get("nodeId").is_none());
        assert!(projected.get("session_id").is_none());
        assert!(projected.get("runtimeEpoch").is_none());
        assert!(projected.get("refs").is_none());
        assert!(!projected.to_string().contains("terminal-session:42"));
        assert_eq!(
            projected.pointer("/stable/id"),
            Some(&serde_json::json!("4e22e673-067e-46e2-8b9f-902d7b21af4c"))
        );
    }

    #[test]
    pub(in crate::workspace) fn model_result_projection_bounds_structured_data() {
        let value = serde_json::json!({
            "content": "x".repeat(AI_MODEL_RESULT_STRING_MAX_CHARS + 1),
            "items": vec![serde_json::Value::Null; AI_MODEL_RESULT_DATA_MAX_NODES],
        });

        let (projected, truncated) = ai_model_safe_runtime_value_with_limits(&value);

        assert!(truncated);
        assert!(
            projected["content"]
                .as_str()
                .is_some_and(|content| content.chars().count() <= AI_MODEL_RESULT_STRING_MAX_CHARS)
        );
        assert!(
            projected["items"]
                .as_array()
                .is_some_and(|items| items.len() < AI_MODEL_RESULT_DATA_MAX_NODES)
        );
    }




    #[test]
    pub(in crate::workspace) fn active_context_matches_tab_session_or_node_refs() {
        let mut target = sample_target();
        target.refs.insert("tabId".to_string(), "7".to_string());
        target
            .refs
            .insert("sessionId".to_string(), "42".to_string());

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
    pub(in crate::workspace) fn default_policy_annotation_does_not_mark_bypass() {
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
        assert!(
            result
                .envelope
                .pointer("/meta/policyDecision/profileId")
                .is_none()
        );
    }



    #[test]
    fn terminal_control_keys_map_to_protocol_bytes() {
        assert_eq!(
            ai_terminal_input_payload(&serde_json::json!({ "key": "ctrl_c" })),
            b"\x03"
        );
        assert_eq!(
            ai_terminal_input_payload(&serde_json::json!({ "key": "page_down" })),
            b"\x1b[6~"
        );
    }

    #[test]
    fn terminal_wait_matches_only_the_requested_completed_command() {
        let records = vec![oxideterm_gpui_terminal::TerminalAiCommandRecord {
            command_id: "command-1".to_string(),
            command: "printf done".to_string(),
            source: oxideterm_terminal::TerminalCommandMarkDetectionSource::Ai,
            status: oxideterm_gpui_terminal::TerminalCommandFactStatus::Closed,
            started_at: 10,
            finished_at: Some(20),
            exit_code: Some(0),
            start_line: 1,
            end_line: Some(2),
        }];

        let matched = ai_terminal_wait_match(
            &serde_json::json!({
                "condition": "command_completed",
                "command_id": "command-1"
            }),
            "",
            "done",
            false,
            false,
            &records,
        );

        assert_eq!(
            matched
                .as_ref()
                .and_then(|value| value.pointer("/command/exitCode")),
            Some(&serde_json::json!(0))
        );
        assert!(
            ai_terminal_wait_match(
                &serde_json::json!({
                    "condition": "command_completed",
                    "command_id": "command-2"
                }),
                "",
                "done",
                false,
                false,
                &records,
            )
            .is_none()
        );
    }
}
