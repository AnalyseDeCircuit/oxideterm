// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Core IDE state for OxideTerm's native editor surface.
//!
//! This crate owns project, tab, dirty-buffer, close-confirmation, and
//! snapshot/restore state. It intentionally does not depend on GPUI, SFTP, SSH,
//! or terminal panes. Upper layers provide file-system adapters and render the
//! state however they need.

mod filesystem;
mod model;
mod tree;
mod workspace;
#[cfg(test)]
mod workspace_tests;

pub use filesystem::{
    AsyncIdeFileSystem, FileStat, FileSystemCapabilities, IdeFileCheck, IdeFileData, IdeFileError,
    IdeFileErrorKind, IdeFileSystem, IdeFsFuture, IdePathStat, IdeProjectInfo, IdeSearchQuery,
    IdeWatchEvent, IdeWatchKey, WriteMode, tauri_project_search_include_globs,
};
pub use model::{
    BufferSnapshot, CloseRequestId, DirtyCloseDecision, DirtyCloseRequest, EditorBuffer, EditorTab,
    EditorTabId, FileKind, FileTreeEntry, IdeLocation, OpenFileOutcome, ProjectId, ProjectSnapshot,
    ReloadError, RestoreSkipReason, RestoreSnapshotResult, SavedFileVersion, WorkspaceSnapshot,
};
pub use tree::{FileTreeDirectorySnapshot, FileTreeSnapshot, FileTreeState};
pub use workspace::{IdeWorkspace, SaveError, WorkspaceError};
