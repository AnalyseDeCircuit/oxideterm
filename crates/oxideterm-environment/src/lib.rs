// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Terminal environment awareness primitives shared by local and SSH probes.
//!
//! This crate owns reusable environment detectors such as Git repository
//! probing. It deliberately avoids GPUI, terminal panes, SSH handles, and
//! process spawning so UI/backend owners can provide their own execution path.

pub mod model;
pub mod parse;
pub mod probe;
pub mod store;
pub mod terminal_context;

pub use model::{
    GitBranchIdentity, GitBranchListOutcome, GitBranchReference, GitCheckoutOutcome, GitProbeError,
    GitProbeKey, GitProbeOutcome, GitProbeScope, GitRepositorySnapshot,
};
pub use parse::{
    GitCommandOutput, interpret_git_branch_list_output, interpret_git_branch_list_outputs,
    interpret_git_checkout_status, interpret_git_command_outputs, parse_shell_branch_list_output,
    parse_shell_checkout_output, parse_shell_probe_output,
};
pub use probe::{
    GitProbeCommandArgs, git_branch_args, git_branch_list_args, git_head_args, git_repo_root_args,
    git_worktree_list_args, remote_shell_branch_list_command, remote_shell_checkout_command,
    remote_shell_probe_command, shell_quote,
};
pub use store::{GitProbeEntry, GitProbeState, GitStatusStore};
pub use terminal_context::infer_terminal_cwd_from_text;
