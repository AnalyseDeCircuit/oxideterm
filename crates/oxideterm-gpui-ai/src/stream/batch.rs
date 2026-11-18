use std::collections::VecDeque;

use oxideterm_ai::AiStreamEvent;

use super::{AiStreamDelivery, AiStreamDeliveryEvent};
use crate::state::{
    AiContextTokenBreakdown, AiPendingCursorRequest, AiPendingUserQuestion, AiPreparedPromptUsage,
    AiWorkspaceEntity,
};

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
}

impl AiChatDeliveryBatch {
    #[cfg(test)]
    pub(crate) fn new(deliveries: VecDeque<AiStreamDelivery>) -> Self {
        Self {
            pending_text: None,
            deliveries,
        }
    }

    pub(crate) fn extend(&mut self, deliveries: impl IntoIterator<Item = AiStreamDelivery>) {
        self.deliveries.extend(deliveries);
    }

    pub(crate) fn has_deliveries(&self) -> bool {
        !self.deliveries.is_empty()
    }

    /// Apply conversation-owned deliveries up to the next operation that needs
    /// the window or the live runtime. Do not advance further until that
    /// operation has had a chance to cancel or replace its generation.
    pub(crate) fn next_host_delivery(
        &mut self,
        ai: &mut AiWorkspaceEntity,
    ) -> Option<AiStreamDelivery> {
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

    pub(crate) fn next_window_delivery(
        &mut self,
        ai: &mut AiWorkspaceEntity,
        i18n: &oxideterm_i18n::I18n,
        diagnostic: impl Fn(serde_json::Value) -> serde_json::Value,
        cx: &mut gpui::Context<AiWorkspaceEntity>,
    ) -> Option<AiChatWindowDelivery> {
        while let Some(delivery) = self.next_host_delivery(ai) {
            match delivery.event {
                AiStreamDeliveryEvent::Guardrail {
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
                        &diagnostic,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::AssistantRound {
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
                        &diagnostic,
                    );
                }
                AiStreamDeliveryEvent::RoundSummary {
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
                AiStreamDeliveryEvent::RoundStatefulMarker { round_id, marker } => {
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
                AiStreamDeliveryEvent::Diagnostic {
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
                        &diagnostic,
                    );
                }
                AiStreamDeliveryEvent::ToolStatus {
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
                } if status != "pending_user_approval" => {
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
                        &diagnostic,
                        cx,
                    );
                }

                AiStreamDeliveryEvent::AcpClientEvent {
                    event:
                        event @ (oxideterm_ai::AcpClientEvent::SessionUpdate(_)
                        | oxideterm_ai::AcpClientEvent::CreateElicitation { .. }
                        | oxideterm_ai::AcpClientEvent::CompleteElicitation(_)),
                    ..
                } => {
                    // Expand in place so ACP text and provider text share the same batch ordering.
                    for event in oxideterm_ai::acp_client_event_to_ai_stream_events(event)
                        .into_iter()
                        .rev()
                    {
                        self.deliveries.push_front(AiStreamDelivery {
                            generation: delivery.generation,
                            conversation_id: delivery.conversation_id.clone(),
                            assistant_id: delivery.assistant_id.clone(),
                            event: AiStreamDeliveryEvent::Stream(event),
                        });
                    }
                }
                AiStreamDeliveryEvent::AcpClientEvent {
                    event:
                        oxideterm_ai::AcpClientEvent::CursorRequest {
                            request,
                            response_tx,
                        },
                    ..
                } => {
                    self.flush_text(ai);
                    if !request.valid() || !ai.is_chat_stream_generation(delivery.generation) {
                        let _ =
                            response_tx.send(Ok(oxideterm_ai::CursorRequest::cancelled_response()));
                        continue;
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
                        &diagnostic,
                        cx,
                    );
                    let selections = match &request {
                        oxideterm_ai::CursorRequest::Questions(request) => {
                            vec![Vec::new(); request.questions.len()]
                        }
                        oxideterm_ai::CursorRequest::Plan(_) => Vec::new(),
                    };
                    ai.pending_cursor_requests.insert(
                        (delivery.generation, id),
                        AiPendingCursorRequest {
                            conversation_id: delivery.conversation_id.clone(),
                            assistant_id: delivery.assistant_id.clone(),
                            request,
                            selections,
                            response_tx: Some(response_tx),
                        },
                    );
                    return Some(AiChatWindowDelivery::Attention {
                        generation: delivery.generation,
                        conversation_id: delivery.conversation_id,
                        assistant_id: delivery.assistant_id,
                        label: "ai.questions.waiting",
                    });
                }
                AiStreamDeliveryEvent::AcpSessionStarted {
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
                AiStreamDeliveryEvent::UserQuestionRequested {
                    call,
                    dispatch,
                    sender,
                } => {
                    if sender.is_closed()
                        || dispatch
                            .as_ref()
                            .is_some_and(|guard| guard.check().is_err())
                    {
                        continue;
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
                        &diagnostic,
                        cx,
                    );
                    ai.pending_user_questions.insert(
                        (delivery.generation, call.id),
                        AiPendingUserQuestion {
                            conversation_id: delivery.conversation_id.clone(),
                            dispatch,
                            sender,
                        },
                    );
                    return Some(AiChatWindowDelivery::Attention {
                        generation: delivery.generation,
                        conversation_id: delivery.conversation_id,
                        assistant_id: delivery.assistant_id,
                        label: "ai.questions.waiting",
                    });
                }
                AiStreamDeliveryEvent::ToolCandidateSelectionRequested {
                    tool_call_id,
                    name,
                    arguments,
                    candidates,
                    sender,
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
                        &diagnostic,
                        cx,
                    );
                    ai.register_tool_candidate_selection(
                        delivery.generation,
                        tool_call_id,
                        candidate_count,
                        sender,
                    );
                    cx.notify();
                    if ai.chat_stream_generation() == delivery.generation {
                        return Some(AiChatWindowDelivery::FocusCandidate {
                            generation: delivery.generation,
                        });
                    }
                }
                event => {
                    self.flush_text(ai);
                    return Some(AiChatWindowDelivery::Host(AiStreamDelivery {
                        event,
                        ..delivery
                    }));
                }
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
