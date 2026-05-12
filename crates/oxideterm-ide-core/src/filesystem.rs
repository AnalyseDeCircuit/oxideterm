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

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeWatchKey {
    pub node_id: String,
    pub path: String,
}

impl IdeWatchKey {
    pub fn new(node_id: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            path: path.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeWatchEvent {
    pub path: String,
    pub kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeSearchQuery {
    pub pattern: String,
    pub root_path: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub include_hidden: bool,
    pub max_results: u32,
    pub stale_token: u64,
}

impl IdeSearchQuery {
    pub fn tauri_literal_project_search(
        pattern: impl Into<String>,
        root_path: impl Into<String>,
        max_results: u32,
        stale_token: u64,
    ) -> Self {
        Self {
            pattern: pattern.into(),
            root_path: root_path.into(),
            case_sensitive: false,
            regex: false,
            include_globs: tauri_project_search_include_globs(),
            exclude_globs: Vec::new(),
            include_hidden: false,
            max_results,
            stale_token,
        }
    }

    pub fn cache_key(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.root_path,
            self.pattern,
            self.case_sensitive,
            self.regex,
            self.include_hidden,
            self.include_globs.join(","),
            self.exclude_globs.join(",")
        )
    }
}

pub fn tauri_project_search_include_globs() -> Vec<String> {
    [
        "*.ts", "*.tsx", "*.js", "*.jsx", "*.json", "*.rs", "*.toml", "*.md", "*.txt", "*.py",
        "*.go", "*.java", "*.c", "*.cpp", "*.h", "*.css", "*.scss", "*.html", "*.vue", "*.svelte",
        "*.yaml", "*.yml", "*.sh", "*.bash",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
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
