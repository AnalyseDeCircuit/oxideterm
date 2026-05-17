use std::collections::HashMap;

use gpui::Div;
use oxideterm_ai::{
    AiAutocompleteCandidate, AiAutocompleteKind, AiChatMessage, AiChatMessageMetadata,
    AiChatRole, AiChatStreamConfig, AiConversation, AiMessageBranches, AiProviderView,
    AiPolicySafetyMode, AiReferenceMatch, AiStreamEvent, AiToolUsePolicy,
    AiToolCall, ModelSelectorProviderProbe, ResolvedAiExecutionProfile, active_model_or_provider_default,
    active_provider_view, ai_autocomplete_candidates, ai_help_markdown as ai_help_markdown_core,
    ai_input_system_prompt, ai_reference_context_block, apply_ai_autocomplete_candidate,
    apply_chat_request_overrides, check_model_selector_provider_online, extract_ai_error_context,
    generate_chat_title, infer_ai_cwd, model_selector_display_name,
    model_max_response_tokens as ai_model_max_response_tokens, model_selector_truncated_label,
    model_selector_visible_provider_groups, parse_ai_user_input,
    provider_chat_requires_key as ai_provider_chat_requires_key,
    provider_views as ai_provider_views, resolve_ai_policy_decision, resolve_ai_slash_command,
    resolve_ai_execution_profile, resolve_model_selector_provider_probe,
    select_provider_model as ai_select_provider_model, stream_chat_completion, tool_policy_from_parts,
    ai_detected_intent_system_prompt, ai_visible_suggestion_content, detect_ai_intent,
    parse_ai_suggestions,
};
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_gpui_markdown::{MarkdownOptions, markdown_with_options};
use oxideterm_gpui_settings_view::SettingsTab;
use oxideterm_settings::AiThinkingStyle;
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView, confirm_dialog,
    TextInputView,
    ai::{
        AiContextUsage, AiModelSelectorPlacement, AiModelSelectorProviderState, AiSafetyMode,
        AiTone, AiToolCallView, AiToolRisk, AiToolStatus, ai_autocomplete_item,
        ai_autocomplete_popup, ai_chat_input_chips, ai_chat_panel,
        ai_chat_input_editor, ai_chat_input_footer, ai_chat_input_frame, ai_chat_input_root,
        ai_chat_scroll_area, ai_context_chip, ai_context_popover, ai_context_popover_header,
        ai_context_usage_indicator, ai_message_action, ai_message_author, ai_message_body,
        ai_message_model_badge, ai_message_time, ai_model_selector_dropdown,
        ai_model_selector_empty_search, ai_model_selector_footer, ai_model_selector_key_status,
        ai_model_selector_list, ai_model_selector_local_status, ai_model_selector_model_row,
        ai_model_selector_models_panel, ai_model_selector_no_provider_button,
        ai_model_selector_provider_header, ai_model_selector_provider_message,
        ai_model_selector_refresh_button, ai_model_selector_root, ai_model_selector_search_bar,
        ai_model_selector_trigger_compact, ai_profile_button, ai_raw_block, ai_guardrail_block,
        ai_safety_indicator, ai_send_button, ai_status_indicator, ai_stop_button,
        ai_thinking_block, ai_thinking_compact, ai_thinking_content, ai_thinking_header,
        ai_tool_approval_bar, ai_tool_approval_button, ai_tool_args_pre, ai_tool_block,
        ai_tool_details, ai_tool_heading, ai_tool_item, ai_tool_item_header, ai_tool_output_pre,
        ai_tool_section_label,
    },
    tauri_ui_font_family as settings_ui_font_family,
    text_input::{text_caret, text_input, text_input_anchor_probe},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AiHeaderAction {
    NewChat,
    Settings,
}

#[derive(Clone)]
pub(super) struct AiPendingChatStream {
    pub(super) conversation_id: String,
    pub(super) config: AiChatStreamConfig,
    pub(super) request_content: Option<String>,
    pub(super) task_system_prompt: Option<String>,
    pub(super) rag_system_prompt: Option<String>,
}

include!("ai/render.rs");
include!("ai/input.rs");
include!("ai/model_selector.rs");
include!("ai/actions.rs");
include!("ai/helpers.rs");
