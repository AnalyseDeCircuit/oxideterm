use std::collections::VecDeque;

use oxideterm_ai::AiStreamEvent;

use super::{
    AiStreamDelivery, AiStreamDeliveryEvent, PendingCursor, PortableStreamDelivery,
    PortableStreamEvent, QueueCursor, QueueQuestion,
};
use crate::state::{AiContextTokenBreakdown, AiPreparedPromptUsage, AiWorkspaceEntity};

pub enum AiChatWindowDelivery {
    Host(AiStreamDelivery),
    Attention {
        generation: u64,
        conversation_id: String,
        assistant_id: String,
        label: &'static str,
    },
    FocusCandidate {
        generation: u64,
    },
}

impl AiChatWindowDelivery {
    pub(crate) fn is_current(&self, ai: &AiWorkspaceEntity) -> bool {
        match self {
            Self::Host(delivery) => ai.chat_stream_matches(
                delivery.generation,
                &delivery.conversation_id,
                &delivery.assistant_id,
            ),
            Self::Attention {
                generation,
                conversation_id,
                assistant_id,
                ..
            } => ai.chat_stream_matches(*generation, conversation_id, assistant_id),
            Self::FocusCandidate { generation } => {
                ai.chat_stream_generation() == *generation
                    && ai.is_chat_stream_generation(*generation)
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AiStreamTextKind {
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

#[derive(Default)]
pub(crate) struct AiChatDeliveryBatch {
    pending_text: Option<PendingText>,
    deliveries: VecDeque<AiStreamDelivery>,
    #[cfg(not(target_family = "wasm"))]
    pub(crate) hosted: VecDeque<PortableStreamDelivery>,
}

impl AiChatDeliveryBatch {
    #[cfg(test)]
    pub(crate) fn new(deliveries: VecDeque<AiStreamDelivery>) -> Self {
        Self {
            deliveries,
            ..Default::default()
        }
    }

    pub(crate) fn extend(&mut self, deliveries: impl IntoIterator<Item = AiStreamDelivery>) {
        self.deliveries.extend(deliveries);
    }

    pub(crate) fn has_deliveries(&self) -> bool {
        #[cfg(not(target_family = "wasm"))]
        if !self.hosted.is_empty() {
            return true;
        }
        !self.deliveries.is_empty()
    }

    /// Apply conversation-owned deliveries up to the next operation that needs
    /// the window or the live runtime. Do not advance further until that
    /// operation has had a chance to cancel or replace its generation.
    pub(crate) fn next_window_delivery(
        &mut self,
        ai: &mut AiWorkspaceEntity,
        i18n: &oxideterm_i18n::I18n,
        diagnostic: impl Fn(serde_json::Value) -> serde_json::Value,
        cx: &mut gpui::Context<AiWorkspaceEntity>,
    ) -> Option<AiChatWindowDelivery> {
        loop {
            let Some(delivery) = self.deliveries.pop_front() else {
                #[cfg(not(target_family = "wasm"))]
                if let Some(hosted) = self.hosted.pop_front() {
                    if ai.chat_stream_matches(
                        hosted.generation,
                        &hosted.conversation_id,
                        &hosted.assistant_id,
                    ) && let Some(effect) = self.reduce(hosted, ai, i18n, &diagnostic, cx)
                    {
                        return Some(effect);
                    }
                    continue;
                }
                break;
            };
            if !ai.chat_stream_matches(
                delivery.generation,
                &delivery.conversation_id,
                &delivery.assistant_id,
            ) {
                // Dropping a stale request also drops its retained response channel.
                continue;
            }
            #[cfg(not(target_family = "wasm"))]
            let delivery = match super::native::route(delivery, &mut self.deliveries) {
                super::native::NativeDelivery::Reduce(delivery) => delivery,
                super::native::NativeDelivery::Host(delivery) => {
                    self.flush_text(ai);
                    return Some(AiChatWindowDelivery::Host(delivery));
                }
                super::native::NativeDelivery::Expanded => continue,
            };
            if let Some(effect) = self.reduce(delivery, ai, i18n, &diagnostic, cx) {
                return Some(effect);
            }
        }
        self.flush_text(ai);
        None
    }
    fn reduce<Q: QueueQuestion, C: QueueCursor>(
        &mut self,
        delivery: PortableStreamDelivery<Q, C>,
        ai: &mut AiWorkspaceEntity,
        i18n: &oxideterm_i18n::I18n,
        diagnostic: &impl Fn(serde_json::Value) -> serde_json::Value,
        cx: &mut gpui::Context<AiWorkspaceEntity>,
    ) -> Option<AiChatWindowDelivery> {
        match delivery.event {
            PortableStreamEvent::Stream(AiStreamEvent::Content(text)) => {
                self.push_text(
                    ai,
                    delivery.generation,
                    delivery.conversation_id,
                    delivery.assistant_id,
                    AiStreamTextKind::Content,
                    text,
                );
            }
            PortableStreamEvent::Stream(AiStreamEvent::Thinking(text)) => {
                self.push_text(
                    ai,
                    delivery.generation,
                    delivery.conversation_id,
                    delivery.assistant_id,
                    AiStreamTextKind::Thinking,
                    text,
                );
            }
            PortableStreamEvent::Stream(
                event @ (AiStreamEvent::Usage { .. }
                | AiStreamEvent::ProviderResponsePart { .. }
                | AiStreamEvent::ToolCall { .. }
                | AiStreamEvent::ToolCallComplete { .. }),
            ) => {
                self.flush_text(ai);
                ai.apply_stream_event_state(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    event,
                    None,
                );
            }
            PortableStreamEvent::ToolImages(images) => {
                self.flush_text(ai);
                ai.update_chat_message(
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    |message| {
                        let mut attachments = oxideterm_ai::message_images(message).to_vec();
                        for image in images {
                            if !attachments.iter().any(|existing| existing.id == image.id) {
                                attachments.push(image);
                            }
                        }
                        oxideterm_ai::set_message_images(message, attachments);
                    },
                );
                ai.persist_chat_state();
            }
            PortableStreamEvent::HistoryBarrier(reply) => {
                self.flush_text(ai);
                ai.persist_chat_state();
                ai.after_history_commit(reply);
            }
            PortableStreamEvent::Checkpoint(checkpoint) => {
                self.flush_text(ai);
                ai.update_chat_message(
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    |message| {
                        oxideterm_ai::agent::set_message_checkpoint(message, &checkpoint);
                    },
                );
                ai.persist_chat_state();
            }
            PortableStreamEvent::PromptUsage {
                last_user_message_id,
                provider_id,
                model,
                breakdown,
                max_tokens,
            } => {
                if !ai.is_child_stream(delivery.generation) {
                    self.flush_text(ai);
                    ai.set_prepared_prompt_usage(AiPreparedPromptUsage {
                        conversation_id: delivery.conversation_id,
                        last_user_message_id,
                        provider_id,
                        model,
                        breakdown: ai_context_token_breakdown_from_prompt(breakdown, max_tokens),
                    });
                }
            }
            PortableStreamEvent::TurnWireReplay {
                scope,
                user_override,
                messages,
            } => {
                if !ai.is_child_stream(delivery.generation) {
                    ai.record_live_turn_replay(
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        scope,
                        oxideterm_ai::stream_state::AiLiveTurnReplay {
                            user_override,
                            messages,
                        },
                    );
                }
            }
            PortableStreamEvent::Guardrail {
                code,
                message,
                raw_text,
            } => {
                self.flush_text(ai);
                ai.apply_ai_guardrail(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &code,
                    &message,
                    raw_text,
                    diagnostic,
                    cx,
                );
            }
            PortableStreamEvent::AssistantRound {
                round_id,
                round_number,
                response_length,
                tool_call_ids,
                synthetic,
                retry_attempt,
                hard_deny_triggered,
            } => {
                self.flush_text(ai);
                ai.persist_ai_assistant_round(
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    round_id,
                    round_number,
                    response_length,
                    tool_call_ids,
                    synthetic,
                    retry_attempt,
                    hard_deny_triggered,
                    diagnostic,
                );
            }
            PortableStreamEvent::RoundSummary {
                round_id,
                text,
                metadata,
            } => {
                self.flush_text(ai);
                ai.apply_ai_round_summary(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &round_id,
                    &text,
                    metadata,
                    cx,
                );
            }
            PortableStreamEvent::RoundStatefulMarker { round_id, marker } => {
                self.flush_text(ai);
                ai.apply_ai_round_stateful_marker(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &round_id,
                    marker,
                    cx,
                );
            }
            PortableStreamEvent::Diagnostic {
                event_type,
                round_id,
                data,
            } => {
                self.flush_text(ai);
                ai.persist_ai_stream_diagnostic(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &event_type,
                    round_id,
                    data,
                    diagnostic,
                );
            }
            PortableStreamEvent::ToolStatus {
                tool_call_id,
                name,
                arguments,
                status,
                result,
                risk,
                summary,
                synthetic_denied,
                raw_text,
                round_id,
                round_number,
            } => {
                let needs_attention = status == "pending_user_approval"
                    && ai.is_chat_stream_generation(delivery.generation);
                self.flush_text(ai);
                ai.apply_ai_tool_status(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &tool_call_id,
                    &name,
                    &arguments,
                    &status,
                    result,
                    risk,
                    summary,
                    synthetic_denied,
                    raw_text,
                    round_id,
                    round_number,
                    diagnostic,
                    cx,
                );
                if needs_attention {
                    return Some(AiChatWindowDelivery::Attention {
                        generation: delivery.generation,
                        conversation_id: delivery.conversation_id,
                        assistant_id: delivery.assistant_id,
                        label: "ai.agents.approval",
                    });
                }
            }

            PortableStreamEvent::ToolApprovalRequested {
                tool_call_id,
                name,
                arguments,
                risk,
                summary,
                reply,
            } => {
                self.flush_text(ai);
                ai.apply_ai_tool_status(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &tool_call_id,
                    &name,
                    &arguments,
                    "pending_user_approval",
                    None,
                    Some(risk),
                    Some(summary),
                    false,
                    None,
                    None,
                    None,
                    diagnostic,
                    cx,
                );
                ai.register_tool_approval_decision(delivery.generation, tool_call_id, reply);
                if ai.is_chat_stream_generation(delivery.generation) {
                    return Some(AiChatWindowDelivery::Attention {
                        generation: delivery.generation,
                        conversation_id: delivery.conversation_id,
                        assistant_id: delivery.assistant_id,
                        label: "ai.agents.approval",
                    });
                }
            }
            PortableStreamEvent::CursorRequested { request, reply } => {
                self.flush_text(ai);
                if !request.valid() || !ai.is_chat_stream_generation(delivery.generation) {
                    reply.respond(oxideterm_ai::CursorRequest::cancelled_response());
                    return None;
                }
                let id = request.tool_call_id().to_owned();
                ai.apply_ai_tool_status(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &id,
                    request.method(),
                    "{}",
                    "waiting_user",
                    None,
                    Some("read".to_owned()),
                    Some(i18n.t("ai.questions.waiting")),
                    false,
                    None,
                    None,
                    None,
                    diagnostic,
                    cx,
                );
                let selections = match &request {
                    oxideterm_ai::CursorRequest::Questions(request) => {
                        vec![Vec::new(); request.questions.len()]
                    }
                    oxideterm_ai::CursorRequest::Plan(_) => Vec::new(),
                };
                C::install(
                    ai,
                    (delivery.generation, id),
                    PendingCursor {
                        conversation_id: delivery.conversation_id.clone(),
                        assistant_id: delivery.assistant_id.clone(),
                        request,
                        selections,
                        response_tx: Some(reply),
                    },
                );
                return Some(AiChatWindowDelivery::Attention {
                    generation: delivery.generation,
                    conversation_id: delivery.conversation_id,
                    assistant_id: delivery.assistant_id,
                    label: "ai.questions.waiting",
                });
            }
            PortableStreamEvent::AcpSessionUpdated(update) => {
                self.flush_text(ai);
                ai.apply_acp_session_state_update(&delivery.conversation_id, update);
                cx.notify();
            }
            PortableStreamEvent::AcpPermissionRequested { request, reply } => {
                self.flush_text(ai);
                let options = request
                    .options
                    .iter()
                    .map(|o| serde_json::json!({"id":o.option_id,"name":o.name,"kind":o.kind}))
                    .collect::<Vec<_>>();
                ai.apply_ai_tool_status(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &request.tool_call_id,
                    &request.name,
                    &request.arguments,
                    "pending_user_approval",
                    Some(serde_json::json!({"acpPermissionOptions":options})),
                    Some(request.risk),
                    Some(request.summary),
                    false,
                    None,
                    None,
                    None,
                    diagnostic,
                    cx,
                );
                if ai.register_acp_permission_decision(
                    delivery.generation,
                    request.tool_call_id,
                    reply,
                ) {
                    return Some(AiChatWindowDelivery::Attention {
                        generation: delivery.generation,
                        conversation_id: delivery.conversation_id,
                        assistant_id: delivery.assistant_id,
                        label: "ai.agents.approval",
                    });
                }
            }
            PortableStreamEvent::AcpSessionStarted {
                session_id,
                session_metadata,
                session_config_options,
                session_modes,
                agent_id,
            } => {
                self.flush_text(ai);
                if ai.apply_acp_session_started_state(
                    delivery.generation,
                    &delivery.conversation_id,
                    &session_id,
                    session_metadata,
                    session_config_options,
                    session_modes,
                    &agent_id,
                ) {
                    cx.notify();
                }
            }
            PortableStreamEvent::UserQuestionRequested { call, reply } => {
                if !reply.is_live() {
                    return None;
                }
                self.flush_text(ai);
                let question = serde_json::from_str(&call.arguments).unwrap_or_default();
                ai.apply_ai_tool_status(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &call.id,
                    &call.name,
                    &call.arguments,
                    "waiting_user",
                    Some(question),
                    Some("read".to_owned()),
                    Some(i18n.t("ai.questions.waiting")),
                    false,
                    None,
                    None,
                    None,
                    diagnostic,
                    cx,
                );
                reply.install(
                    ai,
                    (delivery.generation, call.id),
                    delivery.conversation_id.clone(),
                );
                return Some(AiChatWindowDelivery::Attention {
                    generation: delivery.generation,
                    conversation_id: delivery.conversation_id,
                    assistant_id: delivery.assistant_id,
                    label: "ai.questions.waiting",
                });
            }
            PortableStreamEvent::ToolCandidateSelectionRequested {
                tool_call_id,
                name,
                arguments,
                candidates,
                reply,
            } => {
                self.flush_text(ai);
                let candidate_count = candidates.len();
                let label = i18n.t("ai.tool_use.choose_target");
                let result = serde_json::json!({
                    "ok": false,
                    "summary": label,
                    "disambiguation": { "candidates": candidates },
                    "recoverable": true,
                });
                ai.apply_ai_tool_status(
                    delivery.generation,
                    &delivery.conversation_id,
                    &delivery.assistant_id,
                    &tool_call_id,
                    &name,
                    &arguments,
                    "pending_user_selection",
                    Some(result),
                    Some("read".to_owned()),
                    Some(label),
                    false,
                    None,
                    None,
                    None,
                    diagnostic,
                    cx,
                );
                ai.register_tool_candidate_selection_decision(
                    delivery.generation,
                    tool_call_id,
                    candidate_count,
                    reply,
                );
                cx.notify();
                if ai.chat_stream_generation() == delivery.generation {
                    return Some(AiChatWindowDelivery::FocusCandidate {
                        generation: delivery.generation,
                    });
                }
            }
            PortableStreamEvent::Stream(event) => {
                self.flush_text(ai);
                return Some(AiChatWindowDelivery::Host(AiStreamDelivery {
                    generation: delivery.generation,
                    conversation_id: delivery.conversation_id,
                    assistant_id: delivery.assistant_id,
                    event: AiStreamDeliveryEvent::Stream(event),
                }));
            }
        }
        None
    }

    fn push_text(
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

    fn flush_text(&mut self, ai: &mut AiWorkspaceEntity) {
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
        request_scope: breakdown.request_scope,
        last_actual_input: breakdown.last_actual_input,
        system_instructions: breakdown.system_instructions,
        tool_definitions: breakdown.tool_definitions,
        reserved_output: breakdown.reserved_output,
        messages: breakdown.messages,
        tool_results: breakdown.tool_results,
        // Output reserve is capacity held back, not part of the next input.
        total: breakdown.prompt_tokens(),
        max_tokens,
    }
}
