use serde_json::Value;

use crate::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiReasoningLevel, AiReasoningRequestFormat,
    AiToolCall, AiToolChoice, AiToolDefinition, model_reasoning_capability,
};

pub(crate) fn openai_chat_body(config: &AiChatStreamConfig, messages: &[AiChatMessage]) -> Value {
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": openai_chat_messages(config, messages),
        "stream": true,
    });
    if let Some(tokens) = config.max_response_tokens.filter(|tokens| *tokens > 0)
        && let Some(object) = body.as_object_mut()
    {
        object.insert("max_tokens".to_string(), serde_json::json!(tokens));
    }
    if let Some(object) = body.as_object_mut() {
        apply_reasoning_options(object, config);
        apply_tool_options(object, config);
    }
    body
}

fn apply_tool_options(body: &mut serde_json::Map<String, Value>, config: &AiChatStreamConfig) {
    if config.tools.is_empty() {
        return;
    }
    body.insert(
        "tools".to_string(),
        serde_json::json!(openai_tool_definitions(&config.tools)),
    );
    match &config.tool_choice {
        AiToolChoice::Auto => {}
        AiToolChoice::Required => {
            // GLM documents only automatic tool selection, while Kimi K2.6
            // and K2.7 reject `required`.
            if config.provider_type != "glm" && !kimi_model_rejects_required_tool_choice(config) {
                body.insert("tool_choice".to_string(), serde_json::json!("required"));
            }
        }
        AiToolChoice::Named(name) if !name.is_empty() => {
            // Neither Kimi nor GLM documents named tool selection.
            if !matches!(config.provider_type.as_str(), "kimi" | "glm") {
                body.insert(
                    "tool_choice".to_string(),
                    serde_json::json!({
                        "type": "function",
                        "function": { "name": name },
                    }),
                );
            }
        }
        AiToolChoice::Named(_) => {}
    }
}

fn kimi_model_rejects_required_tool_choice(config: &AiChatStreamConfig) -> bool {
    if config.provider_type != "kimi" {
        return false;
    }
    let model = config.model.to_ascii_lowercase();
    model.starts_with("kimi-k2.6") || model.starts_with("kimi-k2.7-code")
}

fn openai_tool_definitions(tools: &[AiToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                },
            })
        })
        .collect()
}

fn apply_reasoning_options(body: &mut serde_json::Map<String, Value>, config: &AiChatStreamConfig) {
    let requested = config.reasoning_effort.as_deref().unwrap_or("auto");
    let effort = if config.provider_type == "xai" {
        crate::normalize_reasoning_level_for_model("xai", &config.model, requested)
    } else {
        AiReasoningLevel::parse(requested)
    };
    if effort == AiReasoningLevel::Auto {
        return;
    }
    let capability = model_reasoning_capability(&config.provider_type, &config.model);

    match capability.request_format {
        AiReasoningRequestFormat::DeepSeek => {
            if effort == AiReasoningLevel::None {
                body.insert(
                    "thinking".to_string(),
                    serde_json::json!({ "type": "disabled" }),
                );
                return;
            }
            body.insert(
                "thinking".to_string(),
                serde_json::json!({ "type": "enabled" }),
            );
            body.insert(
                "reasoning_effort".to_string(),
                // DeepSeek performs its own model-specific effort mapping, so
                // preserve the user's documented low/high/xhigh/max request.
                serde_json::json!(effort.as_str()),
            );
        }
        AiReasoningRequestFormat::OpenAi => {
            body.insert(
                "reasoning_effort".to_string(),
                serde_json::json!(effort.as_str()),
            );
        }
        AiReasoningRequestFormat::KimiThinking => {
            if effort == AiReasoningLevel::None {
                body.insert(
                    "thinking".to_string(),
                    serde_json::json!({ "type": "disabled" }),
                );
            }
        }
        AiReasoningRequestFormat::GlmEffort => {
            body.insert(
                "thinking".to_string(),
                serde_json::json!({ "type": "enabled" }),
            );
            body.insert(
                "reasoning_effort".to_string(),
                serde_json::json!(effort.as_str()),
            );
        }
        AiReasoningRequestFormat::GlmThinking => {
            if effort == AiReasoningLevel::None {
                body.insert(
                    "thinking".to_string(),
                    serde_json::json!({ "type": "disabled" }),
                );
            }
        }
        _ => {}
    }
}

pub(crate) fn openai_chat_messages(
    config: &AiChatStreamConfig,
    messages: &[AiChatMessage],
) -> Vec<Value> {
    let mut system_parts = Vec::new();
    let mut non_system = Vec::new();
    for message in messages {
        if crate::runtime_context::is_runtime_context_message(message) {
            non_system.push(message);
            continue;
        }
        match message.role {
            AiChatRole::System if !message.content.is_empty() => {
                system_parts.push(message.content.clone());
            }
            AiChatRole::System => {}
            _ => non_system.push(message),
        }
    }

    // Tauri only normalizes system messages when there is at least one
    // non-empty system prompt; all-empty system messages are sent as-is.
    let normalized = if system_parts.is_empty() {
        messages.iter().collect::<Vec<_>>()
    } else {
        non_system
    };

    let last_user_index = normalized
        .iter()
        .rposition(|message| message.role == AiChatRole::User)
        .unwrap_or(usize::MAX);
    let mut out = normalized
        .iter()
        .enumerate()
        .map(|(index, message)| openai_message_value(config, message, index, last_user_index))
        .collect::<Vec<_>>();
    if !system_parts.is_empty() {
        out.insert(
            0,
            serde_json::json!({
                "role": "system",
                "content": system_parts.join("\n\n"),
            }),
        );
    }
    out
}

fn openai_message_value(
    config: &AiChatStreamConfig,
    message: &AiChatMessage,
    index: usize,
    last_user_index: usize,
) -> Value {
    // Keep current application data out of the cacheable system prefix without
    // making it a new user turn for DeepSeek reasoning replay.
    if crate::runtime_context::is_runtime_context_message(message) {
        return serde_json::json!({"role": "user", "content": message.content});
    }
    match message.role {
        AiChatRole::User => serde_json::json!({
            "role": "user",
            "content": message.content,
        }),
        AiChatRole::System => serde_json::json!({
            "role": "system",
            "content": message.content,
        }),
        AiChatRole::Tool => {
            let mut tool = serde_json::json!({
                "role": "tool",
                "content": message.content,
            });
            if let Some(tool_call_id) = message.tool_call_id.as_ref()
                && let Some(object) = tool.as_object_mut()
            {
                object.insert(
                    "tool_call_id".to_string(),
                    Value::String(tool_call_id.clone()),
                );
            }
            tool
        }
        AiChatRole::Assistant => {
            let calls = tool_calls_from_message(message);
            if calls.is_empty() {
                let mut assistant = serde_json::json!({
                    "role": "assistant",
                    "content": message.content,
                });
                // K3 and K2.7 require preserved thinking across complete
                // assistant messages, including turns without tool calls.
                if kimi_model_requires_preserved_thinking(config)
                    && let Some(reasoning) = message.thinking_content.as_ref()
                    && let Some(object) = assistant.as_object_mut()
                {
                    object.insert(
                        "reasoning_content".to_string(),
                        Value::String(reasoning.clone()),
                    );
                }
                assistant
            } else {
                let mut assistant = serde_json::json!({
                    "role": "assistant",
                    "content": if message.content.is_empty() {
                        Value::Null
                    } else {
                        Value::String(message.content.clone())
                    },
                    "tool_calls": calls
                        .into_iter()
                        .map(|call| serde_json::json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments,
                            },
                        }))
                        .collect::<Vec<_>>(),
                });
                if let Some(reasoning) = message.thinking_content.as_ref()
                    && should_preserve_reasoning_content(config, index, last_user_index)
                    && let Some(object) = assistant.as_object_mut()
                {
                    object.insert(
                        "reasoning_content".to_string(),
                        Value::String(reasoning.clone()),
                    );
                }
                assistant
            }
        }
    }
}

fn kimi_model_requires_preserved_thinking(config: &AiChatStreamConfig) -> bool {
    if config.provider_type != "kimi" {
        return false;
    }
    let model = config.model.to_ascii_lowercase();
    model.starts_with("kimi-k3") || model.starts_with("kimi-k2.7-code")
}

fn should_preserve_reasoning_content(
    config: &AiChatStreamConfig,
    index: usize,
    last_user_index: usize,
) -> bool {
    config.provider_type != "deepseek" || index > last_user_index
}

fn tool_calls_from_message(message: &AiChatMessage) -> Vec<AiToolCall> {
    message
        .tool_calls
        .iter()
        .filter_map(AiToolCall::from_value)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AiExecutionBackend, AiPolicySafetyMode, AiToolUsePolicy};

    fn config(provider_type: &str, reasoning_effort: &str) -> AiChatStreamConfig {
        AiChatStreamConfig {
            api_protocol: crate::AiApiProtocol::default(),
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
            reasoning_effort: Some(reasoning_effort.to_string()),
            safety_mode: AiPolicySafetyMode::Default,
            profile_id: None,
            memory_context: None,
            memory_entry_ids: Vec::new(),
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: AiToolChoice::Auto,
        }
    }

    #[test]
    fn reasoning_payload_matches_provider_and_model_protocol() {
        for (provider, model, effort, thinking, reasoning) in [
            ("openai", "model", "xhigh", None, Some("xhigh")),
            ("openai", "model", "off", None, Some("none")),
            ("deepseek", "model", "none", Some("disabled"), None),
            ("deepseek", "model", "low", Some("enabled"), Some("low")),
            ("deepseek", "model", "xhigh", Some("enabled"), Some("xhigh")),
            ("deepseek", "model", "max", Some("enabled"), Some("max")),
            ("kimi", "kimi-k3", "max", None, Some("max")),
            ("kimi", "kimi-k2.6", "none", Some("disabled"), None),
            ("glm", "glm-5.2", "xhigh", Some("enabled"), Some("xhigh")),
            ("glm", "glm-4.7", "none", Some("disabled"), None),
        ] {
            let mut config = config(provider, effort);
            config.model = model.to_string();
            let body = openai_chat_body(&config, &[]);
            assert_eq!(
                body.get("thinking"),
                thinking
                    .map(|kind| serde_json::json!({"type": kind}))
                    .as_ref(),
                "{provider}/{model}/{effort}: thinking"
            );
            assert_eq!(
                body.get("reasoning_effort"),
                reasoning.map(Value::from).as_ref(),
                "{provider}/{model}/{effort}: reasoning_effort"
            );
        }
    }

    #[test]
    fn vendor_tool_choice_preserves_tools_and_omits_unsupported_modes() {
        let tools = vec![AiToolDefinition {
            name: "run_command".to_string(),
            description: "Run command".to_string(),
            parameters: serde_json::json!({ "type": "object" }),
        }];
        let named = AiToolChoice::Named("run_command".into());
        for (provider, model, choice, expected) in [
            (
                "kimi",
                "kimi-k2.7-code-highspeed",
                AiToolChoice::Required,
                None,
            ),
            ("kimi", "kimi-k3", AiToolChoice::Required, Some("required")),
            ("kimi", "kimi-k3", named.clone(), None),
            ("glm", "glm-5.2", AiToolChoice::Required, None),
            ("glm", "glm-5.2", named, None),
        ] {
            let mut config = config(provider, "auto");
            config.model = model.to_string();
            config.tools = tools.clone();
            config.tool_choice = choice;

            let body = openai_chat_body(&config, &[]);
            assert_eq!(
                body.get("tool_choice"),
                expected.map(serde_json::Value::from).as_ref(),
                "{provider}/{model}: {:?}",
                config.tool_choice
            );
            assert_eq!(body["tools"][0]["function"]["name"], "run_command");
        }
    }

    #[test]
    fn runtime_context_preserves_the_provider_prefix_and_tool_round() {
        let first_context = serde_json::json!({"runtimeContext": {
            "protocolVersion": 2, "snapshotId": "snap_first", "observedAtMs": 100,
            "liveHandles": [{"handleId": "rt_first"}]
        }})
        .to_string();
        let second_context = serde_json::json!({"runtimeContext": {
            "protocolVersion": 2, "snapshotId": "snap_second", "observedAtMs": 200,
            "liveHandles": [{"handleId": "rt_second"}]
        }})
        .to_string();
        let first: Vec<AiChatMessage> = serde_json::from_value(serde_json::json!([
            {"id":"base-system","role":"system","content":"Stable policies","timestamp_ms":0},
            {"id":"user","role":"user","content":"Inspect the shell","timestamp_ms":1},
            {"id":"assistant","role":"assistant","content":"","timestamp_ms":2,
             "thinking_content":"Inspect the current shell",
             "tool_calls":[{"id":"call-1","name":"run_command","arguments":"{}"}]},
            {"id":"result","role":"tool","content":"{\"output\":\"safe\"}","timestamp_ms":3,"tool_call_id":"call-1"},
            {"id":"runtime-context-v2","role":"system","content":first_context,"timestamp_ms":4}
        ])).unwrap();
        let mut second = first.clone();
        second.last_mut().unwrap().content = second_context.clone();

        for provider in ["deepseek", "openai", "ollama"] {
            let config = config(provider, "auto");
            let first_wire = openai_chat_body(&config, &first);
            let second_wire = openai_chat_body(&config, &second);
            assert_eq!(
                first_wire["messages"][0],
                serde_json::json!({"role":"system","content":"Stable policies"})
            );
            assert_eq!(
                first_wire["messages"].as_array().unwrap()[..4],
                second_wire["messages"].as_array().unwrap()[..4]
            );
            assert_eq!(
                first_wire["messages"][2]["reasoning_content"],
                "Inspect the current shell"
            );
            assert_eq!(first_wire["messages"][2]["tool_calls"][0]["id"], "call-1");
            assert_eq!(first_wire["messages"][3]["tool_call_id"], "call-1");
            assert_eq!(
                first_wire["messages"][4],
                serde_json::json!({"role":"user","content":first_context})
            );
            assert_eq!(second_wire["messages"][4]["content"], second_context);
        }

        let (first_system, first_wire) = super::super::anthropic::anthropic_chat_messages(&first);
        let (second_system, second_wire) =
            super::super::anthropic::anthropic_chat_messages(&second);
        assert_eq!(first_system.as_deref(), Some("Stable policies"));
        assert_eq!(second_system, first_system);
        assert_eq!(first_wire[..2], second_wire[..2]);
        assert_eq!(
            first_wire[2]["content"][0],
            serde_json::json!({
                "type":"tool_result","tool_use_id":"call-1","content":"{\"output\":\"safe\"}"
            })
        );
        assert_eq!(first_wire[2]["content"][1]["text"], first_context);
        assert_eq!(second_wire[2]["content"][1]["text"], second_context);

        let (first_system, first_wire) = super::super::gemini::gemini_chat_contents(&first);
        let (second_system, second_wire) = super::super::gemini::gemini_chat_contents(&second);
        assert_eq!(first_system.as_deref(), Some("Stable policies"));
        assert_eq!(second_system, first_system);
        assert_eq!(first_wire[..2], second_wire[..2]);
        assert_eq!(
            first_wire[2]["parts"][0],
            serde_json::json!({
                "functionResponse":{"name":"run_command","response":{"output":"safe"}}
            })
        );
        assert_eq!(first_wire[2]["parts"][1]["text"], first_context);
        assert_eq!(second_wire[2]["parts"][1]["text"], second_context);

        let config = config("openai", "auto");
        let first_wire = super::super::responses_payload::responses_body(&config, &first);
        let second_wire = super::super::responses_payload::responses_body(&config, &second);
        assert_eq!(
            first_wire["input"].as_array().unwrap()[..4],
            second_wire["input"].as_array().unwrap()[..4]
        );
        assert_eq!(first_wire["input"][2]["type"], "function_call");
        assert_eq!(first_wire["input"][3]["call_id"], "call-1");
        assert_eq!(first_wire["input"][4]["content"], first_context);
        assert_eq!(second_wire["input"][4]["content"], second_context);
    }

    #[test]
    fn openai_message_conversion_preserves_tool_calls_and_results() {
        let assistant = AiChatMessage {
            id: "a1".to_string(),
            role: AiChatRole::Assistant,
            content: String::new(),
            timestamp_ms: 1,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: vec![serde_json::json!({
                "id": "call-1",
                "name": "run_command",
                "arguments": "{\"command\":\"pwd\"}",
            })],
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        let result = AiChatMessage {
            id: "t1".to_string(),
            role: AiChatRole::Tool,
            content: "{\"ok\":true}".to_string(),
            timestamp_ms: 2,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: Some("call-1".to_string()),
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };

        let converted = openai_chat_messages(&config("openai", "auto"), &[assistant, result]);
        assert_eq!(converted[0]["role"].as_str(), Some("assistant"));
        assert!(converted[0]["content"].is_null());
        assert_eq!(
            converted[0]["tool_calls"][0]["function"]["arguments"].as_str(),
            Some("{\"command\":\"pwd\"}")
        );
        assert_eq!(converted[1]["role"].as_str(), Some("tool"));
        assert_eq!(converted[1]["tool_call_id"].as_str(), Some("call-1"));
    }
}
