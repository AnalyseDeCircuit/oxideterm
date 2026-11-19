use gpui::{Div, PathPromptOptions, Rgba};
use oxideterm_settings::{Language, PersistedSettings, RemoteShellIntegrationMode};
use oxideterm_settings_model::{
    KnowledgeDeleteTarget, SettingsDynamicSectionCounts, SettingsInputDraftApply,
    SettingsSectionId, ThemeTarget, ai_mcp_configs, ai_provider_views,
    apply_persisted_settings_input_draft, cloud_sync_form_input_value_ref,
    plugin_setting_draft_to_value,
};

pub(in crate::workspace) use pages::open_path_external;

pub(in crate::workspace) use oxideterm_gpui_settings_view::terminal_page::terminal_session_log_root_directory;

use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_ai::{
    AiProviderView, apply_provider_model_refresh as ai_apply_provider_model_refresh,
    provider_id as ai_provider_id, provider_key_display_state as ai_provider_key_display_state,
    provider_view as ai_provider_view,
    remove_provider_at_with_scoped_settings as ai_remove_provider_at_with_scoped_settings,
};
use oxideterm_connections::LOCAL_SHELL_PRIVILEGE_CONNECTION_ID;
use oxideterm_gpui_settings_view::*;
use oxideterm_gpui_ui::{
    button::ButtonVariant,
    select::SelectAnchorId,
    text_input::{TextInputContentAlign, TextInputView},
};
use oxideterm_session_adapter::upstream_proxy_config_from_global_settings;

pub(in crate::workspace) use oxideterm_gpui_settings_view::appearance_page::{
    APPEARANCE_BORDER_RADIUS_MAX, APPEARANCE_BORDER_RADIUS_MIN,
};

mod ai_page;
mod appearance;
mod background;
mod cards;
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
