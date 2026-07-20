// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Local filesystem behavior for terminal current-directory awareness.

use std::{cmp::Ordering, path::PathBuf};

use super::model::{CurrentDirectoryEntry, CurrentDirectoryEntryKind};

/// List a local directory using the same normalized paths exposed to shells.
pub fn list_local_current_directory(
    cwd: &str,
    max_entries: usize,
) -> Option<Vec<CurrentDirectoryEntry>> {
    let actual_cwd = expand_local_home_path(cwd);
    let entries = std::fs::read_dir(&actual_cwd).ok()?;
    let mut rows = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
            let kind = if metadata.is_dir() {
                CurrentDirectoryEntryKind::Directory
            } else if metadata.is_file() || metadata.file_type().is_symlink() {
                CurrentDirectoryEntryKind::File
            } else {
                return None;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            let path = display_child_path(cwd, &entry.path(), &name);
            CurrentDirectoryEntry::new_with_kind(name, path, kind)
        })
        .collect::<Vec<_>>();
    sort_current_directory_entries(&mut rows);
    rows.truncate(max_entries);
    Some(rows)
}

/// Sort directories before files and compare names case-insensitively.
pub fn sort_current_directory_entries(entries: &mut [CurrentDirectoryEntry]) {
    entries.sort_by(current_directory_entry_order);
}

/// Return whether a typed value is an explicit path rather than a bare name.
pub fn current_directory_path_is_explicit(value: &str) -> bool {
    value == "~"
        || value.starts_with("~/")
        || value.starts_with('/')
        || value.starts_with("\\\\")
        || (value.len() > 2 && value.as_bytes().get(1) == Some(&b':'))
}

pub(crate) fn expand_local_home_path(path: &str) -> PathBuf {
    let path = path.trim();
    if path == "~" {
        return local_home().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = local_home()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

fn current_directory_entry_order(
    left: &CurrentDirectoryEntry,
    right: &CurrentDirectoryEntry,
) -> Ordering {
    match (left.kind(), right.kind()) {
        (CurrentDirectoryEntryKind::Directory, CurrentDirectoryEntryKind::File) => Ordering::Less,
        (CurrentDirectoryEntryKind::File, CurrentDirectoryEntryKind::Directory) => {
            Ordering::Greater
        }
        _ => left.name().to_lowercase().cmp(&right.name().to_lowercase()),
    }
}

fn local_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

fn display_child_path(cwd: &str, absolute_path: &std::path::Path, name: &str) -> String {
    let cwd = cwd.trim_end_matches(['/', '\\']);
    if cwd == "~" {
        format!("~/{name}")
    } else if cwd.starts_with("~/") {
        format!("{cwd}/{name}")
    } else {
        absolute_path.to_string_lossy().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "oxideterm-environment-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn explicit_path_detection_rejects_bare_names() {
        assert!(!current_directory_path_is_explicit("Documents"));
        assert!(current_directory_path_is_explicit("~/Documents"));
        assert!(current_directory_path_is_explicit("/Users/example"));
        assert!(current_directory_path_is_explicit("C:\\Users"));
    }

    #[test]
    fn display_child_preserves_home_relative_paths() {
        assert_eq!(
            display_child_path("~", std::path::Path::new("/home/a/Documents"), "Documents"),
            "~/Documents"
        );
        assert_eq!(
            display_child_path(
                "~/Documents",
                std::path::Path::new("/home/a/Documents/OxideTerm"),
                "OxideTerm",
            ),
            "~/Documents/OxideTerm"
        );
    }

    #[test]
    fn entry_sort_keeps_directories_before_files() {
        let mut entries = vec![
            CurrentDirectoryEntry::new_file("A.txt", "/tmp/A.txt").unwrap(),
            CurrentDirectoryEntry::new("z-dir", "/tmp/z-dir").unwrap(),
        ];

        sort_current_directory_entries(&mut entries);

        assert_eq!(entries[0].kind(), CurrentDirectoryEntryKind::Directory);
    }

    #[test]
    fn local_listing_returns_sorted_domain_entries() {
        let root = temporary_directory("cwd-list");
        std::fs::create_dir_all(root.join("z-dir")).unwrap();
        std::fs::write(root.join("A.txt"), "test").unwrap();

        let entries = list_local_current_directory(&root.to_string_lossy(), 10).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name(), "z-dir");
        assert_eq!(entries[0].kind(), CurrentDirectoryEntryKind::Directory);
        assert_eq!(entries[1].name(), "A.txt");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
