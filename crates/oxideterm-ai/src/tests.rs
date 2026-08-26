use std::collections::HashMap;

use serde_json::Value;

use super::*;
use crate::providers::{parse_provider_context_windows, parse_provider_models};
use crate::streaming::{
    gemini_chat_body, gemini_chat_contents, openai_chat_messages, parse_anthropic_data_line,
    parse_gemini_data_line, parse_openai_data_line,
};
use crate::{AiPolicySafetyMode, AiToolChoice, AiToolUsePolicy};

fn test_stream_config(provider_type: &str) -> AiChatStreamConfig {
    AiChatStreamConfig {
        execution_backend: AiExecutionBackend::Provider,
        provider_id: Some("provider".to_string()),
        acp_agent_id: None,
        acp_session_id: None,
        acp_config_selection: None,
        provider_type: provider_type.to_string(),
        base_url: "https://api.example.test".to_string(),
        model: "model".to_string(),
        api_key: None,
        max_response_tokens: None,
        reasoning_effort: Some("auto".to_string()),
        safety_mode: AiPolicySafetyMode::Default,
        profile_id: None,
        memory_context: None,
        memory_entry_ids: Vec::new(),
        tool_policy: AiToolUsePolicy::default(),
        tools: Vec::new(),
        tool_choice: AiToolChoice::Auto,
    }
}

fn chat_message(id: &str, role: AiChatRole, content: &str) -> AiChatMessage {
    AiChatMessage {
        id: id.to_string(),
        role,
        content: content.to_string(),
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
fn orchestrator_send_terminal_input_exposes_bounded_control_keys() {
    let tools = orchestrator_tool_definitions();
    let terminal_input = tools
        .iter()
        .find(|tool| tool.name == "send_terminal_input")
        .expect("send_terminal_input tool");
    let properties = terminal_input
        .parameters
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .expect("tool properties");

    assert!(
        terminal_input
            .description
            .contains("control/navigation key")
    );
    assert!(terminal_input.description.contains("run_command instead"));
    assert!(properties.contains_key("text"));
    assert!(properties.contains_key("append_enter"));
    assert_eq!(
        properties
            .get("key")
            .and_then(|value| value.get("enum"))
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(16)
    );
}

#[test]
fn orchestrator_skill_tools_are_bounded_read_actions() {
    let loaded = canonicalize_orchestrator_tool_arguments(
        "load_skill",
        serde_json::json!({ "id": "release-review" }),
    )
    .expect("valid skill load");
    assert_eq!(loaded["id"], "release-review");
    assert!(
        canonicalize_orchestrator_tool_arguments(
            "read_skill_resource",
            serde_json::json!({
                "id": "release-review",
                "path": "references/checklist.md",
                "unexpected": true
            }),
        )
        .is_err()
    );
    assert_eq!(
        orchestrator_risk_for_tool("load_skill", None),
        AiActionRisk::Read
    );
    assert_eq!(
        orchestrator_risk_for_tool("read_skill_resource", None),
        AiActionRisk::Read
    );
}

#[test]
fn v2_terminal_and_connection_tools_reject_legacy_target_authority() {
    let tools = orchestrator_tool_definitions();
    for name in [
        "connect_target",
        "run_command",
        "observe_terminal",
        "send_terminal_input",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .expect("v2 tool definition");
        let properties = tool
            .parameters
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("v2 tool properties");

        assert_eq!(
            tool.parameters.get("additionalProperties"),
            Some(&serde_json::json!(false)),
            "{name} must reject unknown legacy authority fields"
        );
        assert!(
            !properties.contains_key("target_id"),
            "{name} must not accept a raw runtime target id"
        );
    }
    let connect_target = tools
        .iter()
        .find(|tool| tool.name == "connect_target")
        .expect("connect target definition");
    assert_eq!(
        connect_target
            .parameters
            .pointer("/properties/resource_ref/required"),
        Some(&serde_json::json!(["kind", "id"]))
    );

    for name in [
        "read_resource",
        "write_resource",
        "transfer_resource",
        "open_app_surface",
        "get_state",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == name)
            .expect("v2 tool definition");
        let properties = tool
            .parameters
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("v2 tool properties");

        assert_eq!(
            tool.parameters.get("additionalProperties"),
            Some(&serde_json::json!(false)),
            "{name} must reject unknown legacy authority fields"
        );
        assert!(
            !properties.contains_key("target_id"),
            "{name} must not accept a raw runtime target id"
        );
    }
}

#[test]
fn orchestrator_v2_authority_inventory_covers_every_tool() {
    let mut inventory = serde_json::json!({
        "list_targets": { "authority": "discovery", "fields": [] },
        "select_target": { "authority": "discovery", "fields": [] },
        "connect_target": { "authority": "stable_resource", "fields": ["resource_ref"] },
        "run_command": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "observe_terminal": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "send_terminal_input": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "wait_terminal_output": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "get_terminal_command_status": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "read_resource": { "authority": "discriminated", "fields": ["resource_ref", "handle_id"] },
        "write_resource": { "authority": "discriminated", "fields": ["resource_ref", "handle_id"] },
        "transfer_resource": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "open_app_surface": { "authority": "discriminated", "fields": ["resource_ref", "handle_id"] },
        "get_state": { "authority": "discriminated", "fields": ["resource_ref", "handle_id"] },
        "remember_preference": { "authority": "memory_store", "fields": [] },
        "recall_preferences": { "authority": "memory_store", "fields": [] },
        "create_background_task": { "authority": "stable_resource_only", "fields": [] },
        "list_background_tasks": { "authority": "conversation_owner", "fields": [] },
        "get_background_task": { "authority": "conversation_owner", "fields": [] },
        "cancel_background_task": { "authority": "conversation_owner", "fields": [] },
        "inspect_host_tools": { "authority": "discriminated", "fields": ["resource_ref", "handle_id"] },
        "control_host_tool": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "list_forwards": { "authority": "discriminated", "fields": ["resource_ref", "handle_id"] },
        "manage_forward": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "list_plugins": { "authority": "plugin_registry", "fields": [] },
        "manage_plugin": { "authority": "plugin_registry", "fields": [] },
        "list_transport_profiles": { "authority": "profile_store", "fields": [] },
        "open_transport_profile": { "authority": "profile_store", "fields": [] },
        "get_transport_session_state": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "manage_serial_session": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "manage_telnet_session": { "authority": "runtime_handle", "fields": ["handle_id"] },
        "list_remote_desktop_sessions": { "authority": "remote_desktop_owner", "fields": [] },
        "manage_remote_desktop_session": { "authority": "remote_desktop_owner", "fields": [] },
        "get_cloud_sync_state": { "authority": "cloud_sync_owner", "fields": [] },
        "manage_cloud_sync": { "authority": "cloud_sync_owner", "fields": [] },
        "list_credentials": { "authority": "credential_metadata_store", "fields": [] },
        "manage_credential": { "authority": "credential_store", "fields": [] },
        "list_memory_entries": { "authority": "memory_store", "fields": [] },
        "manage_memory_entry": { "authority": "memory_store", "fields": [] }
    });
    inventory
        .as_object_mut()
        .expect("the contract inventory is an object")
        .insert(
            "configure_cloud_sync".to_string(),
            serde_json::json!({ "authority": "cloud_sync_owner", "fields": [] }),
        );
    inventory
        .as_object_mut()
        .expect("the contract inventory is an object")
        .insert(
            "load_skill".to_string(),
            serde_json::json!({ "authority": "skill_registry", "fields": [] }),
        );
    inventory
        .as_object_mut()
        .expect("the contract inventory is an object")
        .insert(
            "read_skill_resource".to_string(),
            serde_json::json!({ "authority": "loaded_skill", "fields": [] }),
        );
    let tools = orchestrator_tool_definitions();
    let inventory = inventory
        .as_object()
        .expect("the contract inventory is an object");

    assert_eq!(tools.len(), inventory.len());
    for tool in &tools {
        let contract = inventory
            .get(&tool.name)
            .unwrap_or_else(|| panic!("{} is missing from the v2 inventory", tool.name));
        assert_eq!(
            tool.parameters.get("additionalProperties"),
            Some(&serde_json::json!(false)),
            "{} must reject unknown authority fields",
            tool.name
        );
        let properties = tool
            .parameters
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("tool properties");
        for field in contract
            .get("fields")
            .and_then(serde_json::Value::as_array)
            .expect("authority field inventory")
        {
            let field = field.as_str().expect("authority field name");
            assert!(
                properties.contains_key(field),
                "{} must expose its declared {field} authority field",
                tool.name
            );
        }
        assert!(
            !properties.contains_key("target_id"),
            "{} must never restore v1 raw target authority",
            tool.name
        );
    }
}

#[test]
fn canonical_v2_arguments_reject_unknown_and_legacy_authority_fields() {
    for arguments in [
        serde_json::json!({
            "handle_id": "rt_current",
            "command": "pwd",
            "target_id": "terminal-session:42",
        }),
        serde_json::json!({
            "handle_id": "rt_current",
            "command": "pwd",
            "capabilities": ["terminal.run_command"],
        }),
    ] {
        assert_eq!(
            canonicalize_orchestrator_tool_arguments("run_command", arguments),
            Err(OrchestratorArgumentError::InvalidArguments)
        );
    }
}

#[test]
fn canonical_v2_arguments_enforce_discriminated_resource_authority() {
    let settings_with_live_handle = serde_json::json!({
        "resource": "settings",
        "handle_id": "rt_current",
    });
    let file_with_stable_reference = serde_json::json!({
        "resource": "file",
        "resource_ref": {
            "kind": "settings_scope",
            "id": "app",
        },
        "path": "/tmp/example.txt",
    });

    assert!(
        canonicalize_orchestrator_tool_arguments("read_resource", settings_with_live_handle)
            .is_err()
    );
    assert!(
        canonicalize_orchestrator_tool_arguments("read_resource", file_with_stable_reference)
            .is_err()
    );
}

#[test]
fn canonical_v2_file_write_preserves_the_approved_argument_object() {
    let arguments = serde_json::json!({
        "resource": "file",
        "handle_id": "rt_current",
        "path": "/tmp/example.txt",
        "content": "replacement text",
        "expected_hash": "sha256:example",
        "dry_run": true,
    });

    let canonical = canonicalize_orchestrator_tool_arguments("write_resource", arguments.clone())
        .expect("valid v2 file write");

    assert_eq!(canonical, arguments);
}

#[test]
fn creates_provider_without_secret_material() {
    let template = provider_template_by_type("openai");
    let provider = new_provider_from_template(
        template,
        generated_provider_id("openai", 42),
        "OpenAI".into(),
        42,
    );

    assert_eq!(
        provider_string(&provider, "type").as_deref(),
        Some("openai")
    );
    assert!(provider.get("defaultModel").is_none());
    assert!(provider.get("apiKey").is_none());
    assert!(provider.get("secret").is_none());
    assert_eq!(
        provider
            .get("models")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn settings_provider_mutations_stay_out_of_gpui() {
    let openai = provider_template_by_type("openai");
    let ollama = provider_template_by_type("ollama");
    let mut providers = Vec::new();
    let mut active_provider_id = None;
    let mut active_model = None;

    add_provider_from_template(
        &mut providers,
        &mut active_provider_id,
        &mut active_model,
        openai,
        "custom-openai-1".into(),
        "OpenAI".into(),
        1,
    );
    add_provider_from_template(
        &mut providers,
        &mut active_provider_id,
        &mut active_model,
        ollama,
        "custom-ollama-2".into(),
        "Ollama".into(),
        2,
    );

    assert_eq!(active_provider_id.as_deref(), Some("custom-openai-1"));
    assert_eq!(active_model, None);

    active_model = None;
    add_provider_from_template(
        &mut providers,
        &mut active_provider_id,
        &mut active_model,
        openai,
        "custom-openai-3".into(),
        "OpenAI 3".into(),
        3,
    );
    assert_eq!(active_provider_id.as_deref(), Some("custom-openai-1"));
    assert_eq!(active_model, None);

    select_provider_model(
        &mut active_provider_id,
        &mut active_model,
        "custom-ollama-2",
        "llama3.2".into(),
    );
    assert_eq!(active_provider_id.as_deref(), Some("custom-ollama-2"));
    assert_eq!(active_model.as_deref(), Some("llama3.2"));
    assert!(providers[1].get("defaultModel").is_none());

    let empty_default_provider = AiProviderView {
        id: "custom-empty".into(),
        provider_type: "openai_compatible".into(),
        name: "Empty".into(),
        base_url: "https://".into(),
        models: Vec::new(),
        enabled: true,
        custom: true,
    };
    set_active_provider_selection(
        &mut active_provider_id,
        &mut active_model,
        &empty_default_provider,
    );
    assert_eq!(active_provider_id.as_deref(), Some("custom-empty"));
    assert_eq!(active_model, None);

    let mut context_windows = serde_json::Map::new();
    assert!(!apply_provider_model_refresh(
        &mut providers,
        &mut context_windows,
        1,
        "stale-provider",
        ProviderModelRefresh {
            models: vec!["stale".into()],
            context_windows: HashMap::new(),
        },
    ));
    assert!(apply_provider_model_refresh(
        &mut providers,
        &mut context_windows,
        1,
        "custom-ollama-2",
        ProviderModelRefresh {
            models: vec!["llama3.2".into(), "qwen2.5".into()],
            context_windows: HashMap::from([("llama3.2".into(), 131_072)]),
        },
    ));
    assert_eq!(
        providers[1]
            .get("models")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        context_windows["custom-ollama-2"]["llama3.2"].as_i64(),
        Some(131_072)
    );

    let mut reasoning_provider_overrides =
        serde_json::Map::from_iter([("custom-ollama-2".into(), serde_json::json!("high"))]);
    let mut reasoning_model_overrides =
        serde_json::Map::from_iter([("custom-ollama-2".into(), serde_json::json!({}))]);
    let mut user_context_windows =
        serde_json::Map::from_iter([("custom-ollama-2".into(), serde_json::json!({}))]);
    let mut model_max_response_tokens =
        serde_json::Map::from_iter([("custom-ollama-2".into(), serde_json::json!({}))]);

    active_provider_id = Some("custom-ollama-2".into());
    let removed = remove_provider_at_with_scoped_settings(
        &mut providers,
        &mut active_provider_id,
        &mut active_model,
        &mut reasoning_provider_overrides,
        &mut reasoning_model_overrides,
        &mut user_context_windows,
        &mut model_max_response_tokens,
        1,
    );
    assert_eq!(removed.as_deref(), Some("custom-ollama-2"));
    assert_eq!(active_provider_id.as_deref(), Some("custom-openai-1"));
    assert_eq!(active_model, None);
    assert!(reasoning_provider_overrides.is_empty());
    assert!(reasoning_model_overrides.is_empty());
    assert!(user_context_windows.is_empty());
    assert!(model_max_response_tokens.is_empty());
}

#[test]
fn parses_provider_model_payloads() {
    assert_eq!(
        parse_provider_models(
            "openai",
            &serde_json::json!({
                "data": [{"id": "gpt-4o-mini"}, {"id": "gpt-4o-mini"}, {"id": "gpt-4o"}]
            })
        ),
        vec!["gpt-4o", "gpt-4o-mini", "gpt-4o-mini"]
    );
    assert_eq!(
        parse_provider_models(
            "gemini",
            &serde_json::json!({
                "models": [
                    {"name": "models/embedding-001", "supportedGenerationMethods": ["embedContent"]},
                    {"name": "models/gemini-2.0-flash", "supportedGenerationMethods": ["generateContent"]}
                ]
            })
        ),
        vec!["gemini-2.0-flash"]
    );
    assert_eq!(
        parse_provider_models(
            "ollama",
            &serde_json::json!({
                "models": [{"name": "llama3.2"}]
            })
        ),
        vec!["llama3.2"]
    );
    assert_eq!(
        parse_provider_models(
            "openai_compatible",
            &serde_json::json!({
                "models": [{"key": "model-b"}, {"id": "model-a"}, {"id": ""}, {"id": "  spaced"}]
            })
        ),
        vec!["  spaced", "model-a", "model-b"]
    );
}

#[test]
fn parses_provider_context_windows() {
    assert_eq!(
        parse_provider_context_windows(
            "openai_compatible",
            &serde_json::json!({
                "data": [
                    {"id": "model-a", "context_window": 32768},
                    {"id": "model-b", "context_length": 8192},
                    {"id": "model-zero", "context_window": 0},
                    {"id": "model-negative", "context_length": -1}
                ]
            })
        ),
        HashMap::from([
            ("model-a".to_string(), 32768),
            ("model-b".to_string(), 8192)
        ])
    );
}

#[test]
fn ai_policy_read_only_mode_allows_reads_and_denies_every_mutation_class() {
    let mut auto_approve_tools = HashMap::new();
    for key in ["write_resource:file", "run_command", "send_terminal_input"] {
        auto_approve_tools.insert(key.to_string(), true);
    }
    let policy = AiToolUsePolicy {
        enabled: true,
        auto_approve_tools,
        disabled_tools: Vec::new(),
        max_rounds: Some(10),
        max_calls_per_round: Some(8),
    };

    let read_decision = resolve_ai_policy_decision(
        "observe_terminal",
        None,
        &policy,
        AiPolicySafetyMode::ReadOnly,
        None,
    );
    assert_eq!(read_decision.decision, AiPolicyDecisionKind::Allow);

    let mutation_cases = [
        (
            "write_resource",
            serde_json::json!({ "resource": "file" }),
            AiActionRisk::Write,
        ),
        (
            "run_command",
            serde_json::json!({ "command": "pwd" }),
            AiActionRisk::Execute,
        ),
        (
            "send_terminal_input",
            serde_json::json!({ "input": "q" }),
            AiActionRisk::Interactive,
        ),
        (
            "run_command",
            serde_json::json!({ "command": "sudo reboot" }),
            AiActionRisk::Destructive,
        ),
    ];
    for (tool_name, args, expected_risk) in mutation_cases {
        let decision = resolve_ai_policy_decision(
            tool_name,
            Some(&args),
            &policy,
            AiPolicySafetyMode::ReadOnly,
            None,
        );
        assert_eq!(decision.risk, expected_risk);
        assert_eq!(decision.decision, AiPolicyDecisionKind::Deny);
        assert_eq!(decision.reason_code, "read_only_mode_denied");
    }

    assert_eq!(
        serde_json::to_value(AiPolicySafetyMode::ReadOnly).unwrap(),
        serde_json::json!("read_only")
    );
}

#[test]
fn application_tool_policy_classifies_mutations_by_action() {
    assert_eq!(
        orchestrator_risk_for_tool(
            "control_host_tool",
            Some(&serde_json::json!({ "action": "kill_session" })),
        ),
        AiActionRisk::Destructive
    );
    assert_eq!(
        orchestrator_risk_for_tool(
            "control_host_tool",
            Some(&serde_json::json!({ "action": "restart" })),
        ),
        AiActionRisk::Execute
    );
    assert_eq!(
        orchestrator_risk_for_tool(
            "manage_forward",
            Some(&serde_json::json!({ "action": "delete" })),
        ),
        AiActionRisk::Destructive
    );
    assert_eq!(
        orchestrator_risk_for_tool(
            "manage_plugin",
            Some(&serde_json::json!({ "action": "uninstall" })),
        ),
        AiActionRisk::Destructive
    );
    assert_eq!(
        orchestrator_risk_for_tool("create_background_task", None),
        AiActionRisk::Write
    );
    assert_eq!(
        orchestrator_risk_for_tool("inspect_host_tools", None),
        AiActionRisk::Read
    );
    assert_eq!(
        orchestrator_risk_for_tool("configure_cloud_sync", None),
        AiActionRisk::Write
    );
}

#[test]
fn cloud_sync_configuration_tool_accepts_non_secret_patch_and_rejects_secret_fields() {
    let patch = serde_json::json!({
        "backend_type": "http-json",
        "endpoint": "https://sync.example.test",
        "auto_upload_interval_mins": 15.0,
        "scope": {
            "sync_connections": true,
            "sync_sensitive_credentials": true,
            "app_settings_sections": ["general", "network"]
        }
    });
    assert_eq!(
        canonicalize_orchestrator_tool_arguments("configure_cloud_sync", patch.clone()),
        Ok(patch)
    );
    assert!(
        canonicalize_orchestrator_tool_arguments(
            "configure_cloud_sync",
            serde_json::json!({ "token": "must-not-cross-the-ai-boundary" })
        )
        .is_err()
    );
    assert!(
        canonicalize_orchestrator_tool_arguments(
            "configure_cloud_sync",
            serde_json::json!({ "scope": {} })
        )
        .is_err()
    );
}

#[test]
fn reasoning_effort_resolution_preserves_priority_and_exact_levels() {
    let provider_overrides = serde_json::json!({
        "provider-1": "high",
        "provider-legacy": "xhigh"
    })
    .as_object()
    .cloned()
    .unwrap();
    let model_overrides = serde_json::json!({
        "provider-1": {
            "model-a": "max"
        },
        "provider-legacy": {
            "model-old": "none"
        }
    })
    .as_object()
    .cloned()
    .unwrap();

    assert_eq!(
        resolve_ai_reasoning_effort(
            Some("off"),
            &provider_overrides,
            &model_overrides,
            Some("provider-1"),
            Some("model-a"),
        ),
        "max"
    );
    assert_eq!(
        resolve_ai_reasoning_effort(
            Some("off"),
            &provider_overrides,
            &model_overrides,
            Some("provider-1"),
            Some("model-b"),
        ),
        "high"
    );
    assert_eq!(
        resolve_ai_reasoning_effort(
            Some("medium"),
            &provider_overrides,
            &model_overrides,
            Some("provider-2"),
            Some("model-a"),
        ),
        "medium"
    );
    assert_eq!(
        resolve_ai_reasoning_effort(
            Some("minimal"),
            &provider_overrides,
            &model_overrides,
            Some("provider-3"),
            Some("model-z"),
        ),
        "minimal"
    );
    assert_eq!(
        resolve_ai_reasoning_effort(
            Some("auto"),
            &provider_overrides,
            &model_overrides,
            Some("provider-legacy"),
            Some("model-old"),
        ),
        "none"
    );
}

#[test]
fn sanitize_json_for_ai_uses_keys_to_redact_short_or_unstructured_secrets() {
    let input = serde_json::json!({
        "apiKey": "short",
        "nested": {
            "Authorization": "opaque credential",
            "output": "export TOKEN=long-secret-value",
        },
        "key": "visible-key-name",
        "usage": {
            "inputTokens": 42,
            "maxToken": 8192,
        },
        "safe": ["visible", 3],
    });

    let sanitized = sanitize_json_for_ai(&input);

    assert_eq!(sanitized["apiKey"], "[REDACTED]");
    assert_eq!(sanitized["nested"]["Authorization"], "[REDACTED]");
    assert_eq!(sanitized["nested"]["output"], "export TOKEN=[REDACTED]");
    assert_eq!(sanitized["key"], "visible-key-name");
    assert_eq!(sanitized["usage"]["inputTokens"], 42);
    assert_eq!(sanitized["usage"]["maxToken"], 8192);
    assert_eq!(sanitized["safe"], serde_json::json!(["visible", 3]));
    assert!(!sanitized.to_string().contains("opaque credential"));
}

#[test]
fn sanitize_api_messages_redacts_provider_content_without_touching_tool_calls() {
    let original = vec![
        chat_message(
            "system-1",
            AiChatRole::System,
            "Custom prompt with API_KEY=secretvalue123456789",
        ),
        AiChatMessage {
            tool_calls: vec![serde_json::json!({
                "id": "call-1",
                "name": "write_resource",
                "arguments": "{\"token\":\"secretvalue123456789\"}",
            })],
            ..chat_message("assistant-1", AiChatRole::Assistant, "")
        },
        AiChatMessage {
            tool_call_id: Some("call-1".to_string()),
            ..chat_message(
                "tool-1",
                AiChatRole::Tool,
                "{\"output\":\"AUTH_TOKEN=secretvalue123456789\"}",
            )
        },
    ];

    let sanitized = crate::context_sanitizer::sanitize_api_messages_for_provider(original.clone());

    assert_eq!(
        original[0].content,
        "Custom prompt with API_KEY=secretvalue123456789"
    );
    assert!(sanitized[0].content.contains("API_KEY=[REDACTED]"));
    assert!(!sanitized[2].content.contains("secretvalue123456789"));
    assert_eq!(sanitized[1].tool_calls, original[1].tool_calls);
}

#[test]
fn shared_provider_key_debug_is_redacted() {
    let key =
        SharedAiProviderKey::new(zeroize::Zeroizing::new("provider-secret-value".to_string()));

    let debug = format!("{key:?}");

    assert_eq!(debug, "SharedAiProviderKey(<redacted>)");
    assert!(!debug.contains("provider-secret-value"));
}

#[test]
fn slash_help_and_request_overrides_are_core_logic() {
    let help = ai_help_markdown(|key| format!("desc:{key}"));
    assert!(help.contains("`/help`"));
    assert!(help.contains("desc:ai.slash.help_desc"));

    let command = resolve_ai_slash_command("fix").unwrap();
    let prompt = slash_task_system_prompt(command).unwrap();
    assert!(prompt.contains("## Task Mode: /fix"));
    let parsed = parse_ai_user_input("/fix @terminal bad command");
    let combined = ai_input_system_prompt(Some(command), &parsed.participants).unwrap();
    assert!(combined.contains("## Task Mode: /fix"));
    assert!(combined.contains("## Active Participants"));
    assert!(combined.contains("preferred_target_view=live_sessions"));

    let mut history = vec![chat_message("u1", AiChatRole::User, "/fix bad command")];
    apply_chat_request_overrides(
        &mut history,
        Some("bad command".into()),
        Some(prompt.clone()),
    );
    assert_eq!(history[0].role, AiChatRole::System);
    assert_eq!(history[0].content, prompt);
    assert_eq!(history[1].content, "bad command");
}

#[test]
fn parses_and_sanitizes_follow_up_suggestions() {
    let parsed = parse_ai_suggestions(
        "Answer\n<suggestions>\n<s icon=\"Zap\">Run deploy</s>\n<s icon=\"Search\">Show logs</s>\n</suggestions>",
    );
    assert_eq!(parsed.clean_content, "Answer");
    assert!(parsed.has_suggestions_block);
    assert_eq!(parsed.suggestions.len(), 2);
    assert_eq!(parsed.suggestions[0].icon, "Zap");
    assert_eq!(parsed.suggestions[0].text, "Run deploy");

    assert_eq!(
        ai_visible_suggestion_content("Answer\n<suggestions>\n<s icon=\"Zap\">..."),
        "Answer"
    );

    let parsed = parse_ai_suggestions(
        "Answer\n<suggestions>\n<s icon=\"Search\"></s>\n<s icon=\"Bug\">   </s>\n</suggestions>",
    );

    assert!(parsed.has_suggestions_block);
    assert_eq!(parsed.clean_content, "Answer");
    assert!(parsed.suggestions.is_empty());
    assert_eq!(
        ai_visible_suggestion_content(
            "Answer\n<suggestions>\n<s icon=\"Search\"></s>\n</suggestions>"
        ),
        "Answer"
    );

    let localized_text = "检查连接状态".repeat(25);
    let parsed = parse_ai_suggestions(&format!(
        "Answer\n<suggestions>\n<s icon=\"Search\">{localized_text}</s>\n</suggestions>",
    ));

    assert!(parsed.has_suggestions_block);
    assert_eq!(parsed.clean_content, "Answer");
    assert_eq!(parsed.suggestions.len(), 1);
    assert_eq!(parsed.suggestions[0].text, localized_text);
}

#[test]
fn chat_request_overrides_inject_current_context_as_system_message() {
    let mut history = vec![AiChatMessage {
        id: "u1".into(),
        role: AiChatRole::User,
        content: "#buffer explain".into(),
        timestamp_ms: 1,
        model: None,
        context: Some("--- #buffer ---\nerror output".into()),
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
    }];

    apply_chat_request_overrides(&mut history, Some("explain".into()), None);

    assert_eq!(history[0].role, AiChatRole::System);
    assert!(history[0].content.starts_with("Current terminal context:"));
    assert!(history[0].content.contains("--- #buffer ---"));
    assert_eq!(history[1].content, "explain");
}

#[test]
fn chat_persistence_missing_file_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = AiChatPersistenceStore::new(dir.path().join("missing.redb"));

    assert_eq!(store.load_state().unwrap(), AiChatState::default());
}

#[test]
fn chat_persistence_save_state_rejects_stale_projection_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id =
        state.create_conversation("conversation-stale".into(), Some("Stale".into()), 42, None);
    state.add_message(
        &conversation_id,
        AiChatMessage {
            id: "assistant-1".into(),
            role: AiChatRole::Assistant,
            content: "fresh projection".into(),
            timestamp_ms: 43,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: Some(serde_json::json!({
                "id": "assistant-1",
                "status": "complete",
                "parts": [{ "type": "text", "text": "fresh projection" }],
                "toolRounds": [],
                "plainTextSummary": "fresh projection",
            })),
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
    );

    store
        .save_state_with_projection_updated_at(state.clone(), 2_000)
        .unwrap();

    let mut stale_state = state.clone();
    stale_state.update_message(&conversation_id, "assistant-1", |message| {
        message.content = "stale projection".into();
        message.turn = Some(serde_json::json!({
            "id": "assistant-1",
            "status": "complete",
            "parts": [{ "type": "text", "text": "stale projection" }],
            "toolRounds": [],
            "plainTextSummary": "stale projection",
        }));
    });
    store
        .save_state_with_projection_updated_at(stale_state.clone(), 1_500)
        .unwrap();

    let loaded = store.load_state().unwrap();
    let message = loaded.conversations[0].messages[0].clone();
    assert_eq!(message.content, "fresh projection");
    assert_eq!(
        message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(serde_json::Value::as_array)
            .and_then(|parts| parts.first())
            .and_then(|part| part.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("fresh projection")
    );

    stale_state.update_message(&conversation_id, "assistant-1", |message| {
        message.content = "newer projection".into();
    });
    store
        .save_state_with_projection_updated_at(stale_state.clone(), 2_500)
        .unwrap();
    let loaded = store.load_state().unwrap();
    assert_eq!(
        loaded.conversations[0].messages[0].content,
        "newer projection"
    );
}

#[test]
fn chat_persistence_hydrates_interrupted_stream_as_closed_turn() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id = state.create_conversation(
        "conversation-interrupted".into(),
        Some("Interrupted".into()),
        1,
        None,
    );
    state.add_message(
        &conversation_id,
        AiChatMessage {
            id: "assistant-1".into(),
            role: AiChatRole::Assistant,
            content: "Partial answer".into(),
            timestamp_ms: 2,
            model: Some("deepseek-v4-pro".into()),
            context: None,
            is_streaming: true,
            thinking_content: Some("working".into()),
            metadata: None,
            tool_call_id: None,
            tool_calls: vec![serde_json::json!({
                "id": "call-1",
                "name": "get_state",
                "arguments": "{}",
                "status": "running",
                "result": serde_json::Value::Null,
            })],
            turn: Some(serde_json::json!({
                "id": "assistant-1",
                "status": "streaming",
                "parts": [
                    { "type": "thinking", "text": "working", "streaming": true },
                    { "type": "tool_call", "id": "call-1", "name": "get_state", "argumentsText": "{}", "status": "complete" },
                    { "type": "text", "text": "Partial answer" }
                ],
                "toolRounds": [{
                    "id": "assistant-1-round-1",
                    "round": 1,
                    "toolCalls": [{
                        "id": "call-1",
                        "name": "get_state",
                        "argumentsText": "{}",
                        "executionState": "running"
                    }]
                }],
                "plainTextSummary": "Partial answer",
            })),
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
    );

    store.save_state(state.clone()).unwrap();

    let loaded = store.load_state().unwrap();
    let message = &loaded.conversations[0].messages[0];
    assert!(!message.is_streaming);
    let turn = message.turn.as_ref().expect("turn");
    assert_eq!(
        turn.get("status").and_then(serde_json::Value::as_str),
        Some("complete")
    );
    assert_eq!(
        turn.pointer("/parts/0/streaming")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        message.tool_calls[0]
            .get("status")
            .and_then(serde_json::Value::as_str),
        Some("rejected")
    );
    assert_eq!(
        turn.pointer("/toolRounds/0/toolCalls/0/approvalState")
            .and_then(serde_json::Value::as_str),
        Some("rejected")
    );
}

#[test]
fn chat_persistence_replays_completed_stream_turn_and_transcript_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id =
        state.create_conversation("conversation-replay".into(), Some("Replay".into()), 1, None);
    state.add_message(
        &conversation_id,
        AiChatMessage {
            id: "user-1".into(),
            role: AiChatRole::User,
            content: "open terminal".into(),
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
    );
    state.add_message(
        &conversation_id,
        AiChatMessage {
            id: "assistant-1".into(),
            role: AiChatRole::Assistant,
            content: "Opened.".into(),
            timestamp_ms: 3,
            model: Some("deepseek-v4-pro".into()),
            context: None,
            is_streaming: false,
            thinking_content: Some("Need a terminal".into()),
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: Some(serde_json::json!({
                "id": "assistant-1",
                "status": "complete",
                "parts": [
                    { "type": "thinking", "text": "Need a terminal", "streaming": false },
                    { "type": "tool_call", "id": "call-1", "name": "open_app_surface", "argumentsText": "{\"surface\":\"local_terminal\"}", "status": "complete" },
                    { "type": "tool_result", "toolCallId": "call-1", "toolName": "open_app_surface", "success": true, "output": "opened", "durationMs": 12 },
                    { "type": "text", "text": "Opened." }
                ],
                "toolRounds": [{
                    "id": "assistant-1-round-1",
                    "round": 1,
                    "toolCalls": [{
                        "id": "call-1",
                        "name": "open_app_surface",
                        "argumentsText": "{\"surface\":\"local_terminal\"}",
                        "executionState": "completed"
                    }]
                }],
                "plainTextSummary": "Opened.",
            })),
            transcript_ref: Some(serde_json::json!({
                "conversationId": "conversation-replay",
                "startEntryId": "transcript-user-user-1",
                "endEntryId": "assistant-1",
            })),
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
    );

    store.save_state(state.clone()).unwrap();
    store
        .append_transcript_entries(
            &conversation_id,
            vec![
                PersistedTranscriptEntry {
                    id: "transcript-user-user-1".into(),
                    conversation_id: conversation_id.clone(),
                    turn_id: None,
                    parent_id: None,
                    timestamp: 2,
                    kind: "user_message".into(),
                    payload: serde_json::json!({ "messageId": "user-1", "role": "user", "content": "open terminal" }),
                },
                PersistedTranscriptEntry {
                    id: "transcript-assistant-start-assistant-1".into(),
                    conversation_id: conversation_id.clone(),
                    turn_id: Some("assistant-1".into()),
                    parent_id: Some("user-1".into()),
                    timestamp: 3,
                    kind: "assistant_turn_start".into(),
                    payload: serde_json::json!({ "messageId": "assistant-1", "requestMessageId": "user-1" }),
                },
                PersistedTranscriptEntry {
                    id: "transcript-tool-call-call-1".into(),
                    conversation_id: conversation_id.clone(),
                    turn_id: Some("assistant-1".into()),
                    parent_id: Some("assistant-1-round-1".into()),
                    timestamp: 4,
                    kind: "tool_call".into(),
                    payload: serde_json::json!({ "id": "call-1", "name": "open_app_surface", "argumentsText": "{\"surface\":\"local_terminal\"}", "roundId": "assistant-1-round-1" }),
                },
                PersistedTranscriptEntry {
                    id: "transcript-tool-result-call-1".into(),
                    conversation_id: conversation_id.clone(),
                    turn_id: Some("assistant-1".into()),
                    parent_id: Some("call-1".into()),
                    timestamp: 5,
                    kind: "tool_result".into(),
                    payload: serde_json::json!({ "toolCallId": "call-1", "toolName": "open_app_surface", "success": true, "output": "opened", "roundId": "assistant-1-round-1" }),
                },
                PersistedTranscriptEntry {
                    id: "transcript-assistant-parts-assistant-1".into(),
                    conversation_id: conversation_id.clone(),
                    turn_id: Some("assistant-1".into()),
                    parent_id: Some("assistant-1".into()),
                    timestamp: 6,
                    kind: "assistant_part".into(),
                    payload: serde_json::json!({ "completeTurnParts": true }),
                },
                PersistedTranscriptEntry {
                    id: "transcript-assistant-end-assistant-1".into(),
                    conversation_id: conversation_id.clone(),
                    turn_id: Some("assistant-1".into()),
                    parent_id: Some("assistant-1".into()),
                    timestamp: 7,
                    kind: "assistant_turn_end".into(),
                    payload: serde_json::json!({ "messageId": "assistant-1", "status": "complete", "plainTextSummary": "Opened.", "toolRoundCount": 1 }),
                },
            ],
        )
        .unwrap();

    let loaded = store.load_state().unwrap();
    let assistant = &loaded.conversations[0].messages[1];
    let part_types = assistant
        .turn
        .as_ref()
        .and_then(|turn| turn.get("parts"))
        .and_then(serde_json::Value::as_array)
        .expect("parts")
        .iter()
        .map(|part| {
            part.get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        part_types,
        vec!["thinking", "tool_call", "tool_result", "text"]
    );

    drop(store);
    let db = redb::Database::create(&path).unwrap();
    let read = db.begin_read().unwrap();
    let transcript_index = read
        .open_table(redb::TableDefinition::<&str, &[u8]>::new(
            "conversation_transcript",
        ))
        .unwrap();
    let transcript_table = read
        .open_table(redb::TableDefinition::<&str, &[u8]>::new(
            "ai_chat_transcript",
        ))
        .unwrap();
    let ids: Vec<String> = rmp_serde::from_slice(
        transcript_index
            .get("conversation-replay")
            .unwrap()
            .unwrap()
            .value(),
    )
    .unwrap();
    let kinds = ids
        .iter()
        .map(|id| {
            let entry: PersistedTranscriptEntry =
                rmp_serde::from_slice(transcript_table.get(id.as_str()).unwrap().unwrap().value())
                    .unwrap();
            entry.kind
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            "user_message",
            "assistant_turn_start",
            "tool_call",
            "tool_result",
            "assistant_part",
            "assistant_turn_end",
        ]
    );
}

#[test]
fn chat_persistence_appends_transcript_and_diagnostic_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let transcript = PersistedTranscriptEntry {
        id: "tr-1".into(),
        conversation_id: "conversation-1".into(),
        turn_id: Some("assistant-1".into()),
        parent_id: Some("message-1".into()),
        timestamp: 44,
        kind: "assistant_turn_start".into(),
        payload: serde_json::json!({ "messageId": "assistant-1" }),
    };
    let diagnostic = PersistedDiagnosticEvent {
        id: "diag-1".into(),
        conversation_id: "conversation-1".into(),
        turn_id: Some("assistant-1".into()),
        round_id: None,
        timestamp: 45,
        event_type: "budget_level_changed".into(),
        data: serde_json::json!({ "source": "sidebar", "nextLevel": 2 }),
    };

    store
        .append_transcript_entries("conversation-1", vec![transcript])
        .unwrap();
    store
        .append_diagnostic_events("conversation-1", vec![diagnostic.clone()])
        .unwrap();
    store
        .append_diagnostic_events("conversation-1", vec![diagnostic.clone()])
        .unwrap();

    let tail = store.diagnostic_tail("conversation-1", 10).unwrap();
    assert_eq!(tail, vec![diagnostic]);

    drop(store);
    let db = redb::Database::create(&path).unwrap();
    let read = db.begin_read().unwrap();
    let transcript_index = read
        .open_table(redb::TableDefinition::<&str, &[u8]>::new(
            "conversation_transcript",
        ))
        .unwrap();
    let transcript_ids: Vec<String> = rmp_serde::from_slice(
        transcript_index
            .get("conversation-1")
            .unwrap()
            .unwrap()
            .value(),
    )
    .unwrap();
    assert_eq!(transcript_ids, vec!["tr-1"]);
    let diagnostic_index = read
        .open_table(redb::TableDefinition::<&str, &[u8]>::new(
            "conversation_diagnostic_events",
        ))
        .unwrap();
    let diagnostic_ids: Vec<String> = rmp_serde::from_slice(
        diagnostic_index
            .get("conversation-1")
            .unwrap()
            .unwrap()
            .value(),
    )
    .unwrap();
    assert_eq!(diagnostic_ids, vec!["diag-1"]);
}

#[test]
fn chat_persistence_redacts_transcript_and_diagnostic_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    store
        .append_transcript_entries(
            "conversation-secret",
            vec![PersistedTranscriptEntry {
                id: "tr-secret".into(),
                conversation_id: "conversation-secret".into(),
                turn_id: Some("assistant-secret".into()),
                parent_id: None,
                timestamp: 44,
                kind: "tool_result".into(),
                payload: serde_json::json!({
                    "apiKey": "short-secret",
                    "output": "export TOKEN=transcript-secret-value",
                }),
            }],
        )
        .unwrap();
    store
        .append_diagnostic_events(
            "conversation-secret",
            vec![PersistedDiagnosticEvent {
                id: "diag-secret".into(),
                conversation_id: "conversation-secret".into(),
                turn_id: Some("assistant-secret".into()),
                round_id: None,
                timestamp: 45,
                event_type: "tool_result".into(),
                data: serde_json::json!({
                    "Authorization": "Bearer diagnostic-secret-value",
                }),
            }],
        )
        .unwrap();

    let diagnostic = store
        .diagnostic_tail("conversation-secret", 1)
        .unwrap()
        .pop()
        .expect("diagnostic");
    assert_eq!(diagnostic.data["Authorization"], "[REDACTED]");
    assert!(
        !diagnostic
            .data
            .to_string()
            .contains("diagnostic-secret-value")
    );

    drop(store);
    let db = redb::Database::create(&path).unwrap();
    let read = db.begin_read().unwrap();
    // Inspect the durable row rather than only the caller-owned input.
    let transcript_table = read
        .open_table(redb::TableDefinition::<&str, &[u8]>::new(
            "ai_chat_transcript",
        ))
        .unwrap();
    let stored = transcript_table.get("tr-secret").unwrap().unwrap();
    let transcript: PersistedTranscriptEntry = rmp_serde::from_slice(stored.value()).unwrap();
    assert_eq!(transcript.payload["apiKey"], "[REDACTED]");
    assert!(!transcript.payload.to_string().contains("short-secret"));
    assert!(
        !transcript
            .payload
            .to_string()
            .contains("transcript-secret-value")
    );
}

#[test]
fn chat_persistence_redacts_conversation_projection_before_storage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id = state.create_conversation(
        "conversation-secret".into(),
        Some("API_KEY=title-secret-value".into()),
        42,
        None,
    );
    let mut message = chat_message(
        "assistant-secret",
        AiChatRole::Assistant,
        "export TOKEN=content-secret-value",
    );
    message.context = Some("Authorization: Bearer context-secret-value".into());
    message.thinking_content = Some("password=thinking-secret-value".into());
    message.tool_calls = vec![serde_json::json!({
        "id": "tool-secret",
        "arguments": "{\"apiKey\":\"short\"}",
    })];
    message.turn = Some(serde_json::json!({
        "parts": [{
            "type": "guardrail",
            "rawText": "PRIVATE_KEY=turn-secret-value",
        }],
    }));
    state.add_message(&conversation_id, message);
    state.conversations[0].session_metadata = Some(serde_json::json!({
        "authToken": "metadata-secret-value",
    }));

    store.save_state(state).unwrap();

    let loaded = store.load_state().unwrap();
    let retained = format!("{loaded:?}");
    for secret in [
        "title-secret-value",
        "content-secret-value",
        "context-secret-value",
        "thinking-secret-value",
        "\"short\"",
        "turn-secret-value",
        "metadata-secret-value",
    ] {
        assert!(!retained.contains(secret));
    }
    assert!(retained.contains("[REDACTED]"));
}

#[test]
fn legacy_runtime_history_fixture_is_readable_and_non_actionable() {
    const LEGACY_HANDLE: &str = "rt_0123456789abcdef0123456789abcdef";
    const FIXTURE_SECRET: &str = "fixture-secret-value";
    let state: AiChatState =
        serde_json::from_str(include_str!("testdata/legacy_conversation_2_0_13.json"))
            .expect("the checked-in legacy fixture is valid");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);

    store.save_state(state).unwrap();
    let loaded = store.load_state().unwrap();
    let conversation = &loaded.conversations[0];
    assert_eq!(conversation.title, "Legacy runtime history");
    assert!(
        conversation.messages[1]
            .content
            .contains("completed successfully"),
        "legacy visible history remains readable"
    );
    let tool_call = conversation.messages[1].tool_calls[0]
        .as_object()
        .expect("persisted tool call");
    assert_eq!(tool_call.get("historical"), Some(&serde_json::json!(true)));
    assert_eq!(tool_call.get("actionable"), Some(&serde_json::json!(false)));

    let projection = serde_json::to_string(&loaded).unwrap();
    for forbidden in [
        LEGACY_HANDLE,
        FIXTURE_SECRET,
        "\"target_id\"",
        "\"targetId\"",
        "\"sessionId\"",
        "\"node_id\"",
        "\"nodeId\"",
        "\"tab_id\"",
        "\"tabId\"",
        "\"pane_id\"",
        "\"paneId\"",
        "\"runtimeEpoch\"",
    ] {
        assert!(
            !projection.contains(forbidden),
            "persisted projection retained forbidden runtime authority: {forbidden}"
        );
    }

    store.save_state(loaded.clone()).unwrap();
    assert_eq!(
        store.load_state().unwrap(),
        loaded,
        "projection-time migration must be idempotent"
    );
    drop(store);
    let stored_bytes = std::fs::read(path).unwrap();
    assert!(
        !stored_bytes
            .windows(LEGACY_HANDLE.len())
            .any(|window| window == LEGACY_HANDLE.as_bytes())
    );
    assert!(
        !stored_bytes
            .windows(FIXTURE_SECRET.len())
            .any(|window| window == FIXTURE_SECRET.as_bytes())
    );
}

#[test]
fn chat_persistence_hydrates_round_summaries_from_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id =
        state.create_conversation("conversation-1".into(), Some("Hello".into()), 42, None);
    let mut assistant = chat_message("assistant-1", AiChatRole::Assistant, "answer");
    assistant.turn = Some(serde_json::json!({
        "id": "assistant-1",
        "status": "complete",
        "plainTextSummary": "answer",
        "parts": [],
        "toolRounds": [{
            "id": "assistant-1-round-1",
            "round": 1,
            "toolCalls": [],
        }],
        "pendingSummaries": [],
    }));
    state.add_message(
        &conversation_id,
        chat_message("user-1", AiChatRole::User, "hello"),
    );
    state.add_message(&conversation_id, assistant);
    store.save_state(state.clone()).unwrap();
    store
        .append_transcript_entries(
            &conversation_id,
            vec![PersistedTranscriptEntry {
                id: "summary-1".into(),
                conversation_id: conversation_id.clone(),
                turn_id: Some("assistant-1".into()),
                parent_id: Some("assistant-1-round-1".into()),
                timestamp: 45,
                kind: "summary_created".into(),
                payload: serde_json::json!({
                    "messageId": "assistant-1",
                    "summaryText": "run_command: ok - printed cwd",
                    "summaryKind": "round",
                    "roundId": "assistant-1-round-1",
                    "source": "background",
                    "summarizationMode": "background",
                    "contextLengthBefore": 256,
                }),
            }],
        )
        .unwrap();

    let loaded = store.load_conversation(&conversation_id).unwrap().unwrap();
    let assistant = loaded
        .messages
        .iter()
        .find(|message| message.id == "assistant-1")
        .expect("assistant message");
    let turn = assistant.turn.as_ref().expect("assistant turn");
    let round = &turn
        .get("toolRounds")
        .and_then(serde_json::Value::as_array)
        .expect("tool rounds")[0];
    assert_eq!(round["summary"], "run_command: ok - printed cwd");
    assert_eq!(round["summaryMetadata"]["contextLengthBefore"], 256);
    assert_eq!(
        turn.get("pendingSummaries")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn chat_persistence_preserves_message_branches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id = state.create_conversation(
        "conversation-branches".into(),
        Some("Branch".into()),
        42,
        None,
    );
    let mut edited = chat_message("message-live", AiChatRole::User, "new prompt");
    edited.branches = Some(AiMessageBranches {
        total: 2,
        active_index: 1,
        tails: HashMap::from([(
            0,
            vec![
                chat_message("message-old", AiChatRole::User, "old prompt"),
                chat_message("reply-old", AiChatRole::Assistant, "old reply"),
            ],
        )]),
    });
    state.add_message(&conversation_id, edited);

    store.save_state(state).unwrap();
    let reloaded = store.load_state().unwrap();
    let message = &reloaded.conversations[0].messages[0];
    let branches = message.branches.as_ref().unwrap();
    assert_eq!(branches.total, 2);
    assert_eq!(branches.active_index, 1);
    assert_eq!(branches.tails[&0][0].content, "old prompt");
    assert_eq!(branches.tails[&0][1].content, "old reply");
}

#[test]
fn chat_persistence_preserves_follow_up_suggestions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id = state.create_conversation(
        "conversation-suggestions".into(),
        Some("Suggestions".into()),
        42,
        None,
    );
    let mut reply = chat_message("reply", AiChatRole::Assistant, "Answer");
    reply.suggestions = vec![AiFollowUpSuggestion {
        icon: "Zap".into(),
        text: "Run deploy".into(),
    }];
    state.add_message(&conversation_id, reply);

    store.save_state(state).unwrap();
    let reloaded = store.load_state().unwrap();
    let message = &reloaded.conversations[0].messages[0];
    assert_eq!(message.suggestions.len(), 1);
    assert_eq!(message.suggestions[0].icon, "Zap");
    assert_eq!(message.suggestions[0].text, "Run deploy");
}

#[test]
fn chat_persistence_loads_metadata_first_and_conversation_on_demand() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let older = state.create_conversation("older".into(), Some("Older".into()), 1, None);
    state.add_message(
        &older,
        chat_message("older-message", AiChatRole::User, "old"),
    );
    let newer = state.create_conversation("newer".into(), Some("Newer".into()), 3, None);
    state.add_message(
        &newer,
        chat_message("newer-message", AiChatRole::User, "new"),
    );
    store.save_state(state).unwrap();

    let reloaded = store.load_state().unwrap();
    assert_eq!(reloaded.active_conversation_id.as_deref(), Some("newer"));
    assert!(reloaded.conversations[0].messages_loaded);
    assert_eq!(reloaded.conversations[0].messages[0].content, "new");
    assert!(!reloaded.conversations[1].messages_loaded);
    assert!(reloaded.conversations[1].messages.is_empty());
    assert_eq!(reloaded.conversations[1].message_count, 1);

    let older_full = store.load_conversation("older").unwrap().unwrap();
    assert!(older_full.messages_loaded);
    assert_eq!(older_full.messages[0].content, "old");
}

#[test]
fn chat_persistence_keeps_more_than_legacy_conversation_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();

    // Conversation metadata is cheap to load and must not be truncated because
    // a later full-state save treats absent IDs as explicit deletions.
    for index in 0..125 {
        state.create_conversation(
            format!("conversation-{index}"),
            Some(format!("Conversation {index}")),
            index,
            None,
        );
    }
    store.save_state(state).unwrap();

    let reloaded = store.load_state().unwrap();
    assert_eq!(reloaded.conversations.len(), 125);
    store.save_state(reloaded).unwrap();
    assert!(store.load_conversation("conversation-0").unwrap().is_some());
}

#[test]
fn chat_persistence_keeps_more_than_legacy_message_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_history.redb");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id =
        state.create_conversation("long-conversation".into(), Some("Long".into()), 1, None);

    // Local retention is intentionally wider than prompt history. The prompt
    // budget and automatic compaction remain responsible for model input size.
    for index in 0..250 {
        state.add_message(
            &conversation_id,
            chat_message(
                &format!("message-{index}"),
                AiChatRole::User,
                &format!("content-{index}"),
            ),
        );
    }
    store.save_state(state).unwrap();

    let reloaded = store.load_conversation(&conversation_id).unwrap().unwrap();
    assert_eq!(reloaded.messages.len(), 250);
    assert_eq!(reloaded.messages.first().unwrap().id, "message-0");
}

#[test]
fn openai_stream_parser_extracts_content_and_done() {
    let parsed = parse_openai_data_line(
        r#"data: {"choices":[{"delta":{"content":"hello"},"finish_reason":null}]}"#,
    );
    assert!(parsed.saw_frame);
    assert_eq!(parsed.events, vec![AiStreamEvent::Content("hello".into())]);

    let done = parse_openai_data_line("data: [DONE]");
    assert_eq!(done.events, vec![AiStreamEvent::Done]);
}

fn assistant_tool_call_message(
    id: &str,
    content: &str,
    thinking_content: Option<&str>,
    tool_call_id: &str,
) -> AiChatMessage {
    AiChatMessage {
        id: id.into(),
        role: AiChatRole::Assistant,
        content: content.into(),
        timestamp_ms: 2,
        model: None,
        context: None,
        is_streaming: false,
        thinking_content: thinking_content.map(str::to_string),
        metadata: None,
        tool_call_id: None,
        tool_calls: vec![serde_json::json!({
            "id": tool_call_id,
            "name": "open_app_surface",
            "arguments": "{\"surface\":\"local_terminal\"}"
        })],
        turn: None,
        transcript_ref: None,
        summary_ref: None,
        branches: None,
        suggestions: Vec::new(),
    }
}

fn tool_result_message(id: &str, tool_call_id: &str) -> AiChatMessage {
    AiChatMessage {
        id: id.into(),
        role: AiChatRole::Tool,
        content: "{\"ok\":true}".into(),
        timestamp_ms: 3,
        model: None,
        context: None,
        is_streaming: false,
        thinking_content: None,
        metadata: None,
        tool_call_id: Some(tool_call_id.into()),
        tool_calls: Vec::new(),
        turn: None,
        transcript_ref: None,
        summary_ref: None,
        branches: None,
        suggestions: Vec::new(),
    }
}

#[test]
fn deepseek_tool_subturn_preserves_reasoning_only_after_latest_user() {
    let messages = vec![
        chat_message("u1", AiChatRole::User, "old request"),
        assistant_tool_call_message("a1", "", Some("old reasoning"), "old-call"),
        tool_result_message("t1", "old-call"),
        chat_message("u2", AiChatRole::User, "please open a terminal"),
        assistant_tool_call_message("a2", "", Some("current reasoning"), "current-call"),
        tool_result_message("t2", "current-call"),
    ];

    let converted = openai_chat_messages(&test_stream_config("deepseek"), &messages);
    assert!(converted[1].get("reasoning_content").is_none());
    assert_eq!(
        converted[4]["reasoning_content"].as_str(),
        Some("current reasoning")
    );
}

#[test]
fn openai_compatible_tool_subturn_preserves_reasoning_for_kimi_style_models() {
    let messages = vec![
        chat_message("u1", AiChatRole::User, "old request"),
        assistant_tool_call_message("a1", "", Some("kimi reasoning"), "call-1"),
        tool_result_message("t1", "call-1"),
        chat_message("u2", AiChatRole::User, "next request"),
    ];

    let converted = openai_chat_messages(&test_stream_config("openai_compatible"), &messages);
    assert_eq!(
        converted[1]["reasoning_content"].as_str(),
        Some("kimi reasoning")
    );
}

#[test]
fn kimi_preserves_reasoning_for_assistant_turns_without_tools() {
    let mut assistant = chat_message("a1", AiChatRole::Assistant, "answer");
    assistant.thinking_content = Some("kimi reasoning".to_string());

    let mut config = test_stream_config("kimi");
    config.model = "kimi-k3".to_string();
    let converted = openai_chat_messages(&config, &[assistant]);
    assert_eq!(
        converted[0]["reasoning_content"].as_str(),
        Some("kimi reasoning")
    );
}

#[test]
fn gemini_messages_merge_roles_and_system_instruction() {
    let messages = vec![
        chat_message("1", AiChatRole::System, "sys"),
        chat_message("2", AiChatRole::User, "one"),
        chat_message("3", AiChatRole::User, "two"),
        chat_message("4", AiChatRole::Assistant, "answer"),
    ];
    let (system, contents) = gemini_chat_contents(&messages);
    assert_eq!(system.as_deref(), Some("sys"));
    assert_eq!(contents[0]["role"], "user");
    assert_eq!(contents[0]["parts"][0]["text"], "one");
    assert_eq!(contents[0]["parts"][1]["text"], "two");
    assert_eq!(contents[1]["role"], "model");

    let messages = vec![
        chat_message("1", AiChatRole::System, "sys"),
        chat_message("2", AiChatRole::System, ""),
        chat_message("3", AiChatRole::User, "one"),
    ];
    let (system, _) = gemini_chat_contents(&messages);
    assert_eq!(system.as_deref(), Some("sys\n\n"));

    let messages = vec![
        chat_message("1", AiChatRole::System, ""),
        chat_message("2", AiChatRole::System, "sys"),
        chat_message("3", AiChatRole::User, "one"),
    ];
    let (system, _) = gemini_chat_contents(&messages);
    assert_eq!(system.as_deref(), Some("sys"));

    let body = gemini_chat_body(
        &test_stream_config("gemini"),
        &[chat_message("1", AiChatRole::System, "")],
    );
    assert!(body.get("system_instruction").is_none());
}

#[test]
fn anthropic_and_gemini_stream_parsers_extract_content() {
    let anthropic = parse_anthropic_data_line(
        r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}"#,
    );
    assert_eq!(anthropic.events, vec![AiStreamEvent::Content("hi".into())]);

    let gemini = parse_gemini_data_line(
        r#"data: {"candidates":[{"content":{"parts":[{"text":"hello"}]}}]}"#,
    );
    assert_eq!(
        gemini.events,
        vec![
            AiStreamEvent::ProviderResponsePart {
                provider_type: "gemini".to_string(),
                part: serde_json::json!({"text": "hello"}),
            },
            AiStreamEvent::Content("hello".into()),
        ]
    );

    let gemini_tool = parse_gemini_data_line(
        r#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_state","args":{"scope":"active"}}}]}}]}"#,
    );
    assert_eq!(gemini_tool.events.len(), 2);
    match &gemini_tool.events[1] {
        AiStreamEvent::ToolCallComplete {
            name, arguments, ..
        } => {
            assert_eq!(name, "get_state");
            assert_eq!(arguments, "{\"scope\":\"active\"}");
        }
        other => panic!("expected Gemini tool call, got {other:?}"),
    }

    let gemini_array_tool = parse_gemini_data_line(
        r#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_state","args":["scope","active"]}}]}}]}"#,
    );
    match &gemini_array_tool.events[1] {
        AiStreamEvent::ToolCallComplete { arguments, .. } => {
            assert_eq!(arguments, "[\"scope\",\"active\"]");
        }
        other => panic!("expected Gemini tool call, got {other:?}"),
    }

    let gemini_empty_string_tool = parse_gemini_data_line(
        r#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_state","args":""}}]}}]}"#,
    );
    match &gemini_empty_string_tool.events[1] {
        AiStreamEvent::ToolCallComplete { arguments, .. } => {
            assert_eq!(arguments, "{}");
        }
        other => panic!("expected Gemini tool call, got {other:?}"),
    }
}

#[test]
fn gemini_signed_parts_round_trip_without_rebuilding() {
    let line = r#"data: {"candidates":[{"content":{"parts":[{"text":"checking","thoughtSignature":"text-signature"},{"functionCall":{"name":"get_state","args":{"scope":"active"}},"thoughtSignature":"call-signature"}]}}]}"#;
    let parsed = parse_gemini_data_line(line);
    let mut assistant = chat_message("assistant", AiChatRole::Assistant, "checking");
    let mut provider_parts = Vec::new();

    for event in parsed.events {
        match event {
            AiStreamEvent::ProviderResponsePart {
                provider_type,
                part,
            } if provider_type == "gemini" => provider_parts.push(part),
            AiStreamEvent::ToolCallComplete {
                id,
                name,
                arguments,
            } => upsert_ai_tool_call(&mut assistant, &id, &name, &arguments, "completed"),
            _ => {}
        }
    }
    set_ai_provider_parts(&mut assistant, "gemini", provider_parts);

    let (_, contents) = gemini_chat_contents(&[assistant]);

    assert_eq!(
        contents[1]["parts"],
        serde_json::json!([
            {
                "text": "checking",
                "thoughtSignature": "text-signature",
            },
            {
                "functionCall": {
                    "name": "get_state",
                    "args": { "scope": "active" },
                },
                "thoughtSignature": "call-signature",
            },
        ])
    );
}
