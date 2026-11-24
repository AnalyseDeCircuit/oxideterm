use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use parking_lot::Mutex;

use super::{
    AiPromptTokenBreakdown, ai_message_payload_estimated_tokens, ai_prompt_token_breakdown,
};
use crate::{AiChatMessage, AiChatStreamConfig};

/// One conversation's most recent measured request. No prompt text is retained.
#[derive(Clone, Default)]
pub struct AiInputUsage(Arc<Mutex<Option<(AiRequestPrefix, usize)>>>);

pub(crate) struct AiRequestPrefix {
    scope: u64,
    messages: usize,
    signature: u64,
    runtime_tokens: usize,
    turn_context_tokens: usize,
    breakdown: AiPromptTokenBreakdown,
}

impl AiChatStreamConfig {
    pub fn input_scope(&self) -> u64 {
        let mut hash = DefaultHasher::new();
        (
            &self.provider_id,
            &self.provider_type,
            &self.base_url,
            &self.model,
            &self.reasoning_effort,
            self.omit_history_images,
        )
            .hash(&mut hash);
        std::mem::discriminant(&self.api_protocol).hash(&mut hash);
        std::mem::discriminant(&self.image_input).hash(&mut hash);
        self.tool_choice.hash(&mut hash);
        self.tools.hash(&mut hash);
        hash.finish()
    }
}

pub(crate) const RUNTIME_CONTEXT_POLICY_MESSAGE_ID: &str = "runtime-context-policy";

fn is_runtime(message: &AiChatMessage) -> bool {
    message.role == crate::AiChatRole::System
        && matches!(
            message.id.as_str(),
            crate::RUNTIME_CONTEXT_MESSAGE_ID | RUNTIME_CONTEXT_POLICY_MESSAGE_ID
        )
}

fn is_turn_context(message: &AiChatMessage) -> bool {
    message.role == crate::AiChatRole::System && super::is_runtime_ai_history_system(message)
}

fn hash_message(message: &AiChatMessage, hash: &mut DefaultHasher) {
    std::mem::discriminant(&message.role).hash(hash);
    message.content.hash(hash);
    message.tool_call_id.hash(hash);
    message.tool_calls.hash(hash);
    message.thinking_content.hash(hash);
    message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("providerParts"))
        .hash(hash);
    for image in crate::message_images(message) {
        (&image.id, image.width, image.height).hash(hash);
    }
}

#[cfg(not(target_family = "wasm"))]
impl AiRequestPrefix {
    pub(crate) fn capture(
        config: &AiChatStreamConfig,
        history: &[AiChatMessage],
        breakdown: AiPromptTokenBreakdown,
    ) -> Self {
        let mut hash = DefaultHasher::new();
        let mut messages = 0;
        let mut runtime_tokens = 0;
        let mut turn_context_tokens = 0;
        for message in history {
            if is_runtime(message) {
                runtime_tokens +=
                    ai_message_payload_estimated_tokens(message, &config.provider_type);
            } else if is_turn_context(message) {
                turn_context_tokens +=
                    ai_message_payload_estimated_tokens(message, &config.provider_type);
            } else {
                messages += 1;
                hash_message(message, &mut hash);
            }
        }
        Self {
            scope: config.input_scope(),
            messages,
            signature: hash.finish(),
            runtime_tokens,
            turn_context_tokens,
            breakdown,
        }
    }
}

impl AiInputUsage {
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn record(&self, prefix: AiRequestPrefix, input: usize) {
        *self.0.lock() = Some((prefix, input));
    }

    pub fn clear(&self) {
        self.0.lock().take();
    }

    pub fn cache_key(&self) -> impl Hash {
        self.0.lock().as_ref().map(|(p, input)| {
            (
                p.scope,
                p.runtime_tokens,
                p.turn_context_tokens,
                p.signature,
                *input,
            )
        })
    }

    pub fn last_input_for(&self, scope: u64) -> Option<usize> {
        self.0
            .lock()
            .as_ref()
            .filter(|(prefix, _)| prefix.scope == scope)
            .map(|(_, input)| *input)
    }

    fn estimate(
        &self,
        config: &AiChatStreamConfig,
        history: &[AiChatMessage],
        reserved: usize,
        scope: u64,
    ) -> Option<AiPromptTokenBreakdown> {
        let guard = self.0.lock();
        let (prefix, actual) = guard.as_ref()?;
        if prefix.scope != scope {
            return None;
        }
        let mut hash = DefaultHasher::new();
        let mut messages = history
            .iter()
            .filter(|message| !is_runtime(message) && !is_turn_context(message));
        for _ in 0..prefix.messages {
            hash_message(messages.next()?, &mut hash);
        }
        if hash.finish() != prefix.signature {
            return None;
        }
        let mut predicted = *actual;
        let mut result = prefix.breakdown.clone();
        result.reserved_output = reserved;
        // The idle preview has no live observations yet. Keep their last measured
        // contribution until the tool loop supplies this request's observations.
        if history.iter().any(is_runtime) {
            let runtime = history
                .iter()
                .filter(|m| is_runtime(m))
                .map(|m| ai_message_payload_estimated_tokens(m, &config.provider_type))
                .sum::<usize>();
            predicted = predicted
                .saturating_add(runtime)
                .saturating_sub(prefix.runtime_tokens);
            result.system_instructions = result
                .system_instructions
                .saturating_sub(prefix.runtime_tokens)
                .saturating_add(runtime);
        }
        // Per-turn context can move after the newest user message, or disappear.
        let turn_context_tokens: usize = history
            .iter()
            .filter(|m| is_turn_context(m))
            .map(|m| ai_message_payload_estimated_tokens(m, &config.provider_type))
            .sum();
        predicted = predicted
            .saturating_add(turn_context_tokens)
            .saturating_sub(prefix.turn_context_tokens);
        result.system_instructions = result
            .system_instructions
            .saturating_sub(prefix.turn_context_tokens)
            .saturating_add(turn_context_tokens);
        for message in messages {
            let delta = ai_prompt_token_breakdown(
                std::slice::from_ref(message),
                &[],
                &config.provider_type,
                0,
            );
            result.system_instructions += delta.system_instructions;
            result.messages += delta.messages;
            result.tool_results += delta.tool_results;
            predicted = predicted.saturating_add(delta.prompt_tokens());
        }
        result.next_input = Some(predicted);
        result.last_actual_input = Some(*actual);
        Some(result)
    }
}

pub fn ai_request_token_breakdown(
    history: &[AiChatMessage],
    config: &AiChatStreamConfig,
    reserved: usize,
) -> AiPromptTokenBreakdown {
    let scope = config.input_scope();
    if let Some(result) = config
        .input_usage
        .estimate(config, history, reserved, scope)
    {
        return result;
    }
    let mut result =
        ai_prompt_token_breakdown(history, &config.tools, &config.provider_type, reserved);
    result.request_scope = scope;
    result.last_actual_input = config.input_usage.last_input_for(scope);
    result
}
