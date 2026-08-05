// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, path::PathBuf};

use crate::SftpError;

pub fn is_absolute_remote_path(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    if path.len() >= 3 {
        let bytes = path.as_bytes();
        return bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\');
    }
    false
}

pub fn join_local_path(base: &str, component: &str) -> String {
    let mut path = PathBuf::from(base);
    path.push(component);
    path.to_string_lossy().to_string()
}

pub(crate) fn validate_remote_entry_name(name: &str) -> Result<(), SftpError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\n', '\r', '\0'])
    {
        return Err(SftpError::ProtocolError(
            "SFTP directory entry is not a safe path component".to_string(),
        ));
    }
    Ok(())
}

pub fn join_remote_path(base: &str, component: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{component}")
    } else {
        format!("{base}/{component}")
    }
}

/// Normalizes a remote path to the absolute forward-slash form used by SFTP.
pub fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    let normalized = trimmed.replace('\\', "/").replace("//", "/");
    if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    }
}

/// Returns the parent of an absolute remote path without crossing the root.
pub fn remote_parent_path(path: &str) -> String {
    let normalized = normalize_remote_path(path);
    if normalized == "/" {
        return normalized;
    }
    let mut parts = normalized
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    parts.pop();
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

/// Builds each directory prefix needed before creating a nested remote path.
pub fn remote_directory_prefixes(path: &str) -> Vec<String> {
    let absolute = path.starts_with('/');
    let components = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    (0..components.len())
        .map(|index| {
            let joined = components[..=index].join("/");
            if absolute {
                format!("/{joined}")
            } else {
                joined
            }
        })
        .collect()
}

/// Chooses the first desktop-style numbered name that does not already exist.
pub fn unique_conflict_name<'a>(
    name: &str,
    existing_names: impl IntoIterator<Item = &'a str>,
) -> String {
    let existing_names = existing_names.into_iter().collect::<HashSet<_>>();
    let (base_name, extension) = match name.rfind('.') {
        Some(index) if index > 0 => (&name[..index], &name[index..]),
        _ => (name, ""),
    };

    let mut counter = 1usize;
    loop {
        let candidate = format!("{base_name} ({counter}){extension}");
        if !existing_names.contains(candidate.as_str()) {
            return candidate;
        }
        counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_absolute_handles_unix_and_windows_openssh() {
        assert!(is_absolute_remote_path("/home/me"));
        assert!(is_absolute_remote_path("C:/Users/me"));
        assert!(is_absolute_remote_path("D:\\Data"));
        assert!(!is_absolute_remote_path("relative/path"));
    }

    #[test]
    fn joins_remote_paths_with_forward_slashes() {
        assert_eq!(join_remote_path("/home", "file.txt"), "/home/file.txt");
        assert_eq!(join_remote_path("/home/", "file.txt"), "/home/file.txt");
        assert_eq!(join_remote_path("/", "home"), "/home");
    }

    #[test]
    fn rejects_remote_entry_names_that_can_escape_a_local_download_root() {
        assert!(validate_remote_entry_name("notes.txt").is_ok());
        assert!(validate_remote_entry_name("../notes.txt").is_err());
        assert!(validate_remote_entry_name("folder/notes.txt").is_err());
        assert!(validate_remote_entry_name(r"folder\notes.txt").is_err());
        assert!(validate_remote_entry_name("..").is_err());
    }

    #[test]
    fn normalizes_and_navigates_remote_paths() {
        assert_eq!(
            normalize_remote_path(" home//user\\docs "),
            "/home/user/docs"
        );
        assert_eq!(remote_parent_path("/home/user/docs"), "/home/user");
        assert_eq!(remote_parent_path("/home"), "/");
        assert_eq!(remote_parent_path("/"), "/");
    }

    #[test]
    fn remote_prefixes_preserve_absolute_shape() {
        assert_eq!(
            remote_directory_prefixes("/home/user/docs"),
            vec!["/home", "/home/user", "/home/user/docs"]
        );
        assert_eq!(
            remote_directory_prefixes("home/user"),
            vec!["home", "home/user"]
        );
    }

    #[test]
    fn conflict_name_preserves_extension_and_skips_existing_numbers() {
        let existing = ["report.txt", "report (1).txt", "report (2).txt"];

        assert_eq!(
            unique_conflict_name("report.txt", existing),
            "report (3).txt"
        );
        assert_eq!(unique_conflict_name(".env", [".env"]), ".env (1)");
    }
}
