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
            suggestions: Vec::new(),
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
    fn history_trimming_keeps_latest_regular_message_when_budget_is_zero() {
        let mut history = vec![
            test_message("system", AiChatRole::System, "large system".repeat(100)),
            test_message("user-1", AiChatRole::User, "first".to_string()),
            test_message("assistant-1", AiChatRole::Assistant, "answer".to_string()),
            test_message("user-2", AiChatRole::User, "latest".to_string()),
        ];

        let trimmed = trim_ai_stream_history_to_budget(&mut history, 100, 100);

        assert_eq!(trimmed, 2);
        assert_eq!(
            history
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["system", "user-2"]
        );
    }

    #[test]
    fn token_estimate_counts_message_content_only_like_tauri_chat_store() {
        let mut message = test_message("assistant", AiChatRole::Assistant, "hello".to_string());
        let content_only = ai_message_estimated_tokens(&message);
        message.thinking_content = Some("hidden thinking should not count".repeat(20));
        message.context = Some("legacy context should not count".repeat(20));
        message.tool_calls = vec![serde_json::json!({
            "id": "call-1",
            "name": "run_command",
            "arguments": "{\"command\":\"echo hi\"}",
            "result": { "output": "large tool output".repeat(20) }
        })];

        assert_eq!(ai_message_estimated_tokens(&message), content_only);
    }

    #[test]
    fn token_estimate_uses_utf16_length_like_tauri() {
        assert_eq!(ai_estimated_tokens("😀"), 1);
        assert_eq!(ai_estimated_tokens("😀😀😀😀"), 3);
    }

    #[test]
    fn context_indicator_tool_definition_tokens_use_real_orchestrator_schema() {
        let tools = oxideterm_ai::orchestrator_tool_definitions();

        assert_eq!(
            ai_estimated_tool_definitions_tokens(),
            ai_tool_definitions_estimated_tokens(&tools)
        );
        assert!(ai_estimated_tool_definitions_tokens() > tools.len() * 10);
    }

    #[test]
    fn native_chat_tool_definitions_include_mcp_bridge_tools() {
        let registry = oxideterm_ai::McpRegistry::new(oxideterm_ai::AiProviderKeyStore::new());
        let policy = oxideterm_ai::AiToolUsePolicy {
            enabled: true,
            disabled_tools: vec!["list_mcp_resources".to_string(), "read_resource".to_string()],
            ..oxideterm_ai::AiToolUsePolicy::default()
        };

        let tools = ai_stream_tool_definitions(true, &policy, &registry);
        let names = tools.iter().map(|tool| tool.name.as_str()).collect::<Vec<_>>();

        assert!(names.contains(&"read_resource"));
        assert!(names.contains(&"read_mcp_resource"));
        assert!(!names.contains(&"list_mcp_resources"));
    }

    #[test]
    fn context_indicator_tool_result_tokens_only_count_user_and_assistant_messages() {
        let tool_call = serde_json::json!({
            "arguments": "{\"command\":\"echo hi\"}",
            "result": { "output": "large tool output" },
        });
        let mut system = test_message("system", AiChatRole::System, String::new());
        system.tool_calls = vec![tool_call.clone()];
        let mut assistant = test_message("assistant", AiChatRole::Assistant, String::new());
        assistant.tool_calls = vec![tool_call];
        let conversation = AiConversation {
            id: "conv-1".to_string(),
            title: "Conversation".to_string(),
            messages: vec![system, assistant.clone()],
            created_at_ms: 0,
            updated_at_ms: 0,
            origin: "sidebar".to_string(),
            profile_id: None,
            message_count: 2,
            session_id: None,
            session_metadata: None,
            messages_loaded: true,
        };

        assert_eq!(
            ai_conversation_tool_result_tokens(&conversation),
            ai_tool_call_estimated_tokens(&assistant.tool_calls[0])
        );
    }

    #[test]
    fn sftp_target_shape_is_node_runtime_scoped_like_tauri() {
        let node_id = NodeId::new("node-1".to_string());
        let mut config = oxideterm_ssh::SshConfig::default();
        config.host = "example.com".to_string();
        config.username = "alice".to_string();
        let node = WorkspaceSshNode {
            saved_connection_id: Some("conn-1".to_string()),
            config,
            title: "example".to_string(),
            terminal_ids: Vec::new(),
            readiness: NodeReadiness::Ready,
        };

        let target = ai_sftp_target_for_node(&node_id, &node, "sftp-1".to_string());

        assert_eq!(target.id, "sftp-session:sftp-1");
        assert_eq!(target.kind, "sftp-session");
        assert_eq!(
            target.capabilities,
            vec![
                "filesystem.read".to_string(),
                "filesystem.write".to_string(),
                "state.list".to_string(),
            ]
        );
        assert_eq!(target.refs.get("nodeId").map(String::as_str), Some("node-1"));
        assert_eq!(target.refs.get("sessionId").map(String::as_str), Some("sftp-1"));
        assert_eq!(target.refs.get("connectionId").map(String::as_str), Some("conn-1"));
        assert!(!target.refs.contains_key("tabId"));
        assert_eq!(
            target.metadata.get("host").and_then(serde_json::Value::as_str),
            Some("example.com")
        );
    }

    #[test]
    fn ide_workspace_target_uses_editor_tab_refs_like_tauri() {
        let node_id = NodeId::new("node-1".to_string());
        let mut config = oxideterm_ssh::SshConfig::default();
        config.host = "example.com".to_string();
        config.username = "alice".to_string();
        let node = WorkspaceSshNode {
            saved_connection_id: Some("conn-1".to_string()),
            config,
            title: "example".to_string(),
            terminal_ids: Vec::new(),
            readiness: NodeReadiness::Ready,
        };

        let target = ai_ide_workspace_target_for_node(
            &node_id,
            &node,
            Some("editor-tab-1".to_string()),
            Some("/srv/app".to_string()),
            Some("app".to_string()),
        );

        assert_eq!(target.id, "ide-workspace:node-1");
        assert_eq!(target.kind, "ide-workspace");
        assert_eq!(target.label, "app");
        assert_eq!(target.refs.get("nodeId").map(String::as_str), Some("node-1"));
        assert_eq!(
            target.refs.get("connectionId").map(String::as_str),
            Some("conn-1")
        );
        assert_eq!(
            target.refs.get("tabId").map(String::as_str),
            Some("editor-tab-1")
        );
        assert_eq!(
            target.metadata.get("rootPath").and_then(serde_json::Value::as_str),
            Some("/srv/app")
        );
        assert_eq!(
            target.metadata.get("activeTabId").and_then(serde_json::Value::as_str),
            Some("editor-tab-1")
        );
    }

    #[test]
    fn connect_result_terminal_target_keeps_tauri_synthetic_refs() {
        let mut refs = std::collections::BTreeMap::new();
        refs.insert("sessionId".to_string(), "session-1".to_string());
        refs.insert("tabId".to_string(), "tab-1".to_string());
        let terminal = AiOrchestratorTarget {
            id: "terminal-session:session-1".to_string(),
            kind: "terminal-session".to_string(),
            label: "SSH terminal session-".to_string(),
            state: "connected".to_string(),
            capabilities: vec![
                "terminal.observe".to_string(),
                "terminal.send".to_string(),
                "terminal.wait".to_string(),
                "state.list".to_string(),
            ],
            refs,
            metadata: serde_json::json!({
                "paneId": 7,
                "terminalType": "terminal",
            }),
            terminal_buffer: None,
            terminal_screen: None,
            ssh_handle: None,
        };

        let target = ai_connect_result_terminal_target(
            &terminal,
            "prod (alice@example.com:22)",
            Some("node-1"),
            Some("conn-1"),
        );

        assert_eq!(target.label, "prod (alice@example.com:22) terminal");
        assert_eq!(
            target.refs.get("sessionId").map(String::as_str),
            Some("session-1")
        );
        assert_eq!(target.refs.get("nodeId").map(String::as_str), Some("node-1"));
        assert_eq!(
            target.refs.get("connectionId").map(String::as_str),
            Some("conn-1")
        );
        assert!(!target.refs.contains_key("tabId"));
        assert_eq!(
            target.metadata.get("terminalType").and_then(serde_json::Value::as_str),
            Some("terminal")
        );
        assert!(target.metadata.get("paneId").is_none());
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
    fn chat_request_max_response_tokens_matches_tauri_reserve_fallback() {
        let settings = oxideterm_settings::PersistedSettings::default();

        assert_eq!(
            ai_chat_request_max_response_tokens(&settings, "builtin-openai", "gpt-4o-mini"),
            Some(4096)
        );
    }

    #[test]
    fn chat_request_max_response_tokens_prefers_user_override() {
        let mut settings = oxideterm_settings::PersistedSettings::default();
        settings.ai.model_max_response_tokens.insert(
            "builtin-openai".to_string(),
            serde_json::json!({ "gpt-4o-mini": 2048 }),
        );

        assert_eq!(
            ai_chat_request_max_response_tokens(&settings, "builtin-openai", "gpt-4o-mini"),
            Some(2048)
        );
    }

    #[test]
    fn user_memory_prompt_truncates_like_tauri_character_limit() {
        let memory = "你".repeat(4_001);

        let prompt = ai_user_memory_prompt(&memory, true).expect("memory prompt");

        assert!(prompt.contains(&"你".repeat(4_000)));
        assert!(!prompt.contains(&"你".repeat(4_001)));
        assert!(prompt.contains("\n...[truncated]"));
    }

    #[test]
    fn user_memory_prompt_respects_disabled_setting() {
        assert!(ai_user_memory_prompt("remember this", false).is_none());
    }

    #[test]
    fn compaction_plan_uses_tauri_manual_and_silent_keep_budgets() {
        let messages = (0..6)
            .map(|index| {
                test_message(
                    &format!("m-{index}"),
                    if index % 2 == 0 {
                        AiChatRole::User
                    } else {
                        AiChatRole::Assistant
                    },
                    "x".repeat(1_000),
                )
            })
            .collect::<Vec<_>>();

        let silent = ai_compaction_plan(&messages, 2_000, true).expect("silent plan");
        let manual = ai_compaction_plan(&messages, 2_000, false).expect("manual plan");

        assert!(silent.keep_messages.len() >= manual.keep_messages.len());
        assert!(silent.compact_messages.len() <= manual.compact_messages.len());
    }

    #[test]
    fn compaction_plan_skips_when_less_than_two_messages_would_compact() {
        let messages = vec![
            test_message("u-1", AiChatRole::User, "short".to_string()),
            test_message("a-1", AiChatRole::Assistant, "short".to_string()),
            test_message("u-2", AiChatRole::User, "short".to_string()),
            test_message("a-2", AiChatRole::Assistant, "short".to_string()),
        ];

        assert!(ai_compaction_plan(&messages, 100_000, true).is_none());
    }

    #[test]
    fn compaction_plan_keeps_tauri_zero_budget_boundary() {
        let messages = vec![
            test_message("u-1", AiChatRole::User, "first".to_string()),
            test_message("a-1", AiChatRole::Assistant, "answer".to_string()),
            test_message("u-2", AiChatRole::User, String::new()),
            test_message("a-2", AiChatRole::Assistant, "a".to_string()),
        ];

        let plan = ai_compaction_plan(&messages, 1, true).expect("zero-budget plan");

        assert_eq!(
            plan.keep_messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a-2"]
        );
    }

    #[test]
    fn compaction_summary_prompt_matches_tauri_shape() {
        let anchor = AiChatMessage {
            id: "anchor-1".to_string(),
            role: AiChatRole::System,
            content: " previous summary ".to_string(),
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
            suggestions: Vec::new(),
        };
        let messages = vec![
            anchor,
            test_message("u-1", AiChatRole::User, " question ".to_string()),
            test_message("tool-1", AiChatRole::Tool, "tool output".to_string()),
            test_message("a-1", AiChatRole::Assistant, " answer ".to_string()),
        ];

        let prompt = ai_compaction_summary_messages(&messages);

        assert_eq!(prompt.len(), 2);
        assert_eq!(prompt[0].role, AiChatRole::System);
        assert_eq!(prompt[1].role, AiChatRole::User);
        assert!(prompt[1].content.contains("[Previous Summary]:  previous summary "));
        assert!(prompt[1].content.contains("User:  question "));
        assert!(prompt[1].content.contains("Assistant:  answer "));
        assert!(!prompt[1].content.contains("tool output"));
    }

    #[test]
    fn conversation_summary_prompt_excludes_tool_messages_like_tauri() {
        let messages = vec![
            test_message("u-1", AiChatRole::User, " question ".to_string()),
            test_message("tool-1", AiChatRole::Tool, "tool output".to_string()),
            test_message("a-1", AiChatRole::Assistant, " answer ".to_string()),
        ];

        let prompt = ai_conversation_summary_messages(&messages);

        assert_eq!(prompt.len(), 2);
        assert!(prompt[1].content.contains("User:  question "));
        assert!(prompt[1].content.contains("Assistant:  answer "));
        assert!(!prompt[1].content.contains("tool output"));
    }

    #[test]
    fn compaction_anchor_snapshot_keeps_only_tauri_message_core() {
        let mut message = test_message("a-1", AiChatRole::Assistant, "answer".to_string());
        message.model = Some("gpt-4o".to_string());
        message.context = Some("terminal context".to_string());
        message.thinking_content = Some("reasoning".to_string());
        message.tool_call_id = Some("call-1".to_string());
        message.tool_calls = vec![serde_json::json!({ "id": "call-1" })];
        message.turn = Some(serde_json::json!({ "parts": [] }));
        message.transcript_ref = Some(serde_json::json!({ "endEntryId": "entry-1" }));
        message.summary_ref = Some(serde_json::json!({ "kind": "conversation" }));
        message.suggestions = vec![oxideterm_ai::AiFollowUpSuggestion {
            icon: "Zap".to_string(),
            text: "Next".to_string(),
        }];

        let snapshot = ai_compaction_anchor_snapshot(&[message]);

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].id, "a-1");
        assert_eq!(snapshot[0].role, AiChatRole::Assistant);
        assert_eq!(snapshot[0].content, "answer");
        assert!(snapshot[0].model.is_none());
        assert!(snapshot[0].context.is_none());
        assert!(snapshot[0].thinking_content.is_none());
        assert!(snapshot[0].tool_call_id.is_none());
        assert!(snapshot[0].tool_calls.is_empty());
        assert!(snapshot[0].turn.is_none());
        assert!(snapshot[0].transcript_ref.is_none());
        assert!(snapshot[0].summary_ref.is_none());
        assert!(snapshot[0].suggestions.is_empty());
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
            suggestions: Vec::new(),
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
    fn conversation_summary_reference_supports_transcript_lookup_prompt() {
        let summarized = vec![
            test_message("u-1", AiChatRole::User, "first".to_string()),
            test_message("a-1", AiChatRole::Assistant, "answer".to_string()),
            test_message("u-2", AiChatRole::User, "second".to_string()),
            test_message("a-2", AiChatRole::Assistant, "answer".to_string()),
        ];
        let source_ref = ai_summary_source_transcript_ref(&summarized, "conv-1");
        let mut summary = test_message(
            "summary-1",
            AiChatRole::Assistant,
            "summary".to_string(),
        );
        summary.transcript_ref = Some(serde_json::json!({
            "conversationId": "conv-1",
            "endEntryId": "transcript-summary-created-summary-1",
        }));
        summary.summary_ref = Some(serde_json::json!({
            "kind": "conversation",
            "roundId": null,
            "transcriptRef": source_ref,
        }));

        let lookup_ref = ai_find_prompt_transcript_lookup_reference(&[summary])
            .expect("conversation summary transcript lookup reference");
        let lookup_prompt = ai_build_transcript_lookup_prompt_reference(lookup_ref);

        assert!(lookup_prompt.contains("conversation=conv-1"));
        assert!(lookup_prompt.contains("start=u-1"));
        assert!(lookup_prompt.contains("end=a-2"));
    }

    #[test]
    fn transcript_lookup_prompt_missing_conversation_matches_tauri_undefined_string() {
        let lookup_prompt =
            ai_build_transcript_lookup_prompt_reference(serde_json::json!({ "startEntryId": "s" }));

        assert!(lookup_prompt.contains("conversation=undefined"));
        assert!(lookup_prompt.contains("start=s"));
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
            suggestions: Vec::new(),
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
    fn required_tool_prompt_is_available_before_history_budgeting() {
        let mut tool_policy = AiToolUsePolicy::default();
        tool_policy.enabled = true;
        let config = AiChatStreamConfig {
            provider_id: Some("provider-1".to_string()),
            provider_type: "openai".to_string(),
            base_url: "https://api.example.test".to_string(),
            model: "model".to_string(),
            api_key: None,
            max_response_tokens: None,
            reasoning_effort: Some("auto".to_string()),
            safety_mode: AiPolicySafetyMode::Default,
            profile_id: None,
            tool_policy,
            tools: Vec::new(),
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        };

        let prompt = ai_orchestrator_obligation_prompt_for_text(&config, "打开本地终端")
            .expect("required tool prompt");

        assert!(prompt.contains("## Required Tool Call"));
        assert!(prompt.contains("open_app_surface"));
    }

    #[test]
    fn rag_prompt_inserts_before_suggestions_and_runtime_rules() {
        let mut system_prompt = [
            "base",
            "## Follow-Up Suggestions",
            "suggestions",
            "## OxideSens Runtime Rules",
            "rules",
        ]
        .join("\n\n");

        ai_insert_rag_prompt_before_runtime_tail(&mut system_prompt, "## Relevant Knowledge Base");

        let rag_index = system_prompt.find("## Relevant Knowledge Base").unwrap();
        let suggestions_index = system_prompt.find("## Follow-Up Suggestions").unwrap();
        let runtime_index = system_prompt.find("## OxideSens Runtime Rules").unwrap();
        assert!(rag_index < suggestions_index);
        assert!(rag_index < runtime_index);
    }

    #[test]
    fn required_tool_buffer_flushes_only_after_tool_call() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut assistant_content = String::new();
        let mut assistant_thinking = String::new();
        let mut buffered_content = "我已经打开了终端。".to_string();
        let mut buffered_thinking = "需要调用工具。".to_string();

        flush_ai_required_tool_buffer(
            &tx,
            1,
            "conversation-1",
            "assistant-1",
            &mut assistant_content,
            &mut assistant_thinking,
            &mut buffered_content,
            &mut buffered_thinking,
        )
        .expect("flush");

        assert!(buffered_content.is_empty());
        assert!(buffered_thinking.is_empty());
        assert_eq!(assistant_content, "我已经打开了终端。");
        assert_eq!(assistant_thinking, "需要调用工具。");

        let first = rx.recv().expect("thinking delivery");
        assert!(matches!(
            first.event,
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Thinking(_))
        ));
        let second = rx.recv().expect("content delivery");
        assert!(matches!(
            second.event,
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(_))
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
            suggestions: Vec::new(),
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
            suggestions: Vec::new(),
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
            suggestions: Vec::new(),
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
            suggestions: Vec::new(),
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
            suggestions: Vec::new(),
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
            suggestions: Vec::new(),
            },
            AiChatMessage {
                id: "anchor-1".to_string(),
                role: AiChatRole::System,
                content: " 用户之前打开过本地终端。 ".to_string(),
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
            suggestions: Vec::new(),
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
            suggestions: Vec::new(),
            },
        ];

        normalize_ai_stream_history_for_provider(&mut history);

        assert_eq!(history.len(), 3);
        assert_eq!(history[0].id, "task-mode");
        assert_eq!(history[1].role, AiChatRole::System);
        assert_eq!(
            history[1].content,
            "Previous conversation summary:\n 用户之前打开过本地终端。 "
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
    fn tool_arguments_must_parse_to_json_object() {
        assert_eq!(
            parse_ai_tool_args("{\"target\":\"local\"}")
                .and_then(|value| value.get("target").cloned()),
            Some(serde_json::json!("local"))
        );
        assert!(parse_ai_tool_args("not json").is_none());
        assert!(parse_ai_tool_args("[\"not\", \"an\", \"object\"]").is_none());
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
                suggestions: Vec::new(),
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

        let stopped = finalize_streaming_ai_messages_on_cancel(&mut conversation);

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
        assert_eq!(
            conversation.messages[0]
                .turn
                .as_ref()
                .and_then(|turn| turn.get("status"))
                .and_then(serde_json::Value::as_str),
            Some("complete")
        );
        assert!(!conversation.messages[0].is_streaming);
        assert_eq!(
            stopped,
            vec![AiStoppedAssistantTurn {
                message_id: "assistant-1".to_string(),
                status: "complete",
                retained: true,
            }]
        );
    }

    #[test]
    fn cancel_removes_empty_streaming_placeholder_like_tauri_abort() {
        let mut conversation = AiConversation {
            id: "conv-1".to_string(),
            title: "Chat".to_string(),
            messages: vec![AiChatMessage {
                id: "assistant-empty".to_string(),
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
                turn: Some(serde_json::json!({
                    "id": "assistant-empty",
                    "status": "streaming",
                    "parts": [],
                    "toolRounds": [],
                    "plainTextSummary": "",
                })),
                transcript_ref: None,
                summary_ref: None,
                branches: None,
                suggestions: Vec::new(),
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

        let stopped = finalize_streaming_ai_messages_on_cancel(&mut conversation);

        assert!(conversation.messages.is_empty());
        assert_eq!(conversation.message_count, 0);
        assert_eq!(
            stopped,
            vec![AiStoppedAssistantTurn {
                message_id: "assistant-empty".to_string(),
                status: "error",
                retained: false,
            }]
        );
    }
}
