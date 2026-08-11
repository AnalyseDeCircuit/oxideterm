// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Git action validation and shell-visible command planning.

use crate::shell::shell_quote;

use super::model::{GitBranchReference, GitOperationKind};

/// Repository-wide Git action that can be sent to an interactive terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitRepositoryAction {
    Fetch,
    Pull,
    Push,
    Publish,
    Status,
    Diff,
    DiffStaged,
    Log,
    Stash,
    StashList,
    StashPop,
    StageAll,
    UnstageAll,
    Commit,
    CommitVerbose,
    CommitSignoff,
    Amend,
    AmendNoEdit,
    RebasePull,
    RebaseInteractive,
    FetchAll,
    PushTags,
    LogStat,
    Reflog,
    BranchVerbose,
    RemoteList,
    TagList,
    WorktreeList,
    StashShowLatest,
    StashApplyLatest,
    StashDropLatest,
    ConflictFiles,
    Continue(GitOperationKind),
    Abort(GitOperationKind),
    Skip(GitOperationKind),
}

/// Git action scoped to one worktree path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitPathAction {
    Stage,
    Unstage,
    Diff,
    DiffStaged,
    Open,
    Ours,
    Theirs,
}

/// Shell-visible Git command and any trusted CWD transition it causes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitActionPlan {
    command: String,
    cwd_after_command: Option<String>,
}

impl GitActionPlan {
    /// Builds a commit plan from one user-visible subject line.
    pub fn commit_message(message: &str) -> Option<Self> {
        let message = message.trim();
        git_action_arg_is_valid(message).then(|| Self {
            command: format!("git commit -m {}", shell_quote(message)),
            cwd_after_command: None,
        })
    }

    /// Selects a branch, or changes into its linked worktree when present.
    pub fn select_branch(branch: &GitBranchReference) -> Option<Self> {
        let branch_name = branch.name().trim();
        if !git_action_arg_is_valid(branch_name) {
            return None;
        }

        let (command, cwd_after_command) = if let Some(worktree_path) = branch.worktree_path() {
            (
                format!("cd {}", shell_quote(worktree_path)),
                Some(worktree_path.to_string()),
            )
        } else {
            (checkout_command(branch_name), None)
        };
        Some(Self {
            command,
            cwd_after_command,
        })
    }

    /// Builds a checkout plan for a branch not represented by a branch row.
    pub fn checkout_name(branch_name: &str) -> Option<Self> {
        branch_plan(branch_name, checkout_command)
    }

    /// Builds a rebase plan for a validated target branch.
    pub fn rebase_onto_name(branch_name: &str) -> Option<Self> {
        branch_plan(branch_name, |branch| {
            format!("git rebase {}", shell_quote(branch))
        })
    }

    /// Builds a branch creation plan for a validated branch name.
    pub fn create_branch_name(branch_name: &str) -> Option<Self> {
        branch_plan(branch_name, |branch| {
            format!("git checkout -b {}", shell_quote(branch))
        })
    }

    /// Builds a rename plan for the current branch.
    pub fn rename_current_branch(branch_name: &str) -> Option<Self> {
        branch_plan(branch_name, |branch| {
            format!("git branch -m {}", shell_quote(branch))
        })
    }

    /// Builds a plan that tracks the selected remote branch.
    pub fn track_remote_branch(branch_name: &str) -> Option<Self> {
        branch_plan(branch_name, |branch| {
            format!("git switch --track {}", shell_quote(branch))
        })
    }

    /// Builds the fixed command for a repository-wide action.
    pub fn repository_action(action: GitRepositoryAction) -> Self {
        Self {
            command: repository_action_command(action).to_string(),
            cwd_after_command: None,
        }
    }

    /// Builds a path action while preserving Git's `--` option boundary.
    pub fn path_action(action: GitPathAction, path: &str) -> Option<Self> {
        git_action_arg_is_valid(path).then(|| Self {
            command: path_action_command(action, path),
            cwd_after_command: None,
        })
    }

    /// Returns the command that should remain visible in the active terminal.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Returns the trusted CWD transition caused by a worktree selection.
    pub fn cwd_after_command(&self) -> Option<&str> {
        self.cwd_after_command.as_deref()
    }
}

/// Rejects empty values and control characters before one shell argument is built.
pub fn git_action_arg_is_valid(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

fn branch_plan(branch_name: &str, command: impl FnOnce(&str) -> String) -> Option<GitActionPlan> {
    let branch_name = branch_name.trim();
    git_action_arg_is_valid(branch_name).then(|| GitActionPlan {
        command: command(branch_name),
        cwd_after_command: None,
    })
}

fn checkout_command(branch: &str) -> String {
    format!("git checkout {}", shell_quote(branch))
}

fn path_action_command(action: GitPathAction, path: &str) -> String {
    let path = shell_quote(path);
    match action {
        GitPathAction::Stage => format!("git add -- {path}"),
        GitPathAction::Unstage => format!("git restore --staged -- {path}"),
        GitPathAction::Diff => format!("git diff -- {path}"),
        GitPathAction::DiffStaged => format!("git diff --cached -- {path}"),
        GitPathAction::Open => format!("${{EDITOR:-vi}} -- {path}"),
        GitPathAction::Ours => format!("git checkout --ours -- {path}"),
        GitPathAction::Theirs => format!("git checkout --theirs -- {path}"),
    }
}

fn repository_action_command(action: GitRepositoryAction) -> &'static str {
    match action {
        GitRepositoryAction::Fetch => "git fetch --prune",
        GitRepositoryAction::Pull => "git pull --ff-only",
        GitRepositoryAction::Push => "git push",
        GitRepositoryAction::Publish => "git push -u origin HEAD",
        GitRepositoryAction::Status => "git status --short --branch",
        GitRepositoryAction::Diff => "git diff --stat",
        GitRepositoryAction::DiffStaged => "git diff --cached --stat",
        GitRepositoryAction::Log => "git log --oneline --decorate --graph -20",
        GitRepositoryAction::Stash => "git stash push",
        GitRepositoryAction::StashList => "git stash list",
        GitRepositoryAction::StashPop => "git stash pop",
        GitRepositoryAction::StageAll => "git add -A",
        GitRepositoryAction::UnstageAll => "git restore --staged .",
        GitRepositoryAction::Commit => "git commit",
        GitRepositoryAction::CommitVerbose => "git commit -v",
        GitRepositoryAction::CommitSignoff => "git commit -s",
        GitRepositoryAction::Amend => "git commit --amend",
        GitRepositoryAction::AmendNoEdit => "git commit --amend --no-edit",
        GitRepositoryAction::RebasePull => "git pull --rebase",
        GitRepositoryAction::RebaseInteractive => "git rebase -i @{upstream}",
        GitRepositoryAction::FetchAll => "git fetch --all --prune",
        GitRepositoryAction::PushTags => "git push --tags",
        GitRepositoryAction::LogStat => "git log --stat -20",
        GitRepositoryAction::Reflog => "git reflog -20",
        GitRepositoryAction::BranchVerbose => "git branch -vv",
        GitRepositoryAction::RemoteList => "git remote -v",
        GitRepositoryAction::TagList => "git tag --list",
        GitRepositoryAction::WorktreeList => "git worktree list",
        GitRepositoryAction::StashShowLatest => "git stash show -p stash@{0}",
        GitRepositoryAction::StashApplyLatest => "git stash apply stash@{0}",
        GitRepositoryAction::StashDropLatest => "git stash drop stash@{0}",
        GitRepositoryAction::ConflictFiles => "git diff --name-only --diff-filter=U",
        GitRepositoryAction::Continue(operation) => operation_command(operation, "continue"),
        GitRepositoryAction::Abort(operation) => operation_command(operation, "abort"),
        GitRepositoryAction::Skip(operation) => operation_command(operation, "skip"),
    }
}

fn operation_command(operation: GitOperationKind, verb: &str) -> &'static str {
    match (operation, verb) {
        (GitOperationKind::Merge, "continue") => "git merge --continue",
        (GitOperationKind::Merge, "abort") => "git merge --abort",
        (GitOperationKind::Rebase, "continue") => "git rebase --continue",
        (GitOperationKind::Rebase, "abort") => "git rebase --abort",
        (GitOperationKind::Rebase, "skip") => "git rebase --skip",
        (GitOperationKind::CherryPick, "continue") => "git cherry-pick --continue",
        (GitOperationKind::CherryPick, "abort") => "git cherry-pick --abort",
        (GitOperationKind::CherryPick, "skip") => "git cherry-pick --skip",
        (GitOperationKind::Revert, "continue") => "git revert --continue",
        (GitOperationKind::Revert, "abort") => "git revert --abort",
        (GitOperationKind::Revert, "skip") => "git revert --skip",
        // Merge has no skip verb; preserve the existing harmless fallback.
        (GitOperationKind::Merge, "skip") | (_, _) => "git status --short --branch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_plans_quote_names_and_track_worktree_cwd() {
        let branch =
            GitBranchReference::with_worktree_path("main", false, Some("/tmp/Oxide Term")).unwrap();
        let worktree_plan = GitActionPlan::select_branch(&branch).unwrap();
        assert_eq!(worktree_plan.command(), "cd '/tmp/Oxide Term'");
        assert_eq!(worktree_plan.cwd_after_command(), Some("/tmp/Oxide Term"));

        let branch = GitBranchReference::new("feature/it's-ok", false).unwrap();
        let checkout_plan = GitActionPlan::select_branch(&branch).unwrap();
        assert_eq!(
            checkout_plan.command(),
            "git checkout 'feature/it'\\''s-ok'"
        );
        assert_eq!(checkout_plan.cwd_after_command(), None);
    }

    #[test]
    fn branch_name_plans_validate_and_quote_one_argument() {
        assert_eq!(
            GitActionPlan::checkout_name("origin/feature/it's-ok")
                .unwrap()
                .command(),
            "git checkout 'origin/feature/it'\\''s-ok'"
        );
        assert_eq!(
            GitActionPlan::rebase_onto_name("main branch")
                .unwrap()
                .command(),
            "git rebase 'main branch'"
        );
        assert_eq!(
            GitActionPlan::create_branch_name("feature/it works")
                .unwrap()
                .command(),
            "git checkout -b 'feature/it works'"
        );
        assert_eq!(
            GitActionPlan::rename_current_branch("feature/new")
                .unwrap()
                .command(),
            "git branch -m 'feature/new'"
        );
        assert_eq!(
            GitActionPlan::track_remote_branch("origin/feature/new")
                .unwrap()
                .command(),
            "git switch --track 'origin/feature/new'"
        );
        assert!(GitActionPlan::checkout_name("feature\nbad").is_none());
        assert!(GitActionPlan::create_branch_name("feature\nbad").is_none());
    }

    #[test]
    fn commit_message_plan_requires_one_safe_subject() {
        assert_eq!(
            GitActionPlan::commit_message("fix: preserve user's draft")
                .unwrap()
                .command(),
            "git commit -m 'fix: preserve user'\\''s draft'"
        );
        assert!(GitActionPlan::commit_message("   ").is_none());
        assert!(GitActionPlan::commit_message("fix: first\nsecond").is_none());
    }

    #[test]
    fn path_plans_preserve_editor_and_option_boundaries() {
        assert_eq!(
            GitActionPlan::path_action(GitPathAction::Stage, "src/it works.rs")
                .unwrap()
                .command(),
            "git add -- 'src/it works.rs'"
        );
        assert_eq!(
            GitActionPlan::path_action(GitPathAction::DiffStaged, "a'b.rs")
                .unwrap()
                .command(),
            "git diff --cached -- 'a'\\''b.rs'"
        );
        assert_eq!(
            GitActionPlan::path_action(GitPathAction::Open, "notes.txt")
                .unwrap()
                .command(),
            "${EDITOR:-vi} -- 'notes.txt'"
        );
        assert!(GitActionPlan::path_action(GitPathAction::Open, "bad\npath").is_none());
    }

    #[test]
    fn repository_actions_keep_current_visible_commands() {
        let cases = [
            (GitRepositoryAction::Fetch, "git fetch --prune"),
            (GitRepositoryAction::FetchAll, "git fetch --all --prune"),
            (GitRepositoryAction::Pull, "git pull --ff-only"),
            (GitRepositoryAction::Push, "git push"),
            (GitRepositoryAction::Publish, "git push -u origin HEAD"),
            (GitRepositoryAction::Status, "git status --short --branch"),
            (GitRepositoryAction::Diff, "git diff --stat"),
            (GitRepositoryAction::DiffStaged, "git diff --cached --stat"),
            (
                GitRepositoryAction::Log,
                "git log --oneline --decorate --graph -20",
            ),
            (GitRepositoryAction::Stash, "git stash push"),
            (GitRepositoryAction::StashList, "git stash list"),
            (GitRepositoryAction::StashPop, "git stash pop"),
            (GitRepositoryAction::StageAll, "git add -A"),
            (GitRepositoryAction::UnstageAll, "git restore --staged ."),
            (GitRepositoryAction::Commit, "git commit"),
            (GitRepositoryAction::CommitVerbose, "git commit -v"),
            (GitRepositoryAction::CommitSignoff, "git commit -s"),
            (GitRepositoryAction::Amend, "git commit --amend"),
            (
                GitRepositoryAction::AmendNoEdit,
                "git commit --amend --no-edit",
            ),
            (GitRepositoryAction::RebasePull, "git pull --rebase"),
            (
                GitRepositoryAction::RebaseInteractive,
                "git rebase -i @{upstream}",
            ),
            (GitRepositoryAction::PushTags, "git push --tags"),
            (GitRepositoryAction::LogStat, "git log --stat -20"),
            (GitRepositoryAction::Reflog, "git reflog -20"),
            (GitRepositoryAction::BranchVerbose, "git branch -vv"),
            (GitRepositoryAction::RemoteList, "git remote -v"),
            (GitRepositoryAction::TagList, "git tag --list"),
            (GitRepositoryAction::WorktreeList, "git worktree list"),
            (
                GitRepositoryAction::StashShowLatest,
                "git stash show -p stash@{0}",
            ),
            (
                GitRepositoryAction::StashApplyLatest,
                "git stash apply stash@{0}",
            ),
            (
                GitRepositoryAction::StashDropLatest,
                "git stash drop stash@{0}",
            ),
            (
                GitRepositoryAction::ConflictFiles,
                "git diff --name-only --diff-filter=U",
            ),
            (
                GitRepositoryAction::Continue(GitOperationKind::Rebase),
                "git rebase --continue",
            ),
            (
                GitRepositoryAction::Abort(GitOperationKind::CherryPick),
                "git cherry-pick --abort",
            ),
            (
                GitRepositoryAction::Skip(GitOperationKind::Revert),
                "git revert --skip",
            ),
            (
                GitRepositoryAction::Skip(GitOperationKind::Merge),
                "git status --short --branch",
            ),
        ];

        for (action, expected) in cases {
            assert_eq!(GitActionPlan::repository_action(action).command(), expected);
        }
    }
}
