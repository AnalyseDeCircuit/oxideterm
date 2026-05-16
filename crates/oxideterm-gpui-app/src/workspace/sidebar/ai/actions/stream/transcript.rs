fn ai_transcript_boundary_id(message: Option<&AiChatMessage>, edge: &str) -> Option<String> {
    let message = message?;
    let transcript_ref = message.transcript_ref.as_ref();
    let primary = if edge == "start" {
        "startEntryId"
    } else {
        "endEntryId"
    };
    let fallback = if edge == "start" {
        "endEntryId"
    } else {
        "startEntryId"
    };
    transcript_ref
        .and_then(|value| value.get(primary))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            transcript_ref
                .and_then(|value| value.get(fallback))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_string)
        .or_else(|| Some(message.id.clone()))
}

fn ai_summary_source_transcript_ref(
    messages: &[AiChatMessage],
    conversation_id: &str,
) -> serde_json::Value {
    let start_entry_id = ai_transcript_boundary_id(messages.first(), "start");
    let end_entry_id = ai_transcript_boundary_id(messages.last(), "end");
    serde_json::json!({
        "conversationId": conversation_id,
        "startEntryId": start_entry_id,
        "endEntryId": end_entry_id,
    })
}

fn ai_find_prompt_transcript_lookup_reference(
    messages: &[AiChatMessage],
) -> Option<serde_json::Value> {
    messages.iter().rev().find_map(|message| {
        message
            .summary_ref
            .as_ref()
            .and_then(|summary_ref| summary_ref.get("transcriptRef"))
            .filter(|transcript_ref| !transcript_ref.is_null())
            .cloned()
    })
}

fn ai_build_transcript_lookup_prompt_reference(transcript_ref: serde_json::Value) -> String {
    let start_entry_id = transcript_ref
        .get("startEntryId")
        .and_then(serde_json::Value::as_str);
    let end_entry_id = transcript_ref
        .get("endEntryId")
        .and_then(serde_json::Value::as_str);
    let conversation_id = transcript_ref
        .get("conversationId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let mut range_parts = Vec::new();
    if let Some(start_entry_id) = start_entry_id {
        range_parts.push(format!("start={start_entry_id}"));
    }
    if let Some(end_entry_id) = end_entry_id {
        range_parts.push(format!("end={end_entry_id}"));
    }
    let range_text = if range_parts.is_empty() {
        "range=unknown".to_string()
    } else {
        range_parts.join(", ")
    };

    [
        "Earlier history is intentionally compacted out of this prompt.".to_string(),
        format!("Transcript reference: conversation={conversation_id}, {range_text}."),
        "Use the visible summary as the authoritative compressed context. Do not infer omitted details unless they are restated here or fetched through transcript lookup tooling.".to_string(),
    ]
    .join(" ")
}

fn ai_transcript_entry(
    id: String,
    conversation_id: &str,
    kind: &str,
    payload: serde_json::Value,
    turn_id: Option<String>,
    parent_id: Option<String>,
    timestamp: i64,
) -> oxideterm_ai::PersistedTranscriptEntry {
    oxideterm_ai::PersistedTranscriptEntry {
        id,
        conversation_id: conversation_id.to_string(),
        turn_id,
        parent_id,
        timestamp,
        kind: kind.to_string(),
        payload,
    }
}

fn ai_diagnostic_event(
    id: String,
    conversation_id: &str,
    event_type: &str,
    turn_id: Option<String>,
    round_id: Option<String>,
    timestamp: i64,
    data: serde_json::Value,
) -> oxideterm_ai::PersistedDiagnosticEvent {
    oxideterm_ai::PersistedDiagnosticEvent {
        id,
        conversation_id: conversation_id.to_string(),
        turn_id,
        round_id,
        timestamp,
        event_type: event_type.to_string(),
        data,
    }
}

