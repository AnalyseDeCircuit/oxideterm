// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{FileInfo, FileType, encode_to_encoding};
use oxideterm_preview::{TextLineEnding, restore_text_line_endings};
use oxideterm_topology::NodeId;
use zeroize::Zeroizing;

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

pub fn remote_listing_from_file_infos(cwd: String, entries: Vec<FileInfo>) -> RemoteSftpListing {
    let mut files = entries
        .into_iter()
        .map(|entry| SftpFileEntry {
            // Selection keys are lossless components, not the rendered label.
            name: entry
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&entry.name)
                .to_owned(),
            path: entry.path,
            file_type: match entry.file_type {
                FileType::Directory => SftpFileType::Directory,
                FileType::File | FileType::Symlink | FileType::Unknown => SftpFileType::File,
            },
            size_known: true,
            size: entry.size,
            modified: Some(entry.modified),
            permissions: Some(entry.permissions),
            owner: entry.owner,
            group: entry.group,
            is_symlink: entry.is_symlink,
            symlink_target: entry.symlink_target,
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| match (left.file_type, right.file_type) {
        (SftpFileType::Directory, SftpFileType::File) => std::cmp::Ordering::Less,
        (SftpFileType::File, SftpFileType::Directory) => std::cmp::Ordering::Greater,
        _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
    });
    RemoteSftpListing { cwd, files }
}

#[derive(Clone, Debug)]
pub struct SftpPreviewSaveResult {
    pub version: Option<String>,
    pub mtime: Option<u64>,
    pub size: Option<u64>,
    pub encoding_used: String,
    pub atomic_write: bool,
}

pub fn encode_preview_text<'a>(
    text: &str,
    encoding: &'a str,
    line_ending: TextLineEnding,
) -> (Zeroizing<Vec<u8>>, &'a str) {
    let encoding = if encoding.trim().is_empty() {
        "UTF-8"
    } else {
        encoding
    };
    let restored = Zeroizing::new(restore_text_line_endings(text, line_ending));
    (
        Zeroizing::new(encode_to_encoding(&restored, encoding)),
        encoding,
    )
}

/// Identifies the remote endpoint without implying NodeRouter ownership.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SftpRemoteId {
    Node(NodeId),
    Standalone(String),
    Ftp(String),
    #[cfg(target_family = "wasm")]
    DeviceFolder(u64),
}

impl SftpRemoteId {
    pub fn storage_key(&self) -> String {
        match self {
            Self::Node(node_id) => node_id.0.clone(),
            Self::Standalone(profile_id) => format!("standalone-sftp:{profile_id}"),
            Self::Ftp(profile_id) => format!("ftp:{profile_id}"),
            #[cfg(target_family = "wasm")]
            Self::DeviceFolder(id) => format!("device-folder:{id}"),
        }
    }

    pub fn node_id(&self) -> Option<&NodeId> {
        match self {
            Self::Node(node_id) => Some(node_id),
            #[cfg(target_family = "wasm")]
            Self::DeviceFolder(_) => None,
            Self::Standalone(_) | Self::Ftp(_) => None,
        }
    }

    pub fn standalone_endpoint_id(&self) -> Option<&str> {
        match self {
            #[cfg(target_family = "wasm")]
            Self::DeviceFolder(_) => None,
            Self::Node(_) | Self::Ftp(_) => None,
            Self::Standalone(endpoint_id) => Some(endpoint_id),
        }
    }
}
