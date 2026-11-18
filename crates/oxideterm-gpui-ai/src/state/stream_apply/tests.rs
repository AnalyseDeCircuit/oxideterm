use std::sync::Arc;

use gpui::AppContext;
use oxideterm_ai::AiChatRole;

use super::*;
use crate::state::AiChatMessageSignatureCache;

fn assistant_message() -> AiChatMessage {
    AiChatMessage {
        is_streaming: true,
        ..test_message("assistant-1", AiChatRole::Assistant, String::new())
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

#[gpui::test]
fn runtime_evidence_records_are_entity_owned(cx: &mut gpui::TestAppContext) {
    let task_runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("AI runtime"),
    );
    let entity = cx.new(|cx| {
        AiWorkspaceEntity::new(task_runtime, oxideterm_ai::AiProviderKeyStore::new(), cx)
    });
    entity.update(cx, |entity, _cx| {
        let representative_runtime_epoch = "epoch-test-only";
        let stable_id = "4e22e673-067e-46e2-8b9f-902d7b21af4c";
        let result = serde_json::json!({
            "output": "API_KEY=supersecret123456",
            "data": { "exitCode": 0 },
            "execution": { "visibleInTerminal": true },
            "meta": { "runtimeEpoch": representative_runtime_epoch },
            "targets": [{"authority": {"resource_ref": {
                "kind": "saved_connection", "id": stable_id, "label": "Production"
            }}}],
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
        let diagnostic = ai_tool_execution_record_json(&record);
        assert_eq!(diagnostic["resourceKind"], "saved_connection");
        assert_eq!(diagnostic["executionSurface"], "visible_terminal");
        assert!(!diagnostic.to_string().contains(stable_id));
    });
}

#[gpui::test]
fn responses_history_delivery_ignores_cancelled_generations(cx: &mut gpui::TestAppContext) {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let entity =
        cx.new(|cx| AiWorkspaceEntity::new(runtime, oxideterm_ai::AiProviderKeyStore::new(), cx));
    entity.update(cx, |ai, _| {
            let conversation = ai.create_conversation("responses-conversation".into(), None, 1, None);
            ai.add_message(&conversation, test_message("assistant", AiChatRole::Assistant, "visible".into()));
            let (generation, _) = ai.begin_chat_stream(conversation.clone(), "assistant".into());
            let part = serde_json::json!({"output":[{"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"opaque"}],"results":[],"callIds":{}});
            let event = AiStreamEvent::ProviderResponsePart {provider_type:"responses:scope".into(),part:part.clone()};
            ai.apply_stream_event_state(generation, &conversation, "assistant", event.clone(), None);
            ai.cancel_chat_stream_for(&conversation);
            let (next, _) = ai.begin_chat_stream(conversation.clone(), "assistant".into());
            ai.apply_stream_event_state(generation, &conversation, "assistant", event, None);
            ai.apply_stream_event_state(next, &conversation, "assistant", AiStreamEvent::Content(" next".into()), None);
            let message = &ai.conversation_state().conversations[0].messages[0];
            assert_eq!(message.content, "visible next");
            assert_eq!(oxideterm_ai::ai_provider_parts(message,"responses:scope"),Some([part].as_slice()));
        });
}

#[gpui::test]
fn chat_batch_leaves_deliveries_after_the_host_boundary_unapplied(cx: &mut gpui::TestAppContext) {
    use crate::stream::{AiChatDeliveryBatch, AiStreamDelivery, AiStreamDeliveryEvent};

    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let entity =
        cx.new(|cx| AiWorkspaceEntity::new(runtime, oxideterm_ai::AiProviderKeyStore::new(), cx));
    entity.update(cx, |ai, _| {
        let id = ai.create_conversation("batch".into(), None, 1, None);
        ai.add_message(
            &id,
            test_message("assistant", AiChatRole::Assistant, "start: ".into()),
        );
        let (generation, _) = ai.begin_chat_stream(id.clone(), "assistant".into());
        let delivery = |event| AiStreamDelivery {
            generation,
            conversation_id: id.clone(),
            assistant_id: "assistant".into(),
            event: AiStreamDeliveryEvent::Stream(event),
        };
        let mut batch = AiChatDeliveryBatch::new(
            [
                delivery(AiStreamEvent::Content("one".into())),
                delivery(AiStreamEvent::Content(" two".into())),
                delivery(AiStreamEvent::Done),
                delivery(AiStreamEvent::Content(" should not appear".into())),
            ]
            .into(),
        );
        assert!(matches!(
            batch.next_host_delivery(ai).unwrap().event,
            AiStreamDeliveryEvent::Stream(AiStreamEvent::Done)
        ));
        batch.flush_text(ai);
        assert_eq!(
            ai.conversation_state()
                .active_conversation()
                .unwrap()
                .messages[0]
                .content,
            "start: one two"
        );

        ai.apply_stream_event_state(generation, &id, "assistant", AiStreamEvent::Done, None);
        assert!(batch.next_host_delivery(ai).is_none());
        let message = &ai
            .conversation_state()
            .active_conversation()
            .unwrap()
            .messages[0];
        assert_eq!(message.content, "start: one two");
        assert!(!message.is_streaming);
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
fn acp_session_started_ignores_stale_generation_and_persists_current_metadata() {
    let mut conversations = vec![AiConversation {
        archived: false,
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
    let current_state = ai_acp_session_state(&conversations[0]).expect("current ACP session state");
    assert!(modes_removed);
    assert_eq!(current_state.current_mode_id, None);
    assert!(current_state.available_modes.is_empty());
}

#[test]
fn acp_handoff_cursor_advances_only_for_the_matching_agent() {
    let mut conversation = AiConversation {
        archived: false,
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
        ai_acp_session_state(&conversation).and_then(|state| state.handoff_cursor),
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
fn result_binding_strips_protocol_blocks_and_preserves_visible_text() {
    for (scenario, content, expected, has_turn) in [
        (
            "plain claim",
            "我刚才真正的系统状态：运行时间 12 days。",
            "我刚才真正的系统状态：运行时间 12 days。",
            false,
        ),
        (
            "complete block",
            concat!(
                "磁盘是 468G，已用 72G。\n<evidence_claims>",
                r#"{"claims":[{"text":"磁盘是 468G，已用 72G。","evidence":["tool-1.output"],"confidence":"verified"}]}"#,
                "</evidence_claims>"
            ),
            "磁盘是 468G，已用 72G。",
            true,
        ),
        (
            "incomplete block",
            "磁盘是 468G。\n<evidence_claims>{\"claims\":[",
            "磁盘是 468G。",
            false,
        ),
    ] {
        let mut message = assistant_message();
        message.content = content.to_string();
        if has_turn {
            append_ai_turn_text_part(&mut message, "text", content, false);
        }
        strip_ai_evidence_claims(&mut message);
        assert_eq!(message.content, expected, "{scenario}");
        if has_turn {
            let parts = message.turn.as_ref().unwrap()["parts"].as_array().unwrap();
            assert_eq!(parts.len(), 1, "{scenario}");
            assert_eq!(parts[0]["type"], "text", "{scenario}");
            assert_eq!(parts[0]["text"], expected, "{scenario}");
        } else {
            assert!(message.turn.is_none(), "{scenario}");
        }
    }
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
