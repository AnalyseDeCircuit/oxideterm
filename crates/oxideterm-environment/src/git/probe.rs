// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

pub const SHELL_PROBE_SENTINEL: &str = "OXIDETERM_GIT_PROBE_V1";
pub const SHELL_BRANCH_LIST_SENTINEL: &str = "OXIDETERM_GIT_BRANCH_LIST_V1";
pub const SHELL_STAGED_DIFF_SENTINEL: &str = "OXIDETERM_GIT_STAGED_DIFF_V1";

pub type GitProbeCommandArgs = &'static [&'static str];

pub fn git_repo_root_args() -> GitProbeCommandArgs {
    &["rev-parse", "--show-toplevel"]
}

pub fn git_branch_args() -> GitProbeCommandArgs {
    &["symbolic-ref", "--short", "HEAD"]
}

pub fn git_head_args() -> GitProbeCommandArgs {
    &["rev-parse", "--short", "HEAD"]
}

pub fn git_absolute_git_dir_args() -> GitProbeCommandArgs {
    &["rev-parse", "--absolute-git-dir"]
}

pub fn git_status_args() -> GitProbeCommandArgs {
    &["status", "--porcelain=v2", "--branch"]
}

pub fn git_branch_list_args() -> GitProbeCommandArgs {
    &["branch", "--format=%(HEAD)%09%(refname:short)"]
}

pub fn git_worktree_list_args() -> GitProbeCommandArgs {
    &["worktree", "list", "--porcelain"]
}

pub fn git_staged_diff_stat_args() -> GitProbeCommandArgs {
    &["diff", "--cached", "--stat", "--"]
}

pub fn git_staged_diff_patch_args() -> GitProbeCommandArgs {
    &["diff", "--cached", "--patch", "--no-ext-diff", "--"]
}

pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Build a POSIX shell command for remote SSH exec probes.
///
/// The protocol uses NUL-separated records so paths and branch names containing
/// tabs or spaces do not need ad-hoc escaping in the parser.
pub fn remote_shell_probe_command(cwd: &str) -> String {
    format!(
        "{}{}{}",
        remote_shell_cd_prelude(cwd, SHELL_PROBE_SENTINEL),
        shell_operation_probe_body(),
        shell_probe_body(),
    )
}

/// Build a POSIX shell command that lists local branches and linked worktrees remotely.
pub fn remote_shell_branch_list_command(cwd: &str) -> String {
    format!(
        "{}{}",
        remote_shell_cd_prelude(cwd, SHELL_BRANCH_LIST_SENTINEL),
        shell_branch_list_body(),
    )
}

/// Build a POSIX shell command that captures staged diff content remotely.
pub fn remote_shell_staged_diff_command(cwd: &str) -> String {
    format!(
        "{}{}",
        remote_shell_cd_prelude(cwd, SHELL_STAGED_DIFF_SENTINEL),
        shell_staged_diff_body(),
    )
}

fn remote_shell_cd_prelude(cwd: &str, sentinel: &str) -> String {
    format!(
        "cd -- {} 2>/dev/null || {{ printf '{}\\0state\\0cwd_missing\\0'; exit 0; }}\n",
        remote_shell_cd_target(cwd),
        sentinel,
    )
}

fn remote_shell_cd_target(cwd: &str) -> String {
    let cwd = cwd.trim();
    if cwd == "~" {
        return "\"$HOME\"".to_string();
    }
    if let Some(rest) = cwd.strip_prefix("~/") {
        if rest.is_empty() {
            "\"$HOME\"".to_string()
        } else {
            format!("\"$HOME\"/{}", shell_quote(rest))
        }
    } else {
        shell_quote(cwd)
    }
}

fn shell_probe_body() -> &'static str {
    concat!(
        "GIT_OPTIONAL_LOCKS=0; export GIT_OPTIONAL_LOCKS\n",
        "printf 'OXIDETERM_GIT_PROBE_V1\\0'\n",
        "if ! command -v git >/dev/null 2>&1; then printf 'state\\0git_missing\\0'; exit 0; fi\n",
        "root=$(git rev-parse --show-toplevel 2>/dev/null) || { printf 'state\\0not_repo\\0'; exit 0; }\n",
        "branch=$(git symbolic-ref --short HEAD 2>/dev/null || true)\n",
        "head=$(git rev-parse --short HEAD 2>/dev/null || true)\n",
        "status=$(git status --porcelain=v2 --branch 2>/dev/null || true)\n",
        "operation=$(git_operation_state)\n",
        "printf 'state\\0repo\\0root\\0%s\\0' \"$root\"\n",
        "if [ -n \"$branch\" ]; then printf 'branch\\0%s\\0' \"$branch\"; else printf 'detached\\0%s\\0' \"$head\"; fi\n",
        "printf 'status\\0%s\\0' \"$status\"\n",
        "printf 'operation\\0%s\\0' \"$operation\"\n",
    )
}

fn shell_operation_probe_body() -> &'static str {
    concat!(
        "git_operation_state() {\n",
        "  if [ -d \"$(git rev-parse --git-path rebase-merge 2>/dev/null)\" ] || [ -d \"$(git rev-parse --git-path rebase-apply 2>/dev/null)\" ]; then printf 'rebase'; return; fi\n",
        "  if [ -f \"$(git rev-parse --git-path MERGE_HEAD 2>/dev/null)\" ]; then printf 'merge'; return; fi\n",
        "  if [ -f \"$(git rev-parse --git-path CHERRY_PICK_HEAD 2>/dev/null)\" ]; then printf 'cherry_pick'; return; fi\n",
        "  if [ -f \"$(git rev-parse --git-path REVERT_HEAD 2>/dev/null)\" ]; then printf 'revert'; return; fi\n",
        "}\n",
    )
}

fn shell_branch_list_body() -> &'static str {
    concat!(
        "GIT_OPTIONAL_LOCKS=0; export GIT_OPTIONAL_LOCKS\n",
        "printf 'OXIDETERM_GIT_BRANCH_LIST_V1\\0'\n",
        "if ! command -v git >/dev/null 2>&1; then printf 'state\\0git_missing\\0'; exit 0; fi\n",
        "git rev-parse --show-toplevel >/dev/null 2>&1 || { printf 'state\\0not_repo\\0'; exit 0; }\n",
        "printf 'state\\0ok\\0'\n",
        "tab=$(printf '\\t')\n",
        "git branch --format='%(HEAD)%09%(refname:short)' 2>/dev/null | while IFS=\"$tab\" read -r marker name; do\n",
        "  if [ -z \"$name\" ]; then continue; fi\n",
        "  current=0\n",
        "  if [ \"$marker\" = \"*\" ]; then current=1; fi\n",
        "  printf 'branch\\0%s\\0current\\0%s\\0' \"$name\" \"$current\"\n",
        "done\n",
        "worktree_path=\n",
        "git worktree list --porcelain 2>/dev/null | while IFS= read -r line; do\n",
        "  case \"$line\" in\n",
        "    'worktree '*) worktree_path=${line#worktree } ;;\n",
        "    'branch refs/heads/'*)\n",
        "      worktree_branch=${line#branch refs/heads/}\n",
        "      if [ -n \"$worktree_branch\" ] && [ -n \"$worktree_path\" ]; then\n",
        "        printf 'worktree\\0%s\\0path\\0%s\\0' \"$worktree_branch\" \"$worktree_path\"\n",
        "      fi\n",
        "      ;;\n",
        "    '') worktree_path= ;;\n",
        "  esac\n",
        "done\n",
    )
}

fn shell_staged_diff_body() -> &'static str {
    concat!(
        "GIT_OPTIONAL_LOCKS=0; export GIT_OPTIONAL_LOCKS\n",
        "printf 'OXIDETERM_GIT_STAGED_DIFF_V1\\0'\n",
        "if ! command -v git >/dev/null 2>&1; then printf 'state\\0git_missing\\0'; exit 0; fi\n",
        "git rev-parse --show-toplevel >/dev/null 2>&1 || { printf 'state\\0not_repo\\0'; exit 0; }\n",
        "stat=$(git diff --cached --stat -- 2>/dev/null || true)\n",
        "patch=$(git diff --cached --patch --no-ext-diff -- 2>/dev/null || true)\n",
        "if [ -z \"$stat\" ] && [ -z \"$patch\" ]; then printf 'state\\0empty\\0'; exit 0; fi\n",
        "printf 'state\\0ok\\0stat\\0%s\\0patch\\0%s\\0' \"$stat\" \"$patch\"\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_handles_spaces_and_quotes() {
        assert_eq!(shell_quote("/tmp/project"), "'/tmp/project'");
        assert_eq!(shell_quote("/tmp/Oxide Term"), "'/tmp/Oxide Term'");
        assert_eq!(shell_quote("/tmp/it's-ok"), "'/tmp/it'\\''s-ok'");
    }

    #[test]
    fn remote_shell_probe_contains_quoted_cwd_and_protocol_sentinel() {
        let command = remote_shell_probe_command("/tmp/it's-ok");
        assert!(command.contains("cd -- '/tmp/it'\\''s-ok'"));
        assert!(command.contains(SHELL_PROBE_SENTINEL));
        assert!(command.contains("GIT_OPTIONAL_LOCKS=0"));
        assert!(command.contains("git_operation_state()"));
    }

    #[test]
    fn remote_shell_probe_expands_home_relative_cwd_without_hardcoded_home() {
        let command = remote_shell_probe_command("~/project dir");
        assert!(command.contains("cd -- \"$HOME\"/'project dir'"));
        assert!(!command.contains("/home/"));
    }

    #[test]
    fn remote_branch_list_command_quotes_cwd() {
        let list = remote_shell_branch_list_command("/tmp/project");
        assert!(list.contains(SHELL_BRANCH_LIST_SENTINEL));
        assert!(list.contains("git branch --format='%(HEAD)%09%(refname:short)'"));
        assert!(list.contains("git worktree list --porcelain"));
        assert!(list.contains("tab=$(printf '\\t')"));
    }

    #[test]
    fn local_branch_list_uses_real_tab_format_escape() {
        assert_eq!(
            git_branch_list_args(),
            &["branch", "--format=%(HEAD)%09%(refname:short)"]
        );
        assert_eq!(
            git_worktree_list_args(),
            &["worktree", "list", "--porcelain"]
        );
        assert_eq!(git_status_args(), &["status", "--porcelain=v2", "--branch"]);
        assert_eq!(
            git_absolute_git_dir_args(),
            &["rev-parse", "--absolute-git-dir"]
        );
        assert_eq!(
            git_staged_diff_stat_args(),
            &["diff", "--cached", "--stat", "--"]
        );
        assert_eq!(
            git_staged_diff_patch_args(),
            &["diff", "--cached", "--patch", "--no-ext-diff", "--"]
        );
    }

    #[test]
    fn remote_staged_diff_command_quotes_cwd_and_emits_sentinel() {
        let command = remote_shell_staged_diff_command("/tmp/Oxide Term");
        assert!(command.contains("cd -- '/tmp/Oxide Term'"));
        assert!(command.contains(SHELL_STAGED_DIFF_SENTINEL));
        assert!(command.contains("git diff --cached --stat --"));
        assert!(command.contains("git diff --cached --patch --no-ext-diff --"));
    }
}
