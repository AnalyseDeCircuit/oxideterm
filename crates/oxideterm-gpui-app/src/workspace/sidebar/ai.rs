use std::sync::Arc;

use crate::workspace::ai_state::AiPendingChatStream;
use crate::workspace::ai_state::{AiChatPopover, AiWorkspaceVisibility};
use crate::workspace::ime::WorkspaceImeTarget;
use crate::workspace::*;
use gpui::{Context, Window};
use oxideterm_ai::AiExecutedToolResult;
use oxideterm_ai::stream_state::*;
use oxideterm_ai::{
    AiAutocompleteCandidate, AiChatMessage, AiChatRole, AiChatStreamConfig, AiConversation,
    AiExecutionBackend, AiPolicySafetyMode, AiProviderView, AiReasoningLevel, AiReferenceMatch,
    AiStreamEvent, AiToolCall, AiToolUsePolicy, ModelSelectorProviderProbe, ToolSessionId,
    active_model_selection, active_provider_view, ai_detected_intent_system_prompt,
    ai_help_markdown as ai_help_markdown_core, ai_input_system_prompt, ai_reference_context_block,
    detect_ai_intent, extract_ai_error_context, generate_chat_title, infer_ai_cwd,
    model_selector_visible_provider_groups, parse_ai_user_input,
    provider_chat_requires_key as ai_provider_chat_requires_key,
    provider_views as ai_provider_views, resolve_ai_policy_decision, resolve_ai_slash_command,
    resolve_model_selector_provider_probe, select_provider_model as ai_select_provider_model,
    tool_policy_from_parts,
};
use oxideterm_gpui_settings_view::SettingsTab;
use oxideterm_gpui_ui::{ai::AiSafetyMode, modal::overlay_content_boundary};
use oxideterm_settings::{AcpAgentConfig, AcpAgentRuntimeState, AiActiveBackend};
use oxideterm_settings_model::set_ai_model_reasoning_override;

pub(in crate::workspace) use oxideterm_gpui_ai::state::{
    AiAcpSessionState, ai_acp_session_state, ai_message_backend_for_stream,
};

include!("ai/render.rs");
include!("ai/input.rs");
include!("ai/images.rs");
include!("ai/model_selector.rs");
include!("ai/actions.rs");
include!("ai/helpers.rs");
include!("ai/terminal_inline.rs");
