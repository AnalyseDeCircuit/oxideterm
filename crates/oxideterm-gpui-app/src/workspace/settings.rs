use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{
    AnchoredPositionMode, Corner, Div, PathPromptOptions, Rgba, anchored, deferred, point, relative,
};
use oxideterm_settings::{
    FrostedGlassMode, HighlightRule, IdeAgentMode, Language, MAX_HIGHLIGHT_RULES,
    PersistedSettings, create_default_highlight_rule, reindex_highlight_rules,
};
use oxideterm_settings_model::{
    AI_MODEL_REFRESH_MISSING_API_KEY, AiMcpServerDraft, AiModelRefreshDelivery,
    AiProviderModelChipItem, AiProviderModelPanel, AiSettingsSection, AiToolPolicyGroup,
    KNOWLEDGE_EMBEDDING_BATCH_SIZE, KnowledgeDeleteTarget, KnowledgeExternalEdit,
    SETTINGS_SECTION_HEADER_ITEM_COUNT, SettingsDynamicSectionCounts, SettingsInputDraftApply,
    TERMINAL_THEME_COLOR_FIELDS, ThemeColorField, ThemeEditorSection, ThemeEditorState,
    UI_THEME_COLOR_FIELDS, ai_add_execution_profile, ai_context_max_chars_label_key,
    ai_context_visible_lines_label_key, ai_default_execution_profile, ai_delete_execution_profile,
    ai_duplicate_execution_profile, ai_execution_profile_id, ai_execution_profile_signature,
    ai_execution_profiles_need_normalization, ai_mcp_auth_mode_value, ai_mcp_clean_record,
    ai_mcp_configs, ai_mcp_draft_input_value, ai_mcp_draft_valid, ai_mcp_server_signature,
    ai_mcp_split_args, ai_mcp_transport_label, ai_mcp_transport_value,
    ai_model_context_window_panels,
    ai_model_context_window_row as ai_model_context_window_row_model, ai_model_reasoning_panels,
    ai_model_reasoning_row as ai_model_reasoning_row_model, ai_normalize_execution_profiles,
    ai_patch_execution_profile, ai_provider_card_signature, ai_provider_model_chip_rows,
    ai_provider_model_row_signature, ai_provider_views, ai_reasoning_effort_from_profile_value,
    ai_reasoning_label_key, ai_reasoning_profile_value, ai_set_default_execution_profile,
    ai_tool_auto_approve_total_count, ai_tool_auto_approved_count, ai_tool_policy_groups,
    ai_update_provider, app_ui_colors_to_colors, apply_ai_mcp_draft_input,
    apply_cloud_sync_form_input_draft, apply_persisted_settings_input_draft,
    cloud_sync_form_input_value, current_time_millis, custom_theme_display_name,
    delete_custom_theme_from_settings, editor_terminal_theme, editor_ui_colors,
    import_custom_theme, import_knowledge_file, is_custom_theme_id, parse_color_hex,
    persisted_settings_input_value, plugin_setting_draft_to_value, plugin_setting_input_value,
    reconnect_attempt_label, reconnect_base_delay_options, reconnect_delay_label,
    reconnect_max_attempt_options, reconnect_max_delay_options, save_theme_editor_to_settings,
    set_ai_model_reasoning_override, set_ai_provider_reasoning_override,
    set_ai_user_context_window, settings_multiline_line_ranges, settings_multiline_line_selection,
    settings_section_list_identity as settings_model_section_list_identity,
    settings_section_list_item_count as settings_model_section_list_item_count,
    terminal_theme_to_colors, theme_editor_from_settings, toggle_string_set,
};
use oxideterm_theme::BUILT_IN_THEMES;

use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_ai::{
    AI_PROVIDER_TEMPLATES, AiProviderKeyDisplayState, AiProviderRefreshKeyPolicy, AiProviderView,
    add_provider_from_template as ai_add_provider_from_template,
    apply_provider_model_refresh as ai_apply_provider_model_refresh, fetch_provider_models,
    generated_provider_id, provider_id as ai_provider_id,
    provider_key_display_state as ai_provider_key_display_state,
    provider_refresh_key_policy as ai_provider_refresh_key_policy,
    provider_string as ai_provider_string,
    provider_template_by_type as ai_provider_template_by_type, provider_view as ai_provider_view,
    remove_provider_at_with_scoped_settings as ai_remove_provider_at_with_scoped_settings,
    set_active_provider_selection as ai_set_active_provider_selection,
    set_provider_default_model as ai_set_provider_default_model,
    take_provider_key_secret as ai_take_provider_key_secret,
};
use oxideterm_connections::{
    SshConfigHost, list_available_ssh_keys, list_ssh_config_hosts, resolve_ssh_config_alias,
    saved_connection_from_ssh_host,
};
use oxideterm_gpui_settings_view::*;
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView, button,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        SplitFooterButtonOptions, ToolbarButtonIconPosition, ToolbarButtonOptions,
        split_footer_button,
    },
    checkbox::checkbox,
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary, popover_backdrop,
    },
    select::{
        OverlayAnchor, SelectAnchorId, readonly_value_trigger, select_anchor_probe, select_label,
        select_option, select_option_action, select_overlay_popup,
        select_panel_overlay_popup_with_max_height, select_separator,
        select_trigger_with_focus_visible,
    },
    separator::{SeparatorOrientation, separator},
    slider::{SliderView, slider},
    text_input::{
        TextInputContentAlign, TextInputView, text_caret, text_input, text_input_anchor_probe,
        text_input_value_segments, text_input_with_content_align,
    },
};

pub(in crate::workspace) fn settings_store_modified_time(
    path: &std::path::Path,
) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

include!("settings/surface.rs");
include!("settings/cards.rs");
include!("settings/controls.rs");
include!("settings/terminal_display.rs");
include!("settings/highlight.rs");
include!("settings/terminal_controls.rs");
include!("settings/local_terminal.rs");
include!("settings/general_terminal_pages.rs");
include!("settings/appearance.rs");
include!("settings/connections_page.rs");
include!("settings/sftp_page.rs");
include!("settings/ide_page.rs");
include!("settings/ai_page.rs");
include!("settings/pages.rs");

fn settings_tab_lucide(icon: SettingsTabIcon) -> LucideIcon {
    match icon {
        SettingsTabIcon::BookOpen => LucideIcon::BookOpen,
        SettingsTabIcon::Code2 => LucideIcon::Code2,
        SettingsTabIcon::HardDrive => LucideIcon::HardDrive,
        SettingsTabIcon::HelpCircle => LucideIcon::HelpCircle,
        SettingsTabIcon::Key => LucideIcon::Key,
        SettingsTabIcon::Keyboard => LucideIcon::Keyboard,
        SettingsTabIcon::Monitor => LucideIcon::Monitor,
        SettingsTabIcon::Shield => LucideIcon::Shield,
        SettingsTabIcon::Sparkles => LucideIcon::Sparkles,
        SettingsTabIcon::Square => LucideIcon::Square,
        SettingsTabIcon::Terminal => LucideIcon::Terminal,
        SettingsTabIcon::WifiOff => LucideIcon::WifiOff,
    }
}

fn settings_background_tab_lucide(icon: SettingsBackgroundTabIcon) -> LucideIcon {
    match icon {
        SettingsBackgroundTabIcon::Activity => LucideIcon::Activity,
        SettingsBackgroundTabIcon::ArrowLeftRight => LucideIcon::ArrowLeftRight,
        SettingsBackgroundTabIcon::Code2 => LucideIcon::Code2,
        SettingsBackgroundTabIcon::Folder => LucideIcon::Folder,
        SettingsBackgroundTabIcon::FolderInput => LucideIcon::FolderInput,
        SettingsBackgroundTabIcon::ListTree => LucideIcon::ListTree,
        SettingsBackgroundTabIcon::Monitor => LucideIcon::Monitor,
        SettingsBackgroundTabIcon::Network => LucideIcon::Network,
        SettingsBackgroundTabIcon::Puzzle => LucideIcon::Puzzle,
        SettingsBackgroundTabIcon::Rocket => LucideIcon::Rocket,
        SettingsBackgroundTabIcon::Settings => LucideIcon::Settings,
        SettingsBackgroundTabIcon::Terminal => LucideIcon::Terminal,
    }
}
