// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};

use super::model::{
    GitBranchIdentity, GitBranchListOutcome, GitBranchReference, GitChangedPath, GitOperationKind,
    GitProbeError, GitProbeOutcome, GitRepositorySnapshot, GitRepositoryStatus,
    GitStagedDiffContext, GitStagedDiffOutcome,
};
use super::probe::{SHELL_BRANCH_LIST_SENTINEL, SHELL_PROBE_SENTINEL, SHELL_STAGED_DIFF_SENTINEL};

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
    interpret_git_command_outputs_with_status_and_operation(
        root,
        branch,
        head,
        GitCommandOutput::failure(""),
        GitCommandOutput::failure(""),
    )
}

pub fn interpret_git_command_outputs_with_status(
    root: GitCommandOutput,
    branch: GitCommandOutput,
    head: GitCommandOutput,
    status: GitCommandOutput,
) -> GitProbeOutcome {
    interpret_git_command_outputs_with_status_and_operation(
        root,
        branch,
        head,
        status,
        GitCommandOutput::failure(""),
    )
}

pub fn interpret_git_command_outputs_with_status_and_operation(
    root: GitCommandOutput,
    branch: GitCommandOutput,
    head: GitCommandOutput,
    status: GitCommandOutput,
    operation: GitCommandOutput,
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
        return ready_with_status(
            repo_root,
            GitBranchIdentity::Branch(branch_name),
            parse_successful_git_status(status)
                .with_operation(parse_successful_git_operation(operation)),
        );
    }

    if head.success
        && let Some(head_name) = non_empty_line(&head.stdout)
    {
        return ready_with_status(
            repo_root,
            GitBranchIdentity::Detached(head_name),
            parse_successful_git_status(status)
                .with_operation(parse_successful_git_operation(operation)),
        );
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
                return ready_with_status(
                    root.to_string(),
                    GitBranchIdentity::Branch(branch.to_string()),
                    field_value(fields, "status")
                        .map(parse_git_status_summary)
                        .unwrap_or_default()
                        .with_operation(
                            field_value(fields, "operation").and_then(parse_git_operation_kind),
                        ),
                );
            }
            if let Some(head) =
                field_value(fields, "detached").filter(|value| !value.trim().is_empty())
            {
                return ready_with_status(
                    root.to_string(),
                    GitBranchIdentity::Detached(head.to_string()),
                    field_value(fields, "status")
                        .map(parse_git_status_summary)
                        .unwrap_or_default()
                        .with_operation(
                            field_value(fields, "operation").and_then(parse_git_operation_kind),
                        ),
                );
            }
            GitProbeOutcome::Error(GitProbeError::new("missing git branch"))
        }
        Some(_) => GitProbeOutcome::Error(GitProbeError::new("unknown git probe state")),
        None => GitProbeOutcome::Error(GitProbeError::new("missing git probe state")),
    }
}

pub fn parse_git_operation_kind(value: &str) -> Option<GitOperationKind> {
    match value.trim() {
        "merge" => Some(GitOperationKind::Merge),
        "rebase" => Some(GitOperationKind::Rebase),
        "cherry_pick" => Some(GitOperationKind::CherryPick),
        "revert" => Some(GitOperationKind::Revert),
        _ => None,
    }
}

pub fn parse_git_status_summary(output: &str) -> GitRepositoryStatus {
    let mut upstream = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut staged = 0;
    let mut modified = 0;
    let mut untracked = 0;
    let mut conflicts = 0;
    let mut paths = Vec::new();

    for line in output.lines() {
        if let Some(value) = line.strip_prefix("# branch.upstream ") {
            upstream = Some(value.to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.ab ") {
            let (parsed_ahead, parsed_behind) = parse_branch_ahead_behind(value);
            ahead = parsed_ahead;
            behind = parsed_behind;
            continue;
        }
        if let Some(status) = porcelain_v2_xy(line, '1').or_else(|| porcelain_v2_xy(line, '2')) {
            if status.0 != '.' {
                staged += 1;
            }
            if status.1 != '.' {
                modified += 1;
            }
            if let Some(path) = parse_porcelain_v2_changed_path(line, status.0, status.1) {
                paths.push(path);
            }
            continue;
        }
        if line.starts_with("u ") {
            conflicts += 1;
            if let Some(path) = parse_porcelain_v2_conflict_path(line) {
                paths.push(path);
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("? ") {
            untracked += 1;
            if let Some(path) =
                GitChangedPath::from_parts(path, None::<String>, false, false, true, false)
            {
                paths.push(path);
            }
        }
    }

    GitRepositoryStatus::new(
        upstream, ahead, behind, staged, modified, untracked, conflicts,
    )
    .with_paths(paths)
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

pub fn interpret_git_staged_diff_outputs(
    stat: GitCommandOutput,
    patch: GitCommandOutput,
) -> GitStagedDiffOutcome {
    if !stat.success || !patch.success {
        return GitStagedDiffOutcome::Error(GitProbeError::new("git staged diff failed"));
    }
    GitStagedDiffContext::new(stat.stdout, patch.stdout)
        .map(GitStagedDiffOutcome::Ready)
        .unwrap_or(GitStagedDiffOutcome::Empty)
}

pub fn parse_shell_staged_diff_output(output: &str) -> GitStagedDiffOutcome {
    let fields = match shell_fields_after_sentinel(output, SHELL_STAGED_DIFF_SENTINEL) {
        Ok(fields) => fields,
        Err(error) => return GitStagedDiffOutcome::Error(error),
    };

    match field_value(&fields, "state") {
        Some("ok") => {
            let stat = field_value(&fields, "stat").unwrap_or_default();
            let patch = field_value(&fields, "patch").unwrap_or_default();
            GitStagedDiffContext::new(stat.to_string(), patch.to_string())
                .map(GitStagedDiffOutcome::Ready)
                .unwrap_or(GitStagedDiffOutcome::Empty)
        }
        Some("empty") => GitStagedDiffOutcome::Empty,
        Some("not_repo") => GitStagedDiffOutcome::NotRepository,
        Some("git_missing") => GitStagedDiffOutcome::GitUnavailable,
        Some("cwd_missing") => GitStagedDiffOutcome::CwdUnavailable,
        Some(_) => GitStagedDiffOutcome::Error(GitProbeError::new("unknown git diff state")),
        None => GitStagedDiffOutcome::Error(GitProbeError::new("missing git diff state")),
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

fn parse_successful_git_status(status: GitCommandOutput) -> GitRepositoryStatus {
    status
        .success
        .then(|| parse_git_status_summary(&status.stdout))
        .unwrap_or_default()
}

fn parse_successful_git_operation(operation: GitCommandOutput) -> Option<GitOperationKind> {
    operation
        .success
        .then(|| parse_git_operation_kind(&operation.stdout))
        .flatten()
}

fn parse_branch_ahead_behind(value: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    for part in value.split_whitespace() {
        if let Some(value) = part.strip_prefix('+') {
            ahead = value.parse().unwrap_or(0);
        } else if let Some(value) = part.strip_prefix('-') {
            behind = value.parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

fn porcelain_v2_xy(line: &str, record_kind: char) -> Option<(char, char)> {
    let mut parts = line.split_whitespace();
    (parts.next()?.chars().next()? == record_kind).then_some(())?;
    let xy = parts.next()?;
    let mut chars = xy.chars();
    let x = chars.next()?;
    let y = chars.next()?;
    Some((x, y))
}

fn parse_porcelain_v2_changed_path(
    line: &str,
    staged: char,
    modified: char,
) -> Option<GitChangedPath> {
    let record_kind = line.chars().next()?;
    let path_field = match record_kind {
        '1' => line.splitn(9, ' ').nth(8)?,
        '2' => line.splitn(10, ' ').nth(9)?,
        _ => return None,
    };
    let (path, original_path) = split_porcelain_path_pair(path_field);
    GitChangedPath::from_parts(
        path,
        original_path,
        staged != '.',
        modified != '.',
        false,
        false,
    )
}

fn parse_porcelain_v2_conflict_path(line: &str) -> Option<GitChangedPath> {
    let path = line.splitn(11, ' ').nth(10)?;
    GitChangedPath::from_parts(path, None::<String>, false, false, false, true)
}

fn split_porcelain_path_pair(path_field: &str) -> (&str, Option<&str>) {
    path_field
        .split_once('\t')
        .map(|(path, original)| (path, Some(original)))
        .unwrap_or((path_field, None))
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

fn ready_with_status(
    repo_root: String,
    branch: GitBranchIdentity,
    status: GitRepositoryStatus,
) -> GitProbeOutcome {
    GitRepositorySnapshot::with_status(repo_root, branch, status)
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
    fn command_outputs_parse_porcelain_v2_status() {
        let outcome = interpret_git_command_outputs_with_status_and_operation(
            GitCommandOutput::success("/repo\n"),
            GitCommandOutput::success("main\n"),
            GitCommandOutput::success("abc123\n"),
            GitCommandOutput::success(
                "# branch.oid abc123\n\
                 # branch.head main\n\
                 # branch.upstream origin/main\n\
                 # branch.ab +2 -1\n\
                 1 M. N... 100644 100644 100644 a b file.rs\n\
                 1 .M N... 100644 100644 100644 a b other.rs\n\
                 2 R. N... 100644 100644 100644 a b R100 old\tnew\n\
                 u UU N... 100644 100644 100644 100644 a b c d conflict.rs\n\
                 ? notes.txt\n",
            ),
            GitCommandOutput::success("rebase\n"),
        );

        let GitProbeOutcome::Ready(snapshot) = outcome else {
            panic!("expected ready git snapshot");
        };
        assert_eq!(snapshot.status.upstream(), Some("origin/main"));
        assert_eq!(snapshot.status.ahead(), 2);
        assert_eq!(snapshot.status.behind(), 1);
        assert_eq!(snapshot.status.staged(), 2);
        assert_eq!(snapshot.status.modified(), 1);
        assert_eq!(snapshot.status.untracked(), 1);
        assert_eq!(snapshot.status.conflicts(), 1);
        assert_eq!(snapshot.status.operation(), Some(GitOperationKind::Rebase));
        assert_eq!(snapshot.status.paths().len(), 5);
        assert_eq!(snapshot.status.paths()[0].path(), "file.rs");
        assert!(snapshot.status.paths()[0].staged());
        assert!(!snapshot.status.paths()[0].modified());
        assert_eq!(snapshot.status.paths()[2].path(), "old");
        assert_eq!(snapshot.status.paths()[2].original_path(), Some("new"));
        assert!(snapshot.status.paths()[3].conflict());
        assert_eq!(snapshot.status.paths()[4].path(), "notes.txt");
        assert!(snapshot.status.paths()[4].untracked());
        assert!(snapshot.status.is_dirty());
        assert!(snapshot.status.has_conflicts());
    }

    #[test]
    fn shell_probe_output_parses_nul_records() {
        let output = "noise\nOXIDETERM_GIT_PROBE_V1\0state\0repo\0root\0/tmp/Oxide Term\0branch\0feat/git\0status\0# branch.upstream origin/feat\n# branch.ab +1 -0\n? scratch.txt\n\0operation\0merge\0";
        let outcome = parse_shell_probe_output(output);

        let GitProbeOutcome::Ready(snapshot) = outcome else {
            panic!("expected ready shell git snapshot");
        };
        assert_eq!(snapshot.repo_root, "/tmp/Oxide Term");
        assert_eq!(
            snapshot.branch,
            GitBranchIdentity::Branch("feat/git".to_string())
        );
        assert_eq!(snapshot.status.upstream(), Some("origin/feat"));
        assert_eq!(snapshot.status.ahead(), 1);
        assert_eq!(snapshot.status.untracked(), 1);
        assert_eq!(snapshot.status.operation(), Some(GitOperationKind::Merge));
        assert_eq!(snapshot.status.paths().len(), 1);
        assert_eq!(snapshot.status.paths()[0].path(), "scratch.txt");
        assert!(snapshot.status.paths()[0].untracked());
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
    fn staged_diff_outputs_empty_when_no_cached_changes() {
        assert_eq!(
            interpret_git_staged_diff_outputs(
                GitCommandOutput::success(""),
                GitCommandOutput::success(""),
            ),
            GitStagedDiffOutcome::Empty
        );
    }

    #[test]
    fn staged_diff_outputs_keep_stat_and_patch() {
        let outcome = interpret_git_staged_diff_outputs(
            GitCommandOutput::success(" src/lib.rs | 2 ++\n"),
            GitCommandOutput::success("diff --git a/src/lib.rs b/src/lib.rs\n"),
        );

        assert_eq!(
            outcome,
            GitStagedDiffOutcome::Ready(
                GitStagedDiffContext::new(
                    " src/lib.rs | 2 ++\n",
                    "diff --git a/src/lib.rs b/src/lib.rs\n"
                )
                .unwrap()
            )
        );
    }

    #[test]
    fn shell_staged_diff_output_parses_nul_records() {
        let output = "noise\nOXIDETERM_GIT_STAGED_DIFF_V1\0state\0ok\0stat\0 src/lib.rs | 1 +\0patch\0diff --git a/src/lib.rs b/src/lib.rs\n+added\n\0";

        let GitStagedDiffOutcome::Ready(context) = parse_shell_staged_diff_output(output) else {
            panic!("expected staged diff context");
        };
        assert_eq!(context.stat(), " src/lib.rs | 1 +");
        assert!(context.patch().contains("+added"));
    }

    #[test]
    fn shell_staged_diff_output_handles_empty_state() {
        assert_eq!(
            parse_shell_staged_diff_output("OXIDETERM_GIT_STAGED_DIFF_V1\0state\0empty\0"),
            GitStagedDiffOutcome::Empty
        );
    }
}
