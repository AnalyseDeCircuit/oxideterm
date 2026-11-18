use gpui::{Div, PathPromptOptions, Rgba, relative};
use oxideterm_settings::{Language, PersistedSettings, RemoteShellIntegrationMode};
use oxideterm_settings_model::{
    AcpAgentPreset, AiProviderModelChipItem, AiProviderModelPanel, AiToolPolicyGroup,
    AiToolPolicyGroupState, KnowledgeDeleteTarget, SettingsDynamicSectionCounts,
    SettingsInputDraftApply, SettingsSectionId, ThemeTarget, ai_add_acp_agent,
    ai_add_acp_agent_preset, ai_delete_acp_agent, ai_mcp_configs, ai_mcp_server_signature,
    ai_mcp_transport_label, ai_model_context_window_panels,
    ai_model_context_window_row as ai_model_context_window_row_model, ai_provider_card_signature,
    ai_provider_model_chip_rows, ai_provider_model_row_signature, ai_provider_views,
    ai_tool_auto_approve_total_count, ai_tool_auto_approved_count, ai_tool_policy_groups,
    ai_update_provider, apply_persisted_settings_input_draft, cloud_sync_form_input_value_ref,
    current_time_millis, plugin_setting_draft_to_value, set_ai_tool_policy_group_approval,
    set_ai_user_context_window,
};

pub(in crate::workspace) use pages::open_path_external;

pub(in crate::workspace) use oxideterm_gpui_settings_view::terminal_page::terminal_session_log_root_directory;

use super::*;
use super::{ai_state::AiSettingsViewSection, ime::WorkspaceImeTarget};
use oxideterm_ai::{
    AiProviderKeyDisplayState, AiProviderView,
    add_provider_from_template as ai_add_provider_from_template,
    add_provider_model as ai_add_provider_model,
    apply_provider_model_refresh as ai_apply_provider_model_refresh, generated_provider_id,
    provider_id as ai_provider_id, provider_key_display_state as ai_provider_key_display_state,
    provider_string as ai_provider_string,
    provider_template_by_type as ai_provider_template_by_type, provider_view as ai_provider_view,
    remove_provider_at_with_scoped_settings as ai_remove_provider_at_with_scoped_settings,
    set_active_provider_selection as ai_set_active_provider_selection,
};
use oxideterm_connections::LOCAL_SHELL_PRIVILEGE_CONNECTION_ID;
use oxideterm_gpui_settings_view::*;
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        SplitFooterButtonEdge, SplitFooterButtonOptions, ToolbarButtonOptions, split_footer_button,
    },
    checkbox::{CheckboxOptions, CheckboxState, checkbox, checkbox_with_state},
    entity_row::{EntityListRowOptions, entity_list_row},
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop, overlay_content_boundary,
    },
    select::SelectAnchorId,
    switch::switch,
    text_input::{TextInputContentAlign, TextInputView, text_caret, text_input_anchor_probe},
};
use oxideterm_session_adapter::upstream_proxy_config_from_global_settings;

pub(in crate::workspace) use oxideterm_gpui_settings_view::dialog::{
    settings_dialog_inert_overlay, settings_dialog_transition,
};

pub(in crate::workspace) use oxideterm_gpui_settings_view::appearance_page::{
    APPEARANCE_BORDER_RADIUS_MAX, APPEARANCE_BORDER_RADIUS_MIN,
};

mod ai_page;
mod appearance;
mod background;
mod cards;
pub(in crate::workspace) use cards::SettingsCalloutTone;
mod cli_companion;
mod connections_page;
mod controls;
#[cfg(test)]
mod known_hosts;
pub(in crate::workspace) use oxideterm_gpui_settings_view::entity::{
    CliCompanionOperation, KeybindingRecordingKeyAction, KeybindingResetConfirmKeyAction,
    PortableStatusRefresh, SettingsWorkspaceEntity, SettingsWorkspaceEvent, SettingsWorkspaceToast,
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
pub(in crate::workspace) use terminal_triggers::TerminalTriggersSettingsState;
mod update;
mod update_ui;

pub(in crate::workspace) use ai_page::AiTextEditorDialog;
pub(in crate::workspace) use cli_companion::{
    CLI_COMPANION_COMMAND_NAME, LEGACY_CLI_COMPANION_COMMAND_NAME,
};
pub(in crate::workspace) use oxideterm_gpui_settings_view::remote_shell::{
    RemoteShellIntegrationAction, RemoteShellIntegrationCardSnapshot,
    RemoteShellIntegrationConfirmSnapshot, RemoteShellIntegrationConfirmSource,
    RemoteShellIntegrationFailure, RemoteShellIntegrationGateOutcome, RemoteShellIntegrationNotice,
    RemoteShellIntegrationRuntimeState,
};

#[cfg(test)]
mod ownership_tests;
