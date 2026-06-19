// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

pub const SHELL_PROBE_SENTINEL: &str = "OXIDETERM_GIT_PROBE_V1";
pub const SHELL_BRANCH_LIST_SENTINEL: &str = "OXIDETERM_GIT_BRANCH_LIST_V1";
pub const SHELL_CHECKOUT_SENTINEL: &str = "OXIDETERM_GIT_CHECKOUT_V1";

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

pub fn git_branch_list_args() -> GitProbeCommandArgs {
    &["branch", "--format=%(HEAD)%09%(refname:short)"]
}

pub fn git_worktree_list_args() -> GitProbeCommandArgs {
    &["worktree", "list", "--porcelain"]
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
        "{}{}",
        remote_shell_cd_prelude(cwd, SHELL_PROBE_SENTINEL),
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

/// Build a POSIX shell command that checks out one selected branch name remotely.
pub fn remote_shell_checkout_command(cwd: &str, branch: &str) -> String {
    format!(
        "{}branch={}\n{}",
        remote_shell_cd_prelude(cwd, SHELL_CHECKOUT_SENTINEL),
        shell_quote(branch),
        shell_checkout_body(),
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
        "printf 'state\\0repo\\0root\\0%s\\0' \"$root\"\n",
        "if [ -n \"$branch\" ]; then printf 'branch\\0%s\\0' \"$branch\"; else printf 'detached\\0%s\\0' \"$head\"; fi\n",
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

fn shell_checkout_body() -> &'static str {
    concat!(
        "GIT_OPTIONAL_LOCKS=0; export GIT_OPTIONAL_LOCKS\n",
        "printf 'OXIDETERM_GIT_CHECKOUT_V1\\0'\n",
        "if ! command -v git >/dev/null 2>&1; then printf 'state\\0git_missing\\0'; exit 0; fi\n",
        "git rev-parse --show-toplevel >/dev/null 2>&1 || { printf 'state\\0not_repo\\0'; exit 0; }\n",
        "output=$(git checkout --quiet \"$branch\" 2>&1)\n",
        "status=$?\n",
        "if [ \"$status\" -eq 0 ]; then printf 'state\\0switched\\0'; else printf 'state\\0failed\\0message\\0%s\\0' \"$output\"; fi\n",
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
    }

    #[test]
    fn remote_shell_probe_expands_home_relative_cwd_without_hardcoded_home() {
        let command = remote_shell_probe_command("~/project dir");
        assert!(command.contains("cd -- \"$HOME\"/'project dir'"));
        assert!(!command.contains("/home/"));
    }

    #[test]
    fn remote_branch_commands_quote_cwd_and_branch() {
        let list = remote_shell_branch_list_command("/tmp/project");
        assert!(list.contains(SHELL_BRANCH_LIST_SENTINEL));
        assert!(list.contains("git branch --format='%(HEAD)%09%(refname:short)'"));
        assert!(list.contains("git worktree list --porcelain"));
        assert!(list.contains("tab=$(printf '\\t')"));

        let checkout = remote_shell_checkout_command("/tmp/project", "feature/it's-ok");
        assert!(checkout.contains("branch='feature/it'\\''s-ok'"));
        assert!(checkout.contains(SHELL_CHECKOUT_SENTINEL));
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
    }
}
