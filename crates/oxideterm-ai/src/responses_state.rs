use crate::{AiChatMessage, AiChatRole};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Associates wire IDs with runtime IDs without changing either identity domain.
pub fn responses_round_state(
    parts: &[Value],
    call_ids: &HashMap<String, String>,
    results: &[AiChatMessage],
) -> Option<Value> {
    let output = parts.first()?.get("output")?.as_array()?;
    let ids: serde_json::Map<String, Value> = call_ids
        .iter()
        .map(|(wire, local)| (local.clone(), json!(wire)))
        .collect();
    let results: Vec<Value> = results
        .iter()
        .filter(|message| message.role == AiChatRole::Tool)
        .filter_map(|message| {
            let local = message.tool_call_id.as_ref()?;
            let wire = ids.get(local)?.as_str()?;
            Some(json!({
                "type": "function_call_output",
                "call_id": wire,
                "output": crate::sanitize_for_ai(&message.content),
            }))
        })
        .collect();
    Some(json!({ "output": output, "callIds": ids, "results": results }))
}

/// Only completed rounds reach durable history; interrupted calls remain UI projections.
pub fn append_responses_round(message: &mut AiChatMessage, scope: &str, round: Value) {
    if !scope.starts_with("responses:") {
        return;
    }
    crate::ensure_ai_turn(message);
    let Some(turn) = message.turn.as_mut().and_then(Value::as_object_mut) else {
        return;
    };
    let providers = turn.entry("providerParts").or_insert_with(|| json!({}));
    let Some(providers) = providers.as_object_mut() else {
        return;
    };
    let rounds = providers.entry(scope).or_insert_with(|| json!([]));
    if let Some(rounds) = rounds.as_array_mut() {
        let mut round = crate::sanitize_tool_protocol_json_for_persistence(&round);
        // Tool results contain embedded JSON, including runtime handles, that must not survive a restart.
        let names: HashMap<String, String> = round["output"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                Some((
                    item["call_id"].as_str()?.into(),
                    item["name"].as_str()?.into(),
                ))
            })
            .collect();
        if let Some(results) = round["results"].as_array_mut() {
            for result in results {
                let name = result["call_id"]
                    .as_str()
                    .and_then(|id| names.get(id))
                    .map(String::as_str)
                    .unwrap_or_default();
                if let Some(output) = result["output"].as_str()
                    && let Ok(value) = serde_json::from_str::<Value>(output)
                {
                    result["output"] = Value::String(
                        crate::sanitize_tool_result_json_for_persistence(name, &value).to_string(),
                    );
                }
            }
        }
        rounds.push(round);
    }
}

pub fn has_responses_history(message: &AiChatMessage) -> bool {
    message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("providerParts"))
        .and_then(Value::as_object)
        .is_some_and(|providers| providers.keys().any(|key| key.starts_with("responses:")))
}

pub(crate) fn responses_history_tokens(message: &AiChatMessage) -> Option<usize> {
    let providers = message.turn.as_ref()?.get("providerParts")?.as_object()?;
    let parts = providers
        .iter()
        .filter(|(key, _)| key.starts_with("responses:"))
        .map(|(_, value)| crate::ai_estimated_tokens(&value.to_string()))
        .sum();
    (parts > 0).then_some(parts)
}

/// An allowlist of transport categories, never remote error text.
pub fn responses_error_label(error: &str) -> Option<&'static str> {
    match error {
        "responses_failed" => Some("settings_view.ai.responses_failed"),
        "responses_incomplete_limit" => Some("settings_view.ai.responses_incomplete_limit"),
        "responses_incomplete_filter" => Some("settings_view.ai.responses_incomplete_filter"),
        "responses_incomplete" => Some("settings_view.ai.responses_incomplete"),
        "responses_disconnected" => Some("settings_view.ai.responses_disconnected"),
        _ => None,
    }
}

/// Discard only the request copy's foreign protocol state before normalization and budgeting.
pub fn scope_responses_history(messages: &mut [AiChatMessage], config: &crate::AiChatStreamConfig) {
    let active = config.uses_responses().then(|| config.response_state_key());
    for message in messages {
        if let Some(providers) = message
            .turn
            .as_mut()
            .and_then(|turn| turn.get_mut("providerParts"))
            .and_then(Value::as_object_mut)
        {
            providers.retain(|key, _| {
                !key.starts_with("responses:") || active.as_deref() == Some(key.as_str())
            });
        }
    }
}
