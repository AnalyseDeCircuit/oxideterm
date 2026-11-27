use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

pub use crate::SettingsNavigationDraftAction;
use crate::{SettingsInput, SettingsKeybindingScopeFilter, SettingsSlider};
use crate::{SettingsRouteSnapshot, SettingsRouteState};
use gpui::{AppContext, Context, Entity, EventEmitter, KeyDownEvent, Task};
use oxideterm_connections::{
    ConnectionImportDuplicateStrategy, ConnectionImportPreview, ConnectionImportSource,
    ManagedSshKeyInfo, ManagedSshKeyUsage, PrivilegePromptType, PrivilegeResponseMode,
};
use oxideterm_gpui_ui::confirm::ConfirmDialogAction;
use oxideterm_settings_model::{
    AiSettingsPage, SettingsNavigationLayout, SettingsTab, TerminalSettingsPage,
    ThemeEditorSection, ThemeEditorState, ThemeTarget, app_ui_colors_to_colors,
    editor_terminal_theme, terminal_theme_to_colors,
};
use oxideterm_theme::{derive_ui_colors_from_terminal, theme_by_id};
use zeroize::{Zeroize, Zeroizing};

use oxideterm_gpui_ui::confirm as browser_behavior;

use super::native_update::NativeUpdateRuntime;
use oxideterm_settings_model::CliCompanionStatus;

mod cli;
mod completion;
#[cfg(test)]
mod ownership_tests;
pub mod picker;
pub use completion::SettingsCompletion;
mod connections;
mod known_hosts;
mod network;
mod portable;
mod privilege;
mod totp;

pub use connections::{ManagedKeyFileImportRequest, ManagedKeyPasteImportRequest};
pub use known_hosts::KnownHostsState;
pub use totp::TotpDraft;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortableSettingsDialog {
    ChangePassword,
}

#[derive(Clone, Debug)]
pub enum SettingsManagedKeyDialog {
    ImportFile,
    Paste,
    Rename {
        key_id: String,
    },
    Delete {
        key: ManagedSshKeyInfo,
        usage: ManagedSshKeyUsage,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortableSettingsAction {
    AutoUnlock,
    ChangePassword,
}

pub fn settings_store_modified_time(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

const EXTERNAL_STORE_WATCH_INTERVAL: Duration = Duration::from_millis(530);

/// Tracks persisted settings stores without retaining their contents.
struct ExternalStoreWatch {
    settings_path: PathBuf,
    connections_path: PathBuf,
    settings_modified: Option<SystemTime>,
    connections_modified: Option<SystemTime>,
}

impl ExternalStoreWatch {
    fn new(settings_path: PathBuf, connections_path: PathBuf) -> Self {
        let settings_modified = settings_store_modified_time(&settings_path);
        let connections_modified = settings_store_modified_time(&connections_path);
        Self {
            settings_path,
            connections_path,
            settings_modified,
            connections_modified,
        }
    }

    fn refresh_observed_state(&mut self) {
        self.settings_modified = settings_store_modified_time(&self.settings_path);
        self.connections_modified = settings_store_modified_time(&self.connections_path);
    }

    fn take_external_change(&mut self) -> Option<ExternalStoreChange> {
        let settings_modified = settings_store_modified_time(&self.settings_path);
        let connections_modified = settings_store_modified_time(&self.connections_path);
        self.take_change_for_modified_times(settings_modified, connections_modified)
    }

    fn take_change_for_modified_times(
        &mut self,
        settings_modified: Option<SystemTime>,
        connections_modified: Option<SystemTime>,
    ) -> Option<ExternalStoreChange> {
        let change = ExternalStoreChange {
            settings: settings_modified != self.settings_modified,
            connections: connections_modified != self.connections_modified,
        };
        // Advance before publishing so a failed reload does not create an event storm.
        self.settings_modified = settings_modified;
        self.connections_modified = connections_modified;
        (change.settings || change.connections).then_some(change)
    }
}

/// Non-secret result produced by the portable runtime status worker.
pub struct PortableStatusRefresh {
    pub status: Result<oxideterm_portable_runtime::PortableStatusSnapshot, String>,
    pub exportable_secret_count: usize,
}

/// Read-only projection used after releasing the settings Entity borrow.
#[derive(Clone)]
pub struct PortableStatusSnapshot {
    pub status: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    pub error: Option<String>,
    pub exportable_secret_count: Option<usize>,
    pub refresh_pending: bool,
}

pub struct PortablePasswordDialogSnapshot {
    pub open: bool,
    pub pending: bool,
    pub error: Option<String>,
    pub current_password_present: bool,
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
}

/// Copies only non-secret render state for the active managed-key dialog.
pub enum ManagedKeyDialogSnapshot {
    ImportFile {
        file_path: String,
        file_name: String,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
    },
    Paste {
        name: String,
        private_key_present: bool,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
    },
    Rename {
        name: String,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
    },
    Delete {
        key: oxideterm_connections::ManagedSshKeyInfo,
        usage: oxideterm_connections::ManagedSshKeyUsage,
        presence: oxideterm_gpui_ui::motion::ExitPresence,
    },
}

pub use crate::proxy_page::{NetworkProxyPasswordSnapshot, NetworkProxyTestSnapshot};

/// Editable privilege credential state with a zeroizing secret owner.
pub struct PrivilegeCredentialDraft {
    pub credential_id: Option<String>,
    pub label: String,
    pub prompt_type: PrivilegePromptType,
    pub response_mode: PrivilegeResponseMode,
    pub username_hint: String,
    pub prompt_patterns: String,
    pub host_patterns: String,
    pub second_factor_patterns: String,
    pub second_factor_user_hint: String,
    pub totp_credential_id: Option<String>,
    pub secret: Zeroizing<String>,
    pub enabled: bool,
    pub require_click_to_send: bool,
}

impl Default for PrivilegeCredentialDraft {
    fn default() -> Self {
        Self {
            credential_id: None,
            label: String::new(),
            prompt_type: PrivilegePromptType::Sudo,
            response_mode: PrivilegeResponseMode::PasswordOnly,
            username_hint: String::new(),
            prompt_patterns: String::new(),
            host_patterns: "*".to_string(),
            second_factor_patterns: String::new(),
            second_factor_user_hint: String::new(),
            totp_credential_id: None,
            secret: Zeroizing::new(String::new()),
            enabled: true,
            require_click_to_send: true,
        }
    }
}

pub struct PrivilegeCredentialSnapshot {
    pub credential_id: Option<String>,
    pub label: String,
    pub prompt_type: PrivilegePromptType,
    pub response_mode: PrivilegeResponseMode,
    pub username_hint: String,
    pub prompt_patterns: String,
    pub host_patterns: String,
    pub second_factor_patterns: String,
    pub second_factor_user_hint: String,
    pub totp_credential_id: Option<String>,
    pub enabled: bool,
    pub require_click_to_send: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct CliCompanionSnapshot {
    pub status: Option<CliCompanionStatus>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct SshConfigImportSnapshot {
    pub open: bool,
    pub selected_hosts: HashSet<String>,
    pub status: Option<String>,
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
}

/// Read-only projection for the connection importer settings surface.
pub struct ConnectionImportSnapshot {
    pub source: ConnectionImportSource,
    pub paths: Vec<String>,
    pub preview: Option<ConnectionImportPreview>,
    pub selected_draft_ids: HashSet<String>,
    pub duplicate_strategy: ConnectionImportDuplicateStrategy,
    pub status: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliCompanionOperation {
    Refresh,
    Install,
    Uninstall,
    UninstallLegacy,
    Migrate,
}

#[derive(Debug, Eq, PartialEq)]
pub enum DataDirectoryConfirm {
    Conflict {
        path: PathBuf,
        files_found: Vec<String>,
    },
    Reset,
}

pub enum DataDirectoryOperationResult {
    Changed,
    Reset,
    Failed(String),
}

pub enum BackgroundGalleryOperationResult {
    Updated(Option<bool>, Option<String>),
    Failed,
    MediaFailed(oxideterm_gpui_background::BackgroundFailure),
}

pub enum ThemeImportResult {
    Imported {
        target: ThemeTarget,
        system_dark: Option<bool>,
        theme_id: String,
        name: String,
        value: serde_json::Value,
    },
    Failed(String),
}

/// Moves a completed theme editor action into the root persistence adapter.
pub enum ThemeEditorOperationResult {
    Save(Arc<ThemeEditorState>),
    Delete(Arc<ThemeEditorState>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThemeEditorExitAction {
    Cancel,
    Save,
    Delete,
}

pub use crate::keybinding_page::{
    KeybindingRecordingCommit, KeybindingRecordingFooterAction, KeybindingRecordingKeyAction,
    KeybindingResetConfirmSnapshot,
};

pub enum KeybindingFileOperationResult {
    Exported,
    ExportFailed,
    Imported {
        overrides: serde_json::Map<String, serde_json::Value>,
    },
    ImportFailed,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LaunchAtLoginError {
    #[cfg(not(target_os = "macos"))]
    ApprovalRequired,
    OperationFailed(Arc<str>),
    #[cfg(not(target_os = "macos"))]
    TaskFailed(Arc<str>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchAtLoginSnapshot {
    pub enabled: bool,
    pub pending: bool,
    pub error: Option<LaunchAtLoginError>,
}

/// Converts managed gallery paths once at the filesystem boundary.
fn background_gallery_strings(settings_path: &Path) -> anyhow::Result<Vec<String>> {
    Ok(oxideterm_settings::list_background_images(settings_path)?
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

fn is_theme_editor_input(input: SettingsInput) -> bool {
    input.is_background()
        || matches!(
            input,
            SettingsInput::CustomThemeName
                | SettingsInput::CustomThemeTerminalColor(_)
                | SettingsInput::CustomThemeUiColor(_)
        )
}

/// Owns settings work that must complete independently from root rendering.
pub struct SettingsWorkspaceEntity {
    pub section_list: crate::surface::SettingsSectionList,
    pub theme_background_preview: Entity<oxideterm_gpui_background::BackgroundPreview>,
    pub background_preview: Entity<oxideterm_gpui_background::BackgroundPreview>,
    pub editor_background_preview: Entity<oxideterm_gpui_background::BackgroundPreview>,
    route: SettingsRouteState,
    pub theme_preview_page: crate::ThemePreviewPage,
    pub(crate) command_specs: Option<crate::command_specs::CommandSpecsDraft>,
    pub appearance_edit_dark: Option<bool>,
    pub(crate) slider_drag: Option<SettingsSlider>,
    external_store_watch: Option<ExternalStoreWatch>,
    external_store_watch_task: Option<Task<()>>,
    portable_status: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    portable_status_error: Option<String>,
    portable_exportable_secret_count: Option<usize>,
    portable_refresh_pending: bool,
    portable_refresh_task: Option<Task<()>>,
    pub portable_dialog: Option<PortableSettingsDialog>,
    pub portable_action_pending: Option<PortableSettingsAction>,
    pub portable_action_error: Option<String>,
    pub portable_current_password: Zeroizing<String>,
    pub portable_new_password: Zeroizing<String>,
    pub portable_confirm_password: Zeroizing<String>,
    pub settings_focused_input: Option<SettingsInput>,
    provider_input_draft: Zeroizing<String>,
    pub terminal_triggers: crate::terminal_triggers::TerminalTriggersSettingsState,
    pub highlight: crate::highlight_page::HighlightSettingsState,
    trigger_input_draft: Zeroizing<String>,
    pub terminal_cjk_font_custom: bool,
    pub ide_cjk_font_custom: bool,
    pub portable_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub portable_dialog_exit_task: Option<Task<()>>,
    pub portable_action_task: Option<Task<()>>,
    pub managed_key_dialog: Option<SettingsManagedKeyDialog>,
    pub managed_key_status: Option<String>,
    pub known_hosts: KnownHostsState,
    pub managed_key_file_path: String,
    pub managed_key_file_name: String,
    pub managed_key_file_passphrase: Zeroizing<String>,
    pub managed_key_paste_name: String,
    pub managed_key_paste_private_key: Zeroizing<String>,
    pub managed_key_paste_passphrase: Zeroizing<String>,
    pub managed_key_rename_name: String,
    pub managed_key_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub managed_key_dialog_exit_task: Option<Task<()>>,
    pub managed_key_file_picker_task: Option<Task<()>>,
    pub public_mcp_client_editor: Option<String>,
    pub new_mcp_client_mode: oxideterm_public_mcp::ClientApprovalMode,
    pub network_proxy_password: Zeroizing<String>,
    pub network_proxy_password_status: Option<String>,
    pub network_proxy_test_host: String,
    pub network_proxy_test_port: String,
    pub network_proxy_test_pending: bool,
    pub network_proxy_test_result: Option<Result<u128, String>>,
    pub network_proxy_test_task: Option<Task<()>>,
    pub network_proxy_test_abort: Option<tokio::task::AbortHandle>,
    pub privilege_draft: PrivilegeCredentialDraft,
    pub totp_draft: Option<TotpDraft>,
    pub totp_error: Option<oxideterm_connections::totp::TotpError>,
    pub privilege_error: Option<String>,
    pub privilege_editor_open: bool,
    pub privilege_scope_id: Option<String>,
    pub cli_companion_status: Option<CliCompanionStatus>,
    pub cli_companion_loading: bool,
    pub cli_companion_error: Option<String>,
    pub cli_companion_task: Option<Task<()>>,
    pub(crate) discovery: crate::connections_page::DiscoverySettingsState,
    pub ssh_config_import_dialog_open: bool,
    pub ssh_config_selected_hosts: HashSet<String>,
    pub connection_import_status: Option<String>,
    pub connection_import_source: ConnectionImportSource,
    pub connection_import_paths: Vec<String>,
    pub connection_import_preview: Option<ConnectionImportPreview>,
    pub selected_connection_import_drafts: HashSet<String>,
    pub connection_import_duplicate_strategy: ConnectionImportDuplicateStrategy,
    pub connection_import_target_group: String,
    pub connection_import_path_picker_task: Option<Task<()>>,
    pub ssh_config_import_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub ssh_config_import_dialog_exit_task: Option<Task<()>>,
    data_directory_confirm: Option<DataDirectoryConfirm>,
    data_directory_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    data_directory_picker_task: Option<Task<()>>,
    data_directory_confirm_exit_task: Option<Task<()>>,
    data_directory_results: VecDeque<DataDirectoryOperationResult>,
    background_blur_preview: Option<i64>,
    background_blur_system_dark: Option<bool>,
    background_blur_commit_generation: u64,
    background_blur_commit_task: Option<Task<()>>,
    background_images: Arc<[String]>,
    background_media_info: HashMap<PathBuf, Result<bool, oxideterm_background_media::MediaError>>,
    background_media_info_tasks: HashMap<PathBuf, Task<()>>,
    background_gallery_task: Option<Task<()>>,
    background_pack_images_pending: Vec<PathBuf>,
    background_gallery_system_dark: Option<bool>,
    background_gallery_results: VecDeque<BackgroundGalleryOperationResult>,
    theme_import_task: Option<Task<()>>,
    theme_import_results: VecDeque<ThemeImportResult>,
    theme_editor: Option<Arc<ThemeEditorState>>,
    theme_editor_presence: oxideterm_gpui_ui::motion::ExitPresence,
    theme_editor_exit_action: Option<ThemeEditorExitAction>,
    theme_editor_exit_task: Option<Task<()>>,
    theme_editor_results: VecDeque<ThemeEditorOperationResult>,
    settings_search_open: bool,
    settings_search_query: String,
    pub(crate) shortcuts: crate::keybinding_page::KeybindingState,

    pub skill_search_query: String,
    pub ai_tool_policy_expanded: bool,
    pub ai_context_windows: crate::ai::context_windows::AiContextWindowState,
    pub ai_mcp: crate::ai::mcp_draft::McpDraftState,
    pub ai_acp_expanded: bool,
    pub ai_subagent_picker_open: bool,
    pub skill_scope_filter: Option<oxideterm_skills::SkillScope>,
    pub expanded_skill_paths: HashSet<PathBuf>,
    pub skill_diagnostics_expanded: bool,
    pub background_effects_expanded: Option<bool>,
    pub background_interactions_expanded: Option<bool>,
    pub(crate) background_scope_motion: oxideterm_gpui_ui::motion::UserSegmentedControlMotionState,
    pub(crate) background_scope_task: Option<Task<()>>,

    keybinding_file_operation_generation: u64,
    keybinding_file_operation_task: Option<Task<()>>,
    keybinding_file_operation_results: VecDeque<KeybindingFileOperationResult>,
    keybinding_reset_confirm_open: bool,
    keybinding_reset_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    keybinding_reset_confirm_focused_action: Option<ConfirmDialogAction>,
    keybinding_reset_confirm_exit_task: Option<Task<()>>,
    launch_at_login_enabled: bool,
    launch_at_login_pending: bool,
    launch_at_login_error: Option<LaunchAtLoginError>,
    launch_at_login_generation: u64,
    launch_at_login_task: Option<Task<()>>,
    pub(crate) native_update: NativeUpdateRuntime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeybindingResetConfirmKeyAction {
    Cancel,
    Confirm,
    Handled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsWorkspaceToast {
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalStoreChange {
    pub settings: bool,
    pub connections: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsWorkspaceEvent {
    SubmitAiMcp,
    ExternalStoresChanged(ExternalStoreChange),
    ResetNativeUpdateOverlay,
    ShowNativeUpdateNotification,
    ShowNativeUpdateToast(SettingsWorkspaceToast),
    RequestAutomaticNativeUpdateCheck,
    RequestQuitAfterNativeUpdate,
    DataDirectoryConfirmOpened,
    DataDirectoryOperationReady,
    BackgroundBlurCommitReady(Option<bool>, i64),
    BackgroundGalleryOperationReady,
    ThemeImportReady,
    ThemeEditorOperationReady,
    KeybindingFileOperationReady,
    HighlightDraftChanged(SettingsInput),
    RefreshTerminalHighlightPreferences,
    PortablePasswordChangeFinished {
        success: bool,
    },
    CliCompanionFinished {
        operation: CliCompanionOperation,
        success: bool,
    },
}

impl EventEmitter<SettingsWorkspaceEvent> for SettingsWorkspaceEntity {}

impl SettingsWorkspaceEntity {
    pub fn edit_theme_editor_background(
        &mut self,
        edit: impl FnOnce(&mut oxideterm_settings::BackgroundStyle),
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.theme_editor.as_mut() {
            edit(&mut Arc::make_mut(editor).background);
            cx.notify();
        }
    }
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            section_list: crate::surface::SettingsSectionList::default(),
            terminal_triggers: Default::default(),
            highlight: Default::default(),
            trigger_input_draft: Zeroizing::new(String::new()),
            theme_background_preview: cx.new(oxideterm_gpui_background::BackgroundPreview::new),
            background_preview: cx.new(oxideterm_gpui_background::BackgroundPreview::new),
            editor_background_preview: cx.new(oxideterm_gpui_background::BackgroundPreview::new),
            route: SettingsRouteState::default(),
            theme_preview_page: Default::default(),
            command_specs: None,
            appearance_edit_dark: None,
            slider_drag: None,
            external_store_watch: None,
            external_store_watch_task: None,
            portable_status: None,
            portable_status_error: None,
            portable_exportable_secret_count: None,
            portable_refresh_pending: false,
            portable_refresh_task: None,
            portable_dialog: None,
            portable_action_pending: None,
            portable_action_error: None,
            portable_current_password: Zeroizing::new(String::new()),
            portable_new_password: Zeroizing::new(String::new()),
            portable_confirm_password: Zeroizing::new(String::new()),
            settings_focused_input: None,
            provider_input_draft: Zeroizing::new(String::new()),
            terminal_cjk_font_custom: false,
            ide_cjk_font_custom: false,
            portable_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            portable_dialog_exit_task: None,
            portable_action_task: None,
            managed_key_dialog: None,
            managed_key_status: None,
            known_hosts: Default::default(),
            managed_key_file_path: String::new(),
            managed_key_file_name: String::new(),
            managed_key_file_passphrase: Zeroizing::new(String::new()),
            managed_key_paste_name: String::new(),
            managed_key_paste_private_key: Zeroizing::new(String::new()),
            managed_key_paste_passphrase: Zeroizing::new(String::new()),
            managed_key_rename_name: String::new(),
            managed_key_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            managed_key_dialog_exit_task: None,
            managed_key_file_picker_task: None,
            public_mcp_client_editor: None,
            new_mcp_client_mode: oxideterm_public_mcp::ClientApprovalMode::Standard,
            network_proxy_password: Zeroizing::new(String::new()),
            network_proxy_password_status: None,
            network_proxy_test_host: String::new(),
            network_proxy_test_port: "22".to_string(),
            network_proxy_test_pending: false,
            network_proxy_test_result: None,
            network_proxy_test_task: None,
            network_proxy_test_abort: None,
            privilege_draft: PrivilegeCredentialDraft::default(),
            totp_draft: None,
            totp_error: None,
            privilege_error: None,
            privilege_editor_open: false,
            privilege_scope_id: None,
            cli_companion_status: None,
            cli_companion_loading: false,
            cli_companion_error: None,
            cli_companion_task: None,
            discovery: Default::default(),
            ssh_config_import_dialog_open: false,
            ssh_config_selected_hosts: HashSet::new(),
            connection_import_status: None,
            connection_import_source: ConnectionImportSource::SecureCrt,
            connection_import_paths: Vec::new(),
            connection_import_preview: None,
            selected_connection_import_drafts: HashSet::new(),
            connection_import_duplicate_strategy: ConnectionImportDuplicateStrategy::Skip,
            connection_import_target_group: String::new(),
            connection_import_path_picker_task: None,
            ssh_config_import_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            ssh_config_import_dialog_exit_task: None,
            data_directory_confirm: None,
            data_directory_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            data_directory_picker_task: None,
            data_directory_confirm_exit_task: None,
            data_directory_results: VecDeque::new(),
            background_blur_preview: None,
            background_blur_system_dark: None,
            background_blur_commit_generation: 0,
            background_blur_commit_task: None,
            background_images: Arc::from([]),
            background_media_info: HashMap::new(),
            background_media_info_tasks: HashMap::new(),
            background_gallery_task: None,
            background_pack_images_pending: Vec::new(),
            background_gallery_system_dark: None,
            background_gallery_results: VecDeque::new(),
            theme_import_task: None,
            theme_import_results: VecDeque::new(),
            theme_editor: None,
            theme_editor_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            theme_editor_exit_action: None,
            theme_editor_exit_task: None,
            theme_editor_results: VecDeque::new(),
            settings_search_open: false,
            settings_search_query: String::new(),
            shortcuts: Default::default(),

            skill_search_query: String::new(),
            ai_tool_policy_expanded: true,
            ai_context_windows: Default::default(),
            ai_mcp: Default::default(),
            ai_acp_expanded: false,
            ai_subagent_picker_open: false,
            skill_scope_filter: None,
            expanded_skill_paths: HashSet::new(),
            skill_diagnostics_expanded: false,
            background_effects_expanded: None,
            background_interactions_expanded: None,
            background_scope_motion: Default::default(),
            background_scope_task: None,

            keybinding_file_operation_generation: 0,
            keybinding_file_operation_task: None,
            keybinding_file_operation_results: VecDeque::new(),
            keybinding_reset_confirm_open: false,
            keybinding_reset_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            keybinding_reset_confirm_focused_action: None,
            keybinding_reset_confirm_exit_task: None,
            launch_at_login_enabled: false,
            launch_at_login_pending: false,
            launch_at_login_error: None,
            launch_at_login_generation: 0,
            launch_at_login_task: None,
            native_update: NativeUpdateRuntime::new(cx),
        }
    }

    pub fn route_snapshot(&self) -> SettingsRouteSnapshot {
        self.route.route_snapshot()
    }

    pub fn set_active_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) -> bool {
        if !self.route.set_active_tab(tab) {
            return false;
        }
        self.totp_draft = None;
        self.totp_error = None;
        if tab != SettingsTab::Discovery {
            self.discovery.editor = None;
        }
        if tab == SettingsTab::Connections {
            self.refresh_known_hosts(cx);
        }
        cx.notify();
        true
    }

    pub fn open_ai_section(&mut self, section: &str, cx: &mut Context<Self>) {
        if let Some(tab) = crate::settings_tab_from_ai_section(section) {
            self.set_active_tab(tab, cx);
        }
        if let Some(page) = crate::terminal_settings_page_from_ai_section(section) {
            self.set_terminal_page(page, cx);
        }
    }

    pub fn set_terminal_page(
        &mut self,
        page: TerminalSettingsPage,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.route.set_terminal_page(page);
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn set_ai_page(&mut self, page: AiSettingsPage, cx: &mut Context<Self>) -> bool {
        let changed = self.route.set_ai_page(page);
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn open_navigation_editor(
        &mut self,
        persisted_groups: &[Vec<String>],
        cx: &mut Context<Self>,
    ) {
        self.route.open_navigation_editor(persisted_groups);
        cx.notify();
    }

    pub fn close_navigation_editor(&mut self, cx: &mut Context<Self>) {
        if self.route.close_navigation_editor() {
            cx.notify();
        }
    }

    pub fn navigation_draft_snapshot(&self) -> Option<Arc<SettingsNavigationLayout>> {
        self.route.navigation_draft_snapshot()
    }

    pub fn navigation_editor_open(&self) -> bool {
        self.route.navigation_editor_open()
    }

    pub fn apply_navigation_draft_action(
        &mut self,
        action: SettingsNavigationDraftAction,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.route.apply_navigation_draft_action(action);
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn take_navigation_editor_persisted_groups(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Vec<Vec<String>>> {
        let groups = self.route.take_navigation_editor_persisted_groups()?;
        cx.notify();
        Some(groups)
    }

    pub fn launch_at_login_snapshot(&self) -> LaunchAtLoginSnapshot {
        LaunchAtLoginSnapshot {
            enabled: self.launch_at_login_enabled,
            pending: self.launch_at_login_pending,
            error: self.launch_at_login_error.clone(),
        }
    }

    #[cfg(any(not(target_os = "macos"), test))]
    pub fn start_launch_at_login_operation(
        &mut self,
        operation: impl std::future::Future<Output = Result<bool, LaunchAtLoginError>> + 'static,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.launch_at_login_generation = self.launch_at_login_generation.wrapping_add(1);
        let generation = self.launch_at_login_generation;
        // Replacing the retained task cancels the old foreground completion;
        // the generation also rejects a result already queued for delivery.
        self.launch_at_login_task = None;
        self.launch_at_login_pending = true;
        self.launch_at_login_error = None;
        self.launch_at_login_task = Some(cx.spawn(async move |settings, cx| {
            let result = operation.await;
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_launch_at_login_operation(generation, result, cx);
            });
        }));
        cx.notify();
        generation
    }

    fn finish_launch_at_login_operation(
        &mut self,
        generation: u64,
        result: Result<bool, LaunchAtLoginError>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.launch_at_login_generation {
            return;
        }
        self.launch_at_login_task = None;
        self.launch_at_login_pending = false;
        match result {
            Ok(enabled) => {
                self.launch_at_login_enabled = enabled;
                self.launch_at_login_error = None;
            }
            Err(error) => self.launch_at_login_error = Some(error),
        }
        cx.notify();
    }

    #[cfg(target_os = "macos")]
    pub fn finish_launch_at_login_settings_handoff(
        &mut self,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        self.launch_at_login_generation = self.launch_at_login_generation.wrapping_add(1);
        let generation = self.launch_at_login_generation;
        let enabled = self.launch_at_login_enabled;
        self.finish_launch_at_login_operation(
            generation,
            result
                .map(|()| enabled)
                .map_err(|error| LaunchAtLoginError::OperationFailed(error.into())),
            cx,
        );
    }

    pub fn keybinding_scope_filter(&self) -> SettingsKeybindingScopeFilter {
        self.shortcuts.keybinding_scope_filter()
    }

    pub fn set_keybinding_scope_filter(
        &mut self,
        filter: SettingsKeybindingScopeFilter,
        cx: &mut Context<Self>,
    ) -> bool {
        self.shortcuts.set_keybinding_scope_filter(filter, cx)
    }

    pub fn keybinding_search_query(&self) -> &str {
        self.shortcuts.keybinding_search_query()
    }

    pub fn keybinding_recording_action_id(&self) -> Option<&str> {
        self.shortcuts.keybinding_recording_action_id()
    }

    pub fn keybinding_recording_combo(&self) -> Option<&crate::keybindings::KeyCombo> {
        self.shortcuts.keybinding_recording_combo()
    }

    pub fn keybinding_conflicts(&self) -> &[String] {
        self.shortcuts.keybinding_conflicts()
    }

    pub fn keybinding_recording_footer_focus(&self) -> Option<KeybindingRecordingFooterAction> {
        self.shortcuts.keybinding_recording_footer_focus()
    }

    pub fn start_keybinding_recording(
        &mut self,
        action_id: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.shortcuts.start_keybinding_recording(action_id, cx)
    }

    pub fn stop_keybinding_recording(&mut self, cx: &mut Context<Self>) {
        self.shortcuts.stop_keybinding_recording(cx)
    }

    pub fn handle_keybinding_recording_key(
        &mut self,
        event: &KeyDownEvent,
        overrides: &serde_json::Map<String, serde_json::Value>,
        definitions: &[crate::keybindings::ActionDefinition],
        cx: &mut Context<Self>,
    ) -> Option<KeybindingRecordingKeyAction> {
        self.shortcuts
            .handle_keybinding_recording_key(event, overrides, definitions, cx)
    }

    pub fn activate_keybinding_recording_footer(
        &mut self,
        action: KeybindingRecordingFooterAction,
        cx: &mut Context<Self>,
    ) -> bool {
        self.shortcuts
            .activate_keybinding_recording_footer(action, cx)
    }

    pub fn take_keybinding_recording_commit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<KeybindingRecordingCommit> {
        self.shortcuts.take_keybinding_recording_commit(cx)
    }

    pub fn open_keybinding_reset_confirm(&mut self, cx: &mut Context<Self>) {
        self.keybinding_reset_confirm_exit_task = None;
        self.keybinding_reset_confirm_open = true;
        self.keybinding_reset_confirm_presence.reopen();
        self.keybinding_reset_confirm_focused_action = None;
        cx.notify();
    }

    pub fn keybinding_reset_confirm_snapshot(&self) -> Option<KeybindingResetConfirmSnapshot> {
        self.keybinding_reset_confirm_open
            .then_some(KeybindingResetConfirmSnapshot {
                phase: self.keybinding_reset_confirm_presence.phase(),
                focused_action: self.keybinding_reset_confirm_focused_action,
            })
    }

    pub fn begin_keybinding_reset_confirm_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.keybinding_reset_confirm_open {
            return false;
        }
        let Some(generation) = self.keybinding_reset_confirm_presence.begin_exit() else {
            return false;
        };
        self.keybinding_reset_confirm_focused_action = None;
        self.keybinding_reset_confirm_exit_task = None;
        if delay.is_zero() {
            self.finish_keybinding_reset_confirm_exit(generation, cx);
            return true;
        }
        // Retaining the task makes reopen and Entity release cancel stale exits.
        self.keybinding_reset_confirm_exit_task = Some(cx.spawn(async move |settings, cx| {
            cx.background_executor().timer(delay).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_keybinding_reset_confirm_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    pub fn handle_keybinding_reset_confirm_key(
        &mut self,
        key: &str,
        shift: bool,
        blocked_by_primary_modifier: bool,
        cx: &mut Context<Self>,
    ) -> Option<KeybindingResetConfirmKeyAction> {
        if blocked_by_primary_modifier
            || !self.keybinding_reset_confirm_open
            || self.keybinding_reset_confirm_presence.phase()
                != oxideterm_gpui_ui::motion::ExitPhase::Visible
        {
            return None;
        }
        const ACTIONS: [ConfirmDialogAction; 2] =
            [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];
        match browser_behavior::modal_footer_key_action(
            key,
            shift,
            &ACTIONS,
            self.keybinding_reset_confirm_focused_action,
            ConfirmDialogAction::Cancel,
        ) {
            Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                self.keybinding_reset_confirm_focused_action = None;
                Some(KeybindingResetConfirmKeyAction::Cancel)
            }
            Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                self.keybinding_reset_confirm_focused_action = Some(action);
                cx.notify();
                Some(KeybindingResetConfirmKeyAction::Handled)
            }
            Some(browser_behavior::ModalFooterKeyAction::Activate(action)) => {
                self.keybinding_reset_confirm_focused_action = None;
                Some(match action {
                    ConfirmDialogAction::Cancel => KeybindingResetConfirmKeyAction::Cancel,
                    ConfirmDialogAction::Confirm => KeybindingResetConfirmKeyAction::Confirm,
                })
            }
            None => None,
        }
    }

    fn finish_keybinding_reset_confirm_exit(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.keybinding_reset_confirm_exit_task = None;
        if self.keybinding_reset_confirm_open
            && self
                .keybinding_reset_confirm_presence
                .finish_exit(generation)
        {
            self.keybinding_reset_confirm_open = false;
            self.keybinding_reset_confirm_presence.reopen();
            self.keybinding_reset_confirm_focused_action = None;
            cx.notify();
        }
    }

    pub fn start_external_store_watch(
        &mut self,
        settings_path: PathBuf,
        connections_path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.external_store_watch = Some(ExternalStoreWatch::new(settings_path, connections_path));
        // Retaining the task makes Entity release the only cancellation boundary.
        self.external_store_watch_task = Some(cx.spawn(async move |settings, cx| {
            loop {
                cx.background_executor()
                    .timer(EXTERNAL_STORE_WATCH_INTERVAL)
                    .await;
                let should_continue = settings
                    .update(cx, |settings, cx| {
                        let Some(watch) = settings.external_store_watch.as_mut() else {
                            return false;
                        };
                        if let Some(change) = watch.take_external_change() {
                            cx.emit(SettingsWorkspaceEvent::ExternalStoresChanged(change));
                        }
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
    }

    pub fn acknowledge_external_store_state(&mut self) {
        if let Some(watch) = self.external_store_watch.as_mut() {
            watch.refresh_observed_state();
        }
    }

    pub fn data_directory_confirm(&self) -> Option<&DataDirectoryConfirm> {
        self.data_directory_confirm.as_ref()
    }

    pub fn data_directory_confirm_is_visible(&self) -> bool {
        self.data_directory_confirm.is_some()
            && self.data_directory_confirm_presence.phase()
                == oxideterm_gpui_ui::motion::ExitPhase::Visible
    }

    pub fn data_directory_confirm_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.data_directory_confirm_presence.phase()
    }

    pub fn open_data_directory_reset_confirm(&mut self, cx: &mut Context<Self>) {
        self.data_directory_confirm_exit_task = None;
        self.data_directory_confirm_presence.reopen();
        self.data_directory_confirm = Some(DataDirectoryConfirm::Reset);
        cx.emit(SettingsWorkspaceEvent::DataDirectoryConfirmOpened);
        cx.notify();
    }

    pub fn start_data_directory_picker(
        &mut self,
        selection: impl std::future::Future<Output = Option<PathBuf>> + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.data_directory_picker_task.is_some() {
            return false;
        }
        self.data_directory_picker_task = Some(cx.spawn(async move |settings, cx| {
            let selected_path = selection.await;
            let _ = settings.update(cx, |settings, cx| {
                settings.data_directory_picker_task = None;
                let Some(path) = selected_path else {
                    return;
                };
                match oxideterm_settings::check_data_directory(&path) {
                    Ok(check) if check.has_existing_data => {
                        // Preserve the selected path until the user resolves
                        // the overwrite confirmation, even if settings hides.
                        settings.data_directory_confirm = Some(DataDirectoryConfirm::Conflict {
                            path,
                            files_found: check.files_found,
                        });
                        settings.data_directory_confirm_presence.reopen();
                        cx.emit(SettingsWorkspaceEvent::DataDirectoryConfirmOpened);
                    }
                    Ok(_) => settings.apply_data_directory(path, cx),
                    Err(error) => {
                        settings
                            .data_directory_results
                            .push_back(DataDirectoryOperationResult::Failed(error.to_string()));
                        cx.emit(SettingsWorkspaceEvent::DataDirectoryOperationReady);
                    }
                }
                cx.notify();
            });
        }));
        true
    }

    pub fn begin_data_directory_confirm_exit(
        &mut self,
        confirmed: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.data_directory_confirm.is_none() {
            return false;
        }
        let Some(generation) = self.data_directory_confirm_presence.begin_exit() else {
            return false;
        };
        if delay.is_zero() {
            self.finish_data_directory_confirm_exit(generation, confirmed, cx);
            return true;
        }
        self.data_directory_confirm_exit_task = Some(cx.spawn(async move |settings, cx| {
            cx.background_executor().timer(delay).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.data_directory_confirm_exit_task = None;
                settings.finish_data_directory_confirm_exit(generation, confirmed, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_data_directory_confirm_exit(
        &mut self,
        generation: u64,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.data_directory_confirm_presence.finish_exit(generation) {
            return;
        }
        self.data_directory_confirm_presence.reopen();
        let confirm = self.data_directory_confirm.take();
        if confirmed {
            match confirm {
                Some(DataDirectoryConfirm::Conflict { path, .. }) => {
                    self.apply_data_directory(path, cx);
                }
                Some(DataDirectoryConfirm::Reset) => self.reset_data_directory(cx),
                None => {}
            }
        }
        cx.notify();
    }

    fn apply_data_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let result = match oxideterm_settings::set_data_directory(&path) {
            Ok(()) => DataDirectoryOperationResult::Changed,
            Err(error) => DataDirectoryOperationResult::Failed(error.to_string()),
        };
        self.data_directory_results.push_back(result);
        cx.emit(SettingsWorkspaceEvent::DataDirectoryOperationReady);
    }

    fn reset_data_directory(&mut self, cx: &mut Context<Self>) {
        let result = match oxideterm_settings::reset_data_directory() {
            Ok(()) => DataDirectoryOperationResult::Reset,
            Err(error) => DataDirectoryOperationResult::Failed(error.to_string()),
        };
        self.data_directory_results.push_back(result);
        cx.emit(SettingsWorkspaceEvent::DataDirectoryOperationReady);
    }

    pub fn take_data_directory_results(&mut self) -> VecDeque<DataDirectoryOperationResult> {
        std::mem::take(&mut self.data_directory_results)
    }

    pub fn background_blur_preview(&self, system_dark: Option<bool>) -> Option<i64> {
        (self.background_blur_system_dark == system_dark)
            .then_some(self.background_blur_preview)
            .flatten()
    }

    pub fn finish_background_blur_drag(&mut self, cx: &mut Context<Self>) {
        self.background_blur_commit_task = None;
        self.finish_background_blur_commit(self.background_blur_commit_generation, cx);
    }

    pub fn update_background_blur_preview(
        &mut self,
        persisted_value: i64,
        preview_value: i64,
        system_dark: Option<bool>,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_blur_preview == Some(preview_value)
            && self.background_blur_system_dark == system_dark
            || (self.background_blur_preview.is_none() && persisted_value == preview_value)
        {
            return false;
        }

        if self.background_blur_preview.is_some() && self.background_blur_system_dark != system_dark
        {
            self.finish_background_blur_drag(cx);
        }
        self.background_blur_preview = Some(preview_value);
        self.background_blur_system_dark = system_dark;
        self.background_blur_commit_generation =
            self.background_blur_commit_generation.wrapping_add(1);
        let generation = self.background_blur_commit_generation;
        self.background_blur_commit_task = None;

        if delay.is_zero() {
            self.finish_background_blur_commit(generation, cx);
            return true;
        }

        // Replacing this retained task cancels the previous debounce without
        // leaving a detached root timer alive after another slider movement.
        self.background_blur_commit_task = Some(cx.spawn(async move |settings, cx| {
            cx.background_executor().timer(delay).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.background_blur_commit_task = None;
                settings.finish_background_blur_commit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_background_blur_commit(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.background_blur_commit_generation != generation {
            return;
        }
        let Some(value) = self.background_blur_preview.take() else {
            return;
        };
        cx.emit(SettingsWorkspaceEvent::BackgroundBlurCommitReady(
            self.background_blur_system_dark,
            value,
        ));
        cx.notify();
    }

    pub fn initialize_background_gallery(&mut self, images: Vec<String>) {
        self.background_images = Arc::from(images);
    }

    pub fn background_media_is_animated(
        &mut self,
        path: Option<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(path) = path.map(PathBuf::from) else {
            return false;
        };
        if let Some(info) = self.background_media_info.get(&path) {
            return matches!(info, Ok(true));
        }
        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.background_media_info_tasks.entry(path)
        {
            let inspect_path = entry.key().clone();
            // The settings entity owns these short metadata requests, shared by both previews.
            let task = cx.spawn(async move |settings, cx| {
                let read_path = inspect_path.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { oxideterm_background_media::is_animated_media(&read_path) })
                    .await;
                let _ = settings.update(cx, |settings, cx| {
                    settings.background_media_info_tasks.remove(&inspect_path);
                    settings.background_media_info.insert(inspect_path, result);
                    cx.notify();
                });
            });
            entry.insert(task);
        }
        false
    }

    pub fn background_media_has_animation(&self, path: Option<&str>) -> bool {
        path.and_then(|path| self.background_media_info.get(Path::new(path)))
            .is_some_and(|info| matches!(info, Ok(true)))
    }

    pub fn background_images_snapshot(&self) -> Arc<[String]> {
        Arc::clone(&self.background_images)
    }

    pub fn add_background_pack_images(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        if self.background_gallery_task.is_some() {
            self.background_pack_images_pending.extend_from_slice(paths);
        }
        let mut gallery = self.background_images.to_vec();
        for path in paths {
            let path = path.to_string_lossy().into_owned();
            if !gallery.contains(&path) {
                gallery.insert(0, path);
            }
        }
        self.background_images = Arc::from(gallery);
        // Installing media changes only the gallery, never either appearance's selection.
        cx.notify();
    }

    pub fn start_background_image_import(
        &mut self,
        selection: impl std::future::Future<Output = Option<Vec<PathBuf>>> + 'static,
        settings_path: PathBuf,
        current_path: Option<PathBuf>,
        system_dark: Option<bool>,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_gallery_task.is_some() {
            return false;
        }
        self.background_gallery_system_dark = system_dark;

        self.background_gallery_task = Some(cx.spawn(async move |settings, cx| {
            let Some(paths) = selection.await else {
                let _ = settings.update(cx, |settings, cx| {
                    settings.background_gallery_task = None;
                    settings.background_pack_images_pending.clear();
                    cx.notify();
                });
                return;
            };
            let source_paths = paths;
            if source_paths.is_empty() {
                let _ = settings.update(cx, |settings, cx| {
                    settings.background_gallery_task = None;
                    settings.background_pack_images_pending.clear();
                    cx.notify();
                });
                return;
            }

            let validation = runtime
                .spawn_blocking(move || {
                    for path in &source_paths {
                        if !oxideterm_settings::is_supported_background_image(path) {
                            return Err(oxideterm_background_media::MediaError::Unsupported);
                        }
                        oxideterm_background_media::decode_poster(
                            path,
                            oxideterm_background_media::OutputParams {
                                width: 256,
                                height: 144,
                                fit: oxideterm_background_media::BackgroundFit::Cover,
                                blur: 0.0,
                                limits: Default::default(),
                            },
                        )?;
                    }
                    Ok(source_paths)
                })
                .await;
            let source_paths = match validation {
                Ok(Ok(paths)) => paths,
                result => {
                    let failure = match result {
                        Ok(Err(oxideterm_background_media::MediaError::Unsupported)) => {
                            oxideterm_gpui_background::BackgroundFailure::Unsupported
                        }
                        Ok(Err(oxideterm_background_media::MediaError::ResourceExhausted)) => {
                            oxideterm_gpui_background::BackgroundFailure::ResourceExhausted
                        }
                        _ => oxideterm_gpui_background::BackgroundFailure::Decode,
                    };
                    let _ = settings.update(cx, |settings, cx| {
                        settings.background_gallery_task = None;
                        settings.background_pack_images_pending.clear();
                        settings
                            .background_gallery_results
                            .push_back(BackgroundGalleryOperationResult::MediaFailed(failure));
                        cx.emit(SettingsWorkspaceEvent::BackgroundGalleryOperationReady);
                        cx.notify();
                    });
                    return;
                }
            };

            let result = runtime
                .spawn_blocking(move || -> anyhow::Result<(Vec<String>, Option<String>)> {
                    let mut active_path = current_path.filter(|path| {
                        path.is_file()
                            && oxideterm_settings::is_supported_background_image(path.as_path())
                    });
                    if let Some(current) = active_path.as_ref()
                        && !oxideterm_settings::is_managed_background_image(&settings_path, current)
                    {
                        // Preserve a compatibility path inside the managed
                        // gallery before another image becomes active.
                        active_path = oxideterm_settings::import_background_images(
                            &settings_path,
                            std::slice::from_ref(current),
                        )?
                        .into_iter()
                        .next();
                    }

                    let imported = oxideterm_settings::import_background_images(
                        &settings_path,
                        &source_paths,
                    )?;
                    if active_path.is_none() {
                        active_path = imported.first().cloned();
                    }
                    let mut gallery = background_gallery_strings(&settings_path)?;
                    let active_path = active_path.map(|path| path.to_string_lossy().into_owned());
                    if let Some(active) = active_path.as_ref()
                        && !gallery.contains(active)
                    {
                        gallery.insert(0, active.clone());
                    }
                    Ok((gallery, active_path))
                })
                .await
                .map_err(|_| ())
                .and_then(|result| result.map_err(|_| ()));

            let _ = settings.update(cx, |settings, cx| {
                settings.background_gallery_task = None;
                settings.finish_background_gallery_operation(result, cx);
            });
        }));
        cx.notify();
        true
    }

    pub fn remove_background_image(
        &mut self,
        settings_path: PathBuf,
        image_path: String,
        current_path: Option<String>,
        system_dark: Option<bool>,
        runtime: tokio::runtime::Handle,
        is_bundled: fn(&Path, &Path) -> bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_gallery_task.is_some()
            || is_bundled(&settings_path, Path::new(&image_path))
        {
            return false;
        }
        self.background_gallery_system_dark = system_dark;

        if !oxideterm_settings::is_managed_background_image(&settings_path, Path::new(&image_path))
        {
            // Compatibility paths are user-owned; removing one only updates
            // the gallery and never deletes the source file.
            let mut gallery = self.background_images.to_vec();
            gallery.retain(|candidate| candidate != &image_path);
            let active_path = current_path.filter(|active| active != &image_path);
            self.finish_background_gallery_operation(Ok((gallery, active_path)), cx);
            return true;
        }

        self.background_gallery_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime
                .spawn_blocking(move || -> anyhow::Result<(Vec<String>, Option<String>)> {
                    oxideterm_settings::remove_background_image(
                        &settings_path,
                        Path::new(&image_path),
                    )?;
                    let mut gallery = background_gallery_strings(&settings_path)?;
                    let active_path = current_path.filter(|active| active != &image_path);
                    if let Some(active) = active_path.as_ref()
                        && !gallery.contains(active)
                    {
                        gallery.insert(0, active.clone());
                    }
                    Ok((gallery, active_path))
                })
                .await
                .map_err(|_| ())
                .and_then(|result| result.map_err(|_| ()));

            let _ = settings.update(cx, |settings, cx| {
                settings.background_gallery_task = None;
                settings.finish_background_gallery_operation(result, cx);
            });
        }));
        cx.notify();
        true
    }

    pub fn clear_background_image_gallery(
        &mut self,
        settings_path: PathBuf,
        current_path: Option<String>,
        system_dark: Option<bool>,
        runtime: tokio::runtime::Handle,
        is_bundled: fn(&Path, &Path) -> bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_gallery_task.is_some() {
            return false;
        }
        self.background_gallery_system_dark = system_dark;

        self.background_gallery_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime
                .spawn_blocking(move || -> anyhow::Result<(Vec<String>, Option<String>)> {
                    for image_path in oxideterm_settings::list_background_images(&settings_path)? {
                        if !is_bundled(&settings_path, &image_path) {
                            oxideterm_settings::remove_background_image(
                                &settings_path,
                                &image_path,
                            )?;
                        }
                    }
                    let gallery = background_gallery_strings(&settings_path)?;
                    let active_path = current_path.filter(|active| gallery.contains(active));
                    Ok((gallery, active_path))
                })
                .await
                .map_err(|_| ())
                .and_then(|result| result.map_err(|_| ()));

            let _ = settings.update(cx, |settings, cx| {
                settings.background_gallery_task = None;
                settings.finish_background_gallery_operation(result, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_background_gallery_operation(
        &mut self,
        result: Result<(Vec<String>, Option<String>), ()>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((mut gallery, active_path)) => {
                // A gallery worker may have listed files before the package install completed.
                for path in std::mem::take(&mut self.background_pack_images_pending) {
                    if path.is_file() {
                        let path = path.to_string_lossy().into_owned();
                        if !gallery.contains(&path) {
                            gallery.insert(0, path);
                        }
                    }
                }
                self.background_images = Arc::from(gallery);
                self.background_gallery_results.push_back(
                    BackgroundGalleryOperationResult::Updated(
                        self.background_gallery_system_dark,
                        active_path,
                    ),
                );
            }
            Err(()) => {
                self.background_pack_images_pending.clear();
                self.background_gallery_results
                    .push_back(BackgroundGalleryOperationResult::Failed);
            }
        }
        cx.emit(SettingsWorkspaceEvent::BackgroundGalleryOperationReady);
        cx.notify();
    }

    pub fn take_background_gallery_results(
        &mut self,
    ) -> VecDeque<BackgroundGalleryOperationResult> {
        std::mem::take(&mut self.background_gallery_results)
    }

    pub fn start_theme_import(
        &mut self,
        target: ThemeTarget,
        system_dark: Option<bool>,
        selection: impl std::future::Future<Output = Option<PathBuf>> + 'static,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.theme_import_task.is_some() {
            return false;
        }

        self.theme_import_task = Some(cx.spawn(async move |settings, cx| {
            let Some(path) = selection.await else {
                let _ = settings.update(cx, |settings, cx| {
                    settings.theme_import_task = None;
                    cx.notify();
                });
                return;
            };
            let result = runtime
                .spawn_blocking(move || {
                    std::fs::read_to_string(path)
                        .map_err(|error| error.to_string())
                        .and_then(|contents| {
                            oxideterm_settings_model::import_custom_theme(&contents)
                        })
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);

            let _ = settings.update(cx, |settings, cx| {
                settings.theme_import_task = None;
                match result {
                    Ok((theme_id, name, value)) => {
                        settings
                            .theme_import_results
                            .push_back(ThemeImportResult::Imported {
                                target,
                                system_dark,
                                theme_id,
                                name,
                                value,
                            });
                    }
                    Err(error) => settings
                        .theme_import_results
                        .push_back(ThemeImportResult::Failed(error)),
                }
                cx.emit(SettingsWorkspaceEvent::ThemeImportReady);
                cx.notify();
            });
        }));
        cx.notify();
        true
    }

    pub fn take_theme_import_results(&mut self) -> VecDeque<ThemeImportResult> {
        std::mem::take(&mut self.theme_import_results)
    }

    pub fn theme_editor(&self) -> Option<&ThemeEditorState> {
        self.theme_editor.as_deref()
    }

    /// Shares one immutable render snapshot without copying the color arrays.
    pub fn theme_editor_snapshot(&self) -> Option<Arc<ThemeEditorState>> {
        self.theme_editor.as_ref().map(Arc::clone)
    }

    pub fn theme_editor_open(&self) -> bool {
        self.theme_editor.is_some()
    }

    pub fn theme_editor_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.theme_editor_presence.phase()
    }

    pub fn open_theme_editor(&mut self, editor: ThemeEditorState, cx: &mut Context<Self>) {
        // Replacing the retained task cancels a stale exit before installing
        // the new draft. The presence generation is an additional stale guard.
        self.theme_editor_exit_task = None;
        self.theme_editor_exit_action = None;
        self.theme_editor = Some(Arc::new(editor));
        self.theme_editor_presence.reopen();
        // A modal editor supersedes any settings input that was focused behind it.
        self.settings_focused_input = None;
        cx.notify();
    }

    pub fn select_theme_editor_section(
        &mut self,
        section: ThemeEditorSection,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.theme_editor.as_mut() else {
            return false;
        };
        let editor = Arc::make_mut(editor);
        if editor.active_section == section {
            return false;
        }
        editor.active_section = section;
        cx.notify();
        true
    }

    pub fn duplicate_theme_editor_from(&mut self, theme_id: &str, cx: &mut Context<Self>) -> bool {
        let Some(editor) = self.theme_editor.as_mut() else {
            return false;
        };
        let editor = Arc::make_mut(editor);
        let theme = theme_by_id(theme_id);
        editor.duplicate_theme.clear();
        editor.duplicate_theme.push_str(theme.id);
        editor.duplicate_theme_touched = true;
        editor.terminal_colors = terminal_theme_to_colors(theme.terminal);
        editor.ui_colors =
            app_ui_colors_to_colors(oxideterm_theme::ThemeTokens::from_builtin(theme).ui);
        cx.notify();
        true
    }

    pub fn derive_theme_editor_ui_colors(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(editor) = self.theme_editor.as_mut() else {
            return false;
        };
        let editor = Arc::make_mut(editor);
        let terminal = editor_terminal_theme(&editor.terminal_colors);
        editor.ui_colors = app_ui_colors_to_colors(derive_ui_colors_from_terminal(terminal));
        cx.notify();
        true
    }

    pub fn cancel_theme_editor(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        self.begin_theme_editor_exit(ThemeEditorExitAction::Cancel, delay, cx)
    }

    pub fn save_theme_editor(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        if self
            .theme_editor
            .as_ref()
            .is_none_or(|editor| editor.name.trim().is_empty())
        {
            return false;
        }
        self.begin_theme_editor_exit(ThemeEditorExitAction::Save, delay, cx)
    }

    pub fn delete_theme_editor(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        if self
            .theme_editor
            .as_ref()
            .and_then(|editor| editor.edit_theme_id.as_ref())
            .is_none()
        {
            return false;
        }
        self.begin_theme_editor_exit(ThemeEditorExitAction::Delete, delay, cx)
    }

    fn begin_theme_editor_exit(
        &mut self,
        action: ThemeEditorExitAction,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.theme_editor.is_none() {
            return false;
        }
        let Some(generation) = self.theme_editor_presence.begin_exit() else {
            return false;
        };
        self.theme_editor_exit_action = Some(action);
        self.theme_editor_exit_task = None;
        if self
            .settings_focused_input
            .is_some_and(is_theme_editor_input)
        {
            self.settings_focused_input = None;
        }
        if delay.is_zero() {
            self.finish_theme_editor_exit(generation, cx);
            return true;
        }
        // The settings Entity retains the exit task so reopen and release
        // cancel it without a reverse dependency on the workspace root.
        self.theme_editor_exit_task = Some(cx.spawn(async move |settings, cx| {
            cx.background_executor().timer(delay).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_theme_editor_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_theme_editor_exit(&mut self, generation: u64, cx: &mut Context<Self>) {
        if !self.theme_editor_presence.finish_exit(generation) {
            return;
        }
        self.theme_editor_exit_task = None;
        let action = self
            .theme_editor_exit_action
            .take()
            .unwrap_or(ThemeEditorExitAction::Cancel);
        let editor = self.theme_editor.take();
        self.editor_background_preview
            .update(cx, |preview, cx| preview.set_preferences(None, cx));
        if self
            .settings_focused_input
            .is_some_and(is_theme_editor_input)
        {
            self.settings_focused_input = None;
        }
        let result = match (action, editor) {
            (ThemeEditorExitAction::Save, Some(editor)) => {
                Some(ThemeEditorOperationResult::Save(editor))
            }
            (ThemeEditorExitAction::Delete, Some(editor)) if editor.edit_theme_id.is_some() => {
                Some(ThemeEditorOperationResult::Delete(editor))
            }
            (ThemeEditorExitAction::Cancel, _)
            | (ThemeEditorExitAction::Delete, Some(_))
            | (_, None) => None,
        };
        self.theme_editor_presence.reopen();
        if let Some(result) = result {
            self.theme_editor_results.push_back(result);
            cx.emit(SettingsWorkspaceEvent::ThemeEditorOperationReady);
        }
        cx.notify();
    }

    pub fn take_theme_editor_results(&mut self) -> VecDeque<ThemeEditorOperationResult> {
        std::mem::take(&mut self.theme_editor_results)
    }

    pub fn start_keybinding_export(
        &mut self,
        selection: impl std::future::Future<Output = Option<PathBuf>> + 'static,
        overrides: serde_json::Map<String, serde_json::Value>,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> u64 {
        let generation = self.replace_keybinding_file_operation();
        self.keybinding_file_operation_task = Some(cx.spawn(async move |settings, cx| {
            let Some(directory) = selection.await else {
                let _ = settings.update(cx, |settings, cx| {
                    settings.finish_keybinding_file_operation(generation, None, cx);
                });
                return;
            };
            let result = runtime
                .spawn_blocking(move || {
                    let path = directory.join("oxideterm-keybindings.json");
                    serde_json::to_string_pretty(&overrides)
                        .map_err(|_| ())
                        .and_then(|json| std::fs::write(path, json).map_err(|_| ()))
                })
                .await
                .map_err(|_| ())
                .and_then(|result| result)
                .map(|()| KeybindingFileOperationResult::Exported)
                .unwrap_or(KeybindingFileOperationResult::ExportFailed);
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_keybinding_file_operation(generation, Some(result), cx);
            });
        }));
        cx.notify();
        generation
    }

    pub fn start_keybinding_import(
        &mut self,
        selection: impl std::future::Future<Output = Option<PathBuf>> + 'static,
        runtime: tokio::runtime::Handle,
        sanitize: fn(
            serde_json::Value,
        ) -> Result<serde_json::Map<String, serde_json::Value>, String>,
        cx: &mut Context<Self>,
    ) -> u64 {
        let generation = self.replace_keybinding_file_operation();
        self.keybinding_file_operation_task = Some(cx.spawn(async move |settings, cx| {
            let Some(path) = selection.await else {
                let _ = settings.update(cx, |settings, cx| {
                    settings.finish_keybinding_file_operation(generation, None, cx);
                });
                return;
            };
            let result = runtime
                .spawn_blocking(move || {
                    std::fs::read_to_string(path)
                        .map_err(|_| ())
                        .and_then(|content| {
                            serde_json::from_str::<serde_json::Value>(&content).map_err(|_| ())
                        })
                        .and_then(|value| sanitize(value).map_err(|_| ()))
                })
                .await
                .map_err(|_| ())
                .and_then(|result| result)
                .map(|overrides| KeybindingFileOperationResult::Imported { overrides })
                .unwrap_or(KeybindingFileOperationResult::ImportFailed);
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_keybinding_file_operation(generation, Some(result), cx);
            });
        }));
        cx.notify();
        generation
    }

    fn replace_keybinding_file_operation(&mut self) -> u64 {
        // Dropping the retained task cancels the superseded dialog or worker.
        self.keybinding_file_operation_task = None;
        self.keybinding_file_operation_generation =
            self.keybinding_file_operation_generation.wrapping_add(1);
        self.keybinding_file_operation_generation
    }

    fn finish_keybinding_file_operation(
        &mut self,
        generation: u64,
        result: Option<KeybindingFileOperationResult>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.keybinding_file_operation_generation {
            return;
        }
        self.keybinding_file_operation_task = None;
        // Retire the generation before publishing so duplicate or late
        // completions cannot enqueue the same user-visible result twice.
        self.keybinding_file_operation_generation =
            self.keybinding_file_operation_generation.wrapping_add(1);
        let Some(result) = result else {
            cx.notify();
            return;
        };
        if matches!(&result, KeybindingFileOperationResult::Imported { .. }) {
            self.shortcuts.clear_recording();
        }
        self.keybinding_file_operation_results.push_back(result);
        cx.emit(SettingsWorkspaceEvent::KeybindingFileOperationReady);
        cx.notify();
    }

    pub fn take_keybinding_file_operation_results(
        &mut self,
    ) -> VecDeque<KeybindingFileOperationResult> {
        std::mem::take(&mut self.keybinding_file_operation_results)
    }

    pub fn portable_status_snapshot(&self) -> PortableStatusSnapshot {
        PortableStatusSnapshot {
            status: self.portable_status.clone(),
            error: self.portable_status_error.clone(),
            exportable_secret_count: self.portable_exportable_secret_count,
            refresh_pending: self.portable_refresh_pending,
        }
    }

    pub fn portable_mode(&self) -> Option<bool> {
        self.portable_status
            .as_ref()
            .map(|status| status.is_portable)
    }

    pub fn start_portable_status_refresh(
        &mut self,
        force: bool,
        runtime: Arc<tokio::runtime::Runtime>,
        worker: impl FnOnce() -> PortableStatusRefresh + Send + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.portable_refresh_pending {
            return false;
        }
        if !force
            && (self.portable_status.is_some() || self.portable_status_error.is_some())
            && self.portable_exportable_secret_count.is_some()
        {
            return false;
        }

        self.portable_refresh_pending = true;
        self.portable_refresh_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime.spawn_blocking(worker).await;
            let _ = settings.update(cx, |settings, cx| {
                settings
                    .finish_portable_status_refresh(result.map_err(|error| error.to_string()), cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_portable_status_refresh(
        &mut self,
        result: Result<PortableStatusRefresh, String>,
        cx: &mut Context<Self>,
    ) {
        self.portable_refresh_task = None;
        self.portable_refresh_pending = false;
        match result {
            Ok(PortableStatusRefresh {
                status: Ok(status),
                exportable_secret_count,
            }) => {
                self.portable_status = Some(status);
                self.portable_status_error = None;
                self.portable_exportable_secret_count = Some(exportable_secret_count);
            }
            Ok(PortableStatusRefresh {
                status: Err(error),
                exportable_secret_count,
            }) => {
                self.portable_status = None;
                self.portable_status_error = Some(error);
                self.portable_exportable_secret_count = Some(exportable_secret_count);
            }
            Err(error) => {
                self.portable_status = None;
                self.portable_status_error = Some(error);
            }
        }
        cx.notify();
    }

    pub fn invalidate_portable_status(&mut self, cx: &mut Context<Self>) {
        self.portable_status = None;
        self.portable_status_error = None;
        cx.notify();
    }

    pub fn settings_entity_focused_input(&self) -> Option<SettingsInput> {
        self.settings_focused_input
    }

    pub fn settings_search_open(&self) -> bool {
        self.settings_search_open
    }

    pub fn settings_search_query(&self) -> &str {
        &self.settings_search_query
    }

    pub fn open_settings_search(&mut self, cx: &mut Context<Self>) {
        self.settings_search_open = true;
        cx.notify();
    }

    pub fn close_settings_search(&mut self, clear_query: bool, cx: &mut Context<Self>) -> bool {
        let mut changed = std::mem::take(&mut self.settings_search_open);
        if self.settings_focused_input == Some(SettingsInput::SettingsSearch) {
            self.settings_focused_input = None;
            changed = true;
        }
        if clear_query && !self.settings_search_query.is_empty() {
            self.settings_search_query.clear();
            changed = true;
        }
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn clear_settings_search_query(&mut self, cx: &mut Context<Self>) -> bool {
        if self.settings_search_query.is_empty() {
            return false;
        }
        self.settings_search_query.clear();
        cx.notify();
        true
    }

    pub fn settings_entity_input_value(&self, input: SettingsInput) -> Option<&str> {
        if input.is_ai_mcp() {
            return self.ai_mcp.input(input);
        }
        if crate::ai::providers::is_provider_draft(input) {
            return Some(if self.settings_focused_input == Some(input) {
                self.provider_input_draft.as_str()
            } else {
                ""
            });
        }
        if crate::highlight_page::is_highlight_input(input) {
            return (self.settings_focused_input == Some(input))
                .then_some(self.highlight.input_draft.as_str());
        }
        if crate::terminal_triggers::is_terminal_trigger_input(input) {
            return (self.settings_focused_input == Some(input))
                .then_some(self.trigger_input_draft.as_str());
        }
        match input {
            SettingsInput::TerminalCommandSpecsJson => self
                .command_specs
                .as_ref()
                .map(|draft| draft.value.as_str()),
            SettingsInput::SettingsSearch => Some(&self.settings_search_query),
            SettingsInput::CustomThemeName => self
                .theme_editor
                .as_deref()
                .map(|editor| editor.name.as_str()),
            SettingsInput::CustomThemeTerminalColor(index) => self
                .theme_editor
                .as_deref()
                .and_then(|editor| editor.terminal_colors.get(index))
                .map(String::as_str),
            SettingsInput::CustomThemeUiColor(index) => self
                .theme_editor
                .as_deref()
                .and_then(|editor| editor.ui_colors.get(index))
                .map(String::as_str),
            SettingsInput::KeybindingSearch => Some(&self.shortcuts.keybinding_search_query),
            SettingsInput::SkillSearch => Some(&self.skill_search_query),
            SettingsInput::PortableCurrentPassword => Some(&self.portable_current_password),
            SettingsInput::PortableNewPassword => Some(&self.portable_new_password),
            SettingsInput::PortableConfirmPassword => Some(&self.portable_confirm_password),
            SettingsInput::DiscoverySecret(index) => self.discovery.editor.as_ref()?.fields.get(index)?.secret_value(),
            SettingsInput::DiscoveryField(index) | SettingsInput::DiscoveryMultiline(index) => self
                .discovery
                .editor
                .as_ref()?
                .fields
                .get(index)
                .map(|field| field.value.as_str()),
            SettingsInput::ManagedKeyFilePath => Some(&self.managed_key_file_path),
            SettingsInput::ManagedKeyFileName => Some(&self.managed_key_file_name),
            SettingsInput::ManagedKeyFilePassphrase => Some(&self.managed_key_file_passphrase),
            SettingsInput::ManagedKeyPasteName => Some(&self.managed_key_paste_name),
            SettingsInput::ManagedKeyPastePrivateKey => Some(&self.managed_key_paste_private_key),
            SettingsInput::ManagedKeyPastePassphrase => Some(&self.managed_key_paste_passphrase),
            SettingsInput::ManagedKeyRenameName => Some(&self.managed_key_rename_name),
            SettingsInput::NetworkProxyPassword => Some(&self.network_proxy_password),
            SettingsInput::NetworkProxyTestHost => Some(&self.network_proxy_test_host),
            SettingsInput::NetworkProxyTestPort => Some(&self.network_proxy_test_port),
            SettingsInput::LocalPrivilegeLabel => Some(&self.privilege_draft.label),
            SettingsInput::TotpName => self.totp_draft.as_ref().map(|draft| draft.name.as_str()),
            SettingsInput::TotpSecret => {
                self.totp_draft.as_ref().map(|draft| draft.secret.as_str())
            }
            SettingsInput::TotpPattern => {
                self.totp_draft.as_ref().map(|draft| draft.pattern.as_str())
            }
            SettingsInput::TotpPeriod => {
                self.totp_draft.as_ref().map(|draft| draft.period.as_str())
            }
            SettingsInput::LocalPrivilegeUsernameHint => Some(&self.privilege_draft.username_hint),
            SettingsInput::LocalPrivilegeSecret => Some(&self.privilege_draft.secret),
            SettingsInput::LocalPrivilegePromptPatterns => {
                Some(&self.privilege_draft.prompt_patterns)
            }
            SettingsInput::LocalPrivilegeHostPatterns => Some(&self.privilege_draft.host_patterns),
            SettingsInput::LocalPrivilegeSecondFactorPatterns => {
                Some(&self.privilege_draft.second_factor_patterns)
            }
            SettingsInput::LocalPrivilegeSecondFactorUserHint => {
                Some(&self.privilege_draft.second_factor_user_hint)
            }
            SettingsInput::ConnectionImportTargetGroup => {
                Some(&self.connection_import_target_group)
            }
            SettingsInput::KnownHostsSearch => Some(&self.known_hosts.query),
            _ => None,
        }
    }

    pub fn focus_settings_entity_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_focused_input != Some(input) {
            self.provider_input_draft.zeroize();
        }
        if crate::terminal_triggers::is_terminal_trigger_input(input) {
            if !self.terminal_triggers.editor_open {
                return false;
            }
            if self.settings_focused_input != Some(input) {
                let Some(value) = self.terminal_triggers.editor_input_value(input) else {
                    return false;
                };
                self.trigger_input_draft = Zeroizing::new(value);
            }
            self.settings_focused_input = Some(input);
            cx.notify();
            return true;
        }
        let portable_open = self.portable_dialog == Some(PortableSettingsDialog::ChangePassword);
        let can_focus = match input {
            _ if input.is_ai_mcp() => self.ai_mcp.input(input).is_some(),
            SettingsInput::TerminalCommandSpecsJson => self.command_specs_open(),
            SettingsInput::DiscoverySecret(_)
            | SettingsInput::DiscoveryField(_)
            | SettingsInput::DiscoveryMultiline(_) => {
                self.settings_entity_input_value(input).is_some()
            }
            SettingsInput::SettingsSearch => self.settings_search_open,
            SettingsInput::CustomThemeName
            | SettingsInput::CustomThemeTerminalColor(_)
            | SettingsInput::CustomThemeUiColor(_) => self.theme_editor.is_some(),
            SettingsInput::KeybindingSearch => true,
            SettingsInput::SkillSearch => true,
            SettingsInput::AiProviderApiKey(_) | SettingsInput::AiProviderNewModel(_) => true,
            SettingsInput::PortableCurrentPassword
            | SettingsInput::PortableNewPassword
            | SettingsInput::PortableConfirmPassword => portable_open,
            SettingsInput::ManagedKeyFilePath
            | SettingsInput::ManagedKeyFileName
            | SettingsInput::ManagedKeyFilePassphrase => matches!(
                self.managed_key_dialog,
                Some(SettingsManagedKeyDialog::ImportFile)
            ),
            SettingsInput::ManagedKeyPasteName
            | SettingsInput::ManagedKeyPastePrivateKey
            | SettingsInput::ManagedKeyPastePassphrase => {
                matches!(
                    self.managed_key_dialog,
                    Some(SettingsManagedKeyDialog::Paste)
                )
            }
            SettingsInput::ManagedKeyRenameName => matches!(
                self.managed_key_dialog,
                Some(SettingsManagedKeyDialog::Rename { .. })
            ),
            SettingsInput::NetworkProxyPassword
            | SettingsInput::NetworkProxyTestHost
            | SettingsInput::NetworkProxyTestPort => true,
            SettingsInput::LocalPrivilegeLabel
            | SettingsInput::TotpName
            | SettingsInput::TotpSecret
            | SettingsInput::TotpPattern
            | SettingsInput::TotpPeriod
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns
            | SettingsInput::LocalPrivilegeHostPatterns
            | SettingsInput::LocalPrivilegeSecondFactorPatterns
            | SettingsInput::LocalPrivilegeSecondFactorUserHint => true,
            SettingsInput::ConnectionImportTargetGroup | SettingsInput::KnownHostsSearch => true,
            _ => false,
        };
        if !can_focus {
            return false;
        }
        self.trigger_input_draft.zeroize();
        self.settings_focused_input = Some(input);
        cx.notify();
        true
    }

    pub fn focus_highlight_input(
        &mut self,
        input: SettingsInput,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if self.settings_focused_input != Some(input) {
            self.provider_input_draft.zeroize();
            self.trigger_input_draft.zeroize();
            self.highlight.input_draft = value;
            self.settings_focused_input = Some(input);
            cx.notify();
        }
    }

    pub fn queue_highlight_preferences_refresh(&mut self, cx: &mut Context<Self>) {
        self.highlight.refresh_task = Some(cx.spawn(async |settings, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let _ = settings.update(cx, |settings, cx| {
                settings.highlight.refresh_task = None;
                cx.emit(SettingsWorkspaceEvent::RefreshTerminalHighlightPreferences);
            });
        }));
    }

    pub fn blur_settings_entity_input(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.settings_focused_input.take().is_some();
        if changed {
            self.provider_input_draft.zeroize();
            self.trigger_input_draft.zeroize();
            self.highlight.input_draft.clear();
            cx.notify();
        }
        changed
    }

    pub fn take_provider_key(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> Option<Zeroizing<String>> {
        if !matches!(input, SettingsInput::AiProviderApiKey(_))
            || self.settings_focused_input != Some(input)
        {
            return None;
        }
        self.settings_focused_input = None;
        let secret = std::mem::take(&mut self.provider_input_draft);
        cx.notify();
        (!secret.trim().is_empty()).then_some(secret)
    }

    pub fn clear_provider_model_draft(&mut self, input: SettingsInput, cx: &mut Context<Self>) {
        if matches!(input, SettingsInput::AiProviderNewModel(_))
            && self.settings_focused_input == Some(input)
        {
            self.provider_input_draft.zeroize();
            cx.notify();
        }
    }

    pub fn replace_settings_entity_input(
        &mut self,
        input: SettingsInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_focused_input != Some(input) {
            return false;
        }
        let Some(value) = self.settings_entity_input_mut(input) else {
            return false;
        };
        oxideterm_editor_core::utf16::replace_utf16(value, replacement_range, text);
        if matches!(
            input,
            SettingsInput::CustomThemeTerminalColor(_) | SettingsInput::CustomThemeUiColor(_)
        ) {
            // Preserve the legacy color-editor contract: partial values remain
            // valid while typing, but surrounding whitespace is discarded.
            let trimmed = value.trim();
            if trimmed.len() != value.len() {
                *value = trimmed.to_string();
            }
        }
        self.clear_settings_entity_input_error(input);
        self.apply_active_trigger_input();
        if crate::highlight_page::is_highlight_input(input) {
            cx.emit(SettingsWorkspaceEvent::HighlightDraftChanged(input));
        }
        cx.notify();
        true
    }

    pub fn pop_settings_entity_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_focused_input != Some(input) {
            return false;
        }
        let Some(value) = self.settings_entity_input_mut(input) else {
            return false;
        };
        value.pop();
        self.clear_settings_entity_input_error(input);
        self.apply_active_trigger_input();
        if crate::highlight_page::is_highlight_input(input) {
            cx.emit(SettingsWorkspaceEvent::HighlightDraftChanged(input));
        }
        cx.notify();
        true
    }

    pub fn apply_active_trigger_input(&mut self) -> bool {
        self.settings_focused_input
            .filter(|input| crate::terminal_triggers::is_terminal_trigger_input(*input))
            .is_none_or(|input| {
                self.terminal_triggers
                    .apply_editor_input(input, &self.trigger_input_draft)
            })
    }

    fn clear_settings_entity_input_error(&mut self, input: SettingsInput) {
        match input {
            SettingsInput::DiscoveryField(index) | SettingsInput::DiscoveryMultiline(index) => {
                if let Some(editor) = &mut self.discovery.editor {
                    editor.clear_dependent_secrets(index);
                }
            }
            SettingsInput::PortableCurrentPassword
            | SettingsInput::PortableNewPassword
            | SettingsInput::PortableConfirmPassword => self.portable_action_error = None,
            SettingsInput::ManagedKeyFilePath
            | SettingsInput::ManagedKeyFileName
            | SettingsInput::ManagedKeyFilePassphrase
            | SettingsInput::ManagedKeyPasteName
            | SettingsInput::ManagedKeyPastePrivateKey
            | SettingsInput::ManagedKeyPastePassphrase
            | SettingsInput::ManagedKeyRenameName => self.managed_key_status = None,
            SettingsInput::NetworkProxyPassword => self.network_proxy_password_status = None,
            SettingsInput::NetworkProxyTestHost | SettingsInput::NetworkProxyTestPort => {
                self.network_proxy_test_result = None;
            }
            SettingsInput::LocalPrivilegeLabel
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns
            | SettingsInput::LocalPrivilegeHostPatterns
            | SettingsInput::LocalPrivilegeSecondFactorPatterns
            | SettingsInput::LocalPrivilegeSecondFactorUserHint => self.privilege_error = None,
            SettingsInput::TotpName
            | SettingsInput::TotpSecret
            | SettingsInput::TotpPattern
            | SettingsInput::TotpPeriod => self.totp_error = None,
            _ => {}
        }
    }

    fn settings_entity_input_mut(&mut self, input: SettingsInput) -> Option<&mut String> {
        if input.is_ai_mcp() {
            return self.ai_mcp.input_mut(input);
        }
        if crate::ai::providers::is_provider_draft(input) {
            return Some(&mut self.provider_input_draft);
        }
        if crate::highlight_page::is_highlight_input(input) {
            return Some(&mut self.highlight.input_draft);
        }
        if crate::terminal_triggers::is_terminal_trigger_input(input) {
            return Some(&mut self.trigger_input_draft);
        }
        match input {
            SettingsInput::TerminalCommandSpecsJson => {
                self.command_specs.as_mut().map(|draft| &mut *draft.value)
            }
            SettingsInput::SettingsSearch => Some(&mut self.settings_search_query),
            SettingsInput::CustomThemeName => self
                .theme_editor
                .as_mut()
                .map(Arc::make_mut)
                .map(|editor| &mut editor.name),
            SettingsInput::CustomThemeTerminalColor(index) => self
                .theme_editor
                .as_mut()
                .map(Arc::make_mut)
                .and_then(|editor| editor.terminal_colors.get_mut(index)),
            SettingsInput::CustomThemeUiColor(index) => self
                .theme_editor
                .as_mut()
                .map(Arc::make_mut)
                .and_then(|editor| editor.ui_colors.get_mut(index)),
            SettingsInput::KeybindingSearch => Some(&mut self.shortcuts.keybinding_search_query),
            SettingsInput::SkillSearch => Some(&mut self.skill_search_query),
            SettingsInput::PortableCurrentPassword => Some(&mut self.portable_current_password),
            SettingsInput::PortableNewPassword => Some(&mut self.portable_new_password),
            SettingsInput::PortableConfirmPassword => Some(&mut self.portable_confirm_password),
            SettingsInput::DiscoverySecret(index) => self.discovery.editor.as_mut()?.fields.get_mut(index)?.secret_value_mut(),
            SettingsInput::DiscoveryField(index) | SettingsInput::DiscoveryMultiline(index) => self
                .discovery
                .editor
                .as_mut()?
                .fields
                .get_mut(index)
                .map(|field| &mut field.value),
            SettingsInput::ManagedKeyFilePath => Some(&mut self.managed_key_file_path),
            SettingsInput::ManagedKeyFileName => Some(&mut self.managed_key_file_name),
            SettingsInput::ManagedKeyFilePassphrase => Some(&mut self.managed_key_file_passphrase),
            SettingsInput::ManagedKeyPasteName => Some(&mut self.managed_key_paste_name),
            SettingsInput::ManagedKeyPastePrivateKey => {
                Some(&mut self.managed_key_paste_private_key)
            }
            SettingsInput::ManagedKeyPastePassphrase => {
                Some(&mut self.managed_key_paste_passphrase)
            }
            SettingsInput::ManagedKeyRenameName => Some(&mut self.managed_key_rename_name),
            SettingsInput::NetworkProxyPassword => Some(&mut self.network_proxy_password),
            SettingsInput::NetworkProxyTestHost => Some(&mut self.network_proxy_test_host),
            SettingsInput::NetworkProxyTestPort => Some(&mut self.network_proxy_test_port),
            SettingsInput::LocalPrivilegeLabel => Some(&mut self.privilege_draft.label),
            SettingsInput::TotpName => self.totp_draft.as_mut().map(|draft| &mut draft.name),
            SettingsInput::TotpSecret => self.totp_draft.as_mut().map(|draft| &mut *draft.secret),
            SettingsInput::TotpPattern => self.totp_draft.as_mut().map(|draft| &mut draft.pattern),
            SettingsInput::TotpPeriod => self.totp_draft.as_mut().map(|draft| &mut draft.period),
            SettingsInput::LocalPrivilegeUsernameHint => {
                Some(&mut self.privilege_draft.username_hint)
            }
            SettingsInput::LocalPrivilegeSecret => Some(&mut self.privilege_draft.secret),
            SettingsInput::LocalPrivilegePromptPatterns => {
                Some(&mut self.privilege_draft.prompt_patterns)
            }
            SettingsInput::LocalPrivilegeHostPatterns => {
                Some(&mut self.privilege_draft.host_patterns)
            }
            SettingsInput::LocalPrivilegeSecondFactorPatterns => {
                Some(&mut self.privilege_draft.second_factor_patterns)
            }
            SettingsInput::LocalPrivilegeSecondFactorUserHint => {
                Some(&mut self.privilege_draft.second_factor_user_hint)
            }
            SettingsInput::ConnectionImportTargetGroup => {
                Some(&mut self.connection_import_target_group)
            }
            SettingsInput::KnownHostsSearch => Some(&mut self.known_hosts.query),
            _ => None,
        }
    }
}

impl Drop for SettingsWorkspaceEntity {
    fn drop(&mut self) {
        if let Some(abort) = self.network_proxy_test_abort.take() {
            abort.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::{Duration, SystemTime},
    };

    use gpui::{AppContext, TestAppContext};
    use oxideterm_settings::PersistedSettings;
    use oxideterm_settings_model::{SettingsTab, ThemeTarget, theme_editor_from_settings};

    use super::{
        ExternalStoreChange, ExternalStoreWatch, KeybindingFileOperationResult, LaunchAtLoginError,
        SettingsWorkspaceEntity, ThemeEditorOperationResult,
    };

    struct DropSignal(Arc<AtomicBool>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[test]
    fn external_store_watch_advances_before_publishing_each_change() {
        let initial = SystemTime::UNIX_EPOCH;
        let changed = initial + Duration::from_secs(1);
        let mut watch = ExternalStoreWatch {
            settings_path: PathBuf::new(),
            connections_path: PathBuf::new(),
            settings_modified: Some(initial),
            connections_modified: Some(initial),
        };

        assert_eq!(
            watch.take_change_for_modified_times(Some(changed), Some(initial)),
            Some(ExternalStoreChange {
                settings: true,
                connections: false
            })
        );
        assert_eq!(
            watch.take_change_for_modified_times(Some(changed), Some(initial)),
            None
        );
        assert_eq!(
            watch.take_change_for_modified_times(Some(changed), Some(changed)),
            Some(ExternalStoreChange {
                settings: false,
                connections: true
            })
        );
    }

    #[gpui::test]
    fn hidden_settings_page_keeps_single_flight_worker_completion_exact_once(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("visibility test runtime"),
        );
        let worker_completions = Arc::new(AtomicUsize::new(0));
        let worker_completions_for_task = Arc::clone(&worker_completions);
        let (worker_release_tx, worker_release_rx) = std::sync::mpsc::sync_channel(1);
        let (worker_done_tx, worker_done_rx) = std::sync::mpsc::sync_channel(1);
        entity.update(cx, |entity, cx| {
            entity.set_active_tab(SettingsTab::Portable, cx);
            assert!(entity.start_portable_status_refresh(
                true,
                Arc::clone(&runtime),
                move || {
                    worker_release_rx
                        .recv()
                        .expect("worker release sender should remain alive");
                    worker_completions_for_task.fetch_add(1, Ordering::AcqRel);
                    worker_done_tx
                        .send(())
                        .expect("worker completion receiver should remain alive");
                    super::PortableStatusRefresh {
                        status: Err("portable unavailable while hidden".to_string()),
                        exportable_secret_count: 2,
                    }
                },
                cx,
            ));
            assert!(!entity.start_portable_status_refresh(
                false,
                runtime,
                || unreachable!("single-flight worker"),
                cx,
            ));
            // The worker result remains lifecycle-significant after the page hides.
            entity.set_active_tab(SettingsTab::Help, cx);
        });
        cx.executor().allow_parking();
        cx.run_until_parked();
        worker_release_tx
            .send(())
            .expect("portable worker should remain alive while hidden");
        worker_done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("portable worker should finish after release");
        let worker_delivery_deadline = std::time::Instant::now() + Duration::from_secs(2);
        while entity.read_with(cx, |entity, _cx| entity.portable_refresh_pending) {
            assert!(
                std::time::Instant::now() < worker_delivery_deadline,
                "portable worker completion should reach the Entity while hidden"
            );
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(1));
        }
        entity.read_with(cx, |entity, _cx| {
            let snapshot = entity.portable_status_snapshot();
            assert_eq!(worker_completions.load(Ordering::Acquire), 1);
            assert!(!snapshot.refresh_pending);
            assert_eq!(
                snapshot.error.as_deref(),
                Some("portable unavailable while hidden")
            );
            assert_eq!(snapshot.exportable_secret_count, Some(2));
            assert!(entity.portable_refresh_task.is_none());
        });
    }

    #[gpui::test]
    fn external_store_watch_continues_across_hidden_settings_routes(cx: &mut TestAppContext) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        let mut events = cx.events(&entity);
        entity.update(cx, |entity, cx| {
            entity.start_external_store_watch(PathBuf::new(), PathBuf::new(), cx);
            entity.set_active_tab(SettingsTab::Help, cx);
        });
        // A retained long-running watch parks between ticks instead of ending
        // when no Settings page is currently mounted.
        cx.executor().allow_parking();
        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            entity
                .external_store_watch
                .as_mut()
                .expect("external watch state")
                .settings_modified = Some(SystemTime::UNIX_EPOCH);
        });
        cx.executor()
            .advance_clock(super::EXTERNAL_STORE_WATCH_INTERVAL + Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(
            events.try_recv().expect("settings file change event"),
            super::SettingsWorkspaceEvent::ExternalStoresChanged(ExternalStoreChange {
                settings: true,
                connections: false,
            })
        );
        assert!(events.try_recv().is_err());

        entity.update(cx, |entity, _cx| {
            entity
                .external_store_watch
                .as_mut()
                .expect("external watch state")
                .connections_modified = Some(SystemTime::UNIX_EPOCH);
        });
        cx.executor()
            .advance_clock(super::EXTERNAL_STORE_WATCH_INTERVAL + Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(
            events.try_recv().expect("connections file change event"),
            super::SettingsWorkspaceEvent::ExternalStoresChanged(ExternalStoreChange {
                settings: false,
                connections: true,
            })
        );
        assert!(events.try_recv().is_err());
        entity.read_with(cx, |entity, _cx| {
            assert_eq!(entity.route_snapshot().active_tab, SettingsTab::Help);
            assert!(entity.external_store_watch_task.is_some());
        });

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();
    }

    #[gpui::test]
    fn launch_at_login_replacement_completion_and_release_preserve_task_ownership(
        cx: &mut TestAppContext,
    ) {
        let first_dropped = Arc::new(AtomicBool::new(false));
        let entity = cx.new(SettingsWorkspaceEntity::new);
        let first_generation = entity.update(cx, |entity, cx| {
            let first_dropped_for_future = Arc::clone(&first_dropped);
            entity.start_launch_at_login_operation(
                async move {
                    let _signal = DropSignal(first_dropped_for_future);
                    std::future::pending::<Result<bool, LaunchAtLoginError>>().await
                },
                cx,
            )
        });
        cx.run_until_parked();

        let second_generation = entity.update(cx, |entity, cx| {
            entity.start_launch_at_login_operation(
                std::future::ready(Ok::<bool, LaunchAtLoginError>(true)),
                cx,
            )
        });
        cx.run_until_parked();

        assert_ne!(first_generation, second_generation);
        assert!(first_dropped.load(Ordering::Acquire));
        entity.update(cx, |entity, cx| {
            let snapshot = entity.launch_at_login_snapshot();
            assert!(snapshot.enabled);
            assert!(!snapshot.pending);
            assert_eq!(snapshot.error, None);

            entity.finish_launch_at_login_operation(
                first_generation,
                Err(LaunchAtLoginError::OperationFailed(Arc::from(
                    "stale result",
                ))),
                cx,
            );
            assert_eq!(
                entity.launch_at_login_snapshot(),
                super::LaunchAtLoginSnapshot {
                    enabled: true,
                    pending: false,
                    error: None,
                }
            );
        });

        let dropped = Arc::new(AtomicBool::new(false));
        entity.update(cx, |entity, cx| {
            let dropped_for_future = Arc::clone(&dropped);
            entity.start_launch_at_login_operation(
                async move {
                    let _signal = DropSignal(dropped_for_future);
                    std::future::pending::<Result<bool, LaunchAtLoginError>>().await
                },
                cx,
            );
        });
        cx.run_until_parked();

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(
            dropped.load(Ordering::Acquire),
            "release cancels the current launch-at-login task"
        );
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    fn keybinding_file_replacement_completion_and_release_preserve_task_ownership(
        cx: &mut TestAppContext,
    ) {
        let runtime = tokio::runtime::Runtime::new().expect("create keybinding file runtime");
        let first_dropped = Arc::new(AtomicBool::new(false));
        let entity = cx.new(SettingsWorkspaceEntity::new);
        let first_generation = entity.update(cx, |entity, cx| {
            let first_dropped_for_future = Arc::clone(&first_dropped);
            entity.start_keybinding_export(
                async move {
                    let _signal = DropSignal(first_dropped_for_future);
                    std::future::pending::<Option<PathBuf>>().await
                },
                serde_json::Map::new(),
                runtime.handle().clone(),
                cx,
            )
        });
        cx.run_until_parked();
        let second_generation = entity.update(cx, |entity, cx| {
            let generation = entity.start_keybinding_export(
                std::future::pending::<Option<PathBuf>>(),
                serde_json::Map::new(),
                runtime.handle().clone(),
                cx,
            );
            assert!(entity.keybinding_file_operation_task.is_some());
            generation
        });
        cx.run_until_parked();
        assert_ne!(first_generation, second_generation);
        assert!(first_dropped.load(Ordering::Acquire));

        entity.update(cx, |entity, cx| {
            entity.finish_keybinding_file_operation(
                first_generation,
                Some(KeybindingFileOperationResult::Exported),
                cx,
            );
            assert!(entity.keybinding_file_operation_results.is_empty());

            entity.finish_keybinding_file_operation(
                second_generation,
                Some(KeybindingFileOperationResult::ImportFailed),
                cx,
            );
            entity.finish_keybinding_file_operation(
                second_generation,
                Some(KeybindingFileOperationResult::Exported),
                cx,
            );
            let results = entity.take_keybinding_file_operation_results();
            assert_eq!(results.len(), 1);
            assert!(matches!(
                results.front(),
                Some(KeybindingFileOperationResult::ImportFailed)
            ));
        });

        let dropped = Arc::new(AtomicBool::new(false));
        entity.update(cx, |entity, cx| {
            let dropped_for_future = Arc::clone(&dropped);
            entity.start_keybinding_export(
                async move {
                    let _signal = DropSignal(dropped_for_future);
                    std::future::pending::<Option<PathBuf>>().await
                },
                serde_json::Map::new(),
                runtime.handle().clone(),
                cx,
            );
        });
        cx.run_until_parked();

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(
            dropped.load(Ordering::Acquire),
            "release cancels the current keybinding file task"
        );
    }

    #[gpui::test]
    fn theme_editor_reopen_cancels_stale_retained_exit(cx: &mut TestAppContext) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        entity.update(cx, |entity, cx| {
            entity.open_theme_editor(
                theme_editor_from_settings(
                    &PersistedSettings::default(),
                    ThemeTarget::Terminal,
                    None,
                    "First".to_string(),
                ),
                cx,
            );
            assert!(entity.cancel_theme_editor(Duration::from_secs(60), cx));
            assert!(entity.theme_editor_exit_task.is_some());
            assert_eq!(
                entity.theme_editor_phase(),
                oxideterm_gpui_ui::motion::ExitPhase::Exiting
            );

            entity.open_theme_editor(
                theme_editor_from_settings(
                    &PersistedSettings::default(),
                    ThemeTarget::Terminal,
                    None,
                    "Second".to_string(),
                ),
                cx,
            );
            assert!(entity.theme_editor_exit_task.is_none());
            assert_eq!(
                entity.theme_editor_phase(),
                oxideterm_gpui_ui::motion::ExitPhase::Visible
            );
            assert_eq!(
                entity.theme_editor().map(|editor| editor.name.as_str()),
                Some("Second")
            );
        });
    }

    #[gpui::test]
    fn theme_editor_background_cancel_discards_draft_and_save_delivers_it(cx: &mut TestAppContext) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        entity.update(cx, |entity, _| {
            entity.initialize_background_gallery(vec!["existing.webp".into()])
        });
        for save in [false, true] {
            entity.update(cx, |entity, cx| {
                entity.open_theme_editor(
                    theme_editor_from_settings(
                        &PersistedSettings::default(),
                        ThemeTarget::Terminal,
                        None,
                        "Mine".into(),
                    ),
                    cx,
                );
                entity.edit_theme_editor_background(
                    |background| {
                        background.image = Some("draft.webp".into());
                        background.opacity = 0.37;
                        background.alignment = oxideterm_settings::BackgroundAlignment::TopRight;
                    },
                    cx,
                );
                entity.add_background_pack_images(&[PathBuf::from("installed.mp4")], cx);
                entity.add_background_pack_images(&[PathBuf::from("installed.mp4")], cx);
                assert_eq!(
                    &*entity.background_images_snapshot(),
                    ["installed.mp4", "existing.webp"]
                );
                assert_eq!(
                    entity.theme_editor().unwrap().background.image.as_deref(),
                    Some("draft.webp")
                );
                entity.settings_focused_input =
                    Some(crate::SettingsInput::BackgroundEffectColor(0));
                if save {
                    entity.save_theme_editor(Duration::ZERO, cx);
                } else {
                    entity.cancel_theme_editor(Duration::ZERO, cx);
                }
                let mut results = entity.take_theme_editor_results();
                if save {
                    let Some(ThemeEditorOperationResult::Save(editor)) = results.pop_front() else {
                        panic!("save must deliver the complete appearance draft");
                    };
                    assert_eq!(editor.background.image.as_deref(), Some("draft.webp"));
                    assert_eq!(editor.background.opacity, 0.37);
                    assert_eq!(
                        editor.background.alignment,
                        oxideterm_settings::BackgroundAlignment::TopRight
                    );
                }
                assert!(
                    results.is_empty(),
                    "cancel must emit no persistence operation, and save must emit exactly one"
                );
                assert!(!entity.theme_editor_open());
                assert!(
                    entity.settings_focused_input.is_none(),
                    "closing the editor must release its background input focus"
                );
            });
        }
    }
}
