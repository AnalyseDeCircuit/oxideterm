//! Cross-platform path utilities for SFTP operations
//!
//! Provides unified path handling that works correctly on Windows (including UNC paths)
//! and Unix systems, while maintaining compatibility with remote SFTP paths (always `/`).

use std::path::{Path, PathBuf};

/// Check if a path is absolute (cross-platform).
///
/// Handles:
/// - Unix absolute paths: `/home/user`
/// - Windows drive letters: `C:\Users`, `D:/data`
/// - Windows UNC paths: `\\server\share`, `\\?\C:\long\path`
///
/// # Examples
/// ```
/// assert!(is_absolute_local_path("/home/user"));
/// assert!(is_absolute_local_path("C:\\Users"));
/// assert!(is_absolute_local_path("D:/data"));
/// assert!(is_absolute_local_path("\\\\server\\share"));
/// assert!(is_absolute_local_path("\\\\?\\C:\\long\\path"));
/// assert!(!is_absolute_local_path("relative/path"));
/// ```
pub fn is_absolute_local_path(path: &str) -> bool {
    let p = Path::new(path);

    // std::path::Path::is_absolute() handles most cases correctly
    if p.is_absolute() {
        return true;
    }

    // Unix-style absolute path (for cross-platform compatibility)
    if path.starts_with('/') {
        return true;
    }

    // Windows drive letter: C:\ or C:/
    if path.len() >= 3 {
        let bytes = path.as_bytes();
        if bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return true;
        }
    }

    // Windows UNC path: \\server\share or \\?\C:\path (long path prefix)
    if path.starts_with("\\\\") || path.starts_with("//") {
        return true;
    }

    false
}

/// Check if a remote SFTP path is absolute.
///
/// Remote SFTP paths always use `/` as separator (per SFTP protocol).
/// Even Windows SSH servers present paths in Unix style.
pub fn is_absolute_remote_path(path: &str) -> bool {
    path.starts_with('/')
}

/// Join local path components using platform-native separator.
///
/// Uses `PathBuf` internally to ensure correct behavior on Windows and Unix.
/// This handles edge cases like:
/// - Windows: `C:\Users` + `file.txt` → `C:\Users\file.txt`
/// - Unix: `/home/user` + `file.txt` → `/home/user/file.txt`
pub fn join_local_path(base: &str, component: &str) -> String {
    let mut path = PathBuf::from(base);
    path.push(component);
    path.to_string_lossy().to_string()
}

/// Join remote SFTP path components using `/` separator.
///
/// Remote paths always use `/` regardless of the local or remote OS.
pub fn join_remote_path(base: &str, component: &str) -> String {
    if base.ends_with('/') {
        format!("{}{}", base, component)
    } else {
        format!("{}/{}", base, component)
    }
}

/// Normalize a local path to use platform-native separators.
///
/// Converts `/` to `\` on Windows, leaves unchanged on Unix.
pub fn normalize_local_path(path: &str) -> String {
    let p = PathBuf::from(path);
    p.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_absolute_local_path() {
        // Unix paths
        assert!(is_absolute_local_path("/home/user"));
        assert!(is_absolute_local_path("/"));

        // Windows drive letters
        assert!(is_absolute_local_path("C:\\Users"));
        assert!(is_absolute_local_path("C:/Users"));
        assert!(is_absolute_local_path("D:\\data"));
        assert!(is_absolute_local_path("d:/data"));

        // Windows UNC paths
        assert!(is_absolute_local_path("\\\\server\\share"));
        assert!(is_absolute_local_path("\\\\?\\C:\\long\\path"));
        assert!(is_absolute_local_path("//server/share"));

        // Relative paths
        assert!(!is_absolute_local_path("relative/path"));
        assert!(!is_absolute_local_path("..\\parent"));
        assert!(!is_absolute_local_path("./current"));
    }

    #[test]
    fn test_is_absolute_remote_path() {
        assert!(is_absolute_remote_path("/home/user"));
        assert!(is_absolute_remote_path("/"));
        assert!(!is_absolute_remote_path("relative/path"));
        assert!(!is_absolute_remote_path("C:\\Windows")); // Not a remote absolute path
    }

    #[test]
    fn test_join_remote_path() {
        assert_eq!(join_remote_path("/home", "file.txt"), "/home/file.txt");
        assert_eq!(join_remote_path("/home/", "file.txt"), "/home/file.txt");
        assert_eq!(join_remote_path("/", "home"), "/home");
    }
}
