#[cfg(test)]
mod ai_turn_order_tests {
    use super::*;

    #[gpui::test]
    fn runtime_evidence_records_are_entity_owned(cx: &mut gpui::TestAppContext) {
        let task_runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("AI runtime"),
        );
        let entity = cx.new(|cx| {
            crate::workspace::ai_state::AiWorkspaceEntity::new(
                task_runtime,
                oxideterm_ai::AiProviderKeyStore::new(),
                cx,
            )
        });
        entity.update(cx, |entity, _cx| {
            let representative_runtime_epoch = "epoch-test-only";
            let result = serde_json::json!({
                "output": "API_KEY=supersecret123456",
                "data": { "exitCode": 0 },
                "meta": { "runtimeEpoch": representative_runtime_epoch },
            });
            let record = entity
                .record_ai_tool_execution_status(
                    "conversation-a",
                    "assistant-a",
                    "tool-a",
                    "run_command",
                    r#"{"command":"codex --help API_KEY=supersecret123456"}"#,
                    "completed",
                    Some(&result),
                    Some("execute"),
                    10,
                )
                .expect("tool execution record");
            let facts = entity.record_ai_tool_result_facts(&record, Some(&result), 10);

            assert_eq!(entity.tool_execution_records.len(), 1);
            assert!(!facts.is_empty());
            assert_eq!(entity.tool_result_facts.len(), facts.len());
            let retained = format!(
                "{:?}{:?}",
                entity.tool_execution_records, entity.tool_result_facts
            );
            assert!(!retained.contains("supersecret123456"));
            assert!(retained.contains("command_chars="));
            assert!(retained.contains("exit_code: 0"));
            assert!(!retained.contains("target_id"));
            assert!(!retained.contains("session_id"));
            assert!(!retained.contains("node_id"));
            assert!(!retained.contains(representative_runtime_epoch));
        });
    }

    #[test]
    fn persisted_tool_arguments_drop_secret_capable_execution_payloads() {
        let arguments = serde_json::json!({
            "command": "curl https://example.test",
            "apiKey": "short",
            "headers": {
                "Authorization": "Bearer opaque-value",
            },
        })
        .to_string();

        let sanitized = sanitize_ai_tool_arguments_for_persistence(&arguments);

        assert!(!sanitized.contains("curl https://example.test"));
        assert!(!sanitized.contains("\"command\""));
        assert!(!sanitized.contains("\"short\""));
        assert!(!sanitized.contains("opaque-value"));
        assert!(!sanitized.contains("\"headers\""));
    }

    #[test]
    fn approval_arguments_keep_only_a_locally_redacted_command() {
        let arguments = serde_json::json!({
            "command": "curl https://example.test",
            "apiKey": "short",
        })
        .to_string();

        let sanitized = sanitize_ai_tool_arguments_for_approval(&arguments);

        assert!(sanitized.contains("curl https://example.test"));
        assert!(!sanitized.contains("\"short\""));
        assert!(sanitized.contains("[REDACTED]"));
    }

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
    fn diagnostic_resource_projection_omits_stable_identifiers() {
        let stable_id = "4e22e673-067e-46e2-8b9f-902d7b21af4c";
        let resource_kind = ai_tool_argument_resource_kind(Some(&serde_json::json!({
            "resource_ref": {
                "kind": "saved_connection",
                "id": stable_id,
                "label": "Production",
            },
        })));
        let mut record = test_tool_execution_record("tool-resource");
        record.resource_kind = resource_kind;

        let diagnostic = ai_tool_execution_record_json(&record).to_string();

        assert!(!diagnostic.contains(stable_id));
        assert!(diagnostic.contains("saved_connection"));
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

    fn test_tool_execution_record(tool_call_id: &str) -> AiToolExecutionRecord {
        AiToolExecutionRecord {
            record_id: format!("tool-exec-{tool_call_id}"),
            conversation_id: "conversation-1".to_string(),
            assistant_message_id: "assistant-1".to_string(),
            tool_call_id: tool_call_id.to_string(),
            tool_name: "run_command".to_string(),
            argument_summary: "runtime_target=current command=df -h /".to_string(),
            resource_kind: None,
            target_kind: Some("local-shell".to_string()),
            risk: "execute".to_string(),
            approval_source: Some("policy_allowed".to_string()),
            execution_surface: "visible_terminal".to_string(),
            visible_in_terminal: Some(true),
            status: "completed".to_string(),
            success: Some(true),
            error_code: None,
            duration_ms: Some(12),
            started_at: 1,
            finished_at: Some(2),
        }
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
    fn second_model_round_replaces_the_previous_runtime_context() {
        let mut history = vec![test_message(
            "latest-user",
            AiChatRole::User,
            "inspect the active terminal".to_string(),
        )];

        replace_ai_runtime_context_message(
            &mut history,
            r#"{"runtimeContext":{"snapshotId":"snap_first"}}"#.to_string(),
        );
        replace_ai_runtime_context_message(
            &mut history,
            r#"{"runtimeContext":{"snapshotId":"snap_second"}}"#.to_string(),
        );

        let runtime_messages = history
            .iter()
            .filter(|message| message.id == AI_RUNTIME_CONTEXT_MESSAGE_ID)
            .collect::<Vec<_>>();
        assert_eq!(runtime_messages.len(), 1);
        assert!(runtime_messages[0].content.contains("snap_second"));
        assert!(!runtime_messages[0].content.contains("snap_first"));
    }

    #[test]
    fn native_chat_tool_definitions_include_mcp_bridge_tools() {
        let registry = oxideterm_ai::McpRegistry::new(oxideterm_ai::AiProviderKeyStore::new());
        let policy = oxideterm_ai::AiToolUsePolicy {
            enabled: true,
            disabled_tools: vec![
                "list_mcp_resources".to_string(),
                "read_resource".to_string(),
            ],
            ..oxideterm_ai::AiToolUsePolicy::default()
        };

        let tools = ai_stream_tool_definitions(true, true, &policy, &registry);
        let names = tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"read_resource"));
        assert!(names.contains(&"read_mcp_resource"));
        assert!(!names.contains(&"list_mcp_resources"));
    }

    #[test]
    fn acp_session_started_ignores_stale_generation_and_persists_current_metadata() {
        let mut conversations = vec![AiConversation {
            id: "conv-1".to_string(),
            title: "Conversation".to_string(),
            messages: Vec::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            origin: "sidebar".to_string(),
            profile_id: None,
            message_count: 0,
            session_id: None,
            session_metadata: None,
            messages_loaded: true,
            turn_count: 0,
        }];

        let stale_applied = apply_ai_acp_session_started_to_conversations(
            &mut conversations,
            2,
            1,
            "conv-1",
            "stale-session",
            Some(serde_json::json!({ "source": "stale" })),
            Vec::new(),
            None,
            "agent-1",
        );

        assert!(!stale_applied);
        assert_eq!(conversations[0].session_id, None);
        assert_eq!(conversations[0].session_metadata, None);

        let current_applied = apply_ai_acp_session_started_to_conversations(
            &mut conversations,
            2,
            2,
            "conv-1",
            "fresh-session",
            Some(serde_json::json!({ "source": "fresh" })),
            vec![oxideterm_ai::AcpSessionConfigOption {
                config_id: "model".to_string(),
                name: "Model".to_string(),
                category: Some("model".to_string()),
                current_value_id: "model-a".to_string(),
                choices: vec![oxideterm_ai::AcpSessionConfigChoice {
                    value_id: "model-a".to_string(),
                    label: "Model A".to_string(),
                }],
            }],
            Some(oxideterm_ai::AcpSessionModeState {
                current_mode_id: "agent".to_string(),
                available_modes: vec![oxideterm_ai::AcpSessionMode {
                    mode_id: "agent".to_string(),
                    name: "Agent".to_string(),
                    description: Some("Agent mode".to_string()),
                }],
            }),
            "agent-1",
        );

        assert!(current_applied);
        assert_eq!(
            conversations[0].session_id.as_deref(),
            Some("fresh-session")
        );
        assert_eq!(
            conversations[0]
                .session_metadata
                .as_ref()
                .and_then(|metadata| metadata.get("acp")),
            Some(&serde_json::json!({
                "agentId": "agent-1",
                "sessionId": "fresh-session",
                "metadata": { "source": "fresh" },
                "configOptions": [{
                    "configId": "model",
                    "name": "Model",
                    "category": "model",
                    "currentValueId": "model-a",
                    "choices": [{ "valueId": "model-a", "label": "Model A" }],
                }],
                "modelSelection": {
                    "configId": "model",
                    "valueId": "model-a",
                },
                "configSelections": [{
                    "configId": "model",
                    "valueId": "model-a",
                }],
                "currentModeId": "agent",
                "availableModes": [{
                    "modeId": "agent",
                    "name": "Agent",
                    "description": "Agent mode",
                }],
                "availableCommands": [],
                "plan": null,
                "usage": null,
                "title": null,
            }))
        );

        let modes_removed = apply_ai_acp_session_started_to_conversations(
            &mut conversations,
            2,
            2,
            "conv-1",
            "mode-less-session",
            None,
            Vec::new(),
            None,
            "agent-1",
        );
        let current_state =
            ai_acp_session_state(&conversations[0]).expect("current ACP session state");
        assert!(modes_removed);
        assert_eq!(current_state.current_mode_id, None);
        assert!(current_state.available_modes.is_empty());
    }

    #[test]
    fn acp_handoff_cursor_advances_only_for_the_matching_agent() {
        let mut conversation = AiConversation {
            id: "conv-1".to_string(),
            title: "Conversation".to_string(),
            messages: vec![AiChatMessage {
                id: "assistant-1".to_string(),
                role: AiChatRole::Assistant,
                content: "done".to_string(),
                timestamp_ms: 42,
                model: Some("model".to_string()),
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
            }],
            created_at_ms: 0,
            updated_at_ms: 0,
            origin: "sidebar".to_string(),
            profile_id: None,
            message_count: 1,
            session_id: Some("session-1".to_string()),
            session_metadata: Some(serde_json::json!({
                "acp": {
                    "agentId": "agent-1",
                    "sessionId": "session-1",
                    "metadata": null,
                    "configOptions": [],
                    "modelSelection": null,
                    "configSelections": [],
                    "currentModeId": null,
                    "availableModes": [],
                    "availableCommands": [],
                    "plan": null,
                    "usage": null,
                    "title": null
                }
            })),
            messages_loaded: true,
            turn_count: 0,
        };

        assert!(!store_ai_acp_handoff_cursor_in_conversation(
            &mut conversation,
            "agent-2",
            "assistant-1",
        ));
        assert!(
            ai_acp_session_state(&conversation)
                .and_then(|state| state.handoff_cursor)
                .is_none()
        );

        assert!(store_ai_acp_handoff_cursor_in_conversation(
            &mut conversation,
            "agent-1",
            "assistant-1",
        ));
        assert_eq!(
            ai_acp_session_state(&conversation)
                .and_then(|state| state.handoff_cursor),
            Some(oxideterm_ai::AcpConversationHandoffCursor {
                message_id: "assistant-1".to_string(),
                timestamp_ms: 42,
            })
        );

        let mut resumed = conversation.clone();
        assert!(apply_ai_acp_session_started_to_conversations(
            std::slice::from_mut(&mut resumed),
            1,
            1,
            "conv-1",
            "session-1",
            None,
            Vec::new(),
            None,
            "agent-1",
        ));
        assert!(
            ai_acp_session_state(&resumed)
                .and_then(|state| state.handoff_cursor)
                .is_some()
        );

        assert!(apply_ai_acp_session_started_to_conversations(
            std::slice::from_mut(&mut conversation),
            1,
            1,
            "conv-1",
            "replacement-session",
            None,
            Vec::new(),
            None,
            "agent-1",
        ));
        assert!(
            ai_acp_session_state(&conversation)
                .and_then(|state| state.handoff_cursor)
                .is_none()
        );
    }

    #[test]
    fn acp_config_catalog_replaces_stale_selections_with_agent_state() {
        let config_options = vec![oxideterm_ai::AcpSessionConfigOption {
            config_id: "model".to_string(),
            name: "Model".to_string(),
            category: Some("model".to_string()),
            current_value_id: "model-b".to_string(),
            choices: vec![oxideterm_ai::AcpSessionConfigChoice {
                value_id: "model-b".to_string(),
                label: "Model B".to_string(),
            }],
        }];
        let mut model_selection = Some(oxideterm_ai::AcpSessionConfigSelection {
            config_id: "model".to_string(),
            value_id: "model-a".to_string(),
        });
        let mut config_selections = vec![
            oxideterm_ai::AcpSessionConfigSelection {
                config_id: "model".to_string(),
                value_id: "model-b".to_string(),
            },
            oxideterm_ai::AcpSessionConfigSelection {
                config_id: "removed-option".to_string(),
                value_id: "removed-value".to_string(),
            },
        ];

        synchronize_ai_acp_config_selections(
            &config_options,
            &mut model_selection,
            &mut config_selections,
        );

        assert_eq!(
            model_selection,
            Some(oxideterm_ai::AcpSessionConfigSelection {
                config_id: "model".to_string(),
                value_id: "model-b".to_string(),
            })
        );
        assert_eq!(
            config_selections,
            vec![oxideterm_ai::AcpSessionConfigSelection {
                config_id: "model".to_string(),
                value_id: "model-b".to_string(),
            }]
        );
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
                original_user_count: Some(2),
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

        assert_eq!(
            history[0].content,
            "Previous conversation summary:\nsummary"
        );
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

        let lookup_ref = ai_find_prompt_transcript_lookup_reference(&[summary])
            .expect("conversation summary transcript lookup reference");
        let lookup_prompt = ai_build_transcript_lookup_prompt_reference(lookup_ref);

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
                suggestions: Vec::new(),
            })
            .collect::<Vec<_>>();

        condense_ai_tool_messages(&mut history);

        assert!(
            history[0]
                .content
                .starts_with("[condensed] read_resource -> ok:")
        );
        assert!(
            history[1]
                .content
                .starts_with("[condensed] read_resource -> ok:")
        );
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
    fn tool_execution_argument_summary_omits_write_content() {
        let args = serde_json::json!({
            "target_id": "ssh-node:node-1",
            "resource": "file",
            "path": "/tmp/report.txt",
            "content": "super secret draft",
        });

        let summary = ai_tool_argument_summary("write_resource", Some(&args));

        assert!(summary.contains("resource=file"));
        assert!(!summary.contains("ssh-node:node-1"));
        assert!(!summary.contains("/tmp/report.txt"));
        assert!(!summary.contains("super secret draft"));
    }

    #[test]
    fn tool_execution_surface_prefers_visible_terminal_result() {
        let result = serde_json::json!({
            "execution": {
                "visibleInTerminal": true,
                "target": { "id": "ssh-node:node-1", "kind": "ssh-node" }
            }
        });
        let args = serde_json::json!({
            "target_id": "ssh-node:node-1",
            "command": "uptime",
        });

        assert_eq!(
            ai_tool_execution_surface("run_command", Some(&args), Some(&result)),
            "visible_terminal"
        );
    }

    #[test]
    fn result_binding_keeps_unbacked_fact_claim_visible() {
        let mut message = assistant_message();
        message.content = "我刚才真正的系统状态：运行时间 12 days。".to_string();

        strip_ai_evidence_claims(&mut message);

        assert_eq!(message.content, "我刚才真正的系统状态：运行时间 12 days。");
        assert!(message.turn.is_none());
    }

    #[test]
    fn result_binding_strips_structured_evidence_claim_block() {
        let mut message = assistant_message();
        message.content = concat!(
            "磁盘是 468G，已用 72G。",
            "\n<evidence_claims>",
            r#"{"claims":[{"text":"磁盘是 468G，已用 72G。","evidence":["tool-1.output"],"confidence":"verified"}]}"#,
            "</evidence_claims>"
        )
        .to_string();
        let streamed_content = message.content.clone();
        append_ai_turn_text_part(&mut message, "text", &streamed_content, false);

        strip_ai_evidence_claims(&mut message);

        assert_eq!(message.content, "磁盘是 468G，已用 72G。");
        let parts = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "磁盘是 468G，已用 72G。");
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn result_binding_drops_incomplete_evidence_claim_block() {
        let mut message = assistant_message();
        message.content = "磁盘是 468G。\n<evidence_claims>{\"claims\":[".to_string();

        strip_ai_evidence_claims(&mut message);

        assert_eq!(message.content, "磁盘是 468G。");
    }

    #[test]
    fn tool_result_fact_extraction_keeps_only_structured_execution_values() {
        let record = test_tool_execution_record("tool-1");
        let result = serde_json::json!({
            "summary": "Remote command completed.",
            "output": "Filesystem Size Used\n/ 468G 72G",
            "execution": {
                "exitCode": 0,
                "visibleInTerminal": true,
                "state": "output_captured"
            }
        });

        let facts = extract_ai_tool_result_facts(&record, Some(&result), 42);

        assert!(!facts.iter().any(|fact| fact.fact_id == "tool-1.output"));
        assert!(!facts.iter().any(|fact| fact.fact_id == "tool-1.summary"));
        assert!(
            facts
                .iter()
                .any(|fact| fact.fact_id == "tool-1.execution.exit_code")
        );
        assert!(
            facts
                .iter()
                .any(|fact| fact.fact_id == "tool-1.execution.visible_in_terminal")
        );
        assert!(
            facts
                .iter()
                .any(|fact| fact.fact_id == "tool-1.execution.state")
        );
        let diagnostic = serde_json::json!(
            facts
                .iter()
                .map(ai_tool_result_fact_json)
                .collect::<Vec<_>>()
        )
        .to_string();
        assert!(!diagnostic.contains("468G"));
        assert!(!diagnostic.contains("textHash"));
        assert!(!diagnostic.contains("outputPreview"));
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
    fn chat_message_signatures_rehash_only_the_invalidated_row() {
        let mut cache = AiChatMessageSignatureCache::default();
        let computed = std::cell::Cell::new(0usize);
        let message_ids = (0..256)
            .map(|index| format!("message-{index}"))
            .collect::<Vec<_>>();
        cache.select_conversation("conversation-1");

        for message_id in &message_ids {
            cache.signature_for(message_id, || {
                computed.set(computed.get().saturating_add(1));
                1
            });
        }
        assert_eq!(computed.get(), message_ids.len());

        for message_id in &message_ids {
            cache.signature_for(message_id, || {
                computed.set(computed.get().saturating_add(1));
                2
            });
        }
        assert_eq!(
            computed.get(),
            message_ids.len(),
            "an unchanged scroll render must reuse all message signatures"
        );

        cache.invalidate_message(&message_ids[128]);
        for message_id in &message_ids {
            cache.signature_for(message_id, || {
                computed.set(computed.get().saturating_add(1));
                3
            });
        }
        assert_eq!(
            computed.get(),
            message_ids.len() + 1,
            "a streamed update must rehash only its owning message"
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
                    original_user_count: None,
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
            turn_count: 0,
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
                && part.get("toolCallId").and_then(serde_json::Value::as_str) == Some("call-1")
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
            turn_count: 0,
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
