use gpui::{
    AnchoredPositionMode, Corner, Div, PathPromptOptions, Rgba, anchored, deferred, point, relative,
};
use oxideterm_settings::{
    HighlightRule, HighlightRuleSet, IdeAgentMode, Language,
    MAX_HIGHLIGHT_RULE_SETS, MAX_HIGHLIGHT_RULES, PersistedSettings,
    RECOMMENDED_FOCUS_HANDOFF_COMMANDS, RemoteShellIntegrationMode, SettingsApplicationProxyMode,
    SettingsUpstreamProxyAuth,
    create_default_highlight_rule, reindex_highlight_rules, sanitize_highlight_rule_sets,
};
use oxideterm_settings_model::{
    AcpAgentPreset, AiProviderModelChipItem, AiProviderModelPanel,
    AiToolPolicyGroup, AiToolPolicyGroupState, CUSTOM_SEMANTIC_SCHEME_PREFIX,
    KnowledgeDeleteTarget, SETTINGS_SECTION_HEADER_ITEM_COUNT,
    SettingsDynamicSectionCounts, SettingsInputDraftApply, SettingsSectionId,
    ThemeEditorState, ThemeTarget, add_custom_semantic_rule,
    ai_add_acp_agent, ai_add_acp_agent_preset, ai_delete_acp_agent, ai_mcp_configs,
    ai_mcp_server_signature, ai_mcp_transport_label, ai_model_context_window_panels,
    ai_model_context_window_row as ai_model_context_window_row_model, ai_provider_card_signature,
    ai_provider_model_chip_rows, ai_provider_model_row_signature, ai_provider_views,
    ai_tool_auto_approve_total_count, ai_tool_auto_approved_count, ai_tool_policy_groups,
    ai_update_provider, apply_cloud_sync_form_input_owned, apply_persisted_settings_input_draft,
    cloud_sync_form_input_value_ref, create_custom_semantic_scheme, current_time_millis,
    custom_theme_display_name, delete_custom_semantic_rule, delete_custom_semantic_scheme,
    delete_custom_theme_from_settings, edit_custom_semantic_scheme, editor_terminal_theme,
    editor_ui_colors, export_custom_semantic_scheme, import_custom_semantic_scheme_named,
    persisted_settings_input_value,
    plugin_setting_draft_to_value, plugin_setting_input_value, save_theme_editor_snapshot_to_settings,
    set_ai_tool_policy_group_approval, set_ai_user_context_window, settings_multiline_line_ranges,
    settings_multiline_line_selection,
    settings_section_list_item_count as settings_model_section_list_item_count,
    settings_section_list_item_index, settings_tab_section_at, take_cloud_sync_form_input_value,
    theme_editor_from_settings,
};
use oxideterm_ssh::{HostKeyStatus, UpstreamProxyConfig, probe_upstream_proxy_route};

pub(in crate::workspace) use pages::open_path_external;

pub(in crate::workspace) fn terminal_session_log_root_directory(
    settings_path: &Path,
    configured_directory: Option<&str>,
) -> PathBuf {
    if let Some(directory) = configured_directory
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let directory = PathBuf::from(directory);
        if directory.is_absolute() {
            return directory;
        }
    }

    // A path synchronized from another operating system may not be absolute here.
    settings_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("logs")
        .join("terminal")
}

use super::*;
use super::{ai_state::AiSettingsViewSection, ime::WorkspaceImeTarget};
use oxideterm_ai::{
    AI_PROVIDER_TEMPLATES, AiProviderKeyDisplayState, AiProviderView,
    add_provider_from_template as ai_add_provider_from_template,
    add_provider_model as ai_add_provider_model,
    apply_provider_model_refresh as ai_apply_provider_model_refresh, generated_provider_id,
    provider_id as ai_provider_id, provider_key_display_state as ai_provider_key_display_state,
    provider_string as ai_provider_string,
    provider_template_by_type as ai_provider_template_by_type, provider_view as ai_provider_view,
    remove_provider_at_with_scoped_settings as ai_remove_provider_at_with_scoped_settings,
    set_active_provider_selection as ai_set_active_provider_selection,
};
use oxideterm_connections::{
    ConnectionImportDuplicateStrategy, ConnectionImportSource, LOCAL_SHELL_PRIVILEGE_CONNECTION_ID,
    ManagedSshKeyInfo, SSH_COMMON_PRIVILEGE_SCOPE_ID, SecretString, apply_connection_import,
    preview_connection_import,
};
use oxideterm_gpui_settings_view::*;
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        SplitFooterButtonEdge, SplitFooterButtonOptions,
        ToolbarButtonOptions, split_footer_button,
    },
    checkbox::{CheckboxOptions, CheckboxState, checkbox, checkbox_with_state},
    entity_row::{EntityListRowOptions, entity_list_row},
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary, popover_backdrop,
    },
    select::{
        OverlayAnchor, SelectAnchorId, select_option,
        select_option_action, select_overlay_popup, select_panel_overlay_popup_with_max_height,
    },
    slider::slider_pointer_percent,
    switch::switch,
    text_input::{
        TextInputContentAlign, TextInputView, text_caret, text_input_anchor_probe,
        text_input_value_segments,
    },
};
use oxideterm_network_proxy::install_application_proxy_policy_from_settings;
use oxideterm_session_adapter::upstream_proxy_config_from_global_settings;

pub(in crate::workspace) use oxideterm_gpui_settings_view::dialog::{
    settings_dialog_inert_overlay, settings_dialog_transition,
};

pub(in crate::workspace) const APPEARANCE_BORDER_RADIUS_MIN: f32 = 0.0; // Tauri AppearanceTab Slider min={0}.
pub(in crate::workspace) const APPEARANCE_BORDER_RADIUS_MAX: f32 = 16.0; // Tauri AppearanceTab Slider max={16} and settings normalization.
pub(in crate::workspace) const APPEARANCE_UI_FONT_SIZE_MIN: f32 = 11.0;
pub(in crate::workspace) const APPEARANCE_UI_FONT_SIZE_MAX: f32 = 20.0;

mod ai_page;
mod appearance;
mod background;
mod cards;
pub(in crate::workspace) use cards::SettingsCalloutTone;
mod cli_companion;
mod connections_page;
mod controls;
mod known_hosts;
pub(in crate::workspace) use oxideterm_gpui_settings_view::entity::{
    BackgroundGalleryOperationResult, CliCompanionOperation, DataDirectoryConfirm,
    DataDirectoryOperationResult,
    KeybindingFileOperationResult, KeybindingRecordingKeyAction,
    KeybindingResetConfirmKeyAction, PortableStatusRefresh,
    SettingsWorkspaceEntity, SettingsWorkspaceEvent,
    SettingsWorkspaceToast, ThemeEditorOperationResult, ThemeImportResult,
};
mod general_terminal_pages;
mod highlight;
mod local_terminal;
pub(in crate::workspace) use local_terminal::expand_local_terminal_cwd;
mod navigation_editor;
mod network_page;
mod pages;
mod portable_runtime;
mod privilege_credentials_page;
mod remote_shell_integration;
mod search;
mod surface;
mod surface_mount;
mod terminal_controls;
mod terminal_preview_font;
mod terminal_triggers;
mod totp_credentials_page;
pub(in crate::workspace) use terminal_triggers::TerminalTriggersSettingsState;
mod update;
mod update_ui;

use ai_page::AI_PROVIDER_SELECT_W;
pub(in crate::workspace) use ai_page::AiTextEditorDialog;
pub(in crate::workspace) use cli_companion::{
    CLI_COMPANION_COMMAND_NAME, LEGACY_CLI_COMPANION_COMMAND_NAME,
};
use connections_page::{
    connection_import_duplicate_strategy_label,
    connection_import_source_label, connection_import_source_options,
};
use network_page::{NetworkProxyAuthMode, network_proxy_auth_label};
use pages::settings_keybinding_scope_matches;
pub(in crate::workspace) use oxideterm_gpui_settings_view::remote_shell::{
    RemoteShellIntegrationAction, RemoteShellIntegrationCardSnapshot,
    RemoteShellIntegrationConfirmSnapshot, RemoteShellIntegrationConfirmSource,
    RemoteShellIntegrationFailure, RemoteShellIntegrationGateOutcome, RemoteShellIntegrationNotice,
    RemoteShellIntegrationRuntimeState,
};
pub(in crate::workspace) use oxideterm_gpui_settings_view::native_update::NativeUpdateRenderState;


#[cfg(test)]
mod ownership_tests;
