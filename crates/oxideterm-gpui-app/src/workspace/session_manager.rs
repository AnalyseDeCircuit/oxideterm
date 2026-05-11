use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Datelike, Local, Utc};
use gpui::{StatefulInteractiveElement, prelude::*};
use oxideterm_connections::{
    AuthType, ConnectionAuthDraft, ConnectionAuthDraftKind, ConnectionDraft, ConnectionInfo,
    ConnectionStore, ProxyHopDraft, SaveConnectionRequest, SavedAuth, SavedConnection,
    SavedProxyHop, SecretString, SshConfigHost, list_ssh_config_hosts, resolve_ssh_config_alias,
    save_request_from_draft, saved_connection_from_ssh_host,
};
use oxideterm_gpui_ui::{
    IconBadgeMetrics, TauriTableCellOptions, TauriTableCellStyle, TauriTableColors,
    TauriTableMetrics,
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with},
    checkbox, icon_badge,
    modal::{dialog_backdrop, popover_backdrop},
    modal_body, modal_container, modal_footer, modal_overlay,
    surface::{color_for_background, color_for_background_or_alpha},
    tauri_table_cell, tauri_table_checkbox_cell, tauri_table_header, tauri_table_row,
    tauri_table_sort_header, tauri_table_spacer_cell,
    text_input::{text_caret, text_input_anchor_probe},
};
use oxideterm_ssh::{AuthMethod, ProxyHopConfig};

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
const MANAGER_RESPONSIVE_SM: f32 = 640.0;
const MANAGER_RESPONSIVE_MD: f32 = 768.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum SessionManagerInput {
    Search,
    SavedSearch,
    NewGroup,
    AutoRouteDisplayName,
}

impl SessionManagerInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
            Self::SavedSearch => 2,
            Self::NewGroup => 3,
            Self::AutoRouteDisplayName => 4,
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
    pub(super) row_menu_opens_above: bool,
    pub(super) row_context_menu_connection_id: Option<String>,
    pub(super) row_context_menu_x: f32,
    pub(super) row_context_menu_y: f32,
    pub(super) expanded_groups: HashSet<String>,
    pub(super) focused_input: Option<SessionManagerInput>,
    pub(super) show_new_group: bool,
    pub(super) new_group_name: String,
    pub(super) show_import: bool,
    pub(super) ssh_config_hosts: Vec<SshConfigHost>,
    pub(super) selected_import_aliases: HashSet<String>,
    pub(super) show_batch_move: bool,
    pub(super) status: Option<String>,
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
            row_menu_opens_above: false,
            row_context_menu_connection_id: None,
            row_context_menu_x: 0.0,
            row_context_menu_y: 0.0,
            expanded_groups: HashSet::new(),
            focused_input: None,
            show_new_group: false,
            new_group_name: String::new(),
            show_import: false,
            ssh_config_hosts: Vec::new(),
            selected_import_aliases: HashSet::new(),
            show_batch_move: false,
            status: None,
        }
    }
}

include!("session_manager/surface.rs");
include!("session_manager/tree.rs");
include!("session_manager/table.rs");
include!("session_manager/controls.rs");
include!("session_manager/dialogs.rs");
include!("session_manager/actions.rs");
include!("session_manager/helpers.rs");
include!("session_manager/auto_route.rs");
include!("session_manager/tests.rs");
