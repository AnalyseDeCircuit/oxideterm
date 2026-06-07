// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

pub use oxideterm_preview::{
    PreviewAssetKind as AssetFileKind, PreviewContent, detect_and_decode, encode_to_encoding,
    extension_to_language, font_mime_type, generate_hex_dump, is_font_extension,
    is_likely_text_content,
};

pub mod constants {
    pub const MAX_TEXT_PREVIEW_SIZE: u64 = 1024 * 1024;
    pub const MAX_PREVIEW_SIZE: u64 = 10 * 1024 * 1024;
    pub const MAX_MEDIA_PREVIEW_SIZE: u64 = 50 * 1024 * 1024;
    pub const MAX_OFFICE_CONVERT_SIZE: u64 = 10 * 1024 * 1024;
    pub const HEX_CHUNK_SIZE: u64 = 16 * 1024;
    pub const STREAMING_PREVIEW_CHUNK_SIZE: usize = 256 * 1024;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub size: u64,
    pub modified: i64,
    pub permissions: String,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    File,
    Directory,
    Symlink,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Name,
    NameDesc,
    Size,
    SizeDesc,
    Modified,
    ModifiedDesc,
    Type,
    TypeDesc,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilter {
    #[serde(default)]
    pub show_hidden: bool,
    pub pattern: Option<String>,
    #[serde(default)]
    pub sort: SortOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub id: String,
    pub remote_path: String,
    pub local_path: String,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub speed: u64,
    pub eta_seconds: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferState {
    Pending,
    InProgress,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Backward-compatible namespace for the bulk SFTP chunk cap.
pub struct AdaptiveChunkSizer;

impl AdaptiveChunkSizer {
    pub const MAX_CHUNK: usize = 2 * 1024 * 1024;
}

pub fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "ps1"
            | "bat"
            | "cmd"
            | "bashrc"
            | "zshrc"
            | "profile"
            | "gitconfig"
            | "gitignore"
            | "dockerignore"
            | "conf"
            | "cfg"
            | "ini"
            | "properties"
            | "env"
            | "envrc"
            | "yaml"
            | "yml"
            | "toml"
            | "json"
            | "jsonc"
            | "json5"
            | "xml"
            | "svg"
            | "html"
            | "htm"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "c"
            | "h"
            | "cpp"
            | "java"
            | "go"
            | "rb"
            | "php"
            | "swift"
            | "kt"
            | "scala"
            | "r"
            | "lua"
            | "pl"
            | "sql"
            | "txt"
            | "text"
            | "md"
            | "markdown"
            | "rst"
            | "adoc"
            | "org"
            | "tex"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "dockerfile"
            | "makefile"
            | "mk"
            | "cmake"
            | "gradle"
            | "diff"
            | "patch"
            | "log"
            | "csv"
            | "tsv"
    )
}

pub fn is_office_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "odg" | "rtf"
    )
}
