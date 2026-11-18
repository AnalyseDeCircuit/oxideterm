use oxideterm_ai::stream_state::AiCompactionPlan;

use crate::state::AiPendingChatStream;

pub use oxideterm_ai::tool_loop::{AiStreamDelivery, AiStreamDeliveryEvent, AiStreamDeliverySender};

pub struct AiCompactionDelivery {
    pub generation: u64,
    pub kind: AiCompactionDeliveryKind,
    pub conversation_id: String,
    pub base_ids: Vec<String>,
    pub plan: Option<AiCompactionPlan>,
    pub summary: String,
    pub failed: bool,
    pub resume_after: Option<AiPendingChatStream>,
    pub silent: bool,
}

pub type AiCompactionDeliverySender =
    oxideterm_workspace::delivery::ActiveDeliverySender<AiCompactionDelivery>;

pub enum AiCompactionDeliveryKind {
    Compact,
    Summary,
}

mod batch;
pub use batch::{AiChatDeliveryBatch, AiStreamTextKind, ai_context_token_breakdown_from_prompt};

/// Published after the window adapter has applied and acknowledged a chat batch.
pub struct AiChatBatchApplied;

impl gpui::EventEmitter<AiChatBatchApplied> for crate::state::AiWorkspaceEntity {}
