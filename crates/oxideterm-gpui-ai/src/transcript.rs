use oxideterm_ai::AiChatMessage;

pub fn ai_transcript_boundary_id(message: Option<&AiChatMessage>, edge: &str) -> Option<String> {
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

pub fn ai_summary_source_transcript_ref(
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
