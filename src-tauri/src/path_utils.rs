// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared path utilities

use std::path::{Path, PathBuf};

/// Expand `~` or `~/...` to the user's home directory.
///
/// Returns the original path unchanged if `~` prefix is not present
/// or if the home directory cannot be determined.
pub fn expand_tilde(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped).to_string_lossy().into_owned();
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

/// Expand `~` or `~/...` to the user's home directory (Path variant).
///
/// Returns the original path unchanged if `~` prefix is not present
/// or if the home directory cannot be determined.
pub fn expand_tilde_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();

    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if path_str == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }

    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde_home_only() {
        let result = expand_tilde("~");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(result, home);
    }

    #[test]
    fn test_expand_tilde_with_subpath() {
        let result = expand_tilde("~/Documents/test");
        let home = dirs::home_dir().unwrap();
        let expected = home.join("Documents/test").to_string_lossy().to_string();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        assert_eq!(expand_tilde("/usr/local/bin"), "/usr/local/bin");
    }

    #[test]
    fn test_expand_tilde_empty() {
        assert_eq!(expand_tilde(""), "");
    }

    #[test]
    fn test_expand_tilde_tilde_not_prefix() {
        // ~ in the middle should NOT be expanded
        assert_eq!(expand_tilde("/home/user/~stuff"), "/home/user/~stuff");
    }

    #[test]
    fn test_expand_tilde_path_home_only() {
        let result = expand_tilde_path(Path::new("~"));
        let home = dirs::home_dir().unwrap();
        assert_eq!(result, home);
    }

    #[test]
    fn test_expand_tilde_path_with_subpath() {
        let result = expand_tilde_path(Path::new("~/Documents/test"));
        let home = dirs::home_dir().unwrap();
        assert_eq!(result, home.join("Documents/test"));
    }

    #[test]
    fn test_expand_tilde_path_absolute() {
        let input = Path::new("/absolute/path");
        let result = expand_tilde_path(input);
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_expand_tilde_path_relative() {
        let input = Path::new("relative/path");
        let result = expand_tilde_path(input);
        assert_eq!(result, PathBuf::from("relative/path"));
    }
}
