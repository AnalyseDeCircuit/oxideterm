use oxideterm_ai::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiConversation, active_model_selection,
    active_provider_view, provider_views as ai_provider_views, stream_state::*,
};
use oxideterm_settings::{AiActiveBackend, AiSettings};

use crate::{
    sidebar::transcript::ai_chat_message_base_signature,
    state::{
        AiChatMessageSignatureCache, AiContextTokenBreakdown, AiContextTokenBreakdownKey,
        AiWorkspaceEntity,
        skills::{AiSkillCatalog, AiSkillCatalogHost},
    },
    stream::ai_context_token_breakdown_from_prompt,
};

use super::{
    ai_model_context_window, build_ai_base_system_prompt, build_ai_memory_context_prompt,
    build_ai_turn_context_prompt,
};

pub struct AiConversationPrompt<'a, Skills: AiSkillCatalogHost + ?Sized = AiSkillCatalog> {
    pub state: &'a AiWorkspaceEntity,
    pub settings: &'a AiSettings,
    pub settings_updated_at: u64,
    pub skills: &'a Skills,
}

fn append_deduped_context_message(history: &mut Vec<AiChatMessage>, id: &str, content: String) {
    if history
        .iter()
        .rev()
        .find(|message| message.id == id)
        .is_some_and(|previous| previous.content == content)
    {
        return;
    }
    history.push(AiChatMessage {
        id: id.to_owned(),
        role: AiChatRole::System,
        content,
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
    });
}

fn insert_transcript_lookup(history: &mut Vec<AiChatMessage>, content: String) {
    // Append as a trailing user-context message instead of inserting at index 1
    // so crossing budget level 3 never mutates the cached static system prefix.
    append_deduped_context_message(history, "transcript-lookup-reference", content);
}

impl<Skills: AiSkillCatalogHost + ?Sized> AiConversationPrompt<'_, Skills> {
    fn context_window(&self, config: &AiChatStreamConfig) -> usize {
        ai_model_context_window(self.settings, config.provider_id.as_deref(), &config.model)
    }

    pub fn should_force_ai_pre_send_compaction(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
    ) -> bool {
        let Some(conversation) = self
            .state
            .history
            .model_contexts
            .get(conversation_id)
            .or_else(|| {
                self.state
                    .conversation_state()
                    .conversations
                    .iter()
                    .find(|c| c.id == conversation_id)
            })
        else {
            return false;
        };
        let history = self.compose_ai_stream_history_for_conversation(
            Some(conversation_id),
            conversation.messages.clone(),
            config,
            request_content.map(str::to_owned),
            task_system_prompt,
            rag_system_prompt,
        );
        history
            .iter()
            .filter(|m| m.role != AiChatRole::System)
            .take(4)
            .count()
            == 4
            && ai_input_needs_compaction(
                ai_request_token_breakdown(&history, config, 0).prompt_tokens(),
                self.context_window(config),
            )
            && ai_find_prompt_transcript_lookup_reference(&conversation.messages).is_none()
    }

    pub fn should_auto_compact_history(
        &self,
        conversation: &AiConversation,
        mut config: AiChatStreamConfig,
    ) -> bool {
        config.input_usage = self.state.input_usage_for(&conversation.id);
        let history = self.compose_ai_stream_history_for_conversation(
            Some(&conversation.id),
            conversation.messages.clone(),
            &config,
            None,
            None,
            None,
        );
        let next = ai_request_token_breakdown(&history, &config, 0).prompt_tokens();
        let draft = if self
            .state
            .conversation_state()
            .active_conversation_id
            .as_deref()
            == Some(&conversation.id)
        {
            self.draft_tokens(&config)
        } else {
            0
        };
        ai_input_needs_compaction(next.saturating_add(draft), self.context_window(&config))
    }

    pub fn build_ai_stream_history(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        rag_system_prompt: Option<String>,
    ) -> Option<(Vec<AiChatMessage>, usize)> {
        let transcript_lookup_prompt = self.ai_transcript_lookup_prompt_for_conversation(
            conversation_id,
            config,
            request_content.as_deref(),
            task_system_prompt.as_deref(),
            rag_system_prompt.as_deref(),
        );
        let history = self
            .state
            .history
            .model_contexts
            .get(conversation_id)
            .map(|conversation| conversation.messages.clone())?;
        let mut history = self.compose_ai_stream_history_for_conversation(
            Some(conversation_id),
            history,
            config,
            request_content,
            task_system_prompt.as_deref(),
            rag_system_prompt.as_deref(),
        );
        let context_window = self.context_window(config);
        if let Some(transcript_lookup_prompt) = transcript_lookup_prompt {
            insert_transcript_lookup(&mut history, transcript_lookup_prompt);
        }
        let reserve = config
            .max_response_tokens
            .and_then(|tokens| usize::try_from(tokens).ok())
            .filter(|tokens| *tokens > 0)
            .unwrap_or_else(|| ai_response_reserve(context_window));
        let breakdown = ai_request_token_breakdown(&history, config, reserve);
        let trimmed_count = if breakdown.next_input.is_some()
            && breakdown.prompt_tokens()
                <= compute_ai_prompt_budget(context_window, reserve, 0, None).usable_prompt_budget
        {
            0
        } else {
            trim_ai_stream_history_to_request_budget(
                &mut history,
                &config.tools,
                &config.provider_type,
                context_window,
                reserve,
            )
        };
        Some((history, trimmed_count))
    }

    pub fn compose_ai_stream_history(
        &self,
        history: Vec<AiChatMessage>,
        config: &AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
    ) -> Vec<AiChatMessage> {
        self.compose_ai_stream_history_for_conversation(
            None,
            history,
            config,
            request_content,
            task_system_prompt,
            rag_system_prompt,
        )
    }

    fn compose_ai_stream_history_for_conversation(
        &self,
        conversation_id: Option<&str>,
        history: Vec<AiChatMessage>,
        config: &AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
    ) -> Vec<AiChatMessage> {
        let base_system_prompt = build_ai_base_system_prompt(
            self.settings,
            config,
            self.skills.prompt(self.settings.skills.enabled),
        );
        let turn_context_prompt =
            build_ai_turn_context_prompt(rag_system_prompt, task_system_prompt);
        let memory_context_prompt = build_ai_memory_context_prompt(self.settings, config);
        let mut history = compose_ai_provider_history(
            history,
            config,
            request_content,
            base_system_prompt,
            turn_context_prompt,
        );
        if let Some(conversation_id) = conversation_id {
            let replays = self
                .state
                .live_turn_replays_for(conversation_id, config.input_scope());
            expand_ai_live_turn_replays(&mut history, &replays);
        }
        if let Some(memory) = memory_context_prompt {
            // Skip re-injecting identical memory context when an earlier replayed
            // turn already carries the exact same snapshot.
            append_deduped_context_message(&mut history, "memory-context", memory);
        }
        history
    }

    pub fn ai_send_budget_decision(
        &self,
        conversation: &AiConversation,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
    ) -> Option<AiPromptBudgetDecision> {
        let context_window = self.context_window(config);
        let history = self.compose_ai_stream_history_for_conversation(
            Some(&conversation.id),
            conversation.messages.clone(),
            config,
            request_content.map(str::to_string),
            task_system_prompt,
            rag_system_prompt,
        );
        Some(ai_send_budget_decision(
            conversation,
            config,
            &history,
            context_window,
        ))
    }

    pub fn ai_budget_diagnostic_payload(
        &self,
        conversation: &AiConversation,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
        decision: Option<AiPromptBudgetDecision>,
        trimmed_count: usize,
    ) -> serde_json::Value {
        let context_window = self.context_window(config);
        let history = self.compose_ai_stream_history_for_conversation(
            Some(&conversation.id),
            conversation.messages.clone(),
            config,
            request_content.map(str::to_string),
            task_system_prompt,
            rag_system_prompt,
        );
        ai_budget_diagnostic_payload(
            conversation,
            config,
            &history,
            context_window,
            decision,
            trimmed_count,
        )
    }

    pub fn ai_transcript_lookup_prompt_for_conversation(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
    ) -> Option<String> {
        let conversation = self
            .state
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)?;
        let decision = self.ai_send_budget_decision(
            conversation,
            config,
            request_content,
            task_system_prompt,
            rag_system_prompt,
        )?;
        (decision.level >= 3)
            .then(|| ai_find_prompt_transcript_lookup_reference(&conversation.messages))
            .flatten()
            .map(ai_build_transcript_lookup_prompt_reference)
    }

    fn active_ai_acp_usage(&self) -> Option<(usize, usize)> {
        if self.settings.active_backend != AiActiveBackend::Acp {
            return None;
        }
        let agent_id = self.settings.active_acp_agent_id.as_deref()?;
        let usage = self.state.active_ai_acp_session_state(agent_id)?.usage?;
        let used = usage.get("used")?.as_u64()?.try_into().ok()?;
        let size = usage.get("size")?.as_u64()?.try_into().ok()?;
        Some((used, size))
    }

    pub fn ai_context_token_breakdown(
        &self,
        config: Option<&AiChatStreamConfig>,
    ) -> AiContextTokenBreakdown {
        let mut result = self.breakdown_without_draft(config);
        if let Some(config) = config {
            let draft = self.draft_tokens(config);
            result.messages = result.messages.saturating_add(draft);
            result.total = result.total.saturating_add(draft);
        }
        result
    }

    fn draft_tokens(&self, config: &AiChatStreamConfig) -> usize {
        let chat = self.state.chat_ui();
        ai_estimated_tokens(&chat.draft)
            + chat
                .images
                .iter()
                .map(|image| {
                    oxideterm_ai::estimate_image_tokens(
                        &config.provider_type,
                        &config.model,
                        image.width,
                        image.height,
                    )
                    .tokens
                })
                .sum::<usize>()
    }

    fn breakdown_without_draft(
        &self,
        config: Option<&AiChatStreamConfig>,
    ) -> AiContextTokenBreakdown {
        let settings = self.settings;
        let provider_id = config
            .as_ref()
            .and_then(|config| config.provider_id.as_deref())
            .unwrap_or("acp")
            .to_string();
        let model = config
            .as_ref()
            .map(|config| config.model.clone())
            .unwrap_or_else(|| {
                active_model_selection(settings.active_model.as_deref()).unwrap_or_default()
            });
        let max_tokens = config
            .as_ref()
            .map(|config| self.context_window(config))
            .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW);
        let conversation_fingerprint = {
            let ai = self.state;
            ai_conversation_token_fingerprint(
                ai.conversation_state().active_conversation(),
                &ai.chat_ui().message_signature_cache,
            )
        };
        let conversation = self.state.conversation_state().active_conversation();
        if let Some(prepared) =
            self.state
                .chat_ui()
                .prepared_prompt_usage
                .as_ref()
                .filter(|prepared| {
                    conversation
                        .is_some_and(|conversation| conversation.id == prepared.conversation_id)
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
                                    || (message.role == AiChatRole::Assistant
                                        && message.is_streaming
                                        && message.content.is_empty())
                            })
                        })
                        && config.is_some_and(|config| {
                            prepared.breakdown.request_scope == config.input_scope()
                                && config
                                    .input_usage
                                    .last_input_for(prepared.breakdown.request_scope)
                                    == prepared.breakdown.last_actual_input
                        })
                        && prepared.provider_id == provider_id
                        && prepared.model == model
                })
        {
            return prepared.breakdown.clone();
        }
        let conversation_id = conversation.map(|conversation| conversation.id.as_str());
        let request_configuration_fingerprint = config
            .as_ref()
            .map(|config| self.ai_request_configuration_fingerprint(conversation_id, config))
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
            let cache = self.state.chat_ui().context_token_cache.borrow();
            if cache.key.as_ref() == Some(&cache_key)
                && let Some(cached) = cache.breakdown_without_draft.as_ref()
            {
                return cached.clone();
            }
        }
        let breakdown_without_draft = if let Some(config) = config.as_ref() {
            let mut history = self.compose_ai_stream_history_for_conversation(
                conversation_id,
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
                    )
            {
                insert_transcript_lookup(&mut history, transcript_lookup_prompt);
            }
            let reserved_output = config
                .max_response_tokens
                .and_then(|tokens| usize::try_from(tokens).ok())
                .filter(|tokens| *tokens > 0)
                .unwrap_or_else(|| ai_response_reserve(max_tokens));
            // Show the proposed input before automatic compaction or emergency trimming.
            ai_context_token_breakdown_from_prompt(
                ai_request_token_breakdown(&history, config, reserved_output),
                max_tokens,
            )
        } else {
            AiContextTokenBreakdown {
                request_scope: 0,
                last_actual_input: None,
                system_instructions: 0,
                tool_definitions: 0,
                reserved_output: 0,
                messages: 0,
                tool_results: 0,
                total: 0,
                max_tokens,
            }
        };
        let mut cache = self.state.chat_ui().context_token_cache.borrow_mut();
        cache.key = Some(cache_key);
        cache.breakdown_without_draft = Some(breakdown_without_draft.clone());
        breakdown_without_draft
    }

    fn ai_request_configuration_fingerprint(
        &self,
        conversation_id: Option<&str>,
        config: &AiChatStreamConfig,
    ) -> u64 {
        let settings = self.settings;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.settings_updated_at, &mut hasher);
        std::hash::Hash::hash(&config.input_scope(), &mut hasher);
        std::hash::Hash::hash(&config.input_usage.cache_key(), &mut hasher);
        std::hash::Hash::hash(
            &self
                .state
                .live_turn_replay_fingerprint(conversation_id, config.input_scope()),
            &mut hasher,
        );
        std::hash::Hash::hash(&config.provider_id, &mut hasher);
        std::hash::Hash::hash(&config.provider_type, &mut hasher);
        std::hash::Hash::hash(&config.model, &mut hasher);
        std::hash::Hash::hash(&config.omit_history_images, &mut hasher);
        std::hash::Hash::hash(&config.max_response_tokens, &mut hasher);
        std::hash::Hash::hash(&config.tool_policy.enabled, &mut hasher);
        ai_hash_text_shape(settings.custom_system_prompt.trim(), &mut hasher);
        if let Some(memory_context) = config.memory_context.as_deref() {
            ai_hash_text_shape(memory_context, &mut hasher);
        }
        let providers = ai_provider_views(&settings.providers);
        if let Some(provider) = active_provider_view(&providers, config.provider_id.as_deref()) {
            ai_hash_text_shape(&provider.name, &mut hasher);
        }
        if settings.skills.enabled {
            // Skill content hashes make catalog edits invalidate the prompt
            // without rebuilding the full catalog string on every repaint.
            self.skills
                .visit_enabled_versions(&mut |id, hash, priority| {
                    ai_hash_text_shape(id, &mut hasher);
                    ai_hash_text_shape(hash, &mut hasher);
                    std::hash::Hash::hash(&priority, &mut hasher);
                });
        }
        std::hash::Hasher::finish(&hasher)
    }

    pub fn context_usage_counts(
        &self,
        config: impl FnOnce() -> Option<AiChatStreamConfig>,
    ) -> Option<(usize, usize)> {
        ai_context_counts_for_backend(
            self.settings.active_backend,
            self.active_ai_acp_usage(),
            || {
                let breakdown = self.ai_context_token_breakdown(config().as_ref());
                (breakdown.total, breakdown.max_tokens)
            },
        )
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

fn ai_conversation_token_fingerprint(
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

fn ai_hash_text_shape(text: &str, hasher: &mut std::collections::hash_map::DefaultHasher) {
    // Context usage cache correctness matters more than sampling a few bytes;
    // equal-length edits in the middle of a prompt must invalidate the entry.
    std::hash::Hash::hash(text.as_bytes(), hasher);
}
