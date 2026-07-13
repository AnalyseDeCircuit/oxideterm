use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    path::PathBuf,
    time::Duration,
};

use crate::workspace::new_connection::{
    NewConnectionUpstreamProxyAuth, NewConnectionUpstreamProxyPolicy,
};
use crate::workspace::quick_commands::QuickCommandImportStrategy;
use crate::workspace::session_icons;
use chrono::{DateTime, Datelike, Local, Utc};
use gpui::{Div, prelude::*, rgba};
use oxideterm_connections::{
    AuthType, ConnectionAuthDraft, ConnectionAuthDraftKind, ConnectionDraft, ConnectionInfo,
    ConnectionStore, ProxyHopDraft, RawTcpProfile, RawUdpProfile, SaveConnectionRequest, SavedAuth,
    SavedConnection, SavedUpstreamProxyAuth, SavedUpstreamProxyConfig, SavedUpstreamProxyPolicy,
    SavedUpstreamProxyProtocol, SecretString, SerialProfile, TelnetProfile,
    oxide_file::{
        ExportPreflightResult, ForwardDetail, ImportConflictStrategy, ImportPreview,
        ImportResultEnvelope, OxideExportOptions, OxideFile, OxideFileError, OxideForwardRecord,
        OxideImportOptions, OxideMetadata, apply_oxide_import_with_options_with_progress,
        export_connections_to_oxide_with_progress, preflight_export,
        preview_oxide_import_with_progress,
    },
    save_request_from_draft,
};
use oxideterm_forwarding::{ForwardType, OwnedForwardImportRecord, PersistedForward};
use oxideterm_gpui_ui::{
    ConfirmDialogVariant, ConfirmDialogView,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonIconPosition, ToolbarButtonOptions,
    },
    checkbox, confirm_dialog,
    context_menu::{ContextMenuActionableStyle, context_menu_event_boundary},
    dropdown_menu::{DropdownMenuItemKind, dropdown_menu_content, dropdown_menu_item},
    modal::{dismissible_dialog_backdrop, modal_backdrop, overlay_content_boundary},
    modal_body, modal_container, modal_footer, modal_overlay,
    surface::{color_for_background, color_for_background_or_alpha},
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
const SESSION_MANAGER_LIGHT_DIALOG_BACKDROP_ALPHA: u32 = 0x66; // Keep lightweight manager dialogs readable without heavy blur.
const MANAGER_TOOLBAR_SEARCH_WIDTH: f32 = 384.0; // Tauri max-w-sm
const MANAGER_ROW_TEXT_SIZE: f32 = 14.0;
const MANAGER_ROW_META_TEXT_SIZE: f32 = 12.0;
const MANAGER_TABLE_HEADER_TEXT_SIZE: f32 = 12.0;
const MANAGER_ROW_ACTION_BUTTON: f32 = 24.0; // Tauri h-6 w-6
const MANAGER_VIEW_MODE_MENU_WIDTH: f32 = 168.0; // Tauri DropdownMenuContent min-w-[160px] plus native menu padding.
const MANAGER_VIEW_MODE_MENU_HEIGHT: f32 = 104.0; // Three compact radio rows plus menu padding.
const MANAGER_SORT_MENU_WIDTH: f32 = 184.0; // Sort fields reuse the compact toolbar dropdown rhythm.
const MANAGER_SORT_MENU_HEIGHT: f32 = 220.0; // Seven compact radio rows plus menu padding.
const MANAGER_BATCH_MOVE_MENU_WIDTH: f32 = 220.0; // Tauri batch move DropdownMenuContent natural width.
const MANAGER_BATCH_MOVE_MENU_HEIGHT: f32 = 260.0; // Keeps long group lists scrollable without covering the viewport.
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
    RawTcpProfile { id: String, name: String },
    RawUdpProfile { id: String, name: String },
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
    pub(super) imported_raw_tcp_profiles: usize,
    pub(super) skipped_raw_tcp_profiles: usize,
    pub(super) imported_raw_udp_profiles: usize,
    pub(super) skipped_raw_udp_profiles: usize,
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

#[derive(Clone, Debug)]
pub(super) struct SessionManagerState {
    pub(super) selected_group: Option<String>,
    pub(super) view_mode: SessionManagerViewMode,
    pub(super) sort_field: SessionSortField,
    pub(super) sort_direction: SortDirection,
    pub(super) search_query: String,
    pub(super) saved_search_query: String,
    pub(super) selected_ids: HashSet<String>,
    pub(super) view_mode_menu_open: bool,
    pub(super) sort_menu_open: bool,
    pub(super) expanded_groups: HashSet<String>,
    pub(super) focused_input: Option<SessionManagerInput>,
    pub(super) show_new_group: bool,
    pub(super) new_group_name: String,
    pub(super) focused_basic_dialog_footer_action: Option<SessionManagerBasicDialogFooterAction>,
    pub(super) show_batch_move: bool,
    pub(super) delete_confirm: Option<SessionManagerDeleteConfirm>,
    pub(super) oxide_import_dialog: Option<OxideImportDialogState>,
    pub(super) oxide_export_dialog: Option<OxideExportDialogState>,
    pub(super) status: Option<String>,
    pub(super) saved_sidebar_scroll_handle: UniformListScrollHandle,
}

impl Default for SessionManagerState {
    fn default() -> Self {
        Self {
            selected_group: None,
            view_mode: SessionManagerViewMode::Grid,
            sort_field: SessionSortField::LastUsed,
            sort_direction: SortDirection::Desc,
            search_query: String::new(),
            saved_search_query: String::new(),
            selected_ids: HashSet::new(),
            view_mode_menu_open: false,
            sort_menu_open: false,
            expanded_groups: HashSet::new(),
            focused_input: None,
            show_new_group: false,
            new_group_name: String::new(),
            focused_basic_dialog_footer_action: None,
            show_batch_move: false,
            delete_confirm: None,
            oxide_import_dialog: None,
            oxide_export_dialog: None,
            status: None,
            saved_sidebar_scroll_handle: UniformListScrollHandle::new(),
        }
    }
}

#[derive(Clone)]
pub(super) struct OxideImportDialogState {
    pub(super) presence: oxideterm_gpui_ui::motion::ExitPresence,
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
    pub(super) import_raw_tcp_profiles: bool,
    pub(super) import_raw_udp_profiles: bool,
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
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
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
            import_raw_tcp_profiles: true,
            import_raw_udp_profiles: true,
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
            .field("import_raw_tcp_profiles", &self.import_raw_tcp_profiles)
            .field("import_raw_udp_profiles", &self.import_raw_udp_profiles)
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
    pub(super) presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) selected_ids: HashSet<String>,
    pub(super) available_forwards: Vec<PersistedForward>,
    pub(super) selected_forward_ids: HashSet<String>,
    pub(super) include_app_settings: bool,
    pub(super) selected_app_settings_sections: HashSet<String>,
    pub(super) include_local_terminal_env_vars: bool,
    pub(super) include_quick_commands: bool,
    pub(super) include_serial_profiles: bool,
    pub(super) include_raw_tcp_profiles: bool,
    pub(super) include_raw_udp_profiles: bool,
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
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
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
            include_raw_tcp_profiles: true,
            include_raw_udp_profiles: true,
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
            .field("include_raw_tcp_profiles", &self.include_raw_tcp_profiles)
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
mod auto_route;
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
    actions::*, auto_route::*, controls::*, dialogs::*, helpers::*, oxide_actions::*,
    oxide_dialog_common::*, oxide_dialog_helpers::*, oxide_export_dialogs::*,
    oxide_export_selection_dialogs::*, oxide_export_summary_dialogs::*, oxide_import_dialogs::*,
    oxide_import_preview_dialogs::*, oxide_import_result_dialogs::*, surface::*, tree::*, views::*,
};

// Preserve the workspace-facing session manager API at its original visibility.
pub(in crate::workspace) use self::{
    auto_route::AutoRouteModalState,
    helpers::{
        duplicate_connection_template_name, form_from_saved_connection, save_request_from_form,
        save_request_from_form_with_existing_auth, upstream_proxy_config_from_form,
    },
};

#[cfg(test)]
mod tests;
