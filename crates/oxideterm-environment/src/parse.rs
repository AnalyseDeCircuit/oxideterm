// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};

use crate::model::{
    GitBranchIdentity, GitBranchListOutcome, GitBranchReference, GitCheckoutOutcome, GitProbeError,
    GitProbeOutcome, GitRepositorySnapshot,
};
use crate::probe::{SHELL_BRANCH_LIST_SENTINEL, SHELL_CHECKOUT_SENTINEL, SHELL_PROBE_SENTINEL};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitCommandOutput {
    pub success: bool,
    pub stdout: String,
}

impl GitCommandOutput {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            success: true,
            stdout: stdout.into(),
        }
    }

    pub fn failure(stdout: impl Into<String>) -> Self {
        Self {
            success: false,
            stdout: stdout.into(),
        }
    }
}

pub fn interpret_git_command_outputs(
    root: GitCommandOutput,
    branch: GitCommandOutput,
    head: GitCommandOutput,
) -> GitProbeOutcome {
    if !root.success {
        return GitProbeOutcome::NotRepository;
    }

    let Some(repo_root) = non_empty_line(&root.stdout) else {
        return GitProbeOutcome::Error(GitProbeError::new("git root output was empty"));
    };

    if branch.success
        && let Some(branch_name) = non_empty_line(&branch.stdout)
    {
        return ready(repo_root, GitBranchIdentity::Branch(branch_name));
    }

    if head.success
        && let Some(head_name) = non_empty_line(&head.stdout)
    {
        return ready(repo_root, GitBranchIdentity::Detached(head_name));
    }

    GitProbeOutcome::Error(GitProbeError::new("git branch output was empty"))
}

pub fn parse_shell_probe_output(output: &str) -> GitProbeOutcome {
    let Some(start) = output.find(SHELL_PROBE_SENTINEL) else {
        return GitProbeOutcome::Error(GitProbeError::new("missing git probe sentinel"));
    };
    let records = output[start..].split('\0').collect::<Vec<_>>();
    let Some(position) = records
        .iter()
        .position(|part| *part == SHELL_PROBE_SENTINEL)
    else {
        return GitProbeOutcome::Error(GitProbeError::new("malformed git probe sentinel"));
    };
    let fields = &records[position + 1..];

    match field_value(fields, "state") {
        Some("not_repo") => GitProbeOutcome::NotRepository,
        Some("git_missing") => GitProbeOutcome::GitUnavailable,
        Some("cwd_missing") => GitProbeOutcome::CwdUnavailable,
        Some("repo") => {
            let Some(root) = field_value(fields, "root").filter(|value| !value.trim().is_empty())
            else {
                return GitProbeOutcome::Error(GitProbeError::new("missing git root"));
            };
            if let Some(branch) =
                field_value(fields, "branch").filter(|value| !value.trim().is_empty())
            {
                return ready(
                    root.to_string(),
                    GitBranchIdentity::Branch(branch.to_string()),
                );
            }
            if let Some(head) =
                field_value(fields, "detached").filter(|value| !value.trim().is_empty())
            {
                return ready(
                    root.to_string(),
                    GitBranchIdentity::Detached(head.to_string()),
                );
            }
            GitProbeOutcome::Error(GitProbeError::new("missing git branch"))
        }
        Some(_) => GitProbeOutcome::Error(GitProbeError::new("unknown git probe state")),
        None => GitProbeOutcome::Error(GitProbeError::new("missing git probe state")),
    }
}

pub fn interpret_git_branch_list_output(branches: GitCommandOutput) -> GitBranchListOutcome {
    interpret_git_branch_list_outputs(branches, GitCommandOutput::failure(""))
}

pub fn interpret_git_branch_list_outputs(
    branches: GitCommandOutput,
    worktrees: GitCommandOutput,
) -> GitBranchListOutcome {
    if !branches.success {
        return GitBranchListOutcome::NotRepository;
    }
    let worktree_paths = worktrees
        .success
        .then(|| parse_worktree_branch_paths(&worktrees.stdout))
        .unwrap_or_default();
    GitBranchListOutcome::Ready(parse_branch_list_lines(&branches.stdout, &worktree_paths))
}

pub fn parse_shell_branch_list_output(output: &str) -> GitBranchListOutcome {
    let fields = match shell_fields_after_sentinel(output, SHELL_BRANCH_LIST_SENTINEL) {
        Ok(fields) => fields,
        Err(error) => return GitBranchListOutcome::Error(error),
    };

    match field_value(&fields, "state") {
        Some("ok") => GitBranchListOutcome::Ready(parse_branch_fields(&fields)),
        Some("not_repo") => GitBranchListOutcome::NotRepository,
        Some("git_missing") => GitBranchListOutcome::GitUnavailable,
        Some("cwd_missing") => GitBranchListOutcome::CwdUnavailable,
        Some(_) => GitBranchListOutcome::Error(GitProbeError::new("unknown git branch state")),
        None => GitBranchListOutcome::Error(GitProbeError::new("missing git branch state")),
    }
}

pub fn interpret_git_checkout_status(success: bool, stderr: impl AsRef<str>) -> GitCheckoutOutcome {
    if success {
        return GitCheckoutOutcome::Switched;
    }
    GitCheckoutOutcome::Error(GitProbeError::new(first_non_empty_line(
        stderr.as_ref(),
        "git checkout failed",
    )))
}

pub fn parse_shell_checkout_output(output: &str) -> GitCheckoutOutcome {
    let fields = match shell_fields_after_sentinel(output, SHELL_CHECKOUT_SENTINEL) {
        Ok(fields) => fields,
        Err(error) => return GitCheckoutOutcome::Error(error),
    };

    match field_value(&fields, "state") {
        Some("switched") => GitCheckoutOutcome::Switched,
        Some("not_repo") => GitCheckoutOutcome::NotRepository,
        Some("git_missing") => GitCheckoutOutcome::GitUnavailable,
        Some("cwd_missing") => GitCheckoutOutcome::CwdUnavailable,
        Some("failed") => GitCheckoutOutcome::Error(GitProbeError::new(
            field_value(&fields, "message")
                .map(|message| first_non_empty_line(message, "git checkout failed"))
                .unwrap_or_else(|| "git checkout failed".to_string()),
        )),
        Some(_) => GitCheckoutOutcome::Error(GitProbeError::new("unknown git checkout state")),
        None => GitCheckoutOutcome::Error(GitProbeError::new("missing git checkout state")),
    }
}

fn shell_fields_after_sentinel<'a>(
    output: &'a str,
    sentinel: &str,
) -> Result<Vec<&'a str>, GitProbeError> {
    let Some(start) = output.find(sentinel) else {
        return Err(GitProbeError::new("missing git shell sentinel"));
    };
    let records = output[start..].split('\0').collect::<Vec<_>>();
    let Some(position) = records.iter().position(|part| *part == sentinel) else {
        return Err(GitProbeError::new("malformed git shell sentinel"));
    };
    Ok(records[position + 1..].to_vec())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GitBranchCandidate {
    name: String,
    current: bool,
}

impl GitBranchCandidate {
    fn local(name: impl Into<String>, current: bool) -> Option<Self> {
        candidate(name, current)
    }
}

fn candidate(name: impl Into<String>, current: bool) -> Option<GitBranchCandidate> {
    let name = name.into();
    let name = name.trim();
    (!name.is_empty()).then(|| GitBranchCandidate {
        name: name.to_string(),
        current,
    })
}

fn parse_branch_list_lines(
    output: &str,
    worktree_paths: &HashMap<String, String>,
) -> Vec<GitBranchReference> {
    branch_references_from_candidates(
        output.lines().filter_map(parse_branch_list_line),
        worktree_paths,
    )
}

fn parse_branch_list_line(line: &str) -> Option<GitBranchCandidate> {
    let columns = line.split('\t').collect::<Vec<_>>();
    match columns.as_slice() {
        [marker, name] => GitBranchCandidate::local(*name, marker.trim() == "*"),
        [marker, refname, short_name] => {
            parse_branch_ref_columns(marker.trim() == "*", refname, short_name)
        }
        _ => None,
    }
}

fn parse_branch_ref_columns(
    current: bool,
    refname: &str,
    short_name: &str,
) -> Option<GitBranchCandidate> {
    if let Some(name) = refname.strip_prefix("refs/heads/") {
        return GitBranchCandidate::local(name, current);
    }

    // Older callers may still pass refname plus short_name columns. Keep
    // local refs compatible but never turn remote refs into branch actions.
    refname
        .is_empty()
        .then(|| GitBranchCandidate::local(short_name, current))
        .flatten()
}

fn parse_branch_fields(fields: &[&str]) -> Vec<GitBranchReference> {
    let mut branches = Vec::new();
    let mut worktree_paths = HashMap::new();
    let mut index = 0;
    while index + 1 < fields.len() {
        match fields[index] {
            "branch" => {
                let name = fields[index + 1];
                index += 2;
                let mut current = false;
                while index + 1 < fields.len() && !matches!(fields[index], "branch" | "worktree") {
                    if fields[index] == "current" {
                        current = matches!(fields[index + 1], "1" | "true" | "*");
                    }
                    index += 2;
                }
                if let Some(branch) = candidate(name, current) {
                    branches.push(branch);
                }
            }
            "worktree" => {
                let name = fields[index + 1];
                index += 2;
                let mut path = None;
                while index + 1 < fields.len() && !matches!(fields[index], "branch" | "worktree") {
                    if fields[index] == "path" {
                        path = Some(fields[index + 1]);
                    }
                    index += 2;
                }
                if let Some(path) = path
                    && !name.trim().is_empty()
                    && !path.trim().is_empty()
                {
                    worktree_paths.insert(name.to_string(), path.to_string());
                }
            }
            _ => index += 2,
        }
    }
    branch_references_from_candidates(branches, &worktree_paths)
}

fn branch_references_from_candidates(
    candidates: impl IntoIterator<Item = GitBranchCandidate>,
    worktree_paths: &HashMap<String, String>,
) -> Vec<GitBranchReference> {
    let mut emitted = HashSet::new();
    let mut branches = Vec::new();
    for candidate in candidates {
        if !emitted.insert(candidate.name.clone()) {
            continue;
        }
        if let Some(path) = worktree_paths.get(&candidate.name) {
            if let Some(branch) = GitBranchReference::with_worktree_path(
                candidate.name,
                candidate.current,
                Some(path.clone()),
            ) {
                branches.push(branch);
            }
        } else if let Some(branch) = GitBranchReference::new(candidate.name, candidate.current) {
            branches.push(branch);
        }
    }
    branches
}

fn parse_worktree_branch_paths(output: &str) -> HashMap<String, String> {
    let mut branch_paths = HashMap::new();
    let mut worktree_path: Option<&str> = None;

    for line in output.lines() {
        if line.is_empty() {
            worktree_path = None;
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            worktree_path = Some(path);
            continue;
        }
        let Some(branch) = line.strip_prefix("branch refs/heads/") else {
            continue;
        };
        let Some(path) = worktree_path else {
            continue;
        };
        if !branch.trim().is_empty() && !path.trim().is_empty() {
            branch_paths.insert(branch.to_string(), path.to_string());
        }
    }

    branch_paths
}

fn field_value<'a>(fields: &'a [&str], key: &str) -> Option<&'a str> {
    fields.chunks_exact(2).find_map(|chunk| {
        if chunk[0] == key {
            Some(chunk[1])
        } else {
            None
        }
    })
}

fn non_empty_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn first_non_empty_line(output: &str, fallback: &str) -> String {
    non_empty_line(output).unwrap_or_else(|| fallback.to_string())
}

fn ready(repo_root: String, branch: GitBranchIdentity) -> GitProbeOutcome {
    GitRepositorySnapshot::new(repo_root, branch)
        .map(GitProbeOutcome::Ready)
        .unwrap_or_else(|| GitProbeOutcome::Error(GitProbeError::new("invalid git snapshot")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_outputs_parse_branch() {
        let outcome = interpret_git_command_outputs(
            GitCommandOutput::success("/repo\n"),
            GitCommandOutput::success("main\n"),
            GitCommandOutput::success("abc123\n"),
        );

        assert_eq!(
            outcome,
            GitProbeOutcome::Ready(
                GitRepositorySnapshot::new("/repo", GitBranchIdentity::Branch("main".to_string()),)
                    .unwrap()
            )
        );
    }

    #[test]
    fn command_outputs_fall_back_to_detached_head() {
        let outcome = interpret_git_command_outputs(
            GitCommandOutput::success("/repo\n"),
            GitCommandOutput::failure(""),
            GitCommandOutput::success("abc123\n"),
        );

        assert_eq!(
            outcome,
            GitProbeOutcome::Ready(
                GitRepositorySnapshot::new(
                    "/repo",
                    GitBranchIdentity::Detached("abc123".to_string()),
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn shell_probe_output_parses_nul_records() {
        let output =
            "noise\nOXIDETERM_GIT_PROBE_V1\0state\0repo\0root\0/tmp/Oxide Term\0branch\0feat/git\0";
        let outcome = parse_shell_probe_output(output);

        assert_eq!(
            outcome,
            GitProbeOutcome::Ready(
                GitRepositorySnapshot::new(
                    "/tmp/Oxide Term",
                    GitBranchIdentity::Branch("feat/git".to_string()),
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn shell_probe_output_handles_not_repo() {
        let output = "OXIDETERM_GIT_PROBE_V1\0state\0not_repo\0";
        assert_eq!(
            parse_shell_probe_output(output),
            GitProbeOutcome::NotRepository
        );
    }

    #[test]
    fn branch_list_output_marks_current_branch() {
        let outcome = interpret_git_branch_list_output(GitCommandOutput::success(
            "*\tmain\n \texperiment/rust-native-v2\n",
        ));

        assert_eq!(
            outcome,
            GitBranchListOutcome::Ready(vec![
                GitBranchReference::new("main", true).unwrap(),
                GitBranchReference::new("experiment/rust-native-v2", false).unwrap(),
            ])
        );
    }

    #[test]
    fn branch_list_output_attaches_worktree_paths() {
        let outcome = interpret_git_branch_list_outputs(
            GitCommandOutput::success("*\texperiment/rust-native-v2\n \tmain\n"),
            GitCommandOutput::success(
                "worktree /Users/dominical/Documents/OxideTerm\n\
                 HEAD 1111111\n\
                 branch refs/heads/experiment/rust-native-v2\n\
                 \n\
                 worktree /Users/dominical/Documents/OxideTerm-main\n\
                 HEAD 2222222\n\
                 branch refs/heads/main\n",
            ),
        );

        let GitBranchListOutcome::Ready(branches) = outcome else {
            panic!("expected branch list");
        };
        assert_eq!(
            branches,
            vec![
                GitBranchReference::with_worktree_path(
                    "experiment/rust-native-v2",
                    true,
                    Some("/Users/dominical/Documents/OxideTerm"),
                )
                .unwrap(),
                GitBranchReference::with_worktree_path(
                    "main",
                    false,
                    Some("/Users/dominical/Documents/OxideTerm-main"),
                )
                .unwrap(),
            ]
        );
        assert_eq!(
            branches[1].worktree_path(),
            Some("/Users/dominical/Documents/OxideTerm-main")
        );
    }

    #[test]
    fn branch_list_output_ignores_remote_refs() {
        let outcome = interpret_git_branch_list_output(GitCommandOutput::success(
            "\trefs/heads/main\tmain\n\
             \trefs/remotes/origin/feature/shared\torigin/feature/shared\n\
             \trefs/remotes/upstream/feature/shared\tupstream/feature/shared\n",
        ));

        assert_eq!(
            outcome,
            GitBranchListOutcome::Ready(vec![GitBranchReference::new("main", false).unwrap()])
        );
    }

    #[test]
    fn shell_branch_list_output_parses_branches() {
        let output = "noise\nOXIDETERM_GIT_BRANCH_LIST_V1\0state\0ok\0branch\0main\0current\01\0branch\0feature/x\0current\00\0worktree\0feature/x\0path\0/tmp/feature-x\0";

        assert_eq!(
            parse_shell_branch_list_output(output),
            GitBranchListOutcome::Ready(vec![
                GitBranchReference::new("main", true).unwrap(),
                GitBranchReference::with_worktree_path("feature/x", false, Some("/tmp/feature-x"))
                    .unwrap(),
            ])
        );
    }

    #[test]
    fn checkout_status_uses_first_error_line() {
        assert_eq!(
            interpret_git_checkout_status(false, "error: local changes would be overwritten\nmore"),
            GitCheckoutOutcome::Error(GitProbeError::new(
                "error: local changes would be overwritten"
            ))
        );
    }

    #[test]
    fn shell_checkout_output_parses_failure_message() {
        let output = "OXIDETERM_GIT_CHECKOUT_V1\0state\0failed\0message\0error: nope\nmore\0";

        assert_eq!(
            parse_shell_checkout_output(output),
            GitCheckoutOutcome::Error(GitProbeError::new("error: nope"))
        );
    }
}
