// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::filesystem::{AsyncIdeFileSystem, IdeFileError, IdeFileSystem, WriteMode};
use crate::model::{
    BufferSnapshot, CloseRequestId, DirtyCloseDecision, DirtyCloseRequest, EditorBuffer, EditorTab,
    EditorTabId, FileTreeEntry, IdeLocation, OpenFileOutcome, ProjectId, ProjectSnapshot,
    ReloadError, RestoreSkipReason, RestoreSnapshotResult, SavedFileVersion, WorkspaceSnapshot,
};
use crate::tree::FileTreeState;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WorkspaceError {
    #[error("no project is open")]
    NoProject,
    #[error("unknown editor tab")]
    UnknownTab,
    #[error("unknown close request")]
    UnknownCloseRequest,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SaveError {
    #[error("unknown editor tab")]
    UnknownTab,
    #[error("file-system error: {0}")]
    File(#[from] IdeFileError),
}

#[derive(Clone, Debug)]
struct ProjectState {
    id: ProjectId,
    root: IdeLocation,
    title: String,
    generation: u64,
}

impl ProjectState {
    fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            id: self.id,
            root: self.root.clone(),
            title: self.title.clone(),
            generation: self.generation,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct IdeWorkspace {
    project: Option<ProjectState>,
    tabs: Vec<EditorTab>,
    buffers: HashMap<EditorTabId, EditorBuffer>,
    tab_by_location: HashMap<String, EditorTabId>,
    active_tab: Option<EditorTabId>,
    tree: FileTreeState,
    pending_close: Option<DirtyCloseRequest>,
    closed_project_keys: HashSet<String>,
    generation: u64,
}

impl IdeWorkspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_project(&mut self, root: IdeLocation, title: impl Into<String>) -> ProjectId {
        self.generation += 1;
        self.closed_project_keys.remove(&root.stable_key());
        self.project = Some(ProjectState {
            id: ProjectId::new(),
            root,
            title: title.into(),
            generation: self.generation,
        });
        self.tabs.clear();
        self.buffers.clear();
        self.tab_by_location.clear();
        self.active_tab = None;
        self.pending_close = None;
        self.tree.clear();
        self.project.as_ref().expect("project just opened").id
    }

    pub fn close_project(&mut self) {
        if let Some(project) = self.project.take() {
            self.closed_project_keys.insert(project.root.stable_key());
        }
        self.tabs.clear();
        self.buffers.clear();
        self.tab_by_location.clear();
        self.active_tab = None;
        self.pending_close = None;
        self.tree.clear();
    }

    pub fn active_tab(&self) -> Option<EditorTabId> {
        self.active_tab
    }

    pub fn set_active_tab(&mut self, tab_id: EditorTabId) -> Result<(), WorkspaceError> {
        if self.tabs.iter().any(|tab| tab.id == tab_id) {
            self.active_tab = Some(tab_id);
            Ok(())
        } else {
            Err(WorkspaceError::UnknownTab)
        }
    }

    pub fn tabs(&self) -> &[EditorTab] {
        &self.tabs
    }

    pub fn pending_close(&self) -> Option<&DirtyCloseRequest> {
        self.pending_close.as_ref()
    }

    pub fn file_tree(&self) -> &FileTreeState {
        &self.tree
    }

    pub fn set_tree_children(
        &mut self,
        directory: IdeLocation,
        children: Vec<FileTreeEntry>,
    ) -> Result<(), WorkspaceError> {
        self.ensure_project()?;
        self.tree.set_children(directory, children);
        Ok(())
    }

    pub fn set_tree_expanded(
        &mut self,
        directory: &IdeLocation,
        expanded: bool,
    ) -> Result<(), WorkspaceError> {
        self.ensure_project()?;
        if expanded {
            self.tree.expand(directory);
        } else {
            self.tree.collapse(directory);
        }
        Ok(())
    }

    pub fn select_tree_entry(
        &mut self,
        location: Option<IdeLocation>,
    ) -> Result<(), WorkspaceError> {
        self.ensure_project()?;
        self.tree.set_selected(location);
        Ok(())
    }

    pub fn buffer(&self, tab_id: EditorTabId) -> Option<&EditorBuffer> {
        self.buffers.get(&tab_id)
    }

    pub fn has_dirty_buffers(&self) -> bool {
        self.buffers.values().any(EditorBuffer::is_dirty)
    }

    pub fn open_file(
        &mut self,
        location: IdeLocation,
        text: impl Into<String>,
        version: SavedFileVersion,
    ) -> Result<OpenFileOutcome, WorkspaceError> {
        self.ensure_project()?;

        let location_key = location.stable_key();
        if let Some(tab_id) = self.tab_by_location.get(&location_key).copied() {
            self.active_tab = Some(tab_id);
            return Ok(OpenFileOutcome::Reused(tab_id));
        }

        let tab_id = EditorTabId::new();
        let title = location.display_name();
        self.tabs.push(EditorTab {
            id: tab_id,
            location: location.clone(),
            title,
            is_pinned: false,
        });
        self.buffers
            .insert(tab_id, EditorBuffer::new(location.clone(), text, version));
        self.tab_by_location.insert(location_key, tab_id);
        self.active_tab = Some(tab_id);

        Ok(OpenFileOutcome::Opened(tab_id))
    }

    pub fn replace_buffer_text(
        &mut self,
        tab_id: EditorTabId,
        text: impl Into<String>,
    ) -> Result<(), WorkspaceError> {
        let text = text.into();
        let buffer = self
            .buffers
            .get_mut(&tab_id)
            .ok_or(WorkspaceError::UnknownTab)?;
        if buffer.text == text {
            return Ok(());
        }
        buffer.text = text;
        buffer.revision += 1;
        Ok(())
    }

    pub fn mark_saved(
        &mut self,
        tab_id: EditorTabId,
        version: SavedFileVersion,
    ) -> Result<(), WorkspaceError> {
        let buffer = self
            .buffers
            .get_mut(&tab_id)
            .ok_or(WorkspaceError::UnknownTab)?;
        buffer.saved_text = buffer.text.clone();
        buffer.version = version;
        buffer.saved_revision = buffer.revision;
        Ok(())
    }

    pub fn save_tab_with(
        &mut self,
        fs: &dyn IdeFileSystem,
        tab_id: EditorTabId,
    ) -> Result<SavedFileVersion, SaveError> {
        let (location, text, expected_version) = {
            let buffer = self.buffers.get(&tab_id).ok_or(SaveError::UnknownTab)?;
            (
                buffer.location.clone(),
                buffer.text.clone(),
                buffer.version.clone(),
            )
        };
        let mode = if fs.capabilities().atomic_write {
            WriteMode::AtomicReplace
        } else {
            WriteMode::CreateOrReplace
        };
        let version = fs.write_file(&location, &text, Some(&expected_version), mode)?;
        self.mark_saved(tab_id, version.clone())
            .map_err(|_| SaveError::UnknownTab)?;
        Ok(version)
    }

    pub async fn save_tab_with_async(
        &mut self,
        fs: &dyn AsyncIdeFileSystem,
        tab_id: EditorTabId,
    ) -> Result<SavedFileVersion, SaveError> {
        let (location, text, expected_version) = {
            let buffer = self.buffers.get(&tab_id).ok_or(SaveError::UnknownTab)?;
            (
                buffer.location.clone(),
                buffer.text.clone(),
                buffer.version.clone(),
            )
        };
        let mode = if fs.capabilities().atomic_write {
            WriteMode::AtomicReplace
        } else {
            WriteMode::CreateOrReplace
        };
        let version = fs
            .write_file(&location, &text, Some(&expected_version), mode)
            .await?;
        self.mark_saved(tab_id, version.clone())
            .map_err(|_| SaveError::UnknownTab)?;
        Ok(version)
    }

    pub fn reload_clean_buffer(
        &mut self,
        tab_id: EditorTabId,
        text: impl Into<String>,
        version: SavedFileVersion,
    ) -> Result<(), ReloadError> {
        let Some(buffer) = self.buffers.get_mut(&tab_id) else {
            return Ok(());
        };
        if buffer.is_dirty() {
            return Err(ReloadError::DirtyBuffer);
        }
        let text = text.into();
        buffer.text = text.clone();
        buffer.saved_text = text;
        buffer.version = version;
        buffer.revision += 1;
        buffer.saved_revision = buffer.revision;
        Ok(())
    }

    pub fn reload_tab_with(
        &mut self,
        fs: &dyn IdeFileSystem,
        tab_id: EditorTabId,
    ) -> Result<(), ReloadError> {
        let location = self
            .buffers
            .get(&tab_id)
            .map(|buffer| buffer.location.clone())
            .ok_or(ReloadError::UnknownTab)?;
        if self
            .buffers
            .get(&tab_id)
            .is_some_and(EditorBuffer::is_dirty)
        {
            return Err(ReloadError::DirtyBuffer);
        }
        let data = fs.read_file(&location).map_err(ReloadError::File)?;
        self.reload_clean_buffer(tab_id, data.text, data.version)
    }

    pub async fn reload_tab_with_async(
        &mut self,
        fs: &dyn AsyncIdeFileSystem,
        tab_id: EditorTabId,
    ) -> Result<(), ReloadError> {
        let location = self
            .buffers
            .get(&tab_id)
            .map(|buffer| buffer.location.clone())
            .ok_or(ReloadError::UnknownTab)?;
        if self
            .buffers
            .get(&tab_id)
            .is_some_and(EditorBuffer::is_dirty)
        {
            return Err(ReloadError::DirtyBuffer);
        }
        let data = fs.read_file(&location).await.map_err(ReloadError::File)?;
        self.reload_clean_buffer(tab_id, data.text, data.version)
    }

    pub fn request_close_tab(
        &mut self,
        tab_id: EditorTabId,
    ) -> Result<Option<DirtyCloseRequest>, WorkspaceError> {
        let tab = self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .ok_or(WorkspaceError::UnknownTab)?;
        let buffer = self
            .buffers
            .get(&tab_id)
            .ok_or(WorkspaceError::UnknownTab)?;

        if !buffer.is_dirty() {
            self.close_tab_now(tab_id);
            return Ok(None);
        }

        // Dirty close state lives in the IDE owner so every surface presents the
        // same save/discard/cancel prompt instead of each UI inventing policy.
        let request = DirtyCloseRequest {
            id: CloseRequestId::new(),
            tab_id,
            title: tab.title.clone(),
            location: tab.location.clone(),
        };
        self.pending_close = Some(request.clone());
        Ok(Some(request))
    }

    pub fn toggle_tab_pin(&mut self, tab_id: EditorTabId) -> Result<bool, WorkspaceError> {
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id)
            .ok_or(WorkspaceError::UnknownTab)?;
        tab.is_pinned = !tab.is_pinned;
        Ok(tab.is_pinned)
    }

    pub fn move_tab_before(
        &mut self,
        tab_id: EditorTabId,
        before_tab_id: EditorTabId,
    ) -> Result<(), WorkspaceError> {
        self.ensure_project()?;
        if tab_id == before_tab_id {
            return Ok(());
        }
        let Some(from) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return Err(WorkspaceError::UnknownTab);
        };
        let Some(mut to) = self.tabs.iter().position(|tab| tab.id == before_tab_id) else {
            return Err(WorkspaceError::UnknownTab);
        };
        let tab = self.tabs.remove(from);
        if from < to {
            to = to.saturating_sub(1);
        }
        self.tabs.insert(to, tab);
        Ok(())
    }

    pub fn move_tab_to_index(
        &mut self,
        tab_id: EditorTabId,
        target_index: usize,
    ) -> Result<(), WorkspaceError> {
        self.ensure_project()?;
        let Some(from) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return Err(WorkspaceError::UnknownTab);
        };
        let target_index = target_index.min(self.tabs.len().saturating_sub(1));
        if from == target_index {
            return Ok(());
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(target_index.min(self.tabs.len()), tab);
        Ok(())
    }

    pub fn request_close_all_tabs(&mut self) -> Result<Option<DirtyCloseRequest>, WorkspaceError> {
        if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| {
                self.buffers
                    .get(&tab.id)
                    .is_some_and(EditorBuffer::is_dirty)
            })
            .cloned()
        {
            return self.request_close_tab(tab.id);
        }
        self.tabs.clear();
        self.buffers.clear();
        self.tab_by_location.clear();
        self.active_tab = None;
        self.pending_close = None;
        Ok(None)
    }

    pub fn resolve_dirty_close(
        &mut self,
        request_id: CloseRequestId,
        decision: DirtyCloseDecision,
    ) -> Result<Option<DirtyCloseRequest>, WorkspaceError> {
        let request = self
            .pending_close
            .clone()
            .filter(|request| request.id == request_id)
            .ok_or(WorkspaceError::UnknownCloseRequest)?;

        match decision {
            DirtyCloseDecision::Save => Ok(Some(request)),
            DirtyCloseDecision::Discard => {
                self.pending_close = None;
                self.close_tab_now(request.tab_id);
                Ok(None)
            }
            DirtyCloseDecision::Cancel => {
                self.pending_close = None;
                Ok(None)
            }
        }
    }

    pub fn complete_dirty_close_after_save(
        &mut self,
        request_id: CloseRequestId,
        version: SavedFileVersion,
    ) -> Result<(), WorkspaceError> {
        let request = self
            .pending_close
            .clone()
            .filter(|request| request.id == request_id)
            .ok_or(WorkspaceError::UnknownCloseRequest)?;
        self.mark_saved(request.tab_id, version)?;
        self.pending_close = None;
        self.close_tab_now(request.tab_id);
        Ok(())
    }

    pub fn snapshot(&self) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let project = self.project.as_ref().ok_or(WorkspaceError::NoProject)?;
        let buffers = self
            .tabs
            .iter()
            .filter_map(|tab| {
                self.buffers.get(&tab.id).map(|buffer| BufferSnapshot {
                    tab_id: tab.id,
                    location: buffer.location.clone(),
                    text: buffer.text.clone(),
                    saved_text: buffer.saved_text.clone(),
                    version: buffer.version.clone(),
                    revision: buffer.revision,
                    saved_revision: buffer.saved_revision,
                })
            })
            .collect();

        Ok(WorkspaceSnapshot {
            project: project.snapshot(),
            tabs: self.tabs.clone(),
            active_tab: self.active_tab,
            buffers,
            tree: self.tree.snapshot(),
        })
    }

    pub fn restore_snapshot(&mut self, snapshot: WorkspaceSnapshot) -> RestoreSnapshotResult {
        let project_key = snapshot.project.root.stable_key();
        if self.closed_project_keys.contains(&project_key) {
            return RestoreSnapshotResult::Skipped(RestoreSkipReason::ProjectWasClosedByUser);
        }

        if let Some(project) = &self.project {
            if project.root.stable_key() != project_key {
                return RestoreSnapshotResult::Skipped(RestoreSkipReason::DifferentProjectOpen);
            }
            if self.has_dirty_buffers() {
                return RestoreSnapshotResult::Skipped(RestoreSkipReason::ExistingDirtyBuffers);
            }
        }

        // Restore replaces clean local IDE state only. Dirty current edits win
        // over stale snapshots because reconnect can race with user typing.
        self.project = Some(ProjectState {
            id: snapshot.project.id,
            root: snapshot.project.root,
            title: snapshot.project.title,
            generation: snapshot.project.generation,
        });
        self.tabs = snapshot.tabs;
        self.tree = FileTreeState::restore(snapshot.tree);
        self.buffers.clear();
        self.tab_by_location.clear();

        for buffer in snapshot.buffers {
            self.tab_by_location
                .insert(buffer.location.stable_key(), buffer.tab_id);
            self.buffers.insert(
                buffer.tab_id,
                EditorBuffer {
                    location: buffer.location,
                    text: buffer.text,
                    saved_text: buffer.saved_text,
                    version: buffer.version,
                    revision: buffer.revision,
                    saved_revision: buffer.saved_revision,
                },
            );
        }
        self.active_tab = snapshot.active_tab;
        self.pending_close = None;

        RestoreSnapshotResult::Restored {
            tab_count: self.tabs.len(),
        }
    }

    fn ensure_project(&self) -> Result<(), WorkspaceError> {
        self.project
            .as_ref()
            .map(|_| ())
            .ok_or(WorkspaceError::NoProject)
    }

    fn close_tab_now(&mut self, tab_id: EditorTabId) {
        if let Some(buffer) = self.buffers.remove(&tab_id) {
            self.tab_by_location.remove(&buffer.location.stable_key());
        }
        self.tabs.retain(|tab| tab.id != tab_id);
        if self.active_tab == Some(tab_id) {
            self.active_tab = self.tabs.last().map(|tab| tab.id);
        }
    }
}
