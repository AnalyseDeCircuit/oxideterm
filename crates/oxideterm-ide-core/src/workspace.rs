// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::model::{
    BufferSnapshot, CloseRequestId, DirtyCloseDecision, DirtyCloseRequest, EditorBuffer, EditorTab,
    EditorTabId, IdeLocation, OpenFileOutcome, ProjectId, ProjectSnapshot, ReloadError,
    RestoreSkipReason, RestoreSnapshotResult, SavedFileVersion, WorkspaceSnapshot,
};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WorkspaceError {
    #[error("no project is open")]
    NoProject,
    #[error("unknown editor tab")]
    UnknownTab,
    #[error("unknown close request")]
    UnknownCloseRequest,
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
    }

    pub fn active_tab(&self) -> Option<EditorTabId> {
        self.active_tab
    }

    pub fn tabs(&self) -> &[EditorTab] {
        &self.tabs
    }

    pub fn pending_close(&self) -> Option<&DirtyCloseRequest> {
        self.pending_close.as_ref()
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
        let buffer = self
            .buffers
            .get_mut(&tab_id)
            .ok_or(WorkspaceError::UnknownTab)?;
        buffer.text = text.into();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn local_file(name: &str) -> IdeLocation {
        IdeLocation::local(format!("/tmp/oxideterm/{name}"))
    }

    #[test]
    fn open_file_reuses_existing_tab_for_same_location() {
        let mut workspace = IdeWorkspace::new();
        workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");

        let first = workspace
            .open_file(
                local_file("main.rs"),
                "fn main() {}",
                SavedFileVersion::unknown(),
            )
            .unwrap();
        let second = workspace
            .open_file(
                local_file("main.rs"),
                "ignored",
                SavedFileVersion::unknown(),
            )
            .unwrap();

        let OpenFileOutcome::Opened(tab_id) = first else {
            panic!("first open should allocate a tab");
        };
        assert_eq!(second, OpenFileOutcome::Reused(tab_id));
        assert_eq!(workspace.tabs().len(), 1);
        assert_eq!(workspace.active_tab(), Some(tab_id));
    }

    #[test]
    fn edits_mark_dirty_and_save_clears_dirty() {
        let mut workspace = IdeWorkspace::new();
        workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
        let OpenFileOutcome::Opened(tab_id) = workspace
            .open_file(local_file("README.md"), "old", SavedFileVersion::unknown())
            .unwrap()
        else {
            panic!("file should open");
        };

        workspace.replace_buffer_text(tab_id, "new").unwrap();
        assert!(workspace.buffer(tab_id).unwrap().is_dirty());

        workspace
            .mark_saved(
                tab_id,
                SavedFileVersion {
                    size_bytes: Some(3),
                    modified_millis: Some(10),
                    etag: Some("v2".to_string()),
                },
            )
            .unwrap();
        assert!(!workspace.buffer(tab_id).unwrap().is_dirty());
    }

    #[test]
    fn dirty_close_requires_confirmation_and_cancel_keeps_tab() {
        let mut workspace = IdeWorkspace::new();
        workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
        let OpenFileOutcome::Opened(tab_id) = workspace
            .open_file(local_file("dirty.txt"), "old", SavedFileVersion::unknown())
            .unwrap()
        else {
            panic!("file should open");
        };
        workspace.replace_buffer_text(tab_id, "new").unwrap();

        let request = workspace.request_close_tab(tab_id).unwrap().unwrap();
        assert_eq!(request.tab_id, tab_id);
        assert!(workspace.pending_close().is_some());

        workspace
            .resolve_dirty_close(request.id, DirtyCloseDecision::Cancel)
            .unwrap();
        assert_eq!(workspace.tabs().len(), 1);
        assert!(workspace.pending_close().is_none());
    }

    #[test]
    fn dirty_close_discard_removes_tab() {
        let mut workspace = IdeWorkspace::new();
        workspace.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
        let OpenFileOutcome::Opened(tab_id) = workspace
            .open_file(local_file("dirty.txt"), "old", SavedFileVersion::unknown())
            .unwrap()
        else {
            panic!("file should open");
        };
        workspace.replace_buffer_text(tab_id, "new").unwrap();
        let request = workspace.request_close_tab(tab_id).unwrap().unwrap();

        workspace
            .resolve_dirty_close(request.id, DirtyCloseDecision::Discard)
            .unwrap();
        assert!(workspace.tabs().is_empty());
        assert!(workspace.buffer(tab_id).is_none());
    }

    #[test]
    fn snapshot_restore_preserves_dirty_buffers_and_active_tab() {
        let mut source = IdeWorkspace::new();
        source.open_project(IdeLocation::remote("node-a", "/home/demo"), "demo");
        let OpenFileOutcome::Opened(first) = source
            .open_file(
                IdeLocation::remote("node-a", "/home/demo/a.rs"),
                "saved",
                SavedFileVersion::unknown(),
            )
            .unwrap()
        else {
            panic!("file should open");
        };
        let OpenFileOutcome::Opened(second) = source
            .open_file(
                IdeLocation::remote("node-a", "/home/demo/b.rs"),
                "b",
                SavedFileVersion::unknown(),
            )
            .unwrap()
        else {
            panic!("file should open");
        };
        source.replace_buffer_text(first, "dirty").unwrap();

        let snapshot = source.snapshot().unwrap();
        let mut restored = IdeWorkspace::new();
        assert_eq!(
            restored.restore_snapshot(snapshot),
            RestoreSnapshotResult::Restored { tab_count: 2 }
        );
        assert_eq!(restored.active_tab(), Some(second));
        assert_eq!(restored.buffer(first).unwrap().text, "dirty");
        assert!(restored.buffer(first).unwrap().is_dirty());
    }

    #[test]
    fn restore_skips_after_user_closed_project() {
        let mut source = IdeWorkspace::new();
        source.open_project(IdeLocation::remote("node-a", "/home/demo"), "demo");
        source
            .open_file(
                IdeLocation::remote("node-a", "/home/demo/a.rs"),
                "a",
                SavedFileVersion::unknown(),
            )
            .unwrap();
        let snapshot = source.snapshot().unwrap();

        let mut target = IdeWorkspace::new();
        target.open_project(IdeLocation::remote("node-a", "/home/demo"), "demo");
        target.close_project();

        assert_eq!(
            target.restore_snapshot(snapshot),
            RestoreSnapshotResult::Skipped(RestoreSkipReason::ProjectWasClosedByUser)
        );
    }

    #[test]
    fn restore_skips_when_current_project_has_dirty_edits() {
        let mut source = IdeWorkspace::new();
        source.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
        source
            .open_file(local_file("a.rs"), "a", SavedFileVersion::unknown())
            .unwrap();
        let snapshot = source.snapshot().unwrap();

        let mut target = IdeWorkspace::new();
        target.open_project(IdeLocation::local("/tmp/oxideterm"), "OxideTerm");
        let OpenFileOutcome::Opened(tab_id) = target
            .open_file(local_file("b.rs"), "b", SavedFileVersion::unknown())
            .unwrap()
        else {
            panic!("file should open");
        };
        target.replace_buffer_text(tab_id, "dirty").unwrap();

        assert_eq!(
            target.restore_snapshot(snapshot),
            RestoreSnapshotResult::Skipped(RestoreSkipReason::ExistingDirtyBuffers)
        );
    }
}
