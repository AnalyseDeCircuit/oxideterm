// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::Duration,
};

use gpui::{
    AnchoredPositionMode, AnyElement, App, AppContext, ClipboardItem, Context, Corner, Entity,
    EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    Point, Render, SharedString, Styled, Timer, Window, anchored, deferred, div, prelude::*, px,
    rgb, rgba, svg,
};
use oxideterm_editor_syntax::LanguageId;
use oxideterm_gpui_editor::TextEditorView;
use oxideterm_gpui_ui::{
    button::ButtonVariant,
    button::{ButtonOptions, ButtonRadius, ButtonSize, button_with},
    modal::{dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title},
    tauri_ui_font_family,
};
use oxideterm_ide_core::{
    AsyncIdeFileSystem, CloseRequestId, DirtyCloseDecision, EditorTabId, FileKind, FileTreeEntry,
    IdeFileCheck, IdeLocation, IdeWorkspace, SavedFileVersion, WorkspaceSnapshot, WriteMode,
};
use oxideterm_ide_fs::{AgentStatus, NodeAgentIdeFileSystem, NodeAgentMode};
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
const IDE_MODAL_BACKDROP_ALPHA: u32 = 0xcc;
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
const TAILWIND_RED_400: u32 = 0xf87171;
const TAILWIND_RED_500: u32 = 0xef4444;
const TAILWIND_EMERALD_400: u32 = 0x34d399;
const TAILWIND_AMBER_400: u32 = 0xfbbf24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeSurfaceEvent {
    RememberAgentMode(NodeAgentMode),
    ProjectOpened,
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct AgentStatusMenu {
    x: f32,
    y: f32,
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
    saving_tabs: HashSet<EditorTabId>,
    save_after_close: Option<CloseRequestId>,
    pending_restore_files: Vec<String>,
    pending_restore_dirty_contents: BTreeMap<String, String>,
    last_error: Option<String>,
    folder_picker: FolderPickerState,
    folder_switch_confirm_open: bool,
    tree_rows_cache: Option<TreeRowsCache>,
    tab_context_menu: Option<TabContextMenu>,
    tree_context_menu: Option<TreeContextMenu>,
    tab_drag: Option<TabDrag>,
    agent_opt_in_open: bool,
    agent_opt_in_remember: bool,
    agent_status_menu: Option<AgentStatusMenu>,
    agent_remove_confirm_open: bool,
    agent_action: Option<AgentActionKind>,
    agent_poll_generation: u64,
}

impl IdeSurface {
    pub fn new(
        router: NodeRouter,
        tokens: ThemeTokens,
        labels: IdeLabels,
        runtime_settings: IdeRuntimeSettings,
        backend_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            workspace: IdeWorkspace::new(),
            fs: NodeAgentIdeFileSystem::new(router, runtime_settings.agent_mode),
            tokens,
            labels,
            runtime_settings,
            focus_handle: cx.focus_handle(),
            backend_runtime,
            load_state: IdeLoadState::Empty,
            node_id: None,
            root_path: None,
            git_branch: None,
            tree_width: IDE_TREE_DEFAULT_WIDTH,
            generation: 0,
            editors: HashMap::new(),
            loading_paths: HashSet::new(),
            saving_tabs: HashSet::new(),
            save_after_close: None,
            pending_restore_files: Vec::new(),
            pending_restore_dirty_contents: BTreeMap::new(),
            last_error: None,
            folder_picker: FolderPickerState::default(),
            folder_switch_confirm_open: false,
            tree_rows_cache: None,
            tab_context_menu: None,
            tree_context_menu: None,
            tab_drag: None,
            agent_opt_in_open: false,
            agent_opt_in_remember: false,
            agent_status_menu: None,
            agent_remove_confirm_open: false,
            agent_action: None,
            agent_poll_generation: 0,
        }
    }

    pub fn load_state(&self) -> &IdeLoadState {
        &self.load_state
    }

    pub fn set_visual_and_runtime_settings(
        &mut self,
        tokens: ThemeTokens,
        runtime_settings: IdeRuntimeSettings,
        cx: &mut Context<Self>,
    ) {
        self.tokens = tokens;
        self.runtime_settings = runtime_settings;
        self.fs.set_mode(runtime_settings.agent_mode);
        if runtime_settings.agent_mode != NodeAgentMode::Ask {
            self.agent_opt_in_open = false;
        }
        for editor in self.editors.values() {
            apply_editor_runtime_settings(editor, self.tokens, self.runtime_settings, cx);
        }
        cx.notify();
    }

    pub fn snapshot(&mut self, cx: &mut Context<Self>) -> Option<WorkspaceSnapshot> {
        self.sync_all_editors(cx);
        self.workspace.snapshot().ok()
    }

    pub fn reconnect_snapshot(&mut self, cx: &mut Context<Self>) -> Option<ReconnectIdeSnapshot> {
        self.sync_all_editors(cx);
        let snapshot = self.workspace.snapshot().ok()?;
        let (connection_id, project_path) = match &snapshot.project.root {
            IdeLocation::Remote { node_id, path } => (node_id.clone(), path.clone()),
            IdeLocation::Local { .. } => return None,
        };
        let tab_paths = snapshot
            .tabs
            .iter()
            .filter_map(|tab| match &tab.location {
                IdeLocation::Remote { path, .. } => Some(path.clone()),
                IdeLocation::Local { .. } => None,
            })
            .collect::<Vec<_>>();
        let dirty_contents = snapshot
            .buffers
            .iter()
            .filter(|buffer| {
                buffer.revision != buffer.saved_revision || buffer.text != buffer.saved_text
            })
            .filter_map(|buffer| match &buffer.location {
                IdeLocation::Remote { path, .. } => Some((path.clone(), buffer.text.clone())),
                IdeLocation::Local { .. } => None,
            })
            .collect::<BTreeMap<_, _>>();

        Some(ReconnectIdeSnapshot {
            project_path,
            tab_paths,
            connection_id,
            dirty_contents,
        })
    }

    pub fn open_remote_project(
        &mut self,
        node_id: impl Into<String>,
        root_path: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let node_id = node_id.into();
        let root_path = root_path.into();
        if self.pending_restore_files.is_empty() {
            self.pending_restore_dirty_contents.clear();
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.node_id = Some(node_id.clone());
        self.root_path = Some(root_path.clone());
        self.git_branch = None;
        self.load_state = IdeLoadState::Loading;
        self.last_error = None;
        self.editors.clear();
        self.workspace = IdeWorkspace::new();
        cx.notify();

        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(backend_runtime.spawn(async move {
                open_project_with_root_listing(fs, node_id, root_path).await
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(result) => this.apply_project_open(result, cx),
                    Err(error) => {
                        this.load_state = IdeLoadState::Error(error.message);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub fn open_remote_project_with_files(
        &mut self,
        node_id: impl Into<String>,
        root_path: impl Into<String>,
        file_paths: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.pending_restore_files = file_paths;
        self.pending_restore_dirty_contents.clear();
        self.open_remote_project(node_id, root_path, cx);
    }

    pub fn restore_reconnect_snapshot(
        &mut self,
        snapshot: ReconnectIdeSnapshot,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_all_editors(cx);
        let same_project_open = self.root_path.as_deref() == Some(snapshot.project_path.as_str())
            && self.node_id.as_deref() == Some(snapshot.connection_id.as_str());

        if self.root_path.is_some() && !same_project_open {
            return false;
        }

        self.pending_restore_dirty_contents = snapshot.dirty_contents;
        if same_project_open {
            for path in snapshot.tab_paths {
                self.open_remote_file(
                    IdeLocation::remote(snapshot.connection_id.clone(), path),
                    cx,
                );
            }
        } else {
            self.pending_restore_files = snapshot.tab_paths;
            self.open_remote_project(snapshot.connection_id, snapshot.project_path, cx);
        }
        true
    }

    pub fn open_remote_folder_picker_for_node(
        &mut self,
        node_id: impl Into<String>,
        initial_path: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let node_id = node_id.into();
        let initial_path = normalize_remote_path(&initial_path.into());
        self.node_id = Some(node_id.clone());
        self.folder_picker.open = true;
        self.folder_picker.node_id = Some(node_id.clone());
        self.folder_picker.path_input_focused = true;
        self.load_folder_picker_path(node_id, initial_path, cx);
    }

    fn request_open_folder_picker(&mut self, cx: &mut Context<Self>) {
        self.sync_all_editors(cx);
        if self.workspace.has_dirty_buffers() {
            self.folder_switch_confirm_open = true;
            cx.notify();
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let initial_path = self.root_path.clone().unwrap_or_else(|| "/".to_string());
        self.open_remote_folder_picker_for_node(node_id, initial_path, cx);
    }

    fn load_folder_picker_current(&mut self, cx: &mut Context<Self>) {
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        let path = self.folder_picker.current_path.clone();
        self.load_folder_picker_path(node_id, path, cx);
    }

    fn load_folder_picker_path(
        &mut self,
        node_id: String,
        path: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let path = normalize_remote_path(&path.into());
        self.folder_picker.open = true;
        self.folder_picker.node_id = Some(node_id.clone());
        self.folder_picker.current_path = path.clone();
        self.folder_picker.path_input = path.clone();
        self.folder_picker.loading = true;
        self.folder_picker.error = None;
        self.folder_picker.selected_folder = None;
        self.folder_picker.generation = self.folder_picker.generation.wrapping_add(1);
        let generation = self.folder_picker.generation;
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let path_for_task = path.clone();
            let result = await_ide_backend(backend_runtime.spawn(async move {
                let location = IdeLocation::remote(node_id, path_for_task);
                fs.list_dir(&location).await.map(folder_picker_dirs)
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                // The Tauri dialog resets async state on every path change. The
                // generation guard gives GPUI the same observable behavior when
                // an older SFTP list returns after a newer navigation request.
                if this.folder_picker.generation != generation {
                    return;
                }
                this.folder_picker.loading = false;
                match result {
                    Ok(folders) => {
                        this.folder_picker.error = None;
                        this.folder_picker.current_path = path;
                        this.folder_picker.path_input = this.folder_picker.current_path.clone();
                        this.folder_picker.folders = folders;
                        this.folder_picker.selected_folder = None;
                    }
                    Err(error) => this.folder_picker.error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn enter_folder_picker_folder(&mut self, folder_name: &str, cx: &mut Context<Self>) {
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        let path = join_remote_child(&self.folder_picker.current_path, folder_name);
        self.load_folder_picker_path(node_id, path, cx);
    }

    fn go_folder_picker_parent(&mut self, cx: &mut Context<Self>) {
        if self.folder_picker.current_path == "/" || self.folder_picker.loading {
            return;
        }
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        let parent = parent_remote_path(&self.folder_picker.current_path);
        self.load_folder_picker_path(node_id, parent, cx);
    }

    fn go_folder_picker_home(&mut self, cx: &mut Context<Self>) {
        if self.folder_picker.loading {
            return;
        }
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        self.load_folder_picker_path(node_id, "/", cx);
    }

    fn submit_folder_picker_path(&mut self, cx: &mut Context<Self>) {
        if self.folder_picker.loading {
            return;
        }
        let path = self.folder_picker.path_input.trim().to_string();
        if path.is_empty() {
            return;
        }
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        self.load_folder_picker_path(node_id, path, cx);
    }

    fn selected_folder_picker_path(&self) -> String {
        match self.folder_picker.selected_folder.as_deref() {
            Some(name) => join_remote_child(&self.folder_picker.current_path, name),
            None => self.folder_picker.current_path.clone(),
        }
    }

    fn confirm_folder_picker(&mut self, cx: &mut Context<Self>) {
        if self.folder_picker.loading {
            return;
        }
        let Some(node_id) = self.folder_picker.node_id.clone() else {
            return;
        };
        let final_path = self.selected_folder_picker_path();
        self.folder_picker.open = false;
        self.folder_picker.path_input_focused = false;
        self.open_remote_project(node_id, final_path, cx);
    }

    fn close_folder_picker(&mut self, cx: &mut Context<Self>) {
        self.folder_picker.open = false;
        self.folder_picker.path_input_focused = false;
        cx.notify();
    }

    fn handle_folder_picker_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.folder_picker.open {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_folder_picker(cx),
            "enter" => self.submit_folder_picker_path(cx),
            "backspace" if self.folder_picker.path_input_focused => {
                self.folder_picker.path_input.pop();
                cx.notify();
            }
            _ if self.folder_picker.path_input_focused => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && !text.is_empty()
                    && !text.chars().any(char::is_control)
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                {
                    self.folder_picker.path_input.push_str(text);
                    cx.notify();
                }
            }
            _ => {}
        }
        cx.stop_propagation();
    }

    pub fn project_root_path(&self) -> Option<String> {
        self.root_path.clone()
    }

    pub fn open_file_paths(&self) -> Vec<String> {
        self.workspace
            .tabs()
            .iter()
            .filter_map(|tab| match &tab.location {
                IdeLocation::Remote { path, .. } => Some(path.clone()),
                IdeLocation::Local { .. } => None,
            })
            .collect()
    }

    pub fn retry_open_project(&mut self, cx: &mut Context<Self>) {
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        self.open_remote_project(node_id, root_path, cx);
    }

    pub fn restore_snapshot(&mut self, snapshot: WorkspaceSnapshot, cx: &mut Context<Self>) {
        let node_id = match &snapshot.project.root {
            IdeLocation::Remote { node_id, .. } => node_id.clone(),
            IdeLocation::Local { .. } => return,
        };
        let root_path = match &snapshot.project.root {
            IdeLocation::Remote { path, .. } => path.clone(),
            IdeLocation::Local { .. } => return,
        };
        let buffers = snapshot.buffers.clone();
        let result = self.workspace.restore_snapshot(snapshot);
        if !matches!(
            result,
            oxideterm_ide_core::RestoreSnapshotResult::Restored { .. }
        ) {
            return;
        }

        self.node_id = Some(node_id);
        self.root_path = Some(root_path);
        self.load_state = IdeLoadState::Ready;
        self.editors.clear();
        for buffer in buffers {
            self.create_editor(buffer.tab_id, &buffer.location, buffer.text, cx);
        }
        self.refresh_agent_status(cx);
        self.schedule_next_agent_status_poll(cx);
        cx.notify();
    }

    fn apply_project_open(&mut self, result: ProjectOpenResult, cx: &mut Context<Self>) {
        let root = result.root.clone();
        self.workspace.open_project(root.clone(), result.title);
        let _ = self.workspace.set_tree_expanded(&root, true);
        let _ = self.workspace.set_tree_children(root, result.children);
        self.node_id = Some(result.node_id);
        self.git_branch = result.git_branch;
        self.load_state = IdeLoadState::Ready;
        self.agent_opt_in_open = self.runtime_settings.agent_mode == NodeAgentMode::Ask;
        self.refresh_agent_status(cx);
        self.schedule_next_agent_status_poll(cx);
        cx.emit(IdeSurfaceEvent::ProjectOpened);
        let node_id = self.node_id.clone();
        for path in std::mem::take(&mut self.pending_restore_files) {
            if let Some(node_id) = node_id.clone() {
                self.open_remote_file(IdeLocation::remote(node_id, path), cx);
            }
        }
        cx.notify();
    }

    fn load_directory(&mut self, directory: IdeLocation, cx: &mut Context<Self>) {
        let key = directory.stable_key();
        if self.loading_paths.contains(&key) {
            return;
        }
        self.loading_paths.insert(key.clone());
        let fs = self.fs.clone();
        let generation = self.generation;
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let directory_for_task = directory.clone();
            let result = await_ide_backend(backend_runtime.spawn(async move {
                fs.list_dir(&directory_for_task)
                    .await
                    .map(sort_tree_entries)
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_paths.remove(&key);
                match result {
                    Ok(children) => {
                        let _ = this.workspace.set_tree_expanded(&directory, true);
                        let _ = this.workspace.set_tree_children(directory, children);
                    }
                    Err(error) => this.last_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_tree_entry(&mut self, entry: FileTreeEntry, cx: &mut Context<Self>) {
        let _ = self
            .workspace
            .select_tree_entry(Some(entry.location.clone()));
        match entry.kind {
            FileKind::Directory => {
                if self.workspace.file_tree().is_expanded(&entry.location) {
                    let _ = self.workspace.set_tree_expanded(&entry.location, false);
                    cx.notify();
                } else {
                    self.load_directory(entry.location, cx);
                }
            }
            FileKind::File | FileKind::Symlink | FileKind::Other => {
                self.open_remote_file(entry.location, cx);
            }
        }
    }

    fn open_remote_file(&mut self, location: IdeLocation, cx: &mut Context<Self>) {
        if let Some(tab_id) = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.location == location)
            .map(|tab| tab.id)
        {
            let _ = self.workspace.set_active_tab(tab_id);
            self.apply_pending_reconnect_dirty_for_tab(tab_id, cx);
            cx.notify();
            return;
        }
        let key = location.stable_key();
        if self.loading_paths.contains(&key) {
            return;
        }
        self.loading_paths.insert(key.clone());
        let fs = self.fs.clone();
        let generation = self.generation;
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(backend_runtime.spawn({
                let location = location.clone();
                async move { open_text_file(fs, location).await }
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_paths.remove(&key);
                match result {
                    Ok(result) => {
                        let dirty_text = remote_path(&result.location)
                            .and_then(|path| this.pending_restore_dirty_contents.remove(path));
                        match this.workspace.open_file(
                            result.location.clone(),
                            result.text.clone(),
                            result.version,
                        ) {
                            Ok(outcome) => {
                                let tab_id = match outcome {
                                    oxideterm_ide_core::OpenFileOutcome::Opened(tab_id)
                                    | oxideterm_ide_core::OpenFileOutcome::Reused(tab_id) => tab_id,
                                };
                                this.create_editor(tab_id, &result.location, result.text, cx);
                                if let Some(dirty_text) = dirty_text {
                                    this.apply_reconnect_dirty_text(tab_id, dirty_text, cx);
                                }
                            }
                            Err(error) => this.last_error = Some(error.to_string()),
                        }
                    }
                    Err(error) => this.last_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn create_editor(
        &mut self,
        tab_id: EditorTabId,
        location: &IdeLocation,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let tokens = self.tokens;
        let runtime_settings = self.runtime_settings;
        let language = language_for_location(location, &text);
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(text, &tokens, cx);
            editor.apply_ide_runtime_settings(
                &tokens,
                runtime_settings.editor_font_size,
                runtime_settings.editor_line_height,
                runtime_settings.word_wrap,
                runtime_settings.background_active,
                cx,
            );
            editor.set_language(language, cx);
            editor
        });
        self.editors.insert(tab_id, editor);
    }

    fn apply_pending_reconnect_dirty_for_tab(
        &mut self,
        tab_id: EditorTabId,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self
            .workspace
            .buffer(tab_id)
            .and_then(|buffer| remote_path(&buffer.location).map(ToOwned::to_owned))
        else {
            return;
        };
        let Some(dirty_text) = self.pending_restore_dirty_contents.remove(&path) else {
            return;
        };
        self.apply_reconnect_dirty_text(tab_id, dirty_text, cx);
    }

    fn apply_reconnect_dirty_text(
        &mut self,
        tab_id: EditorTabId,
        dirty_text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.workspace.buffer(tab_id).cloned() else {
            return;
        };
        if buffer.is_dirty() || dirty_text == buffer.saved_text {
            return;
        }

        // Tauri only writes snapshot dirtyContents back into clean tabs. Native
        // keeps the same user-intent rule so edits made after the snapshot win.
        let _ = self
            .workspace
            .replace_buffer_text(tab_id, dirty_text.clone());
        if let Some(editor) = self.editors.get(&tab_id) {
            editor.update(cx, |editor, cx| {
                editor.replace_text_external(dirty_text, cx);
            });
        }
    }

    fn activate_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let previous = self.workspace.active_tab();
        if previous == Some(tab_id) {
            return;
        }
        // Tauri auto-saves the previously active dirty tab when activeTabId
        // changes. Window-blur save-all still needs a GPUI focus-loss hook.
        if self.runtime_settings.auto_save
            && let Some(previous_tab_id) = previous
            && self.is_tab_dirty(previous_tab_id, cx)
            && !self.saving_tabs.contains(&previous_tab_id)
        {
            self.save_tab(previous_tab_id, cx);
        }
        let _ = self.workspace.set_active_tab(tab_id);
        cx.notify();
    }

    fn close_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.sync_editor_to_workspace(tab_id, cx);
        match self.workspace.request_close_tab(tab_id) {
            Ok(None) => {
                self.editors.remove(&tab_id);
                cx.notify();
            }
            Ok(Some(_)) => cx.notify(),
            Err(error) => {
                self.last_error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn toggle_tab_pin(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        if let Err(error) = self.workspace.toggle_tab_pin(tab_id) {
            self.last_error = Some(error.to_string());
        }
        cx.notify();
    }

    fn start_tab_drag(&mut self, tab_id: EditorTabId, position: Point<Pixels>) {
        self.tab_drag = Some(TabDrag {
            tab_id,
            start_position: position,
            over_tab_id: tab_id,
            activated: false,
        });
    }

    fn update_tab_drag(
        &mut self,
        target_tab_id: EditorTabId,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.tab_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let distance = f32::from(event.position.x - drag.start_position.x).abs();
        if !drag.activated && distance < IDE_TAB_REORDER_ACTIVATION_PX {
            return;
        }
        drag.activated = true;
        drag.over_tab_id = target_tab_id;
        self.tab_drag = Some(drag);
        cx.notify();
    }

    fn finish_tab_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.tab_drag.take() {
            if drag.activated
                && drag.tab_id != drag.over_tab_id
                && let Some(target_index) = self
                    .workspace
                    .tabs()
                    .iter()
                    .position(|tab| tab.id == drag.over_tab_id)
            {
                let _ = self.workspace.move_tab_to_index(drag.tab_id, target_index);
            }
            cx.notify();
        }
    }

    fn resolve_dirty_close(&mut self, decision: DirtyCloseDecision, cx: &mut Context<Self>) {
        let Some(request) = self.workspace.pending_close().cloned() else {
            return;
        };
        match decision {
            DirtyCloseDecision::Save => {
                self.save_after_close = Some(request.id);
                self.save_tab(request.tab_id, cx);
            }
            DirtyCloseDecision::Discard | DirtyCloseDecision::Cancel => {
                let closing_tab = request.tab_id;
                let resolved = self.workspace.resolve_dirty_close(request.id, decision);
                if matches!(resolved, Ok(None)) && decision == DirtyCloseDecision::Discard {
                    self.editors.remove(&closing_tab);
                }
                cx.notify();
            }
        }
    }

    fn save_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.sync_editor_to_workspace(tab_id, cx);
        let Some(buffer) = self.workspace.buffer(tab_id).cloned() else {
            return;
        };
        if self.saving_tabs.contains(&tab_id) {
            return;
        }
        self.saving_tabs.insert(tab_id);
        let close_request = self.save_after_close.take();
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let mode = if fs.capabilities().atomic_write {
                WriteMode::AtomicReplace
            } else {
                WriteMode::CreateOrReplace
            };
            let result = await_ide_backend(backend_runtime.spawn(async move {
                fs.write_file(&buffer.location, &buffer.text, Some(&buffer.version), mode)
                    .await
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                this.saving_tabs.remove(&tab_id);
                match result {
                    Ok(version) => {
                        if let Some(request_id) = close_request {
                            let _ = this
                                .workspace
                                .complete_dirty_close_after_save(request_id, version.clone());
                            this.editors.remove(&tab_id);
                        } else {
                            let _ = this.workspace.mark_saved(tab_id, version);
                        }
                        if let Some(editor) = this.editors.get(&tab_id) {
                            editor.update(cx, |editor, cx| editor.mark_saved_external(cx));
                        }
                    }
                    Err(error) => {
                        let message = format!("{}: {}", this.labels.save_failed, error.message);
                        this.last_error = Some(message.clone());
                        if let Some(editor) = this.editors.get(&tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.mark_save_failed_external(message, cx)
                            });
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn sync_editor_to_workspace(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(editor) = self.editors.get(&tab_id) else {
            return;
        };
        let text = editor.read(cx).buffer().text();
        let _ = self.workspace.replace_buffer_text(tab_id, text);
    }

    fn sync_all_editors(&mut self, cx: &mut Context<Self>) {
        let tab_ids = self.editors.keys().copied().collect::<Vec<_>>();
        for tab_id in tab_ids {
            self.sync_editor_to_workspace(tab_id, cx);
        }
    }

    fn active_editor(&self) -> Option<Entity<TextEditorView>> {
        self.workspace
            .active_tab()
            .and_then(|tab_id| self.editors.get(&tab_id).cloned())
    }

    fn is_tab_dirty(&self, tab_id: EditorTabId, cx: &mut Context<Self>) -> bool {
        self.editors
            .get(&tab_id)
            .map(|editor| editor.read(cx).buffer().is_dirty())
            .or_else(|| {
                self.workspace
                    .buffer(tab_id)
                    .map(|buffer| buffer.is_dirty())
            })
            .unwrap_or(false)
    }
}

impl Render for IdeSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.tokens.ui;
        let font_family = tauri_ui_font_family(self.tokens.metrics.font_family);
        let mut root = div()
            .id("oxideterm-gpui-ide")
            .relative()
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .font_family(SharedString::from(font_family))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(theme.text))
            .bg(if self.runtime_settings.background_active {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    let closed_tab_menu = this.tab_context_menu.take().is_some();
                    let closed_tree_menu = this.tree_context_menu.take().is_some();
                    let closed_agent_menu = this.agent_status_menu.take().is_some();
                    if closed_tab_menu || closed_tree_menu || closed_agent_menu {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.finish_tab_drag(cx);
                }),
            )
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_folder_picker_key(event, window, cx);
            }));

        let body = match &self.load_state {
            IdeLoadState::Empty => self.render_empty_project(cx),
            IdeLoadState::Loading => self.render_loading_project(cx),
            IdeLoadState::Error(message) => self.render_project_error(message.clone(), cx),
            IdeLoadState::Disconnected | IdeLoadState::Ready => self.render_workspace(cx),
        };
        root = root.child(body);

        if matches!(self.load_state, IdeLoadState::Disconnected) {
            root = root.child(self.render_disconnected_overlay());
        }
        if self.workspace.pending_close().is_some() {
            root = root.child(self.render_dirty_close_dialog(cx));
        }
        if self.folder_switch_confirm_open {
            root = root.child(self.render_folder_switch_confirm_dialog(cx));
        }
        if self.folder_picker.open {
            root = root.child(self.render_folder_picker_dialog(cx));
        }
        if let Some(menu) = self.tab_context_menu {
            root = root.child(self.render_tab_context_menu(menu, _window, cx));
        }
        if let Some(menu) = self.tree_context_menu.clone() {
            root = root.child(self.render_tree_context_menu(menu, _window, cx));
        }
        if let Some(menu) = self.agent_status_menu {
            root = root.child(self.render_agent_status_menu(menu, _window, cx));
        }
        if self.agent_remove_confirm_open {
            root = root.child(self.render_agent_remove_confirm_dialog(cx));
        }
        if self.agent_opt_in_open {
            root = root.child(self.render_agent_opt_in_dialog(cx));
        }
        root
    }
}

impl Focusable for IdeSurface {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<IdeSurfaceEvent> for IdeSurface {}

impl IdeSurface {
    fn render_empty_project(&self, _cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.labels.no_project.clone())
            .into_any_element()
    }

    fn render_loading_project(&self, _cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .items_center()
            .justify_center()
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.icon("lucide/loader-circle.svg", 24.0, self.tokens.ui.accent))
            .child(self.labels.loading_project.clone())
            .into_any_element()
    }

    fn render_project_error(&self, message: String, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .items_center()
            .justify_center()
            .text_color(rgb(tokens.ui.text_muted))
            .child(self.icon("lucide/alert-triangle.svg", 28.0, tokens.ui.warning))
            .child(
                div()
                    .text_color(rgb(tokens.ui.text_heading))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{}: {message}", self.labels.open_failed)),
            )
            .child(
                button_with(
                    tokens,
                    self.labels.retry.clone(),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.retry_open_project(cx);
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_workspace(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(self.render_tree_panel(cx))
                    .child(self.render_editor_area(cx)),
            )
            .child(self.render_status_bar(cx))
            .into_any_element()
    }

    fn render_tree_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let width = self
            .tree_width
            .clamp(IDE_TREE_MIN_WIDTH, IDE_TREE_MAX_WIDTH);
        let mut tree = div()
            .w(px(width))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba((self.tokens.ui.border << 8) | IDE_BORDER_HALF_ALPHA))
            .bg(self.ide_bg(self.tokens.ui.bg, IDE_BG_HALF_ALPHA));

        let Some(snapshot) = self.workspace.snapshot().ok() else {
            return tree.into_any_element();
        };
        let root_location = snapshot.project.root.clone();
        let root_title = snapshot.project.title.clone();
        tree = tree
            .child(
                div()
                    .h(px(36.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.icon(
                                "lucide/folder-open.svg",
                                IDE_ICON_SIZE,
                                self.tokens.ui.info,
                            ))
                            .child(div().truncate().child(snapshot.project.title.clone())),
                    )
                    .child({
                        let folder_disabled = self.workspace.has_dirty_buffers()
                            || matches!(self.load_state, IdeLoadState::Loading);
                        let refresh_disabled = matches!(self.load_state, IdeLoadState::Loading);
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .size(px(IDE_TREE_TOOLBAR_BUTTON_SIZE))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.sm))
                                    .opacity(if folder_disabled { 0.5 } else { 1.0 })
                                    .hover(|style| {
                                        if folder_disabled {
                                            style
                                        } else {
                                            style.bg(rgba(
                                                (self.tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA,
                                            ))
                                        }
                                    })
                                    .child(self.icon(
                                        "lucide/folder-input.svg",
                                        IDE_TREE_TOOLBAR_ICON_SIZE,
                                        self.tokens.ui.text_muted,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            if !folder_disabled {
                                                this.request_open_folder_picker(cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .size(px(IDE_TREE_TOOLBAR_BUTTON_SIZE))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.sm))
                                    .opacity(if refresh_disabled { 0.5 } else { 1.0 })
                                    .hover(|style| {
                                        if refresh_disabled {
                                            style
                                        } else {
                                            style.bg(rgba(
                                                (self.tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA,
                                            ))
                                        }
                                    })
                                    .child(self.icon(
                                        "lucide/refresh-cw.svg",
                                        IDE_TREE_TOOLBAR_ICON_SIZE,
                                        self.tokens.ui.text_muted,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            if !refresh_disabled {
                                                this.retry_open_project(cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                    }),
            )
            .child(
                div()
                    .id("ide-tree-scroll")
                    .flex_1()
                    .min_h_0()
                    .py_1()
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.open_tree_context_menu(
                                root_location.clone(),
                                true,
                                root_title.clone(),
                                event.position,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    // Tauri's tree renders the loaded root files directly with
                    // `rootFiles.map(...)`; keep the same concrete row list so
                    // SFTP listings cannot disappear behind GPUI list sizing.
                    .child(self.render_tree_rows(snapshot.project.root, cx)),
            );
        tree.into_any_element()
    }

    fn render_tree_rows(&mut self, root: IdeLocation, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.flatten_tree_rows(root);
        let mut list = div()
            .id("ide-tree-scroll-content")
            .size_full()
            .overflow_y_scroll();
        if rows.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.labels.no_subfolders.clone()),
            );
        }
        for row in rows.iter().cloned() {
            list = list.child(self.render_tree_row(row.entry, row.depth, row.expanded, cx));
        }
        list.into_any_element()
    }

    fn flatten_tree_rows(&mut self, root: IdeLocation) -> Arc<Vec<TreeRenderRow>> {
        let root_key = root.stable_key();
        let tree_revision = self.workspace.file_tree().revision();
        if let Some(cache) = self.tree_rows_cache.as_ref()
            && cache.root_key == root_key
            && cache.tree_revision == tree_revision
        {
            return cache.rows.clone();
        }

        let mut rows = Vec::new();
        self.push_flattened_tree_rows(root, 0, &mut rows);
        // FileTreeState owns a revision counter, so the GPUI surface can keep
        // the expensive flattened tree stable across renders while selection
        // and loading state continue to resolve live per row.
        let rows = Arc::new(rows);
        self.tree_rows_cache = Some(TreeRowsCache {
            root_key,
            tree_revision,
            rows: rows.clone(),
        });
        rows
    }

    fn push_flattened_tree_rows(
        &self,
        directory: IdeLocation,
        depth: usize,
        rows: &mut Vec<TreeRenderRow>,
    ) {
        let children = self
            .workspace
            .file_tree()
            .children(&directory)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        for entry in children {
            let expanded = self.workspace.file_tree().is_expanded(&entry.location);
            rows.push(TreeRenderRow {
                entry: entry.clone(),
                depth,
                expanded,
            });
            if expanded && matches!(entry.kind, FileKind::Directory) {
                self.push_flattened_tree_rows(entry.location, depth + 1, rows);
            }
        }
    }

    fn render_tree_row(
        &mut self,
        entry: FileTreeEntry,
        depth: usize,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.workspace.file_tree().selected() == Some(&entry.location);
        let is_dir = matches!(entry.kind, FileKind::Directory);
        let path_key = entry.location.stable_key();
        let loading = self.loading_paths.contains(&path_key);
        let icon = if is_dir {
            file_icons::folder_icon(expanded, entry.name == ".git", &self.tokens)
        } else {
            file_icons::file_icon(&entry.name, &self.tokens)
        };
        let row_bg = if selected {
            // Tauri tree open state is `bg-theme-accent/10 text-theme-accent`.
            rgba((self.tokens.ui.accent << 8) | IDE_TREE_SELECTED_ALPHA)
        } else {
            rgba(0x00000000)
        };
        div()
            .h(px(IDE_ROW_HEIGHT))
            .px_1()
            .flex()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .bg(row_bg)
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .hover(|style| style.bg(rgba((self.tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let entry = entry.clone();
                    move |this, _event, _window, cx| {
                        this.tree_context_menu = None;
                        this.open_tree_entry(entry.clone(), cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let entry = entry.clone();
                    move |this, event: &MouseDownEvent, _window, cx| {
                        let _ = this
                            .workspace
                            .select_tree_entry(Some(entry.location.clone()));
                        this.open_tree_context_menu(
                            entry.location.clone(),
                            matches!(entry.kind, FileKind::Directory),
                            entry.name.clone(),
                            event.position,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
            )
            .child(div().w(px((depth as f32) * IDE_TREE_INDENT_STEP)))
            .child(if is_dir {
                if expanded {
                    self.icon(
                        "lucide/chevron-down.svg",
                        14.0,
                        self.tokens.ui.text_secondary,
                    )
                } else {
                    self.icon(
                        "lucide/chevron-right.svg",
                        14.0,
                        self.tokens.ui.text_secondary,
                    )
                }
            } else {
                div().w(px(14.0)).into_any_element()
            })
            .child(if loading {
                self.icon(
                    "lucide/loader-circle.svg",
                    IDE_ICON_SIZE,
                    self.tokens.ui.accent,
                )
            } else if is_dir {
                self.icon(icon.path, IDE_ICON_SIZE, icon.color)
            } else {
                self.icon(icon.path, IDE_FILE_ICON_SIZE, icon.color)
            })
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if selected {
                        self.tokens.ui.accent
                    } else if is_dir {
                        self.tokens.ui.text
                    } else {
                        self.tokens.ui.text_muted
                    }))
                    .child(entry.name),
            )
            .into_any_element()
    }

    fn render_editor_area(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .size_full()
            .flex()
            .flex_col()
            .bg(self.ide_editor_content_bg(self.tokens.ui.bg))
            .child(self.render_tabs(cx))
            .child(div().flex_1().min_h_0().child(match self.active_editor() {
                Some(editor) => editor.into_any_element(),
                None => self.render_empty_editor(cx),
            }))
            .into_any_element()
    }

    fn render_tabs(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = self.workspace.tabs().to_vec();
        let active_tab = self.workspace.active_tab();
        let mut row = div()
            .id("ide-tabs-scroll")
            .h(px(34.0))
            .flex()
            .items_center()
            .overflow_x_scroll()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.ide_bg(self.tokens.ui.bg, IDE_BG_HALF_ALPHA));

        for tab in tabs {
            let active = Some(tab.id) == active_tab;
            let dirty = self.is_tab_dirty(tab.id, cx);
            let tab_id = tab.id;
            let is_dragging = self
                .tab_drag
                .is_some_and(|drag| drag.activated && drag.tab_id == tab_id);
            let file_icon = file_icons::file_icon(&tab.title, &self.tokens);
            row = row.child(
                div()
                    .h_full()
                    .px(px(IDE_TAB_PADDING_X))
                    .py(px(IDE_TAB_PADDING_Y))
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .border_r_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | IDE_BORDER_HALF_ALPHA))
                    .relative()
                    .bg(if active {
                        rgb(self.tokens.ui.bg_hover)
                    } else {
                        self.ide_bg(self.tokens.ui.bg, IDE_BG_HALF_ALPHA)
                    })
                    .opacity(if is_dragging { 0.7 } else { 1.0 })
                    .when(is_dragging, |this| {
                        this.shadow_lg().rounded(px(self.tokens.radii.sm))
                    })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.tab_context_menu = None;
                            this.tree_context_menu = None;
                            this.start_tab_drag(tab_id, event.position);
                            if event.click_count >= 2 {
                                this.toggle_tab_pin(tab_id, cx);
                            } else {
                                this.activate_tab(tab_id, cx);
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_tab(tab_id, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                            this.update_tab_drag(tab_id, event, cx);
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                            this.finish_tab_drag(cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            this.tab_context_menu = Some(TabContextMenu {
                                tab_id,
                                x: f32::from(event.position.x),
                                y: f32::from(event.position.y),
                            });
                            this.tree_context_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .when(tab.is_pinned, |this| {
                        this.child(self.icon("lucide/pin.svg", 12.0, self.tokens.ui.accent))
                    })
                    .child(self.icon(file_icon.path, IDE_FILE_ICON_SIZE, file_icon.color))
                    .child(
                        div()
                            .max_w(px(120.0))
                            .truncate()
                            .text_color(rgb(if active {
                                self.tokens.ui.text
                            } else {
                                self.tokens.ui.text_muted
                            }))
                            .when(dirty, |this| this.italic())
                            .child(tab.title.clone()),
                    )
                    .when(dirty, |this| {
                        this.child(
                            div()
                                .size(px(6.0))
                                .rounded(px(self.tokens.radii.active_indicator))
                                .bg(rgb(self.tokens.ui.accent)),
                        )
                    })
                    .child(
                        div()
                            .ml_1()
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .hover(|style| style.bg(rgba((self.tokens.ui.bg_active << 8) | 0xcc)))
                            .child(self.icon("lucide/x.svg", 12.0, self.tokens.ui.text_secondary))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.close_tab(tab_id, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .when(active, |this| {
                        this.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(px(2.0))
                                .bg(rgb(self.tokens.ui.accent)),
                        )
                    }),
            );
        }
        row.into_any_element()
    }

    fn render_tab_context_menu(
        &self,
        menu: TabContextMenu,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let x = menu
            .x
            .min(f32::from(viewport.width) - IDE_TAB_CONTEXT_MENU_WIDTH - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(f32::from(viewport.height) - IDE_TAB_CONTEXT_MENU_ITEM_HEIGHT * 2.0 - 16.0)
            .max(8.0);
        let pinned = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.id == menu.tab_id)
            .map(|tab| tab.is_pinned)
            .unwrap_or(false);

        // Tauri `IdeEditorTabs.tsx` uses a fixed z-50 elevated menu with
        // min-w-[140px], rounded-md, py-1, and two text-xs actions.
        let popup = div()
            .w(px(IDE_TAB_CONTEXT_MENU_WIDTH))
            .py(px(IDE_TAB_CONTEXT_MENU_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            .child(self.render_tab_context_menu_item(
                "lucide/pin.svg",
                if pinned {
                    self.labels.unpin_tab.clone()
                } else {
                    self.labels.pin_tab.clone()
                },
                cx.listener(move |this, _event, _window, cx| {
                    this.toggle_tab_pin(menu.tab_id, cx);
                    this.tab_context_menu = None;
                    cx.stop_propagation();
                }),
            ))
            .child(self.render_tab_context_menu_item(
                "lucide/x.svg",
                self.labels.close_tab.clone(),
                cx.listener(move |this, _event, _window, cx| {
                    this.close_tab(menu.tab_id, cx);
                    this.tab_context_menu = None;
                    cx.stop_propagation();
                }),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event, _window, cx| {
                    cx.stop_propagation();
                }),
            )
            .into_any_element();

        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(x), px(y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(popup),
        )
        .with_priority(IDE_TAB_CONTEXT_MENU_Z)
        .into_any_element()
    }

    fn render_tab_context_menu_item(
        &self,
        icon: &'static str,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        div()
            .h(px(IDE_TAB_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(self.icon(icon, 12.0, self.tokens.ui.text))
            .child(div().truncate().child(label))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn open_tree_context_menu(
        &mut self,
        location: IdeLocation,
        is_directory: bool,
        name: String,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.tab_context_menu = None;
        self.tree_context_menu = Some(TreeContextMenu {
            location,
            is_directory,
            name,
            x: f32::from(position.x),
            y: f32::from(position.y),
        });
        cx.notify();
    }

    fn render_tree_context_menu(
        &self,
        menu: TreeContextMenu,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let x = menu
            .x
            .min(f32::from(viewport.width) - IDE_TREE_CONTEXT_MENU_WIDTH - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(f32::from(viewport.height) - IDE_TREE_CONTEXT_MENU_MAX_HEIGHT - 8.0)
            .max(8.0);

        let popup = div()
            .w(px(IDE_TREE_CONTEXT_MENU_WIDTH))
            .py(px(IDE_TREE_CONTEXT_MENU_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg))
            .shadow_lg()
            .child(self.render_tree_context_menu_item(
                "lucide/file-plus.svg",
                self.labels.context_new_file.clone(),
                None,
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_tree_context_menu_item(
                "lucide/folder-plus.svg",
                self.labels.context_new_folder.clone(),
                None,
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_tree_context_menu_divider())
            .child(self.render_tree_context_menu_item(
                "lucide/edit-3.svg",
                self.labels.context_rename.clone(),
                Some("F2"),
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_tree_context_menu_item(
                "lucide/trash-2.svg",
                self.labels.context_delete.clone(),
                None,
                true,
                cx.listener(|this, _event, _window, cx| {
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_tree_context_menu_divider())
            .child(self.render_tree_context_menu_item(
                "lucide/copy.svg",
                self.labels.context_copy_path.clone(),
                None,
                false,
                cx.listener({
                    let path = location_path(menu.location.clone());
                    move |this, _event, _window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                        this.tree_context_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            ))
            .child(self.render_tree_context_menu_item(
                "lucide/terminal.svg",
                self.labels.context_open_in_terminal.clone(),
                None,
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .into_any_element();

        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(x), px(y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(popup),
        )
        .with_priority(IDE_TREE_CONTEXT_MENU_Z)
        .into_any_element()
    }

    fn render_tree_context_menu_item(
        &self,
        icon: &'static str,
        label: String,
        shortcut: Option<&'static str>,
        danger: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let text_color = if danger {
            TAILWIND_RED_400
        } else {
            self.tokens.ui.text
        };
        let hover_bg = if danger {
            rgba((TAILWIND_RED_500 << 8) | IDE_TREE_CONTEXT_MENU_DANGER_BG_ALPHA)
        } else {
            rgb(self.tokens.ui.bg_hover)
        };
        div()
            .h(px(IDE_TREE_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .px_3()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(text_color))
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .child(
                svg()
                    .path(icon)
                    .size(px(12.0))
                    .text_color(rgba((text_color << 8) | IDE_TREE_CONTEXT_MENU_ICON_ALPHA)),
            )
            .child(div().w(px(8.0)))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .when_some(shortcut, |this, shortcut| {
                this.child(
                    div()
                        .ml_4()
                        .text_size(px(IDE_TREE_CONTEXT_MENU_SHORTCUT_SIZE))
                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                        .child(shortcut),
                )
            })
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_tree_context_menu_divider(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .my(px(IDE_TREE_CONTEXT_MENU_PADDING_Y))
            .bg(rgb(self.tokens.ui.border))
            .into_any_element()
    }

    fn render_empty_editor(&self, _cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(self.ide_editor_content_bg(self.tokens.ui.bg))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.icon(
                "lucide/code-2.svg",
                IDE_EMPTY_ICON_SIZE,
                self.tokens.ui.text_muted,
            ))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.labels.no_open_files.clone()),
            )
            .child(self.labels.click_to_open.clone())
            .into_any_element()
    }

    fn render_status_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let dirty_count = self
            .workspace
            .tabs()
            .iter()
            .filter(|tab| self.is_tab_dirty(tab.id, cx))
            .count();
        let active_path = self
            .workspace
            .active_tab()
            .and_then(|tab_id| self.workspace.buffer(tab_id))
            .map(|buffer| match &buffer.location {
                IdeLocation::Remote { path, .. } => path.clone(),
                IdeLocation::Local { path } => path.display().to_string(),
            })
            .unwrap_or_default();

        div()
            .h(px(IDE_STATUS_BAR_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(if self.runtime_settings.background_active {
                rgba((self.tokens.ui.bg_panel << 8) | IDE_BG_ACTIVE_THEME_ALPHA)
            } else {
                rgb(self.tokens.ui.bg_panel)
            })
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.render_agent_status_trigger(cx))
                    .when_some(self.git_branch.clone(), |this, branch| {
                        this.child(format!("git: {branch}"))
                    })
                    .when(dirty_count > 0, |this| {
                        this.child(format!("{dirty_count} unsaved"))
                    }),
            )
            .child(div().truncate().child(active_path))
            .into_any_element()
    }

    fn render_agent_status_trigger(&self, cx: &mut Context<Self>) -> AnyElement {
        let status = self.fs.status();
        let (icon, label, color, opacity) = self.agent_status_trigger_parts(&status);
        div()
            .flex()
            .items_center()
            .gap_1()
            .mr_4()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.xs))
            .text_color(rgb(color))
            .opacity(opacity)
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(self.icon(icon, 12.0, color))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.agent_status_menu = Some(AgentStatusMenu {
                        x: f32::from(event.position.x),
                        y: f32::from(event.position.y),
                    });
                    this.tab_context_menu = None;
                    this.tree_context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn agent_status_trigger_parts(&self, status: &AgentStatus) -> (&'static str, String, u32, f32) {
        if self.agent_action == Some(AgentActionKind::Refresh) {
            return (
                "lucide/hard-drive.svg",
                "...".to_string(),
                self.tokens.ui.text_muted,
                0.5,
            );
        }
        match status {
            AgentStatus::Ready { .. } => (
                "lucide/cpu.svg",
                "Agent".to_string(),
                TAILWIND_EMERALD_400,
                1.0,
            ),
            AgentStatus::Deploying => (
                "lucide/hard-drive.svg",
                "Agent...".to_string(),
                TAILWIND_AMBER_400,
                1.0,
            ),
            AgentStatus::ManualUploadRequired { .. } | AgentStatus::ManualUpdateRequired { .. } => {
                (
                    "lucide/hard-drive.svg",
                    "SFTP".to_string(),
                    TAILWIND_AMBER_400,
                    1.0,
                )
            }
            AgentStatus::Failed { .. }
            | AgentStatus::UnsupportedArch { .. }
            | AgentStatus::NotDeployed
            | AgentStatus::SftpFallback => (
                "lucide/hard-drive.svg",
                "SFTP".to_string(),
                self.tokens.ui.text_muted,
                1.0,
            ),
        }
    }

    fn render_agent_status_menu(
        &self,
        menu: AgentStatusMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = self.fs.status();
        let manual = matches!(
            status,
            AgentStatus::ManualUploadRequired { .. } | AgentStatus::ManualUpdateRequired { .. }
        );
        let width = if manual {
            IDE_AGENT_MENU_MANUAL_WIDTH
        } else {
            IDE_AGENT_MENU_WIDTH
        };
        let height = self.agent_status_menu_height(&status);
        let x = menu.x.max(8.0);
        let y = (menu.y - height).max(8.0);
        let popup = div()
            .w(px(width))
            .py(px(IDE_AGENT_MENU_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_panel))
            .shadow_lg()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .occlude()
            .child(
                div()
                    .px(px(IDE_AGENT_MENU_DESCRIPTION_PADDING_X))
                    .py(px(IDE_AGENT_MENU_DESCRIPTION_PADDING_Y))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.agent_status_description(&status)),
            )
            .when(manual, |this| {
                this.child(self.render_agent_manual_body(&status))
            })
            .child(self.render_agent_status_menu_divider())
            .when(self.agent_can_deploy(&status), |this| {
                this.child(self.render_agent_status_menu_item(
                    if self.agent_action == Some(AgentActionKind::Deploy) {
                        "lucide/loader-circle.svg"
                    } else {
                        "lucide/rocket.svg"
                    },
                    if matches!(
                        status,
                        AgentStatus::ManualUploadRequired { .. }
                            | AgentStatus::ManualUpdateRequired { .. }
                    ) {
                        self.labels.agent_retry_btn.clone()
                    } else {
                        self.labels.agent_deploy_btn.clone()
                    },
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.agent_status_menu = None;
                        this.start_deploy_agent(cx);
                        cx.stop_propagation();
                    }),
                ))
            })
            .when(matches!(status, AgentStatus::Ready { .. }), |this| {
                this.child(self.render_agent_status_menu_item(
                    if self.agent_action == Some(AgentActionKind::Remove) {
                        "lucide/loader-circle.svg"
                    } else {
                        "lucide/trash-2.svg"
                    },
                    self.labels.agent_remove_btn.clone(),
                    true,
                    cx.listener(|this, _event, _window, cx| {
                        this.agent_status_menu = None;
                        this.agent_remove_confirm_open = true;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .into_any_element();

        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(x), px(y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(popup),
        )
        .with_priority(IDE_AGENT_MENU_Z)
        .into_any_element()
    }

    fn agent_status_menu_height(&self, status: &AgentStatus) -> f32 {
        let action_rows =
            if matches!(status, AgentStatus::Ready { .. }) || self.agent_can_deploy(status) {
                1.0
            } else {
                0.0
            };
        let manual_body = if matches!(
            status,
            AgentStatus::ManualUploadRequired { .. } | AgentStatus::ManualUpdateRequired { .. }
        ) {
            116.0
        } else {
            0.0
        };
        8.0 + 28.0 + manual_body + 1.0 + action_rows * IDE_AGENT_MENU_ITEM_HEIGHT
    }

    fn agent_can_deploy(&self, status: &AgentStatus) -> bool {
        !matches!(status, AgentStatus::Ready { .. } | AgentStatus::Deploying)
    }

    fn agent_status_description(&self, status: &AgentStatus) -> String {
        if self.agent_action == Some(AgentActionKind::Refresh) {
            return self.labels.agent_checking.clone();
        }
        match status {
            AgentStatus::Ready { version, .. } => {
                format!("{} (v{version})", self.labels.agent_ready)
            }
            AgentStatus::Deploying => self.labels.agent_deploying.clone(),
            AgentStatus::ManualUploadRequired { .. } => self.labels.agent_manual_upload.clone(),
            AgentStatus::ManualUpdateRequired { .. } => self.labels.agent_manual_update.clone(),
            AgentStatus::Failed { reason } => format!("{}: {reason}", self.labels.sftp_mode),
            AgentStatus::UnsupportedArch { arch } => format!("{} ({arch})", self.labels.sftp_mode),
            AgentStatus::NotDeployed | AgentStatus::SftpFallback => self.labels.sftp_mode.clone(),
        }
    }

    fn render_agent_manual_body(&self, status: &AgentStatus) -> AnyElement {
        let mut body = div()
            .max_w(px(IDE_AGENT_MENU_MANUAL_WIDTH))
            .px(px(IDE_AGENT_MENU_DESCRIPTION_PADDING_X))
            .pb(px(IDE_AGENT_MENU_DESCRIPTION_PADDING_Y))
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted));

        match status {
            AgentStatus::ManualUploadRequired { arch, remote_path } => {
                body = body
                    .child(
                        self.render_agent_manual_hint(self.labels.agent_manual_upload_hint.clone()),
                    )
                    .child(
                        self.render_agent_manual_code(
                            self.labels.agent_upload_to.clone(),
                            remote_path,
                        ),
                    )
                    .child(
                        self.labels
                            .agent_manual_upload_arch
                            .replace("{{arch}}", arch),
                    );
            }
            AgentStatus::ManualUpdateRequired {
                arch: _,
                remote_path,
                current_agent_version,
                current_compatibility_version,
                expected_compatibility_version,
            } => {
                body = body
                    .child(
                        self.render_agent_manual_hint(self.labels.agent_manual_update_hint.clone()),
                    )
                    .child(
                        self.render_agent_manual_code(
                            self.labels.agent_upload_to.clone(),
                            remote_path,
                        ),
                    )
                    .child(
                        self.labels
                            .agent_manual_update_current_agent_version
                            .replace("{{version}}", current_agent_version),
                    )
                    .child(
                        self.labels
                            .agent_manual_update_current_compatibility_version
                            .replace("{{version}}", &current_compatibility_version.to_string()),
                    )
                    .child(
                        self.labels
                            .agent_manual_update_expected_compatibility_version
                            .replace("{{version}}", &expected_compatibility_version.to_string()),
                    );
            }
            _ => {}
        }
        body.into_any_element()
    }

    fn render_agent_manual_hint(&self, text: String) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap_2()
            .child(self.icon("lucide/info.svg", 14.0, TAILWIND_AMBER_400))
            .child(div().flex_1().child(text))
            .into_any_element()
    }

    fn render_agent_manual_code(&self, label: String, path: &str) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(label)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded(px(self.tokens.radii.xs))
                    .font_family(SharedString::from(
                        self.tokens.metrics.markdown_code_font_family,
                    ))
                    .bg(rgb(self.tokens.ui.bg_sunken))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(path.to_string()),
            )
            .into_any_element()
    }

    fn render_agent_status_menu_item(
        &self,
        icon: &'static str,
        label: String,
        danger: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let text_color = if danger {
            TAILWIND_RED_400
        } else {
            self.tokens.ui.text
        };
        div()
            .h(px(IDE_AGENT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .px_2()
            .gap_2()
            .cursor_pointer()
            .text_color(rgb(text_color))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(self.icon(icon, 12.0, text_color))
            .child(div().flex_1().truncate().child(label))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_agent_status_menu_divider(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .my(px(IDE_AGENT_MENU_PADDING_Y))
            .bg(rgb(self.tokens.ui.border))
            .into_any_element()
    }

    fn render_agent_remove_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(dialog_title(
                        tokens,
                        self.labels.agent_remove_confirm_title.clone(),
                    ))
                    .child(dialog_description(
                        tokens,
                        self.labels.agent_remove_confirm_desc.clone(),
                    )),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.agent_remove_confirm_open = false;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.agent_remove_confirm_btn.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: self.agent_action == Some(AgentActionKind::Remove),
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.start_remove_agent(cx);
                            }),
                        ),
                    ),
            );
        self.render_modal_overlay(dialog)
    }

    fn render_agent_opt_in_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let dialog = div()
            .w(px(IDE_AGENT_OPT_IN_WIDTH))
            .overflow_hidden()
            .rounded(px(tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (tokens.ui.border << 8) | IDE_AGENT_OPT_IN_BORDER_ALPHA,
            ))
            .bg(rgb(tokens.ui.bg_panel))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(IDE_AGENT_OPT_IN_GAP))
                    .px(px(IDE_AGENT_OPT_IN_BODY_PADDING_X))
                    .pt(px(IDE_AGENT_OPT_IN_BODY_PADDING_TOP))
                    .pb(px(IDE_AGENT_OPT_IN_BODY_PADDING_BOTTOM))
                    .child(
                        div()
                            .size(px(IDE_AGENT_OPT_IN_ICON_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(rgba(
                                (tokens.ui.accent << 8) | IDE_AGENT_OPT_IN_ACCENT_BORDER_ALPHA,
                            ))
                            .bg(rgba(
                                (tokens.ui.accent << 8) | IDE_AGENT_OPT_IN_ACCENT_BG_ALPHA,
                            ))
                            .child(self.icon(
                                "lucide/bot.svg",
                                IDE_AGENT_OPT_IN_ICON_INNER_SIZE,
                                tokens.ui.accent,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(tokens.ui.text))
                            .text_align(gpui::TextAlign::Center)
                            .child(self.labels.agent_optin_title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .text_color(rgb(tokens.ui.text_muted))
                            .text_align(gpui::TextAlign::Center)
                            .line_height(px(tokens.metrics.ui_text_sm * 1.45))
                            .child(self.labels.agent_optin_desc.clone()),
                    )
                    .child(self.render_agent_opt_in_benefits())
                    .child(self.render_agent_opt_in_remember(cx)),
            )
            .child(
                div()
                    .flex()
                    .border_t_1()
                    .border_color(rgba((tokens.ui.border << 8) | IDE_HOVER_ALPHA))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .py(px(IDE_AGENT_OPT_IN_ACTION_PADDING_Y))
                            .border_r_1()
                            .border_color(rgba((tokens.ui.border << 8) | IDE_HOVER_ALPHA))
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(tokens.ui.text_muted))
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(rgb(tokens.ui.bg_hover))
                                    .text_color(rgb(tokens.ui.text))
                            })
                            .child(self.icon("lucide/folder-sync.svg", 14.0, tokens.ui.text_muted))
                            .child(self.labels.agent_optin_sftp_only.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.choose_agent_opt_in(NodeAgentMode::Disabled, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .py(px(IDE_AGENT_OPT_IN_ACTION_PADDING_Y))
                            .text_size(px(tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(tokens.ui.accent))
                            .cursor_pointer()
                            .hover(|style| {
                                style.bg(rgba(
                                    (tokens.ui.accent << 8) | IDE_AGENT_OPT_IN_ACCENT_BG_ALPHA,
                                ))
                            })
                            .child(self.icon("lucide/bot.svg", 14.0, tokens.ui.accent))
                            .child(self.labels.agent_optin_enable.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.choose_agent_opt_in(NodeAgentMode::Enabled, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            );
        self.render_modal_overlay(dialog)
    }

    fn render_agent_opt_in_benefits(&self) -> AnyElement {
        let mut benefits = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted));
        for text in [
            self.labels.agent_optin_benefit_watch.clone(),
            self.labels.agent_optin_benefit_git.clone(),
            self.labels.agent_optin_benefit_atomic.clone(),
        ] {
            benefits = benefits.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(self.icon("lucide/check.svg", 12.0, TAILWIND_EMERALD_400))
                    .child(div().flex_1().child(text)),
            );
        }
        benefits.into_any_element()
    }

    fn render_agent_opt_in_remember(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .mt_1()
            .cursor_pointer()
            .child(
                div()
                    .size(px(14.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.xs))
                    .border_1()
                    .border_color(if self.agent_opt_in_remember {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.border)
                    })
                    .bg(if self.agent_opt_in_remember {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.bg_sunken)
                    })
                    .when(self.agent_opt_in_remember, |this| {
                        this.child(self.icon("lucide/check.svg", 10.0, self.tokens.ui.bg))
                    }),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.labels.agent_optin_remember.clone()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.agent_opt_in_remember = !this.agent_opt_in_remember;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn choose_agent_opt_in(&mut self, mode: NodeAgentMode, cx: &mut Context<Self>) {
        self.agent_opt_in_open = false;
        if self.agent_opt_in_remember {
            cx.emit(IdeSurfaceEvent::RememberAgentMode(mode));
        } else {
            self.runtime_settings.agent_mode = mode;
            self.fs.set_mode(mode);
        }
        if mode == NodeAgentMode::Enabled {
            self.start_deploy_agent(cx);
        } else {
            self.refresh_agent_status(cx);
        }
        cx.notify();
    }

    fn start_deploy_agent(&mut self, cx: &mut Context<Self>) {
        if matches!(self.agent_action, Some(AgentActionKind::Deploy)) {
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        self.agent_action = Some(AgentActionKind::Deploy);
        self.runtime_settings.agent_mode = NodeAgentMode::Enabled;
        self.fs.set_mode(NodeAgentMode::Enabled);
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let status = backend_runtime
                .spawn(async move { fs.deploy_agent_for_node(node_id).await })
                .await
                .unwrap_or_else(|error| AgentStatus::Failed {
                    reason: error.to_string(),
                });
            let _ = weak.update(cx, |this, cx| {
                this.agent_action = None;
                let _ = status;
                this.schedule_next_agent_status_poll(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn start_remove_agent(&mut self, cx: &mut Context<Self>) {
        if matches!(self.agent_action, Some(AgentActionKind::Remove)) {
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        self.agent_action = Some(AgentActionKind::Remove);
        self.agent_remove_confirm_open = false;
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(
                backend_runtime.spawn(async move { fs.remove_agent_for_node(node_id).await }),
            )
            .await;
            let _ = weak.update(cx, |this, cx| {
                this.agent_action = None;
                if let Err(error) = result {
                    this.last_error = Some(error.message);
                }
                this.refresh_agent_status(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_agent_status(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.load_state,
            IdeLoadState::Ready | IdeLoadState::Disconnected
        ) || self.agent_action.is_some()
        {
            return;
        }
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        self.agent_action = Some(AgentActionKind::Refresh);
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let _ = backend_runtime
                .spawn(async move { fs.refresh_agent_status(node_id).await })
                .await;
            let _ = weak.update(cx, |this, cx| {
                if this.agent_action == Some(AgentActionKind::Refresh) {
                    this.agent_action = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn schedule_next_agent_status_poll(&mut self, cx: &mut Context<Self>) {
        if self.node_id.is_none()
            || !matches!(
                self.load_state,
                IdeLoadState::Ready | IdeLoadState::Disconnected
            )
        {
            return;
        }
        self.agent_poll_generation = self.agent_poll_generation.wrapping_add(1);
        let generation = self.agent_poll_generation;
        let delay = self.agent_poll_delay();
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.agent_poll_generation != generation {
                    return;
                }
                this.refresh_agent_status(cx);
                this.schedule_next_agent_status_poll(cx);
            });
        })
        .detach();
    }

    fn agent_poll_delay(&self) -> Duration {
        match self.fs.status() {
            AgentStatus::Deploying => Duration::from_secs(IDE_AGENT_POLL_DEPLOYING_SECS),
            AgentStatus::ManualUploadRequired { .. } | AgentStatus::ManualUpdateRequired { .. } => {
                Duration::from_secs(IDE_AGENT_POLL_MANUAL_SECS)
            }
            _ => Duration::from_secs(IDE_AGENT_POLL_READY_SECS),
        }
    }

    fn render_disconnected_overlay(&self) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(rgba(IDE_OVERLAY_ALPHA))
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .text_color(rgb(self.tokens.ui.text))
            .child(self.icon("lucide/wifi-off.svg", 32.0, self.tokens.ui.error))
            .child(self.labels.disconnected_overlay.clone())
            .into_any_element()
    }

    fn render_dirty_close_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(request) = self.workspace.pending_close() else {
            return div().into_any_element();
        };
        let tokens = &self.tokens;
        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(dialog_title(tokens, self.labels.unsaved_changes.clone()))
                    .child(dialog_description(
                        tokens,
                        self.labels
                            .unsaved_changes_desc
                            .replace("{{fileName}}", &request.title),
                    )),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_dirty_close(DirtyCloseDecision::Cancel, cx);
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.discard.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_dirty_close(DirtyCloseDecision::Discard, cx);
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.save.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.resolve_dirty_close(DirtyCloseDecision::Save, cx);
                            }),
                        ),
                    ),
            );
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba((tokens.ui.bg << 8) | IDE_MODAL_BACKDROP_ALPHA))
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(dialog)
            .into_any_element()
    }

    fn render_folder_switch_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let dialog = dialog_content(tokens)
            .child(
                dialog_header(tokens)
                    .child(dialog_title(tokens, self.labels.unsaved_changes.clone()))
                    .child(dialog_description(
                        tokens,
                        self.labels.unsaved_changes_folder.clone(),
                    )),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.folder_switch_confirm_open = false;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.discard.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.folder_switch_confirm_open = false;
                                let Some(node_id) = this.node_id.clone() else {
                                    cx.notify();
                                    return;
                                };
                                let initial_path =
                                    this.root_path.clone().unwrap_or_else(|| "/".to_string());
                                this.open_remote_folder_picker_for_node(node_id, initial_path, cx);
                            }),
                        ),
                    ),
            );

        self.render_modal_overlay(dialog)
    }

    fn render_folder_picker_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let current_path = self.folder_picker.current_path.clone();
        let selected_path = self.selected_folder_picker_path();
        let home_disabled = current_path == "/" || self.folder_picker.loading;
        let up_disabled = current_path == "/" || self.folder_picker.loading;
        let dialog = dialog_content(tokens)
            .w(px(IDE_FOLDER_DIALOG_WIDTH))
            .child(
                dialog_header(tokens)
                    .child(dialog_title(tokens, self.labels.select_folder.clone()))
                    .child(dialog_description(
                        tokens,
                        self.labels.select_folder_desc.clone(),
                    )),
            )
            .child(
                div()
                    .px(px(IDE_FOLDER_DIALOG_BODY_PADDING_X))
                    .py(px(IDE_FOLDER_DIALOG_BODY_GAP))
                    .flex()
                    .flex_col()
                    .gap(px(IDE_FOLDER_DIALOG_BODY_GAP))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.render_folder_path_input(cx))
                            .child(
                                button_with(
                                    tokens,
                                    self.labels.go.clone(),
                                    ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: self.folder_picker.loading,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.submit_folder_picker_path(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .h(px(tokens.metrics.ui_button_sm_height))
                                    .w(px(tokens.metrics.ui_button_sm_height))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(tokens.radii.md))
                                    .border_1()
                                    .border_color(rgb(tokens.ui.border))
                                    .opacity(if home_disabled { 0.5 } else { 1.0 })
                                    .cursor_pointer()
                                    .hover(|style| {
                                        if home_disabled {
                                            style
                                        } else {
                                            style.bg(rgba(
                                                (tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA,
                                            ))
                                        }
                                    })
                                    .child(self.icon(
                                        "lucide/home.svg",
                                        IDE_FOLDER_DIALOG_ICON_SIZE,
                                        tokens.ui.text,
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            if !home_disabled {
                                                this.go_folder_picker_home(cx);
                                            }
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                button_with(
                                    tokens,
                                    self.labels.go_to_parent.clone(),
                                    ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: up_disabled,
                                    },
                                )
                                .child(self.icon(
                                    "lucide/arrow-up.svg",
                                    IDE_FOLDER_DIALOG_ICON_SIZE,
                                    tokens.ui.text,
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.go_folder_picker_parent(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    )
                    .child(self.render_folder_picker_list(cx))
                    .child(
                        div()
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .text_color(rgb(tokens.ui.text_muted))
                            .flex()
                            .items_center()
                            .gap_1()
                            .min_w_0()
                            .child(format!("{}: ", self.labels.selected_path))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .px_1()
                                    .rounded(px(tokens.radii.xs))
                                    .font_family(SharedString::from(
                                        tokens.metrics.markdown_code_font_family,
                                    ))
                                    .bg(rgb(tokens.ui.bg_panel))
                                    .text_color(rgb(tokens.ui.text))
                                    .child(selected_path),
                            ),
                    ),
            )
            .child(
                dialog_footer(tokens)
                    .child(
                        button_with(
                            tokens,
                            self.labels.cancel.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.close_folder_picker(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            tokens,
                            self.labels.open_folder.clone(),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: self.folder_picker.loading,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.confirm_folder_picker(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            );

        self.render_modal_overlay(dialog)
    }

    fn render_folder_path_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let border = if self.folder_picker.path_input_focused {
            tokens.ui.accent
        } else {
            tokens.ui.border
        };
        div()
            .flex_1()
            .min_w_0()
            .h(px(tokens.metrics.form_input_height))
            .px(px(tokens.metrics.form_input_padding_x))
            .flex()
            .items_center()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(border))
            .bg(rgb(tokens.ui.bg_sunken))
            .font_family(SharedString::from(tokens.metrics.markdown_code_font_family))
            .text_size(px(tokens.metrics.ui_text_sm))
            .text_color(rgb(tokens.ui.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    window.focus(&this.focus_handle);
                    this.folder_picker.path_input_focused = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(if self.folder_picker.path_input.is_empty() {
                        "/".to_string()
                    } else {
                        self.folder_picker.path_input.clone()
                    }),
            )
            .into_any_element()
    }

    fn render_folder_picker_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = &self.tokens;
        let mut list = div()
            .id("ide-folder-picker-list")
            .h(px(IDE_FOLDER_DIALOG_LIST_HEIGHT))
            .overflow_y_scroll()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgb(tokens.ui.border))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation());

        if self.folder_picker.loading {
            return list
                .flex()
                .items_center()
                .justify_center()
                .child(self.icon("lucide/loader-circle.svg", 24.0, tokens.ui.text_muted))
                .into_any_element();
        }

        if let Some(error) = self.folder_picker.error.clone() {
            return list
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .p_4()
                .child(self.icon("lucide/alert-circle.svg", 24.0, tokens.ui.error))
                .child(
                    div()
                        .text_size(px(tokens.metrics.ui_text_sm))
                        .text_color(rgb(tokens.ui.error))
                        .text_align(gpui::TextAlign::Center)
                        .child(error),
                )
                .child(
                    button_with(
                        tokens,
                        self.labels.retry.clone(),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.load_folder_picker_current(cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
                .into_any_element();
        }

        if self.folder_picker.folders.is_empty() {
            return list
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(tokens.metrics.ui_text_sm))
                .text_color(rgb(tokens.ui.text_muted))
                .child(self.labels.no_subfolders.clone())
                .into_any_element();
        }

        let mut rows = div()
            .p(px(IDE_FOLDER_DIALOG_LIST_PADDING))
            .flex()
            .flex_col();
        for folder in self.folder_picker.folders.iter().cloned() {
            let selected = self.folder_picker.selected_folder.as_ref() == Some(&folder.name);
            let folder_name = folder.name.clone();
            // Tauri `IdeRemoteFolderDialog.tsx` renders `folders.map(...)`
            // directly inside the fixed-height scroller. The picker list is
            // small and variable-height, so native keeps the same direct rows;
            // uniform_list needs stricter row sizing and made loaded folders
            // look like an empty panel.
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(IDE_FOLDER_DIALOG_ROW_PADDING_X))
                    .py(px(IDE_FOLDER_DIALOG_ROW_PADDING_Y))
                    .rounded(px(tokens.radii.sm))
                    .cursor_pointer()
                    .bg(if selected {
                        rgba((tokens.ui.accent << 8) | IDE_FOLDER_DIALOG_SELECTED_ALPHA)
                    } else {
                        rgba(0x00000000)
                    })
                    .hover(|style| style.bg(rgba((tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA)))
                    .text_color(if selected {
                        rgb(tokens.ui.accent)
                    } else {
                        rgb(tokens.ui.text)
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let folder_name = folder_name.clone();
                            move |this, event: &MouseDownEvent, _window, cx| {
                                if event.click_count >= 2 {
                                    this.enter_folder_picker_folder(&folder_name, cx);
                                } else if this.folder_picker.selected_folder.as_ref()
                                    == Some(&folder_name)
                                {
                                    this.folder_picker.selected_folder = None;
                                    cx.notify();
                                } else {
                                    this.folder_picker.selected_folder = Some(folder_name.clone());
                                    cx.notify();
                                }
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .child(if selected {
                        self.icon(
                            "lucide/folder-open.svg",
                            IDE_FOLDER_DIALOG_ICON_SIZE,
                            tokens.ui.accent,
                        )
                    } else {
                        self.icon(
                            "lucide/folder.svg",
                            IDE_FOLDER_DIALOG_ICON_SIZE,
                            tokens.ui.text_secondary,
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(folder.name.clone()),
                    )
                    .child(self.icon(
                        "lucide/chevron-right.svg",
                        IDE_FOLDER_DIALOG_ICON_SIZE,
                        tokens.ui.text_muted,
                    )),
            );
        }
        list = list.child(rows);
        list.into_any_element()
    }

    fn render_modal_overlay(&self, dialog: impl IntoElement) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba((self.tokens.ui.bg << 8) | IDE_MODAL_BACKDROP_ALPHA))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(dialog)
            .into_any_element()
    }

    fn ide_bg(&self, color: u32, fallback_alpha: u32) -> gpui::Rgba {
        if self.runtime_settings.background_active {
            // Tauri `[data-bg-active]` remaps theme backgrounds to 40% alpha.
            rgba((color << 8) | IDE_BG_ACTIVE_THEME_ALPHA)
        } else {
            rgba((color << 8) | fallback_alpha)
        }
    }

    fn ide_editor_content_bg(&self, color: u32) -> gpui::Rgba {
        if self.runtime_settings.background_active {
            // Tauri IDE leaves CodeMirror's scroller transparent when the tab
            // background is active; the tab strip/status/tree keep the 40% tint.
            rgba((color << 8) | 0x00)
        } else {
            rgb(color)
        }
    }

    fn icon(&self, path: &'static str, size: f32, color: u32) -> AnyElement {
        svg()
            .path(path)
            .size(px(size))
            .text_color(rgb(color))
            .into_any_element()
    }
}

fn apply_editor_runtime_settings(
    editor: &Entity<TextEditorView>,
    tokens: ThemeTokens,
    runtime_settings: IdeRuntimeSettings,
    cx: &mut Context<IdeSurface>,
) {
    editor.update(cx, |editor, cx| {
        editor.apply_ide_runtime_settings(
            &tokens,
            runtime_settings.editor_font_size,
            runtime_settings.editor_line_height,
            runtime_settings.word_wrap,
            runtime_settings.background_active,
            cx,
        );
    });
}

async fn open_project_with_root_listing(
    fs: NodeAgentIdeFileSystem,
    node_id: String,
    root_path: String,
) -> Result<ProjectOpenResult, oxideterm_ide_core::IdeFileError> {
    let project = fs.open_project(node_id.clone(), root_path).await?;
    let root = IdeLocation::remote(node_id.clone(), project.root_path.clone());
    let children = fs.list_dir(&root).await.map(sort_tree_entries)?;
    Ok(ProjectOpenResult {
        node_id,
        root,
        title: project.name,
        git_branch: project.git_branch,
        children,
    })
}

async fn open_text_file(
    fs: NodeAgentIdeFileSystem,
    location: IdeLocation,
) -> Result<FileOpenResult, oxideterm_ide_core::IdeFileError> {
    let (node_id, path) = match &location {
        IdeLocation::Remote { node_id, path } => (node_id.clone(), path.clone()),
        IdeLocation::Local { .. } => {
            return Err(oxideterm_ide_core::IdeFileError::new(
                oxideterm_ide_core::IdeFileErrorKind::Unsupported,
                "GPUI IDE node surface only opens node SFTP files",
            ));
        }
    };
    match fs.check_file(node_id, path).await? {
        IdeFileCheck::Editable { .. } => {
            let data = fs.read_file(&location).await?;
            Ok(FileOpenResult {
                location,
                text: data.text,
                version: data.version,
            })
        }
        IdeFileCheck::TooLarge { size, limit } => Err(oxideterm_ide_core::IdeFileError::new(
            oxideterm_ide_core::IdeFileErrorKind::Unsupported,
            format!("File is too large to edit ({size} > {limit})"),
        )),
        IdeFileCheck::Binary => Err(oxideterm_ide_core::IdeFileError::new(
            oxideterm_ide_core::IdeFileErrorKind::Unsupported,
            "File is binary",
        )),
        IdeFileCheck::NotEditable { reason } => Err(oxideterm_ide_core::IdeFileError::new(
            oxideterm_ide_core::IdeFileErrorKind::Unsupported,
            reason,
        )),
    }
}

async fn await_ide_backend<T>(
    handle: tokio::task::JoinHandle<Result<T, oxideterm_ide_core::IdeFileError>>,
) -> Result<T, oxideterm_ide_core::IdeFileError> {
    handle.await.unwrap_or_else(|error| {
        Err(oxideterm_ide_core::IdeFileError::new(
            oxideterm_ide_core::IdeFileErrorKind::Other,
            format!("IDE backend task failed: {error}"),
        ))
    })
}

fn sort_tree_entries(mut entries: Vec<FileTreeEntry>) -> Vec<FileTreeEntry> {
    entries.sort_by(|left, right| {
        let left_dir = matches!(left.kind, FileKind::Directory);
        let right_dir = matches!(right.kind, FileKind::Directory);
        right_dir
            .cmp(&left_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    entries
}

fn location_path(location: IdeLocation) -> String {
    match location {
        IdeLocation::Remote { path, .. } => path,
        IdeLocation::Local { path } => path.display().to_string(),
    }
}

fn remote_path(location: &IdeLocation) -> Option<&str> {
    match location {
        IdeLocation::Remote { path, .. } => Some(path.as_str()),
        IdeLocation::Local { .. } => None,
    }
}

fn folder_picker_dirs(entries: Vec<FileTreeEntry>) -> Vec<FileTreeEntry> {
    let mut folders = entries
        .into_iter()
        .filter(|entry| matches!(entry.kind, FileKind::Directory))
        .collect::<Vec<_>>();
    folders.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    folders
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/".to_string();
    }
    if trimmed == "/" {
        return "/".to_string();
    }
    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.starts_with('/') {
        without_trailing.to_string()
    } else {
        format!("/{without_trailing}")
    }
}

fn join_remote_child(parent: &str, child: &str) -> String {
    if parent == "/" {
        format!("/{child}")
    } else {
        format!("{}/{child}", parent.trim_end_matches('/'))
    }
}

fn parent_remote_path(path: &str) -> String {
    let path = normalize_remote_path(path);
    if path == "/" {
        return "/".to_string();
    }
    path.rsplit_once('/')
        .map(|(parent, _)| {
            if parent.is_empty() {
                "/".to_string()
            } else {
                parent.to_string()
            }
        })
        .unwrap_or_else(|| "/".to_string())
}

fn language_for_location(location: &IdeLocation, source: &str) -> Option<LanguageId> {
    match location {
        IdeLocation::Local { path } => LanguageId::detect(Some(path.as_path()), source),
        IdeLocation::Remote { path, .. } => LanguageId::detect(Some(Path::new(path)), source),
    }
}
