// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub fn is_absolute_local_path(path: &str) -> bool {
    let candidate = Path::new(path);
    if candidate.is_absolute() || path.starts_with('/') {
        return true;
    }
    if path.len() >= 3 {
        let bytes = path.as_bytes();
        if bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return true;
        }
    }
    path.starts_with("\\\\") || path.starts_with("//")
}

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

pub fn join_remote_path(base: &str, component: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{component}")
    } else {
        format!("{base}/{component}")
    }
}

#[allow(dead_code)]
pub fn normalize_local_path(path: &str) -> String {
    PathBuf::from(path).to_string_lossy().to_string()
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
}
