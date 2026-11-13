// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Trusted Git CWD selection and local Git control-directory inspection.

use std::path::{Path, PathBuf};

use crate::cwd::{CurrentDirectoryScope, CurrentDirectorySnapshot, CurrentDirectorySource};

use super::model::{GitOperationKind, GitProbeScope};

/// Selects a scoped CWD snapshot that is trusted for Git probing.
pub fn git_cwd_from_directory_snapshot(
    scope: &GitProbeScope,
    snapshot: &CurrentDirectorySnapshot,
) -> Option<String> {
    match (scope, snapshot.scope()) {
        (GitProbeScope::Local, CurrentDirectoryScope::Local) => {
            Some(expand_local_git_home(snapshot.path()))
        }
        (GitProbeScope::SshNode(expected), CurrentDirectoryScope::SshNode(actual))
            if expected == actual && remote_git_cwd_source_is_trusted(snapshot.source()) =>
        {
            Some(snapshot.path().to_string())
        }
        _ => None,
    }
}

/// Chooses the authoritative CWD while rejecting visible-text fallback for SSH.
pub fn preferred_git_cwd(
    scope: &GitProbeScope,
    snapshot_cwd: Option<String>,
    visible_cwd: Option<String>,
) -> Option<String> {
    if matches!(scope, GitProbeScope::SshNode(_)) {
        return snapshot_cwd;
    }
    snapshot_cwd.or(visible_cwd)
}

/// Expands local `~` and `~/...` paths before passing them to `git -C`.
pub fn expand_local_git_home(cwd: &str) -> String {
    expand_local_git_home_with(cwd, local_home().as_deref())
}

/// Detects the active operation from Git's control files and directories.
pub fn git_operation_kind_from_git_dir(git_dir: &Path) -> Option<GitOperationKind> {
    if git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir() {
        Some(GitOperationKind::Rebase)
    } else if git_dir.join("MERGE_HEAD").is_file() {
        Some(GitOperationKind::Merge)
    } else if git_dir.join("CHERRY_PICK_HEAD").is_file() {
        Some(GitOperationKind::CherryPick)
    } else if git_dir.join("REVERT_HEAD").is_file() {
        Some(GitOperationKind::Revert)
    } else {
        None
    }
}

fn remote_git_cwd_source_is_trusted(source: CurrentDirectorySource) -> bool {
    matches!(
        source,
        CurrentDirectorySource::ShellIntegration | CurrentDirectorySource::UserAction
    )
}

fn local_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn expand_local_git_home_with(cwd: &str, home: Option<&Path>) -> String {
    if cwd == "~" {
        return home
            .map(|home| home.to_string_lossy().to_string())
            .unwrap_or_else(|| cwd.to_string());
    }
    if let Some(rest) = cwd.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    cwd.to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn snapshot(
        scope: CurrentDirectoryScope,
        path: &str,
        source: CurrentDirectorySource,
    ) -> CurrentDirectorySnapshot {
        CurrentDirectorySnapshot::new(scope, path, source).unwrap()
    }

    #[test]
    fn remote_git_cwd_requires_matching_trusted_snapshot() {
        let scope = GitProbeScope::ssh_node("node-1");
        let trusted = snapshot(
            CurrentDirectoryScope::ssh_node("node-1"),
            "/home/dev/project",
            CurrentDirectorySource::ShellIntegration,
        );
        assert_eq!(
            git_cwd_from_directory_snapshot(&scope, &trusted).as_deref(),
            Some("/home/dev/project")
        );

        let user_action = snapshot(
            CurrentDirectoryScope::ssh_node("node-1"),
            "/home/dev/selected",
            CurrentDirectorySource::UserAction,
        );
        assert_eq!(
            git_cwd_from_directory_snapshot(&scope, &user_action).as_deref(),
            Some("/home/dev/selected")
        );

        let other_node = snapshot(
            CurrentDirectoryScope::ssh_node("node-2"),
            "/home/dev/project",
            CurrentDirectorySource::UserAction,
        );
        assert!(git_cwd_from_directory_snapshot(&scope, &other_node).is_none());

        for source in [
            CurrentDirectorySource::SessionDefault,
            CurrentDirectorySource::VisibleText,
            CurrentDirectorySource::ProcessFallback,
        ] {
            let untrusted = snapshot(
                CurrentDirectoryScope::ssh_node("node-1"),
                "/home/dev/project",
                source,
            );
            assert!(git_cwd_from_directory_snapshot(&scope, &untrusted).is_none());
        }
    }

    #[test]
    fn preferred_cwd_allows_visible_text_only_for_local_git() {
        assert_eq!(
            preferred_git_cwd(
                &GitProbeScope::Local,
                None,
                Some("/home/dev/project".to_string())
            )
            .as_deref(),
            Some("/home/dev/project")
        );
        assert_eq!(
            preferred_git_cwd(
                &GitProbeScope::ssh_node("node-1"),
                None,
                Some("/wrong/from-visible-text".to_string())
            ),
            None
        );
    }

    #[test]
    fn local_home_expansion_preserves_current_rules() {
        #[cfg(windows)]
        let (home, project) = (r"C:\Users\dev", r"C:\Users\dev\project");
        #[cfg(not(windows))]
        let (home, project) = ("/home/dev", "/home/dev/project");
        assert_eq!(expand_local_git_home_with("~", Some(Path::new(home))), home);
        assert_eq!(
            expand_local_git_home_with("~/project", Some(Path::new(home))),
            project
        );
        assert_eq!(
            expand_local_git_home_with("~other/project", Some(Path::new(home))),
            "~other/project"
        );
        assert_eq!(expand_local_git_home_with("~", None), "~");
        assert_eq!(expand_local_git_home_with("~/project", None), "~/project");
    }

    #[test]
    fn git_dir_operation_detection_preserves_priority_and_kinds() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "oxideterm-environment-git-operation-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();

        fs::write(root.join("MERGE_HEAD"), "merge").unwrap();
        assert_eq!(
            git_operation_kind_from_git_dir(&root),
            Some(GitOperationKind::Merge)
        );
        fs::write(root.join("CHERRY_PICK_HEAD"), "cherry-pick").unwrap();
        assert_eq!(
            git_operation_kind_from_git_dir(&root),
            Some(GitOperationKind::Merge)
        );
        fs::remove_file(root.join("MERGE_HEAD")).unwrap();
        assert_eq!(
            git_operation_kind_from_git_dir(&root),
            Some(GitOperationKind::CherryPick)
        );
        fs::remove_file(root.join("CHERRY_PICK_HEAD")).unwrap();
        fs::write(root.join("REVERT_HEAD"), "revert").unwrap();
        assert_eq!(
            git_operation_kind_from_git_dir(&root),
            Some(GitOperationKind::Revert)
        );
        fs::create_dir(root.join("rebase-apply")).unwrap();
        assert_eq!(
            git_operation_kind_from_git_dir(&root),
            Some(GitOperationKind::Rebase)
        );

        fs::remove_dir_all(root).unwrap();
    }
}
