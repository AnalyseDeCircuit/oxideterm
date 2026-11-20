// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use super::{
    batch_state::{BatchAddAuthKind, BatchAddDialogState},
    oxide_delivery,
    oxide_models::{OxideExportConnectionRow, OxideExportForwardGroupRow},
    *,
};
use gpui::{
    Context, EventEmitter, ListAlignment, ListState, Pixels, Point, Render, Rgba, Task, Window,
    div, prelude::*, px,
};
use oxideterm_connections::{
    SshConfigHost,
    oxide_file::{ExportPreflightResult, ImportConflictStrategy, ImportPreview, OxideMetadata},
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_forwarding::PersistedForward;
use oxideterm_gpui_ui::{
    icons::LucideIcon,
    virtual_list::{TauriVirtualListSpec, VirtualListSignatureCache, tauri_virtual_list_state},
};
use oxideterm_settings::{
    ALL_OXIDE_SETTINGS_SECTIONS as OXIDE_APP_SETTINGS_SECTIONS, DEFAULT_OXIDE_SETTINGS_SECTIONS,
};
use oxideterm_workspace::delivery;
use zeroize::Zeroizing;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionManagerOpenTarget {
    Tunnel(String),
    Plugin(String),
    Connection(String),
    SshConfig(String),
    Serial(String),
    Ble(String),
    LocalTerminal(String),
    Telnet(String),
    Ftp(String),
    Mosh(String),
    StandaloneSftp(String),
    RemoteDesktop(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SessionManagerInput {
    Search,
    GroupName,
    OxideImportPassword,
    OxideExportPassword,
    OxideExportConfirmPassword,
    OxideExportDescription,
    BatchPassword,
    BatchKeyPath,
    BatchTemplateName,
}

impl SessionManagerInput {
    pub fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::GroupName => 3,
            Self::OxideImportPassword => 4,
            Self::OxideExportPassword => 5,
            Self::OxideExportConfirmPassword => 6,
            Self::OxideExportDescription => 7,
            Self::BatchPassword => 8,
            Self::BatchKeyPath => 9,
            Self::BatchTemplateName => 10,
        }
    }

    pub fn is_secret(self) -> bool {
        matches!(
            self,
            Self::OxideImportPassword
                | Self::OxideExportPassword
                | Self::OxideExportConfirmPassword
                | Self::BatchPassword
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionManagerViewMode {
    Grid,
    List,
    Tree,
}

impl SessionManagerViewMode {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Grid => "sessionManager.views.grid",
            Self::List => "sessionManager.views.list",
            Self::Tree => "sessionManager.views.tree",
        }
    }

    pub fn icon(self) -> LucideIcon {
        match self {
            Self::Grid => LucideIcon::Layers,
            Self::List => LucideIcon::LayoutList,
            Self::Tree => LucideIcon::ListTree,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionSortField {
    Name,
    Host,
    Port,
    Username,
    AuthType,
    Group,
    LastUsed,
}

impl SessionSortField {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Name => "sessionManager.table.name",
            Self::Host => "sessionManager.table.host",
            Self::Port => "sessionManager.table.port",
            Self::Username => "sessionManager.table.username",
            Self::AuthType => "sessionManager.table.auth_type",
            Self::Group => "sessionManager.table.group",
            Self::LastUsed => "sessionManager.table.last_used",
        }
    }

    pub fn default_direction(self) -> SortDirection {
        match self {
            Self::LastUsed => SortDirection::Desc,
            _ => SortDirection::Asc,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub fn toggled(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    pub fn icon(self) -> LucideIcon {
        match self {
            Self::Asc => LucideIcon::ArrowUpAZ,
            Self::Desc => LucideIcon::ArrowDownAZ,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionManagerBasicDialogFooterAction {
    Cancel,
    Primary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionManagerGroupEditor {
    Create {
        parent_path: Option<String>,
    },
    Rename {
        old_path: String,
        parent_path: Option<String>,
    },
}

pub const SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS: [SessionManagerBasicDialogFooterAction; 2] = [
    SessionManagerBasicDialogFooterAction::Cancel,
    SessionManagerBasicDialogFooterAction::Primary,
];

pub enum SessionManagerWorkspaceEvent {
    OxideEffectsReady(oxide_delivery::OxideWorkspaceEffects),
    RefreshOxideExportPreflight,
    ModalBoundaryChanged,
}

#[derive(Clone, Debug)]
pub enum SessionManagerDeleteConfirm {
    Single {
        id: String,
        name: String,
    },
    SerialProfile {
        id: String,
        name: String,
    },
    BleProfile {
        id: String,
        name: String,
    },
    LocalTerminalProfile {
        id: String,
        name: String,
    },
    TelnetProfile {
        id: String,
        name: String,
    },
    MoshProfile {
        id: String,
        name: String,
    },
    StandaloneSftpProfile {
        id: String,
        name: String,
    },
    RemoteDesktopProfile {
        id: String,
        name: String,
    },
    Batch {
        targets: Vec<SessionManagerSelectionTarget>,
    },
    Group {
        name: String,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SessionManagerSelectionTarget {
    Tunnel(String),
    Plugin(String),
    Connection(String),
    Serial(String),
    Ble(String),
    LocalTerminal(String),
    Telnet(String),
    Ftp(String),
    Mosh(String),
    StandaloneSftp(String),
    RemoteDesktop(String),
}

#[derive(Clone)]
pub struct SessionManagerDrag {
    // The payload owns stable saved-asset identities so virtual rows may be recycled mid-drag.
    pub targets: Vec<SessionManagerSelectionTarget>,
    pub label: String,
    pub position: Point<Pixels>,
    pub background: Rgba,
    pub border: Rgba,
    pub text: Rgba,
}

impl SessionManagerDrag {
    pub fn with_position(&self, position: Point<Pixels>) -> Self {
        let mut preview = self.clone();
        preview.position = position;
        preview
    }
}

#[derive(Clone, Debug)]
pub enum SessionManagerRowActionTarget {
    Tunnel(String),
    Plugin(String),
    Connection(String),
    SshConfig(String),
    Serial(String),
    Ble(String),
    LocalTerminal(String),
    Telnet(String),
    Ftp(String),
    Mosh(String),
    StandaloneSftp(String),
    RemoteDesktop(String),
    GroupRoot,
    Group(String),
}

impl SessionManagerRowActionTarget {
    /// Maps a persisted row target to its open action. Group targets organize
    /// rows instead of opening, so they intentionally have no open target.
    pub fn open_target(&self) -> Option<SessionManagerOpenTarget> {
        match self {
            Self::Tunnel(id) => Some(SessionManagerOpenTarget::Tunnel(id.clone())),
            Self::Plugin(id) => Some(SessionManagerOpenTarget::Plugin(id.clone())),
            Self::Connection(id) => Some(SessionManagerOpenTarget::Connection(id.clone())),
            Self::SshConfig(alias) => Some(SessionManagerOpenTarget::SshConfig(alias.clone())),
            Self::Serial(id) => Some(SessionManagerOpenTarget::Serial(id.clone())),
            Self::Ble(id) => Some(SessionManagerOpenTarget::Ble(id.clone())),
            Self::LocalTerminal(id) => Some(SessionManagerOpenTarget::LocalTerminal(id.clone())),
            Self::Telnet(id) => Some(SessionManagerOpenTarget::Telnet(id.clone())),
            Self::Ftp(id) => Some(SessionManagerOpenTarget::Ftp(id.clone())),
            Self::Mosh(id) => Some(SessionManagerOpenTarget::Mosh(id.clone())),
            Self::StandaloneSftp(id) => Some(SessionManagerOpenTarget::StandaloneSftp(id.clone())),
            Self::RemoteDesktop(id) => Some(SessionManagerOpenTarget::RemoteDesktop(id.clone())),
            Self::GroupRoot | Self::Group(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionManagerRowActionMenuOrigin {
    ActionButton,
    Pointer,
}

#[derive(Clone, Debug)]
pub struct SessionManagerRowActionMenu {
    // Stable ids keep the floating menu independent from temporary row futures.
    pub target: SessionManagerRowActionTarget,
    pub origin: SessionManagerRowActionMenuOrigin,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Default)]
pub struct OxideImportResultView {
    pub imported: usize,
    pub skipped: usize,
    pub merged: usize,
    pub replaced: usize,
    pub renamed: usize,
    pub renames: Vec<(String, String)>,
    pub errors: Vec<String>,
    pub imported_forwards: usize,
    pub skipped_forwards: usize,
    pub imported_app_settings: bool,
    pub skipped_app_settings: bool,
    pub imported_quick_commands: usize,
    pub skipped_quick_commands: bool,
    pub imported_serial_profiles: usize,
    pub skipped_serial_profiles: usize,
    pub imported_telnet_profiles: usize,
    pub skipped_telnet_profiles: usize,
    pub imported_mosh_profiles: usize,
    pub skipped_mosh_profiles: usize,
    pub quick_commands_errors: Vec<String>,
    pub imported_plugin_settings: usize,
    pub skipped_plugin_settings: bool,
    pub imported_portable_secrets: usize,
    pub skipped_portable_secrets: usize,
}

#[derive(Clone, Debug)]
pub struct OxideTransferProgress {
    pub stage: String,
    pub current: usize,
    pub total: usize,
}

impl OxideTransferProgress {
    pub fn new(stage: impl Into<String>, current: usize, total: usize) -> Self {
        Self {
            stage: stage.into(),
            current,
            total,
        }
    }

    pub fn percent(&self) -> usize {
        (self.current.min(self.total) * 100)
            .checked_div(self.total)
            .unwrap_or(0)
            .min(100)
    }
}

pub struct SessionManagerState {
    pub selected_group: Option<String>,
    pub view_mode: SessionManagerViewMode,
    pub sort_field: SessionSortField,
    pub sort_direction: SortDirection,
    pub search_query: String,
    pub sidebar_search_open: bool,
    pub sidebar_sort_open: bool,
    pub selected_items: HashSet<SessionManagerSelectionTarget>,
    // The fixed endpoint of the last plain click, used by Shift+click range selection.
    pub selection_anchor: Option<SessionManagerSelectionTarget>,
    // Selectable rows in current render order. Range selection needs visual
    // adjacency, which `selected_items` (a HashSet) cannot provide.
    pub visible_selection_targets: Vec<SessionManagerSelectionTarget>,
    pub view_mode_menu_open: bool,
    pub sort_menu_open: bool,
    pub row_action_menu: Option<SessionManagerRowActionMenu>,
    pub expanded_groups: HashSet<String>,
    pub focused_input: Option<SessionManagerInput>,
    pub group_editor: Option<SessionManagerGroupEditor>,
    pub group_name_draft: String,
    pub group_editor_error: Option<String>,
    pub group_manager_error: Option<String>,
    pub show_group_manager: bool,
    pub reopen_group_manager_after_delete: bool,
    pub focused_basic_dialog_footer_action: Option<SessionManagerBasicDialogFooterAction>,
    pub show_batch_move: bool,
    pub delete_confirm: Option<SessionManagerDeleteConfirm>,
    pub oxide_import_dialog: Option<OxideImportDialogState>,
    pub oxide_export_dialog: Option<OxideExportDialogState>,
    pub status: Option<String>,
    pub ssh_config_hosts: Vec<SshConfigHost>,
    pub main_grid_list_state: ListState,
    pub main_grid_list_cache: RefCell<VirtualListSignatureCache>,
    pub main_list_state: ListState,
    pub main_list_cache: RefCell<VirtualListSignatureCache>,
    pub main_tree_list_state: ListState,
    pub main_tree_list_cache: RefCell<VirtualListSignatureCache>,
    // Separate list state for the sidebar session-manager tree so the tab and
    // sidebar can render simultaneously without fighting over scroll position.
    pub sidebar_tree_list_state: ListState,
    pub sidebar_tree_list_cache: RefCell<VirtualListSignatureCache>,
    pub oxide_export_connection_list_state: ListState,
    pub oxide_export_connection_list_cache: RefCell<VirtualListSignatureCache>,
    pub oxide_import_connection_preview_list_state: ListState,
    pub oxide_import_connection_preview_list_cache: RefCell<VirtualListSignatureCache>,
    pub oxide_export_forward_group_list_state: ListState,
    pub oxide_export_forward_group_list_cache: RefCell<VirtualListSignatureCache>,
    pub oxide_export_summary_line_list_state: ListState,
    pub oxide_export_summary_line_list_cache: RefCell<VirtualListSignatureCache>,
    pub oxide_import_forward_detail_list_state: ListState,
    pub oxide_import_forward_detail_list_cache: RefCell<VirtualListSignatureCache>,
    pub oxide_import_name_group_list_states: RefCell<HashMap<String, ListState>>,
    pub oxide_import_name_group_list_caches: RefCell<HashMap<String, VirtualListSignatureCache>>,
    pub import_dialog_exit_task: Option<Task<()>>,
    pub export_dialog_exit_task: Option<Task<()>>,
    pub dialog_auto_close_task: Option<Task<()>>,
    pub import_file_picker_task: Option<Task<()>>,
    pub export_file_picker_task: Option<Task<()>>,
    pub(super) oxide_worker_tx:
        Option<delivery::ActiveDeliverySender<oxide_delivery::OxideWorkerDelivery>>,
    pub(super) oxide_worker_rx:
        Option<std::sync::mpsc::Receiver<oxide_delivery::OxideWorkerDelivery>>,
    pub(super) oxide_delivery_task: Option<Task<()>>,
    pub(super) oxide_worker_threads:
        HashMap<oxide_delivery::OxideWorkerKey, std::thread::JoinHandle<()>>,
    ssh_config_load_generation: u64,
    ssh_config_load_task: Option<Task<()>>,
    pub batch_add: Option<BatchAddDialogState>,
    pub batch_add_exit_task: Option<Task<()>>,
}

impl Default for SessionManagerState {
    fn default() -> Self {
        Self {
            selected_group: None,
            // Group management is contextual in the tree, so make that capability discoverable.
            view_mode: SessionManagerViewMode::Tree,
            sort_field: SessionSortField::LastUsed,
            sort_direction: SortDirection::Desc,
            search_query: String::new(),
            sidebar_search_open: false,
            sidebar_sort_open: false,
            selected_items: HashSet::new(),
            selection_anchor: None,
            visible_selection_targets: Vec::new(),
            view_mode_menu_open: false,
            sort_menu_open: false,
            row_action_menu: None,
            expanded_groups: HashSet::new(),
            focused_input: None,
            group_editor: None,
            group_name_draft: String::new(),
            group_editor_error: None,
            group_manager_error: None,
            show_group_manager: false,
            reopen_group_manager_after_delete: false,
            focused_basic_dialog_footer_action: None,
            show_batch_move: false,
            delete_confirm: None,
            oxide_import_dialog: None,
            oxide_export_dialog: None,
            status: None,
            ssh_config_hosts: Vec::new(),
            batch_add: None,
            batch_add_exit_task: None,
            main_grid_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(MANAGER_GRID_ESTIMATED_ROW_HEIGHT),
                    MANAGER_MAIN_VIEW_OVERSCAN,
                ),
            ),
            main_grid_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            main_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(MANAGER_LIST_ESTIMATED_ROW_HEIGHT),
                    MANAGER_MAIN_VIEW_OVERSCAN,
                ),
            ),
            main_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            main_tree_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(MANAGER_TREE_ESTIMATED_ROW_HEIGHT),
                    MANAGER_MAIN_VIEW_OVERSCAN,
                ),
            ),
            main_tree_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            sidebar_tree_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(MANAGER_TREE_ESTIMATED_ROW_HEIGHT),
                    MANAGER_MAIN_VIEW_OVERSCAN,
                ),
            ),
            sidebar_tree_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            oxide_export_connection_list_state: ListState::new(
                OXIDE_EXPORT_CONNECTION_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_CONNECTION_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_CONNECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_connection_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            oxide_import_connection_preview_list_state: ListState::new(
                OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_ESTIMATED_HEIGHT),
                    OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_import_connection_preview_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            oxide_export_forward_group_list_state: ListState::new(
                OXIDE_EXPORT_FORWARD_GROUP_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_FORWARD_GROUP_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_FORWARD_GROUP_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_forward_group_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            oxide_export_summary_line_list_state: ListState::new(
                OXIDE_EXPORT_SUMMARY_LINE_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT),
                    OXIDE_EXPORT_SUMMARY_LINE_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_export_summary_line_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            oxide_import_forward_detail_list_state: ListState::new(
                OXIDE_IMPORT_FORWARD_DETAIL_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(OXIDE_IMPORT_FORWARD_DETAIL_LIST_ESTIMATED_HEIGHT),
                    OXIDE_IMPORT_FORWARD_DETAIL_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            oxide_import_forward_detail_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            oxide_import_name_group_list_states: RefCell::new(HashMap::new()),
            oxide_import_name_group_list_caches: RefCell::new(HashMap::new()),
            import_dialog_exit_task: None,
            export_dialog_exit_task: None,
            dialog_auto_close_task: None,
            import_file_picker_task: None,
            export_file_picker_task: None,
            oxide_worker_tx: None,
            oxide_worker_rx: None,
            oxide_delivery_task: None,
            oxide_worker_threads: HashMap::new(),
            ssh_config_load_generation: 0,
            ssh_config_load_task: None,
        }
    }
}

impl EventEmitter<SessionManagerWorkspaceEvent> for SessionManagerState {}

impl SessionManagerState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut state = Self::default();
        state.initialize_oxide_delivery(cx);
        state
    }

    pub fn set_status(&mut self, status: Option<String>, cx: &mut Context<Self>) {
        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }

    pub fn focused_input(&self) -> Option<SessionManagerInput> {
        self.focused_input
    }

    pub fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn input_value(&self, input: SessionManagerInput) -> Option<&str> {
        match input {
            SessionManagerInput::Search => Some(&self.search_query),
            SessionManagerInput::GroupName => Some(&self.group_name_draft),
            SessionManagerInput::OxideImportPassword => self
                .oxide_import_dialog
                .as_ref()
                .map(|dialog| dialog.password.as_str()),
            SessionManagerInput::OxideExportPassword => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.password.as_str()),
            SessionManagerInput::OxideExportConfirmPassword => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.confirm_password.as_str()),
            SessionManagerInput::OxideExportDescription => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.description.as_str()),
            SessionManagerInput::BatchPassword => self
                .batch_add
                .as_ref()
                .map(|dialog| dialog.password.as_str()),
            SessionManagerInput::BatchKeyPath => self
                .batch_add
                .as_ref()
                .map(|dialog| dialog.key_path.as_str()),
            SessionManagerInput::BatchTemplateName => self
                .batch_add
                .as_ref()
                .map(|dialog| dialog.rename_draft.as_str()),
        }
    }

    /// The key picker trigger only exists while the dialog authenticates with a
    /// key, so its overlay anchor is valid only in that mode.
    pub fn batch_add_uses_key_auth(&self) -> bool {
        self.batch_add
            .as_ref()
            .is_some_and(|dialog| dialog.auth_kind == BatchAddAuthKind::Key)
    }

    pub fn replace_input(
        &mut self,
        input: SessionManagerInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let value = match input {
            SessionManagerInput::Search => &mut self.search_query,
            SessionManagerInput::GroupName => {
                self.group_editor_error = None;
                &mut self.group_name_draft
            }
            SessionManagerInput::OxideImportPassword => {
                let Some(dialog) = self.oxide_import_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.password
            }
            SessionManagerInput::OxideExportPassword => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.password
            }
            SessionManagerInput::OxideExportConfirmPassword => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.confirm_password
            }
            SessionManagerInput::OxideExportDescription => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.description
            }
            SessionManagerInput::BatchPassword => {
                let Some(dialog) = self.batch_add.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.password
            }
            SessionManagerInput::BatchKeyPath => {
                let Some(dialog) = self.batch_add.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.key_path
            }
            SessionManagerInput::BatchTemplateName => {
                let Some(dialog) = self.batch_add.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.rename_draft
            }
        };
        replace_utf16(value, replacement_range, text);
        cx.notify();
        true
    }

    pub fn clear_ssh_config_hosts(&mut self, cx: &mut Context<Self>) {
        self.ssh_config_load_generation = self.ssh_config_load_generation.wrapping_add(1);
        self.ssh_config_load_task = None;
        if !self.ssh_config_hosts.is_empty() {
            self.ssh_config_hosts.clear();
            cx.notify();
        }
    }

    pub fn refresh_ssh_config_hosts(
        &mut self,
        enabled: bool,
        runtime: Arc<tokio::runtime::Runtime>,
        store: &oxideterm_connections::ConnectionStore,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) {
        if !enabled {
            self.clear_ssh_config_hosts(cx);
            return;
        }
        let existing_names = store
            .connections()
            .iter()
            .map(|saved| saved.name.clone())
            .collect();
        self.begin_ssh_config_host_load(
            runtime,
            existing_names,
            i18n.t("settings_view.connections.ssh_config.load_failed"),
            cx,
        );
    }

    pub fn import_ssh_config_host(
        &mut self,
        store: &mut oxideterm_connections::ConnectionStore,
        alias: &str,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        let (imported, status) = match oxideterm_connections::import_ssh_config_alias(store, alias)
        {
            Ok(true) => {
                self.remove_ssh_config_host_alias(alias, cx);
                (
                    true,
                    i18n.t("settings_view.errors.import_success")
                        .replace("{{name}}", alias),
                )
            }
            Ok(false) => (
                false,
                i18n.t("settings_view.connections.ssh_config.batch_import_skipped")
                    .replace("{{count}}", "1"),
            ),
            Err(error) => (
                false,
                i18n.t("settings_view.errors.import_failed")
                    .replace("{{error}}", &error.to_string()),
            ),
        };
        self.set_status(Some(status), cx);
        imported
    }

    fn begin_ssh_config_host_load(
        &mut self,
        runtime: Arc<tokio::runtime::Runtime>,
        existing_names: HashSet<String>,
        load_failed_template: String,
        cx: &mut Context<Self>,
    ) {
        self.ssh_config_load_generation = self.ssh_config_load_generation.wrapping_add(1);
        let generation = self.ssh_config_load_generation;
        // Entity lifetime owns discovery; closing its tab only hides the view.
        self.ssh_config_load_task = Some(cx.spawn(async move |entity, cx| {
            let result = runtime
                .spawn_blocking(move || {
                    oxideterm_connections::list_ssh_config_hosts(&existing_names)
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = entity.update(cx, |session_manager, cx| {
                if generation != session_manager.ssh_config_load_generation {
                    return;
                }
                match result {
                    Ok(hosts) => {
                        session_manager.ssh_config_hosts = hosts;
                    }
                    Err(error) => {
                        session_manager.ssh_config_hosts.clear();
                        session_manager.status =
                            Some(load_failed_template.replace("{{error}}", &error));
                    }
                }
                cx.notify();
            });
        }));
    }

    pub fn remove_ssh_config_host_alias(&mut self, alias: &str, cx: &mut Context<Self>) {
        let previous_count = self.ssh_config_hosts.len();
        self.ssh_config_hosts.retain(|host| host.alias != alias);
        if self.ssh_config_hosts.len() != previous_count {
            cx.notify();
        }
    }
}

pub struct OxideImportDialogState {
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub file_path: Option<PathBuf>,
    pub file_data: Option<Arc<[u8]>>,
    pub metadata_summary: Option<String>,
    pub metadata: Option<OxideMetadata>,
    pub password: Zeroizing<String>,
    pub conflict_strategy: ImportConflictStrategy,
    pub preview: Option<Arc<ImportPreview>>,
    pub selected_names: HashSet<String>,
    pub import_app_settings: bool,
    pub selected_app_settings_sections: HashSet<String>,
    pub expanded_app_settings_sections: HashSet<String>,
    pub import_quick_commands: bool,
    pub import_serial_profiles: bool,
    pub import_telnet_profiles: bool,
    pub import_mosh_profiles: bool,
    pub import_plugin_settings: bool,
    pub selected_plugin_ids: HashSet<String>,
    pub import_forwards: bool,
    pub import_portable_secrets: bool,
    pub restore_managed_keys: bool,
    pub restore_managed_key_passphrases: bool,
    pub busy: bool,
    pub operation_generation: u64,
    pub progress_stage: Option<OxideTransferProgress>,
    pub focused_footer_action: Option<OxideDialogFooterAction>,
    pub error: Option<String>,
    pub result_summary: Option<String>,
    pub result: Option<Arc<OxideImportResultView>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OxideDialogFooterAction {
    Cancel,
    Secondary,
    Primary,
}

impl Default for OxideImportDialogState {
    fn default() -> Self {
        Self {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            file_path: None,
            file_data: None,
            metadata_summary: None,
            metadata: None,
            password: Zeroizing::new(String::new()),
            conflict_strategy: ImportConflictStrategy::Rename,
            preview: None,
            selected_names: HashSet::new(),
            import_app_settings: true,
            selected_app_settings_sections: OXIDE_APP_SETTINGS_SECTIONS
                .iter()
                .map(|section| (*section).to_string())
                .collect(),
            expanded_app_settings_sections: HashSet::new(),
            import_quick_commands: true,
            import_serial_profiles: true,
            import_telnet_profiles: true,
            import_mosh_profiles: true,
            import_plugin_settings: true,
            selected_plugin_ids: HashSet::new(),
            import_forwards: true,
            import_portable_secrets: false,
            restore_managed_keys: true,
            restore_managed_key_passphrases: false,
            busy: false,
            operation_generation: 0,
            progress_stage: None,
            focused_footer_action: Some(OxideDialogFooterAction::Secondary),
            error: None,
            result_summary: None,
            result: None,
        }
    }
}

impl std::fmt::Debug for OxideImportDialogState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxideImportDialogState")
            .field("file_path", &self.file_path)
            .field("file_data", &self.file_data.as_ref().map(|data| data.len()))
            .field("metadata_summary", &self.metadata_summary)
            .field("metadata", &self.metadata)
            .field("password", &"[redacted secret]")
            .field("conflict_strategy", &self.conflict_strategy)
            .field("preview", &self.preview)
            .field("selected_names", &self.selected_names)
            .field("import_app_settings", &self.import_app_settings)
            .field(
                "selected_app_settings_sections",
                &self.selected_app_settings_sections,
            )
            .field(
                "expanded_app_settings_sections",
                &self.expanded_app_settings_sections,
            )
            .field("import_quick_commands", &self.import_quick_commands)
            .field("import_serial_profiles", &self.import_serial_profiles)
            .field("import_telnet_profiles", &self.import_telnet_profiles)
            .field("import_mosh_profiles", &self.import_mosh_profiles)
            .field("import_plugin_settings", &self.import_plugin_settings)
            .field("selected_plugin_ids", &self.selected_plugin_ids)
            .field("import_forwards", &self.import_forwards)
            .field("import_portable_secrets", &self.import_portable_secrets)
            .field("restore_managed_keys", &self.restore_managed_keys)
            .field(
                "restore_managed_key_passphrases",
                &self.restore_managed_key_passphrases,
            )
            .field("busy", &self.busy)
            .field("operation_generation", &self.operation_generation)
            .field("progress_stage", &self.progress_stage)
            .field("focused_footer_action", &self.focused_footer_action)
            .field("error", &self.error)
            .field("result_summary", &self.result_summary)
            .field("result", &self.result)
            .finish()
    }
}

pub struct OxideExportDialogState {
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub selected_ids: HashSet<String>,
    pub connection_rows: Arc<[OxideExportConnectionRow]>,
    pub forward_group_rows: Arc<[OxideExportForwardGroupRow]>,
    pub available_forwards: Vec<PersistedForward>,
    pub selected_forward_ids: HashSet<String>,
    pub include_app_settings: bool,
    pub selected_app_settings_sections: HashSet<String>,
    pub include_local_terminal_env_vars: bool,
    pub include_quick_commands: bool,
    pub include_serial_profiles: bool,
    pub include_telnet_profiles: bool,
    pub include_mosh_profiles: bool,
    pub include_remote_desktop_profiles: bool,
    pub include_ble_profiles: bool,
    pub include_plugin_settings: bool,
    pub plugin_groups: HashMap<String, usize>,
    pub selected_plugin_ids: HashSet<String>,
    pub include_forwards: bool,
    pub include_portable_secrets: bool,
    pub embed_keys: bool,
    pub include_passwords: bool,
    pub include_key_passphrases: bool,
    pub include_managed_keys: bool,
    pub include_managed_key_passphrases: bool,
    pub password: Zeroizing<String>,
    pub confirm_password: Zeroizing<String>,
    pub description: String,
    pub busy: bool,
    pub operation_generation: u64,
    pub progress_stage: Option<OxideTransferProgress>,
    pub focused_footer_action: Option<OxideDialogFooterAction>,
    pub last_export_timestamp: Option<i64>,
    pub preflight: Option<ExportPreflightResult>,
    pub error: Option<String>,
    pub result_summary: Option<String>,
}

impl Default for OxideExportDialogState {
    fn default() -> Self {
        Self {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            selected_ids: HashSet::new(),
            connection_rows: Arc::from([]),
            forward_group_rows: Arc::from([]),
            available_forwards: Vec::new(),
            selected_forward_ids: HashSet::new(),
            include_app_settings: true,
            selected_app_settings_sections: DEFAULT_OXIDE_SETTINGS_SECTIONS
                .iter()
                .map(|section| (*section).to_string())
                .collect(),
            include_local_terminal_env_vars: false,
            include_quick_commands: true,
            include_serial_profiles: true,
            include_telnet_profiles: true,
            include_mosh_profiles: true,
            include_remote_desktop_profiles: true,
            include_ble_profiles: true,
            include_plugin_settings: true,
            plugin_groups: HashMap::new(),
            selected_plugin_ids: HashSet::new(),
            include_forwards: true,
            include_portable_secrets: false,
            embed_keys: false,
            include_passwords: false,
            include_key_passphrases: true,
            include_managed_keys: true,
            include_managed_key_passphrases: false,
            password: Zeroizing::new(String::new()),
            confirm_password: Zeroizing::new(String::new()),
            description: String::new(),
            busy: false,
            operation_generation: 0,
            progress_stage: None,
            focused_footer_action: Some(OxideDialogFooterAction::Cancel),
            last_export_timestamp: None,
            preflight: None,
            error: None,
            result_summary: None,
        }
    }
}

impl std::fmt::Debug for OxideExportDialogState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OxideExportDialogState")
            .field("selected_ids", &self.selected_ids)
            .field("available_forwards", &self.available_forwards)
            .field("selected_forward_ids", &self.selected_forward_ids)
            .field("include_app_settings", &self.include_app_settings)
            .field(
                "selected_app_settings_sections",
                &self.selected_app_settings_sections,
            )
            .field(
                "include_local_terminal_env_vars",
                &self.include_local_terminal_env_vars,
            )
            .field("include_quick_commands", &self.include_quick_commands)
            .field("include_serial_profiles", &self.include_serial_profiles)
            .field("include_telnet_profiles", &self.include_telnet_profiles)
            .field("include_mosh_profiles", &self.include_mosh_profiles)
            .field(
                "include_remote_desktop_profiles",
                &self.include_remote_desktop_profiles,
            )
            .field("include_ble_profiles", &self.include_ble_profiles)
            .field("include_plugin_settings", &self.include_plugin_settings)
            .field("plugin_groups", &self.plugin_groups)
            .field("selected_plugin_ids", &self.selected_plugin_ids)
            .field("include_forwards", &self.include_forwards)
            .field("include_portable_secrets", &self.include_portable_secrets)
            .field("embed_keys", &self.embed_keys)
            .field("include_passwords", &self.include_passwords)
            .field("include_key_passphrases", &self.include_key_passphrases)
            .field("include_managed_keys", &self.include_managed_keys)
            .field(
                "include_managed_key_passphrases",
                &self.include_managed_key_passphrases,
            )
            .field("password", &"[redacted secret]")
            .field("confirm_password", &"[redacted secret]")
            .field("description", &self.description)
            .field("busy", &self.busy)
            .field("operation_generation", &self.operation_generation)
            .field("progress_stage", &self.progress_stage)
            .field("focused_footer_action", &self.focused_footer_action)
            .field("last_export_timestamp", &self.last_export_timestamp)
            .field("preflight", &self.preflight)
            .field("error", &self.error)
            .field("result_summary", &self.result_summary)
            .finish()
    }
}

// Keep the manager split by UI surface and behavior while preserving one workspace boundary.

impl Render for SessionManagerDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // The preview stays compact so the destination folder remains visible while dragging.
        div().pl(self.position.x).pt(self.position.y).child(
            div()
                .max_w(px(MANAGER_DRAG_PREVIEW_MAX_WIDTH))
                .px_3()
                .py_2()
                .rounded(px(MANAGER_DRAG_PREVIEW_RADIUS))
                .border_1()
                .border_color(self.border)
                .bg(self.background)
                .text_color(self.text)
                .text_size(px(MANAGER_ROW_TEXT_SIZE))
                .shadow_lg()
                .child(self.label.clone()),
        )
    }
}
