// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use crate::TRZSZ_API_VERSION;
use crate::error::TrzszError;

const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
const WINDOWS_ILLEGAL_CHARS: [char; 8] = ['<', '>', ':', '"', '|', '?', '*', '\0'];

pub fn validate_api_version(api_version: u32) -> Result<(), TrzszError> {
    if api_version != TRZSZ_API_VERSION {
        return Err(TrzszError::InvalidApiVersion {
            expected: TRZSZ_API_VERSION,
            got: api_version,
        });
    }
    Ok(())
}

pub fn validate_owner_id(owner_id: &str) -> Result<(), TrzszError> {
    if owner_id.is_empty() || owner_id.len() > 256 {
        return Err(TrzszError::InvalidOwnerId);
    }

    if owner_id.chars().any(|ch| ch.is_control()) {
        return Err(TrzszError::InvalidOwnerId);
    }

    Ok(())
}

pub fn canonicalize_existing_root(root_path: &str) -> Result<PathBuf, TrzszError> {
    let root = PathBuf::from(root_path);
    let metadata = fs::metadata(&root).map_err(TrzszError::Io)?;
    if !metadata.is_dir() {
        return Err(TrzszError::InvalidPath(format!(
            "Download root is not a directory: {}",
            root.display()
        )));
    }

    fs::canonicalize(&root).map_err(TrzszError::Io)
}

pub fn sanitize_upload_rel_path(path: &Path) -> Result<Vec<String>, TrzszError> {
    path.iter()
        .map(|component| sanitize_component(&component.to_string_lossy()))
        .collect()
}

pub fn sanitize_download_rel_path(file_name: &str) -> Result<Vec<String>, TrzszError> {
    let normalized = file_name.replace('\\', "/");
    let mut components = Vec::new();
    for component in normalized.split('/') {
        components.push(sanitize_component(component)?);
    }

    if components.is_empty() {
        return Err(TrzszError::InvalidPath("Empty relative path".to_string()));
    }

    Ok(components)
}

pub fn build_download_target_path(
    root: &Path,
    rel_components: &[String],
) -> Result<PathBuf, TrzszError> {
    if rel_components.is_empty() {
        return Err(TrzszError::InvalidPath("Empty relative path".to_string()));
    }

    let mut current = root.to_path_buf();
    for component in &rel_components[..rel_components.len().saturating_sub(1)] {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(TrzszError::SymlinkNotAllowed(current.display().to_string()));
                }
                if !metadata.is_dir() {
                    return Err(TrzszError::InvalidPath(format!(
                        "Parent path is not a directory: {}",
                        current.display()
                    )));
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(TrzszError::Io(error)),
        }
    }

    let final_path = rel_components
        .iter()
        .fold(root.to_path_buf(), |mut path, component| {
            path.push(component);
            path
        });

    if let Ok(metadata) = fs::symlink_metadata(&final_path)
        && metadata.file_type().is_symlink()
    {
        return Err(TrzszError::SymlinkNotAllowed(
            final_path.display().to_string(),
        ));
    }

    Ok(final_path)
}

pub fn validate_download_target_path(root: &Path, final_path: &Path) -> Result<(), TrzszError> {
    ensure_within_root(root, final_path)?;

    let parent = final_path.parent().ok_or_else(|| {
        TrzszError::InvalidPath(format!(
            "Final path has no parent: {}",
            final_path.display()
        ))
    })?;
    ensure_within_root(root, parent)?;

    let relative_parent = parent.strip_prefix(root).map_err(|_| {
        TrzszError::InvalidPath(format!(
            "Target escapes prepared root: {}",
            final_path.display()
        ))
    })?;

    let mut current = root.to_path_buf();
    for component in relative_parent.iter() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|error| match error.kind() {
            ErrorKind::NotFound => TrzszError::InvalidPath(format!(
                "Parent path no longer exists: {}",
                current.display()
            )),
            _ => TrzszError::Io(error),
        })?;

        if metadata.file_type().is_symlink() {
            return Err(TrzszError::SymlinkNotAllowed(current.display().to_string()));
        }
        if !metadata.is_dir() {
            return Err(TrzszError::InvalidPath(format!(
                "Parent path is not a directory: {}",
                current.display()
            )));
        }
    }

    if let Ok(metadata) = fs::symlink_metadata(final_path) {
        if metadata.file_type().is_symlink() {
            return Err(TrzszError::SymlinkNotAllowed(
                final_path.display().to_string(),
            ));
        }
        if metadata.is_dir() {
            return Err(TrzszError::InvalidPath(format!(
                "Target path resolves to a directory: {}",
                final_path.display()
            )));
        }
    }

    Ok(())
}

pub fn ensure_within_root(root: &Path, candidate: &Path) -> Result<(), TrzszError> {
    if !candidate.starts_with(root) {
        return Err(TrzszError::InvalidPath(format!(
            "Target escapes prepared root: {}",
            candidate.display()
        )));
    }

    Ok(())
}

fn sanitize_component(component: &str) -> Result<String, TrzszError> {
    if component.is_empty() {
        return Err(TrzszError::InvalidPath("Empty path component".to_string()));
    }

    let normalized = component.nfc().collect::<String>();
    if normalized.is_empty() {
        return Err(TrzszError::InvalidPath("Empty path component".to_string()));
    }

    if normalized == "." || normalized == ".." {
        return Err(TrzszError::InvalidPath(format!(
            "Relative path component is not allowed: {normalized}"
        )));
    }

    if normalized
        .chars()
        .any(|ch| ch == '/' || ch == '\\' || ch.is_control())
    {
        return Err(TrzszError::InvalidPath(format!(
            "Illegal path component: {normalized}"
        )));
    }

    if cfg!(windows) {
        if normalized
            .chars()
            .any(|ch| WINDOWS_ILLEGAL_CHARS.contains(&ch))
        {
            return Err(TrzszError::InvalidPath(format!(
                "Illegal path component: {normalized}"
            )));
        }

        if normalized.ends_with(' ') || normalized.ends_with('.') {
            return Err(TrzszError::ReservedName(normalized));
        }

        let upper = normalized.to_ascii_uppercase();
        if WINDOWS_RESERVED_NAMES.contains(&upper.as_str()) {
            return Err(TrzszError::ReservedName(normalized));
        }
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        build_download_target_path, sanitize_download_rel_path, validate_download_target_path,
    };

    #[test]
    fn rejects_traversal_components() {
        let error = sanitize_download_rel_path("../evil").expect_err("path should be rejected");
        assert!(
            error
                .to_string()
                .contains("Relative path component is not allowed")
        );
    }

    #[test]
    fn creates_missing_parents_inside_root() {
        let temp = tempdir().expect("tempdir");
        let target = build_download_target_path(
            temp.path(),
            &["nested".to_string(), "file.txt".to_string()],
        )
        .expect("target path");

        assert!(temp.path().join("nested").is_dir());
        assert_eq!(target, temp.path().join("nested").join("file.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_target_during_revalidation() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("file.txt");
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, b"outside").expect("write outside");
        symlink(&outside, &target).expect("symlink");

        let error = validate_download_target_path(temp.path(), &target)
            .expect_err("symlink target should be rejected");

        assert!(error.to_string().contains("Symlink is not allowed"));
    }
}
