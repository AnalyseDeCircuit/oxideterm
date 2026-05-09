// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use gpui::{
    AnyElement, App, AppContext, Context, Div, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement,
    Render, SharedString, Styled, Window, div, prelude::*, px, rgb, rgba, svg,
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
use oxideterm_ide_fs::NodeSftpIdeFileSystem;
use oxideterm_ssh::NodeRouter;
use oxideterm_theme::ThemeTokens;

use crate::labels::IdeLabels;

// Tauri IdeWorkspace.tsx uses a 280px default with 200px/500px resize bounds.
const IDE_TREE_DEFAULT_WIDTH: f32 = 280.0;
const IDE_TREE_MIN_WIDTH: f32 = 200.0;
const IDE_TREE_MAX_WIDTH: f32 = 500.0;
const IDE_STATUS_BAR_HEIGHT: f32 = 24.0;
const IDE_TAB_PADDING_X: f32 = 12.0;
const IDE_TAB_PADDING_Y: f32 = 6.0;
const IDE_ICON_SIZE: f32 = 16.0;
const IDE_EMPTY_ICON_SIZE: f32 = 64.0;
const IDE_ROW_HEIGHT: f32 = 26.0;
const IDE_TREE_TOOLBAR_BUTTON_SIZE: f32 = 24.0;
const IDE_TREE_TOOLBAR_ICON_SIZE: f32 = 14.0;

// Named alpha constants preserve the Tailwind source classes:
// bg-theme-bg/50, hover:bg-theme-bg-hover/30, border-theme-border/50,
// and the disconnected overlay's bg-black/50.
const IDE_BG_HALF_ALPHA: u32 = 0x80;
const IDE_HOVER_ALPHA: u32 = 0x4d;
const IDE_BORDER_HALF_ALPHA: u32 = 0x80;
const IDE_OVERLAY_ALPHA: u32 = 0x80;
const IDE_MODAL_BACKDROP_ALPHA: u32 = 0xcc;

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

/// GPUI IDE owner.
///
/// This is the native equivalent of Tauri's `IdeWorkspace` + `ideStore` owner:
/// project state lives here, while terminal panes remain optional consumers of
/// the same node. SFTP is acquired through `NodeRouter`, never through an open
/// terminal tab, so reconnect restore has a real IDE surface to target.
pub struct IdeSurface {
    workspace: IdeWorkspace,
    fs: NodeSftpIdeFileSystem,
    tokens: ThemeTokens,
    labels: IdeLabels,
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
    last_error: Option<String>,
    folder_picker: FolderPickerState,
    folder_switch_confirm_open: bool,
}

impl IdeSurface {
    pub fn new(
        router: NodeRouter,
        tokens: ThemeTokens,
        labels: IdeLabels,
        backend_runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            workspace: IdeWorkspace::new(),
            fs: NodeSftpIdeFileSystem::new(router),
            tokens,
            labels,
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
            last_error: None,
            folder_picker: FolderPickerState::default(),
            folder_switch_confirm_open: false,
        }
    }

    pub fn load_state(&self) -> &IdeLoadState {
        &self.load_state
    }

    pub fn snapshot(&mut self, cx: &mut Context<Self>) -> Option<WorkspaceSnapshot> {
        self.sync_all_editors(cx);
        self.workspace.snapshot().ok()
    }

    pub fn open_remote_project(
        &mut self,
        node_id: impl Into<String>,
        root_path: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let node_id = node_id.into();
        let root_path = root_path.into();
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
        self.open_remote_project(node_id, root_path, cx);
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
        let language = language_for_location(location, &text);
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(text, &tokens, cx);
            editor.set_language(language, cx);
            editor
        });
        self.editors.insert(tab_id, editor);
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
        let text = editor.read(cx).buffer().text().to_string();
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
            .bg(rgb(theme.bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, _cx| {
                    window.focus(&this.focus_handle);
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
        root
    }
}

impl Focusable for IdeSurface {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

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
            .bg(rgba((self.tokens.ui.bg << 8) | IDE_BG_HALF_ALPHA));

        let Some(snapshot) = self.workspace.snapshot().ok() else {
            return tree.into_any_element();
        };
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
                        let disabled = self.workspace.has_dirty_buffers()
                            || matches!(self.load_state, IdeLoadState::Loading);
                        div()
                            .size(px(IDE_TREE_TOOLBAR_BUTTON_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .opacity(if disabled { 0.5 } else { 1.0 })
                            .hover(|style| {
                                if disabled {
                                    style
                                } else {
                                    style.bg(rgba((self.tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA))
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
                                    if !disabled {
                                        this.request_open_folder_picker(cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            )
                    }),
            )
            .child(
                div()
                    .id("ide-tree-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .py_1()
                    .child(self.render_directory_children(snapshot.project.root, 0, cx)),
            );
        tree.into_any_element()
    }

    fn render_directory_children(
        &mut self,
        directory: IdeLocation,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let children = self
            .workspace
            .file_tree()
            .children(&directory)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        let mut list = div().flex().flex_col();
        for entry in children {
            let expanded = self.workspace.file_tree().is_expanded(&entry.location);
            list = list.child(self.render_tree_row(entry.clone(), depth, expanded, cx));
            if expanded && matches!(entry.kind, FileKind::Directory) {
                list = list.child(self.render_directory_children(entry.location, depth + 1, cx));
            }
        }
        list
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
        let row_bg = if selected {
            rgba((self.tokens.ui.bg_active << 8) | 0xff)
        } else {
            rgba(0x00000000)
        };
        div()
            .h(px(IDE_ROW_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .bg(row_bg)
            .hover(|style| style.bg(rgba((self.tokens.ui.bg_hover << 8) | IDE_HOVER_ALPHA)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let entry = entry.clone();
                    move |this, _event, _window, cx| {
                        this.open_tree_entry(entry.clone(), cx);
                    }
                }),
            )
            .child(div().w(px((depth as f32) * 14.0)))
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
                self.icon("lucide/folder.svg", IDE_ICON_SIZE, self.tokens.ui.info)
            } else {
                self.icon("lucide/file.svg", IDE_ICON_SIZE, self.tokens.ui.info)
            })
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(self.tokens.ui.text))
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
            .bg(rgba((self.tokens.ui.bg << 8) | IDE_BG_HALF_ALPHA))
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
            .bg(rgba((self.tokens.ui.bg << 8) | IDE_BG_HALF_ALPHA));

        for tab in tabs {
            let active = Some(tab.id) == active_tab;
            let dirty = self.is_tab_dirty(tab.id, cx);
            let tab_id = tab.id;
            row = row.child(
                div()
                    .h_full()
                    .px(px(IDE_TAB_PADDING_X))
                    .py(px(IDE_TAB_PADDING_Y))
                    .flex()
                    .items_center()
                    .gap_1()
                    .border_r_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | IDE_BORDER_HALF_ALPHA))
                    .relative()
                    .bg(if active {
                        rgb(self.tokens.ui.bg_hover)
                    } else {
                        rgba((self.tokens.ui.bg << 8) | IDE_BG_HALF_ALPHA)
                    })
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            let _ = this.workspace.set_active_tab(tab_id);
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .max_w(px(180.0))
                            .truncate()
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

    fn render_empty_editor(&self, _cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(rgba((self.tokens.ui.bg << 8) | IDE_BG_HALF_ALPHA))
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
            .bg(rgb(self.tokens.ui.bg_panel))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.labels.sftp_mode.clone())
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
        for folder in &self.folder_picker.folders {
            let selected = self.folder_picker.selected_folder.as_ref() == Some(&folder.name);
            let folder_name = folder.name.clone();
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

    fn icon(&self, path: &'static str, size: f32, color: u32) -> AnyElement {
        svg()
            .path(path)
            .size(px(size))
            .text_color(rgb(color))
            .into_any_element()
    }
}

async fn open_project_with_root_listing(
    fs: NodeSftpIdeFileSystem,
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
    fs: NodeSftpIdeFileSystem,
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
