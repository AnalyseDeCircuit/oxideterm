#[cfg(test)]
mod ai_turn_order_tests {
    use super::*;
    use oxideterm_ai::AiChatMessageMetadata;
    use oxideterm_gpui_ai::transcript::ai_summary_source_transcript_ref;

    #[test]
    fn remote_write_error_debug_omits_sensitive_details() {
        let path = "/private/runtime/secret.txt";
        let error = AiRemoteFileWriteError::ExpectedFileMissing {
            path: path.to_string(),
        };

        let debug = format!("{error:?}");

        assert!(debug.contains("ExpectedFileMissing"));
        assert!(!debug.contains(path));
    }

    #[test]
    fn ai_delivery_redacts_tool_payloads_and_classifies_provider_errors() {
        let (tx, rx) = crate::workspace::delivery::ActiveDeliverySender::channel();
        let call = AiToolCall {
            id: "tool-secret".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({
                "command": "echo visible",
                "apiKey": "short-secret",
            })
            .to_string(),
        };

        send_ai_tool_status_with_payload(
            &tx,
            1,
            "conversation-1",
            "assistant-1",
            &call,
            "completed",
            Some(serde_json::json!({
                "output": "export TOKEN=result-secret-value",
            })),
            Some("execute".to_string()),
            Some("password=summary-secret-value".to_string()),
            false,
            Some("raw-secret-value".to_string()),
            None,
            None,
        )
        .expect("tool status");
        let delivery = rx.recv().expect("tool delivery");
        let AiStreamDeliveryEvent::ToolStatus {
            arguments,
            result,
            summary,
            raw_text,
            ..
        } = delivery.event
        else {
            panic!("expected tool status");
        };
        let retained = format!("{arguments}{result:?}{summary:?}{raw_text:?}");
        assert!(!retained.contains("short-secret"));
        assert!(!retained.contains("result-secret-value"));
        assert!(!retained.contains("summary-secret-value"));
        assert!(!retained.contains("raw-secret-value"));
        assert!(retained.contains("[REDACTED]"));

        send_ai_stream_delivery(
            &tx,
            1,
            "conversation-1",
            "assistant-1",
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(
                "Authorization: Bearer provider-secret-value".to_string(),
            )),
        )
        .expect("provider error");
        let delivery = rx.recv().expect("error delivery");
        assert!(matches!(
            delivery.event,
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(ref error))
                if error == "stream_failed"
        ));
    }

    #[test]
    fn model_visible_settings_projection_excludes_secret_bearing_configuration() {
        let mut settings = oxideterm_settings::PersistedSettings::default();
        settings.ai.custom_system_prompt = "private-system-prompt".to_string();
        settings.ai.memory.content = "private-memory-content".to_string();
        settings.ai.providers = vec![serde_json::json!({
            "id": "private-provider",
            "apiKey": "private-provider-key",
        })];
        settings.ai.mcp_servers = vec![serde_json::json!({
            "id": "private-mcp",
            "headers": { "Authorization": "Bearer private-mcp-token" },
        })];
        settings.ai.acp_agents = vec![
            serde_json::from_value(serde_json::json!({
                "id": "private-agent",
                "command": "agent",
                "args": ["--token", "private-acp-token"],
                "env": { "AGENT_TOKEN": "private-acp-env" },
            }))
            .expect("ACP agent settings"),
        ];

        let projection = ai_model_visible_settings_projection(&settings);
        let serialized = serde_json::to_string(&projection).expect("settings projection");

        assert!(projection.pointer("/ai/toolUse").is_some());
        assert!(projection.get("terminal").is_some());
        assert!(projection.get("sftp").is_some());
        for secret in [
            "private-system-prompt",
            "private-memory-content",
            "private-provider-key",
            "private-mcp-token",
            "private-acp-token",
            "private-acp-env",
        ] {
            assert!(!serialized.contains(secret));
        }
        assert!(projection.pointer("/ai/providers").is_none());
        assert!(projection.pointer("/ai/mcpServers").is_none());
        assert!(projection.pointer("/ai/acpAgents").is_none());
    }

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
    fn second_model_round_replaces_the_previous_runtime_context() {
        let mut history = vec![
            test_message("base-system", AiChatRole::System, "Stable policies".to_string()),
            test_message("latest-user", AiChatRole::User, "inspect the active terminal".to_string()),
        ];

        replace_ai_runtime_context_message(
            &mut history,
            r#"{"runtimeContext":{"snapshotId":"snap_first","observedAtMs":100,"liveHandles":[{"handleId":"rt_first"}]}}"#.to_string(),
        );
        let prefix = history[..history.len() - 1].to_vec();
        let mut call = test_message("round1-call", AiChatRole::Assistant, "Checking".to_string());
        call.tool_calls = vec![serde_json::json!({"id":"call-1","name":"run_command","arguments":"{}"})];
        history.push(call);
        let mut result = test_message("round1-result", AiChatRole::Tool, "Result".to_string());
        result.tool_call_id = Some("call-1".to_string());
        history.push(result);
        let latest = r#"{"runtimeContext":{"snapshotId":"snap_second","observedAtMs":200,"liveHandles":[{"handleId":"rt_second"}]}}"#;
        replace_ai_runtime_context_message(&mut history, latest.to_string());

        assert_eq!(
            history.iter().map(|message| message.id.as_str()).collect::<Vec<_>>(),
            vec!["runtime-context-policy", "base-system", "latest-user", "round1-call", "round1-result", "runtime-context-v2"],
        );
        assert_eq!(&history[..prefix.len()], prefix.as_slice());
        assert!(history[0].content.contains("A stale handle must be rediscovered"));
        let runtime_message = history.last().unwrap();
        assert_eq!(runtime_message.role, AiChatRole::System);
        assert_eq!(runtime_message.content, latest);
        normalize_ai_stream_history_for_provider(&mut history);
        assert_eq!(
            history.iter().map(|message| message.id.as_str()).collect::<Vec<_>>(),
            vec!["latest-user", "round1-call"],
        );
    }












    #[test]
    fn compaction_summary_uses_latest_tool_round_id() {
        let mut message = test_message("a-1", AiChatRole::Assistant, "answer".to_string());
        message.turn = Some(serde_json::json!({
            "toolRounds": [
                { "id": "round-old" },
                { "id": "round-new" }
            ]
        }));

        assert_eq!(
            ai_latest_summary_round_id(&[message]),
            Some("round-new".to_string())
        );
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
    fn round_summary_attaches_after_delivery_and_updates_without_pending_tail() {
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
        upsert_ai_round_summary(
            &mut message,
            "assistant-1-round-1",
            "read_resource: ok - verified config",
            serde_json::json!({ "model": "deepseek-v4-pro" }),
        );

        let turn = message.turn.as_ref().expect("turn");
        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("rounds");
        assert_eq!(
            rounds[0]["summary"],
            "read_resource: ok - verified config"
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
        assert_ne!(rounds[0]["id"], rounds[1]["id"]);
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
    fn tool_arguments_must_match_the_v2_contract() {
        assert_eq!(
            parse_ai_tool_args("list_targets", "{\"view\":\"live_sessions\"}")
                .and_then(|value| value.get("view").cloned()),
            Some(serde_json::json!("live_sessions"))
        );
        assert!(parse_ai_tool_args("list_targets", "{\"target_id\":\"local\"}").is_none());
        assert!(parse_ai_tool_args("list_targets", "not json").is_none());
        assert!(
            parse_ai_tool_args("list_targets", "[\"not\", \"an\", \"object\"]").is_none()
        );
        assert_eq!(
            parse_ai_tool_args(
                "mcp__example__inspect",
                "{\"target_id\":\"terminal-session:42\",\"custom\":true}"
            ),
            Some(serde_json::json!({
                "target_id": "terminal-session:42",
                "custom": true,
            })),
            "an external MCP payload remains server-owned and gains no app authority"
        );
    }


}
