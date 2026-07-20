// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Terminal environment awareness primitives shared by local and SSH probes.
//!
//! This crate owns reusable environment detectors such as Git repository
//! probing. It deliberately avoids GPUI, terminal panes, SSH handles, and
//! process spawning so UI/backend owners can provide their own execution path.

pub mod cwd;
pub mod git;
pub mod project;
mod shell;
pub mod terminal_context;

pub use cwd::{
    CurrentDirectoryEntry, CurrentDirectoryEntryKind, CurrentDirectoryKey, CurrentDirectoryScope,
    CurrentDirectorySnapshot, CurrentDirectorySource, current_directory_cd_command,
    current_directory_parent, current_directory_path_is_explicit, current_directory_report_command,
    current_directory_shell_integration_command, current_directory_shell_path_argument,
    list_local_current_directory, sort_current_directory_entries,
};
pub use git::{
    GitActionPlan, GitBranchIdentity, GitBranchListOutcome, GitBranchReference, GitChangedPath,
    GitCommandOutput, GitOperationKind, GitPathAction, GitProbeCommandArgs, GitProbeEntry,
    GitProbeError, GitProbeKey, GitProbeOutcome, GitProbeScope, GitProbeState, GitRepositoryAction,
    GitRepositorySnapshot, GitRepositoryStatus, GitStagedDiffContext, GitStagedDiffOutcome,
    GitStatusStore, expand_local_git_home, git_absolute_git_dir_args, git_action_arg_is_valid,
    git_branch_args, git_branch_list_args, git_cwd_from_directory_snapshot, git_head_args,
    git_operation_kind_from_git_dir, git_repo_root_args, git_staged_diff_patch_args,
    git_staged_diff_stat_args, git_status_args, git_worktree_list_args,
    interpret_git_branch_list_output, interpret_git_branch_list_outputs,
    interpret_git_command_outputs, interpret_git_command_outputs_with_status,
    interpret_git_command_outputs_with_status_and_operation, interpret_git_staged_diff_outputs,
    parse_git_operation_kind, parse_git_status_summary, parse_shell_branch_list_output,
    parse_shell_probe_output, parse_shell_staged_diff_output, preferred_git_cwd,
    remote_shell_branch_list_command, remote_shell_probe_command, remote_shell_staged_diff_command,
    shell_quote,
};
pub use project::{
    PROJECT_PROBE_MAX_ANCESTORS, PROJECT_PROBE_MAX_FILE_BYTES, PROJECT_SHELL_PROBE_SENTINEL,
    ProjectFacet, ProjectFacetKind, ProjectManifestEntry, ProjectProbeEntry, ProjectProbeError,
    ProjectProbeKey, ProjectProbeOutcome, ProjectProbeScope, ProjectProbeState, ProjectSnapshot,
    ProjectStatusStore, ProjectTask, ProjectTaskGroup, interpret_project_manifest_entries,
    parse_remote_shell_project_probe_output, probe_local_project, project_manifest_file_names,
    remote_project_cwd_source_is_trusted, remote_shell_project_probe_command,
};
pub use terminal_context::infer_terminal_cwd_from_text;
