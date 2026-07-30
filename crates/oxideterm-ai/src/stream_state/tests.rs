//! Behavioral coverage for the extracted stream-state responsibility.

use crate::{AiChatMessage, AiChatMessageMetadata, AiChatRole, AiConversation};

use super::*;

fn message(id: &str, role: AiChatRole, content: &str) -> AiChatMessage {
    AiChatMessage {
        id: id.to_string(),
        role,
        content: content.to_string(),
        timestamp_ms: 0,
        model: None,
        context: None,
        thinking_content: None,
        is_streaming: false,
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
fn provider_history_keeps_runtime_system_messages_and_plain_assistant_text() {
    let runtime = message("task-mode", AiChatRole::System, "Task mode");
    let mut assistant = message("assistant", AiChatRole::Assistant, "Done");
    assistant
        .tool_calls
        .push(serde_json::json!({"id": "call-1"}));
    let mut history = vec![
        message("other-system", AiChatRole::System, "drop"),
        runtime,
        assistant,
        message("tool", AiChatRole::Tool, "drop"),
    ];

    normalize_ai_stream_history_for_provider(&mut history);

    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, "task-mode");
    assert!(history[1].tool_calls.is_empty());
}

#[test]
fn cancellation_rejects_pending_calls_and_retains_meaningful_turn() {
    let mut assistant = message("assistant", AiChatRole::Assistant, "partial");
    assistant.is_streaming = true;
    assistant.tool_calls.push(serde_json::json!({
        "id": "call-1",
        "name": "run_command",
        "arguments": "{}",
        "status": "pending"
    }));
    let mut conversation = AiConversation {
        id: "conversation".to_string(),
        title: "Conversation".to_string(),
        messages: vec![assistant],
        created_at_ms: 0,
        updated_at_ms: 0,
        origin: "test".to_string(),
        profile_id: None,
        message_count: 1,
        session_id: None,
        session_metadata: None,
        messages_loaded: true,
    };

    let stopped = finalize_streaming_ai_messages_on_cancel(&mut conversation);

    assert_eq!(stopped.len(), 1);
    assert!(stopped[0].retained);
    assert_eq!(conversation.messages[0].tool_calls[0]["status"], "rejected");
}

#[test]
fn prompt_budget_uses_configured_safety_margin() {
    let budget = compute_ai_prompt_budget(1_000, 200, 100, Some(50));

    assert_eq!(budget.usable_prompt_budget, 750);
    assert_eq!(budget.history_budget, 650);
}

#[test]
fn compaction_plan_preserves_recent_messages() {
    let messages = (0..6)
        .map(|index| {
            message(
                &format!("message-{index}"),
                if index % 2 == 0 {
                    AiChatRole::User
                } else {
                    AiChatRole::Assistant
                },
                &"x".repeat(1_000),
            )
        })
        .collect::<Vec<_>>();

    let plan = ai_compaction_plan(&messages, 2_000, true).expect("compaction plan");

    assert!(plan.compact_messages.len() >= 2);
    assert_eq!(plan.keep_messages.last(), messages.last());
}

#[test]
fn compaction_snapshot_removes_runtime_only_message_state() {
    let mut source = message("assistant", AiChatRole::Assistant, "answer");
    source.model = Some("model".to_string());
    source.is_streaming = true;
    source.tool_calls.push(serde_json::json!({"id": "call-1"}));

    let snapshot = ai_compaction_anchor_snapshot(&[source]);

    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].model, None);
    assert!(!snapshot[0].is_streaming);
    assert!(snapshot[0].tool_calls.is_empty());
}

#[test]
fn compaction_anchor_normalizes_to_provider_summary() {
    let mut anchor = message("anchor", AiChatRole::System, "summary");
    anchor.metadata = Some(AiChatMessageMetadata {
        kind: "compaction-anchor".to_string(),
        original_count: Some(2),
        compacted_at_ms: Some(1),
        original_messages: None,
    });
    let mut history = vec![anchor];

    normalize_ai_stream_history_for_provider(&mut history);

    assert_eq!(
        history[0].content,
        "Previous conversation summary:\nsummary"
    );
    assert_eq!(history[0].metadata, None);
}

#[test]
fn compaction_and_provider_history_scrub_runtime_handles() {
    let handle = "rt_0123456789abcdef0123456789abcdef";
    let source = message(
        "assistant",
        AiChatRole::Assistant,
        &format!("Earlier authority was {handle}."),
    );

    let summary_messages = ai_compaction_summary_messages(std::slice::from_ref(&source));
    assert!(!summary_messages[1].content.contains(handle));
    let snapshot = ai_compaction_anchor_snapshot(std::slice::from_ref(&source));
    assert!(!snapshot[0].content.contains(handle));

    let mut provider_history = vec![source];
    normalize_ai_stream_history_for_provider(&mut provider_history);
    assert!(!provider_history[0].content.contains(handle));
}

#[test]
fn turn_status_initializes_structured_turn_state() {
    let mut assistant = message("assistant", AiChatRole::Assistant, "answer");

    set_ai_turn_status(&mut assistant, "complete");

    let turn = assistant.turn.expect("turn state");
    assert_eq!(turn["id"], "assistant");
    assert_eq!(turn["status"], "complete");
    assert_eq!(turn["plainTextSummary"], "answer");
}

#[test]
fn tool_status_updates_legacy_and_structured_turn_views() {
    let mut assistant = message("assistant", AiChatRole::Assistant, "");

    update_ai_tool_call_status(
        &mut assistant,
        "call-1",
        "run_command",
        "{}",
        "completed",
        Some(serde_json::json!({"ok": true})),
        None,
        Some("done".to_string()),
        None,
        None,
    );

    assert_eq!(assistant.tool_calls[0]["status"], "completed");
    assert_eq!(assistant.tool_calls[0]["summary"], "done");
    let (round_id, _) =
        ai_turn_round_for_existing_tool_call(&assistant, "call-1").expect("tool round");
    assert!(ai_turn_round_has_result(&assistant, &round_id));
}
