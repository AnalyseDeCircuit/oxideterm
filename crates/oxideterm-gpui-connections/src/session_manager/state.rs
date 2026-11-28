// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::{
    Context, EventEmitter, ListAlignment, ListState, Pixels, Point, Render, Rgba, Window, div,
    prelude::*, px,
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::{
    icons::LucideIcon,
    virtual_list::{TauriVirtualListSpec, VirtualListSignatureCache, tauri_virtual_list_state},
};
use std::{cell::RefCell, collections::HashSet};
#[cfg(not(target_family = "wasm"))]
mod desktop;
#[cfg(not(target_family = "wasm"))]
pub use desktop::*;
#[cfg(not(target_family = "wasm"))]
use gpui::Task;
#[cfg(not(target_family = "wasm"))]
use oxideterm_connections::SshConfigHost;
#[cfg(not(target_family = "wasm"))]
use oxideterm_workspace::delivery;
#[cfg(not(target_family = "wasm"))]
use std::collections::HashMap;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionManagerOpenTarget {
    Tunnel(String),
    Plugin(String),
    Connection(String),
    SshConfig(String),
    Discovery((String, String)),
    Serial(String),
    Container(String),
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
    #[cfg(not(target_family = "wasm"))]
    OxideEffectsReady(oxide_delivery::OxideWorkspaceEffects),
    #[cfg(not(target_family = "wasm"))]
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
    ContainerProfile {
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
    Container(String),
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
    Discovery((String, String)),
    Serial(String),
    Container(String),
    Ble(String),
    LocalTerminal(String),
    Telnet(String),
    Ftp(String),
    Mosh(String),
    StandaloneSftp(String),
    RemoteDesktop(String),
    DiscoverySource(String),
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
            Self::Discovery(alias) => Some(SessionManagerOpenTarget::Discovery(alias.clone())),
            Self::Serial(id) => Some(SessionManagerOpenTarget::Serial(id.clone())),
            Self::Container(id) => Some(SessionManagerOpenTarget::Container(id.clone())),
            Self::Ble(id) => Some(SessionManagerOpenTarget::Ble(id.clone())),
            Self::LocalTerminal(id) => Some(SessionManagerOpenTarget::LocalTerminal(id.clone())),
            Self::Telnet(id) => Some(SessionManagerOpenTarget::Telnet(id.clone())),
            Self::Ftp(id) => Some(SessionManagerOpenTarget::Ftp(id.clone())),
            Self::Mosh(id) => Some(SessionManagerOpenTarget::Mosh(id.clone())),
            Self::StandaloneSftp(id) => Some(SessionManagerOpenTarget::StandaloneSftp(id.clone())),
            Self::RemoteDesktop(id) => Some(SessionManagerOpenTarget::RemoteDesktop(id.clone())),
            Self::DiscoverySource(_) | Self::GroupRoot | Self::Group(_) => None,
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

pub struct SessionManagerState {
    pub selected_group: Option<String>,
    pub view_mode: SessionManagerViewMode,
    pub sort_field: SessionSortField,
    pub sort_direction: SortDirection,
    pub search_query: String,
    pub sidebar_search_open: bool,
    pub sidebar_sort_open: bool,
    pub selected_items: HashSet<SessionManagerSelectionTarget>,
    pub focused_discovery: Option<(String, String)>,
    // The fixed endpoint of the last plain click, used by Shift+click range selection.
    pub selection_anchor: Option<SessionManagerSelectionTarget>,
    pub(super) keyboard_cursor: Option<SessionManagerSelectionTarget>,
    pub(super) reveal_keyboard_cursor: bool,
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
    pub(super) delete_focus: Option<oxideterm_gpui_ui::ConfirmDialogAction>,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_import_dialog: Option<OxideImportDialogState>,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_export_dialog: Option<OxideExportDialogState>,
    pub status: Option<String>,
    #[cfg(not(target_family = "wasm"))]
    pub ssh_config_hosts: Vec<SshConfigHost>,
    #[cfg(not(target_family = "wasm"))]
    pub discovery_entries: Vec<oxideterm_discovery::CatalogEntry>,
    pub discovery_groups: Vec<DiscoveryCatalogGroup>,
    pub expanded_discovery: HashSet<Vec<String>>,
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
    #[cfg(not(target_family = "wasm"))]
    pub oxide_export_connection_list_state: ListState,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_export_connection_list_cache: RefCell<VirtualListSignatureCache>,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_import_connection_preview_list_state: ListState,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_import_connection_preview_list_cache: RefCell<VirtualListSignatureCache>,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_export_forward_group_list_state: ListState,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_export_forward_group_list_cache: RefCell<VirtualListSignatureCache>,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_export_summary_line_list_state: ListState,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_export_summary_line_list_cache: RefCell<VirtualListSignatureCache>,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_import_forward_detail_list_state: ListState,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_import_forward_detail_list_cache: RefCell<VirtualListSignatureCache>,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_import_name_group_list_states: RefCell<HashMap<String, ListState>>,
    #[cfg(not(target_family = "wasm"))]
    pub oxide_import_name_group_list_caches: RefCell<HashMap<String, VirtualListSignatureCache>>,
    #[cfg(not(target_family = "wasm"))]
    pub import_dialog_exit_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub export_dialog_exit_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub dialog_auto_close_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub import_file_picker_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub export_file_picker_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub(super) oxide_worker_tx:
        Option<delivery::ActiveDeliverySender<oxide_delivery::OxideWorkerDelivery>>,
    #[cfg(not(target_family = "wasm"))]
    pub(super) oxide_worker_rx:
        Option<std::sync::mpsc::Receiver<oxide_delivery::OxideWorkerDelivery>>,
    #[cfg(not(target_family = "wasm"))]
    pub(super) oxide_delivery_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub(super) oxide_worker_threads:
        HashMap<oxide_delivery::OxideWorkerKey, std::thread::JoinHandle<()>>,
    #[cfg(not(target_family = "wasm"))]
    ssh_config_load_generation: u64,
    #[cfg(not(target_family = "wasm"))]
    ssh_config_load_task: Option<Task<()>>,
    #[cfg(not(target_family = "wasm"))]
    pub batch_add: Option<BatchAddDialogState>,
    #[cfg(not(target_family = "wasm"))]
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
            focused_discovery: None,
            selection_anchor: None,
            keyboard_cursor: None,
            reveal_keyboard_cursor: false,
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
            delete_focus: None,
            #[cfg(not(target_family = "wasm"))]
            oxide_import_dialog: None,
            #[cfg(not(target_family = "wasm"))]
            oxide_export_dialog: None,
            status: None,
            #[cfg(not(target_family = "wasm"))]
            ssh_config_hosts: Vec::new(),
            #[cfg(not(target_family = "wasm"))]
            discovery_entries: Vec::new(),
            discovery_groups: Vec::new(),
            expanded_discovery: HashSet::new(),
            #[cfg(not(target_family = "wasm"))]
            batch_add: None,
            #[cfg(not(target_family = "wasm"))]
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
            #[cfg(not(target_family = "wasm"))]
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
            #[cfg(not(target_family = "wasm"))]
            oxide_export_connection_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            #[cfg(not(target_family = "wasm"))]
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
            #[cfg(not(target_family = "wasm"))]
            oxide_import_connection_preview_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            #[cfg(not(target_family = "wasm"))]
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
            #[cfg(not(target_family = "wasm"))]
            oxide_export_forward_group_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            #[cfg(not(target_family = "wasm"))]
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
            #[cfg(not(target_family = "wasm"))]
            oxide_export_summary_line_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            #[cfg(not(target_family = "wasm"))]
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
            #[cfg(not(target_family = "wasm"))]
            oxide_import_forward_detail_list_cache: RefCell::new(
                VirtualListSignatureCache::default(),
            ),
            #[cfg(not(target_family = "wasm"))]
            oxide_import_name_group_list_states: RefCell::new(HashMap::new()),
            #[cfg(not(target_family = "wasm"))]
            oxide_import_name_group_list_caches: RefCell::new(HashMap::new()),
            #[cfg(not(target_family = "wasm"))]
            import_dialog_exit_task: None,
            #[cfg(not(target_family = "wasm"))]
            export_dialog_exit_task: None,
            #[cfg(not(target_family = "wasm"))]
            dialog_auto_close_task: None,
            #[cfg(not(target_family = "wasm"))]
            import_file_picker_task: None,
            #[cfg(not(target_family = "wasm"))]
            export_file_picker_task: None,
            #[cfg(not(target_family = "wasm"))]
            oxide_worker_tx: None,
            #[cfg(not(target_family = "wasm"))]
            oxide_worker_rx: None,
            #[cfg(not(target_family = "wasm"))]
            oxide_delivery_task: None,
            #[cfg(not(target_family = "wasm"))]
            oxide_worker_threads: HashMap::new(),
            #[cfg(not(target_family = "wasm"))]
            ssh_config_load_generation: 0,
            #[cfg(not(target_family = "wasm"))]
            ssh_config_load_task: None,
        }
    }
}

impl EventEmitter<SessionManagerWorkspaceEvent> for SessionManagerState {}

impl SessionManagerState {
    #[cfg(not(target_family = "wasm"))]
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
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::OxideImportPassword => self
                .oxide_import_dialog
                .as_ref()
                .map(|dialog| dialog.password.as_str()),
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::OxideExportPassword => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.password.as_str()),
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::OxideExportConfirmPassword => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.confirm_password.as_str()),
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::OxideExportDescription => self
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| dialog.description.as_str()),
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::BatchPassword => self
                .batch_add
                .as_ref()
                .map(|dialog| dialog.password.as_str()),
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::BatchKeyPath => self
                .batch_add
                .as_ref()
                .map(|dialog| dialog.key_path.as_str()),
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::BatchTemplateName => self
                .batch_add
                .as_ref()
                .map(|dialog| dialog.rename_draft.as_str()),
            #[cfg(target_family = "wasm")]
            _ => None,
        }
    }

    /// The key picker trigger only exists while the dialog authenticates with a
    /// key, so its overlay anchor is valid only in that mode.
    #[cfg(not(target_family = "wasm"))]
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
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::OxideImportPassword => {
                let Some(dialog) = self.oxide_import_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.password
            }
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::OxideExportPassword => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.password
            }
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::OxideExportConfirmPassword => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.confirm_password
            }
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::OxideExportDescription => {
                let Some(dialog) = self.oxide_export_dialog.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.description
            }
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::BatchPassword => {
                let Some(dialog) = self.batch_add.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.password
            }
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::BatchKeyPath => {
                let Some(dialog) = self.batch_add.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.key_path
            }
            #[cfg(not(target_family = "wasm"))]
            SessionManagerInput::BatchTemplateName => {
                let Some(dialog) = self.batch_add.as_mut() else {
                    return false;
                };
                dialog.error = None;
                &mut dialog.rename_draft
            }
            #[cfg(target_family = "wasm")]
            _ => return false,
        };
        replace_utf16(value, replacement_range, text);
        cx.notify();
        true
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn update_discovery(
        &mut self,
        entries: Vec<oxideterm_discovery::CatalogEntry>,
        groups: Vec<DiscoveryCatalogGroup>,
        cx: &mut Context<Self>,
    ) {
        if self.discovery_entries != entries || self.discovery_groups != groups {
            if self
                .focused_discovery
                .as_ref()
                .is_some_and(|(source, target)| {
                    !entries
                        .iter()
                        .any(|entry| &entry.source_id == source && &entry.target_id == target)
                })
            {
                self.focused_discovery = None;
            }
            self.discovery_entries = entries;
            self.discovery_groups = groups;
            cx.notify();
        }
    }
}
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
