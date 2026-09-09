//! Behavioral coverage for the extracted stream-state responsibility.

use crate::{
    AiChatMessage, AiChatMessageMetadata, AiChatRole, AiConversation, AiToolDefinition,
    set_ai_provider_parts,
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
        original_messages: None,
        original_user_count: Some(7),
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
        original_messages: None,
        original_user_count: None,
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
        assert_eq!(turn["status"], "complete");
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
fn prompt_budget_uses_configured_safety_margin() {
    let budget = compute_ai_prompt_budget(1_000, 200, 100, Some(50));

    assert_eq!(budget.usable_prompt_budget, 750);
    assert_eq!(budget.history_budget, 650);
}

#[test]
fn prompt_breakdown_counts_every_provider_visible_component() {
    let mut assistant = message("assistant", AiChatRole::Assistant, "answer");
    assistant.thinking_content = Some("reasoning".to_string());
    assistant.tool_calls.push(serde_json::json!({
        "id": "call-1",
        "name": "run_command",
        "arguments": "{\"command\":\"pwd\"}"
    }));
    set_ai_provider_parts(
        &mut assistant,
        "gemini",
        vec![serde_json::json!({"thoughtSignature": "signed-state"})],
    );
    let mut tool = message("tool", AiChatRole::Tool, "tool output");
    tool.tool_call_id = Some("call-1".to_string());
    let tools = vec![AiToolDefinition {
        name: "run_command".to_string(),
        description: "Run a command".to_string(),
        parameters: serde_json::json!({"type": "object"}),
    }];

    let breakdown = ai_prompt_token_breakdown(
        &[
            message("system", AiChatRole::System, "instructions"),
            message("user", AiChatRole::User, "question"),
            assistant,
            tool,
        ],
        &tools,
        "gemini",
        512,
    );

    assert!(breakdown.system_instructions > 0);
    assert!(breakdown.messages > 0);
    assert!(breakdown.tool_results > ai_estimated_tokens("tool output"));
    assert_eq!(
        breakdown.tool_definitions,
        ai_tool_definitions_estimated_tokens(&tools)
    );
    assert_eq!(breakdown.reserved_output, 512);
    assert_eq!(
        breakdown.total(),
        breakdown.prompt_tokens() + breakdown.reserved_output
    );
}

#[test]
fn gemini_provider_parts_replace_the_visible_assistant_projection() {
    let provider_parts = vec![serde_json::json!({
        "text": "provider-native answer",
        "thoughtSignature": "signed-state"
    })];
    let expected_provider_tokens = ai_estimated_tokens(
        &serde_json::to_string(&provider_parts).expect("provider parts should serialize"),
    );
    let mut assistant = message(
        "assistant",
        AiChatRole::Assistant,
        "this visible projection must not also be counted",
    );
    set_ai_provider_parts(&mut assistant, "gemini", provider_parts);

    let breakdown = ai_prompt_token_breakdown(&[assistant], &[], "gemini", 0);

    assert_eq!(breakdown.messages, 0);
    assert_eq!(breakdown.tool_results, expected_provider_tokens);
    assert_eq!(breakdown.prompt_tokens(), expected_provider_tokens);
}

#[test]
fn history_trimming_reserves_fixed_tool_schema_overhead() {
    let mut without_overhead = vec![
        message("system", AiChatRole::System, "system"),
        message("user-1", AiChatRole::User, &"a".repeat(800)),
        message("assistant-1", AiChatRole::Assistant, &"b".repeat(800)),
        message("user-2", AiChatRole::User, "latest"),
    ];
    let mut with_overhead = without_overhead.clone();

    let baseline = trim_ai_stream_history_to_budget(&mut without_overhead, 1_000, 100);
    let reserved =
        trim_ai_stream_history_to_budget_with_overhead(&mut with_overhead, 1_000, 100, 450);

    assert!(reserved > baseline);
    assert_eq!(
        with_overhead.last().map(|message| message.id.as_str()),
        Some("user-2")
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
