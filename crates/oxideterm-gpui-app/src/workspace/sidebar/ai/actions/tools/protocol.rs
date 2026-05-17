const AI_TOOL_CONDENSE_KEEP_RECENT: usize = 5;
const AI_TOOL_CONDENSE_SUMMARY_MAX_CHARS: usize = 300;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AiOrchestratorObligationMode {
    Auto,
    Required,
}

#[derive(Clone, Debug)]
struct AiOrchestratorObligation {
    mode: AiOrchestratorObligationMode,
    reason: String,
    candidate_tools: Vec<String>,
}

impl AiOrchestratorObligation {
    fn auto() -> Self {
        Self {
            mode: AiOrchestratorObligationMode::Auto,
            reason: "No mandatory app action detected.".to_string(),
            candidate_tools: Vec::new(),
        }
    }
}

fn ai_classify_orchestrator_obligation(text: &str) -> AiOrchestratorObligation {
    let lower = text.to_lowercase();
    let discovery = ["有哪些", "哪些", "列出", "看看", "查看", "show", "list", "available", "host", "target", "connection", "主机", "连接"]
        .iter()
        .any(|needle| lower.contains(needle));
    let direct_target_action = ["连接到", "connect to", "run on", "在"].iter().any(|needle| lower.contains(needle))
        && ["运行", "执行", "run", "execute"].iter().any(|needle| lower.contains(needle));
    if discovery && !direct_target_action {
        return AiOrchestratorObligation {
            mode: AiOrchestratorObligationMode::Required,
            reason: "The user is asking for real available app targets; call list_targets before answering.".to_string(),
            candidate_tools: vec!["list_targets".to_string()],
        };
    }

    let action = [
        "连接", "打开", "执行", "运行", "修改", "设置", "上传", "下载", "读取", "写入", "搜索",
        "connect", "open", "run", "execute", "modify", "set", "upload", "download", "read",
        "write", "search",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    if action {
        return AiOrchestratorObligation {
            mode: AiOrchestratorObligationMode::Required,
            reason: "The request asks OxideTerm to inspect, connect, execute, open, or modify real app state.".to_string(),
            candidate_tools: vec![
                "list_targets".to_string(),
                "select_target".to_string(),
                "connect_target".to_string(),
                "run_command".to_string(),
                "open_app_surface".to_string(),
                "read_resource".to_string(),
                "write_resource".to_string(),
            ],
        };
    }

    AiOrchestratorObligation::auto()
}

fn ai_orchestrator_obligation_prompt(obligation: &AiOrchestratorObligation) -> Option<String> {
    if obligation.mode != AiOrchestratorObligationMode::Required {
        return None;
    }
    Some(
        [
            "## Required Tool Call".to_string(),
            obligation.reason.clone(),
            format!(
                "Call one of these tools before the final answer: {}.",
                obligation
                    .candidate_tools
                    .iter()
                    .map(|tool| format!("`{tool}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "If a tool returns disambiguation or multiple targets, ask the user to choose instead of guessing.".to_string(),
        ]
        .join("\n"),
    )
}

fn ai_required_tool_retry_prompt(obligation: &AiOrchestratorObligation) -> String {
    let candidates = if obligation.candidate_tools.is_empty() {
        "the relevant available tool".to_string()
    } else {
        obligation
            .candidate_tools
            .iter()
            .take(8)
            .map(|tool| format!("`{tool}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    [
        "The previous assistant response did not call a structured tool, but this user request requires real app/tool state.".to_string(),
        format!("Reason: {}.", obligation.reason),
        format!("Call one of these tools before giving a final answer: {candidates}."),
        "Do not claim that anything was opened, connected, executed, read, modified, checked, verified, or diagnosed until a tool result proves it.".to_string(),
    ]
    .join("\n")
}

fn ai_should_retry_required_tool_round(obligation: &AiOrchestratorObligation, assistant_text: &str) -> bool {
    if obligation.mode != AiOrchestratorObligationMode::Required || obligation.candidate_tools.is_empty() {
        return false;
    }
    let trimmed = assistant_text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_lowercase();
    let action_claim = [
        "opened", "connected", "executed", "ran", "read", "modified", "changed", "checked",
        "verified", "diagnosed", "found", "failed", "succeeded", "已经", "已", "我来", "我已",
        "现在", "结果是", "连接失败", "执行完成", "修改完成",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    if action_claim {
        return true;
    }
    let looks_like_clarification = trimmed.ends_with('?')
        || trimmed.ends_with('？')
        || ["请", "需要你", "你可以", "是否", "哪一个", "哪个", "确认"]
            .iter()
            .any(|needle| trimmed.contains(needle));
    !looks_like_clarification
}

fn ai_user_explicitly_requested_json(text: &str) -> bool {
    let lower = text.to_lowercase();
    ["json", "jsonl", "json schema", "jsonschema", "payload", "response format", "object literal", "schema"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn ai_should_trigger_hard_deny(assistant_text: &str, user_requested_json: bool) -> bool {
    if user_requested_json {
        return false;
    }
    let trimmed = ai_strip_code_fence(assistant_text);
    if trimmed.is_empty() {
        return false;
    }
    let looks_json = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'));
    if !looks_json {
        return false;
    }
    let lower = trimmed.to_lowercase();
    let field_count = [
        "\"name\"", "\"arguments\"", "\"stdout\"", "\"stderr\"", "\"exit_code\"", "\"exit-code\"",
        "\"status\"", "\"tool_call_id\"", "\"toolname\"", "\"toolcallid\"",
    ]
    .iter()
    .filter(|needle| lower.contains(*needle))
    .count();
    let looks_like_tool_request = lower.contains("\"name\"") && lower.contains("\"arguments\"");
    let looks_like_tool_result = (lower.contains("\"stdout\"") || lower.contains("\"stderr\""))
        && (lower.contains("\"exit_code\"") || lower.contains("\"exit-code\"") || lower.contains("\"status\""));
    looks_like_tool_request || looks_like_tool_result || field_count >= 3
}

fn ai_strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    for prefix in ["```json", "```javascript", "```js", "```text", "```"] {
        if let Some(rest) = trimmed.strip_prefix(prefix)
            && let Some(inner) = rest.strip_suffix("```")
        {
            return inner.trim().to_string();
        }
    }
    trimmed.to_string()
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
        "data": serde_json::Value::Null,
        "error": {
            "code": code,
            "message": message,
            "recoverable": true,
        },
        "targets": [],
        "meta": {
            "toolName": tool_name,
            "durationMs": 0,
            "verified": false,
            "capability": serde_json::Value::Null,
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

fn ai_terminal_input_payload(args: &serde_json::Value) -> String {
    if let Some(control) = args.get("control").and_then(serde_json::Value::as_str) {
        return match control {
            "ctrl-c" => "\u{3}",
            "ctrl-d" => "\u{4}",
            "ctrl-z" => "\u{1a}",
            _ => "",
        }
        .to_string();
    }
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

fn settings_tab_for_ai_section(section: &str) -> Option<SettingsTab> {
    match section {
        "general" => Some(SettingsTab::General),
        "portable" => Some(SettingsTab::Portable),
        "terminal" => Some(SettingsTab::Terminal),
        "appearance" => Some(SettingsTab::Appearance),
        "local" | "local_terminal" => Some(SettingsTab::Local),
        "connections" | "connection_manager" => Some(SettingsTab::Connections),
        "ssh" => Some(SettingsTab::Ssh),
        "reconnect" => Some(SettingsTab::Reconnect),
        "sftp" => Some(SettingsTab::Sftp),
        "ide" => Some(SettingsTab::Ide),
        "ai" | "assistant" => Some(SettingsTab::Ai),
        "knowledge" | "rag" => Some(SettingsTab::Knowledge),
        "keybindings" | "keyboard" => Some(SettingsTab::Keybindings),
        "help" => Some(SettingsTab::Help),
        _ => None,
    }
}

fn terminal_delta_output(before: &str, after: &str) -> String {
    if after.starts_with(before) {
        let delta = after[before.len()..].trim();
        if !delta.is_empty() {
            return delta.to_string();
        }
    }
    trim_tail_chars(after, 4000)
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
    ["password", "passphrase", "sudo", "验证码", "口令", "密码"]
        .iter()
        .any(|needle| tail.contains(needle))
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
}
