use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use gpui::{Context, EventEmitter, KeyDownEvent, Task, Timer};
use oxideterm_connections::{
    ConnectionImportDuplicateStrategy, ConnectionImportPreview, ConnectionImportSource,
    PrivilegeCredentialKind,
};
use oxideterm_gpui_settings_view::{SettingsInput, SettingsKeybindingScopeFilter};
use oxideterm_gpui_ui::confirm::ConfirmDialogAction;
use oxideterm_settings_model::{
    AiSettingsPage, SettingsNavigationLayout, SettingsTab, TerminalSettingsPage,
    ThemeEditorSection, ThemeEditorState, app_ui_colors_to_colors, editor_terminal_theme,
    terminal_theme_to_colors,
};
use oxideterm_theme::{derive_ui_colors_from_terminal, theme_by_id};
use zeroize::Zeroizing;

use crate::workspace::browser_behavior;

use super::update::NativeUpdateRuntime;
use super::{
    CliCompanionStatus, PortableSettingsAction, PortableSettingsDialog, SettingsManagedKeyDialog,
};

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
        let settings_modified = super::settings_store_modified_time(&settings_path);
        let connections_modified = super::settings_store_modified_time(&connections_path);
        Self {
            settings_path,
            connections_path,
            settings_modified,
            connections_modified,
        }
    }

    fn refresh_observed_state(&mut self) {
        self.settings_modified = super::settings_store_modified_time(&self.settings_path);
        self.connections_modified = super::settings_store_modified_time(&self.connections_path);
    }

    fn take_external_change(&mut self) -> bool {
        let settings_modified = super::settings_store_modified_time(&self.settings_path);
        let connections_modified = super::settings_store_modified_time(&self.connections_path);
        self.take_change_for_modified_times(settings_modified, connections_modified)
    }

    fn take_change_for_modified_times(
        &mut self,
        settings_modified: Option<SystemTime>,
        connections_modified: Option<SystemTime>,
    ) -> bool {
        let changed = settings_modified != self.settings_modified
            || connections_modified != self.connections_modified;
        if changed {
            // Advance before publishing so a failed reload does not create an event storm.
            self.settings_modified = settings_modified;
            self.connections_modified = connections_modified;
        }
        changed
    }
}

/// Non-secret result produced by the portable runtime status worker.
pub(in crate::workspace) struct PortableStatusRefresh {
    pub(in crate::workspace) status:
        Result<oxideterm_portable_runtime::PortableStatusSnapshot, String>,
    pub(in crate::workspace) exportable_secret_count: usize,
}

/// Read-only projection used after releasing the settings Entity borrow.
#[derive(Clone)]
pub(in crate::workspace) struct PortableStatusSnapshot {
    pub(in crate::workspace) status: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) exportable_secret_count: Option<usize>,
    pub(in crate::workspace) refresh_pending: bool,
}

pub(in crate::workspace) struct PortablePasswordDialogSnapshot {
    pub(in crate::workspace) open: bool,
    pub(in crate::workspace) pending: bool,
    pub(in crate::workspace) error: Option<String>,
    pub(in crate::workspace) current_password_present: bool,
    pub(in crate::workspace) presence: oxideterm_gpui_ui::motion::ExitPresence,
}

/// Copies only non-secret render state for the active managed-key dialog.
pub(in crate::workspace) enum ManagedKeyDialogSnapshot {
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

pub(in crate::workspace) struct NetworkProxyPasswordSnapshot {
    pub(in crate::workspace) password_present: bool,
    pub(in crate::workspace) password_status: Option<String>,
}

pub(in crate::workspace) struct NetworkProxyTestSnapshot {
    pub(in crate::workspace) test_host: String,
    pub(in crate::workspace) test_port: String,
    pub(in crate::workspace) test_pending: bool,
    pub(in crate::workspace) test_result: Option<Result<u128, String>>,
}

/// Editable privilege credential state with a zeroizing secret owner.
pub(in crate::workspace) struct PrivilegeCredentialDraft {
    pub(super) credential_id: Option<String>,
    pub(super) label: String,
    pub(super) kind: PrivilegeCredentialKind,
    pub(super) username_hint: String,
    pub(super) prompt_patterns: String,
    pub(super) secret: Zeroizing<String>,
    pub(super) enabled: bool,
}

impl Default for PrivilegeCredentialDraft {
    fn default() -> Self {
        Self {
            credential_id: None,
            label: String::new(),
            kind: PrivilegeCredentialKind::SudoPassword,
            username_hint: String::new(),
            prompt_patterns: String::new(),
            secret: Zeroizing::new(String::new()),
            enabled: true,
        }
    }
}

pub(in crate::workspace) struct PrivilegeCredentialSnapshot {
    pub(in crate::workspace) credential_id: Option<String>,
    pub(in crate::workspace) label: String,
    pub(in crate::workspace) kind: PrivilegeCredentialKind,
    pub(in crate::workspace) username_hint: String,
    pub(in crate::workspace) prompt_patterns: String,
    pub(in crate::workspace) enabled: bool,
    pub(in crate::workspace) error: Option<String>,
}

#[derive(Clone)]
pub(in crate::workspace) struct CliCompanionSnapshot {
    pub(in crate::workspace) status: Option<CliCompanionStatus>,
    pub(in crate::workspace) loading: bool,
    pub(in crate::workspace) error: Option<String>,
}

#[derive(Clone)]
pub(in crate::workspace) struct SshConfigImportSnapshot {
    pub(in crate::workspace) open: bool,
    pub(in crate::workspace) selected_hosts: HashSet<String>,
    pub(in crate::workspace) status: Option<String>,
    pub(in crate::workspace) presence: oxideterm_gpui_ui::motion::ExitPresence,
}

/// Read-only projection for the connection importer settings surface.
pub(in crate::workspace) struct ConnectionImportSnapshot {
    pub(in crate::workspace) source: ConnectionImportSource,
    pub(in crate::workspace) paths: Vec<String>,
    pub(in crate::workspace) preview: Option<ConnectionImportPreview>,
    pub(in crate::workspace) selected_draft_ids: HashSet<String>,
    pub(in crate::workspace) duplicate_strategy: ConnectionImportDuplicateStrategy,
    pub(in crate::workspace) status: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum CliCompanionOperation {
    Refresh,
    Install,
    Uninstall,
    UninstallLegacy,
    Migrate,
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::workspace) enum DataDirectoryConfirm {
    Conflict {
        path: PathBuf,
        files_found: Vec<String>,
    },
    Reset,
}

pub(in crate::workspace) enum DataDirectoryOperationResult {
    Changed,
    Reset,
    Failed(String),
}

pub(in crate::workspace) enum BackgroundGalleryOperationResult {
    Updated(Option<String>),
    Failed,
}

pub(in crate::workspace) enum ThemeImportResult {
    Imported {
        theme_id: String,
        name: String,
        value: serde_json::Value,
    },
    Failed(String),
}

/// Moves a completed theme editor action into the root persistence adapter.
pub(in crate::workspace) enum ThemeEditorOperationResult {
    Save(Arc<ThemeEditorState>),
    Delete(Arc<ThemeEditorState>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThemeEditorExitAction {
    Cancel,
    Save,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum KeybindingRecordingFooterAction {
    Confirm,
    Cancel,
}

const KEYBINDING_RECORDING_FOOTER_ACTIONS: [KeybindingRecordingFooterAction; 2] = [
    KeybindingRecordingFooterAction::Confirm,
    KeybindingRecordingFooterAction::Cancel,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum KeybindingRecordingKeyAction {
    Confirm,
    Handled,
}

/// Transfers the completed recording into the persistence/window adapter without cloning it.
pub(in crate::workspace) struct KeybindingRecordingCommit {
    pub(in crate::workspace) action_id: String,
    pub(in crate::workspace) combo: crate::keybindings::KeyCombo,
}

pub(in crate::workspace) enum KeybindingFileOperationResult {
    Exported,
    ExportFailed,
    Imported {
        overrides: serde_json::Map<String, serde_json::Value>,
        target_window: gpui::AnyWindowHandle,
    },
    ImportFailed,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::workspace) enum LaunchAtLoginError {
    #[cfg(not(target_os = "macos"))]
    ApprovalRequired,
    OperationFailed(Arc<str>),
    #[cfg(not(target_os = "macos"))]
    TaskFailed(Arc<str>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct LaunchAtLoginSnapshot {
    pub(in crate::workspace) enabled: bool,
    pub(in crate::workspace) pending: bool,
    pub(in crate::workspace) error: Option<LaunchAtLoginError>,
}

/// Converts managed gallery paths once at the filesystem boundary.
fn background_gallery_strings(settings_path: &Path) -> anyhow::Result<Vec<String>> {
    Ok(oxideterm_settings::list_background_images(settings_path)?
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

fn is_theme_editor_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::CustomThemeName
            | SettingsInput::CustomThemeTerminalColor(_)
            | SettingsInput::CustomThemeUiColor(_)
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct SettingsRouteSnapshot {
    pub(in crate::workspace) active_tab: SettingsTab,
    pub(in crate::workspace) terminal_page: TerminalSettingsPage,
    pub(in crate::workspace) previous_terminal_page: TerminalSettingsPage,
    pub(in crate::workspace) ai_page: AiSettingsPage,
    pub(in crate::workspace) previous_ai_page: AiSettingsPage,
}

/// Keeps settings route history and its navigation editor draft under one writer.
struct SettingsRouteState {
    active_tab: SettingsTab,
    terminal_page: TerminalSettingsPage,
    previous_terminal_page: TerminalSettingsPage,
    ai_page: AiSettingsPage,
    previous_ai_page: AiSettingsPage,
    navigation_draft: Option<Arc<SettingsNavigationLayout>>,
}

impl Default for SettingsRouteState {
    fn default() -> Self {
        Self {
            active_tab: SettingsTab::General,
            terminal_page: TerminalSettingsPage::Display,
            previous_terminal_page: TerminalSettingsPage::Display,
            ai_page: AiSettingsPage::General,
            previous_ai_page: AiSettingsPage::General,
            navigation_draft: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SettingsNavigationDraftAction {
    MoveTabBefore {
        tab: SettingsTab,
        target: SettingsTab,
    },
    MoveTabToGroupStart {
        tab: SettingsTab,
        group_index: usize,
    },
    MoveTabToGroupEnd {
        tab: SettingsTab,
        group_index: usize,
    },
    MoveGroupToPosition {
        source_index: usize,
        target_index: usize,
    },
    MoveGroupToEnd {
        source_index: usize,
    },
    RemoveEmptyGroup {
        group_index: usize,
    },
    AddGroup,
    AddGroupWithTab(SettingsTab),
    RestoreDefault,
}

/// Owns settings work that must complete independently from root rendering.
pub(in crate::workspace) struct SettingsWorkspaceEntity {
    route: SettingsRouteState,
    external_store_watch: Option<ExternalStoreWatch>,
    external_store_watch_task: Option<Task<()>>,
    portable_status: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    portable_status_error: Option<String>,
    portable_exportable_secret_count: Option<usize>,
    portable_refresh_pending: bool,
    portable_refresh_task: Option<Task<()>>,
    pub(super) portable_dialog: Option<PortableSettingsDialog>,
    pub(super) portable_action_pending: Option<PortableSettingsAction>,
    pub(super) portable_action_error: Option<String>,
    pub(super) portable_current_password: Zeroizing<String>,
    pub(super) portable_new_password: Zeroizing<String>,
    pub(super) portable_confirm_password: Zeroizing<String>,
    pub(super) settings_focused_input: Option<SettingsInput>,
    pub(super) portable_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) portable_dialog_exit_task: Option<Task<()>>,
    pub(super) portable_action_task: Option<Task<()>>,
    pub(super) managed_key_dialog: Option<SettingsManagedKeyDialog>,
    pub(super) managed_key_status: Option<String>,
    pub(super) managed_key_file_path: String,
    pub(super) managed_key_file_name: String,
    pub(super) managed_key_file_passphrase: Zeroizing<String>,
    pub(super) managed_key_paste_name: String,
    pub(super) managed_key_paste_private_key: Zeroizing<String>,
    pub(super) managed_key_paste_passphrase: Zeroizing<String>,
    pub(super) managed_key_rename_name: String,
    pub(super) managed_key_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) managed_key_dialog_exit_task: Option<Task<()>>,
    pub(super) managed_key_file_picker_task: Option<Task<()>>,
    pub(super) network_proxy_password: Zeroizing<String>,
    pub(super) network_proxy_password_status: Option<String>,
    pub(super) network_proxy_test_host: String,
    pub(super) network_proxy_test_port: String,
    pub(super) network_proxy_test_pending: bool,
    pub(super) network_proxy_test_result: Option<Result<u128, String>>,
    pub(super) network_proxy_test_task: Option<Task<()>>,
    pub(super) network_proxy_test_abort: Option<tokio::task::AbortHandle>,
    pub(super) privilege_draft: PrivilegeCredentialDraft,
    pub(super) privilege_error: Option<String>,
    pub(super) privilege_editor_open: bool,
    pub(super) privilege_scope_id: Option<String>,
    pub(super) cli_companion_status: Option<CliCompanionStatus>,
    pub(super) cli_companion_loading: bool,
    pub(super) cli_companion_error: Option<String>,
    pub(super) cli_companion_task: Option<Task<()>>,
    pub(super) ssh_config_import_dialog_open: bool,
    pub(super) ssh_config_selected_hosts: HashSet<String>,
    pub(super) connection_import_status: Option<String>,
    pub(super) connection_import_source: ConnectionImportSource,
    pub(super) connection_import_paths: Vec<String>,
    pub(super) connection_import_preview: Option<ConnectionImportPreview>,
    pub(super) selected_connection_import_drafts: HashSet<String>,
    pub(super) connection_import_duplicate_strategy: ConnectionImportDuplicateStrategy,
    pub(super) connection_import_target_group: String,
    pub(super) connection_import_path_picker_task: Option<Task<()>>,
    pub(super) ssh_config_import_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) ssh_config_import_dialog_exit_task: Option<Task<()>>,
    data_directory_confirm: Option<DataDirectoryConfirm>,
    data_directory_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    data_directory_picker_task: Option<Task<()>>,
    data_directory_confirm_exit_task: Option<Task<()>>,
    data_directory_results: VecDeque<DataDirectoryOperationResult>,
    background_blur_preview: Option<i64>,
    background_blur_commit_generation: u64,
    background_blur_commit_task: Option<Task<()>>,
    background_images: Arc<[String]>,
    background_gallery_task: Option<Task<()>>,
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
    keybinding_scope_filter: SettingsKeybindingScopeFilter,
    previous_keybinding_scope_filter: SettingsKeybindingScopeFilter,
    keybinding_search_query: String,
    keybinding_recording_action_id: Option<String>,
    keybinding_conflict_action_ids: Vec<String>,
    keybinding_recording_combo: Option<crate::keybindings::KeyCombo>,
    keybinding_recording_footer_focus: Option<KeybindingRecordingFooterAction>,
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
    pub(super) native_update: NativeUpdateRuntime,
}

#[derive(Clone, Copy)]
pub(in crate::workspace) struct KeybindingResetConfirmSnapshot {
    pub(in crate::workspace) phase: oxideterm_gpui_ui::motion::ExitPhase,
    pub(in crate::workspace) focused_action: Option<ConfirmDialogAction>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum KeybindingResetConfirmKeyAction {
    Cancel,
    Confirm,
    Handled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SettingsWorkspaceToast {
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SettingsWorkspaceEvent {
    ExternalStoresChanged,
    ResetNativeUpdateOverlay,
    ShowNativeUpdateNotification,
    ShowNativeUpdateToast(SettingsWorkspaceToast),
    RequestAutomaticNativeUpdateCheck,
    RequestQuitAfterNativeUpdate,
    DataDirectoryConfirmOpened,
    DataDirectoryOperationReady,
    BackgroundBlurCommitReady(i64),
    BackgroundGalleryOperationReady,
    ThemeImportReady,
    ThemeEditorOperationReady,
    KeybindingFileOperationReady,
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
    pub(in crate::workspace) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            route: SettingsRouteState::default(),
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
            portable_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            portable_dialog_exit_task: None,
            portable_action_task: None,
            managed_key_dialog: None,
            managed_key_status: None,
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
            network_proxy_password: Zeroizing::new(String::new()),
            network_proxy_password_status: None,
            network_proxy_test_host: String::new(),
            network_proxy_test_port: "22".to_string(),
            network_proxy_test_pending: false,
            network_proxy_test_result: None,
            network_proxy_test_task: None,
            network_proxy_test_abort: None,
            privilege_draft: PrivilegeCredentialDraft::default(),
            privilege_error: None,
            privilege_editor_open: false,
            privilege_scope_id: None,
            cli_companion_status: None,
            cli_companion_loading: false,
            cli_companion_error: None,
            cli_companion_task: None,
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
            background_blur_commit_generation: 0,
            background_blur_commit_task: None,
            background_images: Arc::from([]),
            background_gallery_task: None,
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
            keybinding_scope_filter: SettingsKeybindingScopeFilter::All,
            previous_keybinding_scope_filter: SettingsKeybindingScopeFilter::All,
            keybinding_search_query: String::new(),
            keybinding_recording_action_id: None,
            keybinding_conflict_action_ids: Vec::new(),
            keybinding_recording_combo: None,
            keybinding_recording_footer_focus: None,
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

    pub(in crate::workspace) fn route_snapshot(&self) -> SettingsRouteSnapshot {
        SettingsRouteSnapshot {
            active_tab: self.route.active_tab,
            terminal_page: self.route.terminal_page,
            previous_terminal_page: self.route.previous_terminal_page,
            ai_page: self.route.ai_page,
            previous_ai_page: self.route.previous_ai_page,
        }
    }

    pub(in crate::workspace) fn set_active_tab(
        &mut self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.route.active_tab == tab {
            return false;
        }
        self.route.active_tab = tab;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn set_terminal_page(
        &mut self,
        page: TerminalSettingsPage,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.route.terminal_page == page {
            return false;
        }
        self.route.previous_terminal_page = self.route.terminal_page;
        self.route.terminal_page = page;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn set_ai_page(
        &mut self,
        page: AiSettingsPage,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.route.ai_page == page {
            return false;
        }
        self.route.previous_ai_page = self.route.ai_page;
        self.route.ai_page = page;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn open_navigation_editor(
        &mut self,
        persisted_groups: &[Vec<String>],
        cx: &mut Context<Self>,
    ) {
        self.route.navigation_draft = Some(Arc::new(
            SettingsNavigationLayout::from_persisted_groups(persisted_groups),
        ));
        cx.notify();
    }

    pub(in crate::workspace) fn close_navigation_editor(&mut self, cx: &mut Context<Self>) {
        if self.route.navigation_draft.take().is_some() {
            cx.notify();
        }
    }

    /// Returns a shallow immutable render snapshot of the navigation draft.
    pub(in crate::workspace) fn navigation_draft_snapshot(
        &self,
    ) -> Option<Arc<SettingsNavigationLayout>> {
        self.route.navigation_draft.as_ref().map(Arc::clone)
    }

    pub(in crate::workspace) fn navigation_editor_open(&self) -> bool {
        self.route.navigation_draft.is_some()
    }

    pub(in crate::workspace) fn apply_navigation_draft_action(
        &mut self,
        action: SettingsNavigationDraftAction,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(layout) = self.route.navigation_draft.as_mut() else {
            return false;
        };
        let layout = Arc::make_mut(layout);
        let changed = match action {
            SettingsNavigationDraftAction::MoveTabBefore { tab, target } => {
                layout.move_tab_to_position(tab, target)
            }
            SettingsNavigationDraftAction::MoveTabToGroupStart { tab, group_index } => {
                layout.move_tab_to_group_start(tab, group_index)
            }
            SettingsNavigationDraftAction::MoveTabToGroupEnd { tab, group_index } => {
                layout.move_tab_to_group_end(tab, group_index)
            }
            SettingsNavigationDraftAction::MoveGroupToPosition {
                source_index,
                target_index,
            } => layout.move_group_to_position(source_index, target_index),
            SettingsNavigationDraftAction::MoveGroupToEnd { source_index } => {
                layout.move_group_to_end(source_index)
            }
            SettingsNavigationDraftAction::RemoveEmptyGroup { group_index } => {
                layout.remove_empty_group(group_index)
            }
            SettingsNavigationDraftAction::AddGroup => {
                layout.add_group();
                true
            }
            SettingsNavigationDraftAction::AddGroupWithTab(tab) => {
                layout.add_group();
                let destination_group = layout.group_count() - 1;
                // The group addition is itself a state change even if a
                // malformed draft no longer contains the requested page.
                let _ = layout.move_tab_to_group_end(tab, destination_group);
                true
            }
            SettingsNavigationDraftAction::RestoreDefault => {
                if layout.is_default() {
                    false
                } else {
                    *layout = SettingsNavigationLayout::default();
                    true
                }
            }
        };
        if changed {
            cx.notify();
        }
        changed
    }

    /// Closes the editor and serializes only the persistence payload.
    pub(in crate::workspace) fn take_navigation_editor_persisted_groups(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Vec<Vec<String>>> {
        let layout = self.route.navigation_draft.take()?;
        let serialized_groups = layout.to_persisted_groups();
        let default_groups = SettingsNavigationLayout::default().to_persisted_groups();
        // Empty groups are not persisted, so compare serialized forms before
        // choosing the "follow future product defaults" empty payload.
        let persisted_groups = if serialized_groups == default_groups {
            Vec::new()
        } else {
            serialized_groups
        };
        cx.notify();
        Some(persisted_groups)
    }

    pub(in crate::workspace) fn launch_at_login_snapshot(&self) -> LaunchAtLoginSnapshot {
        LaunchAtLoginSnapshot {
            enabled: self.launch_at_login_enabled,
            pending: self.launch_at_login_pending,
            error: self.launch_at_login_error.clone(),
        }
    }

    #[cfg(any(not(target_os = "macos"), test))]
    pub(in crate::workspace) fn start_launch_at_login_operation(
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
    pub(in crate::workspace) fn finish_launch_at_login_settings_handoff(
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

    pub(in crate::workspace) fn keybinding_scope_filter(&self) -> SettingsKeybindingScopeFilter {
        self.keybinding_scope_filter
    }

    pub(in crate::workspace) fn previous_keybinding_scope_filter(
        &self,
    ) -> SettingsKeybindingScopeFilter {
        self.previous_keybinding_scope_filter
    }

    pub(in crate::workspace) fn set_keybinding_scope_filter(
        &mut self,
        filter: SettingsKeybindingScopeFilter,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.keybinding_scope_filter == filter {
            return false;
        }
        self.previous_keybinding_scope_filter = self.keybinding_scope_filter;
        self.keybinding_scope_filter = filter;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn keybinding_search_query(&self) -> &str {
        &self.keybinding_search_query
    }

    pub(in crate::workspace) fn keybinding_recording_action_id(&self) -> Option<&str> {
        self.keybinding_recording_action_id.as_deref()
    }

    pub(in crate::workspace) fn keybinding_recording_combo(
        &self,
    ) -> Option<&crate::keybindings::KeyCombo> {
        self.keybinding_recording_combo.as_ref()
    }

    pub(in crate::workspace) fn keybinding_conflicts(&self) -> &[String] {
        &self.keybinding_conflict_action_ids
    }

    pub(in crate::workspace) fn keybinding_recording_footer_focus(
        &self,
    ) -> Option<KeybindingRecordingFooterAction> {
        self.keybinding_recording_footer_focus
    }

    pub(in crate::workspace) fn start_keybinding_recording(
        &mut self,
        action_id: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.keybinding_recording_action_id = Some(action_id.into());
        self.keybinding_conflict_action_ids.clear();
        self.keybinding_recording_combo = None;
        self.keybinding_recording_footer_focus = None;
        cx.notify();
    }

    pub(in crate::workspace) fn stop_keybinding_recording(&mut self, cx: &mut Context<Self>) {
        let changed = self.keybinding_recording_action_id.take().is_some()
            || !self.keybinding_conflict_action_ids.is_empty()
            || self.keybinding_recording_combo.take().is_some()
            || self.keybinding_recording_footer_focus.take().is_some();
        self.keybinding_conflict_action_ids.clear();
        if changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn handle_keybinding_recording_key(
        &mut self,
        event: &KeyDownEvent,
        overrides: &serde_json::Map<String, serde_json::Value>,
        cx: &mut Context<Self>,
    ) -> Option<KeybindingRecordingKeyAction> {
        if self.keybinding_recording_action_id.is_none() {
            return None;
        }
        if event.keystroke.key.as_str() == "escape"
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
        {
            self.stop_keybinding_recording(cx);
            return Some(KeybindingRecordingKeyAction::Handled);
        }

        if self.keybinding_recording_combo.is_some()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
        {
            match browser_behavior::modal_footer_key_action(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                &KEYBINDING_RECORDING_FOOTER_ACTIONS,
                self.keybinding_recording_footer_focus,
                KeybindingRecordingFooterAction::Confirm,
            ) {
                Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                    self.stop_keybinding_recording(cx);
                    return Some(KeybindingRecordingKeyAction::Handled);
                }
                Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                    // Native captures keydown globally, so the Entity mirrors
                    // the browser footer focus contract after a combo exists.
                    self.keybinding_recording_footer_focus = Some(action);
                    cx.notify();
                    return Some(KeybindingRecordingKeyAction::Handled);
                }
                Some(browser_behavior::ModalFooterKeyAction::Activate(
                    KeybindingRecordingFooterAction::Confirm,
                )) => {
                    self.keybinding_recording_footer_focus = None;
                    return Some(KeybindingRecordingKeyAction::Confirm);
                }
                Some(browser_behavior::ModalFooterKeyAction::Activate(
                    KeybindingRecordingFooterAction::Cancel,
                )) => {
                    self.stop_keybinding_recording(cx);
                    return Some(KeybindingRecordingKeyAction::Handled);
                }
                None => {}
            }
        }

        let action_id = self
            .keybinding_recording_action_id
            .as_deref()
            .expect("recording presence checked above");
        let combo = crate::keybindings::combo_from_keystroke(&event.keystroke)?;
        let side = crate::keybindings::KeybindingSide::current();
        self.keybinding_conflict_action_ids =
            crate::keybindings::conflicts_for_combo(action_id, &combo, overrides, side)
                .into_iter()
                .map(|definition| definition.id.to_string())
                .collect();
        self.keybinding_recording_combo = Some(combo);
        self.keybinding_recording_footer_focus = None;
        cx.notify();
        Some(KeybindingRecordingKeyAction::Handled)
    }

    pub(in crate::workspace) fn activate_keybinding_recording_footer(
        &mut self,
        action: KeybindingRecordingFooterAction,
        cx: &mut Context<Self>,
    ) -> bool {
        self.keybinding_recording_footer_focus = None;
        match action {
            KeybindingRecordingFooterAction::Confirm => true,
            KeybindingRecordingFooterAction::Cancel => {
                self.stop_keybinding_recording(cx);
                false
            }
        }
    }

    pub(in crate::workspace) fn take_keybinding_recording_commit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<KeybindingRecordingCommit> {
        let action_id = self.keybinding_recording_action_id.take()?;
        let Some(combo) = self.keybinding_recording_combo.take() else {
            self.keybinding_recording_action_id = Some(action_id);
            return None;
        };
        self.keybinding_conflict_action_ids.clear();
        self.keybinding_recording_footer_focus = None;
        cx.notify();
        Some(KeybindingRecordingCommit { action_id, combo })
    }

    pub(in crate::workspace) fn open_keybinding_reset_confirm(&mut self, cx: &mut Context<Self>) {
        self.keybinding_reset_confirm_exit_task = None;
        self.keybinding_reset_confirm_open = true;
        self.keybinding_reset_confirm_presence.reopen();
        self.keybinding_reset_confirm_focused_action = None;
        cx.notify();
    }

    pub(in crate::workspace) fn keybinding_reset_confirm_snapshot(
        &self,
    ) -> Option<KeybindingResetConfirmSnapshot> {
        self.keybinding_reset_confirm_open
            .then_some(KeybindingResetConfirmSnapshot {
                phase: self.keybinding_reset_confirm_presence.phase(),
                focused_action: self.keybinding_reset_confirm_focused_action,
            })
    }

    pub(in crate::workspace) fn begin_keybinding_reset_confirm_exit(
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
            Timer::after(delay).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_keybinding_reset_confirm_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    pub(in crate::workspace) fn handle_keybinding_reset_confirm_key(
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

    pub(in crate::workspace) fn start_external_store_watch(
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
                        if watch.take_external_change() {
                            cx.emit(SettingsWorkspaceEvent::ExternalStoresChanged);
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

    pub(in crate::workspace) fn acknowledge_external_store_state(&mut self) {
        if let Some(watch) = self.external_store_watch.as_mut() {
            watch.refresh_observed_state();
        }
    }

    pub(in crate::workspace) fn data_directory_confirm(&self) -> Option<&DataDirectoryConfirm> {
        self.data_directory_confirm.as_ref()
    }

    pub(in crate::workspace) fn data_directory_confirm_is_visible(&self) -> bool {
        self.data_directory_confirm.is_some()
            && self.data_directory_confirm_presence.phase()
                == oxideterm_gpui_ui::motion::ExitPhase::Visible
    }

    pub(in crate::workspace) fn data_directory_confirm_phase(
        &self,
    ) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.data_directory_confirm_presence.phase()
    }

    pub(in crate::workspace) fn open_data_directory_reset_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.data_directory_confirm_exit_task = None;
        self.data_directory_confirm_presence.reopen();
        self.data_directory_confirm = Some(DataDirectoryConfirm::Reset);
        cx.emit(SettingsWorkspaceEvent::DataDirectoryConfirmOpened);
        cx.notify();
    }

    pub(in crate::workspace) fn start_data_directory_picker(
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

    pub(in crate::workspace) fn begin_data_directory_confirm_exit(
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
            Timer::after(delay).await;
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

    pub(in crate::workspace) fn take_data_directory_results(
        &mut self,
    ) -> VecDeque<DataDirectoryOperationResult> {
        std::mem::take(&mut self.data_directory_results)
    }

    pub(in crate::workspace) fn background_blur_preview(&self) -> Option<i64> {
        self.background_blur_preview
    }

    pub(in crate::workspace) fn update_background_blur_preview(
        &mut self,
        persisted_value: i64,
        preview_value: i64,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_blur_preview == Some(preview_value)
            || (self.background_blur_preview.is_none() && persisted_value == preview_value)
        {
            return false;
        }

        self.background_blur_preview = Some(preview_value);
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
        cx.emit(SettingsWorkspaceEvent::BackgroundBlurCommitReady(value));
        cx.notify();
    }

    pub(in crate::workspace) fn initialize_background_gallery(&mut self, images: Vec<String>) {
        self.background_images = Arc::from(images);
    }

    pub(in crate::workspace) fn background_images_snapshot(&self) -> Arc<[String]> {
        Arc::clone(&self.background_images)
    }

    pub(in crate::workspace) fn start_background_image_import(
        &mut self,
        selection: impl std::future::Future<Output = Option<Vec<PathBuf>>> + 'static,
        settings_path: PathBuf,
        current_path: Option<PathBuf>,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_gallery_task.is_some() {
            return false;
        }

        self.background_gallery_task = Some(cx.spawn(async move |settings, cx| {
            let Some(paths) = selection.await else {
                let _ = settings.update(cx, |settings, cx| {
                    settings.background_gallery_task = None;
                    cx.notify();
                });
                return;
            };
            let source_paths = paths
                .into_iter()
                .filter(|path| oxideterm_settings::is_supported_background_image(path))
                .collect::<Vec<_>>();
            if source_paths.is_empty() {
                let _ = settings.update(cx, |settings, cx| {
                    settings.background_gallery_task = None;
                    cx.notify();
                });
                return;
            }

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

    pub(in crate::workspace) fn remove_background_image(
        &mut self,
        settings_path: PathBuf,
        image_path: String,
        current_path: Option<String>,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_gallery_task.is_some()
            || crate::workspace::is_bundled_workspace_background(
                &settings_path,
                Path::new(&image_path),
            )
        {
            return false;
        }

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

    pub(in crate::workspace) fn clear_background_image_gallery(
        &mut self,
        settings_path: PathBuf,
        current_path: Option<String>,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.background_gallery_task.is_some() {
            return false;
        }

        self.background_gallery_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime
                .spawn_blocking(move || -> anyhow::Result<(Vec<String>, Option<String>)> {
                    for image_path in oxideterm_settings::list_background_images(&settings_path)? {
                        if !crate::workspace::is_bundled_workspace_background(
                            &settings_path,
                            &image_path,
                        ) {
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
            Ok((gallery, active_path)) => {
                self.background_images = Arc::from(gallery);
                self.background_gallery_results
                    .push_back(BackgroundGalleryOperationResult::Updated(active_path));
            }
            Err(()) => self
                .background_gallery_results
                .push_back(BackgroundGalleryOperationResult::Failed),
        }
        cx.emit(SettingsWorkspaceEvent::BackgroundGalleryOperationReady);
        cx.notify();
    }

    pub(in crate::workspace) fn take_background_gallery_results(
        &mut self,
    ) -> VecDeque<BackgroundGalleryOperationResult> {
        std::mem::take(&mut self.background_gallery_results)
    }

    pub(in crate::workspace) fn start_theme_import(
        &mut self,
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

    pub(in crate::workspace) fn take_theme_import_results(
        &mut self,
    ) -> VecDeque<ThemeImportResult> {
        std::mem::take(&mut self.theme_import_results)
    }

    pub(in crate::workspace) fn theme_editor(&self) -> Option<&ThemeEditorState> {
        self.theme_editor.as_deref()
    }

    /// Shares one immutable render snapshot without copying the color arrays.
    pub(in crate::workspace) fn theme_editor_snapshot(&self) -> Option<Arc<ThemeEditorState>> {
        self.theme_editor.as_ref().map(Arc::clone)
    }

    pub(in crate::workspace) fn theme_editor_open(&self) -> bool {
        self.theme_editor.is_some()
    }

    pub(in crate::workspace) fn theme_editor_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.theme_editor_presence.phase()
    }

    pub(in crate::workspace) fn open_theme_editor(
        &mut self,
        editor: ThemeEditorState,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::workspace) fn select_theme_editor_section(
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

    pub(in crate::workspace) fn duplicate_theme_editor_from(
        &mut self,
        theme_id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.theme_editor.as_mut() else {
            return false;
        };
        let editor = Arc::make_mut(editor);
        let theme = theme_by_id(theme_id);
        editor.duplicate_theme.clear();
        editor.duplicate_theme.push_str(theme.id);
        editor.duplicate_theme_touched = true;
        editor.terminal_colors = terminal_theme_to_colors(theme.terminal);
        editor.ui_colors = app_ui_colors_to_colors(derive_ui_colors_from_terminal(theme.terminal));
        cx.notify();
        true
    }

    pub(in crate::workspace) fn derive_theme_editor_ui_colors(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.theme_editor.as_mut() else {
            return false;
        };
        let editor = Arc::make_mut(editor);
        let terminal = editor_terminal_theme(&editor.terminal_colors);
        editor.ui_colors = app_ui_colors_to_colors(derive_ui_colors_from_terminal(terminal));
        cx.notify();
        true
    }

    pub(in crate::workspace) fn cancel_theme_editor(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        self.begin_theme_editor_exit(ThemeEditorExitAction::Cancel, delay, cx)
    }

    pub(in crate::workspace) fn save_theme_editor(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .theme_editor
            .as_ref()
            .is_none_or(|editor| editor.name.trim().is_empty())
        {
            return false;
        }
        self.begin_theme_editor_exit(ThemeEditorExitAction::Save, delay, cx)
    }

    pub(in crate::workspace) fn delete_theme_editor(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
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
            Timer::after(delay).await;
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

    pub(in crate::workspace) fn take_theme_editor_results(
        &mut self,
    ) -> VecDeque<ThemeEditorOperationResult> {
        std::mem::take(&mut self.theme_editor_results)
    }

    pub(in crate::workspace) fn start_keybinding_export(
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

    pub(in crate::workspace) fn start_keybinding_import(
        &mut self,
        selection: impl std::future::Future<Output = Option<PathBuf>> + 'static,
        runtime: tokio::runtime::Handle,
        target_window: gpui::AnyWindowHandle,
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
                        .and_then(|value| {
                            crate::keybindings::sanitize_imported_overrides(value).map_err(|_| ())
                        })
                })
                .await
                .map_err(|_| ())
                .and_then(|result| result)
                .map(|overrides| KeybindingFileOperationResult::Imported {
                    overrides,
                    target_window,
                })
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
            self.keybinding_recording_action_id = None;
            self.keybinding_conflict_action_ids.clear();
            self.keybinding_recording_combo = None;
            self.keybinding_recording_footer_focus = None;
        }
        self.keybinding_file_operation_results.push_back(result);
        cx.emit(SettingsWorkspaceEvent::KeybindingFileOperationReady);
        cx.notify();
    }

    pub(in crate::workspace) fn take_keybinding_file_operation_results(
        &mut self,
    ) -> VecDeque<KeybindingFileOperationResult> {
        std::mem::take(&mut self.keybinding_file_operation_results)
    }

    pub(in crate::workspace) fn portable_status_snapshot(&self) -> PortableStatusSnapshot {
        PortableStatusSnapshot {
            status: self.portable_status.clone(),
            error: self.portable_status_error.clone(),
            exportable_secret_count: self.portable_exportable_secret_count,
            refresh_pending: self.portable_refresh_pending,
        }
    }

    pub(in crate::workspace) fn portable_mode(&self) -> Option<bool> {
        self.portable_status
            .as_ref()
            .map(|status| status.is_portable)
    }

    pub(in crate::workspace) fn start_portable_status_refresh(
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

    pub(in crate::workspace) fn invalidate_portable_status(&mut self, cx: &mut Context<Self>) {
        self.portable_status = None;
        self.portable_status_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn settings_entity_focused_input(&self) -> Option<SettingsInput> {
        self.settings_focused_input
    }

    pub(in crate::workspace) fn settings_search_open(&self) -> bool {
        self.settings_search_open
    }

    pub(in crate::workspace) fn settings_search_query(&self) -> &str {
        &self.settings_search_query
    }

    pub(in crate::workspace) fn open_settings_search(&mut self, cx: &mut Context<Self>) {
        self.settings_search_open = true;
        cx.notify();
    }

    pub(in crate::workspace) fn close_settings_search(
        &mut self,
        clear_query: bool,
        cx: &mut Context<Self>,
    ) -> bool {
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

    pub(in crate::workspace) fn clear_settings_search_query(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.settings_search_query.is_empty() {
            return false;
        }
        self.settings_search_query.clear();
        cx.notify();
        true
    }

    pub(in crate::workspace) fn settings_entity_input_value(
        &self,
        input: SettingsInput,
    ) -> Option<&str> {
        match input {
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
            SettingsInput::KeybindingSearch => Some(&self.keybinding_search_query),
            SettingsInput::PortableCurrentPassword => Some(&self.portable_current_password),
            SettingsInput::PortableNewPassword => Some(&self.portable_new_password),
            SettingsInput::PortableConfirmPassword => Some(&self.portable_confirm_password),
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
            SettingsInput::LocalPrivilegeUsernameHint => Some(&self.privilege_draft.username_hint),
            SettingsInput::LocalPrivilegeSecret => Some(&self.privilege_draft.secret),
            SettingsInput::LocalPrivilegePromptPatterns => {
                Some(&self.privilege_draft.prompt_patterns)
            }
            SettingsInput::ConnectionImportTargetGroup => {
                Some(&self.connection_import_target_group)
            }
            _ => None,
        }
    }

    pub(in crate::workspace) fn focus_settings_entity_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        let portable_open = self.portable_dialog == Some(PortableSettingsDialog::ChangePassword);
        let can_focus = match input {
            SettingsInput::SettingsSearch => self.settings_search_open,
            SettingsInput::CustomThemeName
            | SettingsInput::CustomThemeTerminalColor(_)
            | SettingsInput::CustomThemeUiColor(_) => self.theme_editor.is_some(),
            SettingsInput::KeybindingSearch => true,
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
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns => true,
            SettingsInput::ConnectionImportTargetGroup => true,
            _ => false,
        };
        if !can_focus {
            return false;
        }
        self.settings_focused_input = Some(input);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn blur_settings_entity_input(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.settings_focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn replace_settings_entity_input(
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
        cx.notify();
        true
    }

    pub(in crate::workspace) fn pop_settings_entity_input(
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
        cx.notify();
        true
    }

    fn clear_settings_entity_input_error(&mut self, input: SettingsInput) {
        match input {
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
            | SettingsInput::LocalPrivilegePromptPatterns => self.privilege_error = None,
            _ => {}
        }
    }

    fn settings_entity_input_mut(&mut self, input: SettingsInput) -> Option<&mut String> {
        match input {
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
            SettingsInput::KeybindingSearch => Some(&mut self.keybinding_search_query),
            SettingsInput::PortableCurrentPassword => Some(&mut self.portable_current_password),
            SettingsInput::PortableNewPassword => Some(&mut self.portable_new_password),
            SettingsInput::PortableConfirmPassword => Some(&mut self.portable_confirm_password),
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
            SettingsInput::LocalPrivilegeUsernameHint => {
                Some(&mut self.privilege_draft.username_hint)
            }
            SettingsInput::LocalPrivilegeSecret => Some(&mut self.privilege_draft.secret),
            SettingsInput::LocalPrivilegePromptPatterns => {
                Some(&mut self.privilege_draft.prompt_patterns)
            }
            SettingsInput::ConnectionImportTargetGroup => {
                Some(&mut self.connection_import_target_group)
            }
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
    use oxideterm_settings_model::{SettingsTab, theme_editor_from_settings};

    use super::{
        ExternalStoreWatch, KeybindingFileOperationResult, LaunchAtLoginError,
        SettingsWorkspaceEntity,
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

        assert!(watch.take_change_for_modified_times(Some(changed), Some(initial)));
        assert!(!watch.take_change_for_modified_times(Some(changed), Some(initial)));
        assert!(watch.take_change_for_modified_times(Some(changed), Some(changed)));
    }

    #[test]
    fn secret_render_projections_do_not_copy_entity_owned_plaintext() {
        let portable_source = include_str!("portable_runtime/actions.rs");
        let managed_key_source = include_str!("connections_page.rs");
        let proxy_source = include_str!("network_page.rs");

        for forbidden in [
            concat!("portable_current_password", ".to_string()"),
            concat!("portable_new_password", ".to_string()"),
            concat!("portable_confirm_password", ".to_string()"),
        ] {
            assert!(!portable_source.contains(forbidden), "{forbidden}");
        }
        for forbidden in [
            concat!(
                "file_passphrase: self.managed_key_file_passphrase",
                ".clone()"
            ),
            concat!(
                "private_key: self.managed_key_paste_private_key",
                ".clone()"
            ),
            concat!("passphrase: self.managed_key_paste_passphrase", ".clone()"),
        ] {
            assert!(!managed_key_source.contains(forbidden), "{forbidden}");
        }
        assert!(
            !proxy_source.contains(concat!("password: self.network_proxy_password", ".clone()"))
        );
    }

    #[test]
    fn workspace_drop_zeroizes_the_transient_settings_ime_draft() {
        let workspace_source = include_str!("../../workspace.rs");

        assert!(workspace_source.contains("impl Drop for WorkspaceApp"));
        assert!(
            workspace_source.contains("zeroize::Zeroize::zeroize(&mut self.settings_input_draft)")
        );
    }

    #[gpui::test]
    fn hidden_settings_page_keeps_worker_completion_exact_once(cx: &mut TestAppContext) {
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
                runtime,
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
                        exportable_secret_count: 0,
                    }
                },
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
            super::SettingsWorkspaceEvent::ExternalStoresChanged
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
            super::SettingsWorkspaceEvent::ExternalStoresChanged
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
    fn portable_status_refresh_is_single_flight_and_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("test runtime"),
        );

        entity.update(cx, |entity, cx| {
            assert!(entity.start_portable_status_refresh(
                false,
                runtime,
                || super::PortableStatusRefresh {
                    status: Err("unavailable".to_string()),
                    exportable_secret_count: 2,
                },
                cx,
            ));
            assert!(
                !entity.start_portable_status_refresh(
                    false,
                    Arc::new(
                        tokio::runtime::Builder::new_multi_thread()
                            .worker_threads(1)
                            .enable_all()
                            .build()
                            .expect("second test runtime"),
                    ),
                    || unreachable!("single-flight worker"),
                    cx,
                )
            );
            entity.portable_refresh_task = None;
            entity.finish_portable_status_refresh(
                Ok(super::PortableStatusRefresh {
                    status: Err("unavailable".to_string()),
                    exportable_secret_count: 2,
                }),
                cx,
            );
        });

        entity.update(cx, |entity, _cx| {
            let snapshot = entity.portable_status_snapshot();
            assert!(!snapshot.refresh_pending);
            assert_eq!(snapshot.error.as_deref(), Some("unavailable"));
            assert_eq!(snapshot.exportable_secret_count, Some(2));
        });
    }

    #[gpui::test]
    fn launch_at_login_replacement_and_late_completion_are_generation_safe(
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
    }

    #[gpui::test]
    fn settings_entity_release_cancels_launch_at_login_task(cx: &mut TestAppContext) {
        let dropped = Arc::new(AtomicBool::new(false));
        let entity = cx.new(SettingsWorkspaceEntity::new);
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

        assert!(dropped.load(Ordering::Acquire));
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    fn launch_at_login_macos_handoff_uses_typed_shallow_error_state(cx: &mut TestAppContext) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        entity.update(cx, |entity, cx| {
            entity.finish_launch_at_login_settings_handoff(
                Err("system settings unavailable".to_string()),
                cx,
            );
        });

        let first = entity.read_with(cx, |entity, _cx| entity.launch_at_login_snapshot());
        let second = entity.read_with(cx, |entity, _cx| entity.launch_at_login_snapshot());
        let (
            Some(LaunchAtLoginError::OperationFailed(first_error)),
            Some(LaunchAtLoginError::OperationFailed(second_error)),
        ) = (first.error, second.error)
        else {
            panic!("macOS handoff should retain a typed platform error");
        };
        // Render snapshots share the immutable message instead of copying it.
        assert!(Arc::ptr_eq(&first_error, &second_error));
    }

    #[gpui::test]
    fn keybinding_file_task_replacement_and_completion_are_generation_safe(
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
    }

    #[gpui::test]
    fn settings_entity_release_cancels_keybinding_file_task(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("create keybinding file runtime");
        let dropped = Arc::new(AtomicBool::new(false));
        let entity = cx.new(SettingsWorkspaceEntity::new);
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

        assert!(dropped.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn theme_editor_reopen_cancels_stale_retained_exit(cx: &mut TestAppContext) {
        let entity = cx.new(SettingsWorkspaceEntity::new);
        entity.update(cx, |entity, cx| {
            entity.open_theme_editor(
                theme_editor_from_settings(
                    &PersistedSettings::default(),
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
}
