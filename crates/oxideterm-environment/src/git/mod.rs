// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Git environment awareness primitives.
//!
//! This module owns Git-specific DTOs, shell probe descriptions, parsers, and
//! cache state. Non-Git environment detectors should live in their own sibling
//! modules instead of adding more Git-shaped files to the crate root.

pub mod model;
pub mod parse;
pub mod probe;
pub mod store;

pub use model::{
    GitBranchIdentity, GitBranchListOutcome, GitBranchReference, GitChangedPath, GitOperationKind,
    GitProbeError, GitProbeKey, GitProbeOutcome, GitProbeScope, GitRepositorySnapshot,
    GitRepositoryStatus, GitStagedDiffContext, GitStagedDiffOutcome,
};
pub use parse::{
    GitCommandOutput, interpret_git_branch_list_output, interpret_git_branch_list_outputs,
    interpret_git_command_outputs, interpret_git_command_outputs_with_status,
    interpret_git_command_outputs_with_status_and_operation, interpret_git_staged_diff_outputs,
    parse_git_operation_kind, parse_git_status_summary, parse_shell_branch_list_output,
    parse_shell_probe_output, parse_shell_staged_diff_output,
};
pub use probe::{
    GitProbeCommandArgs, git_absolute_git_dir_args, git_branch_args, git_branch_list_args,
    git_head_args, git_repo_root_args, git_staged_diff_patch_args, git_staged_diff_stat_args,
    git_status_args, git_worktree_list_args, remote_shell_branch_list_command,
    remote_shell_probe_command, remote_shell_staged_diff_command, shell_quote,
};
pub use store::{GitProbeEntry, GitProbeState, GitStatusStore};
