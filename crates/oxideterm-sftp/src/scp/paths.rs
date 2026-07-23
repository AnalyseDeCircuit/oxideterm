// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Path containment and staged replacement for legacy SCP transfers.

use std::path::{Component, Path, PathBuf};

use tokio::fs;
use uuid::Uuid;

use super::{SCP_MAX_DIRECTORY_DEPTH, SCP_MAX_DIRECTORY_ENTRIES, run_exec_exit0};
use crate::{SftpError, SftpExecChannelOpener, remote_parent_path, shell_quote};

pub(super) fn validate_received_name(name: &str) -> Result<(), SftpError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\n', '\r', '\0'])
    {
        return Err(SftpError::ProtocolError(format!(
            "Unsafe SCP file name: {name:?}"
        )));
    }
    Ok(())
}

pub(super) fn validate_remote_path(path: &str) -> Result<(), SftpError> {
    if path.trim().is_empty() || path.contains(['\n', '\r', '\0']) {
        return Err(SftpError::InvalidPath(
            "Remote SCP path is empty or contains control characters".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn safe_local_file_name(path: &Path) -> Result<String, SftpError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            SftpError::InvalidPath(format!("Path has no UTF-8 file name: {}", path.display()))
        })?
        .to_string();
    validate_received_name(&name)?;
    Ok(name)
}

pub(super) fn contained_child_path(parent: &Path, name: &str) -> Result<PathBuf, SftpError> {
    validate_received_name(name)?;
    let child = parent.join(name);
    if child
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(SftpError::ProtocolError(
            "SCP path escaped the destination directory".to_string(),
        ));
    }
    Ok(child)
}

pub(super) fn remote_temporary_path(remote_path: &str) -> Result<String, SftpError> {
    remote_generated_sibling(remote_path, "part")
}

pub(super) fn remote_temporary_directory_path(remote_path: &str) -> Result<String, SftpError> {
    remote_generated_sibling(remote_path, "part-dir")
}

pub(super) fn remote_backup_directory_path(remote_path: &str) -> Result<String, SftpError> {
    remote_generated_sibling(remote_path, "backup-dir")
}

fn remote_generated_sibling(remote_path: &str, suffix: &str) -> Result<String, SftpError> {
    let name = safe_local_file_name(Path::new(remote_path))?;
    let parent = remote_parent_path(remote_path);
    let temporary_name = format!(".{name}.oxideterm-{}.{suffix}", Uuid::new_v4());
    Ok(if parent == "/" {
        format!("/{temporary_name}")
    } else if parent == "." {
        temporary_name
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), temporary_name)
    })
}

pub(super) fn remote_directory_replace_command(
    source: &str,
    destination: &str,
    backup: &str,
) -> String {
    let source = shell_quote(source);
    let destination = shell_quote(destination);
    let backup = shell_quote(backup);
    // The old tree stays recoverable until the staged tree has its final name.
    format!(
        "if [ -e {destination} ]; then \
         mv -- {destination} {backup} || exit $?; \
         if mv -- {source} {destination}; then rm -rf -- {backup} || :; \
         else status=$?; mv -- {backup} {destination}; exit \"$status\"; fi; \
         else mv -- {source} {destination}; fi"
    )
}

pub(super) async fn cleanup_remote_directory<O>(opener: &O, path: &str)
where
    O: SftpExecChannelOpener,
{
    // `path` is always a generated sibling, never a user-supplied directory.
    let command = format!("rm -rf -- {}", shell_quote(path));
    let _ = run_exec_exit0(opener, &command).await;
}

pub(super) fn local_temporary_path(local_path: &Path) -> Result<PathBuf, SftpError> {
    let name = safe_local_file_name(local_path)?;
    Ok(local_path.with_file_name(format!(".{name}.oxideterm-{}.part", Uuid::new_v4())))
}

pub(super) fn local_directory_temporary_path(local_path: &Path) -> Result<PathBuf, SftpError> {
    let name = safe_local_file_name(local_path)?;
    Ok(local_path.with_file_name(format!(".{name}.oxideterm-{}.part-dir", Uuid::new_v4())))
}

pub(super) async fn replace_local_path(source: &Path, destination: &Path) -> Result<(), SftpError> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || {
        // The temporary file is a sibling, so the platform helper can replace
        // an existing destination without exposing a delete-before-rename gap.
        oxideterm_atomic_file::durable_replace(&source, &destination)
    })
    .await
    .map_err(|error| SftpError::IoError(std::io::Error::other(error.to_string())))?
    .map_err(SftpError::IoError)
}

pub(super) async fn replace_local_directory(
    source: &Path,
    destination: &Path,
) -> Result<(), SftpError> {
    let destination_exists = fs::try_exists(destination)
        .await
        .map_err(SftpError::IoError)?;
    let backup = destination_exists.then(|| {
        destination.with_file_name(format!(
            ".{}.oxideterm-{}.backup-dir",
            destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("directory"),
            Uuid::new_v4()
        ))
    });
    if let Some(backup) = backup.as_ref() {
        // Keep the previous directory recoverable until the new tree is visible.
        fs::rename(destination, backup)
            .await
            .map_err(SftpError::IoError)?;
    }
    if let Err(error) = fs::rename(source, destination).await {
        if let Some(backup) = backup.as_ref() {
            let _ = fs::rename(backup, destination).await;
        }
        return Err(SftpError::IoError(error));
    }
    if let Some(backup) = backup {
        let _ = fs::remove_dir_all(backup).await;
    }
    Ok(())
}

pub(super) async fn directory_total_size(path: &Path) -> Result<u64, SftpError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        fn visit(
            path: &Path,
            depth: usize,
            total: &mut u64,
            items: &mut u64,
        ) -> Result<(), SftpError> {
            if depth > SCP_MAX_DIRECTORY_DEPTH {
                return Err(SftpError::ProtocolError(
                    "SCP directory nesting limit exceeded".to_string(),
                ));
            }
            for entry in std::fs::read_dir(path).map_err(SftpError::IoError)? {
                let entry = entry.map_err(SftpError::IoError)?;
                let metadata =
                    std::fs::symlink_metadata(entry.path()).map_err(SftpError::IoError)?;
                if metadata.file_type().is_symlink() {
                    return Err(SftpError::InvalidPath(format!(
                        "SCP recursive upload does not follow symlink: {}",
                        entry.path().display()
                    )));
                }
                *items = items.saturating_add(1);
                ensure_entry_limit(*items)?;
                if metadata.is_dir() {
                    visit(&entry.path(), depth + 1, total, items)?;
                } else if metadata.is_file() {
                    *total = total.saturating_add(metadata.len());
                }
            }
            Ok(())
        }
        let mut total = 0;
        let mut items = 0;
        visit(&path, 0, &mut total, &mut items)?;
        Ok(total)
    })
    .await
    .map_err(|error| SftpError::TransferError(format!("SCP directory scan panicked: {error}")))?
}

pub(super) fn ensure_entry_limit(items: u64) -> Result<(), SftpError> {
    if items > SCP_MAX_DIRECTORY_ENTRIES {
        return Err(SftpError::ProtocolError(
            "SCP directory entry limit exceeded".to_string(),
        ));
    }
    Ok(())
}
