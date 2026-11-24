use oxideterm_ai::{AiStreamEvent, AiToolCall};

use zeroize::Zeroizing;

pub struct PortableStreamDelivery<Q = QuestionReply, C = CursorReply> {
    pub generation: u64,
    pub conversation_id: String,
    pub assistant_id: String,
    pub event: PortableStreamEvent<Q, C>,
}

/// Consuming the decision releases this request's waiter; dropping it cancels delivery.
pub type Decision<T> = Box<dyn FnOnce(T) + Send>;

/// The producer owns the waiter and can revoke it while the prompt is visible.
pub trait StreamReply<T>: Send {
    fn active(&self) -> bool;
    fn resolve(self: Box<Self>, value: T);
}

pub type CursorReply = Box<dyn StreamReply<serde_json::Value>>;
pub type CommitReply = Box<dyn StreamReply<bool>>;

pub type QuestionReply = Box<dyn StreamReply<Zeroizing<String>>>;

pub struct PendingQuestion {
    pub conversation_id: String,
    pub reply: QuestionReply,
}

impl PendingQuestion {
    pub fn new(conversation_id: String, reply: QuestionReply) -> Self {
        Self {
            conversation_id,
            reply,
        }
    }

    pub fn active(&self) -> bool {
        self.reply.active()
    }

    pub fn answer_if_current(self, answer: Zeroizing<String>) {
        if self.active() {
            self.reply.resolve(answer);
        }
    }
}

pub trait CursorResponse {
    fn active(&self) -> bool;
    fn respond(self, value: serde_json::Value);
}

impl CursorResponse for CursorReply {
    fn active(&self) -> bool {
        self.as_ref().active()
    }
    fn respond(self, value: serde_json::Value) {
        self.resolve(value);
    }
}

pub struct PendingCursor<C: CursorResponse> {
    pub conversation_id: String,
    pub assistant_id: String,
    pub request: oxideterm_ai::CursorRequest,
    pub selections: Vec<Vec<String>>,
    pub response_tx: Option<C>,
}

impl<C: CursorResponse> PendingCursor<C> {
    pub(crate) fn active(&self) -> bool {
        self.response_tx
            .as_ref()
            .is_some_and(CursorResponse::active)
    }

    pub(crate) fn respond(&mut self, value: serde_json::Value) {
        if let Some(reply) = self.response_tx.take() {
            reply.respond(value);
        }
    }
}

pub(crate) trait CursorInteraction {
    fn view(
        &self,
    ) -> (
        &str,
        &str,
        &oxideterm_ai::CursorRequest,
        &[Vec<String>],
        bool,
    );
    fn choices(&mut self) -> (&oxideterm_ai::CursorRequest, &mut [Vec<String>]);
    fn resolve_choice(&mut self, accepted: bool) -> bool;
}

impl<C: CursorResponse> CursorInteraction for PendingCursor<C> {
    fn view(
        &self,
    ) -> (
        &str,
        &str,
        &oxideterm_ai::CursorRequest,
        &[Vec<String>],
        bool,
    ) {
        (
            &self.conversation_id,
            &self.assistant_id,
            &self.request,
            &self.selections,
            self.active(),
        )
    }
    fn choices(&mut self) -> (&oxideterm_ai::CursorRequest, &mut [Vec<String>]) {
        (&self.request, &mut self.selections)
    }
    fn resolve_choice(&mut self, accepted: bool) -> bool {
        let Some(value) = self.request.response(accepted, &self.selections) else {
            return false;
        };
        self.respond(value);
        true
    }
}

pub(crate) trait QueueQuestion: Sized {
    fn is_live(&self) -> bool;
    fn install(
        self,
        ai: &mut crate::state::AiWorkspaceEntity,
        key: (u64, String),
        conversation: String,
    );
}
pub(crate) trait QueueCursor: CursorResponse + Sized {
    fn install(
        ai: &mut crate::state::AiWorkspaceEntity,
        key: (u64, String),
        cursor: PendingCursor<Self>,
    );
}
impl QueueQuestion for QuestionReply {
    fn is_live(&self) -> bool {
        self.as_ref().active()
    }
    fn install(
        self,
        ai: &mut crate::state::AiWorkspaceEntity,
        key: (u64, String),
        conversation: String,
    ) {
        let pending = PendingQuestion::new(conversation, self);
        #[cfg(target_family = "wasm")]
        ai.pending_user_questions.insert(key, pending);
        #[cfg(not(target_family = "wasm"))]
        ai.host_questions.insert(key, pending);
    }
}
impl QueueCursor for CursorReply {
    fn install(
        ai: &mut crate::state::AiWorkspaceEntity,
        key: (u64, String),
        cursor: PendingCursor<Self>,
    ) {
        #[cfg(target_family = "wasm")]
        ai.pending_cursor_requests.insert(key, cursor);
        #[cfg(not(target_family = "wasm"))]
        ai.host_cursors.insert(key, cursor);
    }
}

impl<C: CursorResponse> Drop for PendingCursor<C> {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.selections);
        if let Some(reply) = self.response_tx.take() {
            reply.respond(oxideterm_ai::CursorRequest::cancelled_response());
        }
    }
}

/// Desktop specializes only the pending reply types to retain its public session API.
pub enum PortableStreamEvent<Q = QuestionReply, C = CursorReply> {
    ToolImages(Vec<oxideterm_ai::AiImageAttachment>),
    Checkpoint(oxideterm_ai::agent::AgentCheckpoint),
    Stream(AiStreamEvent),
    PromptUsage {
        last_user_message_id: Option<String>,
        provider_id: String,
        model: String,
        breakdown: oxideterm_ai::AiPromptTokenBreakdown,
        max_tokens: usize,
    },
    AcpSessionStarted {
        session_id: String,
        session_metadata: Option<serde_json::Value>,
        session_config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        session_modes: Option<oxideterm_ai::AcpSessionModeState>,
        agent_id: String,
    },
    Guardrail {
        code: String,
        message: String,
        raw_text: Option<String>,
    },
    AssistantRound {
        round_id: String,
        round_number: i64,
        response_length: usize,
        tool_call_ids: Vec<String>,
        synthetic: bool,
        retry_attempt: Option<usize>,
        hard_deny_triggered: bool,
    },
    RoundSummary {
        round_id: String,
        text: String,
        metadata: serde_json::Value,
    },
    RoundStatefulMarker {
        round_id: String,
        marker: Option<String>,
    },
    Diagnostic {
        event_type: String,
        round_id: Option<String>,
        data: serde_json::Value,
    },
    ToolStatus {
        tool_call_id: String,
        name: String,
        arguments: String,
        status: String,
        result: Option<serde_json::Value>,
        risk: Option<String>,
        summary: Option<String>,
        synthetic_denied: bool,
        raw_text: Option<String>,
        round_id: Option<String>,
        round_number: Option<i64>,
    },
    TurnWireReplay {
        scope: u64,
        user_override: Option<(String, String)>,
        messages: Vec<oxideterm_ai::AiChatMessage>,
    },
    ToolApprovalRequested {
        tool_call_id: String,
        name: String,
        arguments: String,
        risk: String,
        summary: String,
        reply: Decision<bool>,
    },
    ToolCandidateSelectionRequested {
        tool_call_id: String,
        name: String,
        arguments: String,
        candidates: Vec<serde_json::Value>,
        reply: Decision<Option<usize>>,
    },
    HistoryBarrier(CommitReply),
    AcpSessionUpdated(oxideterm_ai::AcpSessionStateUpdate),
    AcpPermissionRequested {
        request: oxideterm_ai::AcpPermissionRequestProjection,
        reply: Decision<Option<String>>,
    },
    UserQuestionRequested {
        call: AiToolCall,
        reply: Q,
    },
    CursorRequested {
        request: oxideterm_ai::CursorRequest,
        reply: C,
    },
}
