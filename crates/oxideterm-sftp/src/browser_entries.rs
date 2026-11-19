// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SftpFileType {
    File,
    Directory,
}

#[derive(Clone, Debug)]
pub struct SftpFileEntry {
    // Remote components retain their byte identity; decode only for rendering.
    pub name: String,
    pub path: String,
    pub file_type: SftpFileType,
    pub size: u64,
    pub size_known: bool,
    pub modified: Option<i64>,
    pub permissions: Option<String>,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RemoteSftpListing {
    pub cwd: String,
    pub files: Vec<SftpFileEntry>,
}

#[derive(Clone, Debug)]
pub struct SftpPreviewSaveResult {
    pub version: Option<String>,
    pub mtime: Option<u64>,
    pub size: Option<u64>,
    pub encoding_used: String,
    pub atomic_write: bool,
}
