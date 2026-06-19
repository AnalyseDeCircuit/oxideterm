// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;
use std::hash::{Hash, Hasher};

/// Stable ownership scope for a Git probe.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum GitProbeScope {
    Local,
    SshNode(String),
}

impl GitProbeScope {
    pub fn ssh_node(node_id: impl Into<String>) -> Self {
        Self::SshNode(node_id.into())
    }
}

/// Cache key for a terminal Git probe.
#[derive(Clone, Debug, Eq)]
pub struct GitProbeKey {
    scope: GitProbeScope,
    cwd: String,
}

impl PartialEq for GitProbeKey {
    fn eq(&self, other: &Self) -> bool {
        self.scope == other.scope && self.cwd == other.cwd
    }
}

impl Hash for GitProbeKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.scope.hash(state);
        self.cwd.hash(state);
    }
}

impl GitProbeKey {
    pub fn new(scope: GitProbeScope, cwd: impl Into<String>) -> Option<Self> {
        let cwd = cwd.into();
        let cwd = cwd.trim();
        if cwd.is_empty() {
            return None;
        }
        Some(Self {
            scope,
            cwd: cwd.to_string(),
        })
    }

    pub fn scope(&self) -> &GitProbeScope {
        &self.scope
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.scope, GitProbeScope::SshNode(_))
    }
}

/// Human-facing branch identity for a repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitBranchIdentity {
    Branch(String),
    Detached(String),
}

impl GitBranchIdentity {
    pub fn display_text(&self) -> &str {
        match self {
            Self::Branch(branch) | Self::Detached(branch) => branch,
        }
    }

    pub fn is_detached(&self) -> bool {
        matches!(self, Self::Detached(_))
    }
}

/// Mutating Git operation currently waiting for user resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitOperationKind {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl GitOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry_pick",
            Self::Revert => "revert",
        }
    }
}

/// Snapshot of Git metadata that is safe to render in terminal chrome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitRepositorySnapshot {
    pub repo_root: String,
    pub branch: GitBranchIdentity,
    pub status: GitRepositoryStatus,
}

impl GitRepositorySnapshot {
    pub fn new(repo_root: impl Into<String>, branch: GitBranchIdentity) -> Option<Self> {
        Self::with_status(repo_root, branch, GitRepositoryStatus::default())
    }

    pub fn with_status(
        repo_root: impl Into<String>,
        branch: GitBranchIdentity,
        status: GitRepositoryStatus,
    ) -> Option<Self> {
        let repo_root = repo_root.into();
        let repo_root = repo_root.trim();
        if repo_root.is_empty() || branch.display_text().trim().is_empty() {
            return None;
        }
        Some(Self {
            repo_root: repo_root.to_string(),
            branch,
            status,
        })
    }
}

/// Parsed `git status --porcelain=v2 --branch` summary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitRepositoryStatus {
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    staged: u32,
    modified: u32,
    untracked: u32,
    conflicts: u32,
    operation: Option<GitOperationKind>,
}

impl GitRepositoryStatus {
    pub fn new(
        upstream: Option<String>,
        ahead: u32,
        behind: u32,
        staged: u32,
        modified: u32,
        untracked: u32,
        conflicts: u32,
    ) -> Self {
        Self {
            upstream: upstream
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            ahead,
            behind,
            staged,
            modified,
            untracked,
            conflicts,
            operation: None,
        }
    }

    pub fn with_operation(mut self, operation: Option<GitOperationKind>) -> Self {
        self.operation = operation;
        self
    }

    pub fn upstream(&self) -> Option<&str> {
        self.upstream.as_deref()
    }

    pub fn ahead(&self) -> u32 {
        self.ahead
    }

    pub fn behind(&self) -> u32 {
        self.behind
    }

    pub fn staged(&self) -> u32 {
        self.staged
    }

    pub fn modified(&self) -> u32 {
        self.modified
    }

    pub fn untracked(&self) -> u32 {
        self.untracked
    }

    pub fn conflicts(&self) -> u32 {
        self.conflicts
    }

    pub fn operation(&self) -> Option<GitOperationKind> {
        self.operation
    }

    pub fn dirty_count(&self) -> u32 {
        self.staged
            .saturating_add(self.modified)
            .saturating_add(self.untracked)
            .saturating_add(self.conflicts)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_count() > 0
    }

    pub fn has_conflicts(&self) -> bool {
        self.conflicts > 0
    }
}

/// Branch entry that can be offered as a checkout or worktree target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitBranchReference {
    name: String,
    current: bool,
    worktree_path: Option<String>,
}

impl GitBranchReference {
    pub fn new(name: impl Into<String>, current: bool) -> Option<Self> {
        Self::with_worktree_path(name, current, None::<String>)
    }

    pub fn with_worktree_path(
        name: impl Into<String>,
        current: bool,
        worktree_path: Option<impl Into<String>>,
    ) -> Option<Self> {
        let name = name.into();
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let worktree_path = worktree_path
            .map(Into::into)
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty());
        Some(Self {
            name: name.to_string(),
            current,
            worktree_path,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn current(&self) -> bool {
        self.current
    }

    pub fn worktree_path(&self) -> Option<&str> {
        self.worktree_path.as_deref()
    }
}

/// Staged Git diff context that is safe to hand to higher-level consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitStagedDiffContext {
    stat: String,
    patch: String,
}

impl GitStagedDiffContext {
    pub fn new(stat: impl Into<String>, patch: impl Into<String>) -> Option<Self> {
        let stat = stat.into();
        let patch = patch.into();
        if stat.trim().is_empty() && patch.trim().is_empty() {
            return None;
        }
        Some(Self { stat, patch })
    }

    pub fn stat(&self) -> &str {
        &self.stat
    }

    pub fn patch(&self) -> &str {
        &self.patch
    }
}

/// Non-secret error surface for probe failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitProbeError {
    message: String,
}

impl GitProbeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for GitProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Result of one Git probe execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitProbeOutcome {
    Ready(GitRepositorySnapshot),
    NotRepository,
    GitUnavailable,
    CwdUnavailable,
    Error(GitProbeError),
}

/// Result of listing local Git branches in a repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitBranchListOutcome {
    Ready(Vec<GitBranchReference>),
    NotRepository,
    GitUnavailable,
    CwdUnavailable,
    Error(GitProbeError),
}

/// Result of reading staged diff content for AI-assisted commit messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitStagedDiffOutcome {
    Ready(GitStagedDiffContext),
    Empty,
    NotRepository,
    GitUnavailable,
    CwdUnavailable,
    Error(GitProbeError),
}
