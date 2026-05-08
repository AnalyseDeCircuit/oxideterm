// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ProjectId(pub Uuid);

impl ProjectId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct EditorTabId(pub Uuid);

impl EditorTabId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EditorTabId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CloseRequestId(pub Uuid);

impl CloseRequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CloseRequestId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum IdeLocation {
    Local { path: PathBuf },
    Remote { node_id: String, path: String },
}

impl IdeLocation {
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local { path: path.into() }
    }

    pub fn remote(node_id: impl Into<String>, path: impl Into<String>) -> Self {
        Self::Remote {
            node_id: node_id.into(),
            path: path.into(),
        }
    }

    pub fn stable_key(&self) -> String {
        match self {
            Self::Local { path } => format!("local:{}", path.display()),
            Self::Remote { node_id, path } => format!("remote:{node_id}:{path}"),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Local { path } => path
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            Self::Remote { path, .. } => path
                .rsplit('/')
                .find(|part| !part.is_empty())
                .unwrap_or(path)
                .to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SavedFileVersion {
    pub size_bytes: Option<u64>,
    pub modified_millis: Option<i64>,
    pub etag: Option<String>,
}

impl SavedFileVersion {
    pub fn unknown() -> Self {
        Self {
            size_bytes: None,
            modified_millis: None,
            etag: None,
        }
    }
}

impl Default for SavedFileVersion {
    fn default() -> Self {
        Self::unknown()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileTreeEntry {
    pub location: IdeLocation,
    pub kind: FileKind,
    pub name: String,
    pub version: SavedFileVersion,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectSnapshot {
    pub id: ProjectId,
    pub root: IdeLocation,
    pub title: String,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditorBuffer {
    pub location: IdeLocation,
    pub text: String,
    pub saved_text: String,
    pub version: SavedFileVersion,
    pub revision: u64,
    pub saved_revision: u64,
}

impl EditorBuffer {
    pub fn new(location: IdeLocation, text: impl Into<String>, version: SavedFileVersion) -> Self {
        let text = text.into();
        Self {
            location,
            saved_text: text.clone(),
            text,
            version,
            revision: 0,
            saved_revision: 0,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision || self.text != self.saved_text
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditorTab {
    pub id: EditorTabId,
    pub location: IdeLocation,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BufferSnapshot {
    pub tab_id: EditorTabId,
    pub location: IdeLocation,
    pub text: String,
    pub saved_text: String,
    pub version: SavedFileVersion,
    pub revision: u64,
    pub saved_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub project: ProjectSnapshot,
    pub tabs: Vec<EditorTab>,
    pub active_tab: Option<EditorTabId>,
    pub buffers: Vec<BufferSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpenFileOutcome {
    Opened(EditorTabId),
    Reused(EditorTabId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtyCloseDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirtyCloseRequest {
    pub id: CloseRequestId,
    pub tab_id: EditorTabId,
    pub title: String,
    pub location: IdeLocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReloadError {
    DirtyBuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreSkipReason {
    ProjectWasClosedByUser,
    DifferentProjectOpen,
    ExistingDirtyBuffers,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestoreSnapshotResult {
    Restored { tab_count: usize },
    Skipped(RestoreSkipReason),
}
