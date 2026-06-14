use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use crate::workspace::new_connection::{
    NewConnectionUpstreamProxyAuth, NewConnectionUpstreamProxyPolicy,
};
use crate::workspace::quick_commands::QuickCommandImportStrategy;
use chrono::{DateTime, Datelike, Local, Utc};
use gpui::{Div, StatefulInteractiveElement, prelude::*};
use oxideterm_connections::{
    AuthType, ConnectionAuthDraft, ConnectionAuthDraftKind, ConnectionDraft, ConnectionInfo,
    ConnectionStore, ProxyHopDraft, SaveConnectionRequest, SavedAuth, SavedConnection,
    SavedUpstreamProxyAuth, SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy,
    SavedUpstreamProxyProtocol, SecretString, SerialProfile, SshConfigHost, TelnetProfile,
    list_ssh_config_hosts,
    oxide_file::{
        ExportPreflightResult, ForwardDetail, ImportConflictStrategy, ImportPreview,
        ImportResultEnvelope, OxideExportOptions, OxideFile, OxideFileError, OxideForwardRecord,
        OxideImportOptions, OxideMetadata, apply_oxide_import_with_options_with_progress,
        export_connections_to_oxide_with_progress, preflight_export,
        preview_oxide_import_with_progress,
    },
    resolve_ssh_config_alias, save_request_from_draft, saved_connection_from_ssh_host,
};
use oxideterm_forwarding::{ForwardType, OwnedForwardImportRecord, PersistedForward};
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView, IconBadgeMetrics, TauriTableCellOptions,
    TauriTableCellStyle, TauriTableColors, TauriTableMetrics,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonIconPosition, ToolbarButtonOptions,
    },
    checkbox, confirm_dialog,
    context_menu::{
        ContextMenuActionableStyle, ContextMenuItemKind, context_menu_content,
        context_menu_event_boundary, context_menu_item_row,
    },
    icon_badge,
    modal::{dismissible_dialog_backdrop, overlay_content_boundary},
    modal_body, modal_container, modal_footer, modal_overlay,
    surface::{color_for_background, color_for_background_or_alpha},
    tauri_table_checkbox_cell, tauri_table_header, tauri_table_row, tauri_table_spacer_cell,
    text_input::{
        text_caret, text_input_anchor_probe, text_input_secret_mask, text_input_value_segments,
        text_input_visual_range,
    },
};
use oxideterm_session_adapter::upstream_proxy_config_from_saved_policy;
use oxideterm_settings::{
    ALL_OXIDE_SETTINGS_SECTIONS, DEFAULT_OXIDE_SETTINGS_SECTIONS, PersistedSettings,
    export_oxide_settings_snapshot_json, merge_oxide_settings_snapshot,
};
use oxideterm_ssh::{
    AuthMethod, SshConfig, UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyProtocol,
};

use super::*;
use crate::workspace::ime::WorkspaceImeTarget;

const UNGROUPED_FILTER: &str = "__ungrouped__";
const RECENT_FILTER: &str = "__recent__";
const BG_ACTIVE_THEME_ALPHA: u32 = 0x66; // Tauri [data-bg-active] color-mix(... 40%, transparent)
const BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // Tauri bg-hover 50%
const BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri border 75%
const BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60; // Tauri border/50 after active border mix
const BG_ACTIVE_ROW_SELECTED_ALPHA: u32 = 0x1a; // Tauri blue-500/10
const MANAGER_FOLDER_TREE_WIDTH: f32 = 180.0; // Tauri w-[180px]
const MANAGER_TOOLBAR_SEARCH_WIDTH: f32 = 384.0; // Tauri max-w-sm
const MANAGER_COL_CHECKBOX: f32 = 32.0;
const MANAGER_COL_NAME_BASIS: f32 = 140.0;
const MANAGER_COL_NAME_MIN: f32 = 100.0;
const MANAGER_COL_HOST: f32 = 130.0;
const MANAGER_COL_PORT: f32 = 50.0;
const MANAGER_COL_USERNAME: f32 = 90.0;
const MANAGER_COL_AUTH: f32 = 72.0;
const MANAGER_COL_GROUP: f32 = 100.0;
const MANAGER_COL_LAST_USED: f32 = 90.0;
const MANAGER_COL_ACTIONS: f32 = 84.0;
const MANAGER_COLOR_INDICATOR_WIDTH: f32 = 4.0;
const MANAGER_ROW_TEXT_SIZE: f32 = 14.0;
const MANAGER_ROW_META_TEXT_SIZE: f32 = 12.0;
const MANAGER_TABLE_HEADER_TEXT_SIZE: f32 = 12.0;
const MANAGER_AUTH_BADGE_TEXT_SIZE: f32 = 10.0;
const MANAGER_AUTH_BADGE_ICON_SIZE: f32 = 12.0; // Tauri h-3 w-3
const MANAGER_AUTH_BADGE_GAP: f32 = 4.0; // Tauri gap-1
const MANAGER_AUTH_BADGE_PADDING_X: f32 = 6.0; // Tauri px-1.5
const MANAGER_AUTH_BADGE_CHAR_WIDTH: f32 = 6.0; // Approx text-[10px] inline span width
const MANAGER_ROW_ACTION_BUTTON: f32 = 24.0; // Tauri h-6 w-6
const MANAGER_ROW_MORE_BUTTON: f32 = 28.0; // Tauri h-7 w-7
const MANAGER_ROW_MENU_WIDTH: f32 = 184.0;
const MANAGER_ROW_MENU_HEIGHT: f32 = 112.0;
const MANAGER_ROW_CONTEXT_MENU_HEIGHT: f32 = 180.0;
pub(super) const MANAGER_TABLE_VIRTUAL_ROW_HEIGHT: f32 = 37.0; // Tauri ConnectionTable CONNECTION_ROW_HEIGHT
pub(super) const MANAGER_TABLE_VIRTUAL_OVERSCAN: usize = 16; // Tauri useVirtualizer overscan
pub(super) const SAVED_CONNECTION_VIRTUAL_ROW_HEIGHT: f32 = 43.0; // Tauri Sidebar SAVED_CONNECTION_ROW_HEIGHT
pub(super) const SAVED_CONNECTION_VIRTUAL_OVERSCAN: usize = 12; // Tauri savedListVirtualizer overscan
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
    SavedSearch,
    NewGroup,
    AutoRouteDisplayName,
    OxideImportPassword,
    OxideExportPassword,
    OxideExportConfirmPassword,
    OxideExportDescription,
}

impl SessionManagerInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::SavedSearch => 2,
            Self::NewGroup => 3,
            Self::AutoRouteDisplayName => 4,
            Self::OxideImportPassword => 5,
            Self::OxideExportPassword => 6,
            Self::OxideExportConfirmPassword => 7,
            Self::OxideExportDescription => 8,
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
pub(super) enum SessionSortField {
    Name,
    Host,
    Port,
    Username,
    AuthType,
    Group,
    LastUsed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionManagerBasicDialogFooterAction {
    Cancel,
    Primary,
}

const SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS: [SessionManagerBasicDialogFooterAction; 2] = [
    SessionManagerBasicDialogFooterAction::Cancel,
    SessionManagerBasicDialogFooterAction::Primary,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionTransferAction {
    ImportOxide,
    ExportOxide,
}

#[derive(Clone, Debug)]
pub(super) enum SessionManagerDeleteConfirm {
    Single { id: String, name: String },
    SerialProfile { id: String, name: String },
    TelnetProfile { id: String, name: String },
    Batch { ids: Vec<String> },
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

impl SortDirection {
    fn toggled(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SessionManagerState {
    pub(super) selected_group: Option<String>,
    pub(super) search_query: String,
    pub(super) saved_search_query: String,
    pub(super) sort_field: SessionSortField,
    pub(super) sort_direction: SortDirection,
    pub(super) selected_ids: HashSet<String>,
    pub(super) hovered_connection_id: Option<String>,
    pub(super) row_menu_connection_id: Option<String>,
    pub(super) row_menu_x: f32,
    pub(super) row_menu_y: f32,
    pub(super) row_context_menu_connection_id: Option<String>,
    pub(super) row_context_menu_x: f32,
    pub(super) row_context_menu_y: f32,
    pub(super) folder_tree_context_menu_x: Option<f32>,
    pub(super) folder_tree_context_menu_y: Option<f32>,
    pub(super) expanded_groups: HashSet<String>,
    pub(super) focused_input: Option<SessionManagerInput>,
    pub(super) show_new_group: bool,
    pub(super) new_group_name: String,
    pub(super) show_import: bool,
    pub(super) focused_basic_dialog_footer_action: Option<SessionManagerBasicDialogFooterAction>,
    pub(super) ssh_config_hosts: Vec<SshConfigHost>,
    pub(super) selected_import_aliases: HashSet<String>,
    pub(super) show_batch_move: bool,
    pub(super) delete_confirm: Option<SessionManagerDeleteConfirm>,
    pub(super) oxide_import_dialog: Option<OxideImportDialogState>,
    pub(super) oxide_export_dialog: Option<OxideExportDialogState>,
    pub(super) status: Option<String>,
    pub(super) table_scroll_handle: UniformListScrollHandle,
    pub(super) saved_sidebar_scroll_handle: UniformListScrollHandle,
}

impl Default for SessionManagerState {
    fn default() -> Self {
        Self {
            selected_group: None,
            search_query: String::new(),
            saved_search_query: String::new(),
            sort_field: SessionSortField::LastUsed,
            sort_direction: SortDirection::Desc,
            selected_ids: HashSet::new(),
            hovered_connection_id: None,
            row_menu_connection_id: None,
            row_menu_x: 0.0,
            row_menu_y: 0.0,
            row_context_menu_connection_id: None,
            row_context_menu_x: 0.0,
            row_context_menu_y: 0.0,
            folder_tree_context_menu_x: None,
            folder_tree_context_menu_y: None,
            expanded_groups: HashSet::new(),
            focused_input: None,
            show_new_group: false,
            new_group_name: String::new(),
            show_import: false,
            focused_basic_dialog_footer_action: None,
            ssh_config_hosts: Vec::new(),
            selected_import_aliases: HashSet::new(),
            show_batch_move: false,
            delete_confirm: None,
            oxide_import_dialog: None,
            oxide_export_dialog: None,
            status: None,
            table_scroll_handle: UniformListScrollHandle::new(),
            saved_sidebar_scroll_handle: UniformListScrollHandle::new(),
        }
    }
}

#[derive(Clone)]
pub(super) struct OxideImportDialogState {
    pub(super) file_path: Option<PathBuf>,
    pub(super) file_data: Option<Vec<u8>>,
    pub(super) metadata_summary: Option<String>,
    pub(super) metadata: Option<OxideMetadata>,
    pub(super) password: String,
    pub(super) conflict_strategy: ImportConflictStrategy,
    pub(super) preview: Option<ImportPreview>,
    pub(super) selected_names: HashSet<String>,
    pub(super) import_app_settings: bool,
    pub(super) selected_app_settings_sections: HashSet<String>,
    pub(super) expanded_app_settings_sections: HashSet<String>,
    pub(super) import_quick_commands: bool,
    pub(super) import_serial_profiles: bool,
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
    pub(super) result: Option<OxideImportResultView>,
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
            file_path: None,
            file_data: None,
            metadata_summary: None,
            metadata: None,
            password: String::new(),
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

#[derive(Clone)]
pub(super) struct OxideExportDialogState {
    pub(super) selected_ids: HashSet<String>,
    pub(super) available_forwards: Vec<PersistedForward>,
    pub(super) selected_forward_ids: HashSet<String>,
    pub(super) include_app_settings: bool,
    pub(super) selected_app_settings_sections: HashSet<String>,
    pub(super) include_local_terminal_env_vars: bool,
    pub(super) include_quick_commands: bool,
    pub(super) include_serial_profiles: bool,
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
    pub(super) password: String,
    pub(super) confirm_password: String,
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
            selected_ids: HashSet::new(),
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
            password: String::new(),
            confirm_password: String::new(),
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

include!("session_manager/surface.rs");
include!("session_manager/tree.rs");
include!("session_manager/table.rs");
include!("session_manager/controls.rs");
include!("session_manager/dialogs.rs");
include!("session_manager/oxide_dialog_common.rs");
include!("session_manager/oxide_import_dialogs.rs");
include!("session_manager/oxide_import_preview_dialogs.rs");
include!("session_manager/oxide_import_result_dialogs.rs");
include!("session_manager/oxide_export_dialogs.rs");
include!("session_manager/oxide_export_selection_dialogs.rs");
include!("session_manager/oxide_export_summary_dialogs.rs");
include!("session_manager/oxide_dialog_helpers.rs");
include!("session_manager/actions.rs");
include!("session_manager/oxide_actions.rs");
include!("session_manager/helpers.rs");
include!("session_manager/auto_route.rs");
include!("session_manager/tests.rs");
