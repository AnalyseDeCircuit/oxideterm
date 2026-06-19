// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oxideterm_environment::{
    GitBranchListOutcome, GitBranchReference, GitCheckoutOutcome, GitCommandOutput, GitProbeKey,
    GitProbeOutcome, GitProbeScope, GitRepositorySnapshot, git_branch_args, git_branch_list_args,
    git_head_args, git_repo_root_args, git_worktree_list_args, infer_terminal_cwd_from_text,
    interpret_git_branch_list_outputs, interpret_git_checkout_status,
    interpret_git_command_outputs, parse_shell_branch_list_output, parse_shell_checkout_output,
    parse_shell_probe_output, remote_shell_branch_list_command, remote_shell_checkout_command,
    remote_shell_probe_command, shell_quote,
};
use oxideterm_ssh::NodeId;
use tokio::process::Command;

use super::*;

const TERMINAL_GIT_PROBE_TTL_MS: u64 = 5_000;
const TERMINAL_GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const TERMINAL_GIT_BRANCH_LIST_TIMEOUT: Duration = Duration::from_secs(4);
const TERMINAL_GIT_CHECKOUT_TIMEOUT: Duration = Duration::from_secs(12);
const TERMINAL_GIT_REMOTE_MAX_OUTPUT: usize = 8 * 1024;
const TERMINAL_GIT_REMOTE_CHECKOUT_MAX_OUTPUT: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub(in crate::workspace) enum TerminalGitDelivery {
    Probe {
        key: GitProbeKey,
        generation: u64,
        outcome: GitProbeOutcome,
    },
    BranchList {
        key: GitProbeKey,
        generation: u64,
        outcome: GitBranchListOutcome,
    },
    Checkout {
        key: GitProbeKey,
        branch: String,
        outcome: GitCheckoutOutcome,
    },
}

#[derive(Default)]
pub(in crate::workspace) struct TerminalGitBranchPickerState {
    pub open: bool,
    pub key: Option<GitProbeKey>,
    pub query: String,
    pub branches: Vec<GitBranchReference>,
    pub highlighted_branch: Option<String>,
    pub loading: bool,
    pub switching_branch: Option<String>,
    pub error: Option<String>,
    generation: u64,
}

impl TerminalGitBranchPickerState {
    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    fn close(&mut self) {
        *self = Self::default();
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn active_terminal_git_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<GitRepositorySnapshot> {
        let key = self.active_terminal_git_key(cx)?;
        self.terminal_git_store.snapshot(&key).cloned()
    }

    pub(in crate::workspace) fn maybe_refresh_active_terminal_git(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.active_terminal_git_key(cx) else {
            return;
        };
        let now_ms = terminal_git_now_ms();
        if !self
            .terminal_git_store
            .should_probe(&key, now_ms, TERMINAL_GIT_PROBE_TTL_MS)
        {
            return;
        }

        let generation = self.terminal_git_store.mark_loading(key.clone(), now_ms);
        match key.scope() {
            GitProbeScope::Local => self.spawn_local_terminal_git_probe(key, generation),
            GitProbeScope::SshNode(node_id) => {
                let node_id = NodeId::new(node_id.clone());
                self.spawn_remote_terminal_git_probe(key, generation, node_id, cx);
            }
        }
    }

    pub(in crate::workspace) fn poll_terminal_git_results(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Ok(delivery) = self.terminal_git_rx.try_recv() {
            match delivery {
                TerminalGitDelivery::Probe {
                    key,
                    generation,
                    outcome,
                } => {
                    changed |= self.terminal_git_store.finish_probe(
                        &key,
                        generation,
                        outcome,
                        terminal_git_now_ms(),
                    );
                }
                TerminalGitDelivery::BranchList {
                    key,
                    generation,
                    outcome,
                } => {
                    changed |= self.apply_terminal_git_branch_list_result(key, generation, outcome);
                }
                TerminalGitDelivery::Checkout {
                    key,
                    branch,
                    outcome,
                } => {
                    changed |= self.apply_terminal_git_checkout_result(key, branch, outcome, cx);
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn open_terminal_git_branch_picker(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.active_terminal_git_key(cx) else {
            return;
        };

        self.close_terminal_quick_commands_popover();
        self.dismiss_terminal_broadcast_menu();
        self.terminal_command_suggestions_open = false;
        self.terminal_command_suggestion_highlighted = None;
        self.terminal_command_bar_focused = false;
        self.ime_marked_text = None;
        self.clear_ime_selection();

        let generation = self.terminal_git_branch_picker.next_generation();
        self.terminal_git_branch_picker.open = true;
        self.terminal_git_branch_picker.key = Some(key.clone());
        self.terminal_git_branch_picker.query.clear();
        self.terminal_git_branch_picker.branches.clear();
        self.terminal_git_branch_picker.highlighted_branch = None;
        self.terminal_git_branch_picker.loading = true;
        self.terminal_git_branch_picker.switching_branch = None;
        self.terminal_git_branch_picker.error = None;
        self.spawn_terminal_git_branch_list(key, generation, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_git_branch_picker(&mut self) -> bool {
        let was_open = self.terminal_git_branch_picker.open
            || self.terminal_git_branch_picker.switching_branch.is_some();
        if was_open {
            self.terminal_git_branch_picker.close();
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        was_open
    }

    pub(in crate::workspace) fn visible_terminal_git_branches(&self) -> Vec<GitBranchReference> {
        let query = self
            .terminal_git_branch_picker
            .query
            .trim()
            .to_ascii_lowercase();
        self.terminal_git_branch_picker
            .branches
            .iter()
            .filter(|branch| {
                query.is_empty() || branch.name().to_ascii_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    pub(in crate::workspace) fn select_terminal_git_branch(
        &mut self,
        branch: GitBranchReference,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.terminal_git_branch_picker.key.clone() else {
            return;
        };
        let branch_name = branch.name().to_string();
        if branch_name.trim().is_empty()
            || self.terminal_git_branch_picker.switching_branch.is_some()
        {
            return;
        }
        if branch.current() {
            self.close_terminal_git_branch_picker();
            cx.notify();
            return;
        }

        if let Some(worktree_path) = branch.worktree_path() {
            self.send_terminal_git_worktree_cd(branch_name, worktree_path.to_string(), cx);
            return;
        }

        self.terminal_git_branch_picker.switching_branch = Some(branch_name.clone());
        self.terminal_git_branch_picker.error = None;
        self.spawn_terminal_git_checkout(key, branch_name, cx);
        cx.notify();
    }

    fn send_terminal_git_worktree_cd(
        &mut self,
        branch: String,
        worktree_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.active_pane() else {
            self.terminal_git_branch_picker.error =
                Some(self.i18n_replace("terminal.git.checkout_failed", &[("branch", branch)]));
            cx.notify();
            return;
        };
        let command = terminal_git_cd_command(&worktree_path);
        // A branch checked out by another worktree cannot be checked out in the
        // current worktree. Match terminal UX by sending an ordinary cd command
        // to the active shell, so shell integration updates cwd naturally.
        pane.update(cx, |pane, cx| pane.send_command_line(&command, cx));
        self.close_terminal_git_branch_picker();
        cx.notify();
    }

    pub(in crate::workspace) fn handle_terminal_git_branch_picker_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal_git_branch_picker.open {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }

        match key {
            "escape" => {
                self.close_terminal_git_branch_picker();
                cx.notify();
                true
            }
            "up" | "arrowup" => {
                self.step_terminal_git_branch_highlight(false);
                cx.notify();
                true
            }
            "down" | "arrowdown" => {
                self.step_terminal_git_branch_highlight(true);
                cx.notify();
                true
            }
            "home" => {
                self.highlight_terminal_git_branch_edge(false);
                cx.notify();
                true
            }
            "end" => {
                self.highlight_terminal_git_branch_edge(true);
                cx.notify();
                true
            }
            "enter" => {
                let visible = self.visible_terminal_git_branches();
                let branch = self
                    .terminal_git_branch_picker
                    .highlighted_branch
                    .as_deref()
                    .and_then(|highlighted| {
                        visible
                            .iter()
                            .find(|branch| branch.name() == highlighted)
                            .cloned()
                    })
                    .or_else(|| visible.first().cloned());
                if let Some(branch) = branch {
                    self.select_terminal_git_branch(branch, cx);
                }
                true
            }
            _ => false,
        }
    }

    fn step_terminal_git_branch_highlight(&mut self, forward: bool) {
        let visible = self.visible_terminal_git_branches();
        if visible.is_empty() {
            self.terminal_git_branch_picker.highlighted_branch = None;
            return;
        }
        let current = self
            .terminal_git_branch_picker
            .highlighted_branch
            .as_deref()
            .and_then(|highlighted| {
                visible
                    .iter()
                    .position(|branch| branch.name() == highlighted)
            });
        let next = match (current, forward) {
            (Some(index), true) => (index + 1).min(visible.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => visible.len() - 1,
        };
        self.terminal_git_branch_picker.highlighted_branch = Some(visible[next].name().to_string());
    }

    fn highlight_terminal_git_branch_edge(&mut self, last: bool) {
        let visible = self.visible_terminal_git_branches();
        self.terminal_git_branch_picker.highlighted_branch = if last {
            visible.last()
        } else {
            visible.first()
        }
        .map(|branch| branch.name().to_string());
    }

    fn active_terminal_git_key(&self, cx: &mut Context<Self>) -> Option<GitProbeKey> {
        let command_bar_settings = &self.settings_store.settings().terminal.command_bar;
        if !command_bar_settings.enabled || !command_bar_settings.git_status {
            return None;
        }

        let tab = self.active_tab()?;
        let tab_kind = tab.kind.clone();
        let pane_id = tab.active_pane_id?;
        let scope = match tab_kind {
            TabKind::LocalTerminal => GitProbeScope::Local,
            TabKind::SshTerminal => {
                let session_id = self.active_terminal_session_id()?;
                let node_id = self.terminal_ssh_nodes.get(&session_id)?;
                GitProbeScope::ssh_node(node_id.0.clone())
            }
            _ => return None,
        };
        let cwd = self.active_terminal_git_cwd(pane_id, &scope, cx)?;

        GitProbeKey::new(scope, cwd)
    }

    fn active_terminal_git_cwd(
        &self,
        pane_id: PaneId,
        scope: &GitProbeScope,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let pane = self.panes.get(&pane_id)?;
        let pane = pane.read(cx);
        // OSC 7 / shell integration is the authoritative cwd source. The
        // visible-text fallback only recovers the cwd for Git UX; SSH ownership
        // and credential scope must still come from workspace state.
        let cwd = pane
            .current_working_directory()
            .or_else(|| infer_terminal_cwd_from_text(&pane.visible_text_snapshot()))?;
        Some(match scope {
            GitProbeScope::Local => terminal_git_expand_local_home(&cwd),
            GitProbeScope::SshNode(_) => cwd,
        })
    }

    fn spawn_local_terminal_git_probe(&self, key: GitProbeKey, generation: u64) {
        let tx = self.terminal_git_tx.clone();
        let cwd = key.cwd().to_string();
        self.forwarding_runtime.spawn(async move {
            let outcome = run_local_git_probe(&cwd).await;
            let _ = tx.send(TerminalGitDelivery::Probe {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_remote_terminal_git_probe(
        &mut self,
        key: GitProbeKey,
        generation: u64,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let resolved = self.node_router.resolve_connection_now(&node_id);
        let handle = match resolved {
            Ok(resolved) => resolved.handle,
            Err(_) => {
                let changed = self.terminal_git_store.finish_probe(
                    &key,
                    generation,
                    GitProbeOutcome::Error(oxideterm_environment::GitProbeError::new(
                        "ssh node is not ready for git probing",
                    )),
                    terminal_git_now_ms(),
                );
                if changed {
                    cx.notify();
                }
                return;
            }
        };

        let tx = self.terminal_git_tx.clone();
        let command = remote_shell_probe_command(key.cwd());
        self.forwarding_runtime.spawn(async move {
            let outcome = match handle
                .run_command_capture(
                    &command,
                    TERMINAL_GIT_PROBE_TIMEOUT,
                    TERMINAL_GIT_REMOTE_MAX_OUTPUT,
                )
                .await
            {
                Ok(output) => parse_shell_probe_output(&output.stdout),
                Err(_) => GitProbeOutcome::Error(oxideterm_environment::GitProbeError::new(
                    "ssh git probe failed",
                )),
            };
            let _ = tx.send(TerminalGitDelivery::Probe {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_terminal_git_branch_list(
        &mut self,
        key: GitProbeKey,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        match key.scope() {
            GitProbeScope::Local => self.spawn_local_terminal_git_branch_list(key, generation),
            GitProbeScope::SshNode(node_id) => {
                let node_id = NodeId::new(node_id.clone());
                self.spawn_remote_terminal_git_branch_list(key, generation, node_id, cx);
            }
        }
    }

    fn spawn_terminal_git_checkout(
        &mut self,
        key: GitProbeKey,
        branch: String,
        cx: &mut Context<Self>,
    ) {
        match key.scope() {
            GitProbeScope::Local => self.spawn_local_terminal_git_checkout(key, branch),
            GitProbeScope::SshNode(node_id) => {
                let node_id = NodeId::new(node_id.clone());
                self.spawn_remote_terminal_git_checkout(key, branch, node_id, cx);
            }
        }
    }

    fn spawn_local_terminal_git_branch_list(&self, key: GitProbeKey, generation: u64) {
        let tx = self.terminal_git_tx.clone();
        let cwd = key.cwd().to_string();
        self.forwarding_runtime.spawn(async move {
            let outcome = run_local_git_branch_list(&cwd).await;
            let _ = tx.send(TerminalGitDelivery::BranchList {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_local_terminal_git_checkout(&self, key: GitProbeKey, branch: String) {
        let tx = self.terminal_git_tx.clone();
        let cwd = key.cwd().to_string();
        self.forwarding_runtime.spawn(async move {
            let outcome = run_local_git_checkout(&cwd, &branch).await;
            let _ = tx.send(TerminalGitDelivery::Checkout {
                key,
                branch,
                outcome,
            });
        });
    }

    fn spawn_remote_terminal_git_branch_list(
        &mut self,
        key: GitProbeKey,
        generation: u64,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let resolved = self.node_router.resolve_connection_now(&node_id);
        let handle = match resolved {
            Ok(resolved) => resolved.handle,
            Err(_) => {
                self.terminal_git_branch_picker.loading = false;
                self.terminal_git_branch_picker.error =
                    Some(self.i18n.t("terminal.git.branch_node_unavailable"));
                cx.notify();
                return;
            }
        };

        let tx = self.terminal_git_tx.clone();
        let command = remote_shell_branch_list_command(key.cwd());
        self.forwarding_runtime.spawn(async move {
            let outcome = match handle
                .run_command_capture(
                    &command,
                    TERMINAL_GIT_BRANCH_LIST_TIMEOUT,
                    TERMINAL_GIT_REMOTE_MAX_OUTPUT,
                )
                .await
            {
                Ok(output) => parse_shell_branch_list_output(&output.stdout),
                Err(_) => GitBranchListOutcome::Error(oxideterm_environment::GitProbeError::new(
                    "ssh git branch list failed",
                )),
            };
            let _ = tx.send(TerminalGitDelivery::BranchList {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_remote_terminal_git_checkout(
        &mut self,
        key: GitProbeKey,
        branch: String,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let resolved = self.node_router.resolve_connection_now(&node_id);
        let handle = match resolved {
            Ok(resolved) => resolved.handle,
            Err(_) => {
                self.terminal_git_branch_picker.switching_branch = None;
                self.terminal_git_branch_picker.error =
                    Some(self.i18n.t("terminal.git.branch_node_unavailable"));
                cx.notify();
                return;
            }
        };

        let tx = self.terminal_git_tx.clone();
        let command = remote_shell_checkout_command(key.cwd(), &branch);
        self.forwarding_runtime.spawn(async move {
            let outcome = match handle
                .run_command_capture(
                    &command,
                    TERMINAL_GIT_CHECKOUT_TIMEOUT,
                    TERMINAL_GIT_REMOTE_CHECKOUT_MAX_OUTPUT,
                )
                .await
            {
                Ok(output) => parse_shell_checkout_output(&output.stdout),
                Err(_) => GitCheckoutOutcome::Error(oxideterm_environment::GitProbeError::new(
                    "ssh git checkout failed",
                )),
            };
            let _ = tx.send(TerminalGitDelivery::Checkout {
                key,
                branch,
                outcome,
            });
        });
    }

    fn apply_terminal_git_branch_list_result(
        &mut self,
        key: GitProbeKey,
        generation: u64,
        outcome: GitBranchListOutcome,
    ) -> bool {
        if !self.terminal_git_branch_picker.open
            || self.terminal_git_branch_picker.key.as_ref() != Some(&key)
            || self.terminal_git_branch_picker.generation != generation
        {
            return false;
        }

        self.terminal_git_branch_picker.loading = false;
        match outcome {
            GitBranchListOutcome::Ready(branches) => {
                self.terminal_git_branch_picker.error = None;
                self.terminal_git_branch_picker.highlighted_branch = branches
                    .iter()
                    .find(|branch| branch.current())
                    .or_else(|| branches.first())
                    .map(|branch| branch.name().to_string());
                self.terminal_git_branch_picker.branches = branches;
            }
            GitBranchListOutcome::NotRepository => {
                self.terminal_git_branch_picker.branches.clear();
                self.terminal_git_branch_picker.highlighted_branch = None;
                self.terminal_git_branch_picker.error =
                    Some(self.i18n.t("terminal.git.branch_not_repository"));
            }
            GitBranchListOutcome::GitUnavailable => {
                self.terminal_git_branch_picker.branches.clear();
                self.terminal_git_branch_picker.highlighted_branch = None;
                self.terminal_git_branch_picker.error =
                    Some(self.i18n.t("terminal.git.branch_git_unavailable"));
            }
            GitBranchListOutcome::CwdUnavailable => {
                self.terminal_git_branch_picker.branches.clear();
                self.terminal_git_branch_picker.highlighted_branch = None;
                self.terminal_git_branch_picker.error =
                    Some(self.i18n.t("terminal.git.branch_cwd_unavailable"));
            }
            GitBranchListOutcome::Error(error) => {
                self.terminal_git_branch_picker.branches.clear();
                self.terminal_git_branch_picker.highlighted_branch = None;
                self.terminal_git_branch_picker.error = Some(error.message().to_string());
            }
        }
        true
    }

    fn apply_terminal_git_checkout_result(
        &mut self,
        key: GitProbeKey,
        branch: String,
        outcome: GitCheckoutOutcome,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.terminal_git_branch_picker.key.as_ref() != Some(&key)
            || self.terminal_git_branch_picker.switching_branch.as_deref() != Some(&branch)
        {
            return false;
        }

        self.terminal_git_branch_picker.switching_branch = None;
        match outcome {
            GitCheckoutOutcome::Switched => {
                self.push_command_palette_toast(
                    self.i18n_replace(
                        "terminal.git.checkout_success",
                        &[("branch", branch.clone())],
                    ),
                    None,
                    TerminalNoticeVariant::Success,
                );
                self.close_terminal_git_branch_picker();
                self.force_refresh_terminal_git_status(key, cx);
            }
            GitCheckoutOutcome::NotRepository => {
                self.terminal_git_branch_picker.error =
                    Some(self.i18n.t("terminal.git.branch_not_repository"));
            }
            GitCheckoutOutcome::GitUnavailable => {
                self.terminal_git_branch_picker.error =
                    Some(self.i18n.t("terminal.git.branch_git_unavailable"));
            }
            GitCheckoutOutcome::CwdUnavailable => {
                self.terminal_git_branch_picker.error =
                    Some(self.i18n.t("terminal.git.branch_cwd_unavailable"));
            }
            GitCheckoutOutcome::Error(error) => {
                self.terminal_git_branch_picker.error = Some(error.message().to_string());
                self.push_command_palette_toast(
                    self.i18n_replace("terminal.git.checkout_failed", &[("branch", branch)]),
                    Some(error.message().to_string()),
                    TerminalNoticeVariant::Error,
                );
            }
        }
        true
    }

    fn force_refresh_terminal_git_status(&mut self, key: GitProbeKey, cx: &mut Context<Self>) {
        let generation = self
            .terminal_git_store
            .mark_loading(key.clone(), terminal_git_now_ms());
        match key.scope() {
            GitProbeScope::Local => self.spawn_local_terminal_git_probe(key, generation),
            GitProbeScope::SshNode(node_id) => {
                let node_id = NodeId::new(node_id.clone());
                self.spawn_remote_terminal_git_probe(key, generation, node_id, cx);
            }
        }
    }
}

async fn run_local_git_probe(cwd: &str) -> GitProbeOutcome {
    let root = match run_local_git_command(cwd, git_repo_root_args()).await {
        Ok(output) => output,
        Err(LocalGitProbeError::GitMissing) => return GitProbeOutcome::GitUnavailable,
        Err(LocalGitProbeError::Timeout) => {
            return GitProbeOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git probe timed out",
            ));
        }
        Err(LocalGitProbeError::Io) => {
            return GitProbeOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git probe failed",
            ));
        }
    };
    let branch = run_local_git_command(cwd, git_branch_args())
        .await
        .unwrap_or_else(|_| GitCommandOutput::failure(""));
    let head = run_local_git_command(cwd, git_head_args())
        .await
        .unwrap_or_else(|_| GitCommandOutput::failure(""));

    interpret_git_command_outputs(root, branch, head)
}

async fn run_local_git_branch_list(cwd: &str) -> GitBranchListOutcome {
    let branches = match run_local_git_command_with_timeout(
        cwd,
        git_branch_list_args(),
        TERMINAL_GIT_BRANCH_LIST_TIMEOUT,
    )
    .await
    {
        Ok(output) => output,
        Err(LocalGitProbeError::GitMissing) => return GitBranchListOutcome::GitUnavailable,
        Err(LocalGitProbeError::Timeout) => {
            return GitBranchListOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git branch list timed out",
            ));
        }
        Err(LocalGitProbeError::Io) => {
            return GitBranchListOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git branch list failed",
            ));
        }
    };
    let worktrees = run_local_git_command_with_timeout(
        cwd,
        git_worktree_list_args(),
        TERMINAL_GIT_BRANCH_LIST_TIMEOUT,
    )
    .await
    .unwrap_or_else(|_| GitCommandOutput::failure(""));

    interpret_git_branch_list_outputs(branches, worktrees)
}

async fn run_local_git_checkout(cwd: &str, branch: &str) -> GitCheckoutOutcome {
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(cwd)
        .arg("checkout")
        .arg("--quiet")
        .arg(branch)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = match tokio::time::timeout(TERMINAL_GIT_CHECKOUT_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return GitCheckoutOutcome::GitUnavailable;
        }
        Ok(Err(_)) => {
            return GitCheckoutOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git checkout failed",
            ));
        }
        Err(_) => {
            return GitCheckoutOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git checkout timed out",
            ));
        }
    };

    interpret_git_checkout_status(
        output.status.success(),
        String::from_utf8_lossy(&output.stderr),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalGitProbeError {
    GitMissing,
    Timeout,
    Io,
}

async fn run_local_git_command(
    cwd: &str,
    args: oxideterm_environment::GitProbeCommandArgs,
) -> Result<GitCommandOutput, LocalGitProbeError> {
    run_local_git_command_with_timeout(cwd, args, TERMINAL_GIT_PROBE_TIMEOUT).await
}

async fn run_local_git_command_with_timeout(
    cwd: &str,
    args: oxideterm_environment::GitProbeCommandArgs,
    timeout: Duration,
) -> Result<GitCommandOutput, LocalGitProbeError> {
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| LocalGitProbeError::Timeout)?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                LocalGitProbeError::GitMissing
            } else {
                LocalGitProbeError::Io
            }
        })?;

    Ok(GitCommandOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
    })
}

fn terminal_git_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn terminal_git_expand_local_home(cwd: &str) -> String {
    if cwd == "~" {
        return terminal_git_local_home()
            .map(|home| home.to_string_lossy().to_string())
            .unwrap_or_else(|| cwd.to_string());
    }
    if let Some(rest) = cwd.strip_prefix("~/")
        && let Some(home) = terminal_git_local_home()
    {
        // Prompt-derived local paths can use shell shorthand, but `git -C`
        // needs a filesystem path after the shell has already finished.
        return home.join(rest).to_string_lossy().to_string();
    }
    cwd.to_string()
}

fn terminal_git_local_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

fn terminal_git_cd_command(path: &str) -> String {
    format!("cd {}", shell_quote(path))
}
