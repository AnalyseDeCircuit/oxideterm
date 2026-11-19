// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{SftpFileEntry, SftpFileType, normalize_remote_path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SftpSortField {
    Name,
    Size,
    Modified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SftpSortDirection {
    Asc,
    Desc,
}

pub fn list_local_files(path: &str) -> std::io::Result<Vec<SftpFileEntry>> {
    oxideterm_local_files::list_local_files(&oxideterm_local_files::normalize_local_path(path))
        .map(|files| files.into_iter().map(sftp_file_entry_from_local).collect())
}

fn sftp_file_entry_from_local(entry: oxideterm_local_files::LocalFileEntry) -> SftpFileEntry {
    let is_symlink = entry.file_type == oxideterm_local_files::LocalFileType::Symlink;
    SftpFileEntry {
        name: entry.name,
        path: entry.path,
        file_type: match entry.file_type {
            oxideterm_local_files::LocalFileType::Directory => SftpFileType::Directory,
            oxideterm_local_files::LocalFileType::File
            | oxideterm_local_files::LocalFileType::Symlink => SftpFileType::File,
        },
        size_known: true,
        size: entry.size,
        modified: entry.modified,
        permissions: None,
        owner: None,
        group: None,
        is_symlink,
        symlink_target: entry.symlink_target,
    }
}

pub fn sorted_sftp_files(
    files: &[SftpFileEntry],
    filter: &str,
    sort_field: SftpSortField,
    sort_direction: SftpSortDirection,
) -> Vec<SftpFileEntry> {
    let filter = filter.trim().to_lowercase();
    let mut filtered = files
        .iter()
        .filter(|file| filter.is_empty() || file.name.to_lowercase().contains(&filter))
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| {
        if left.file_type == SftpFileType::Directory && right.file_type != SftpFileType::Directory {
            return std::cmp::Ordering::Less;
        }
        if left.file_type != SftpFileType::Directory && right.file_type == SftpFileType::Directory {
            return std::cmp::Ordering::Greater;
        }
        let ordering = match sort_field {
            SftpSortField::Name => left.name.cmp(&right.name),
            SftpSortField::Size => left.size.cmp(&right.size),
            SftpSortField::Modified => left.modified.cmp(&right.modified),
        };
        match sort_direction {
            SftpSortDirection::Asc => ordering,
            SftpSortDirection::Desc => ordering.reverse(),
        }
    });
    filtered
}

pub fn sftp_path_segments(
    path: &str,
    is_remote: bool,
) -> Vec<oxideterm_local_files::LocalPathSegment> {
    if !is_remote {
        return oxideterm_local_files::local_path_segments(path);
    }

    let normalized = normalize_remote_path(path);
    let mut segments = vec![oxideterm_local_files::LocalPathSegment {
        name: "/".to_string(),
        full_path: "/".to_string(),
        root_is_drive: false,
    }];
    let without_root = normalized.trim_start_matches('/');
    let mut current = String::from("/");
    for part in without_root.split('/').filter(|part| !part.is_empty()) {
        current = if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };
        segments.push(oxideterm_local_files::LocalPathSegment {
            name: part.to_string(),
            full_path: current.clone(),
            root_is_drive: false,
        });
    }
    segments
}

pub fn parent_path(path: &str, remote: bool) -> String {
    if remote {
        return crate::remote_parent_path(path);
    }
    oxideterm_local_files::local_parent_path(path)
        .unwrap_or_else(|| oxideterm_local_files::normalize_local_path(path))
}

pub use oxideterm_local_files::join_local_path;

pub fn normalize_external_dropped_path(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let raw = path.to_string_lossy();
    if raw.trim().is_empty() {
        return None;
    }
    if raw.len() >= 2
        && raw.as_bytes()[1] == b':'
        && raw.chars().skip(2).all(|ch| ch == '/' || ch == '\\')
    {
        return Some(std::path::PathBuf::from(format!("{}:\\", &raw[..1])));
    }
    if raw.chars().all(|ch| ch == '/' || ch == '\\') {
        return raw
            .chars()
            .next()
            .map(|root| std::path::PathBuf::from(root.to_string()));
    }
    Some(std::path::PathBuf::from(raw.trim_end_matches(['/', '\\'])))
}

pub fn unique_sftp_conflict_name(name: &str, existing_files: &[SftpFileEntry]) -> String {
    crate::unique_conflict_name(name, existing_files.iter().map(|file| file.name.as_str()))
}

pub fn sftp_sorted_filtered_indices(
    files: &[SftpFileEntry],
    filter: &str,
    sort_field: SftpSortField,
    sort_direction: SftpSortDirection,
) -> Vec<usize> {
    let filter = filter.trim().to_lowercase();
    let mut indices = files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            (filter.is_empty()
                || crate::display_remote_path(&file.name)
                    .to_lowercase()
                    .contains(&filter))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    // Sort lightweight indices so the virtual list never owns a duplicate
    // of every file entry merely to preserve the requested presentation.
    indices.sort_by(|left_index, right_index| {
        let left = &files[*left_index];
        let right = &files[*right_index];
        if left.file_type == SftpFileType::Directory && right.file_type != SftpFileType::Directory {
            return std::cmp::Ordering::Less;
        }
        if left.file_type != SftpFileType::Directory && right.file_type == SftpFileType::Directory {
            return std::cmp::Ordering::Greater;
        }
        let ordering = match sort_field {
            SftpSortField::Name => left.name.cmp(&right.name),
            SftpSortField::Size => left.size.cmp(&right.size),
            SftpSortField::Modified => left.modified.cmp(&right.modified),
        };
        match sort_direction {
            SftpSortDirection::Asc => ordering,
            SftpSortDirection::Desc => ordering.reverse(),
        }
    });
    indices
}

#[cfg(test)]
mod sftp_helper_tests {
    use super::*;

    #[test]
    fn local_listing_reads_the_directory_again() {
        let directory =
            std::env::temp_dir().join(format!("oxideterm-sftp-refresh-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("temporary directory should be created");
        let path = directory.to_string_lossy();

        let initial_files = list_local_files(&path).expect("initial listing should succeed");
        std::fs::write(directory.join("country.mmdb"), b"test")
            .expect("fixture file should be created");
        let refreshed_files = list_local_files(&path).expect("second listing should succeed");

        assert!(!initial_files.iter().any(|file| file.name == "country.mmdb"));
        assert!(
            refreshed_files
                .iter()
                .any(|file| file.name == "country.mmdb")
        );

        // The generated UUID keeps cleanup scoped to this test's directory.
        std::fs::remove_dir_all(&directory).expect("temporary directory should be removed");
    }

    #[test]
    fn local_navigation_preserves_windows_drive_roots() {
        let segments = sftp_path_segments(r"D:\Projects\OxideTerm", false);

        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.full_path.as_str())
                .collect::<Vec<_>>(),
            [r"D:\", r"D:\Projects", r"D:\Projects\OxideTerm"]
        );
        assert_eq!(parent_path(r"D:\Projects\OxideTerm", false), r"D:\Projects");
        assert_eq!(parent_path(r"D:\", false), r"D:\");
    }
}
