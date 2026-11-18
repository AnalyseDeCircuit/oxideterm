use crate::workspace::ai_state::AiPendingChatStream;
use crate::workspace::ai_state::{AiChatPopover, AiWorkspaceVisibility};
use crate::workspace::ime::WorkspaceImeTarget;
use crate::workspace::*;
use gpui::{Context, Window};
use oxideterm_ai::AiExecutedToolResult;
use oxideterm_ai::stream_state::*;
use oxideterm_ai::{
    AiAutocompleteCandidate, AiChatStreamConfig,
    AiExecutionBackend, AiProviderView, AiReasoningLevel, AiReferenceMatch,
    AiStreamEvent, AiToolCall, ToolSessionId,
    infer_ai_cwd,
    select_provider_model as ai_select_provider_model,
};
use oxideterm_gpui_settings_view::SettingsTab;
use oxideterm_gpui_ui::ai::AiSafetyMode;
use oxideterm_settings::AiActiveBackend;
use oxideterm_settings_model::set_ai_model_reasoning_override;

include!("ai/render.rs");
include!("ai/input.rs");
include!("ai/images.rs");
include!("ai/model_selector.rs");
include!("ai/actions.rs");
include!("ai/helpers.rs");
include!("ai/terminal_inline.rs");
