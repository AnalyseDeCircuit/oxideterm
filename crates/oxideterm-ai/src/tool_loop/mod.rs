//! Provider tool rounds. Requests to live application owners travel through a scoped delivery channel.

use std::collections::BTreeMap;

use crate::agent::{AgentExecution, AgentMessageKind, AgentResult, AgentState, AgentText};
use crate::stream_state::*;
use crate::terminal_observation::annotate_ai_run_command_execution_result;
use crate::tool_resources::*;
use crate::tool_result_protocol::*;
use crate::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiStreamEvent, AiToolCall, ToolSessionId,
    ai_should_trigger_hard_deny, ai_user_explicitly_requested_json,
    provider_chat_requires_key as ai_provider_chat_requires_key,
    provider_views as ai_provider_views, resolve_ai_policy_decision,
    sanitize_json_text_for_persistence as sanitize_ai_tool_arguments_for_approval,
    sanitize_tool_arguments_text_for_persistence as sanitize_ai_tool_arguments_for_persistence,
};

fn ai_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

mod acp;
pub use acp::{AcpApplicationToolTurn, acp_session_cwd_from_agent, handle_acp_application_tool_call};
mod delivery;
pub use delivery::{AiStreamDelivery, AiStreamDeliveryEvent, AiStreamDeliverySender};

include!("services.rs");
include!("events.rs");
include!("requests.rs");
include!("agents.rs");
include!("checkpoint.rs");
include!("provider.rs");
include!("round.rs");

#[cfg(test)]
include!("tests.rs");

#[cfg(test)]
include!("stream_contract_tests.rs");
