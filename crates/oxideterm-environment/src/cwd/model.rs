// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::hash::{Hash, Hasher};

/// Ownership scope for a terminal current-directory fact.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum CurrentDirectoryScope {
    Local,
    SshNode(String),
}

impl CurrentDirectoryScope {
    pub fn ssh_node(node_id: impl Into<String>) -> Self {
        Self::SshNode(node_id.into())
    }
}

/// Trust source for the cwd value shown in terminal chrome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CurrentDirectorySource {
    ProcessFallback,
    VisibleText,
    ShellIntegration,
}

/// Stable key for cwd-scoped directory listing work.
#[derive(Clone, Debug, Eq)]
pub struct CurrentDirectoryKey {
    scope: CurrentDirectoryScope,
    path: String,
}

impl CurrentDirectoryKey {
    pub fn new(scope: CurrentDirectoryScope, path: impl Into<String>) -> Option<Self> {
        let path = normalize_current_directory_path(path)?;
        Some(Self { scope, path })
    }

    pub fn scope(&self) -> &CurrentDirectoryScope {
        &self.scope
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl PartialEq for CurrentDirectoryKey {
    fn eq(&self, other: &Self) -> bool {
        self.scope == other.scope && self.path == other.path
    }
}

impl Hash for CurrentDirectoryKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.scope.hash(state);
        self.path.hash(state);
    }
}

/// Current directory snapshot for the active terminal shell channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentDirectorySnapshot {
    key: CurrentDirectoryKey,
    source: CurrentDirectorySource,
}

impl CurrentDirectorySnapshot {
    pub fn new(
        scope: CurrentDirectoryScope,
        path: impl Into<String>,
        source: CurrentDirectorySource,
    ) -> Option<Self> {
        Some(Self {
            key: CurrentDirectoryKey::new(scope, path)?,
            source,
        })
    }

    pub fn key(&self) -> &CurrentDirectoryKey {
        &self.key
    }

    pub fn scope(&self) -> &CurrentDirectoryScope {
        self.key.scope()
    }

    pub fn path(&self) -> &str {
        self.key.path()
    }

    pub fn source(&self) -> CurrentDirectorySource {
        self.source
    }
}

/// One selectable directory row in a cwd switcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentDirectoryEntry {
    name: String,
    path: String,
}

impl CurrentDirectoryEntry {
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Option<Self> {
        let name = name.into();
        let name = name.trim();
        let path = normalize_current_directory_path(path)?;
        if name.is_empty() || name.chars().any(char::is_control) {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            path,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Build the visible shell command that changes the active terminal directory.
pub fn current_directory_cd_command(path: &str) -> Option<String> {
    let path = normalize_current_directory_path(path)?;
    Some(format!("cd {}", shell_cd_target(&path)))
}

/// Build the active-shell command that reports cwd through OSC 7.
pub fn current_directory_report_command() -> &'static str {
    "printf '\\033]7;%s\\007' \"${PWD:-$(pwd)}\""
}

/// Return a conservative parent path for POSIX, home-relative, and Windows paths.
pub fn current_directory_parent(path: &str) -> Option<String> {
    let path = normalize_current_directory_path(path)?;
    let separator = path
        .rfind(['/', '\\'])
        .map(|index| (index, path.as_bytes()[index] as char))?;

    if path == "~" || path == "/" || path.ends_with(":\\") || path.ends_with(":/") {
        return None;
    }

    let (index, separator) = separator;
    if index == 0 && separator == '/' {
        return Some("/".to_string());
    }
    if path.starts_with("~/") && index == 1 {
        return Some("~".to_string());
    }
    if path.len() >= 3 && path.as_bytes().get(1) == Some(&b':') && index == 2 {
        return Some(path[..=index].to_string());
    }
    Some(path[..index].to_string())
}

fn normalize_current_directory_path(path: impl Into<String>) -> Option<String> {
    let path = path.into();
    let path = path.trim();
    if path.is_empty() || path.chars().any(char::is_control) {
        return None;
    }
    Some(trim_redundant_trailing_separators(path))
}

fn trim_redundant_trailing_separators(path: &str) -> String {
    let mut end = path.len();
    while end > 1 {
        let candidate = &path[..end];
        if candidate == "~" || candidate.ends_with(":\\") || candidate.ends_with(":/") {
            break;
        }
        let Some(last) = candidate.chars().next_back() else {
            break;
        };
        if last != '/' && last != '\\' {
            break;
        }
        end -= last.len_utf8();
    }
    path[..end].to_string()
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn shell_cd_target(path: &str) -> String {
    if path == "~" {
        return "~".to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return format!("\"$HOME\"/{}", shell_quote(rest));
    }
    shell_quote(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_rejects_empty_and_control_paths() {
        assert!(
            CurrentDirectorySnapshot::new(
                CurrentDirectoryScope::Local,
                "  ",
                CurrentDirectorySource::ShellIntegration,
            )
            .is_none()
        );
        assert!(
            CurrentDirectorySnapshot::new(
                CurrentDirectoryScope::Local,
                "/tmp/bad\npath",
                CurrentDirectorySource::VisibleText,
            )
            .is_none()
        );
    }

    #[test]
    fn parent_handles_home_and_root_paths() {
        assert_eq!(
            current_directory_parent("~/Documents").as_deref(),
            Some("~")
        );
        assert_eq!(
            current_directory_parent("~/Documents/OxideTerm").as_deref(),
            Some("~/Documents")
        );
        assert_eq!(current_directory_parent("/Users").as_deref(), Some("/"));
        assert_eq!(
            current_directory_parent("/Users/dominical").as_deref(),
            Some("/Users")
        );
        assert_eq!(current_directory_parent("/"), None);
    }

    #[test]
    fn parent_handles_windows_drive_paths() {
        assert_eq!(
            current_directory_parent("C:\\Users\\dominical").as_deref(),
            Some("C:\\Users")
        );
        assert_eq!(
            current_directory_parent("C:\\Users").as_deref(),
            Some("C:\\")
        );
        assert_eq!(current_directory_parent("C:\\"), None);
    }

    #[test]
    fn cd_command_quotes_visible_shell_path() {
        assert_eq!(
            current_directory_cd_command("/Users/dominical/it's ok").as_deref(),
            Some("cd '/Users/dominical/it'\\''s ok'")
        );
    }

    #[test]
    fn cd_command_preserves_home_expansion() {
        assert_eq!(current_directory_cd_command("~").as_deref(), Some("cd ~"));
        assert_eq!(
            current_directory_cd_command("~/Project Files").as_deref(),
            Some("cd \"$HOME\"/'Project Files'")
        );
    }

    #[test]
    fn report_command_uses_osc7() {
        let command = current_directory_report_command();
        assert!(command.contains("]7;%s"));
        assert!(command.contains("${PWD:-$(pwd)}"));
    }

    #[test]
    fn entry_rejects_control_names() {
        assert!(CurrentDirectoryEntry::new("ok", "/tmp/ok").is_some());
        assert!(CurrentDirectoryEntry::new("bad\nname", "/tmp/bad").is_none());
    }
}
