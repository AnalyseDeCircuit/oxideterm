use crate::state::AiWorkspaceEntity;
use gpui::Context;
use oxideterm_ai::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiExecutionBackend, AiPolicySafetyMode,
    AiToolUsePolicy, active_model_selection, active_provider_view,
    provider_views as ai_provider_views,
};

impl AiWorkspaceEntity {
    pub fn submit_inline_prompt(
        &mut self,
        regenerate: bool,
        os_context: String,
        settings: &oxideterm_settings::AiSettings,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) {
        if regenerate {
            if self.terminal_inline_panel().loading {
                return;
            }
            let panel = self.terminal_inline_panel_mut();
            panel.response.clear();
            panel.error = None;
        }
        let Some((prompt, selection)) = self.terminal_inline_request_context() else {
            return;
        };
        let messages = terminal_ai_inline_messages(os_context, selection, prompt);
        self.request_terminal_inline(
            resolve_terminal_ai_inline_config(settings, i18n),
            messages,
            i18n.t("ai.model_selector.api_key_not_found"),
            i18n.t("ai.model_selector.failed_to_get_api_key"),
            i18n.t("settings_view.ai.acp_agent_error_unknown"),
        );
        cx.notify();
    }
}

pub fn resolve_terminal_ai_inline_config(
    settings: &oxideterm_settings::AiSettings,
    i18n: &oxideterm_i18n::I18n,
) -> Result<AiChatStreamConfig, String> {
    let providers = ai_provider_views(&settings.providers);
    let provider = active_provider_view(&providers, settings.active_provider_id.as_deref())
        .cloned()
        .ok_or_else(|| i18n.t("ai.model_selector.no_provider"))?;
    let model = active_model_selection(settings.active_model.as_deref())
        .ok_or_else(|| i18n.t("ai.model_selector.no_model_selected"))?;
    let reasoning_effort = settings
        .reasoning_model_overrides
        .get(&provider.id)
        .and_then(|models| models.get(&model))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("auto");
    let reasoning_effort = oxideterm_ai::normalize_reasoning_level_for_model(
        &provider.provider_type,
        &model,
        reasoning_effort,
    )
    .as_str()
    .to_string();
    Ok(AiChatStreamConfig {
        api_protocol: provider.api_protocol,
        image_input: Default::default(),
        omit_history_images: false,
        execution_backend: AiExecutionBackend::Provider,
        provider_id: Some(provider.id.clone()),
        acp_agent_id: None,
        acp_session_id: None,
        acp_config_selection: None,
        provider_type: provider.provider_type,
        base_url: provider.base_url,
        model: model.clone(),
        api_key: None,
        max_response_tokens: None,
        reasoning_effort: Some(reasoning_effort),
        safety_mode: AiPolicySafetyMode::Default,
        profile_id: None,
        memory_context: None,
        memory_entry_ids: Vec::new(),
        tool_policy: AiToolUsePolicy::default(),
        tools: Vec::new(),
        tool_choice: oxideterm_ai::AiToolChoice::Auto,
    })
}
fn terminal_ai_inline_messages(
    os_context: String,
    selection_context: String,
    prompt: String,
) -> Vec<AiChatMessage> {
    let mut user_content = String::new();
    if !selection_context.trim().is_empty() {
        user_content.push_str("### Context (Selected Text):\n");
        user_content.push_str(&selection_context);
        user_content.push_str("\n\n");
    }
    user_content.push_str("### Question/Instruction:\n");
    user_content.push_str(&prompt);

    vec![
        AiChatMessage {
            id: "terminal-inline-system".to_string(),
            role: AiChatRole::System,
            content: format!(
                "You are OxideSens, an expert terminal assistant. Environment: {os_context}. Respond ONLY with the command or code itself unless asked for explanation. If asked which AI model you are, answer truthfully."
            ),
            timestamp_ms: 0,
            model: None,
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
        },
        AiChatMessage {
            id: "terminal-inline-user".to_string(),
            role: AiChatRole::User,
            content: user_content,
            timestamp_ms: 0,
            model: None,
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
        },
    ]
}

pub fn truncate_ai_inline_context(mut context: String, max_chars: i64) -> String {
    let max_chars = usize::try_from(max_chars).unwrap_or_default();
    if max_chars == 0 || context.chars().count() <= max_chars {
        return context;
    }
    let keep_from = context
        .char_indices()
        .rev()
        .nth(max_chars.saturating_sub(1))
        .map(|(index, _)| index)
        .unwrap_or_default();
    context.drain(..keep_from);
    context
}
