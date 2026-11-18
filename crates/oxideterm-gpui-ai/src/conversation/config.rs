use oxideterm_ai::{
    AiChatStreamConfig, AiExecutionBackend, AiImageInput, AiPolicySafetyMode, AiToolUsePolicy,
    active_model_selection, active_provider_view,
    chat_tools::ai_stream_tool_definitions,
    provider_views as ai_provider_views,
    stream_state::{
        AI_COMPACTION_DEFAULT_CONTEXT_WINDOW, AI_SUGGESTIONS_INSTRUCTION, DEFAULT_AI_SYSTEM_PROMPT,
        ai_context_window_from_maps, ai_orchestrator_system_prompt,
    },
    tool_policy_from_parts,
    tool_resources::ai_scoped_memory_context,
};
use oxideterm_gpui_ui::ai::AiSafetyMode;
use oxideterm_i18n::I18n;
use oxideterm_settings::{AiActiveBackend, AiSettings};

use crate::state::images::ai_image_model_key;
use crate::state::{AiWorkspaceEntity, ai_conversation_reasoning_effort};

pub struct AiMemoryScope<'a> {
    pub user: Option<&'a str>,
    pub project: Option<&'a str>,
    pub host: Option<&'a str>,
}

impl AiMemoryScope<'_> {
    fn read(
        &self,
        settings: &AiSettings,
        provider: Option<&str>,
        model: &str,
    ) -> (Option<String>, Vec<String>) {
        let characters = ai_model_context_window(settings, provider, model)
            .saturating_mul(4)
            .saturating_div(8)
            .clamp(4_000, 32_000);
        ai_scoped_memory_context(
            &settings.memory,
            characters,
            self.user,
            self.project,
            self.host,
        )
    }
}

pub fn ai_model_context_window(
    settings: &AiSettings,
    provider: Option<&str>,
    model: &str,
) -> usize {
    provider
        .and_then(|id| {
            ai_context_window_from_maps(
                &settings.user_context_windows,
                &settings.model_context_windows,
                id,
                model,
            )
        })
        .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW)
}

pub fn ai_tool_use_policy_from_settings(
    settings: &oxideterm_settings::AiToolUseSettings,
) -> AiToolUsePolicy {
    tool_policy_from_parts(
        settings.enabled,
        settings
            .auto_approve_tools
            .iter()
            .filter_map(|(key, value)| value.as_bool().map(|enabled| (key.clone(), enabled))),
        settings.disabled_tools.clone(),
        settings.max_rounds,
        settings.max_calls_per_round,
    )
}

impl AiWorkspaceEntity {
    fn ai_image_model_setting(
        settings: &AiSettings,
        provider: &str,
        endpoint: &str,
        model: &str,
    ) -> AiImageInput {
        settings
            .extra
            .get("imageModelOverrides")
            .and_then(|models| models.get(ai_image_model_key(provider, endpoint, model)))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }

    fn ai_image_text_only(&self, provider: &str, endpoint: &str, model: &str) -> bool {
        self.conversation_state()
            .active_conversation()
            .and_then(|conversation| conversation.session_metadata.as_ref())
            .and_then(|metadata| metadata.get("textOnlyImageModels"))
            .and_then(|models| models.get(ai_image_model_key(provider, endpoint, model)))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    pub fn resolve_ai_stream_config(
        &self,
        settings: &AiSettings,
        i18n: &I18n,
        allow_web: bool,
        active_profile_id: Option<String>,
        scope: AiMemoryScope<'_>,
    ) -> Result<AiChatStreamConfig, String> {
        let tool_policy = ai_tool_use_policy_from_settings(&settings.tool_use);
        if settings.active_backend == AiActiveBackend::Acp {
            let acp_agent_id = settings
                .active_acp_agent_id
                .clone()
                .filter(|agent_id| !agent_id.trim().is_empty())
                .ok_or_else(|| "No ACP agent selected.".to_string())?;
            let session_state = self.active_ai_acp_session_state(&acp_agent_id);
            let model_label = session_state
                .as_ref()
                .and_then(|state| oxideterm_ai::acp_model_config_option(&state.config_options))
                .and_then(|option| {
                    oxideterm_ai::acp_selected_config_choice(
                        option,
                        session_state
                            .as_ref()
                            .and_then(|state| state.model_selection.as_ref()),
                    )
                    .map(|choice| choice.label.clone())
                })
                .unwrap_or_else(|| self.picker_acp_fallback_for_id(settings, &acp_agent_id, i18n));
            let (memory_context, memory_entry_ids) = scope.read(settings, None, &model_label);
            let tools = ai_stream_tool_definitions(
                tool_policy.enabled,
                settings.skills.enabled,
                allow_web,
                &tool_policy,
                self.mcp_registry(),
            );
            return Ok(AiChatStreamConfig {
                api_protocol: oxideterm_ai::AiApiProtocol::default(),
                image_input: Default::default(),
                omit_history_images: false,
                execution_backend: AiExecutionBackend::Acp,
                provider_id: None,
                acp_agent_id: Some(acp_agent_id),
                acp_session_id: session_state.as_ref().map(|state| state.session_id.clone()),
                acp_config_selection: session_state.and_then(|state| state.model_selection),
                provider_type: "acp".to_string(),
                base_url: String::new(),
                model: model_label,
                api_key: None,
                max_response_tokens: None,
                reasoning_effort: None,
                safety_mode: match self.active_conversation_safety_mode() {
                    AiSafetyMode::Bypass => AiPolicySafetyMode::Bypass,
                    AiSafetyMode::ReadOnly => AiPolicySafetyMode::ReadOnly,
                    AiSafetyMode::Default => AiPolicySafetyMode::Default,
                },
                profile_id: active_profile_id,
                memory_context,
                memory_entry_ids,
                tool_policy,
                tools,
                tool_choice: oxideterm_ai::AiToolChoice::Auto,
            });
        }

        let providers = ai_provider_views(&settings.providers);
        let provider = active_provider_view(&providers, settings.active_provider_id.as_deref())
            .cloned()
            .ok_or_else(|| i18n.t("ai.model_selector.no_provider"))?;
        let model = active_model_selection(settings.active_model.as_deref())
            .ok_or_else(|| i18n.t("ai.model_selector.no_model_selected"))?;
        let (memory_context, memory_entry_ids) = scope.read(settings, Some(&provider.id), &model);
        let max_response_tokens = None;
        let configured_reasoning_effort = settings
            .reasoning_model_overrides
            .get(&provider.id)
            .and_then(|models| models.get(&model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");
        let reasoning_effort = self
            .conversation_state()
            .active_conversation()
            .and_then(|conversation| {
                ai_conversation_reasoning_effort(conversation, &provider.id, &model)
            })
            .unwrap_or(configured_reasoning_effort);
        let reasoning_effort = oxideterm_ai::normalize_reasoning_level_for_model(
            &provider.provider_type,
            &model,
            reasoning_effort,
        )
        .as_str()
        .to_string();
        let tools = ai_stream_tool_definitions(
            tool_policy.enabled,
            settings.skills.enabled,
            allow_web,
            &tool_policy,
            self.mcp_registry(),
        );
        let image_input =
            Self::ai_image_model_setting(settings, &provider.id, &provider.base_url, &model);
        let omit_history_images = self.ai_image_text_only(&provider.id, &provider.base_url, &model)
            && oxideterm_ai::model_image_input(&provider.provider_type, &model, image_input)
                != Some(true);
        Ok(AiChatStreamConfig {
            api_protocol: provider.api_protocol,
            image_input,
            omit_history_images,
            execution_backend: AiExecutionBackend::Provider,
            provider_id: Some(provider.id),
            acp_agent_id: None,
            acp_session_id: None,
            acp_config_selection: None,
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            model,
            api_key: None,
            max_response_tokens,
            reasoning_effort: Some(reasoning_effort),
            safety_mode: match self.active_conversation_safety_mode() {
                AiSafetyMode::Bypass => AiPolicySafetyMode::Bypass,
                AiSafetyMode::ReadOnly => AiPolicySafetyMode::ReadOnly,
                AiSafetyMode::Default => AiPolicySafetyMode::Default,
            },
            profile_id: active_profile_id,
            memory_context,
            memory_entry_ids,
            tool_policy,
            tools,
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        })
    }

    pub fn resolve_ai_summary_stream_config(
        &self,
        settings: &AiSettings,
        i18n: &I18n,
        compact: bool,
    ) -> Result<AiChatStreamConfig, String> {
        let providers = ai_provider_views(&settings.providers);
        let provider = active_provider_view(&providers, settings.active_provider_id.as_deref())
            .cloned()
            .ok_or_else(|| i18n.t("ai.model_selector.no_provider"))?;
        let model = active_model_selection(settings.active_model.as_deref())
            .ok_or_else(|| i18n.t("ai.model_selector.no_model_selected"))?;
        let max_response_tokens = if compact {
            let window = oxideterm_ai::model_context_window(
                &model,
                &settings.model_context_windows,
                Some(&provider.id),
                &settings.user_context_windows,
            )
            .max(1) as usize;
            Some(oxideterm_ai::agent::checkpoint_output_budget(window) as i64)
        } else {
            None
        };
        let configured_reasoning_effort = settings
            .reasoning_model_overrides
            .get(&provider.id)
            .and_then(|models| models.get(&model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");
        let reasoning_effort = self
            .conversation_state()
            .active_conversation()
            .and_then(|conversation| {
                ai_conversation_reasoning_effort(conversation, &provider.id, &model)
            })
            .unwrap_or(configured_reasoning_effort);
        let reasoning_effort = oxideterm_ai::normalize_reasoning_level_for_model(
            &provider.provider_type,
            &model,
            reasoning_effort,
        )
        .as_str()
        .to_string();
        let image_input =
            Self::ai_image_model_setting(settings, &provider.id, &provider.base_url, &model);
        let omit_history_images = self.ai_image_text_only(&provider.id, &provider.base_url, &model)
            && oxideterm_ai::model_image_input(&provider.provider_type, &model, image_input)
                != Some(true);
        Ok(AiChatStreamConfig {
            api_protocol: provider.api_protocol,
            image_input,
            omit_history_images,
            execution_backend: AiExecutionBackend::Provider,
            provider_id: Some(provider.id),
            acp_agent_id: None,
            acp_session_id: None,
            acp_config_selection: None,
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            model,
            api_key: None,
            max_response_tokens,
            reasoning_effort: Some(reasoning_effort),
            safety_mode: match self.active_conversation_safety_mode() {
                AiSafetyMode::Bypass => AiPolicySafetyMode::Bypass,
                AiSafetyMode::ReadOnly => AiPolicySafetyMode::ReadOnly,
                AiSafetyMode::Default => AiPolicySafetyMode::Default,
            },
            profile_id: None,
            memory_context: None,
            memory_entry_ids: Vec::new(),
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        })
    }
}

pub fn build_ai_base_system_prompt(
    settings: &AiSettings,
    config: &AiChatStreamConfig,
    rag_system_prompt: Option<&str>,
    task_system_prompt: Option<&str>,
    skill_catalog_prompt: Option<String>,
) -> String {
    let providers = ai_provider_views(&settings.providers);
    let provider = active_provider_view(&providers, config.provider_id.as_deref());
    let provider_label = provider
        .map(|provider| provider.name.as_str())
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(config.provider_type.as_str());
    // This is the final provider boundary for custom, RAG, and runtime context.
    let mut prompt = oxideterm_ai::sanitize_for_ai(settings.custom_system_prompt.trim());
    if prompt.is_empty() {
        prompt = DEFAULT_AI_SYSTEM_PROMPT.to_string();
    }
    let safe_model = oxideterm_ai::sanitize_for_ai(&config.model);
    let safe_provider_label = oxideterm_ai::sanitize_for_ai(provider_label);
    prompt.push_str(&format!(
        "\nYou are currently the model \"{}\", provided by {}.",
        safe_model, safe_provider_label
    ));
    let context_window =
        ai_model_context_window(settings, config.provider_id.as_deref(), &config.model);
    let memory_character_budget = context_window
        .saturating_mul(4)
        .saturating_div(8)
        .clamp(4_000, 32_000);
    if let Some(memory) = config.memory_context.as_deref().and_then(|memory| {
        oxideterm_ai::ai_user_memory_prompt_with_limit(
            memory,
            settings.memory.enabled,
            memory_character_budget,
        )
    }) {
        prompt.push_str("\n\n");
        prompt.push_str(&memory);
    }
    if let Some(rag_system_prompt) = rag_system_prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        prompt.push_str("\n\n");
        prompt.push_str(&oxideterm_ai::sanitize_for_ai(rag_system_prompt));
    }
    if let Some(task_system_prompt) = task_system_prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        prompt.push_str("\n\n");
        prompt.push_str(&oxideterm_ai::sanitize_for_ai(task_system_prompt));
    }
    if let Some(skill_catalog_prompt) = skill_catalog_prompt {
        if let Some(mut context) = oxideterm_audit::AuditContext::current_request()
            .or_else(oxideterm_audit::AuditContext::current)
        {
            context.source = oxideterm_audit::AuditSource::Ai;
            context.observe(
                oxideterm_audit::AuditCategory::Automation,
                "ai_context_provided",
                Some(&format!(
                    "provider=skills model={} bytes={}",
                    config.model,
                    skill_catalog_prompt.len()
                )),
                oxideterm_audit::AuditOutcome::Sent,
                oxideterm_audit::AuditEvidence::Dispatch,
                oxideterm_audit::AuditAuthorization::NotRequired,
            );
        }
        prompt.push_str("\n\n");
        prompt.push_str(&skill_catalog_prompt);
    }
    if context_window >= 8192 {
        prompt.push_str(AI_SUGGESTIONS_INSTRUCTION);
    }
    prompt.push_str("\n\n");
    prompt.push_str(&ai_orchestrator_system_prompt(config.tool_policy.enabled));
    prompt
}
