use oxideterm_gpui_ai::sidebar::transcript::{
    ai_chat_hash_json_value, ai_chat_message_base_signature,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_tool_indicator_info(&self, cx: &App) -> (bool, usize) {
        let tool_use = &self.settings_store.settings().ai.tool_use;
        let enabled = tool_use.enabled;
        let tool_policy = ai_tool_use_policy_from_settings(tool_use);
        let active_tool_count = ai_active_tool_count(
            enabled,
            self.settings_store.settings().ai.skills.enabled,
            self.ai_web_provider_id().is_ok(),
            &tool_policy,
            self.ai.state.read(cx).mcp_registry(),
        );
        (enabled, active_tool_count)
    }

    pub(in crate::workspace) fn active_ai_safety_mode(&self, cx: &App) -> AiSafetyMode {
        self.ai.state.read(cx).active_conversation_safety_mode()
    }

    pub(in crate::workspace) fn ai_editor_text_width(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> f32 {
        let sidebar_width = self.ai.state.read(cx).chat_ui().sidebar_width;
        let text_width = if target == WorkspaceImeTarget::AiMessageEdit {
            self.text_input_anchors
                .bounds(target.anchor_id())
                .map(|bounds| f32::from(bounds.size.width))
                .unwrap_or(sidebar_width - AI_INPUT_SOFT_WRAP_CHROME_PX)
        } else {
            sidebar_width - AI_INPUT_SOFT_WRAP_CHROME_PX
        };
        text_width.max(1.0)
    }

    pub(in crate::workspace) fn ai_editor_visual_lines<'a>(
        &self,
        target: WorkspaceImeTarget,
        text: &'a str,
        cx: &App,
    ) -> Vec<AiInputVisualLine<'a>> {
        let width = self.ai_editor_text_width(target, cx);
        if target == WorkspaceImeTarget::AiMessageEdit {
            ai_input_measured_visual_lines(
                text,
                (width - self.tokens.metrics.form_caret_width).max(1.0),
                gpui::font(oxideterm_gpui_ui::tauri_ui_font_family(
                    &self.settings_store.settings().appearance.ui_font_family,
                )),
                &gpui::WindowTextSystem::new(cx.text_system().clone()),
            )
        } else {
            ai_input_visual_lines(
                text,
                ai_input_soft_wrap_columns(width + AI_INPUT_SOFT_WRAP_CHROME_PX),
            )
        }
    }

    fn active_ai_acp_usage(&self, cx: &App) -> Option<(usize, usize)> {
        if self.settings_store.settings().ai.active_backend != AiActiveBackend::Acp {
            return None;
        }
        let agent_id = self
            .settings_store
            .settings()
            .ai
            .active_acp_agent_id
            .as_deref()?;
        let usage = self.active_ai_acp_session_state(agent_id, cx)?.usage?;
        let used = usage.get("used")?.as_u64()?.try_into().ok()?;
        let size = usage.get("size")?.as_u64()?.try_into().ok()?;
        Some((used, size))
    }

    pub(in crate::workspace) fn ai_context_token_breakdown(
        &self,
        cx: &App,
    ) -> AiContextTokenBreakdown {
        let settings = self.settings_store.settings();
        let config = self.resolve_ai_stream_config(cx).ok();
        let provider_id = config
            .as_ref()
            .and_then(|config| config.provider_id.as_deref())
            .unwrap_or("acp")
            .to_string();
        let model = config
            .as_ref()
            .map(|config| config.model.clone())
            .unwrap_or_else(|| {
                active_model_selection(settings.ai.active_model.as_deref()).unwrap_or_default()
            });
        let max_tokens = config
            .as_ref()
            .map(|config| self.ai_active_model_context_window(config))
            .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW);
        let conversation_fingerprint = {
            let ai = self.ai.state.read(cx);
            ai_conversation_token_fingerprint(
                ai.conversation_state().active_conversation(),
                &ai.chat_ui().message_signature_cache,
            )
        };
        let conversation = self
            .ai
            .state
            .read(cx)
            .conversation_state()
            .active_conversation();
        if let Some(prepared) = self
            .ai
            .state
            .read(cx)
            .chat_ui()
            .prepared_prompt_usage
            .as_ref()
            .filter(|prepared| {
                conversation.is_some_and(|conversation| conversation.id == prepared.conversation_id)
                    && conversation
                        .and_then(|conversation| {
                            conversation
                                .messages
                                .iter()
                                .rev()
                                .find(|message| message.role == AiChatRole::User)
                        })
                        .map(|message| message.id.as_str())
                        == prepared.last_user_message_id.as_deref()
                    && conversation.is_some_and(|conversation| {
                        conversation.messages.last().is_some_and(|message| {
                            message.role == AiChatRole::User
                                || (message.role == AiChatRole::Assistant && message.is_streaming)
                        })
                    })
                    && prepared.provider_id == provider_id
                    && prepared.model == model
            })
        {
            return prepared.breakdown.clone();
        }
        let request_configuration_fingerprint = config
            .as_ref()
            .map(|config| self.ai_request_configuration_fingerprint(config))
            .unwrap_or(0);
        let cache_key = AiContextTokenBreakdownKey {
            conversation_id: conversation.map(|conversation| conversation.id.clone()),
            conversation_fingerprint,
            provider_id: provider_id,
            model: model,
            max_tokens,
            request_configuration_fingerprint,
        };
        {
            let cache = self
                .ai
                .state
                .read(cx)
                .chat_ui()
                .context_token_cache
                .borrow();
            if cache.key.as_ref() == Some(&cache_key)
                && let Some(cached) = cache.breakdown_without_draft.as_ref()
            {
                return cached.clone();
            }
        }
        let breakdown_without_draft = if let Some(config) = config.as_ref() {
            let mut history = self.compose_ai_stream_history(
                conversation
                    .map(|conversation| conversation.messages.clone())
                    .unwrap_or_default(),
                config,
                None,
                None,
                None,
            );
            if let Some(conversation) = conversation
                && let Some(transcript_lookup_prompt) = self
                    .ai_transcript_lookup_prompt_for_conversation(
                        &conversation.id,
                        config,
                        None,
                        None,
                        None,
                        cx,
                    )
            {
                history.insert(
                    1,
                    AiChatMessage {
                        id: "transcript-lookup-reference".to_string(),
                        role: AiChatRole::System,
                        content: transcript_lookup_prompt,
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
                );
            }
            let reserved_output = config
                .max_response_tokens
                .and_then(|tokens| usize::try_from(tokens).ok())
                .filter(|tokens| *tokens > 0)
                .unwrap_or_else(|| ai_response_reserve(max_tokens));
            // Match the send path exactly: the idle indicator describes the
            // next provider request, including history that would be trimmed.
            trim_ai_stream_history_to_request_budget(
                &mut history,
                &config.tools,
                &config.provider_type,
                max_tokens,
                reserved_output,
            );
            ai_context_token_breakdown_from_prompt(
                ai_prompt_token_breakdown(
                    &history,
                    &config.tools,
                    &config.provider_type,
                    reserved_output,
                ),
                max_tokens,
            )
        } else {
            AiContextTokenBreakdown {
                system_instructions: 0,
                tool_definitions: 0,
                reserved_output: 0,
                messages: 0,
                tool_results: 0,
                total: 0,
                max_tokens,
            }
        };
        let mut cache = self
            .ai
            .state
            .read(cx)
            .chat_ui()
            .context_token_cache
            .borrow_mut();
        cache.key = Some(cache_key);
        cache.breakdown_without_draft = Some(breakdown_without_draft.clone());
        breakdown_without_draft
    }

    fn ai_request_configuration_fingerprint(&self, config: &AiChatStreamConfig) -> u64 {
        let settings = self.settings_store.settings();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.settings_store.updated_at(), &mut hasher);
        std::hash::Hash::hash(&config.provider_id, &mut hasher);
        std::hash::Hash::hash(&config.provider_type, &mut hasher);
        std::hash::Hash::hash(&config.model, &mut hasher);
        std::hash::Hash::hash(&config.omit_history_images, &mut hasher);
        std::hash::Hash::hash(&config.max_response_tokens, &mut hasher);
        std::hash::Hash::hash(&config.tool_policy.enabled, &mut hasher);
        ai_hash_text_shape(settings.ai.custom_system_prompt.trim(), &mut hasher);
        if let Some(memory_context) = config.memory_context.as_deref() {
            ai_hash_text_shape(memory_context, &mut hasher);
        }
        let providers = ai_provider_views(&settings.ai.providers);
        if let Some(provider) = active_provider_view(&providers, config.provider_id.as_deref()) {
            ai_hash_text_shape(&provider.name, &mut hasher);
        }
        for tool in &config.tools {
            ai_hash_text_shape(&tool.name, &mut hasher);
            ai_hash_text_shape(&tool.description, &mut hasher);
            ai_chat_hash_json_value(&tool.parameters, &mut hasher);
        }
        if settings.ai.skills.enabled {
            // Skill content hashes make catalog edits invalidate the prompt
            // without rebuilding the full catalog string on every repaint.
            for skill in self
                .ai
                .skills.read()
                .records()
                .filter(|skill| skill.enabled)
            {
                ai_hash_text_shape(&skill.id, &mut hasher);
                ai_hash_text_shape(&skill.content_hash, &mut hasher);
                std::hash::Hash::hash(&skill.priority, &mut hasher);
            }
        }
        std::hash::Hasher::finish(&hasher)
    }

    pub(in crate::workspace) fn ai_chat_autocomplete_items(
        &self,
        cx: &App,
    ) -> Vec<AiAutocompleteCandidate> {
        let settings = &self.settings_store.settings().ai;
        self.ai
            .state
            .read(cx)
            .chat_autocomplete_items(settings.active_backend, || {
                if settings.skills.enabled {
                    self.ai
                        .skills.read()
                        .catalog()
                        .into_iter()
                        .map(|skill| (skill.id, skill.description))
                        .collect()
                } else {
                    Vec::new()
                }
            })
    }

    pub(in crate::workspace) fn apply_ai_chat_autocomplete_candidate(
        &mut self,
        candidate: &AiAutocompleteCandidate,
        cx: &mut Context<Self>,
    ) {
        self.ai.state.update(cx, |ai, cx| {
            ai.apply_chat_autocomplete(candidate);
            cx.notify();
        });
        self.ime_marked_text = None;
    }
}

fn ai_context_counts_for_backend(
    backend: AiActiveBackend,
    acp_usage: Option<(usize, usize)>,
    provider_estimate: impl FnOnce() -> (usize, usize),
) -> Option<(usize, usize)> {
    // ACP owns its history and model budget; the provider estimate cannot measure it.
    match backend {
        AiActiveBackend::Acp => acp_usage.filter(|(_, size)| *size > 0),
        AiActiveBackend::Provider => Some(provider_estimate()),
    }
}

#[cfg(test)]
mod context_usage_tests {
    use super::*;

    #[test]
    fn acp_context_usage_does_not_fall_back_to_provider_history_estimates() {
        for (reported, expected, danger) in [
            (None, None, false),
            (Some((12_000, 100_000)), Some((12_000, 100_000)), false),
            (Some((90_000, 100_000)), Some((90_000, 100_000)), true),
            (Some((900, 0)), None, false),
        ] {
            let counts =
                ai_context_counts_for_backend(AiActiveBackend::Acp, reported, || (95_000, 100_000));
            assert_eq!(counts, expected);
            assert_eq!(
                counts.is_some_and(|(used, size)| {
                    ai_context_percentage(used, size) > AI_CONTEXT_DANGER_PERCENT
                }),
                danger,
            );
        }
        assert_eq!(
            ai_context_counts_for_backend(
                AiActiveBackend::Provider,
                Some((12_000, 100_000)),
                || (95_000, 100_000),
            ),
            Some((95_000, 100_000)),
        );
        assert_eq!(
            ai_context_counts_for_backend(AiActiveBackend::Acp, None, || {
                panic!("ACP must not prepare a provider prompt")
            }),
            None,
        );
    }
}

use oxideterm_gpui_ai::sidebar::editor::{
    AI_INPUT_SOFT_WRAP_CHROME_PX, ai_input_measured_visual_lines, ai_input_soft_wrap_columns,
};
pub(in crate::workspace) use oxideterm_gpui_ai::sidebar::editor::{
    AiInputVisualLine, ai_input_line_index_for_offset, ai_input_visual_lines,
};

pub(in crate::workspace) fn ai_format_tokens(tokens: usize) -> String {
    if tokens >= 1000 {
        format!("{:.1}K", tokens as f32 / 1000.0)
    } else {
        tokens.to_string()
    }
}

pub(in crate::workspace) fn ai_context_percent(tokens: usize, max_tokens: usize) -> String {
    if max_tokens == 0 {
        return "0%".to_string();
    }
    let percent = (tokens as f32 / max_tokens as f32) * 100.0;
    if percent > 0.0 && percent < 0.1 {
        "<0.1%".to_string()
    } else {
        format!("{percent:.1}%")
    }
}

use oxideterm_gpui_ai::stream::ai_context_token_breakdown_from_prompt;

pub(in crate::workspace) fn ai_conversation_token_fingerprint(
    conversation: Option<&AiConversation>,
    signature_cache: &std::cell::RefCell<AiChatMessageSignatureCache>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let Some(conversation) = conversation else {
        return 0;
    };
    std::hash::Hash::hash(&conversation.id, &mut hasher);
    std::hash::Hash::hash(&conversation.messages.len(), &mut hasher);
    let mut signature_cache = signature_cache.borrow_mut();
    signature_cache.select_conversation(&conversation.id);
    for message in &conversation.messages {
        // Streaming already invalidates the changed row in this shared cache,
        // so long conversations are not rehashed during unrelated repaints.
        let signature =
            signature_cache.signature_for(&message.id, || ai_chat_message_base_signature(message));
        std::hash::Hash::hash(&signature, &mut hasher);
    }
    std::hash::Hasher::finish(&hasher)
}

pub(in crate::workspace) fn ai_hash_text_shape(
    text: &str,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    // Context usage cache correctness matters more than sampling a few bytes;
    // equal-length edits in the middle of a prompt must invalidate the entry.
    std::hash::Hash::hash(text.as_bytes(), hasher);
}

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_context_message_usage_counts(
        &self,
        cx: &App,
    ) -> Option<(usize, usize)> {
        ai_context_counts_for_backend(
            self.settings_store.settings().ai.active_backend,
            self.active_ai_acp_usage(cx),
            || {
                let breakdown = self.ai_context_token_breakdown(cx);
                (breakdown.total, breakdown.max_tokens)
            },
        )
    }
}
