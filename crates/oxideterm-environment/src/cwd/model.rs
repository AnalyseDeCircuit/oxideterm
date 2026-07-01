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

/// Kind of path row shown in a current-directory picker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CurrentDirectoryEntryKind {
    Directory,
    File,
}

/// One selectable path row in a cwd switcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentDirectoryEntry {
    name: String,
    path: String,
    kind: CurrentDirectoryEntryKind,
}

impl CurrentDirectoryEntry {
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Option<Self> {
        Self::new_with_kind(name, path, CurrentDirectoryEntryKind::Directory)
    }

    pub fn new_file(name: impl Into<String>, path: impl Into<String>) -> Option<Self> {
        Self::new_with_kind(name, path, CurrentDirectoryEntryKind::File)
    }

    pub fn new_with_kind(
        name: impl Into<String>,
        path: impl Into<String>,
        kind: CurrentDirectoryEntryKind,
    ) -> Option<Self> {
        let name = name.into();
        let name = name.trim();
        let path = normalize_current_directory_path(path)?;
        if name.is_empty() || name.chars().any(char::is_control) {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            path,
            kind,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn kind(&self) -> CurrentDirectoryEntryKind {
        self.kind
    }
}

/// Build a shell-safe path argument for inserting a cwd-aware file path.
pub fn current_directory_shell_path_argument(path: &str) -> Option<String> {
    let path = normalize_current_directory_path(path)?;
    Some(shell_cd_target(&path))
}

/// Build the visible shell command that changes the active terminal directory.
pub fn current_directory_cd_command(path: &str) -> Option<String> {
    Some(format!(
        "cd {}",
        current_directory_shell_path_argument(path)?
    ))
}

/// Build the active-shell command that reports cwd through OSC 7.
pub fn current_directory_report_command() -> &'static str {
    "___oxide_cwd=${PWD:-$(pwd)}; ___oxide_host=${HOSTNAME:-$(hostname 2>/dev/null || printf localhost)}; ___oxide_path=$(printf '%s' \"$___oxide_cwd\" | awk 'BEGIN{for(i=0;i<256;i++)ord[sprintf(\"%c\",i)]=i}{for(i=1;i<=length($0);i++){c=substr($0,i,1);if(c~/[A-Za-z0-9._~\\/:@-]/)printf \"%s\",c;else printf \"%%%02X\",ord[c]}}'); printf '\\033]7;file://%s%s\\007' \"$___oxide_host\" \"$___oxide_path\"; unset ___oxide_cwd ___oxide_host ___oxide_path"
}

/// Build a conservative runtime hook that reports cwd via OSC 7 on prompts.
pub fn current_directory_shell_integration_command() -> &'static str {
    "if [ -n \"${BASH_VERSION-}${ZSH_VERSION-}\" ]; then __oxideterm_osc7(){ local __oxide_cwd __oxide_host __oxide_path; __oxide_cwd=${PWD:-$(pwd)}; __oxide_host=${HOSTNAME:-$(hostname 2>/dev/null || printf localhost)}; __oxide_path=$(printf '%s' \"$__oxide_cwd\" | awk 'BEGIN{for(i=0;i<256;i++)ord[sprintf(\"%c\",i)]=i}{for(i=1;i<=length($0);i++){c=substr($0,i,1);if(c~/[A-Za-z0-9._~\\/:@-]/)printf \"%s\",c;else printf \"%%%02X\",ord[c]}}'); printf '\\033]7;file://%s%s\\007' \"$__oxide_host\" \"$__oxide_path\"; }; if [ -n \"${ZSH_VERSION-}\" ]; then autoload -Uz add-zsh-hook 2>/dev/null; if typeset -f add-zsh-hook >/dev/null 2>&1; then case \" ${precmd_functions[*]-} \" in *\" __oxideterm_osc7 \"*) ;; *) add-zsh-hook precmd __oxideterm_osc7 ;; esac; fi; elif [ -n \"${BASH_VERSION-}\" ]; then case \";${PROMPT_COMMAND-};\" in *\";__oxideterm_osc7;\"*|\"__oxideterm_osc7;\"*) ;; *) PROMPT_COMMAND=\"__oxideterm_osc7${PROMPT_COMMAND:+; $PROMPT_COMMAND}\" ;; esac; fi; __oxideterm_osc7; fi"
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
        assert_eq!(
            current_directory_shell_path_argument("/Users/dominical/it's ok").as_deref(),
            Some("'/Users/dominical/it'\\''s ok'")
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
        assert!(command.contains("]7;file://%s%s"));
        assert!(command.contains("${PWD:-$(pwd)}"));
        assert!(command.contains("%%%02X"));
    }

    #[test]
    fn shell_integration_command_installs_bash_and_zsh_hooks() {
        let command = current_directory_shell_integration_command();
        assert!(command.contains("]7;file://%s%s"));
        assert!(command.contains("PROMPT_COMMAND"));
        assert!(
            command.contains(
                "PROMPT_COMMAND=\"__oxideterm_osc7${PROMPT_COMMAND:+; $PROMPT_COMMAND}\""
            )
        );
        assert!(command.contains("add-zsh-hook precmd"));
        assert!(command.contains("precmd_functions[*]"));
        assert!(command.contains("__oxideterm_osc7"));
        assert!(!command.contains("PS1="));
        assert!(!command.contains("title"));
    }

    #[test]
    fn entry_rejects_control_names() {
        assert!(CurrentDirectoryEntry::new("ok", "/tmp/ok").is_some());
        assert!(CurrentDirectoryEntry::new("bad\nname", "/tmp/bad").is_none());
    }

    #[test]
    fn entry_tracks_file_kind() {
        let entry =
            CurrentDirectoryEntry::new_file("Cargo.toml", "/tmp/Cargo.toml").expect("file entry");
        assert_eq!(entry.kind(), CurrentDirectoryEntryKind::File);
    }
}
