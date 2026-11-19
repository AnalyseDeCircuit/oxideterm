use crate::workspace::ai_state::AiPendingChatStream;
use crate::workspace::ai_state::AiWorkspaceVisibility;
use crate::workspace::ime::WorkspaceImeTarget;
use crate::workspace::*;
use gpui::{Context, Window};
use oxideterm_ai::AiExecutedToolResult;
use oxideterm_ai::{
    AiAutocompleteCandidate, AiChatStreamConfig, AiProviderView, AiReferenceMatch, AiStreamEvent,
    AiToolCall, ToolSessionId, infer_ai_cwd,
};
use oxideterm_gpui_settings_view::SettingsTab;
use oxideterm_gpui_ui::ai::AiSafetyMode;

include!("ai/render.rs");
include!("ai/input.rs");
include!("ai/images.rs");
include!("ai/model_selector.rs");
include!("ai/actions.rs");
include!("ai/helpers.rs");
include!("ai/terminal_inline.rs");
