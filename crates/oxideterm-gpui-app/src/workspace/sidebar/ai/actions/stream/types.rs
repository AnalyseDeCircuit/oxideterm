#[derive(Clone)]
pub(super) struct AiCompactionPlan {
    pub(super) compact_messages: Vec<AiChatMessage>,
    pub(super) keep_messages: Vec<AiChatMessage>,
}

pub(super) struct AiStreamDelivery {
    pub(super) generation: u64,
    pub(super) conversation_id: String,
    pub(super) assistant_id: String,
    pub(super) event: AiStreamDeliveryEvent,
}

pub(super) struct AiCompactionDelivery {
    pub(super) kind: AiCompactionDeliveryKind,
    pub(super) conversation_id: String,
    pub(super) base_ids: Vec<String>,
    pub(super) plan: Option<AiCompactionPlan>,
    pub(super) summary: String,
    pub(super) stream_error: Option<String>,
    pub(super) resume_after: Option<AiPendingChatStream>,
}

pub(super) enum AiCompactionDeliveryKind {
    Compact,
    Summary,
}
