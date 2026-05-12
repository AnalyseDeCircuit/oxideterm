// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    AnchoredPositionMode, AnyElement, App, AppContext, Bounds, ClipboardItem, Context, Corner,
    Entity, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    Point, Render, SharedString, Styled, Timer, UniformListScrollHandle, Window, anchored,
    deferred, div, prelude::*, px, rgb, rgba, svg, uniform_list,
};
use oxideterm_editor_syntax::LanguageId;
use oxideterm_gpui_editor::TextEditorView;
use oxideterm_gpui_ui::{
    button::ButtonVariant,
    button::{ButtonOptions, ButtonRadius, ButtonSize, button_with},
    modal::{
        dialog_backdrop, dialog_content, dialog_description, dialog_footer, dialog_header,
        dialog_title, popover_backdrop,
    },
    select::{SelectAnchorId, select_anchor_probe},
    tauri_ui_font_family,
};
use oxideterm_ide_core::{
    AsyncIdeFileSystem, CloseRequestId, DirtyCloseDecision, EditorTabId, FileKind, FileTreeEntry,
    IdeFileCheck, IdeFileError, IdeFileErrorKind, IdeLocation, IdeWorkspace, SavedFileVersion,
    WorkspaceSnapshot, WriteMode,
};
use oxideterm_ide_fs::{AgentStatus, IdeSearchMatch, NodeAgentIdeFileSystem, NodeAgentMode};
use oxideterm_ssh::{NodeRouter, ReconnectIdeSnapshot};
use oxideterm_theme::ThemeTokens;

use crate::{file_icons, labels::IdeLabels};

// Tauri IdeWorkspace.tsx uses a 280px default with 200px/500px resize bounds.
const IDE_TREE_DEFAULT_WIDTH: f32 = 280.0;
const IDE_TREE_MIN_WIDTH: f32 = 200.0;
const IDE_TREE_MAX_WIDTH: f32 = 500.0;
const IDE_STATUS_BAR_HEIGHT: f32 = 24.0;
const IDE_TAB_PADDING_X: f32 = 12.0;
const IDE_TAB_PADDING_Y: f32 = 6.0;
const IDE_ICON_SIZE: f32 = 16.0;
const IDE_EMPTY_ICON_SIZE: f32 = 64.0;
const IDE_ROW_HEIGHT: f32 = 22.0;
const IDE_TREE_INDENT_STEP: f32 = 12.0;
const IDE_FILE_ICON_SIZE: f32 = 14.0;
const IDE_TREE_TOOLBAR_BUTTON_SIZE: f32 = 24.0;
const IDE_TREE_TOOLBAR_ICON_SIZE: f32 = 14.0;

// Named alpha constants preserve the Tailwind source classes:
// bg-theme-bg/50, hover:bg-theme-bg-hover/30, border-theme-border/50,
// and the disconnected overlay's bg-black/50.
const IDE_BG_HALF_ALPHA: u32 = 0x80;
const IDE_BG_ACTIVE_THEME_ALPHA: u32 = 0x66;
const IDE_HOVER_ALPHA: u32 = 0x4d;
const IDE_BORDER_HALF_ALPHA: u32 = 0x80;
const IDE_OVERLAY_ALPHA: u32 = 0x80;
const IDE_TREE_SELECTED_ALPHA: u32 = 0x1a;

// Tauri `IdeRemoteFolderDialog.tsx` source classes translated to named
// constants: sm:max-w-lg, px-4, space-y-4, h-64, p-1, px-2 py-1.5,
// bg-theme-accent/20, and hover:bg-theme-bg-hover.
const IDE_FOLDER_DIALOG_WIDTH: f32 = 512.0;
const IDE_FOLDER_DIALOG_BODY_PADDING_X: f32 = 16.0;
const IDE_FOLDER_DIALOG_BODY_GAP: f32 = 16.0;
const IDE_FOLDER_DIALOG_LIST_HEIGHT: f32 = 256.0;
const IDE_FOLDER_DIALOG_LIST_PADDING: f32 = 4.0;
const IDE_FOLDER_DIALOG_ROW_PADDING_X: f32 = 8.0;
const IDE_FOLDER_DIALOG_ROW_PADDING_Y: f32 = 6.0;
const IDE_FOLDER_DIALOG_ICON_SIZE: f32 = 16.0;
const IDE_FOLDER_DIALOG_SELECTED_ALPHA: u32 = 0x33;
const IDE_TAB_CONTEXT_MENU_WIDTH: f32 = 140.0;
const IDE_TAB_CONTEXT_MENU_PADDING_Y: f32 = 4.0;
const IDE_TAB_CONTEXT_MENU_ITEM_HEIGHT: f32 = 28.0;
const IDE_TAB_CONTEXT_MENU_Z: usize = 50;
const IDE_TAB_REORDER_ACTIVATION_PX: f32 = 5.0;
const IDE_TREE_CONTEXT_MENU_WIDTH: f32 = 180.0;
const IDE_TREE_CONTEXT_MENU_MAX_HEIGHT: f32 = 280.0;
const IDE_TREE_CONTEXT_MENU_PADDING_Y: f32 = 4.0;
const IDE_TREE_CONTEXT_MENU_ITEM_HEIGHT: f32 = 28.0;
const IDE_TREE_CONTEXT_MENU_Z: usize = 100;
const IDE_TREE_CONTEXT_MENU_SHORTCUT_SIZE: f32 = 10.0;
const IDE_TREE_CONTEXT_MENU_ICON_ALPHA: u32 = 0xb3;
const IDE_TREE_CONTEXT_MENU_DANGER_BG_ALPHA: u32 = 0x1a;
const IDE_AGENT_MENU_WIDTH: f32 = 180.0;
const IDE_AGENT_MENU_MANUAL_WIDTH: f32 = 300.0;
const IDE_AGENT_MENU_PADDING_Y: f32 = 4.0;
const IDE_AGENT_MENU_DESCRIPTION_PADDING_X: f32 = 8.0;
const IDE_AGENT_MENU_DESCRIPTION_PADDING_Y: f32 = 6.0;
const IDE_AGENT_MENU_ITEM_HEIGHT: f32 = 28.0;
const IDE_AGENT_MENU_Z: usize = 110;
const IDE_AGENT_OPT_IN_WIDTH: f32 = 384.0;
const IDE_AGENT_OPT_IN_ICON_SIZE: f32 = 48.0;
const IDE_AGENT_OPT_IN_ICON_INNER_SIZE: f32 = 24.0;
const IDE_AGENT_OPT_IN_BODY_PADDING_X: f32 = 24.0;
const IDE_AGENT_OPT_IN_BODY_PADDING_TOP: f32 = 24.0;
const IDE_AGENT_OPT_IN_BODY_PADDING_BOTTOM: f32 = 16.0;
const IDE_AGENT_OPT_IN_GAP: f32 = 12.0;
const IDE_AGENT_OPT_IN_ACTION_PADDING_Y: f32 = 10.0;
const IDE_AGENT_OPT_IN_BORDER_ALPHA: u32 = 0x99;
const IDE_AGENT_OPT_IN_ACCENT_BG_ALPHA: u32 = 0x1a;
const IDE_AGENT_OPT_IN_ACCENT_BORDER_ALPHA: u32 = 0x33;
const IDE_AGENT_POLL_READY_SECS: u64 = 5;
const IDE_AGENT_POLL_DEPLOYING_SECS: u64 = 2;
const IDE_AGENT_POLL_MANUAL_SECS: u64 = 10;
const IDE_AGENT_WATCH_RETRY_SECS: u64 = 3;
const IDE_SEARCH_DEBOUNCE_MS: u64 = 300;
const IDE_SEARCH_CACHE_TTL_SECS: u64 = 60;
const IDE_SEARCH_CACHE_MAX_ENTRIES: usize = 50;
const IDE_SEARCH_MAX_RESULTS: u32 = 200;
const TAILWIND_RED_400: u32 = 0xf87171;
const TAILWIND_RED_500: u32 = 0xef4444;
const TAILWIND_EMERALD_400: u32 = 0x34d399;
const TAILWIND_AMBER_400: u32 = 0xfbbf24;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdeSurfaceEvent {
    RememberAgentMode(NodeAgentMode),
    ProjectOpened,
    ReconnectRestoreProjectOpened {
        reconnect_node_id: String,
    },
    ReconnectRestoreProjectFailed {
        reconnect_node_id: String,
        message: String,
    },
}

#[derive(Clone, Debug, Default)]
struct FolderPickerState {
    open: bool,
    node_id: Option<String>,
    current_path: String,
    path_input: String,
    folders: Vec<FileTreeEntry>,
    loading: bool,
    error: Option<String>,
    selected_folder: Option<String>,
    path_input_focused: bool,
    generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdeLoadState {
    Empty,
    Loading,
    Ready,
    Error(String),
    Disconnected,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdeRuntimeSettings {
    pub auto_save: bool,
    pub editor_font_size: f32,
    pub editor_line_height: f32,
    pub word_wrap: bool,
    pub background_active: bool,
    pub agent_mode: NodeAgentMode,
}

impl Default for IdeRuntimeSettings {
    fn default() -> Self {
        Self {
            auto_save: false,
            editor_font_size: 14.0,
            editor_line_height: 1.2,
            word_wrap: false,
            background_active: false,
            agent_mode: NodeAgentMode::Ask,
        }
    }
}

#[derive(Clone, Debug)]
struct ProjectOpenResult {
    node_id: String,
    root: IdeLocation,
    title: String,
    git_branch: Option<String>,
    children: Vec<FileTreeEntry>,
}

#[derive(Clone, Debug)]
struct FileOpenResult {
    location: IdeLocation,
    text: String,
    version: SavedFileVersion,
}

#[derive(Clone, Debug)]
struct TreeRenderRow {
    entry: FileTreeEntry,
    depth: usize,
    expanded: bool,
}

#[derive(Clone, Debug)]
struct TreeRowsCache {
    root_key: String,
    tree_revision: u64,
    rows: Arc<Vec<TreeRenderRow>>,
}

#[derive(Clone, Debug)]
struct SearchResultGroup {
    path: String,
    matches: Vec<IdeSearchMatch>,
}

#[derive(Clone, Debug)]
struct SearchCacheEntry {
    results: Vec<SearchResultGroup>,
    timestamp: Instant,
    truncated: bool,
}

#[derive(Clone, Debug, Default)]
struct ProjectSearchState {
    open: bool,
    query: String,
    results: Vec<SearchResultGroup>,
    searching: bool,
    error: Option<String>,
    expanded_paths: HashSet<String>,
    truncated: bool,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabContextMenu {
    tab_id: EditorTabId,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct TreeContextMenu {
    location: IdeLocation,
    is_directory: bool,
    name: String,
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TreeNameInputKind {
    NewFile,
    NewFolder,
    Rename,
}

#[derive(Clone, Debug, PartialEq)]
struct TreeNameInputState {
    kind: TreeNameInputKind,
    target: IdeLocation,
    parent_path: String,
    original_name: Option<String>,
    value: String,
    error: Option<String>,
    submitting: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct DeleteConfirmState {
    location: IdeLocation,
    name: String,
    is_directory: bool,
    affected_tab_count: usize,
    unsaved_tab_count: usize,
    deleting: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AgentStatusMenu {
    trigger_bounds: Bounds<Pixels>,
}

#[derive(Clone, Debug, PartialEq)]
struct ConflictState {
    tab_id: EditorTabId,
    title: String,
    local_mtime: Option<i64>,
    remote_mtime: Option<i64>,
    close_request: Option<CloseRequestId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentActionKind {
    Deploy,
    Remove,
    Refresh,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabDrag {
    tab_id: EditorTabId,
    start_position: Point<Pixels>,
    over_tab_id: EditorTabId,
    activated: bool,
}

/// GPUI IDE owner.
///
/// This is the native equivalent of Tauri's `IdeWorkspace` + `ideStore` owner:
/// project state lives here, while terminal panes remain optional consumers of
/// the same node. SFTP is acquired through `NodeRouter`, never through an open
/// terminal tab, so reconnect restore has a real IDE surface to target.
pub struct IdeSurface {
    workspace: IdeWorkspace,
    fs: NodeAgentIdeFileSystem,
    tokens: ThemeTokens,
    labels: IdeLabels,
    runtime_settings: IdeRuntimeSettings,
    focus_handle: FocusHandle,
    backend_runtime: Arc<tokio::runtime::Runtime>,
    load_state: IdeLoadState,
    node_id: Option<String>,
    root_path: Option<String>,
    git_branch: Option<String>,
    tree_width: f32,
    generation: u64,
    editors: HashMap<EditorTabId, Entity<TextEditorView>>,
    loading_paths: HashSet<String>,
    loading_file_tabs: HashSet<EditorTabId>,
    saving_tabs: HashSet<EditorTabId>,
    save_after_close: Option<CloseRequestId>,
    conflict_state: Option<ConflictState>,
    pending_restore_files: Vec<String>,
    pending_restore_dirty_contents: BTreeMap<String, String>,
    pending_reconnect_restore_node_id: Option<String>,
    pending_reconnect_restore_files_remaining: usize,
    last_error: Option<String>,
    folder_picker: FolderPickerState,
    folder_switch_confirm_open: bool,
    tree_rows_cache: Option<TreeRowsCache>,
    tree_scroll_handle: UniformListScrollHandle,
    search: ProjectSearchState,
    search_cache: HashMap<String, SearchCacheEntry>,
    search_cache_order: Vec<String>,
    pending_search_queries: BTreeMap<String, String>,
    tab_context_menu: Option<TabContextMenu>,
    tree_context_menu: Option<TreeContextMenu>,
    tree_name_input: Option<TreeNameInputState>,
    delete_confirm: Option<DeleteConfirmState>,
    tab_drag: Option<TabDrag>,
    agent_opt_in_open: bool,
    agent_opt_in_remember: bool,
    agent_status_menu: Option<AgentStatusMenu>,
    agent_status_trigger_bounds: Option<Bounds<Pixels>>,
    agent_remove_confirm_open: bool,
    agent_action: Option<AgentActionKind>,
    agent_poll_generation: u64,
    agent_watch_generation: u64,
    watched_root_path: Option<String>,
}

include!("surface/lifecycle.rs");
include!("surface/folder_picker.rs");
include!("surface/editor_actions.rs");
include!("surface/render_root.rs");
include!("surface/render_tree.rs");
include!("surface/render_tabs.rs");
include!("surface/render_agent.rs");
include!("surface/render_dialogs.rs");
include!("surface/render_helpers.rs");
include!("surface/tree_row.rs");
include!("surface/helpers.rs");
