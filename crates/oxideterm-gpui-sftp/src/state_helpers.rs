// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub fn is_sftp_incomplete_store_compat_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("deserialize")
        || error.contains("invalid type")
        || error.contains("connection_not_found")
        || error.contains("notfound")
        || error.contains("not found")
}

pub fn home_path() -> String {
    oxideterm_local_files::home_path()
}

pub fn default_download_path() -> String {
    oxideterm_local_files::default_download_path()
}

pub use oxideterm_sftp::browser_files::{
    join_local_path, list_local_files, normalize_external_dropped_path, parent_path,
    sftp_path_segments, sorted_sftp_files, unique_sftp_conflict_name,
};

pub fn local_files_or_error(
    path: &str,
    listing: std::io::Result<Vec<SftpFileEntry>>,
) -> Vec<SftpFileEntry> {
    listing.unwrap_or_else(|error| {
        vec![sftp_file_entry(
            format!("Unable to read folder: {error}"),
            path.to_string(),
            SftpFileType::File,
            0,
            None,
        )]
    })
}

pub(crate) fn local_drives() -> Vec<SftpDrive> {
    oxideterm_local_files::local_drives()
        .into_iter()
        .map(sftp_drive_from_local)
        .collect()
}

pub(crate) fn sftp_drive_from_local(drive: oxideterm_local_files::LocalDrive) -> SftpDrive {
    SftpDrive {
        name: drive.name,
        path: drive.path,
        drive_type: drive.drive_type,
        total_space: drive.total_space,
        available_space: drive.available_space,
        read_only: drive.read_only,
    }
}

pub fn sftp_file_entry(
    name: String,
    path: String,
    file_type: SftpFileType,
    size: u64,
    modified: Option<i64>,
) -> SftpFileEntry {
    SftpFileEntry {
        name,
        path,
        file_type,
        size,
        size_known: true,
        modified,
        permissions: None,
        owner: None,
        group: None,
        is_symlink: false,
        symlink_target: None,
    }
}

pub fn new_sftp_transfer_id(remote_id: &SftpRemoteId, name: &str) -> String {
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let safe_name = name.replace(['/', '\\', ':'], "_");
    format!("{}-{timestamp_ms}-{safe_name}", remote_id.storage_key())
}

pub fn sftp_conflict_resolution_from_settings(
    action: oxideterm_settings::ConflictAction,
) -> SftpConflictResolution {
    match action {
        oxideterm_settings::ConflictAction::Ask | oxideterm_settings::ConflictAction::Overwrite => {
            SftpConflictResolution::Overwrite
        }
        oxideterm_settings::ConflictAction::Skip => SftpConflictResolution::Skip,
        oxideterm_settings::ConflictAction::Rename => SftpConflictResolution::Rename,
    }
}

pub fn sftp_transfer_conflicts(
    pending_transfers: &[SftpPendingTransfer],
    target_files: &[SftpFileEntry],
) -> Vec<SftpConflictInfo> {
    oxideterm_sftp::find_transfer_conflicts(
        pending_transfers
            .iter()
            .map(|transfer| oxideterm_sftp::ConflictTransfer {
                name: &transfer.name,
                source_size: transfer.source.size,
                source_modified: transfer.source.modified,
                source_is_directory: transfer.source.file_type == SftpFileType::Directory,
                direction: transfer.direction,
            }),
        target_files
            .iter()
            .map(|target| oxideterm_sftp::ConflictTarget {
                name: &target.name,
                size: target.size,
                modified: target.modified,
                is_directory: target.file_type == SftpFileType::Directory,
            }),
    )
}

pub fn sftp_source_not_newer_than_target(
    transfer: &SftpPendingTransfer,
    target_files: &[SftpFileEntry],
) -> bool {
    oxideterm_sftp::source_not_newer_than_target(
        &transfer.name,
        transfer.source.modified,
        target_files
            .iter()
            .map(|target| oxideterm_sftp::ConflictTarget {
                name: &target.name,
                size: target.size,
                modified: target.modified,
                is_directory: target.file_type == SftpFileType::Directory,
            }),
    )
}

pub fn sftp_transfer_state_from_background(state: BackgroundTransferState) -> SftpTransferState {
    match state {
        BackgroundTransferState::Pending => SftpTransferState::Pending,
        BackgroundTransferState::Active => SftpTransferState::Active,
        BackgroundTransferState::Paused => SftpTransferState::Paused,
        BackgroundTransferState::Completed => SftpTransferState::Completed,
        BackgroundTransferState::Cancelled => SftpTransferState::Cancelled,
        BackgroundTransferState::Error => SftpTransferState::Error,
    }
}

pub fn preview_content_text(content: &PreviewContent) -> String {
    match content {
        PreviewContent::Text {
            data,
            encoding,
            confidence,
            has_bom,
            ..
        } => {
            let bom = if *has_bom { ", BOM" } else { "" };
            format!(
                "encoding: {encoding} ({:.0}%{bom})\n\n{data}",
                confidence * 100.0
            )
        }
        PreviewContent::Image { mime_type, data } => {
            format!(
                "{mime_type}\nimage preview payload: {} base64 chars",
                data.len()
            )
        }
        PreviewContent::AssetFile {
            path,
            mime_type,
            kind,
        } => {
            format!("{kind:?} asset\n{mime_type}\n{path}")
        }
        PreviewContent::Hex {
            data,
            total_size,
            offset,
            chunk_size,
            has_more,
        } => {
            format!(
                "hex preview: offset {offset}, chunk {chunk_size}, total {total_size}, has_more {has_more}\n\n{data}"
            )
        }
        PreviewContent::TooLarge {
            size,
            max_size,
            recommend_download,
        } => {
            format!(
                "too large to preview: {size} bytes (limit {max_size}), recommend_download={recommend_download}"
            )
        }
        PreviewContent::Unsupported { mime_type, reason } => {
            format!("unsupported preview: {mime_type}\n{reason}")
        }
    }
}

pub fn sftp_preview_is_markdown(language: Option<&str>, mime_type: Option<&str>) -> bool {
    language.is_some_and(|language| {
        matches!(
            language.to_ascii_lowercase().as_str(),
            "markdown" | "md" | "rmd"
        )
    }) || mime_type.is_some_and(|mime_type| {
        matches!(
            mime_type.to_ascii_lowercase().as_str(),
            "text/markdown" | "text/x-markdown"
        )
    })
}

pub fn sftp_editor_language(language: Option<&str>, name: &str) -> String {
    let raw = language
        .filter(|language| !language.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::path::Path::new(name)
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "text".to_string());
    match raw.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" => "typescript",
        "md" | "markdown" => "markdown",
        "yml" => "yaml",
        "sh" | "bash" | "zsh" => "bash",
        "makefile" | "mk" => "make",
        "txt" | "text" | "conf" | "cfg" | "ini" | "env" => "text",
        other => other,
    }
    .to_string()
}

pub fn sftp_editor_language_id(
    language: Option<&str>,
    path: Option<&str>,
    name: &str,
    source: &str,
) -> Option<LanguageId> {
    // Prefer the remote path because the dialog title can be shortened while the path keeps the real extension.
    path.and_then(|path| LanguageId::detect(Some(Path::new(path)), source))
        .or_else(|| LanguageId::detect(Some(Path::new(name)), source))
        .or_else(|| language.and_then(sftp_editor_language_name_id))
}

pub(crate) fn sftp_editor_language_name_id(language: &str) -> Option<LanguageId> {
    match language.trim().to_ascii_lowercase().as_str() {
        "bash" | "sh" | "shell" => Some(LanguageId::Bash),
        "c" => Some(LanguageId::C),
        "cmake" => Some(LanguageId::CMake),
        "csharp" | "c#" | "cs" => Some(LanguageId::CSharp),
        "cpp" | "c++" | "cc" | "cxx" => Some(LanguageId::Cpp),
        "css" => Some(LanguageId::Css),
        "diff" | "patch" => Some(LanguageId::Diff),
        "dockerfile" | "containerfile" => Some(LanguageId::Dockerfile),
        "elixir" | "ex" | "exs" => Some(LanguageId::Elixir),
        "fish" => Some(LanguageId::Fish),
        "go" => Some(LanguageId::Go),
        "hcl" | "terraform" | "tf" | "tfvars" => Some(LanguageId::Hcl),
        "html" => Some(LanguageId::Html),
        "java" => Some(LanguageId::Java),
        "javascript" | "js" | "jsx" => Some(LanguageId::Javascript),
        "json" | "jsonc" => Some(LanguageId::Json),
        "lisp" | "commonlisp" => Some(LanguageId::Lisp),
        "lua" => Some(LanguageId::Lua),
        "make" | "makefile" => Some(LanguageId::Make),
        "markdown" | "md" | "mdx" => Some(LanguageId::Markdown),
        "nginx" => Some(LanguageId::Nginx),
        "objective-c" | "objectivec" | "objc" => Some(LanguageId::ObjectiveC),
        "perl" | "pl" => Some(LanguageId::Perl),
        "php" => Some(LanguageId::Php),
        "powershell" | "pwsh" | "ps1" => Some(LanguageId::Powershell),
        "proto" | "protobuf" => Some(LanguageId::Protobuf),
        "python" | "py" => Some(LanguageId::Python),
        "r" => Some(LanguageId::R),
        "ruby" | "rb" => Some(LanguageId::Ruby),
        "rust" | "rs" => Some(LanguageId::Rust),
        "scala" => Some(LanguageId::Scala),
        "sql" => Some(LanguageId::Sql),
        "swift" => Some(LanguageId::Swift),
        "toml" => Some(LanguageId::Toml),
        "tsx" => Some(LanguageId::Tsx),
        "typescript" | "ts" => Some(LanguageId::TypeScript),
        "yaml" | "yml" => Some(LanguageId::Yaml),
        "zsh" => Some(LanguageId::Zsh),
        "zig" => Some(LanguageId::Zig),
        _ => None,
    }
}

pub fn sftp_preview_editor_is_network_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    [
        "network",
        "connection",
        "timeout",
        "disconnected",
        "eof",
        "broken pipe",
        "reset by peer",
        "channel closed",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

pub fn sftp_path_completion_candidate(entry: SftpFileEntry) -> PathCompletionCandidate {
    PathCompletionCandidate {
        name: entry.name,
        path: entry.path,
        is_directory: entry.file_type == SftpFileType::Directory,
    }
}
