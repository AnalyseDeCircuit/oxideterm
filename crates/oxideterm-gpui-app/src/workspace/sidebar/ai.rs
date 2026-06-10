use std::{collections::HashMap, sync::Arc};

use gpui::{Context, Div, MouseDownEvent, Rgba, Window};
use oxideterm_ai::{
    AiAutocompleteCandidate, AiAutocompleteKind, AiChatMessage, AiChatMessageMetadata, AiChatRole,
    AiChatStreamConfig, AiConversation, AiExecutionBackend, AiMessageBranches,
    AiOrchestratorObligation, AiOrchestratorObligationMode, AiPolicySafetyMode, AiProviderView,
    AiReferenceMatch, AiStreamEvent, AiToolCall, AiToolUsePolicy, ModelSelectorProviderProbe,
    ResolvedAiExecutionProfile, active_model_or_provider_default, active_provider_view,
    ai_autocomplete_candidates, ai_classify_orchestrator_obligation,
    ai_detected_intent_system_prompt, ai_help_markdown as ai_help_markdown_core,
    ai_input_system_prompt, ai_orchestrator_obligation_prompt, ai_reference_context_block,
    ai_required_tool_retry_prompt, ai_should_trigger_hard_deny,
    ai_user_explicitly_requested_json, ai_visible_suggestion_content,
    apply_ai_autocomplete_candidate, apply_chat_request_overrides,
    check_model_selector_provider_online, detect_ai_intent, extract_ai_error_context,
    generate_chat_title, infer_ai_cwd, model_max_response_tokens as ai_model_max_response_tokens,
    model_selector_display_name, model_selector_truncated_label,
    model_selector_visible_provider_groups, parse_ai_suggestions, parse_ai_user_input,
    provider_chat_requires_key as ai_provider_chat_requires_key,
    provider_views as ai_provider_views, resolve_ai_execution_profile, resolve_ai_policy_decision,
    resolve_ai_slash_command, resolve_model_selector_provider_probe,
    select_provider_model as ai_select_provider_model, stream_chat_completion,
    tool_policy_from_parts,
};
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_gpui_markdown::{
    MarkdownBlockLayout, MarkdownOptions, parser as markdown_parser, render as markdown_render,
};
use oxideterm_gpui_settings_view::SettingsTab;
use oxideterm_settings::AiThinkingStyle;
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView, TextInputView,
    ai::{
        AiContextUsage, AiModelSelectorPlacement, AiModelSelectorProviderState, AiSafetyMode,
        AiTone, AiToolCallView, AiToolRisk, AiToolStatus, ai_autocomplete_item,
        ai_autocomplete_popup, ai_chat_input_chips, ai_chat_panel, ai_chat_input_editor,
        ai_chat_input_footer, ai_chat_input_frame, ai_chat_input_root, ai_context_chip,
        ai_context_popover, ai_context_popover_header, ai_context_usage_indicator,
        ai_message_action, ai_message_author, ai_message_body,
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
    button::{ButtonRadius, ButtonVariant, IconButtonOptions, ToolbarButtonOptions},
    context_menu::{
        ContextMenuActionableStyle, context_menu_action, context_menu_actionable_row,
    },
    modal::overlay_content_boundary,
    tauri_ui_font_family as settings_ui_font_family,
    text_input::{
        text_caret, text_caret_overlay_at_text_end, text_input, text_input_anchor_probe,
        text_input_value_segments_with_color,
    },
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
                this.ai_chat_menu_open = false;
                this.ai_conversation_list_open = false;
                this.ai_safety_menu_open = false;
                listener(this, event, window, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }
}

include!("ai/render.rs");
include!("ai/input.rs");
include!("ai/model_selector.rs");
include!("ai/actions.rs");
include!("ai/helpers.rs");
include!("ai/terminal_inline.rs");
