//! Behavioral coverage for the extracted stream-state responsibility.

use crate::{
    AiChatMessage, AiChatMessageMetadata, AiChatRole, AiConversation, set_ai_provider_parts,
};

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
fn conversation_turn_count_preserves_submissions_across_live_compacted_and_summarized_history() {
    let live = vec![
        message("user-1", AiChatRole::User, "question"),
        message("assistant-tool", AiChatRole::Assistant, "working"),
        message("tool-1", AiChatRole::Tool, "result"),
        message("assistant-1", AiChatRole::Assistant, "answer"),
        message("user-2", AiChatRole::User, "follow-up"),
    ];
    let mut anchor = message("anchor", AiChatRole::System, "summary");
    anchor.metadata = Some(AiChatMessageMetadata {
        kind: "compaction-anchor".to_string(),
        original_count: Some(14),
        compacted_at_ms: Some(1),
        original_ref: None,
        original_messages: None,
        original_user_count: Some(7),
        images: Vec::new(),
    });
    let compacted = vec![
        anchor,
        message("user-8", AiChatRole::User, "continue"),
        message("assistant-8", AiChatRole::Assistant, "done"),
    ];
    let mut summary = message("summary", AiChatRole::Assistant, "summary");
    summary.summary_ref =
        Some(serde_json::json!({ "kind": "conversation", "originalUserCount": 9 }));
    for (messages, expected) in [(live, 2), (compacted, 8), (vec![summary], 9)] {
        assert_eq!(crate::ai_conversation_turn_count(&messages), expected);
    }
}

#[test]
fn provider_history_preserves_text_and_summary_order_without_replaying_old_tool_state() {
    let mut assistant = message("assistant", AiChatRole::Assistant, "本地终端已重新打开。");
    assistant.thinking_content = Some("need a terminal".to_string());
    assistant.tool_calls.push(serde_json::json!({
        "id": "call-1", "name": "open_app_surface",
        "arguments": "{\"surface\":\"local_terminal\"}", "status": "completed",
        "result": { "ok": true, "output": "opened" },
    }));
    let mut tool_only = assistant.clone();
    tool_only.id = "tool-only".to_string();
    tool_only.content.clear();
    let mut anchor = message("anchor", AiChatRole::System, " 用户之前打开过本地终端。 ");
    anchor.metadata = Some(AiChatMessageMetadata {
        kind: "compaction-anchor".to_string(),
        original_count: Some(4),
        compacted_at_ms: Some(1),
        original_ref: None,
        original_messages: None,
        original_user_count: None,
        images: Vec::new(),
    });
    let mut history = vec![
        message("task-mode", AiChatRole::System, "Task instructions"),
        message("stale-system", AiChatRole::System, "drop"),
        anchor,
        message("user", AiChatRole::User, "打开终端"),
        assistant,
        message("tool", AiChatRole::Tool, "{\"ok\":true}"),
        tool_only,
    ];
    normalize_ai_stream_history_for_provider(&mut history);
    assert_eq!(
        history
            .iter()
            .map(|row| (row.id.as_str(), row.role, row.content.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("task-mode", AiChatRole::System, "Task instructions"),
            (
                "anchor",
                AiChatRole::System,
                "Previous conversation summary:\n 用户之前打开过本地终端。 "
            ),
            ("user", AiChatRole::User, "打开终端"),
            ("assistant", AiChatRole::Assistant, "本地终端已重新打开。"),
        ]
    );
    assert!(history[1].metadata.is_none());
    assert!(history[3].tool_calls.is_empty());
    assert!(history[3].thinking_content.is_none());
}

#[test]
fn cancellation_retains_partial_text_and_completes_rejected_tool_results() {
    for (content, status) in [("partial", "pending"), ("", "pending_user_approval")] {
        let mut assistant = message("assistant", AiChatRole::Assistant, content);
        assistant.is_streaming = true;
        assistant.tool_calls.push(serde_json::json!({
            "id": "call-1", "name": "open_app_surface", "arguments": "{}",
            "status": status, "result": null,
        }));
        let mut conversation = AiConversation {
            archived: false,
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
            turn_count: 0,
        };
        let stopped = finalize_streaming_ai_messages_on_cancel(&mut conversation);
        assert_eq!(
            stopped,
            vec![AiStoppedAssistantTurn {
                message_id: "assistant".to_string(),
                status: "complete",
                retained: true,
            }]
        );
        let message = &conversation.messages[0];
        assert_eq!(message.content, content);
        assert!(!message.is_streaming);
        let call = &message.tool_calls[0];
        assert_eq!(call["status"], "rejected");
        assert_eq!(call["result"]["ok"], false);
        assert_eq!(
            call["result"]["error"]["message"],
            "Generation was stopped."
        );
        let turn = message.turn.as_ref().unwrap();
        assert_eq!(turn["id"], "assistant");
        assert_eq!(turn["status"], "complete");
        assert_eq!(turn["plainTextSummary"], content);
        assert!(
            turn["parts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|part| part["type"] == "tool_result" && part["toolCallId"] == "call-1")
        );
    }
}

#[test]
fn history_trimming_reserves_tool_overhead_and_keeps_latest_when_exhausted() {
    let mut without_overhead = vec![
        message("system", AiChatRole::System, "system"),
        message("user-1", AiChatRole::User, &"a".repeat(800)),
        message("assistant-1", AiChatRole::Assistant, &"b".repeat(800)),
        message("user-2", AiChatRole::User, "latest"),
    ];
    let mut with_overhead = without_overhead.clone();
    let mut exhausted_budget = without_overhead.clone();

    let baseline = trim_ai_stream_history_to_budget(&mut without_overhead, 1_000, 100);
    let reserved =
        trim_ai_stream_history_to_budget_with_overhead(&mut with_overhead, 1_000, 100, 450);

    assert_eq!(baseline, 0);
    assert_eq!(reserved, 2);
    assert_eq!(
        with_overhead.last().map(|message| message.id.as_str()),
        Some("user-2")
    );
    assert_eq!(
        trim_ai_stream_history_to_budget(&mut exhausted_budget, 100, 100),
        2
    );
    assert_eq!(
        exhausted_budget
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        ["system", "user-2"]
    );
}

#[test]
fn request_history_trimming_counts_provider_protocol_state() {
    let mut assistant = message("assistant", AiChatRole::Assistant, "answer");
    set_ai_provider_parts(
        &mut assistant,
        "gemini",
        vec![serde_json::json!({"thoughtSignature": "x".repeat(4_000)})],
    );
    let history = vec![assistant, message("latest", AiChatRole::User, "continue")];
    let mut content_only = history.clone();
    let mut request_aware = history;

    let content_only_trimmed = trim_ai_stream_history_to_budget(&mut content_only, 1_000, 100);
    let request_aware_trimmed =
        trim_ai_stream_history_to_request_budget(&mut request_aware, &[], "gemini", 1_000, 100);

    assert_eq!(content_only_trimmed, 0);
    assert_eq!(request_aware_trimmed, 1);
    assert_eq!(request_aware[0].id, "latest");
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
    assert_eq!(
        [plan.compact_messages, plan.keep_messages].concat(),
        messages
    );
    let short = (0..4)
        .map(|index| message(&format!("short-{index}"), AiChatRole::User, "short"))
        .collect::<Vec<_>>();
    assert!(ai_compaction_plan(&short, 100_000, true).is_none());
}

#[test]
fn compaction_and_provider_history_scrub_runtime_handles() {
    let handle = "rt_0123456789abcdef0123456789abcdef";
    let mut source = message(
        "assistant",
        AiChatRole::Assistant,
        &format!("Earlier authority was {handle}."),
    );
    source.model = Some("model".to_string());
    source.is_streaming = true;
    source.tool_calls.push(serde_json::json!({"id": "call-1"}));

    let summary_messages = ai_compaction_summary_messages(std::slice::from_ref(&source));
    assert!(!summary_messages[1].content.contains(handle));
    let snapshot = ai_compaction_anchor_snapshot(std::slice::from_ref(&source));
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].model, None);
    assert!(!snapshot[0].is_streaming);
    assert!(snapshot[0].tool_calls.is_empty());
    assert!(!snapshot[0].content.contains(handle));

    let mut provider_history = vec![source];
    normalize_ai_stream_history_for_provider(&mut provider_history);
    assert!(!provider_history[0].content.contains(handle));
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
