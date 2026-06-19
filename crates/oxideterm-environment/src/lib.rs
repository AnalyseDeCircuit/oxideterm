// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Terminal environment awareness primitives shared by local and SSH probes.
//!
//! This crate owns reusable environment detectors such as Git repository
//! probing. It deliberately avoids GPUI, terminal panes, SSH handles, and
//! process spawning so UI/backend owners can provide their own execution path.

pub mod git;
pub mod terminal_context;

pub use git::{
    GitBranchIdentity, GitBranchListOutcome, GitBranchReference, GitCommandOutput,
    GitOperationKind, GitProbeCommandArgs, GitProbeEntry, GitProbeError, GitProbeKey,
    GitProbeOutcome, GitProbeScope, GitProbeState, GitRepositorySnapshot, GitRepositoryStatus,
    GitStagedDiffContext, GitStagedDiffOutcome, GitStatusStore, git_absolute_git_dir_args,
    git_branch_args, git_branch_list_args, git_head_args, git_repo_root_args,
    git_staged_diff_patch_args, git_staged_diff_stat_args, git_status_args, git_worktree_list_args,
    interpret_git_branch_list_output, interpret_git_branch_list_outputs,
    interpret_git_command_outputs, interpret_git_command_outputs_with_status,
    interpret_git_command_outputs_with_status_and_operation, interpret_git_staged_diff_outputs,
    parse_git_operation_kind, parse_git_status_summary, parse_shell_branch_list_output,
    parse_shell_probe_output, parse_shell_staged_diff_output, remote_shell_branch_list_command,
    remote_shell_probe_command, remote_shell_staged_diff_command, shell_quote,
};
pub use terminal_context::infer_terminal_cwd_from_text;
