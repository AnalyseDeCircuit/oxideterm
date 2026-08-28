use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use crate::workspace::new_connection::{
    NewConnectionProxyHop, NewConnectionUpstreamProxyAuth, NewConnectionUpstreamProxyPolicy,
    identity_agent_from_form, identity_agent_selector, ssh_auth_tab_from_saved_auth,
};
use crate::workspace::quick_commands::QuickCommandImportStrategy;
use crate::workspace::session_icons;
use chrono::{DateTime, Datelike, Local, Utc};
use gpui::{Div, EventEmitter, Pixels, Point, Rgba, Task, prelude::*, rgba};
use oxideterm_connections::{
    AuthType, ConnectionAuthDraft, ConnectionAuthDraftKind, ConnectionDraft, ConnectionInfo,
    ConnectionStore, MoshProfile, ProxyHopDraft, RemoteDesktopProfile, SaveConnectionRequest,
    SavedAuth, SavedConnection, SavedProxyCommand, SavedUpstreamProxyAuth,
    SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy, SavedUpstreamProxyProtocol, SecretString,
    SerialProfile, SshConfigHost, TelnetProfile,
    oxide_file::{
        ExportPreflightResult, ForwardDetail, ImportConflictStrategy, ImportPreview,
        ImportResultEnvelope, OxideExportOptions, OxideFile, OxideFileError, OxideForwardRecord,
        OxideImportOptions, OxideMetadata, apply_oxide_import_with_options_with_progress,
        export_connections_to_oxide_with_progress, preflight_export,
        preview_oxide_import_with_progress,
    },
    save_request_from_draft, validate_group_name,
};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_forwarding::{ForwardType, OwnedForwardImportRecord, PersistedForward};
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonIconPosition, ToolbarButtonOptions,
    },
    checkbox, confirm_dialog,
    context_menu::{ContextMenuActionableStyle, context_menu_event_boundary},
    dropdown_menu::{
        DropdownMenuItemKind, dropdown_menu_content, dropdown_menu_item, dropdown_menu_separator,
    },
    modal::{dismissible_dialog_backdrop, modal_backdrop, overlay_content_boundary},
    surface::{color_for_background, color_for_background_or_alpha},
    text_input::{
        text_caret, text_input_secret_mask, text_input_value_segments, text_input_visual_range,
    },
};
use oxideterm_session_adapter::upstream_proxy_config_from_saved_policy;
use oxideterm_settings::{
    ALL_OXIDE_SETTINGS_SECTIONS, DEFAULT_OXIDE_SETTINGS_SECTIONS, PersistedSettings,
    export_oxide_settings_snapshot_json, merge_oxide_settings_snapshot,
};
use oxideterm_ssh::{UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyProtocol};
use zeroize::Zeroizing;

use super::*;
use crate::workspace::ime::WorkspaceImeTarget;

const BG_ACTIVE_THEME_ALPHA: u32 = 0x66; // Tauri [data-bg-active] color-mix(... 40%, transparent)
const BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // Tauri bg-hover 50%
const BG_ACTIVE_ROW_HOVER_ALPHA: u32 = 0x4d; // Keep full-width row hover quieter than compact controls.
const ROW_HOVER_ALPHA: u32 = 0x66; // Plain-theme rows use the same restrained hierarchy as image-backed rows.
const BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri border 75%
const BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60; // Tauri border/50 after active border mix
const SESSION_MANAGER_LIGHT_DIALOG_BACKDROP_ALPHA: u32 = 0x66; // Keep lightweight manager dialogs readable without heavy blur.
const MANAGER_TOOLBAR_SEARCH_WIDTH: f32 = 384.0; // Tauri max-w-sm
const MANAGER_ROW_TEXT_SIZE: f32 = 14.0;
const MANAGER_ROW_META_TEXT_SIZE: f32 = 12.0;
const MANAGER_TABLE_HEADER_TEXT_SIZE: f32 = 12.0;
const MANAGER_ROW_ACTION_BUTTON: f32 = 24.0; // Tauri h-6 w-6
const MANAGER_ROW_ACTION_ICON_SIZE: f32 = 12.0;
const MANAGER_ROW_ACTION_GAP: f32 = 2.0;
const MANAGER_ROW_ICON_SIZE: f32 = 40.0;
const MANAGER_ROW_DRAG_HANDLE_SIZE: f32 = 24.0;
const MANAGER_DRAG_PREVIEW_MAX_WIDTH: f32 = 280.0;
const MANAGER_DRAG_PREVIEW_RADIUS: f32 = 8.0;
const MANAGER_DRAG_GRIP_WIDTH: f32 = 6.0;
const MANAGER_DRAG_GRIP_DOT_SIZE: f32 = 2.0;
const MANAGER_DRAG_GRIP_DOT_COUNT: usize = 6;
const MANAGER_DRAG_GRIP_ALPHA: u32 = 0xb8;
const MANAGER_DRAG_ROOT_BG_ALPHA: u32 = 0x0d;
const MANAGER_DRAG_GROUP_BG_ALPHA: u32 = 0x16;
const MANAGER_GROUP_MANAGER_INDENT: f32 = 20.0;
const MANAGER_SELECTION_COLUMN_WIDTH: f32 = 16.0;
const MANAGER_LIST_LAST_USED_WIDTH: f32 = 96.0;
const MANAGER_ROW_ACTIONS_WIDTH: f32 = 76.0; // Three compact actions and their two gaps.
const MANAGER_RECENT_ITEM_MIN_WIDTH: f32 = 180.0;
const MANAGER_RECENT_ITEM_BASIS: f32 = 240.0;
const MANAGER_RECENT_ICON_SIZE: f32 = 28.0;
const MANAGER_RECENT_ICON_GLYPH_SIZE: f32 = 14.0;
const MANAGER_RECENT_ACCENT_BG_ALPHA: u32 = 0x1a;
const MANAGER_GRID_CARD_MIN_WIDTH: f32 = 260.0;
const MANAGER_GRID_CARD_BASIS: f32 = 320.0;
const MANAGER_GRID_ESTIMATED_ROW_HEIGHT: f32 = 84.0;
const MANAGER_LIST_ESTIMATED_ROW_HEIGHT: f32 = 57.0;
const MANAGER_TREE_ESTIMATED_ROW_HEIGHT: f32 = 52.0;
const MANAGER_MAIN_VIEW_OVERSCAN: usize = 6;
const MANAGER_ROW_ACTION_MENU_WIDTH: f32 = 176.0;
const MANAGER_ROW_ACTION_MENU_CONNECTION_HEIGHT: f32 = 120.0;
const MANAGER_ROW_ACTION_MENU_PROFILE_HEIGHT: f32 = 44.0;
const MANAGER_ROW_ACTION_MENU_EDITABLE_PROFILE_HEIGHT: f32 = 80.0;
const MANAGER_ROW_ACTION_MENU_GROUP_HEIGHT: f32 = 120.0;
const MANAGER_VIEW_MODE_MENU_WIDTH: f32 = 168.0; // Tauri DropdownMenuContent min-w-[160px] plus native menu padding.
const MANAGER_VIEW_MODE_MENU_HEIGHT: f32 = 104.0; // Three compact radio rows plus menu padding.
const MANAGER_SORT_MENU_WIDTH: f32 = 184.0; // Sort fields reuse the compact toolbar dropdown rhythm.
const MANAGER_SORT_MENU_HEIGHT: f32 = 220.0; // Seven compact radio rows plus menu padding.
const MANAGER_BATCH_MOVE_MENU_WIDTH: f32 = 220.0; // Tauri batch move DropdownMenuContent natural width.
const MANAGER_BATCH_MOVE_MENU_HEIGHT: f32 = 260.0; // Keeps long group lists scrollable without covering the viewport.
const MANAGER_RESPONSIVE_SM: f32 = 640.0;
const MANAGER_RESPONSIVE_MD: f32 = 768.0;
const OXIDE_APP_SETTINGS_SECTIONS: &[&str] = ALL_OXIDE_SETTINGS_SECTIONS;
const OXIDE_MODAL_WIDTH: f32 = 672.0; // Tauri max-w-2xl
const OXIDE_MODAL_MAX_HEIGHT_RATIO: f32 = 0.85; // Tauri max-h-[85vh]
const OXIDE_MODAL_HEADER_PX: f32 = 24.0; // Tauri px-6
const OXIDE_MODAL_HEADER_PY: f32 = 16.0; // Tauri py-4
const OXIDE_MODAL_BODY_P: f32 = 24.0; // Tauri p-6
const OXIDE_MODAL_SECTION_GAP: f32 = 16.0; // Tauri space-y-4
const OXIDE_MODAL_CARD_P: f32 = 12.0; // Tauri p-3
const OXIDE_MODAL_LIST_MAX_H: f32 = 256.0; // Tauri max-h-64
const OXIDE_MODAL_FORWARDS_MAX_H: f32 = 208.0; // Tauri max-h-52
const OXIDE_SELECT_ALL_BUTTON_HEIGHT: f32 = 28.0; // Tauri OxideExportModal Button h-7
const OXIDE_BLUE_500: u32 = 0x3b82f6;
const OXIDE_GREEN_500: u32 = 0x22c55e;
const OXIDE_YELLOW_500: u32 = 0xeab308;
const OXIDE_RED_500: u32 = 0xef4444;
const OXIDE_ORANGE_500: u32 = 0xf97316;
const OXIDE_SLATE_400: u32 = 0x94a3b8;
const OXIDE_TONE_BG_ALPHA: u32 = 0x1a; // Tauri *-500/10
const OXIDE_TONE_BORDER_ALPHA: u32 = 0x33; // Tauri *-500/20
const OXIDE_SUBCARD_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg-elevated/60 and bg-theme-bg/60
const OXIDE_NEW_BADGE_BG_ALPHA: u32 = 0x26; // Tauri bg-green-500/15

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum SessionManagerInput {
    Search,
    GroupName,
    OxideImportPassword,
    OxideExportPassword,
    OxideExportConfirmPassword,
    OxideExportDescription,
}

impl SessionManagerInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::GroupName => 3,
            Self::OxideImportPassword => 4,
            Self::OxideExportPassword => 5,
            Self::OxideExportConfirmPassword => 6,
            Self::OxideExportDescription => 7,
        }
    }

    pub(super) fn is_secret(self) -> bool {
        matches!(
            self,
            Self::OxideImportPassword
                | Self::OxideExportPassword
                | Self::OxideExportConfirmPassword
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionManagerViewMode {
    Grid,
    List,
    Tree,
}

impl SessionManagerViewMode {
    fn label_key(self) -> &'static str {
        match self {
            Self::Grid => "sessionManager.views.grid",
            Self::List => "sessionManager.views.list",
            Self::Tree => "sessionManager.views.tree",
        }
    }

    fn icon(self) -> LucideIcon {
        match self {
            Self::Grid => LucideIcon::Layers,
            Self::List => LucideIcon::LayoutList,
            Self::Tree => LucideIcon::ListTree,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionSortField {
    Name,
    Host,
    Port,
    Username,
    AuthType,
    Group,
    LastUsed,
}

impl SessionSortField {
    fn label_key(self) -> &'static str {
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

    fn default_direction(self) -> SortDirection {
        match self {
            Self::LastUsed => SortDirection::Desc,
            _ => SortDirection::Asc,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn toggled(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    fn icon(self) -> LucideIcon {
        match self {
            Self::Asc => LucideIcon::ArrowUpAZ,
            Self::Desc => LucideIcon::ArrowDownAZ,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionManagerBasicDialogFooterAction {
    Cancel,
    Primary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SessionManagerGroupEditor {
    Create {
        parent_path: Option<String>,
    },
    Rename {
        old_path: String,
        parent_path: Option<String>,
    },
}

const SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS: [SessionManagerBasicDialogFooterAction; 2] = [
    SessionManagerBasicDialogFooterAction::Cancel,
    SessionManagerBasicDialogFooterAction::Primary,
];

pub(super) enum SessionManagerWorkspaceEvent {
    OxideEffectsReady(oxide_actions::OxideWorkspaceEffects),
    RefreshOxideExportPreflight,
}

#[derive(Clone, Debug)]
pub(super) enum SessionManagerDeleteConfirm {
    Single {
        id: String,
        name: String,
    },
    SerialProfile {
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
pub(super) enum SessionManagerSelectionTarget {
    Connection(String),
    Serial(String),
    Telnet(String),
    Mosh(String),
    StandaloneSftp(String),
    RemoteDesktop(String),
}

#[derive(Clone)]
pub(super) struct SessionManagerDrag {
    // The payload owns stable saved-asset identities so virtual rows may be recycled mid-drag.
    pub(super) targets: Vec<SessionManagerSelectionTarget>,
    pub(super) label: String,
    pub(super) position: Point<Pixels>,
    pub(super) background: Rgba,
    pub(super) border: Rgba,
    pub(super) text: Rgba,
}

impl SessionManagerDrag {
    pub(super) fn with_position(&self, position: Point<Pixels>) -> Self {
        let mut preview = self.clone();
        preview.position = position;
        preview
    }
}

#[derive(Clone, Debug)]
pub(super) enum SessionManagerRowActionTarget {
    Connection(String),
    Serial(String),
    Telnet(String),
    Mosh(String),
    StandaloneSftp(String),
    RemoteDesktop(String),
    GroupRoot,
    Group(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionManagerRowActionMenuOrigin {
    ActionButton,
    Pointer,
}

#[derive(Clone, Debug)]
pub(super) struct SessionManagerRowActionMenu {
    // Stable ids keep the floating menu independent from temporary row futures.
    pub(super) target: SessionManagerRowActionTarget,
    pub(super) origin: SessionManagerRowActionMenuOrigin,
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct OxideImportResultView {
    pub(super) imported: usize,
    pub(super) skipped: usize,
    pub(super) merged: usize,
    pub(super) replaced: usize,
    pub(super) renamed: usize,
    pub(super) renames: Vec<(String, String)>,
    pub(super) errors: Vec<String>,
    pub(super) imported_forwards: usize,
    pub(super) skipped_forwards: usize,
    pub(super) imported_app_settings: bool,
    pub(super) skipped_app_settings: bool,
    pub(super) imported_quick_commands: usize,
    pub(super) skipped_quick_commands: bool,
    pub(super) imported_serial_profiles: usize,
    pub(super) skipped_serial_profiles: usize,
    pub(super) imported_telnet_profiles: usize,
    pub(super) skipped_telnet_profiles: usize,
    pub(super) imported_mosh_profiles: usize,
    pub(super) skipped_mosh_profiles: usize,
    pub(super) quick_commands_errors: Vec<String>,
    pub(super) imported_plugin_settings: usize,
    pub(super) skipped_plugin_settings: bool,
    pub(super) imported_portable_secrets: usize,
    pub(super) skipped_portable_secrets: usize,
}

#[derive(Clone, Debug)]
pub(super) struct OxideTransferProgress {
    pub(super) stage: String,
    pub(super) current: usize,
    pub(super) total: usize,
}

impl OxideTransferProgress {
    pub(super) fn new(stage: impl Into<String>, current: usize, total: usize) -> Self {
        Self {
            stage: stage.into(),
            current,
            total,
        }
    }

    pub(super) fn percent(&self) -> usize {
        if self.total == 0 {
            0
        } else {
            ((self.current.min(self.total) * 100) / self.total).min(100)
        }
    }
}

pub(super) struct SessionManagerState {
    pub(super) selected_group: Option<String>,
    pub(super) view_mode: SessionManagerViewMode,
    pub(super) sort_field: SessionSortField,
    pub(super) sort_direction: SortDirection,
    pub(super) search_query: String,
    pub(super) selected_items: HashSet<SessionManagerSelectionTarget>,
    pub(super) view_mode_menu_open: bool,
    pub(super) sort_menu_open: bool,
    pub(super) row_action_menu: Option<SessionManagerRowActionMenu>,
    pub(super) expanded_groups: HashSet<String>,
    pub(super) focused_input: Option<SessionManagerInput>,
    pub(super) group_editor: Option<SessionManagerGroupEditor>,
    pub(super) group_name_draft: String,
    pub(super) group_editor_error: Option<String>,
    pub(super) group_manager_error: Option<String>,
    pub(super) show_group_manager: bool,
    pub(super) reopen_group_manager_after_delete: bool,
    pub(super) focused_basic_dialog_footer_action: Option<SessionManagerBasicDialogFooterAction>,
    pub(super) show_batch_move: bool,
    pub(super) delete_confirm: Option<SessionManagerDeleteConfirm>,
    pub(super) oxide_import_dialog: Option<OxideImportDialogState>,
    pub(super) oxide_export_dialog: Option<OxideExportDialogState>,
    pub(super) status: Option<String>,
    pub(super) ssh_config_hosts: Vec<SshConfigHost>,
    pub(super) main_grid_list_state: ListState,
    pub(super) main_grid_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) main_list_state: ListState,
    pub(super) main_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) main_tree_list_state: ListState,
    pub(super) main_tree_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_export_connection_list_state: ListState,
    pub(super) oxide_export_connection_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_import_connection_preview_list_state: ListState,
    pub(super) oxide_import_connection_preview_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_export_forward_group_list_state: ListState,
    pub(super) oxide_export_forward_group_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_export_summary_line_list_state: ListState,
    pub(super) oxide_export_summary_line_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_import_forward_detail_list_state: ListState,
    pub(super) oxide_import_forward_detail_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) oxide_import_name_group_list_states: RefCell<HashMap<String, ListState>>,
    pub(super) oxide_import_name_group_list_caches:
        RefCell<HashMap<String, VirtualListSignatureCache>>,
    pub(super) import_dialog_exit_task: Option<Task<()>>,
    pub(super) export_dialog_exit_task: Option<Task<()>>,
    pub(super) dialog_auto_close_task: Option<Task<()>>,
    pub(super) import_file_picker_task: Option<Task<()>>,
    pub(super) export_file_picker_task: Option<Task<()>>,
    oxide_worker_tx: Option<delivery::ActiveDeliverySender<oxide_actions::OxideWorkerDelivery>>,
    oxide_worker_rx: Option<std::sync::mpsc::Receiver<oxide_actions::OxideWorkerDelivery>>,
    _oxide_delivery_task: Option<Task<()>>,
    oxide_worker_threads: HashMap<oxide_actions::OxideWorkerKey, std::thread::JoinHandle<()>>,
    ssh_config_load_generation: u64,
    ssh_config_load_task: Option<Task<()>>,
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
            selected_items: HashSet::new(),
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
            _oxide_delivery_task: None,
            oxide_worker_threads: HashMap::new(),
            ssh_config_load_generation: 0,
            ssh_config_load_task: None,
        }
    }
}

impl EventEmitter<SessionManagerWorkspaceEvent> for SessionManagerState {}

impl SessionManagerState {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let mut state = Self::default();
        state.initialize_oxide_delivery(cx);
        state
    }

    pub(in crate::workspace) fn set_status(
        &mut self,
        status: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn focused_input(&self) -> Option<SessionManagerInput> {
        self.focused_input
    }

    pub(in crate::workspace) fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn input_value(&self, input: SessionManagerInput) -> Option<&str> {
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
        }
    }

    pub(in crate::workspace) fn replace_input(
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
        };
        replace_utf16(value, replacement_range, text);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn clear_ssh_config_hosts(&mut self, cx: &mut Context<Self>) {
        self.ssh_config_load_generation = self.ssh_config_load_generation.wrapping_add(1);
        self.ssh_config_load_task = None;
        if !self.ssh_config_hosts.is_empty() {
            self.ssh_config_hosts.clear();
            cx.notify();
        }
    }

    pub(in crate::workspace) fn begin_ssh_config_host_load(
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

    pub(in crate::workspace) fn remove_ssh_config_host_alias(
        &mut self,
        alias: &str,
        cx: &mut Context<Self>,
    ) {
        let previous_count = self.ssh_config_hosts.len();
        self.ssh_config_hosts.retain(|host| host.alias != alias);
        if self.ssh_config_hosts.len() != previous_count {
            cx.notify();
        }
    }
}

pub(super) struct OxideImportDialogState {
    pub(super) presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) file_path: Option<PathBuf>,
    pub(super) file_data: Option<Arc<[u8]>>,
    pub(super) metadata_summary: Option<String>,
    pub(super) metadata: Option<OxideMetadata>,
    pub(super) password: Zeroizing<String>,
    pub(super) conflict_strategy: ImportConflictStrategy,
    pub(super) preview: Option<Arc<ImportPreview>>,
    pub(super) selected_names: HashSet<String>,
    pub(super) import_app_settings: bool,
    pub(super) selected_app_settings_sections: HashSet<String>,
    pub(super) expanded_app_settings_sections: HashSet<String>,
    pub(super) import_quick_commands: bool,
    pub(super) import_serial_profiles: bool,
    pub(super) import_telnet_profiles: bool,
    pub(super) import_mosh_profiles: bool,
    pub(super) import_plugin_settings: bool,
    pub(super) selected_plugin_ids: HashSet<String>,
    pub(super) import_forwards: bool,
    pub(super) import_portable_secrets: bool,
    pub(super) restore_managed_keys: bool,
    pub(super) restore_managed_key_passphrases: bool,
    pub(super) busy: bool,
    pub(super) operation_generation: u64,
    pub(super) progress_stage: Option<OxideTransferProgress>,
    pub(super) focused_footer_action: Option<OxideDialogFooterAction>,
    pub(super) error: Option<String>,
    pub(super) result_summary: Option<String>,
    pub(super) result: Option<Arc<OxideImportResultView>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OxideDialogFooterAction {
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

pub(super) struct OxideExportDialogState {
    pub(super) presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) selected_ids: HashSet<String>,
    connection_rows: Arc<[oxide_export_selection_dialogs::OxideExportConnectionRow]>,
    forward_group_rows: Arc<[oxide_export_selection_dialogs::OxideExportForwardGroupRow]>,
    pub(super) available_forwards: Vec<PersistedForward>,
    pub(super) selected_forward_ids: HashSet<String>,
    pub(super) include_app_settings: bool,
    pub(super) selected_app_settings_sections: HashSet<String>,
    pub(super) include_local_terminal_env_vars: bool,
    pub(super) include_quick_commands: bool,
    pub(super) include_serial_profiles: bool,
    pub(super) include_telnet_profiles: bool,
    pub(super) include_mosh_profiles: bool,
    pub(super) include_remote_desktop_profiles: bool,
    pub(super) include_plugin_settings: bool,
    pub(super) plugin_groups: HashMap<String, usize>,
    pub(super) selected_plugin_ids: HashSet<String>,
    pub(super) include_forwards: bool,
    pub(super) include_portable_secrets: bool,
    pub(super) embed_keys: bool,
    pub(super) include_passwords: bool,
    pub(super) include_key_passphrases: bool,
    pub(super) include_managed_keys: bool,
    pub(super) include_managed_key_passphrases: bool,
    pub(super) password: Zeroizing<String>,
    pub(super) confirm_password: Zeroizing<String>,
    pub(super) description: String,
    pub(super) busy: bool,
    pub(super) operation_generation: u64,
    pub(super) progress_stage: Option<OxideTransferProgress>,
    pub(super) focused_footer_action: Option<OxideDialogFooterAction>,
    pub(super) last_export_timestamp: Option<i64>,
    pub(super) preflight: Option<ExportPreflightResult>,
    pub(super) error: Option<String>,
    pub(super) result_summary: Option<String>,
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
            .field(
                "include_remote_desktop_profiles",
                &self.include_remote_desktop_profiles,
            )
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
mod actions;
mod controls;
mod dialogs;
mod helpers;
mod oxide_actions;
mod oxide_dialog_common;
mod oxide_dialog_helpers;
mod oxide_export_dialogs;
mod oxide_export_selection_dialogs;
mod oxide_export_summary_dialogs;
mod oxide_import_dialogs;
mod oxide_import_preview_dialogs;
mod oxide_import_result_dialogs;
mod surface;
mod tree;
mod views;

// Recreate the former flat include scope without exposing internal helpers to the workspace.
#[allow(unused_imports)]
use self::{
    actions::*, controls::*, dialogs::*, helpers::*, oxide_actions::*, oxide_dialog_common::*,
    oxide_dialog_helpers::*, oxide_export_dialogs::*, oxide_export_selection_dialogs::*,
    oxide_export_summary_dialogs::*, oxide_import_dialogs::*, oxide_import_preview_dialogs::*,
    oxide_import_result_dialogs::*, surface::*, tree::*, views::*,
};

// Preserve the workspace-facing session manager API at its original visibility.
#[cfg(test)]
pub(in crate::workspace) use self::helpers::save_request_from_form;
pub(in crate::workspace) use self::helpers::{
    RuntimeSecretHandoff, duplicate_connection_template_name, form_from_saved_connection,
    restore_legacy_jump_host_in_form, save_request_from_form_with_existing_auth,
    save_request_from_form_with_proxy_hop_prefix, upstream_proxy_config_from_form,
};

#[cfg(test)]
mod tests;
