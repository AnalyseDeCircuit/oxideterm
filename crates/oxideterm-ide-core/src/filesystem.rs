// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{future::Future, pin::Pin};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{FileTreeEntry, IdeLocation, SavedFileVersion};

/// File-system capability flags exposed to the IDE owner.
///
/// The IDE core keeps these as data instead of probing concrete implementations
/// so local disk and node-first SFTP adapters can report different write
/// guarantees without changing editor state code.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FileSystemCapabilities {
    pub atomic_write: bool,
    pub directory_listing: bool,
    pub conflict_detection: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    CreateOrReplace,
    CreateNew,
    AtomicReplace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileStat {
    pub version: SavedFileVersion,
    pub is_read_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeFileData {
    pub text: String,
    pub version: SavedFileVersion,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeProjectInfo {
    pub root_path: String,
    pub name: String,
    pub is_git_repo: bool,
    pub git_branch: Option<String>,
    pub file_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdePathStat {
    pub size: u64,
    pub mtime: u64,
    pub is_dir: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IdeFileCheck {
    Editable { size: u64, mtime: u64 },
    TooLarge { size: u64, limit: u64 },
    Binary,
    NotEditable { reason: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeFileErrorKind {
    Disconnected,
    Timeout,
    PermissionDenied,
    NotFound,
    Conflict,
    Unsupported,
    Other,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{kind:?}: {message}")]
pub struct IdeFileError {
    pub kind: IdeFileErrorKind,
    pub message: String,
}

impl IdeFileError {
    pub fn new(kind: IdeFileErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Boundary implemented by local and remote file providers.
///
/// The trait is synchronous on purpose for the first IDE core slice. GPUI or
/// NodeRouter integrations should call it from their own async/worker layer and
/// feed successful results back into `IdeWorkspace`.
pub trait IdeFileSystem {
    fn capabilities(&self) -> FileSystemCapabilities;

    fn read_file(&self, location: &IdeLocation) -> Result<IdeFileData, IdeFileError>;

    fn stat(&self, location: &IdeLocation) -> Result<FileStat, IdeFileError>;

    fn list_dir(&self, location: &IdeLocation) -> Result<Vec<FileTreeEntry>, IdeFileError>;

    fn write_file(
        &self,
        location: &IdeLocation,
        text: &str,
        expected_version: Option<&SavedFileVersion>,
        mode: WriteMode,
    ) -> Result<SavedFileVersion, IdeFileError>;
}

pub type IdeFsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, IdeFileError>> + Send + 'a>>;

/// Async file-system boundary used by node-first adapters.
///
/// `oxideterm-ide-core` owns editor/project state, but it must not own SSH,
/// SFTP, or GPUI runtimes. The async trait keeps that Tauri-style separation:
/// upper layers acquire a local or node-backed provider, await file work there,
/// then feed plain data back into `IdeWorkspace`.
pub trait AsyncIdeFileSystem {
    fn capabilities(&self) -> FileSystemCapabilities;

    fn read_file<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, IdeFileData>;

    fn stat<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, FileStat>;

    fn list_dir<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, Vec<FileTreeEntry>>;

    fn write_file<'a>(
        &'a self,
        location: &'a IdeLocation,
        text: &'a str,
        expected_version: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> IdeFsFuture<'a, SavedFileVersion>;
}
