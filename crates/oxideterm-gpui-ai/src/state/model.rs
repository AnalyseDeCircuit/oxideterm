use oxideterm_ai::{AiChatStreamConfig, AiConversation, AiExecutionBackend, AiReasoningLevel};

use super::AiWorkspaceEntity;

pub(super) const AI_ACP_SESSION_METADATA_KEY: &str = "acp";
const AI_REASONING_EFFORT_SESSION_METADATA_KEY: &str = "reasoningEffort";

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAcpSessionState {
    pub agent_id: String,
    pub session_id: String,
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    #[serde(default)]
    pub model_selection: Option<oxideterm_ai::AcpSessionConfigSelection>,
    #[serde(default)]
    pub config_selections: Vec<oxideterm_ai::AcpSessionConfigSelection>,
    #[serde(default)]
    pub current_mode_id: Option<String>,
    #[serde(default)]
    pub available_modes: Vec<oxideterm_ai::AcpSessionMode>,
    #[serde(default)]
    pub available_commands: Vec<serde_json::Value>,
    #[serde(default)]
    pub plan: Option<serde_json::Value>,
    #[serde(default)]
    pub usage: Option<serde_json::Value>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_cursor: Option<oxideterm_ai::AcpConversationHandoffCursor>,
}

pub fn ai_acp_session_state(conversation: &AiConversation) -> Option<AiAcpSessionState> {
    conversation
        .session_metadata
        .as_ref()?
        .get(AI_ACP_SESSION_METADATA_KEY)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

pub fn ai_message_backend_for_stream(
    config: &AiChatStreamConfig,
) -> oxideterm_ai::AiMessageBackendProvenance {
    match config.execution_backend {
        AiExecutionBackend::Provider => oxideterm_ai::AiMessageBackendProvenance {
            kind: oxideterm_ai::AiMessageBackendKind::Provider,
            backend_id: config
                .provider_id
                .clone()
                .unwrap_or_else(|| config.provider_type.clone()),
            model: config.model.clone(),
        },
        AiExecutionBackend::Acp => oxideterm_ai::AiMessageBackendProvenance {
            kind: oxideterm_ai::AiMessageBackendKind::Acp,
            backend_id: config.acp_agent_id.clone().unwrap_or_default(),
            model: config.model.clone(),
        },
    }
}

impl AiWorkspaceEntity {
    pub fn set_active_reasoning_level(
        &mut self,
        provider_id: &str,
        model: &str,
        level: AiReasoningLevel,
    ) {
        if let Some(conversation) = self.conversation_state_mut().active_conversation_mut() {
            store_ai_reasoning_level_in_conversation(conversation, provider_id, model, level);
            self.persist_chat_state();
        }
    }

    pub fn set_active_acp_model_selection(
        &mut self,
        agent_id: &str,
        discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        config_id: &str,
        value_id: &str,
    ) -> bool {
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let stored = store_ai_acp_model_selection_in_conversation(
            conversation,
            agent_id,
            discovered_options,
            config_id,
            value_id,
        );
        if stored {
            self.persist_chat_state();
        }
        stored
    }

    pub fn set_active_acp_config_selection(
        &mut self,
        agent_id: &str,
        discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        config_id: &str,
        value_id: &str,
    ) -> bool {
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let stored = store_ai_acp_config_selection_in_conversation(
            conversation,
            agent_id,
            discovered_options,
            config_id,
            value_id,
            false,
        );
        if stored {
            self.persist_chat_state();
        }
        stored
    }

    pub fn set_active_acp_mode(&mut self, agent_id: &str, mode_id: &str) -> bool {
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let Some(mut state) =
            ai_acp_session_state(conversation).filter(|state| state.agent_id == agent_id)
        else {
            return false;
        };
        if !state
            .available_modes
            .iter()
            .any(|mode| mode.mode_id == mode_id)
        {
            return false;
        }
        state.current_mode_id = Some(mode_id.to_string());
        let Some(metadata) = conversation
            .session_metadata
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        else {
            return false;
        };
        let Ok(value) = serde_json::to_value(state) else {
            return false;
        };
        metadata.insert(AI_ACP_SESSION_METADATA_KEY.to_string(), value);
        self.persist_chat_state();
        true
    }
}

pub fn ai_conversation_reasoning_effort<'a>(
    conversation: &'a AiConversation,
    provider_id: &str,
    model: &str,
) -> Option<&'a str> {
    let value = conversation
        .session_metadata
        .as_ref()?
        .get(AI_REASONING_EFFORT_SESSION_METADATA_KEY)?;
    // Read the first implementation's scalar value for backward compatibility.
    value
        .as_str()
        .or_else(|| value.get(provider_id)?.get(model)?.as_str())
}

pub fn store_ai_reasoning_level_in_conversation(
    conversation: &mut AiConversation,
    provider_id: &str,
    model: &str,
    level: AiReasoningLevel,
) {
    let metadata = conversation
        .session_metadata
        .get_or_insert_with(|| serde_json::json!({}));
    if !metadata.is_object() {
        *metadata = serde_json::json!({});
    }
    let object = metadata
        .as_object_mut()
        .expect("reasoning session metadata must be an object");
    let reasoning = object
        .entry(AI_REASONING_EFFORT_SESSION_METADATA_KEY)
        .or_insert_with(|| serde_json::json!({}));
    if !reasoning.is_object() {
        *reasoning = serde_json::json!({});
    }
    let providers = reasoning
        .as_object_mut()
        .expect("reasoning session metadata must be an object");
    let models = providers
        .entry(provider_id.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !models.is_object() {
        *models = serde_json::json!({});
    }
    let models = models
        .as_object_mut()
        .expect("reasoning provider metadata must be an object");
    if level == AiReasoningLevel::Auto {
        models.remove(model);
    } else {
        models.insert(model.to_string(), serde_json::json!(level.as_str()));
    }
}

pub fn store_ai_acp_model_selection_in_conversation(
    conversation: &mut AiConversation,
    agent_id: &str,
    discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    config_id: &str,
    value_id: &str,
) -> bool {
    store_ai_acp_config_selection_in_conversation(
        conversation,
        agent_id,
        discovered_options,
        config_id,
        value_id,
        true,
    )
}

pub fn store_ai_acp_config_selection_in_conversation(
    conversation: &mut AiConversation,
    agent_id: &str,
    discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    config_id: &str,
    value_id: &str,
    is_model_selection: bool,
) -> bool {
    let mut state = ai_acp_session_state(conversation)
        .filter(|state| state.agent_id == agent_id)
        .unwrap_or_else(|| AiAcpSessionState {
            agent_id: agent_id.to_string(),
            session_id: String::new(),
            metadata: None,
            config_options: discovered_options,
            model_selection: None,
            config_selections: Vec::new(),
            current_mode_id: None,
            available_modes: Vec::new(),
            available_commands: Vec::new(),
            plan: None,
            usage: None,
            title: None,
            handoff_cursor: None,
        });
    let Some(option) = state
        .config_options
        .iter_mut()
        .find(|option| option.config_id == config_id)
    else {
        return false;
    };
    if !option
        .choices
        .iter()
        .any(|choice| choice.value_id == value_id)
    {
        return false;
    }

    // An empty session id marks a pre-prompt choice. The prompt path creates a
    // real session and applies this value before sending the user's message.
    option.current_value_id = value_id.to_string();
    let selection = oxideterm_ai::AcpSessionConfigSelection {
        config_id: config_id.to_string(),
        value_id: value_id.to_string(),
    };
    if is_model_selection {
        state.model_selection = Some(selection.clone());
    }
    state
        .config_selections
        .retain(|existing| existing.config_id != config_id);
    state.config_selections.push(selection);
    let conversation_id = conversation.id.clone();
    let metadata = conversation.session_metadata.get_or_insert_with(|| {
        serde_json::json!({
            "conversationId": conversation_id,
            "origin": "sidebar",
        })
    });
    let Some(metadata) = metadata.as_object_mut() else {
        return false;
    };
    let Ok(value) = serde_json::to_value(state) else {
        return false;
    };
    metadata.insert(AI_ACP_SESSION_METADATA_KEY.to_string(), value);
    true
}

#[cfg(test)]
mod acp_model_selection_tests {
    use super::*;

    #[test]
    fn discovered_model_choice_is_stored_for_the_next_real_session() {
        let mut conversation = AiConversation {
            archived: false,
            id: "conversation-1".to_string(),
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
        };
        let options = vec![oxideterm_ai::AcpSessionConfigOption {
            config_id: "model".to_string(),
            name: "Model".to_string(),
            category: Some("model".to_string()),
            current_value_id: "gpt-5.6-sol".to_string(),
            choices: vec![
                oxideterm_ai::AcpSessionConfigChoice {
                    value_id: "gpt-5.6-sol".to_string(),
                    label: "gpt-5.6-sol".to_string(),
                },
                oxideterm_ai::AcpSessionConfigChoice {
                    value_id: "gpt-5.6-terra".to_string(),
                    label: "gpt-5.6-terra".to_string(),
                },
            ],
        }];

        assert!(store_ai_acp_model_selection_in_conversation(
            &mut conversation,
            "codex",
            options,
            "model",
            "gpt-5.6-terra",
        ));

        let state = ai_acp_session_state(&conversation).expect("stored ACP model state");
        assert_eq!(state.agent_id, "codex");
        assert!(state.session_id.is_empty());
        assert_eq!(
            state.model_selection,
            Some(oxideterm_ai::AcpSessionConfigSelection {
                config_id: "model".to_string(),
                value_id: "gpt-5.6-terra".to_string(),
            })
        );
        assert_eq!(state.config_options[0].current_value_id, "gpt-5.6-terra");
    }

    #[test]
    fn reasoning_level_is_scoped_to_the_conversation_without_erasing_other_metadata() {
        let mut conversation = AiConversation {
            archived: false,
            id: "conversation-1".to_string(),
            title: "Conversation".to_string(),
            messages: Vec::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            origin: "sidebar".to_string(),
            profile_id: None,
            message_count: 0,
            session_id: None,
            session_metadata: Some(serde_json::json!({ "other": true })),
            messages_loaded: true,
            turn_count: 0,
        };

        store_ai_reasoning_level_in_conversation(
            &mut conversation,
            "openai",
            "gpt-5.6-sol",
            AiReasoningLevel::High,
        );
        assert_eq!(
            ai_conversation_reasoning_effort(&conversation, "openai", "gpt-5.6-sol"),
            Some("high")
        );
        assert_eq!(
            conversation
                .session_metadata
                .as_ref()
                .and_then(|value| { value.get("other").and_then(serde_json::Value::as_bool) }),
            Some(true)
        );

        store_ai_reasoning_level_in_conversation(
            &mut conversation,
            "openai",
            "gpt-5.6-sol",
            AiReasoningLevel::Auto,
        );
        assert_eq!(
            ai_conversation_reasoning_effort(&conversation, "openai", "gpt-5.6-sol"),
            None
        );
    }
}
