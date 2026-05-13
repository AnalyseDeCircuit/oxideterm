use std::{collections::HashMap, fs};

use serde_json::Value;

use super::*;
use crate::providers::{parse_provider_context_windows, parse_provider_models};
use crate::streaming::{
    anthropic_chat_messages, gemini_chat_contents, openai_chat_messages, parse_anthropic_data_line,
    parse_gemini_data_line, parse_openai_data_line,
};

fn provider(id: &str, provider_type: &str, base_url: &str, enabled: bool) -> AiProviderView {
    AiProviderView {
        id: id.to_string(),
        provider_type: provider_type.to_string(),
        name: id.to_string(),
        base_url: base_url.to_string(),
        default_model: String::new(),
        models: Vec::new(),
        enabled,
        custom: false,
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
    }
}

#[test]
fn provider_templates_match_tauri_order() {
    let types = AI_PROVIDER_TEMPLATES
        .iter()
        .map(|template| template.provider_type)
        .collect::<Vec<_>>();

    assert_eq!(
        types,
        vec![
            "openai_compatible",
            "deepseek",
            "openai",
            "anthropic",
            "gemini",
            "ollama"
        ]
    );
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
    assert_eq!(
        provider_string(&provider, "defaultModel").as_deref(),
        Some("gpt-4o-mini")
    );
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
    assert_eq!(active_model.as_deref(), Some("gpt-4o-mini"));

    select_provider_model(
        &mut providers,
        &mut active_provider_id,
        &mut active_model,
        "custom-ollama-2",
        "llama3.2".into(),
    );
    assert_eq!(active_provider_id.as_deref(), Some("custom-ollama-2"));
    assert_eq!(active_model.as_deref(), Some("llama3.2"));
    assert_eq!(
        provider_string(&providers[1], "defaultModel").as_deref(),
        Some("llama3.2")
    );
    set_provider_default_model(&mut providers, 1, "qwen2.5".into());
    assert_eq!(
        provider_string(&providers[1], "defaultModel").as_deref(),
        Some("qwen2.5")
    );

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

    let removed = remove_provider_at(
        &mut providers,
        &mut active_provider_id,
        &mut active_model,
        1,
    );
    assert_eq!(removed.as_deref(), Some("custom-ollama-2"));
    assert_eq!(active_provider_id.as_deref(), Some("custom-openai-1"));
    assert_eq!(active_model.as_deref(), Some("gpt-4o-mini"));
}

#[test]
fn settings_provider_key_and_token_policy_match_tauri() {
    assert!(!provider_chat_requires_key("ollama"));
    assert!(provider_chat_requires_key("openai"));
    assert_eq!(
        provider_key_display_state("ollama", false),
        AiProviderKeyDisplayState::Keyless
    );
    assert!(provider_key_display_state("ollama", false).has_usable_key());
    assert!(!provider_key_display_state("ollama", false).shows_key_control());
    assert_eq!(
        provider_key_display_state("openai", true),
        AiProviderKeyDisplayState::Stored
    );
    assert_eq!(
        provider_key_display_state("openai", false),
        AiProviderKeyDisplayState::Missing
    );

    assert_eq!(
        provider_refresh_key_policy("ollama"),
        AiProviderRefreshKeyPolicy::NoKey
    );
    assert_eq!(
        provider_refresh_key_policy("openai_compatible"),
        AiProviderRefreshKeyPolicy::OptionalStoredKey
    );
    assert_eq!(
        provider_refresh_key_policy("openai"),
        AiProviderRefreshKeyPolicy::RequiredStoredKey
    );

    let limits = serde_json::Map::from_iter([
        (
            "provider-1".to_string(),
            serde_json::json!({ "gpt-4o-mini": 1024 }),
        ),
        ("fallback-model".to_string(), serde_json::json!(2048)),
    ]);
    assert_eq!(
        model_max_response_tokens(&limits, "provider-1", "gpt-4o-mini"),
        Some(1024)
    );
    assert_eq!(
        model_max_response_tokens(&limits, "provider-2", "fallback-model"),
        Some(2048)
    );

    let mut empty_draft = "   ".to_string();
    assert!(take_provider_key_secret(&mut empty_draft).is_none());
    assert_eq!(empty_draft, "   ");

    let mut secret_draft = "sk-test".to_string();
    let secret = take_provider_key_secret(&mut secret_draft).expect("secret");
    assert_eq!(secret.as_str(), "sk-test");
    assert!(secret_draft.is_empty());
}

#[test]
fn projects_provider_view_with_defaults() {
    let value = serde_json::json!({
        "id": "custom-openai-1",
        "models": ["gpt-4o-mini", "gpt-4o"],
    });

    let view = provider_view(&value).expect("provider view");

    assert_eq!(view.provider_type, "openai_compatible");
    assert_eq!(view.name, "Provider");
    assert!(view.enabled);
    assert!(view.custom);
    assert_eq!(view.models, vec!["gpt-4o-mini", "gpt-4o"]);
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
        vec!["gpt-4o", "gpt-4o-mini"]
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
                "models": [{"key": "model-b"}, {"id": "model-a"}]
            })
        ),
        vec!["model-a", "model-b"]
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
                    {"id": "model-b", "context_length": 8192}
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
fn model_selector_probe_matches_tauri_rules() {
    assert_eq!(
        resolve_model_selector_provider_probe(&provider(
            "disabled",
            "openai",
            "https://api.openai.com/v1",
            false,
        )),
        ModelSelectorProviderProbe::Disabled
    );
    assert_eq!(
        resolve_model_selector_provider_probe(&provider(
            "ollama",
            "ollama",
            "http://localhost:11434",
            true,
        )),
        ModelSelectorProviderProbe::ImplicitKey {
            endpoint: Some("/api/tags"),
        }
    );
    assert_eq!(
        resolve_model_selector_provider_probe(&provider(
            "local",
            "openai_compatible",
            "http://192.168.1.20:1234/v1",
            true,
        )),
        ModelSelectorProviderProbe::ImplicitKey {
            endpoint: Some("/models"),
        }
    );
    assert_eq!(
        resolve_model_selector_provider_probe(&provider(
            "remote",
            "openai_compatible",
            "https://gateway.example/v1",
            true,
        )),
        ModelSelectorProviderProbe::StoredKey
    );
}

#[test]
fn model_selector_local_url_heuristic_matches_tauri() {
    assert!(is_local_provider_url("http://localhost:11434"));
    assert!(is_local_provider_url("http://127.0.0.1:1234/v1"));
    assert!(is_local_provider_url("http://[::1]:1234/v1"));
    assert!(is_local_provider_url("http://workstation.local:1234/v1"));
    assert!(is_local_provider_url("http://10.0.0.5:1234/v1"));
    assert!(is_local_provider_url("http://192.168.1.5:1234/v1"));
    assert!(is_local_provider_url("http://172.16.0.5:1234/v1"));
    assert!(is_local_provider_url("http://172.31.255.5:1234/v1"));
    assert!(!is_local_provider_url("http://172.32.0.5:1234/v1"));
    assert!(!is_local_provider_url("https://api.example.com/v1"));
}

#[test]
fn model_selector_display_and_filter_match_tauri() {
    let mut openai = provider("OpenAI", "openai", "https://api.openai.com/v1", true);
    openai.default_model = "gpt-4o-mini".to_string();
    openai.models = vec!["gpt-4o-mini".to_string(), "gpt-4o".to_string()];
    let mut disabled = provider("Disabled", "openai", "https://api.example", false);
    disabled.models = vec!["hidden-model".to_string()];

    assert_eq!(
        model_selector_display_name(Some(&openai), Some("provider/model-name")),
        "OpenAI/model-name"
    );
    assert_eq!(
        model_selector_truncated_label("0123456789012345678901234"),
        "0123456789012345678901..."
    );

    let groups = model_selector_visible_provider_groups(&[openai.clone(), disabled], "4o-mini");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].provider.id, "OpenAI");
    assert_eq!(groups[0].visible_models, vec!["gpt-4o-mini"]);
}

#[test]
fn active_provider_and_model_helpers_keep_settings_logic_out_of_ui() {
    let mut openai = provider("OpenAI", "openai", "https://api.openai.com/v1", true);
    openai.default_model = "gpt-4o-mini".to_string();
    let ollama = provider("Ollama", "ollama", "http://localhost:11434", true);
    let providers = vec![openai.clone(), ollama];

    let active = active_provider_view(&providers, Some("OpenAI"));
    assert_eq!(active, Some(&openai));
    assert_eq!(
        active_model_or_provider_default(None, &openai).as_deref(),
        Some("gpt-4o-mini")
    );
    assert_eq!(
        active_model_or_provider_default(Some("gpt-4o"), &openai).as_deref(),
        Some("gpt-4o")
    );
}

#[test]
fn chat_title_matches_tauri_helper() {
    assert_eq!(generate_chat_title("hello\nworld"), "hello world");
    assert_eq!(
        generate_chat_title("012345678901234567890123456789x"),
        "012345678901234567890123456789..."
    );
}

#[test]
fn chat_state_conversation_lifecycle_matches_tauri_local_updates() {
    let mut state = AiChatState::default();
    let first = state.create_conversation("first".into(), Some("First".into()), 1, None);
    let second = state.create_conversation("second".into(), Some("Second".into()), 2, None);

    assert_eq!(
        state.active_conversation_id.as_deref(),
        Some(second.as_str())
    );
    state.set_active_conversation(first.clone());
    assert_eq!(
        state.active_conversation_id.as_deref(),
        Some(first.as_str())
    );

    state.rename_conversation(&first, "Renamed".into(), 3);
    assert_eq!(state.conversations[1].title, "Renamed");
    assert_eq!(state.conversations[1].updated_at_ms, 3);

    state.delete_conversation(&first);
    assert_eq!(
        state.active_conversation_id.as_deref(),
        Some(second.as_str())
    );

    state.clear_conversations();
    assert!(state.conversations.is_empty());
    assert!(state.active_conversation_id.is_none());
}

#[test]
fn parses_tauri_style_slash_command_prefix() {
    assert_eq!(
        parse_ai_user_input("/explain ls -la"),
        AiParsedInput {
            slash_command: Some("explain".into()),
            clean_text: "ls -la".into(),
            raw_text: "/explain ls -la".into(),
        }
    );
    assert_eq!(
        parse_ai_user_input(" /explain is normal text").slash_command,
        None
    );
    assert!(resolve_ai_slash_command("clear").is_some_and(|command| command.client_only));
    assert!(
        resolve_ai_slash_command("fix")
            .and_then(|command| command.system_prompt_modifier)
            .is_some()
    );
}

#[test]
fn slash_help_and_request_overrides_are_core_logic() {
    let help = ai_help_markdown(|key| format!("desc:{key}"));
    assert!(help.contains("`/help`"));
    assert!(help.contains("desc:ai.slash.help_desc"));

    let command = resolve_ai_slash_command("fix").unwrap();
    let prompt = slash_task_system_prompt(command).unwrap();
    assert!(prompt.contains("## Task Mode: /fix"));

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
fn chat_persistence_missing_file_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let store = AiChatPersistenceStore::new(dir.path().join("missing.json"));

    assert_eq!(store.load_state().unwrap(), AiChatState::default());
}

#[test]
fn chat_persistence_round_trips_state_and_repairs_active_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ai_conversations.json");
    let store = AiChatPersistenceStore::new(&path);
    let mut state = AiChatState::default();
    let conversation_id =
        state.create_conversation("conversation-1".into(), Some("Hello".into()), 42, None);
    state.add_message(
        &conversation_id,
        AiChatMessage {
            id: "message-1".into(),
            role: AiChatRole::User,
            content: "hello".into(),
            timestamp_ms: 43,
            model: Some("gpt-4o-mini".into()),
            context: None,
            is_streaming: false,
        },
    );

    store.save_state(&state).unwrap();
    assert_eq!(store.load_state().unwrap(), state);

    fs::write(
            &path,
            r#"{"conversations":[{"id":"conversation-2","title":"Recovered","messages":[],"created_at_ms":1,"updated_at_ms":1,"origin":"sidebar","profile_id":null}],"active_conversation_id":"missing"}"#,
        )
        .unwrap();
    let repaired = store.load_state().unwrap();
    assert_eq!(
        repaired.active_conversation_id.as_deref(),
        Some("conversation-2")
    );
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

#[test]
fn openai_chat_messages_merge_system_prompts() {
    let messages = vec![
        AiChatMessage {
            id: "1".into(),
            role: AiChatRole::System,
            content: "one".into(),
            timestamp_ms: 1,
            model: None,
            context: None,
            is_streaming: false,
        },
        AiChatMessage {
            id: "2".into(),
            role: AiChatRole::User,
            content: "hi".into(),
            timestamp_ms: 2,
            model: None,
            context: None,
            is_streaming: false,
        },
        AiChatMessage {
            id: "3".into(),
            role: AiChatRole::System,
            content: "two".into(),
            timestamp_ms: 3,
            model: None,
            context: None,
            is_streaming: false,
        },
    ];
    let converted = openai_chat_messages(&messages);
    assert_eq!(converted[0]["role"], "system");
    assert_eq!(converted[0]["content"], "one\n\ntwo");
    assert_eq!(converted[1]["role"], "user");
    assert_eq!(converted[1]["content"], "hi");
}

#[test]
fn anthropic_messages_merge_roles_and_start_with_user() {
    let messages = vec![
        chat_message("1", AiChatRole::System, "sys"),
        chat_message("2", AiChatRole::Assistant, "hello"),
        chat_message("3", AiChatRole::Assistant, "again"),
        chat_message("4", AiChatRole::User, "question"),
    ];
    let (system, converted) = anthropic_chat_messages(&messages);
    assert_eq!(system.as_deref(), Some("sys"));
    assert_eq!(converted[0]["role"], "user");
    assert_eq!(converted[0]["content"], "(Continue from previous context)");
    assert_eq!(converted[1]["role"], "assistant");
    assert_eq!(converted[1]["content"], "hello\n\nagain");
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
    assert_eq!(gemini.events, vec![AiStreamEvent::Content("hello".into())]);
}
