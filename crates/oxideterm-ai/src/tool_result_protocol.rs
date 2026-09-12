//! Model-facing tool-result envelopes, evidence facts, condensation, and output limits.

use crate::compute_ai_prompt_budget;

/// A runtime-neutral tool execution result ready for protocol formatting.
#[derive(Clone, Debug)]
pub struct AiExecutedToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u128,
    pub envelope: serde_json::Value,
}

pub const AI_TOOL_MODEL_OUTPUT_MAX_CHARS: usize = 12_000;
pub const AI_TOOL_MODEL_ERROR_OUTPUT_MAX_CHARS: usize = 2_000;
pub const AI_TOOL_MODEL_SUMMARY_MAX_CHARS: usize = 1_000;
pub const AI_TOOL_MODEL_ERROR_MESSAGE_MAX_CHARS: usize = 1_000;

pub fn ai_to_usable_budget_threshold(
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

pub fn ai_tool_result_model_content(result: &AiExecutedToolResult) -> String {
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
    for key in ["warnings", "observations", "targets", "nextActions"] {
        ai_insert_non_empty_model_array(&mut payload, key, envelope.get(key));
    }
    for key in ["disambiguation", "outputPreview"] {
        if let Some(value) = envelope.get(key) {
            payload.insert(key.to_string(), value.clone());
        }
    }
    let evidence_facts = ai_tool_result_evidence_facts_for_model(result, &envelope);
    if !evidence_facts.is_empty() {
        payload.insert(
            "evidenceFacts".to_string(),
            serde_json::json!(evidence_facts),
        );
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

pub fn ai_tool_result_evidence_facts_for_model(
    result: &AiExecutedToolResult,
    envelope: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let mut facts = Vec::new();
    for (source_kind, pointer) in [
        ("summary", "/summary"),
        ("output", "/output"),
        ("execution.exit_code", "/execution/exitCode"),
        (
            "execution.visible_in_terminal",
            "/execution/visibleInTerminal",
        ),
        ("execution.state", "/execution/state"),
    ] {
        let Some(value) = envelope.pointer(pointer) else {
            continue;
        };
        if value.as_str().is_some_and(|value| value.trim().is_empty()) {
            continue;
        }
        facts.push(serde_json::json!({
            "factId": format!("{}.{}", result.tool_call_id, source_kind),
            "toolCallId": result.tool_call_id,
            "toolName": result.tool_name,
            "sourceKind": source_kind,
        }));
    }
    facts
}

pub fn ai_tool_result_envelope_or_legacy(result: &AiExecutedToolResult) -> serde_json::Value {
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

pub fn ai_tool_result_model_error(
    envelope: &serde_json::Value,
) -> (Option<serde_json::Value>, bool) {
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

pub fn ai_insert_execution_shortcuts_for_model(
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

pub fn ai_insert_non_empty_model_array(
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

pub fn truncate_ai_tool_result_for_model(value: &str, max_chars: usize) -> (String, bool) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn result(envelope: serde_json::Value) -> AiExecutedToolResult {
        AiExecutedToolResult {
            tool_call_id: "call-1".to_string(),
            tool_name: "run_command".to_string(),
            success: true,
            output: "fallback".to_string(),
            error: None,
            duration_ms: 12,
            envelope,
        }
    }

    #[test]
    fn model_content_keeps_execution_shortcuts_and_evidence() {
        let content = ai_tool_result_model_content(&result(serde_json::json!({
            "ok": true,
            "summary": "done",
            "output": "output",
            "execution": {
                "target": "terminal-session:1",
                "exitCode": 0,
                "visibleInTerminal": true
            },
            "meta": {
                "toolName": "run_command",
                "truncated": false
            }
        })));
        let payload: serde_json::Value = serde_json::from_str(&content).expect("model-facing JSON");

        assert_eq!(payload["target"], "terminal-session:1");
        assert_eq!(payload["exitCode"], 0);
        assert_eq!(payload["evidenceFacts"].as_array().map(Vec::len), Some(4));
    }

    #[test]
    fn model_content_truncates_error_output_more_aggressively() {
        let content = ai_tool_result_model_content(&result(serde_json::json!({
            "ok": false,
            "summary": "failed",
            "output": "x".repeat(AI_TOOL_MODEL_ERROR_OUTPUT_MAX_CHARS + 10),
            "error": {
                "code": "failed",
                "message": "failure"
            }
        })));
        let payload: serde_json::Value = serde_json::from_str(&content).expect("model-facing JSON");

        assert_eq!(payload["truncated"], true);
        assert!(
            payload["output"]
                .as_str()
                .is_some_and(|output| output.contains("[truncated:"))
        );
    }
}
