fn upsert_ai_tool_call(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
) {
    if let Some(slot) = message.tool_calls.iter_mut().find(|call| {
        call.get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|existing| existing == id)
    }) {
        if let Some(object) = slot.as_object_mut() {
            object.insert("name".to_string(), serde_json::json!(name));
            object.insert("arguments".to_string(), serde_json::json!(arguments));
            object.insert("status".to_string(), serde_json::json!(status));
        }
    } else {
        message.tool_calls.push(serde_json::json!({
            "id": id,
            "name": name,
            "arguments": arguments,
            "status": status,
            "result": serde_json::Value::Null,
        }));
    }
}

fn update_ai_tool_call_status(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
    result: Option<serde_json::Value>,
    risk: Option<String>,
    summary: Option<String>,
    round_id_override: Option<&str>,
    round_number_override: Option<i64>,
) {
    upsert_ai_tool_call(message, id, name, arguments, status);
    update_ai_turn_tool_status(
        message,
        id,
        name,
        arguments,
        status,
        round_id_override,
        round_number_override,
    );
    let result_for_turn = result.clone();
    if let Some(slot) = message.tool_calls.iter_mut().find(|call| {
        call.get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|existing| existing == id)
    }) && let Some(object) = slot.as_object_mut()
    {
        if let Some(result) = result {
            object.insert("result".to_string(), result);
        }
        if let Some(risk) = risk {
            object.insert("risk".to_string(), serde_json::json!(risk));
        }
        if let Some(summary) = summary {
            object.insert("summary".to_string(), serde_json::json!(summary));
        }
    }
    if let Some(result) = result_for_turn {
        append_ai_turn_tool_result(message, id, name, status, &result);
    }
}

fn ensure_ai_turn(message: &mut AiChatMessage) {
    let needs_init = !message
        .turn
        .as_ref()
        .is_some_and(|turn| turn.as_object().is_some());
    if needs_init {
        message.turn = Some(serde_json::json!({
            "id": message.id.clone(),
            "status": if message.is_streaming { "streaming" } else { "complete" },
            "plainTextSummary": message.content.clone(),
            "parts": [],
            "toolRounds": [],
        }));
    }

    let Some(object) = message
        .turn
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    object
        .entry("id".to_string())
        .or_insert_with(|| serde_json::json!(message.id.clone()));
    object
        .entry("status".to_string())
        .or_insert_with(|| serde_json::json!(if message.is_streaming { "streaming" } else { "complete" }));
    object
        .entry("parts".to_string())
        .or_insert_with(|| serde_json::json!([]));
    object
        .entry("toolRounds".to_string())
        .or_insert_with(|| serde_json::json!([]));
    object
        .entry("pendingSummaries".to_string())
        .or_insert_with(|| serde_json::json!([]));
}

fn set_ai_turn_status(message: &mut AiChatMessage, status: &str) {
    ensure_ai_turn(message);
    if let Some(object) = message
        .turn
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    {
        object.insert("status".to_string(), serde_json::json!(status));
        object.insert(
            "plainTextSummary".to_string(),
            serde_json::json!(message.content),
        );
    }
}

fn finalize_ai_turn_suggestions(message: &mut AiChatMessage) {
    let parsed = parse_ai_suggestions(&message.content);
    if !parsed.has_suggestions_block {
        return;
    }

    message.content = parsed.clean_content;
    message.suggestions = parsed.suggestions;

    if let Some(parts) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("parts"))
        .and_then(serde_json::Value::as_array_mut)
        && let Some(text_part) = parts.iter_mut().rev().find(|part| {
            part.get("type").and_then(serde_json::Value::as_str) == Some("text")
        })
        && let Some(object) = text_part.as_object_mut()
        && let Some(text) = object.get("text").and_then(serde_json::Value::as_str)
    {
        let part_parsed = parse_ai_suggestions(text);
        if part_parsed.has_suggestions_block {
            object.insert("text".to_string(), serde_json::json!(part_parsed.clean_content));
        }
    }

    if let Some(object) = message
        .turn
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    {
        object.insert(
            "plainTextSummary".to_string(),
            serde_json::json!(message.content),
        );
    }
}

fn mutate_ai_turn_parts(message: &mut AiChatMessage, f: impl FnOnce(&mut Vec<serde_json::Value>)) {
    ensure_ai_turn(message);
    if let Some(parts) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("parts"))
        .and_then(serde_json::Value::as_array_mut)
    {
        f(parts);
    }
}

fn mutate_ai_turn_rounds(message: &mut AiChatMessage, f: impl FnOnce(&mut Vec<serde_json::Value>)) {
    ensure_ai_turn(message);
    if let Some(rounds) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("toolRounds"))
        .and_then(serde_json::Value::as_array_mut)
    {
        f(rounds);
    }
}

fn upsert_ai_round_summary(
    message: &mut AiChatMessage,
    round_id: &str,
    text: &str,
    metadata: serde_json::Value,
) {
    ensure_ai_turn(message);
    if attach_ai_round_summary(message, round_id, text, Some(metadata.clone())) {
        remove_ai_pending_round_summary(message, round_id);
        return;
    }

    if let Some(pending) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("pendingSummaries"))
        .and_then(serde_json::Value::as_array_mut)
    {
        let mut summary = serde_json::json!({
            "roundId": round_id,
            "text": text,
        });
        if let Some(object) = summary.as_object_mut()
            && !metadata.is_null()
        {
            object.insert("metadata".to_string(), metadata);
        }
        if let Some(existing) = pending.iter_mut().find(|summary| {
            summary
                .get("roundId")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == round_id)
        }) {
            *existing = summary;
        } else {
            pending.push(summary);
        }
    }
}

fn normalize_ai_pending_summaries(message: &mut AiChatMessage) {
    ensure_ai_turn(message);
    let pending = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("pendingSummaries"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if pending.is_empty() {
        return;
    }

    let mut unresolved = Vec::new();
    for summary in pending {
        let Some(round_id) = summary
            .get("roundId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let text = summary
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if text.is_empty() {
            continue;
        }
        let metadata = summary.get("metadata").cloned();
        if !attach_ai_round_summary(message, &round_id, &text, metadata) {
            unresolved.push(summary);
        }
    }

    if let Some(pending) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("pendingSummaries"))
        .and_then(serde_json::Value::as_array_mut)
    {
        *pending = unresolved;
    }
}

fn attach_ai_round_summary(
    message: &mut AiChatMessage,
    round_id: &str,
    text: &str,
    metadata: Option<serde_json::Value>,
) -> bool {
    let Some(rounds) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("toolRounds"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };
    let Some(round) = rounds.iter_mut().find(|round| {
        round
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|existing| existing == round_id)
    }) else {
        return false;
    };
    let Some(object) = round.as_object_mut() else {
        return false;
    };
    object.insert("summary".to_string(), serde_json::json!(text));
    if let Some(metadata) = metadata
        && !metadata.is_null()
    {
        object.insert("summaryMetadata".to_string(), metadata);
    }
    true
}

fn remove_ai_pending_round_summary(message: &mut AiChatMessage, round_id: &str) {
    if let Some(pending) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("pendingSummaries"))
        .and_then(serde_json::Value::as_array_mut)
    {
        pending.retain(|summary| {
            !summary
                .get("roundId")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == round_id)
        });
    }
}

fn set_ai_turn_round_stateful_marker(
    message: &mut AiChatMessage,
    round_id: &str,
    marker: Option<&str>,
) {
    ensure_ai_turn(message);
    mutate_ai_turn_rounds(message, |rounds| {
        let Some(round) = rounds.iter_mut().find(|round| {
            round
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == round_id)
        }) else {
            return;
        };
        let Some(object) = round.as_object_mut() else {
            return;
        };
        if let Some(marker) = marker {
            object.insert("statefulMarker".to_string(), serde_json::json!(marker));
        } else {
            object.remove("statefulMarker");
        }
    });
}

fn ai_turn_plain_text_summary(message: &AiChatMessage) -> Option<String> {
    let parts = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("parts"))
        .and_then(serde_json::Value::as_array)?;
    let summary = parts
        .iter()
        .filter(|part| part.get("type").and_then(serde_json::Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
        .collect::<String>();
    Some(summary)
}

fn append_ai_turn_text_part(
    message: &mut AiChatMessage,
    part_type: &str,
    text: &str,
    streaming: bool,
) {
    if text.is_empty() {
        return;
    }
    mutate_ai_turn_parts(message, |parts| {
        if let Some(last) = parts
            .last_mut()
            .and_then(serde_json::Value::as_object_mut)
            .filter(|part| part.get("type").and_then(serde_json::Value::as_str) == Some(part_type))
        {
            let next = last
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
                + text;
            last.insert("text".to_string(), serde_json::json!(next));
            if part_type == "thinking" {
                last.insert("streaming".to_string(), serde_json::json!(streaming));
            }
            return;
        }
        let mut part = serde_json::json!({
            "type": part_type,
            "text": text,
        });
        if part_type == "thinking"
            && let Some(object) = part.as_object_mut()
        {
            object.insert("streaming".to_string(), serde_json::json!(streaming));
        }
        parts.push(part);
    });
}

fn append_ai_turn_error_part(message: &mut AiChatMessage, error: &str) {
    mutate_ai_turn_parts(message, |parts| {
        parts.push(serde_json::json!({
            "type": "error",
            "message": error,
            "code": "stream_error",
        }));
    });
}

fn append_ai_turn_guardrail_part(
    message: &mut AiChatMessage,
    code: &str,
    guardrail_message: &str,
    raw_text: Option<&str>,
) {
    mutate_ai_turn_parts(message, |parts| {
        let mut part = serde_json::json!({
            "type": "guardrail",
            "code": code,
            "message": guardrail_message,
        });
        if let Some(raw_text) = raw_text
            && let Some(object) = part.as_object_mut()
        {
            object.insert("rawText".to_string(), serde_json::json!(raw_text));
        }
        parts.push(part);
    });
}

fn upsert_ai_turn_tool_call(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
) {
    let (round_id, round_number) = ai_turn_round_for_tool_call(message, id);
    mutate_ai_turn_parts(message, |parts| {
        if let Some(existing) = parts.iter_mut().find(|part| {
            part.get("type").and_then(serde_json::Value::as_str) == Some("tool_call")
                && part
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == id)
        }) && let Some(object) = existing.as_object_mut()
        {
            object.insert("name".to_string(), serde_json::json!(name));
            object.insert("argumentsText".to_string(), serde_json::json!(arguments));
            object.insert("status".to_string(), serde_json::json!(status));
            return;
        }

        parts.push(serde_json::json!({
            "type": "tool_call",
            "id": id,
            "name": name,
            "argumentsText": arguments,
            "status": status,
        }));
    });
    upsert_ai_turn_round_tool_call(message, id, name, arguments, status, &round_id, round_number);
}

fn update_ai_turn_tool_status(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
    round_id_override: Option<&str>,
    round_number_override: Option<i64>,
) {
    if round_id_override.is_none() {
        upsert_ai_turn_tool_call(message, id, name, arguments, "complete");
    }
    let (round_id, round_number) =
        ai_turn_round_for_tool_call_with_override(message, id, round_id_override, round_number_override);
    upsert_ai_turn_round_tool_call(message, id, name, arguments, status, &round_id, round_number);
}

fn upsert_ai_turn_round_tool_call(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
    round_id: &str,
    round_number: i64,
) {
    let timestamp = message.timestamp_ms;
    mutate_ai_turn_rounds(message, |rounds| {
        if !rounds.iter().any(|round| {
            round
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == round_id)
        }) {
            rounds.push(serde_json::json!({
                "id": round_id,
                "round": round_number,
                "timestamp": timestamp,
                "toolCalls": [],
            }));
        }
        let Some(tool_calls) = rounds
            .iter_mut()
            .find(|round| {
                round
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == round_id)
            })
            .and_then(|round| round.get_mut("toolCalls"))
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let state_field = match status {
            "pending_user_approval" => Some(("approvalState", "pending")),
            "approved" => Some(("approvalState", "approved")),
            "rejected" => Some(("approvalState", "rejected")),
            "running" => Some(("executionState", "running")),
            "completed" => Some(("executionState", "completed")),
            "error" => Some(("executionState", "error")),
            "pending" | "partial" | "complete" => Some(("executionState", "pending")),
            _ => None,
        };
        if let Some(existing) = tool_calls.iter_mut().find(|tool_call| {
            tool_call
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == id)
        }) && let Some(object) = existing.as_object_mut()
        {
            object.insert("name".to_string(), serde_json::json!(name));
            object.insert("argumentsText".to_string(), serde_json::json!(arguments));
            if let Some((field, value)) = state_field {
                object.insert(field.to_string(), serde_json::json!(value));
            }
            return;
        }
        let mut call = serde_json::json!({
            "id": id,
            "name": name,
            "argumentsText": arguments,
        });
        if let Some((field, value)) = state_field
            && let Some(object) = call.as_object_mut()
        {
            object.insert(field.to_string(), serde_json::json!(value));
        }
        tool_calls.push(call);
    });
    normalize_ai_pending_summaries(message);
}

fn ai_turn_round_for_tool_call(message: &AiChatMessage, id: &str) -> (String, i64) {
    if let Some(existing) = ai_turn_round_for_existing_tool_call(message, id) {
        return existing;
    }

    let Some(rounds) = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("toolRounds"))
        .and_then(serde_json::Value::as_array)
    else {
        return (format!("{}-round-1", message.id), 1);
    };

    let latest_round = rounds
        .iter()
        .filter_map(|round| {
            let id = round.get("id").and_then(serde_json::Value::as_str)?;
            let number = round.get("round").and_then(serde_json::Value::as_i64)?;
            Some((id.to_string(), number))
        })
        .max_by_key(|(_, number)| *number);

    let Some((latest_round_id, latest_round_number)) = latest_round else {
        return (format!("{}-round-1", message.id), 1);
    };

    if ai_turn_round_has_result(message, &latest_round_id) {
        let next = latest_round_number.saturating_add(1);
        (format!("{}-round-{next}", message.id), next)
    } else {
        (latest_round_id, latest_round_number)
    }
}

fn ai_turn_round_for_tool_call_with_override(
    message: &AiChatMessage,
    id: &str,
    round_id_override: Option<&str>,
    round_number_override: Option<i64>,
) -> (String, i64) {
    if let Some(round_id) = round_id_override {
        return (
            round_id.to_string(),
            round_number_override.unwrap_or_else(|| {
                ai_turn_round_for_existing_tool_call(message, id)
                    .map(|(_, number)| number)
                    .unwrap_or(1)
            }),
        );
    }
    ai_turn_round_for_tool_call(message, id)
}

fn ai_turn_round_for_existing_tool_call(message: &AiChatMessage, id: &str) -> Option<(String, i64)> {
    let rounds = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("toolRounds"))
        .and_then(serde_json::Value::as_array)?;
    for round in rounds {
        let has_tool = round
            .get("toolCalls")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tool_calls| {
                tool_calls.iter().any(|tool_call| {
                    tool_call
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|existing| existing == id)
                })
            });
        if has_tool {
            let round_id = round.get("id")?.as_str()?.to_string();
            let round_number = round.get("round")?.as_i64()?;
            return Some((round_id, round_number));
        }
    }
    None
}

fn ai_turn_round_has_result(message: &AiChatMessage, round_id: &str) -> bool {
    let Some(round_tool_ids) = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("toolRounds"))
        .and_then(serde_json::Value::as_array)
        .and_then(|rounds| {
            rounds.iter().find(|round| {
                round
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == round_id)
            })
        })
        .and_then(|round| round.get("toolCalls"))
        .and_then(serde_json::Value::as_array)
        .map(|tool_calls| {
            tool_calls
                .iter()
                .filter_map(|tool_call| tool_call.get("id").and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
    else {
        return false;
    };

    message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("parts"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|parts| {
            parts.iter().any(|part| {
                part.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
                    && part
                        .get("toolCallId")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|tool_call_id| round_tool_ids.iter().any(|id| id == tool_call_id))
            })
        })
}

fn append_ai_turn_tool_result(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    status: &str,
    result: &serde_json::Value,
) {
    let success = result
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(status == "completed");
    let output = result
        .get("output")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default());
    mutate_ai_turn_parts(message, |parts| {
        if let Some(existing) = parts.iter_mut().find(|part| {
            part.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
                && part
                    .get("toolCallId")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == id)
        }) && let Some(object) = existing.as_object_mut()
        {
            object.insert("toolName".to_string(), serde_json::json!(name));
            object.insert("success".to_string(), serde_json::json!(success));
            object.insert("output".to_string(), serde_json::json!(output));
            object.insert("envelope".to_string(), result.clone());
            return;
        }
        parts.push(serde_json::json!({
            "type": "tool_result",
            "toolCallId": id,
            "toolName": name,
            "success": success,
            "output": output,
            "envelope": result,
        }));
    });
}
