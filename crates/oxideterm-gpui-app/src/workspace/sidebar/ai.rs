use std::{collections::HashMap, sync::Arc};

use crate::workspace::ai_state::AiPendingChatStream;
use crate::workspace::ai_state::{AiChatPopover, AiWorkspaceVisibility};
use crate::workspace::ime::WorkspaceImeTarget;
use crate::workspace::*;
use gpui::{Context, Div, MouseDownEvent, Rgba, Window};
use oxideterm_ai::stream_state::*;
use oxideterm_ai::{
    AiAutocompleteCandidate, AiAutocompleteKind, AiChatMessage, AiChatRole,
    AiChatStreamConfig, AiConversation, AiExecutionBackend, AiPolicySafetyMode,
    AiProviderView, AiReasoningLevel, AiReferenceMatch, AiStreamEvent, AiToolCall, AiToolUsePolicy,
    ModelSelectorProviderProbe, ToolSessionId, active_model_selection, active_provider_view,
    ai_autocomplete_candidates, ai_detected_intent_system_prompt,
    ai_help_markdown as ai_help_markdown_core, ai_input_system_prompt, ai_input_token_at_cursor,
    ai_reference_context_block, ai_should_trigger_hard_deny, ai_user_explicitly_requested_json,
    ai_visible_suggestion_content, apply_chat_request_overrides, detect_ai_intent,
    extract_ai_error_context, generate_chat_title, infer_ai_cwd, model_reasoning_capability,
    model_selector_visible_provider_groups, parse_ai_user_input,
    provider_chat_requires_key as ai_provider_chat_requires_key,
    provider_views as ai_provider_views, resolve_ai_policy_decision, resolve_ai_slash_command,
    resolve_model_selector_provider_probe, select_provider_model as ai_select_provider_model,
    stream_chat_completion, tool_policy_from_parts,
};
use oxideterm_ai::{
    AiExecutedToolResult, ai_to_usable_budget_threshold, ai_tool_result_model_content,
};
use oxideterm_gpui_markdown::{
    MarkdownBlockLayout, MarkdownOptions, parser as markdown_parser, render as markdown_render,
};
use oxideterm_gpui_settings_view::SettingsTab;
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView, TextInputView,
    ai::{
        AiContextUsage, AiSafetyMode,
        AiTone, AiToolCallView, AiToolRisk, AiToolStatus, ai_autocomplete_item,
        ai_autocomplete_popup, ai_chat_input_chips, ai_chat_input_editor, ai_chat_input_footer,
        ai_chat_input_frame, ai_chat_input_root_with_background, ai_chat_panel, ai_context_chip,
        ai_context_popover, ai_context_popover_header, ai_context_usage_indicator,
        ai_guardrail_block, ai_message_action, ai_message_author, ai_message_body,
        ai_message_model_label, ai_message_time, ai_raw_block, ai_safety_indicator, ai_send_button,
        ai_status_indicator, ai_stop_button, ai_thinking_block, ai_thinking_compact,
        ai_thinking_content, ai_thinking_header, ai_tool_approval_bar, ai_tool_approval_button,
        ai_tool_args_pre, ai_tool_block, ai_tool_details, ai_tool_heading, ai_tool_item,
        ai_tool_item_header, ai_tool_output_pre, ai_tool_section_label,
    },
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions,
    },
    context_menu::{ContextMenuActionableStyle, context_menu_action, context_menu_actionable_row},
    modal::overlay_content_boundary,
    text_input::text_input_anchor_probe,
};
use oxideterm_settings::{AcpAgentConfig, AcpAgentRuntimeState, AiActiveBackend, AiThinkingStyle};
use oxideterm_settings_model::set_ai_model_reasoning_override;

pub(in crate::workspace) use oxideterm_gpui_ai::state::{
    AiAcpSessionState, ai_acp_session_state, ai_message_backend_for_stream,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiHeaderAction {
    ArchivedConversations,
    NewChat,
    Settings,
}

impl WorkspaceApp {
    fn render_ai_menu_action(
        &self,
        item: Div,
        disabled: bool,
        loading: bool,
        hover_bg: Option<Rgba>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // AI safety/chat menus are Radix dropdown-style command rows in Tauri.
        // Keep this as a single WorkspaceApp listener. Passing a cx.listener
        // through another cx.listener re-enters the same entity and trips GPUI's
        // update guard on menu clicks.
        let item = context_menu_actionable_row(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: hover_bg,
                hover_text_color: None,
            },
        );
        context_menu_action(
            item,
            disabled,
            loading,
            cx.listener(move |this, event, window, cx| {
                this.ai.state.update(cx, |ai, _cx| {
                    ai.set_chat_popover_open(AiChatPopover::Menu, false);
                    ai.set_chat_popover_open(AiChatPopover::ConversationList, false);
                    ai.set_chat_popover_open(AiChatPopover::Safety, false);
                });
                listener(this, event, window, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }
}

include!("ai/render.rs");
include!("ai/input.rs");
include!("ai/images.rs");
include!("ai/model_selector.rs");
include!("ai/actions.rs");
include!("ai/helpers.rs");
include!("ai/terminal_inline.rs");
