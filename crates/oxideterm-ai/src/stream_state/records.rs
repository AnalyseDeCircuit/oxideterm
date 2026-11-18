//! Records emitted while applying a conversation stream.

pub fn ai_transcript_entry(
    id: String,
    conversation_id: &str,
    kind: &str,
    payload: serde_json::Value,
    turn_id: Option<String>,
    parent_id: Option<String>,
    timestamp: i64,
) -> crate::PersistedTranscriptEntry {
    crate::PersistedTranscriptEntry {
        id,
        conversation_id: conversation_id.to_string(),
        turn_id,
        parent_id,
        timestamp,
        kind: kind.to_string(),
        payload,
    }
}

pub fn ai_diagnostic_event(
    id: String,
    conversation_id: &str,
    event_type: &str,
    turn_id: Option<String>,
    round_id: Option<String>,
    timestamp: i64,
    data: serde_json::Value,
) -> crate::PersistedDiagnosticEvent {
    crate::PersistedDiagnosticEvent {
        id,
        conversation_id: conversation_id.to_string(),
        turn_id,
        round_id,
        timestamp,
        event_type: event_type.to_string(),
        data,
    }
}
