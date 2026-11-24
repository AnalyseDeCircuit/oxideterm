use super::{CursorResponse, Decision, PendingCursor, PortableStreamDelivery, PortableStreamEvent};
use crate::state::AiWorkspaceEntity;
pub use oxideterm_ai::tool_loop::{
    AiStreamDelivery, AiStreamDeliveryEvent, AiStreamDeliverySender,
};

pub(crate) struct NativeQuestionReply {
    dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
    sender: tokio::sync::oneshot::Sender<zeroize::Zeroizing<String>>,
}

pub(super) enum NativeDelivery {
    Reduce(PortableStreamDelivery<NativeQuestionReply, oxideterm_ai::CursorResponseSender>),
    Host(AiStreamDelivery),
    Expanded,
}

pub(super) fn route(
    delivery: AiStreamDelivery,
    tail: &mut std::collections::VecDeque<AiStreamDelivery>,
) -> NativeDelivery {
    use AiStreamDeliveryEvent as Live;
    use PortableStreamEvent as Ui;
    let event = match delivery.event {
        Live::ToolImages(value) => Ui::ToolImages(value),
        Live::Checkpoint(value) => Ui::Checkpoint(value),
        Live::Stream(value) => Ui::Stream(value),
        Live::PromptUsage {
            last_user_message_id,
            provider_id,
            model,
            breakdown,
            max_tokens,
        } => Ui::PromptUsage {
            last_user_message_id,
            provider_id,
            model,
            breakdown,
            max_tokens,
        },
        Live::AcpSessionStarted {
            session_id,
            session_metadata,
            session_config_options,
            session_modes,
            agent_id,
        } => Ui::AcpSessionStarted {
            session_id,
            session_metadata,
            session_config_options,
            session_modes,
            agent_id,
        },
        Live::Guardrail {
            code,
            message,
            raw_text,
        } => Ui::Guardrail {
            code,
            message,
            raw_text,
        },
        Live::AssistantRound {
            round_id,
            round_number,
            response_length,
            tool_call_ids,
            synthetic,
            retry_attempt,
            hard_deny_triggered,
        } => Ui::AssistantRound {
            round_id,
            round_number,
            response_length,
            tool_call_ids,
            synthetic,
            retry_attempt,
            hard_deny_triggered,
        },
        Live::RoundSummary {
            round_id,
            text,
            metadata,
        } => Ui::RoundSummary {
            round_id,
            text,
            metadata,
        },
        Live::RoundStatefulMarker { round_id, marker } => {
            Ui::RoundStatefulMarker { round_id, marker }
        }
        Live::Diagnostic {
            event_type,
            round_id,
            data,
        } => Ui::Diagnostic {
            event_type,
            round_id,
            data,
        },
        Live::ToolStatus {
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
        } => Ui::ToolStatus {
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
        },
        Live::TurnWireReplay {
            scope,
            user_override,
            messages,
        } => Ui::TurnWireReplay {
            scope,
            user_override,
            messages,
        },
        Live::ToolApprovalRequested {
            tool_call_id,
            name,
            arguments,
            risk,
            summary,
            sender,
        } => Ui::ToolApprovalRequested {
            tool_call_id,
            name,
            arguments,
            risk,
            summary,
            reply: decision(sender),
        },
        Live::ToolCandidateSelectionRequested {
            tool_call_id,
            name,
            arguments,
            candidates,
            sender,
        } => Ui::ToolCandidateSelectionRequested {
            tool_call_id,
            name,
            arguments,
            candidates,
            reply: decision(sender),
        },

        Live::HistoryBarrier(sender) => Ui::HistoryBarrier(Box::new(sender)),
        Live::UserQuestionRequested {
            call,
            sender,
            dispatch,
        } => Ui::UserQuestionRequested {
            call,
            reply: NativeQuestionReply { dispatch, sender },
        },
        Live::AcpClientEvent {
            event:
                event @ (oxideterm_ai::AcpClientEvent::SessionUpdate(_)
                | oxideterm_ai::AcpClientEvent::CreateElicitation { .. }
                | oxideterm_ai::AcpClientEvent::CompleteElicitation(_)),
            ..
        } => {
            // Insert at the current position so ACP and provider text coalesce in arrival order.
            for event in oxideterm_ai::acp_client_event_to_ai_stream_events(event)
                .into_iter()
                .rev()
            {
                tail.push_front(AiStreamDelivery {
                    generation: delivery.generation,
                    conversation_id: delivery.conversation_id.clone(),
                    assistant_id: delivery.assistant_id.clone(),
                    event: Live::Stream(event),
                });
            }
            return NativeDelivery::Expanded;
        }
        Live::AcpClientEvent {
            event:
                oxideterm_ai::AcpClientEvent::CursorRequest {
                    request,
                    response_tx,
                },
            ..
        } => Ui::CursorRequested {
            request,
            reply: response_tx,
        },
        // The desktop window revalidates the live session and leases at dispatch.
        event @ (Live::ToolResourcesRequested { .. }
        | Live::AgentCommandRequested { .. }
        | Live::AcpClientEvent { .. }
        | Live::ToolPreflightRequested { .. }
        | Live::RuntimeContextRequested { .. }
        | Live::ToolExecutionRequested { .. }) => {
            return NativeDelivery::Host(AiStreamDelivery { event, ..delivery });
        }
    };
    NativeDelivery::Reduce(PortableStreamDelivery {
        generation: delivery.generation,
        conversation_id: delivery.conversation_id,
        assistant_id: delivery.assistant_id,
        event,
    })
}

fn decision<T: Send + 'static>(sender: tokio::sync::oneshot::Sender<T>) -> Decision<T> {
    Box::new(move |value| {
        let _ = sender.send(value);
    })
}

pub struct AiPendingUserQuestion {
    pub conversation_id: String,
    pub dispatch: Option<oxideterm_ai::agent::AgentDispatch>,
    pub sender: tokio::sync::oneshot::Sender<zeroize::Zeroizing<String>>,
}

impl AiPendingUserQuestion {
    pub(crate) fn new(conversation_id: String, reply: NativeQuestionReply) -> Self {
        Self {
            conversation_id,
            dispatch: reply.dispatch,
            sender: reply.sender,
        }
    }

    pub(crate) fn answer_if_current(self, answer: zeroize::Zeroizing<String>) {
        if self.active() {
            let _ = self.sender.send(answer);
        }
    }

    pub(crate) fn active(&self) -> bool {
        !self.sender.is_closed()
            && self
                .dispatch
                .as_ref()
                .is_none_or(|guard| guard.check().is_ok())
    }
}

pub type AiPendingCursorRequest = PendingCursor<oxideterm_ai::CursorResponseSender>;

impl CursorResponse for oxideterm_ai::CursorResponseSender {
    fn active(&self) -> bool {
        !self.is_closed()
    }
    fn respond(self, value: serde_json::Value) {
        let _ = self.send(Ok(value));
    }
}

impl AiWorkspaceEntity {
    pub fn history_barrier(&mut self, sender: tokio::sync::oneshot::Sender<bool>) {
        self.after_history_commit(Box::new(sender));
    }

    pub fn register_tool_approval(
        &mut self,
        generation: u64,
        tool_call_id: String,
        sender: tokio::sync::oneshot::Sender<bool>,
    ) -> bool {
        self.register_tool_approval_decision(generation, tool_call_id, decision(sender))
    }

    pub fn register_tool_candidate_selection(
        &mut self,
        generation: u64,
        tool_call_id: String,
        candidate_count: usize,
        sender: tokio::sync::oneshot::Sender<Option<usize>>,
    ) -> bool {
        self.register_tool_candidate_selection_decision(
            generation,
            tool_call_id,
            candidate_count,
            decision(sender),
        )
    }
}
