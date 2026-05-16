#[cfg(test)]
mod ai_turn_order_tests {
    use super::*;

    fn assistant_message() -> AiChatMessage {
        AiChatMessage {
            id: "assistant-1".to_string(),
            role: AiChatRole::Assistant,
            content: String::new(),
            timestamp_ms: 1,
            model: None,
            context: None,
            is_streaming: true,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
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
        }
    }

    #[test]
    fn history_trimming_uses_tauri_history_budget_ratio() {
        let cjk_100 = "你".repeat(100);
        let mut history = vec![
            test_message("system", AiChatRole::System, cjk_100.clone()),
            test_message("user-1", AiChatRole::User, cjk_100.clone()),
            test_message("assistant-1", AiChatRole::Assistant, cjk_100.clone()),
            test_message("user-2", AiChatRole::User, cjk_100),
        ];

        let trimmed = trim_ai_stream_history_to_budget(&mut history, 1000, 150);

        assert_eq!(trimmed, 1);
        assert_eq!(
            history.iter().map(|message| message.id.as_str()).collect::<Vec<_>>(),
            vec!["system", "assistant-1", "user-2"]
        );
    }

    #[test]
    fn prompt_budget_policy_matches_tauri_levels() {
        let decision = determine_ai_compression_level(AiPromptBudgetInput {
            context_window: 1000,
            response_reserve: 150,
            system_budget: 50,
            history_tokens: 630,
            safety_margin: Some(0),
            trimmable_history_tokens: Some(630),
            summary_eligible_tokens: Some(630),
            can_summarize: true,
            can_lookup_transcript: false,
            in_tool_loop: false,
            auto_compact_threshold: Some(0.80),
            transcript_lookup_threshold: None,
            tool_loop_stop_threshold: None,
        });

        assert_eq!(decision.level, 2);

        let tool_loop_stop = determine_ai_compression_level(AiPromptBudgetInput {
            context_window: 1000,
            response_reserve: 100,
            system_budget: 0,
            history_tokens: 890,
            safety_margin: Some(0),
            trimmable_history_tokens: Some(0),
            summary_eligible_tokens: Some(0),
            can_summarize: false,
            can_lookup_transcript: false,
            in_tool_loop: true,
            auto_compact_threshold: None,
            transcript_lookup_threshold: None,
            tool_loop_stop_threshold: Some(0.98),
        });

        assert_eq!(tool_loop_stop.level, 4);
    }

    #[test]
    fn compaction_reference_survives_provider_history_normalization() {
        let compacted = vec![
            test_message("u-1", AiChatRole::User, "first".to_string()),
            test_message("a-1", AiChatRole::Assistant, "answer".to_string()),
            test_message("u-2", AiChatRole::User, "second".to_string()),
            test_message("a-2", AiChatRole::Assistant, "answer".to_string()),
        ];
        let source_ref = ai_summary_source_transcript_ref(&compacted, "conv-1");
        assert_eq!(
            source_ref.get("startEntryId").and_then(serde_json::Value::as_str),
            Some("u-1")
        );
        assert_eq!(
            source_ref.get("endEntryId").and_then(serde_json::Value::as_str),
            Some("a-2")
        );

        let mut history = vec![AiChatMessage {
            id: "anchor-1".to_string(),
            role: AiChatRole::System,
            content: "summary".to_string(),
            timestamp_ms: 1,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: Some(AiChatMessageMetadata {
                kind: "compaction-anchor".to_string(),
                original_count: Some(compacted.len()),
                compacted_at_ms: Some(1),
                original_messages: Some(compacted),
            }),
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: Some(serde_json::json!({
                "conversationId": "conv-1",
                "endEntryId": "anchor-1",
            })),
            summary_ref: Some(serde_json::json!({
                "kind": "compaction",
                "transcriptRef": source_ref,
            })),
            branches: None,
        }];

        normalize_ai_stream_history_for_provider(&mut history);
        let lookup_ref = ai_find_prompt_transcript_lookup_reference(&history)
            .expect("compaction transcript lookup reference");
        let lookup_prompt = ai_build_transcript_lookup_prompt_reference(lookup_ref);

        assert_eq!(history[0].content, "Previous conversation summary:\nsummary");
        assert!(lookup_prompt.contains("conversation=conv-1"));
        assert!(lookup_prompt.contains("start=u-1"));
        assert!(lookup_prompt.contains("end=a-2"));
    }

    #[test]
    fn old_tool_messages_are_condensed_like_tauri_tool_loop() {
        let mut history = (0..7)
            .map(|index| AiChatMessage {
                id: format!("tool-{index}"),
                role: AiChatRole::Tool,
                content: serde_json::json!({
                    "ok": true,
                    "output": format!("line 1\nline 2\nline 3\nline 4\nline 5 for {index}"),
                    "meta": { "toolName": "read_resource" },
                })
                .to_string(),
                timestamp_ms: index,
                model: None,
                context: None,
                is_streaming: false,
                thinking_content: None,
                metadata: None,
                tool_call_id: Some(format!("call-{index}")),
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            })
            .collect::<Vec<_>>();

        condense_ai_tool_messages(&mut history);

        assert!(history[0].content.starts_with("[condensed] read_resource -> ok:"));
        assert!(history[1].content.starts_with("[condensed] read_resource -> ok:"));
        assert!(!history[2].content.starts_with("[condensed]"));
    }

    #[test]
    fn guardrail_parts_are_structured_like_tauri_turn_model() {
        let mut message = assistant_message();

        append_ai_turn_guardrail_part(
            &mut message,
            "tool-budget-limit",
            "Tool use stopped.",
            Some("raw candidate text"),
        );

        let parts = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(parts[0]["type"], "guardrail");
        assert_eq!(parts[0]["code"], "tool-budget-limit");
        assert_eq!(parts[0]["message"], "Tool use stopped.");
        assert_eq!(parts[0]["rawText"], "raw candidate text");
    }

    #[test]
    fn pending_round_summary_attaches_when_round_arrives() {
        let mut message = assistant_message();

        upsert_ai_round_summary(
            &mut message,
            "assistant-1-round-1",
            "read_resource: ok - inspected config",
            serde_json::json!({
                "source": "background",
                "summarizationMode": "background",
                "contextLengthBefore": 128,
            }),
        );

        assert_eq!(
            message
                .turn
                .as_ref()
                .and_then(|turn| turn.get("pendingSummaries"))
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(1),
        );

        upsert_ai_turn_round_tool_call(
            &mut message,
            "call-1",
            "read_resource",
            "{}",
            "completed",
            "assistant-1-round-1",
            1,
        );

        let turn = message.turn.as_ref().expect("turn");
        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("rounds");
        assert_eq!(rounds[0]["summary"], "read_resource: ok - inspected config");
        assert_eq!(
            rounds[0]["summaryMetadata"]["contextLengthBefore"],
            serde_json::json!(128)
        );
        assert_eq!(
            turn.get("pendingSummaries")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(0),
        );
    }

    #[test]
    fn round_summary_updates_existing_round_without_pending_tail() {
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
        upsert_ai_round_summary(
            &mut message,
            "assistant-1-round-1",
            "run_command: ok - printed working directory",
            serde_json::json!({ "model": "deepseek-v4-pro" }),
        );

        let turn = message.turn.as_ref().expect("turn");
        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("rounds");
        assert_eq!(
            rounds[0]["summary"],
            "run_command: ok - printed working directory"
        );
        assert_eq!(rounds[0]["summaryMetadata"]["model"], "deepseek-v4-pro");
        assert_eq!(
            turn.get("pendingSummaries")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(0),
        );
    }

    #[test]
    fn round_stateful_marker_matches_tauri_awaiting_summary_lifecycle() {
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

        let turn = message.turn.as_ref().expect("turn");
        let round = &turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("rounds")[0];
        assert_eq!(round["statefulMarker"], "awaiting-summary");

        set_ai_turn_round_stateful_marker(&mut message, "assistant-1-round-1", None);
        let round = &message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("toolRounds"))
            .and_then(serde_json::Value::as_array)
            .expect("rounds")[0];
        assert!(round.get("statefulMarker").is_none());
    }

    #[test]
    fn turn_plain_text_summary_uses_text_parts_like_tauri_turn_end() {
        let mut message = assistant_message();

        append_ai_turn_text_part(&mut message, "thinking", "hidden reasoning", false);
        append_ai_turn_text_part(&mut message, "text", "visible ", false);
        append_ai_turn_tool_result(
            &mut message,
            "call-1",
            "run_command",
            "completed",
            &serde_json::json!({ "ok": true, "output": "tool output" }),
        );
        append_ai_turn_text_part(&mut message, "text", "answer", false);

        assert_eq!(
            ai_turn_plain_text_summary(&message).as_deref(),
            Some("visible answer")
        );
    }

    #[test]
    fn synthetic_denied_tool_status_uses_retry_round_override() {
        let mut message = assistant_message();

        update_ai_tool_call_status(
            &mut message,
            "assistant-1-hard-deny-1-tool",
            "tool_use_disabled",
            r#"{"reason":"tool_use_disabled","retryAttempt":1}"#,
            "rejected",
            Some(serde_json::json!({
                "ok": false,
                "output": "",
                "error": { "message": "Tool use is disabled." },
            })),
            Some("write".to_string()),
            Some("Tool use is disabled.".to_string()),
            Some("assistant-1-hard-deny-1"),
            Some(1),
        );

        let rounds = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("toolRounds"))
            .and_then(serde_json::Value::as_array)
            .expect("tool rounds");
        assert_eq!(rounds[0]["id"], "assistant-1-hard-deny-1");
        assert_eq!(rounds[0]["toolCalls"][0]["approvalState"], "rejected");
    }

    #[test]
    fn required_tool_obligation_retries_action_claims() {
        let obligation = ai_classify_orchestrator_obligation("打开本地终端");

        assert_eq!(obligation.mode, AiOrchestratorObligationMode::Required);
        assert!(obligation.candidate_tools.iter().any(|tool| tool == "open_app_surface"));
        assert!(ai_orchestrator_obligation_prompt(&obligation)
            .expect("prompt")
            .contains("Required Tool Call"));
        assert!(ai_should_retry_required_tool_round(
            &obligation,
            "我已经打开了本地终端。"
        ));
        assert!(!ai_should_retry_required_tool_round(
            &obligation,
            "需要你确认打开哪一个终端？"
        ));
    }

    #[test]
    fn pseudo_tool_json_hard_deny_respects_json_requests() {
        let pseudo = r#"{"name":"run_command","arguments":{"command":"ls"},"status":"ok"}"#;

        assert!(ai_should_trigger_hard_deny(pseudo, false));
        assert!(!ai_should_trigger_hard_deny(pseudo, true));
        assert!(!ai_should_trigger_hard_deny("正常回答", false));
    }

    #[test]
    fn turn_parts_keep_tool_call_before_later_text() {
        let mut message = assistant_message();
        upsert_ai_tool_call(&mut message, "call-1", "open_app_surface", "{}", "pending");
        upsert_ai_turn_tool_call(&mut message, "call-1", "open_app_surface", "{}", "complete");
        append_ai_turn_tool_result(
            &mut message,
            "call-1",
            "open_app_surface",
            "completed",
            &serde_json::json!({ "ok": true, "output": "opened" }),
        );
        message.content.push_str("Terminal opened.");
        append_ai_turn_text_part(&mut message, "text", "Terminal opened.", false);

        let parts = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(parts[0]["type"], "tool_call");
        assert_eq!(parts[1]["type"], "tool_result");
        assert_eq!(parts[2]["type"], "text");
        assert_eq!(message.tool_calls.len(), 1);
    }

    #[test]
    fn turn_parts_split_completed_tool_loops_into_distinct_rounds() {
        let mut message = assistant_message();
        upsert_ai_turn_tool_call(&mut message, "call-1", "open_app_surface", "{}", "complete");
        append_ai_turn_tool_result(
            &mut message,
            "call-1",
            "open_app_surface",
            "completed",
            &serde_json::json!({ "ok": true, "output": "opened" }),
        );
        upsert_ai_turn_tool_call(&mut message, "call-2", "get_state", "{}", "complete");
        append_ai_turn_tool_result(
            &mut message,
            "call-2",
            "get_state",
            "completed",
            &serde_json::json!({ "ok": true, "output": "ready" }),
        );

        let turn = message.turn.as_ref().expect("turn");
        let parts = turn
            .get("parts")
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(parts[0]["type"], "tool_call");
        assert_eq!(parts[1]["type"], "tool_result");
        assert_eq!(parts[2]["type"], "tool_call");
        assert_eq!(parts[3]["type"], "tool_result");

        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("tool rounds");
        assert_eq!(rounds.len(), 2);
        assert_eq!(rounds[0]["toolCalls"][0]["id"], "call-1");
        assert_eq!(rounds[1]["toolCalls"][0]["id"], "call-2");
        let first_round = ai_tool_part_round_id(&message, &parts[0]).expect("first round");
        let second_round = ai_tool_part_round_id(&message, &parts[2]).expect("second round");
        assert_ne!(first_round, second_round);
    }

    #[test]
    fn turn_parts_keep_parallel_tool_calls_in_one_round_until_results_arrive() {
        let mut message = assistant_message();
        upsert_ai_turn_tool_call(&mut message, "call-1", "read_resource", "{}", "complete");
        upsert_ai_turn_tool_call(&mut message, "call-2", "get_state", "{}", "complete");
        append_ai_turn_tool_result(
            &mut message,
            "call-1",
            "read_resource",
            "completed",
            &serde_json::json!({ "ok": true, "output": "file" }),
        );
        append_ai_turn_tool_result(
            &mut message,
            "call-2",
            "get_state",
            "completed",
            &serde_json::json!({ "ok": true, "output": "state" }),
        );

        let turn = message.turn.as_ref().expect("turn");
        let parts = turn
            .get("parts")
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(
            parts
                .iter()
                .filter(|part| part.get("type").and_then(serde_json::Value::as_str)
                    == Some("tool_call"))
                .count(),
            2
        );

        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("tool rounds");
        assert_eq!(rounds.len(), 1);
        assert_eq!(
            rounds[0]
                .get("toolCalls")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        let first_round = ai_tool_part_round_id(&message, &parts[0]).expect("first round");
        let second_round = ai_tool_part_round_id(&message, &parts[1]).expect("second round");
        assert_eq!(first_round, second_round);
    }

    #[test]
    fn provider_history_replays_legacy_tool_turns_as_plain_assistant_text() {
        let mut history = vec![
            AiChatMessage {
                id: "user-1".to_string(),
                role: AiChatRole::User,
                content: "打开终端".to_string(),
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
            },
            AiChatMessage {
                id: "assistant-1".to_string(),
                role: AiChatRole::Assistant,
                content: "本地终端已重新打开。".to_string(),
                timestamp_ms: 2,
                model: None,
                context: None,
                is_streaming: false,
                thinking_content: Some("need a terminal".to_string()),
                metadata: None,
                tool_call_id: None,
                tool_calls: vec![serde_json::json!({
                    "id": "call-1",
                    "name": "open_app_surface",
                    "arguments": "{\"surface\":\"local_terminal\"}",
                    "status": "completed",
                    "result": {
                        "ok": true,
                        "output": "opened",
                        "meta": { "toolName": "open_app_surface" }
                    }
                })],
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            },
            AiChatMessage {
                id: "tool-result-call-1".to_string(),
                role: AiChatRole::Tool,
                content: "{\"ok\":true}".to_string(),
                timestamp_ms: 3,
                model: None,
                context: None,
                is_streaming: false,
                thinking_content: None,
                metadata: None,
                tool_call_id: Some("call-1".to_string()),
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            },
        ];

        normalize_ai_stream_history_for_provider(&mut history);

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, AiChatRole::User);
        assert_eq!(history[1].role, AiChatRole::Assistant);
        assert_eq!(history[1].content, "本地终端已重新打开。");
        assert!(history[1].tool_calls.is_empty());
        assert!(history[1].thinking_content.is_none());
    }

    #[test]
    fn provider_history_drops_empty_tool_only_assistant_messages() {
        let mut history = vec![AiChatMessage {
            id: "assistant-tool-only".to_string(),
            role: AiChatRole::Assistant,
            content: String::new(),
            timestamp_ms: 1,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: vec![serde_json::json!({
                "id": "call-1",
                "name": "open_app_surface",
                "arguments": "{}"
            })],
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
        }];

        normalize_ai_stream_history_for_provider(&mut history);

        assert!(history.is_empty());
    }

    #[test]
    fn provider_history_promotes_compaction_anchor_to_front_system_summary() {
        let mut history = vec![
            AiChatMessage {
                id: "task-mode".to_string(),
                role: AiChatRole::System,
                content: "Task instructions".to_string(),
                timestamp_ms: 0,
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
            },
            AiChatMessage {
                id: "stale-system".to_string(),
                role: AiChatRole::System,
                content: "Persisted stale system prompt".to_string(),
                timestamp_ms: 0,
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
            },
            AiChatMessage {
                id: "anchor-1".to_string(),
                role: AiChatRole::System,
                content: "用户之前打开过本地终端。".to_string(),
                timestamp_ms: 1,
                model: None,
                context: None,
                is_streaming: false,
                thinking_content: None,
                metadata: Some(AiChatMessageMetadata {
                    kind: "compaction-anchor".to_string(),
                    original_count: Some(4),
                    compacted_at_ms: Some(1),
                    original_messages: None,
                }),
                tool_call_id: None,
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            },
            AiChatMessage {
                id: "user-1".to_string(),
                role: AiChatRole::User,
                content: "继续".to_string(),
                timestamp_ms: 2,
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
            },
        ];

        normalize_ai_stream_history_for_provider(&mut history);

        assert_eq!(history.len(), 3);
        assert_eq!(history[0].id, "task-mode");
        assert_eq!(history[1].role, AiChatRole::System);
        assert_eq!(
            history[1].content,
            "Previous conversation summary:\n用户之前打开过本地终端。"
        );
        assert!(history[1].metadata.is_none());
        assert_eq!(history[2].role, AiChatRole::User);
        assert!(history.iter().all(|message| message.id != "stale-system"));
    }

    #[test]
    fn completed_tool_calls_are_deduped_by_id_before_protocol_append() {
        let mut completed = Vec::new();
        record_completed_ai_tool_call(
            &mut completed,
            AiToolCall {
                id: "call-1".to_string(),
                name: "read_resource".to_string(),
                arguments: "{\"query\":\"old\"}".to_string(),
            },
        );
        record_completed_ai_tool_call(
            &mut completed,
            AiToolCall {
                id: "call-1".to_string(),
                name: "read_resource".to_string(),
                arguments: "{\"query\":\"new\"}".to_string(),
            },
        );
        record_completed_ai_tool_call(
            &mut completed,
            AiToolCall {
                id: "call-2".to_string(),
                name: "get_state".to_string(),
                arguments: "{}".to_string(),
            },
        );

        assert_eq!(completed.len(), 2);
        assert_eq!(completed[0].id, "call-1");
        assert_eq!(completed[0].arguments, "{\"query\":\"new\"}");
        assert_eq!(completed[1].id, "call-2");
    }

    #[test]
    fn cancel_rejects_streaming_pending_tool_calls_with_results() {
        let mut conversation = AiConversation {
            id: "conv-1".to_string(),
            title: "Chat".to_string(),
            messages: vec![AiChatMessage {
                id: "assistant-1".to_string(),
                role: AiChatRole::Assistant,
                content: String::new(),
                timestamp_ms: 1,
                model: None,
                context: None,
                is_streaming: true,
                thinking_content: None,
                metadata: None,
                tool_call_id: None,
                tool_calls: vec![serde_json::json!({
                    "id": "call-1",
                    "name": "open_app_surface",
                    "arguments": "{}",
                    "status": "pending_user_approval",
                    "result": serde_json::Value::Null,
                })],
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
            origin: "sidebar".to_string(),
            profile_id: None,
            message_count: 1,
            session_id: None,
            session_metadata: None,
            messages_loaded: true,
        };

        reject_incomplete_ai_tool_calls_on_cancel(&mut conversation);

        let call = &conversation.messages[0].tool_calls[0];
        assert_eq!(call["status"], "rejected");
        assert_eq!(call["result"]["ok"], false);
        assert_eq!(
            call["result"]["error"]["message"],
            "Generation was stopped."
        );
        let parts = conversation.messages[0]
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert!(parts.iter().any(|part| {
            part.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
                && part
                    .get("toolCallId")
                    .and_then(serde_json::Value::as_str)
                    == Some("call-1")
        }));
    }
}
