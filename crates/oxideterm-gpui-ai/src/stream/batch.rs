use std::collections::VecDeque;

use oxideterm_ai::AiStreamEvent;

use super::{AiStreamDelivery, AiStreamDeliveryEvent};
use crate::state::{AiContextTokenBreakdown, AiPreparedPromptUsage, AiWorkspaceEntity};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AiStreamTextKind {
    Content,
    Thinking,
}

struct PendingText {
    generation: u64,
    conversation_id: String,
    assistant_id: String,
    kind: AiStreamTextKind,
    text: String,
}

pub struct AiChatDeliveryBatch {
    pending_text: Option<PendingText>,
    deliveries: VecDeque<AiStreamDelivery>,
}

impl AiChatDeliveryBatch {
    pub fn new(deliveries: VecDeque<AiStreamDelivery>) -> Self {
        Self {
            pending_text: None,
            deliveries,
        }
    }

    /// Apply conversation-owned deliveries up to the next operation that needs
    /// the window or the live runtime. Do not advance further until that
    /// operation has had a chance to cancel or replace its generation.
    pub fn next_host_delivery(&mut self, ai: &mut AiWorkspaceEntity) -> Option<AiStreamDelivery> {
        while let Some(delivery) = self.deliveries.pop_front() {
            if !ai.chat_stream_matches(
                delivery.generation,
                &delivery.conversation_id,
                &delivery.assistant_id,
            ) {
                // Dropping a stale request also drops its retained response channel.
                continue;
            }
            let AiStreamDelivery {
                generation,
                conversation_id,
                assistant_id,
                event,
            } = delivery;
            match event {
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Content(text)) => {
                    self.push_text(
                        ai,
                        generation,
                        conversation_id,
                        assistant_id,
                        AiStreamTextKind::Content,
                        text,
                    );
                }
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Thinking(text)) => {
                    self.push_text(
                        ai,
                        generation,
                        conversation_id,
                        assistant_id,
                        AiStreamTextKind::Thinking,
                        text,
                    );
                }
                AiStreamDeliveryEvent::Stream(
                    event @ (AiStreamEvent::Usage { .. }
                    | AiStreamEvent::ProviderResponsePart { .. }
                    | AiStreamEvent::ToolCall { .. }
                    | AiStreamEvent::ToolCallComplete { .. }),
                ) => {
                    self.flush_text(ai);
                    ai.apply_stream_event_state(
                        generation,
                        &conversation_id,
                        &assistant_id,
                        event,
                        None,
                    );
                }
                AiStreamDeliveryEvent::ToolImages(images) => {
                    self.flush_text(ai);
                    ai.update_chat_message(&conversation_id, &assistant_id, |message| {
                        let mut attachments = oxideterm_ai::message_images(message).to_vec();
                        for image in images {
                            if !attachments.iter().any(|existing| existing.id == image.id) {
                                attachments.push(image);
                            }
                        }
                        oxideterm_ai::set_message_images(message, attachments);
                    });
                    ai.persist_chat_state();
                }
                AiStreamDeliveryEvent::HistoryBarrier(sender) => {
                    self.flush_text(ai);
                    ai.persist_chat_state();
                    ai.history_barrier(sender);
                }
                AiStreamDeliveryEvent::Checkpoint(checkpoint) => {
                    self.flush_text(ai);
                    ai.update_chat_message(&conversation_id, &assistant_id, |message| {
                        oxideterm_ai::agent::set_message_checkpoint(message, &checkpoint);
                    });
                    ai.persist_chat_state();
                }
                AiStreamDeliveryEvent::PromptUsage {
                    last_user_message_id,
                    provider_id,
                    model,
                    breakdown,
                    max_tokens,
                } => {
                    if !ai.is_child_stream(generation) {
                        self.flush_text(ai);
                        ai.set_prepared_prompt_usage(AiPreparedPromptUsage {
                            conversation_id,
                            last_user_message_id,
                            provider_id,
                            model,
                            breakdown: ai_context_token_breakdown_from_prompt(
                                breakdown, max_tokens,
                            ),
                        });
                    }
                }
                event => {
                    return Some(AiStreamDelivery {
                        generation,
                        conversation_id,
                        assistant_id,
                        event,
                    });
                }
            }
        }
        self.flush_text(ai);
        None
    }

    pub fn push_text(
        &mut self,
        ai: &mut AiWorkspaceEntity,
        generation: u64,
        conversation_id: String,
        assistant_id: String,
        kind: AiStreamTextKind,
        text: String,
    ) {
        if text.is_empty() {
            return;
        }
        if let Some(pending) = &mut self.pending_text
            && pending.generation == generation
            && pending.conversation_id == conversation_id
            && pending.assistant_id == assistant_id
            && pending.kind == kind
        {
            pending.text.push_str(&text);
            return;
        }
        self.flush_text(ai);
        self.pending_text = Some(PendingText {
            generation,
            conversation_id,
            assistant_id,
            kind,
            text,
        });
    }

    pub fn flush_text(&mut self, ai: &mut AiWorkspaceEntity) {
        let Some(pending) = self.pending_text.take() else {
            return;
        };
        let event = match pending.kind {
            AiStreamTextKind::Content => AiStreamEvent::Content(pending.text),
            AiStreamTextKind::Thinking => AiStreamEvent::Thinking(pending.text),
        };
        ai.apply_stream_event_state(
            pending.generation,
            &pending.conversation_id,
            &pending.assistant_id,
            event,
            None,
        );
    }
}

pub fn ai_context_token_breakdown_from_prompt(
    breakdown: oxideterm_ai::AiPromptTokenBreakdown,
    max_tokens: usize,
) -> AiContextTokenBreakdown {
    AiContextTokenBreakdown {
        system_instructions: breakdown.system_instructions,
        tool_definitions: breakdown.tool_definitions,
        reserved_output: breakdown.reserved_output,
        messages: breakdown.messages,
        tool_results: breakdown.tool_results,
        // The ring reports context already sent to the model. Output reserve
        // remains visible in the popover but is not presented as used input.
        total: breakdown.prompt_tokens(),
        max_tokens,
    }
}
