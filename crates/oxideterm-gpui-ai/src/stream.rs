use oxideterm_ai::stream_state::AiCompactionPlan;

use crate::state::AiPendingChatStream;

pub use oxideterm_ai::tool_loop::{
    AiStreamDelivery, AiStreamDeliveryEvent, AiStreamDeliverySender,
};

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
pub(crate) use batch::AiChatDeliveryBatch;
pub use batch::{AiChatWindowDelivery, ai_context_token_breakdown_from_prompt};

pub type AiChatStreamFrameSource = dyn Fn(&gpui::App) -> Option<AiChatStreamFrame>;

#[derive(Default)]
pub struct AiChatStreamFrame {
    pub i18n: oxideterm_i18n::I18n,
    pub tool_use_enabled: bool,
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

impl AiChatStreamFrame {
    pub fn diagnostic(&self, data: serde_json::Value) -> serde_json::Value {
        ai_stream_diagnostic(
            data,
            self.tool_use_enabled,
            self.provider_id.as_deref(),
            self.model.as_deref(),
        )
    }
}

pub fn ai_stream_diagnostic(
    data: serde_json::Value,
    tool_use_enabled: bool,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> serde_json::Value {
    let mut object = match data {
        serde_json::Value::Object(object) => object,
        other => {
            let mut object = serde_json::Map::new();
            object.insert("value".to_owned(), other);
            object
        }
    };
    object.insert("source".to_owned(), serde_json::json!("sidebar"));
    object.insert(
        "toolUseEnabled".to_owned(),
        serde_json::json!(tool_use_enabled),
    );
    if let Some(provider_id) = provider_id {
        object.insert("providerId".to_owned(), serde_json::json!(provider_id));
    }
    if let Some(model) = model {
        object.insert("model".to_owned(), serde_json::json!(model));
    }
    serde_json::Value::Object(object)
}
