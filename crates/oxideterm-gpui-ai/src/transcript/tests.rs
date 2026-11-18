use super::*;
use crate::sidebar::transcript::ai_message_is_awaiting_tool_summary;
use oxideterm_ai::{
    AiChatMessageMetadata, AiChatRole,
    stream_state::{
        ai_build_transcript_lookup_prompt_reference, ai_find_prompt_transcript_lookup_reference,
        normalize_ai_stream_history_for_provider, set_ai_turn_round_stateful_marker,
        upsert_ai_turn_round_tool_call,
    },
};

fn assistant_message() -> AiChatMessage {
    AiChatMessage {
        is_streaming: true,
        ..test_message("assistant-1", AiChatRole::Assistant, "".to_string())
    }
}

fn test_message(id: &str, role: AiChatRole, content: String) -> AiChatMessage {
    AiChatMessage {
        id: id.to_string(),
        role,
        content,
        timestamp_ms: 1,
        model: None,
        context: None,
        is_streaming: false,
        thinking_content: None,
        metadata: None,
        tool_call_id: None,
        tool_calls: Vec::new(),
        turn: None,
        transcript_ref: None,
        summary_ref: None,
        branches: None,
        suggestions: Vec::new(),
    }
}

#[test]
fn summary_references_survive_provider_history_normalization() {
    let compacted = vec![
        test_message("u-1", AiChatRole::User, "first".to_string()),
        test_message("a-1", AiChatRole::Assistant, "answer".to_string()),
        test_message("u-2", AiChatRole::User, "second".to_string()),
        test_message("a-2", AiChatRole::Assistant, "answer".to_string()),
    ];
    let source_ref = ai_summary_source_transcript_ref(&compacted, "conv-1");
    assert_eq!(
        source_ref
            .get("startEntryId")
            .and_then(serde_json::Value::as_str),
        Some("u-1")
    );
    assert_eq!(
        source_ref
            .get("endEntryId")
            .and_then(serde_json::Value::as_str),
        Some("a-2")
    );

    let anchor = AiChatMessage {
        metadata: Some(AiChatMessageMetadata {
            kind: "compaction-anchor".to_string(),
            original_count: Some(compacted.len()),
            compacted_at_ms: Some(1),
            original_ref: None,
            original_messages: Some(compacted),
            original_user_count: Some(2),
            images: Vec::new(),
        }),
        transcript_ref: Some(serde_json::json!({
            "conversationId": "conv-1",
            "endEntryId": "anchor-1",
        })),
        summary_ref: Some(serde_json::json!({
            "kind": "compaction",
            "transcriptRef": source_ref,
        })),
        ..test_message("anchor-1", AiChatRole::System, "summary".to_string())
    };
    let mut summary = test_message("summary-1", AiChatRole::Assistant, "summary".to_string());
    summary.transcript_ref = Some(serde_json::json!({
        "conversationId": "conv-1",
        "endEntryId": "transcript-summary-created-summary-1",
    }));
    summary.summary_ref = Some(serde_json::json!({
        "kind": "conversation",
        "roundId": null,
        "transcriptRef": source_ref,
    }));
    for (scenario, message, expected_content) in [
        (
            "compaction",
            anchor,
            "Previous conversation summary:\nsummary",
        ),
        ("conversation", summary, "summary"),
    ] {
        let mut history = vec![message];
        normalize_ai_stream_history_for_provider(&mut history);
        assert_eq!(history[0].content, expected_content, "{scenario}");
        let lookup_ref = ai_find_prompt_transcript_lookup_reference(&history)
            .expect("summary transcript reference");
        assert_eq!(lookup_ref, source_ref, "{scenario}");
        let lookup_prompt = ai_build_transcript_lookup_prompt_reference(lookup_ref);
        assert!(lookup_prompt.contains("conversation=conv-1"), "{scenario}");
        assert!(lookup_prompt.contains("start=u-1"), "{scenario}");
        assert!(lookup_prompt.contains("end=a-2"), "{scenario}");
    }
}

#[test]
fn awaiting_summary_indicator_is_hidden_for_historical_messages() {
    let mut message = assistant_message();
    upsert_ai_turn_round_tool_call(
        &mut message,
        "call-1",
        "run_command",
        "{}",
        "completed",
        "assistant-1-round-1",
        1,
    );
    set_ai_turn_round_stateful_marker(
        &mut message,
        "assistant-1-round-1",
        Some("awaiting-summary"),
    );

    assert!(ai_message_is_awaiting_tool_summary(&message));

    message.is_streaming = false;
    assert!(
        !ai_message_is_awaiting_tool_summary(&message),
        "persisted markers must not make completed messages look active"
    );
}
