pub(in crate::workspace) struct AiStreamDelivery {
    pub(in crate::workspace) generation: u64,
    pub(in crate::workspace) conversation_id: String,
    pub(in crate::workspace) assistant_id: String,
    pub(in crate::workspace) event: AiStreamDeliveryEvent,
}

pub(in crate::workspace) type AiStreamDeliverySender =
    crate::workspace::delivery::ActiveDeliverySender<AiStreamDelivery>;

pub(in crate::workspace) struct AiCompactionDelivery {
    pub(in crate::workspace) kind: AiCompactionDeliveryKind,
    pub(in crate::workspace) conversation_id: String,
    pub(in crate::workspace) base_ids: Vec<String>,
    pub(in crate::workspace) plan: Option<AiCompactionPlan>,
    pub(in crate::workspace) summary: String,
    pub(in crate::workspace) stream_error: Option<String>,
    pub(in crate::workspace) resume_after: Option<AiPendingChatStream>,
    pub(in crate::workspace) silent: bool,
}

pub(in crate::workspace) type AiCompactionDeliverySender =
    crate::workspace::delivery::ActiveDeliverySender<AiCompactionDelivery>;

pub(in crate::workspace) enum AiCompactionDeliveryKind {
    Compact,
    Summary,
}
