// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oxideterm_ai::{
    AiChatMessage, AiChatRole, AiChatStreamConfig, AiStreamEvent,
    provider_chat_requires_key as ai_provider_chat_requires_key, stream_chat_completion,
};
use oxideterm_environment::{
    GitBranchListOutcome, GitBranchReference, GitCommandOutput, GitOperationKind, GitProbeKey,
    GitProbeOutcome, GitProbeScope, GitRepositorySnapshot, GitStagedDiffContext,
    GitStagedDiffOutcome, git_absolute_git_dir_args, git_branch_args, git_branch_list_args,
    git_head_args, git_repo_root_args, git_staged_diff_patch_args, git_staged_diff_stat_args,
    git_status_args, git_worktree_list_args, infer_terminal_cwd_from_text,
    interpret_git_branch_list_outputs, interpret_git_command_outputs_with_status_and_operation,
    interpret_git_staged_diff_outputs, parse_shell_branch_list_output, parse_shell_probe_output,
    parse_shell_staged_diff_output, remote_shell_branch_list_command, remote_shell_probe_command,
    remote_shell_staged_diff_command, shell_quote,
};
use oxideterm_ssh::NodeId;
use tokio::process::Command;

use super::*;

const TERMINAL_GIT_PROBE_TTL_MS: u64 = 5_000;
const TERMINAL_GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const TERMINAL_GIT_BRANCH_LIST_TIMEOUT: Duration = Duration::from_secs(4);
const TERMINAL_GIT_AI_DIFF_TIMEOUT: Duration = Duration::from_secs(6);
const TERMINAL_GIT_REMOTE_MAX_OUTPUT: usize = 8 * 1024;
const TERMINAL_GIT_AI_DIFF_REMOTE_MAX_OUTPUT: usize = 128 * 1024;
const TERMINAL_GIT_AI_DIFF_MAX_CHARS: usize = 24_000;
const TERMINAL_GIT_COMMIT_SUBJECT_MAX_CHARS: usize = 96;

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
    AiCommitMessage {
        generation: u64,
        outcome: TerminalGitAiCommitMessageOutcome,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalGitAiCommitMessageOutcome {
    Ready(String),
    EmptyStagedDiff,
    NotRepository,
    GitUnavailable,
    CwdUnavailable,
    Error(String),
}

#[derive(Default)]
pub(in crate::workspace) struct TerminalGitBranchPickerState {
    pub open: bool,
    pub key: Option<GitProbeKey>,
    pub query: String,
    pub branches: Vec<GitBranchReference>,
    pub highlighted_branch: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub ai_commit_loading: bool,
    pub ai_commit_error: Option<String>,
    ai_commit_generation: u64,
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

    fn next_ai_commit_generation(&mut self) -> u64 {
        let next = terminal_git_now_ms().max(self.ai_commit_generation.saturating_add(1));
        self.ai_commit_generation = next;
        next
    }

    fn reset_ai_commit_message(&mut self) {
        self.ai_commit_loading = false;
        self.ai_commit_error = None;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalGitRepositoryAction {
    Fetch,
    Pull,
    Push,
    Status,
    Diff,
    Log,
    Stash,
    StashPop,
    StageAll,
    Commit,
    RebasePull,
    Continue(GitOperationKind),
    Abort(GitOperationKind),
    Skip(GitOperationKind),
}

impl TerminalGitRepositoryAction {
    pub(in crate::workspace) fn label_key(self) -> &'static str {
        match self {
            Self::Fetch => "terminal.git.action_fetch",
            Self::Pull => "terminal.git.action_pull",
            Self::Push => "terminal.git.action_push",
            Self::Status => "terminal.git.action_status",
            Self::Diff => "terminal.git.action_diff",
            Self::Log => "terminal.git.action_log",
            Self::Stash => "terminal.git.action_stash",
            Self::StashPop => "terminal.git.action_stash_pop",
            Self::StageAll => "terminal.git.action_stage_all",
            Self::Commit => "terminal.git.action_commit",
            Self::RebasePull => "terminal.git.action_rebase_pull",
            Self::Continue(_) => "terminal.git.action_continue",
            Self::Abort(_) => "terminal.git.action_abort",
            Self::Skip(_) => "terminal.git.action_skip",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalGitActionPlan {
    command: String,
}

impl TerminalGitActionPlan {
    // The plan layer only builds shell-visible commands. Execution stays with
    // the active pane so local and SSH sessions share the same user-visible path.
    fn select_branch(branch: &GitBranchReference) -> Option<Self> {
        let branch_name = branch.name().trim();
        if !terminal_git_accepts_single_arg(branch_name) {
            return None;
        }

        let command = if let Some(worktree_path) = branch.worktree_path() {
            terminal_git_cd_command(worktree_path)
        } else {
            terminal_git_checkout_command(branch_name)
        };
        Some(Self { command })
    }

    fn checkout_name(branch_name: &str) -> Option<Self> {
        let branch_name = branch_name.trim();
        terminal_git_accepts_single_arg(branch_name).then(|| Self {
            command: terminal_git_checkout_command(branch_name),
        })
    }

    fn rebase_onto_name(branch_name: &str) -> Option<Self> {
        let branch_name = branch_name.trim();
        terminal_git_accepts_single_arg(branch_name).then(|| Self {
            command: terminal_git_rebase_command(branch_name),
        })
    }

    fn repository_action(action: TerminalGitRepositoryAction) -> Self {
        let command = match action {
            TerminalGitRepositoryAction::Fetch => "git fetch --prune",
            // A one-click pull must not create a merge commit. If fast-forward
            // is impossible, Git will explain the recovery path in the terminal.
            TerminalGitRepositoryAction::Pull => "git pull --ff-only",
            TerminalGitRepositoryAction::Push => "git push",
            TerminalGitRepositoryAction::Status => "git status --short --branch",
            TerminalGitRepositoryAction::Diff => "git diff --stat",
            TerminalGitRepositoryAction::Log => "git log --oneline --decorate --graph -20",
            TerminalGitRepositoryAction::Stash => "git stash push",
            TerminalGitRepositoryAction::StashPop => "git stash pop",
            TerminalGitRepositoryAction::StageAll => "git add -A",
            TerminalGitRepositoryAction::Commit => "git commit",
            TerminalGitRepositoryAction::RebasePull => "git pull --rebase",
            TerminalGitRepositoryAction::Continue(operation) => {
                terminal_git_operation_command(operation, "continue")
            }
            TerminalGitRepositoryAction::Abort(operation) => {
                terminal_git_operation_command(operation, "abort")
            }
            TerminalGitRepositoryAction::Skip(operation) => {
                terminal_git_operation_command(operation, "skip")
            }
        };
        Self {
            command: command.to_string(),
        }
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
                TerminalGitDelivery::AiCommitMessage {
                    generation,
                    outcome,
                } => {
                    changed |=
                        self.apply_terminal_git_ai_commit_message_result(generation, outcome, cx);
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
        self.terminal_git_branch_picker.error = None;
        self.terminal_git_branch_picker.reset_ai_commit_message();
        self.spawn_terminal_git_branch_list(key, generation, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_git_branch_picker(&mut self) -> bool {
        let was_open = self.terminal_git_branch_picker.open;
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

    pub(in crate::workspace) fn terminal_git_query_checkout_candidate(&self) -> Option<String> {
        let query = self.terminal_git_branch_picker.query.trim();
        if !terminal_git_accepts_single_arg(query) {
            return None;
        }
        if self
            .terminal_git_branch_picker
            .branches
            .iter()
            .any(|branch| branch.name() == query)
        {
            return None;
        }
        Some(query.to_string())
    }

    pub(in crate::workspace) fn terminal_git_query_rebase_candidate(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let query = self.terminal_git_branch_picker.query.trim();
        if !terminal_git_accepts_single_arg(query) {
            return None;
        }
        let current_branch = self
            .active_terminal_git_snapshot(cx)
            .map(|snapshot| snapshot.branch.display_text().to_string());
        if current_branch.as_deref() == Some(query) {
            return None;
        }
        Some(query.to_string())
    }

    pub(in crate::workspace) fn checkout_terminal_git_query(&mut self, cx: &mut Context<Self>) {
        let Some(branch_name) = self.terminal_git_query_checkout_candidate() else {
            return;
        };
        let Some(plan) = TerminalGitActionPlan::checkout_name(&branch_name) else {
            return;
        };
        let failure_message =
            self.i18n_replace("terminal.git.checkout_failed", &[("branch", branch_name)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn rebase_terminal_git_query(&mut self, cx: &mut Context<Self>) {
        let Some(branch_name) = self.terminal_git_query_rebase_candidate(cx) else {
            return;
        };
        let Some(plan) = TerminalGitActionPlan::rebase_onto_name(&branch_name) else {
            return;
        };
        let action_label = self.i18n.t("terminal.git.action_rebase");
        let failure_message =
            self.i18n_replace("terminal.git.command_failed", &[("action", action_label)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn select_terminal_git_branch(
        &mut self,
        branch: GitBranchReference,
        cx: &mut Context<Self>,
    ) {
        let branch_name = branch.name().to_string();
        if branch_name.trim().is_empty() {
            return;
        }
        if branch.current() {
            self.close_terminal_git_branch_picker();
            cx.notify();
            return;
        }

        let Some(plan) = TerminalGitActionPlan::select_branch(&branch) else {
            return;
        };
        let failure_message =
            self.i18n_replace("terminal.git.checkout_failed", &[("branch", branch_name)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn run_terminal_git_repository_action(
        &mut self,
        action: TerminalGitRepositoryAction,
        cx: &mut Context<Self>,
    ) {
        let plan = TerminalGitActionPlan::repository_action(action);
        let action_label = self.i18n.t(action.label_key());
        let failure_message =
            self.i18n_replace("terminal.git.command_failed", &[("action", action_label)]);
        self.send_terminal_git_command(plan, failure_message, cx);
    }

    pub(in crate::workspace) fn generate_terminal_git_ai_commit_message(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.terminal_git_branch_picker.ai_commit_loading {
            return;
        }

        let Some(key) = self.active_terminal_git_key(cx) else {
            self.terminal_git_branch_picker.ai_commit_error =
                Some(self.i18n.t("terminal.git.ai_commit_not_repository"));
            cx.notify();
            return;
        };
        let config = match self.resolve_terminal_ai_inline_config() {
            Ok(config) => config,
            Err(message) => {
                self.terminal_git_branch_picker.ai_commit_error = Some(message);
                cx.notify();
                return;
            }
        };

        let generation = self.terminal_git_branch_picker.next_ai_commit_generation();
        self.terminal_git_branch_picker.ai_commit_loading = true;
        self.terminal_git_branch_picker.ai_commit_error = None;

        let provider_id = config.provider_id.clone();
        let requires_key = ai_provider_chat_requires_key(&config.provider_type);
        let key_store = self.ai_key_store.clone();
        let api_key_not_found = self.i18n.t("ai.model_selector.api_key_not_found");
        let failed_to_get_key = self.i18n.t("ai.model_selector.failed_to_get_api_key");
        let context_max_chars = self.settings_store.settings().ai.context_max_chars.max(0) as usize;
        let max_context_chars = context_max_chars.clamp(4_000, TERMINAL_GIT_AI_DIFF_MAX_CHARS);
        let tx = self.terminal_git_tx.clone();

        match key.scope() {
            GitProbeScope::Local => {
                let cwd = key.cwd().to_string();
                self.forwarding_runtime.spawn(async move {
                    let outcome = terminal_git_generate_ai_commit_message(
                        run_local_git_staged_diff(&cwd).await,
                        config,
                        provider_id,
                        requires_key,
                        key_store,
                        api_key_not_found,
                        failed_to_get_key,
                        max_context_chars,
                    )
                    .await;
                    let _ = tx.send(TerminalGitDelivery::AiCommitMessage {
                        generation,
                        outcome,
                    });
                });
            }
            GitProbeScope::SshNode(node_id) => {
                let resolved = self
                    .node_router
                    .resolve_connection_now(&NodeId::new(node_id.clone()));
                let handle = match resolved {
                    Ok(resolved) => resolved.handle,
                    Err(_) => {
                        self.terminal_git_branch_picker.ai_commit_loading = false;
                        self.terminal_git_branch_picker.ai_commit_error =
                            Some(self.i18n.t("terminal.git.ai_commit_node_unavailable"));
                        cx.notify();
                        return;
                    }
                };
                let command = remote_shell_staged_diff_command(key.cwd());
                self.forwarding_runtime.spawn(async move {
                    let diff_outcome = match handle
                        .run_command_capture(
                            &command,
                            TERMINAL_GIT_AI_DIFF_TIMEOUT,
                            TERMINAL_GIT_AI_DIFF_REMOTE_MAX_OUTPUT,
                        )
                        .await
                    {
                        Ok(output) => parse_shell_staged_diff_output(&output.stdout),
                        Err(_) => GitStagedDiffOutcome::Error(
                            oxideterm_environment::GitProbeError::new("ssh git staged diff failed"),
                        ),
                    };
                    let outcome = terminal_git_generate_ai_commit_message(
                        diff_outcome,
                        config,
                        provider_id,
                        requires_key,
                        key_store,
                        api_key_not_found,
                        failed_to_get_key,
                        max_context_chars,
                    )
                    .await;
                    let _ = tx.send(TerminalGitDelivery::AiCommitMessage {
                        generation,
                        outcome,
                    });
                });
            }
        }
        cx.notify();
    }

    fn send_terminal_git_command(
        &mut self,
        plan: TerminalGitActionPlan,
        failure_message: String,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.active_pane() else {
            self.terminal_git_branch_picker.error = Some(failure_message);
            cx.notify();
            return;
        };
        // Git actions are sent through the active terminal so the user sees
        // Git's own output, conflict prompts, and any recovery instructions.
        pane.update(cx, |pane, cx| pane.send_command_line(&plan.command, cx));
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

    fn apply_terminal_git_ai_commit_message_result(
        &mut self,
        generation: u64,
        outcome: TerminalGitAiCommitMessageOutcome,
        _cx: &mut Context<Self>,
    ) -> bool {
        if self.terminal_git_branch_picker.ai_commit_generation != generation {
            return false;
        }

        self.terminal_git_branch_picker.ai_commit_loading = false;
        match outcome {
            TerminalGitAiCommitMessageOutcome::Ready(message) => {
                let Some(command) = terminal_git_commit_command_from_ai_message(&message) else {
                    self.terminal_git_branch_picker.ai_commit_error =
                        Some(self.i18n.t("terminal.git.ai_commit_failed"));
                    return true;
                };
                // The AI result becomes an editable command-bar draft. The
                // repository mutation still happens only after the user submits it.
                self.terminal_command_input_collapsed = false;
                self.terminal_command_bar_focused = true;
                self.terminal_command_bar_draft = command;
                self.terminal_command_suggestions_open = false;
                self.terminal_command_suggestion_highlighted = None;
                self.ime_marked_text = None;
                self.clear_ime_selection();
                self.close_terminal_git_branch_picker();
            }
            TerminalGitAiCommitMessageOutcome::EmptyStagedDiff => {
                self.terminal_git_branch_picker.ai_commit_error =
                    Some(self.i18n.t("terminal.git.ai_commit_no_staged_changes"));
            }
            TerminalGitAiCommitMessageOutcome::NotRepository => {
                self.terminal_git_branch_picker.ai_commit_error =
                    Some(self.i18n.t("terminal.git.ai_commit_not_repository"));
            }
            TerminalGitAiCommitMessageOutcome::GitUnavailable => {
                self.terminal_git_branch_picker.ai_commit_error =
                    Some(self.i18n.t("terminal.git.ai_commit_git_unavailable"));
            }
            TerminalGitAiCommitMessageOutcome::CwdUnavailable => {
                self.terminal_git_branch_picker.ai_commit_error =
                    Some(self.i18n.t("terminal.git.ai_commit_cwd_unavailable"));
            }
            TerminalGitAiCommitMessageOutcome::Error(message) => {
                self.terminal_git_branch_picker.ai_commit_error = Some(message);
            }
        }
        true
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
    let status = run_local_git_command(cwd, git_status_args())
        .await
        .unwrap_or_else(|_| GitCommandOutput::failure(""));
    let operation = run_local_git_operation_probe(cwd)
        .await
        .unwrap_or_else(|_| GitCommandOutput::failure(""));

    interpret_git_command_outputs_with_status_and_operation(root, branch, head, status, operation)
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

async fn run_local_git_staged_diff(cwd: &str) -> GitStagedDiffOutcome {
    let root = match run_local_git_command_with_timeout(
        cwd,
        git_repo_root_args(),
        TERMINAL_GIT_AI_DIFF_TIMEOUT,
    )
    .await
    {
        Ok(output) => output,
        Err(LocalGitProbeError::GitMissing) => return GitStagedDiffOutcome::GitUnavailable,
        Err(LocalGitProbeError::Timeout) => {
            return GitStagedDiffOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git staged diff timed out",
            ));
        }
        Err(LocalGitProbeError::Io) => {
            return GitStagedDiffOutcome::Error(oxideterm_environment::GitProbeError::new(
                "local git staged diff failed",
            ));
        }
    };
    if !root.success {
        return GitStagedDiffOutcome::NotRepository;
    }

    let stat = match run_local_git_command_with_timeout(
        cwd,
        git_staged_diff_stat_args(),
        TERMINAL_GIT_AI_DIFF_TIMEOUT,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => return local_git_staged_diff_error(error),
    };
    let patch = match run_local_git_command_with_timeout(
        cwd,
        git_staged_diff_patch_args(),
        TERMINAL_GIT_AI_DIFF_TIMEOUT,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => return local_git_staged_diff_error(error),
    };

    interpret_git_staged_diff_outputs(stat, patch)
}

fn local_git_staged_diff_error(error: LocalGitProbeError) -> GitStagedDiffOutcome {
    match error {
        LocalGitProbeError::GitMissing => GitStagedDiffOutcome::GitUnavailable,
        LocalGitProbeError::Timeout => GitStagedDiffOutcome::Error(
            oxideterm_environment::GitProbeError::new("local git staged diff timed out"),
        ),
        LocalGitProbeError::Io => GitStagedDiffOutcome::Error(
            oxideterm_environment::GitProbeError::new("local git staged diff failed"),
        ),
    }
}

async fn terminal_git_generate_ai_commit_message(
    diff_outcome: GitStagedDiffOutcome,
    mut config: AiChatStreamConfig,
    provider_id: Option<String>,
    requires_key: bool,
    key_store: oxideterm_ai::AiProviderKeyStore,
    api_key_not_found: String,
    failed_to_get_key: String,
    max_context_chars: usize,
) -> TerminalGitAiCommitMessageOutcome {
    let diff_context = match diff_outcome {
        GitStagedDiffOutcome::Ready(context) => context,
        GitStagedDiffOutcome::Empty => return TerminalGitAiCommitMessageOutcome::EmptyStagedDiff,
        GitStagedDiffOutcome::NotRepository => {
            return TerminalGitAiCommitMessageOutcome::NotRepository;
        }
        GitStagedDiffOutcome::GitUnavailable => {
            return TerminalGitAiCommitMessageOutcome::GitUnavailable;
        }
        GitStagedDiffOutcome::CwdUnavailable => {
            return TerminalGitAiCommitMessageOutcome::CwdUnavailable;
        }
        GitStagedDiffOutcome::Error(error) => {
            return TerminalGitAiCommitMessageOutcome::Error(error.message().to_string());
        }
    };

    if let Some(provider_id) = provider_id {
        let key_result =
            tokio::task::spawn_blocking(move || key_store.get_provider_key(&provider_id))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
        match key_result {
            Ok(api_key) => {
                let has_key = api_key.as_ref().is_some_and(|key| !key.trim().is_empty());
                if requires_key && !has_key {
                    return TerminalGitAiCommitMessageOutcome::Error(api_key_not_found);
                }
                // The provider key stays inside the short-lived stream config;
                // it is never stored in UI state, logs, or the generated prompt.
                config.api_key = api_key;
            }
            Err(_) if requires_key => {
                return TerminalGitAiCommitMessageOutcome::Error(failed_to_get_key);
            }
            Err(_) => {}
        }
    }

    let messages = terminal_git_ai_commit_messages(terminal_git_ai_diff_context(
        &diff_context,
        max_context_chars,
    ));
    let (stream_tx, mut stream_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(stream_chat_completion(
        config,
        oxideterm_ai::sanitize_api_messages_for_provider(messages),
        stream_tx,
    ));

    let mut generated = String::new();
    while let Some(event) = stream_rx.recv().await {
        match event {
            AiStreamEvent::Content(chunk) => generated.push_str(&chunk),
            AiStreamEvent::Done => {
                return TerminalGitAiCommitMessageOutcome::Ready(generated);
            }
            AiStreamEvent::Error(message) => {
                return TerminalGitAiCommitMessageOutcome::Error(message);
            }
            AiStreamEvent::Thinking(_)
            | AiStreamEvent::ToolCall { .. }
            | AiStreamEvent::ToolCallComplete { .. } => {}
        }
    }

    TerminalGitAiCommitMessageOutcome::Error("AI commit message generation stopped".to_string())
}

async fn run_local_git_operation_probe(cwd: &str) -> Result<GitCommandOutput, LocalGitProbeError> {
    let git_dir = run_local_git_command(cwd, git_absolute_git_dir_args()).await?;
    if !git_dir.success {
        return Ok(GitCommandOutput::failure(""));
    }
    let Some(git_dir) = git_dir
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    else {
        return Ok(GitCommandOutput::success(""));
    };
    // Local operation detection reads only Git's own control files. The command
    // action still runs visibly in the terminal; this probe only chooses the
    // correct continue/abort/skip verb for the active operation type.
    let operation = terminal_git_operation_kind_from_git_dir(std::path::Path::new(git_dir))
        .map(GitOperationKind::as_str)
        .unwrap_or("");
    Ok(GitCommandOutput::success(operation))
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

fn terminal_git_checkout_command(branch: &str) -> String {
    format!("git checkout {}", shell_quote(branch))
}

fn terminal_git_rebase_command(branch: &str) -> String {
    format!("git rebase {}", shell_quote(branch))
}

fn terminal_git_accepts_single_arg(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

fn terminal_git_operation_kind_from_git_dir(git_dir: &std::path::Path) -> Option<GitOperationKind> {
    if git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir() {
        Some(GitOperationKind::Rebase)
    } else if git_dir.join("MERGE_HEAD").is_file() {
        Some(GitOperationKind::Merge)
    } else if git_dir.join("CHERRY_PICK_HEAD").is_file() {
        Some(GitOperationKind::CherryPick)
    } else if git_dir.join("REVERT_HEAD").is_file() {
        Some(GitOperationKind::Revert)
    } else {
        None
    }
}

fn terminal_git_operation_command(operation: GitOperationKind, verb: &str) -> &'static str {
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
        // Merge has no skip verb. The UI hides this action for merge operations;
        // keep a harmless status command as a defensive fallback.
        (GitOperationKind::Merge, "skip") | (_, _) => "git status --short --branch",
    }
}

fn terminal_git_ai_diff_context(context: &GitStagedDiffContext, max_chars: usize) -> String {
    let mut prompt_context = String::new();
    prompt_context.push_str("### git diff --cached --stat\n");
    prompt_context.push_str(context.stat());
    prompt_context.push_str("\n\n### git diff --cached --patch\n");
    prompt_context.push_str(context.patch());

    // Diff content crosses the AI boundary here. Redact credential-like values
    // before truncation so no preserved prefix can keep a raw secret.
    terminal_git_truncate_ai_context(oxideterm_ai::sanitize_for_ai(&prompt_context), max_chars)
}

fn terminal_git_ai_commit_messages(diff_context: String) -> Vec<AiChatMessage> {
    vec![
        terminal_git_ai_chat_message(
            "terminal-git-commit-system",
            AiChatRole::System,
            "You are OxideTerm's Git commit message assistant. Generate exactly one single-line Git commit subject for the staged changes. Prefer Conventional Commit style when it naturally fits. Use imperative present tense. Do not include markdown, quotes, bullets, explanations, or a git command. Keep it concise.",
        ),
        terminal_git_ai_chat_message(
            "terminal-git-commit-user",
            AiChatRole::User,
            format!(
                "Generate a commit subject for these staged changes:\n\n{}",
                diff_context
            ),
        ),
    ]
}

fn terminal_git_ai_chat_message(
    id: &'static str,
    role: AiChatRole,
    content: impl Into<String>,
) -> AiChatMessage {
    AiChatMessage {
        id: id.to_string(),
        role,
        content: content.into(),
        timestamp_ms: 0,
        model: None,
        context: None,
        thinking_content: None,
        is_streaming: false,
        metadata: None,
        tool_call_id: None,
        tool_calls: Vec::new(),
        turn: None,
        transcript_ref: None,
        summary_ref: None,
        branches: None,
        suggestions: Vec::new(),
    }
}

fn terminal_git_truncate_ai_context(mut context: String, max_chars: usize) -> String {
    if max_chars == 0 || context.chars().count() <= max_chars {
        return context;
    }
    let keep_until = context
        .char_indices()
        .nth(max_chars.saturating_sub(1))
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(context.len());
    context.truncate(keep_until);
    context.push_str("\n\n[OxideTerm truncated the staged diff before sending it to the model.]");
    context
}

fn terminal_git_commit_command_from_ai_message(text: &str) -> Option<String> {
    let subject = terminal_git_clean_ai_commit_subject(text)?;
    Some(format!("git commit -m {}", shell_quote(&subject)))
}

fn terminal_git_clean_ai_commit_subject(text: &str) -> Option<String> {
    let mut subject = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("```"))
        .next()?;

    if let Some(rest) = subject.strip_prefix("- ") {
        subject = rest.trim();
    }
    if let Some(rest) = subject.strip_prefix("$ ") {
        subject = rest.trim();
    }
    if let Some(rest) = subject.strip_prefix("git commit -m ") {
        subject = rest.trim();
    }

    let mut cleaned = subject
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`'))
        .trim()
        .to_string();
    if cleaned.is_empty() || cleaned.chars().any(char::is_control) {
        return None;
    }
    if cleaned.chars().count() > TERMINAL_GIT_COMMIT_SUBJECT_MAX_CHARS {
        cleaned = cleaned
            .chars()
            .take(TERMINAL_GIT_COMMIT_SUBJECT_MAX_CHARS)
            .collect();
    }
    Some(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_action_plan_uses_worktree_cd_when_available() {
        let branch =
            GitBranchReference::with_worktree_path("main", false, Some("/tmp/Oxide Term")).unwrap();

        assert_eq!(
            TerminalGitActionPlan::select_branch(&branch)
                .unwrap()
                .command,
            "cd '/tmp/Oxide Term'"
        );
    }

    #[test]
    fn branch_action_plan_quotes_checkout_branch() {
        let branch = GitBranchReference::new("feature/it's-ok", false).unwrap();

        assert_eq!(
            TerminalGitActionPlan::select_branch(&branch)
                .unwrap()
                .command,
            "git checkout 'feature/it'\\''s-ok'"
        );
    }

    #[test]
    fn repository_action_plan_keeps_mutations_visible_and_conservative() {
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Fetch).command,
            "git fetch --prune"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Pull).command,
            "git pull --ff-only"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Push).command,
            "git push"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Status).command,
            "git status --short --branch"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Diff).command,
            "git diff --stat"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Log).command,
            "git log --oneline --decorate --graph -20"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Stash).command,
            "git stash push"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::StashPop).command,
            "git stash pop"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::StageAll).command,
            "git add -A"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Commit).command,
            "git commit"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::RebasePull)
                .command,
            "git pull --rebase"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Continue(
                GitOperationKind::Rebase,
            ))
            .command,
            "git rebase --continue"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Abort(
                GitOperationKind::CherryPick,
            ))
            .command,
            "git cherry-pick --abort"
        );
        assert_eq!(
            TerminalGitActionPlan::repository_action(TerminalGitRepositoryAction::Skip(
                GitOperationKind::Revert,
            ))
            .command,
            "git revert --skip"
        );
    }

    #[test]
    fn checkout_name_plan_quotes_remote_or_unlisted_branch_names() {
        assert_eq!(
            TerminalGitActionPlan::checkout_name("origin/feature/it's-ok")
                .unwrap()
                .command,
            "git checkout 'origin/feature/it'\\''s-ok'"
        );
    }

    #[test]
    fn checkout_name_plan_rejects_control_characters() {
        assert!(TerminalGitActionPlan::checkout_name("feature\nbad").is_none());
    }

    #[test]
    fn rebase_name_plan_quotes_target_branch() {
        assert_eq!(
            TerminalGitActionPlan::rebase_onto_name("main branch")
                .unwrap()
                .command,
            "git rebase 'main branch'"
        );
    }

    #[test]
    fn operation_commands_match_detected_git_operation() {
        assert_eq!(
            terminal_git_operation_command(GitOperationKind::Rebase, "continue"),
            "git rebase --continue"
        );
        assert_eq!(
            terminal_git_operation_command(GitOperationKind::Merge, "abort"),
            "git merge --abort"
        );
        assert_eq!(
            terminal_git_operation_command(GitOperationKind::CherryPick, "skip"),
            "git cherry-pick --skip"
        );
    }

    #[test]
    fn ai_commit_message_becomes_editable_commit_command() {
        assert_eq!(
            terminal_git_commit_command_from_ai_message("feat: add terminal git actions")
                .as_deref(),
            Some("git commit -m 'feat: add terminal git actions'")
        );
        assert_eq!(
            terminal_git_commit_command_from_ai_message(
                "git commit -m \"fix: quote branch names\""
            )
            .as_deref(),
            Some("git commit -m 'fix: quote branch names'")
        );
    }

    #[test]
    fn ai_commit_message_rejects_empty_or_control_output() {
        assert!(terminal_git_commit_command_from_ai_message("```").is_none());
        assert!(terminal_git_commit_command_from_ai_message("feat: bad\nname\u{7}").is_some());
        assert!(terminal_git_commit_command_from_ai_message("feat: bad\u{7}name").is_none());
    }

    #[test]
    fn ai_diff_context_is_sanitized_before_truncation() {
        let context = GitStagedDiffContext::new(
            " secrets.txt | 1 +\n",
            "+OPENAI_API_KEY=sk-test-secret\n+safe line\n",
        )
        .unwrap();
        let prompt = terminal_git_ai_diff_context(&context, 200);
        assert!(!prompt.contains("sk-test-secret"));
        assert!(prompt.contains("[REDACTED]"));
    }
}
