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
mod workspace;

pub use filesystem::{
    FileStat, FileSystemCapabilities, IdeFileData, IdeFileError, IdeFileErrorKind, IdeFileSystem,
    WriteMode,
};
pub use model::{
    BufferSnapshot, CloseRequestId, DirtyCloseDecision, DirtyCloseRequest, EditorBuffer, EditorTab,
    EditorTabId, FileKind, FileTreeEntry, IdeLocation, OpenFileOutcome, ProjectId, ProjectSnapshot,
    ReloadError, RestoreSkipReason, RestoreSnapshotResult, SavedFileVersion, WorkspaceSnapshot,
};
pub use workspace::{IdeWorkspace, WorkspaceError};
