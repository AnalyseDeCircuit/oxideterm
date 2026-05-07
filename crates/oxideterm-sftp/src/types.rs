// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

pub use oxideterm_preview::{
    PreviewAssetKind as AssetFileKind, PreviewContent, detect_and_decode, encode_to_encoding,
    extension_to_language, generate_hex_dump, is_likely_text_content,
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

pub struct AdaptiveChunkSizer {
    current: usize,
    window_bytes: u64,
    window_start: std::time::Instant,
}

impl AdaptiveChunkSizer {
    pub const MIN_CHUNK: usize = 64 * 1024;
    pub const MAX_CHUNK: usize = 2 * 1024 * 1024;
    const ADAPT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

    pub fn new() -> Self {
        Self {
            current: 256 * 1024,
            window_bytes: 0,
            window_start: std::time::Instant::now(),
        }
    }

    pub fn chunk_size(&self) -> usize {
        self.current
    }

    pub fn record(&mut self, bytes: usize) {
        self.window_bytes += bytes as u64;
        if self.window_start.elapsed() >= Self::ADAPT_INTERVAL {
            let elapsed = self.window_start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                self.current =
                    Self::throughput_to_chunk((self.window_bytes as f64 / elapsed) as u64);
            }
            self.window_bytes = 0;
            self.window_start = std::time::Instant::now();
        }
    }

    fn throughput_to_chunk(bytes_per_sec: u64) -> usize {
        match bytes_per_sec {
            0..=262_144 => Self::MIN_CHUNK,
            262_145..=1_048_576 => 128 * 1024,
            1_048_577..=10_485_760 => 256 * 1024,
            10_485_761..=52_428_800 => 1_048_576,
            _ => Self::MAX_CHUNK,
        }
    }
}

impl Default for AdaptiveChunkSizer {
    fn default() -> Self {
        Self::new()
    }
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
