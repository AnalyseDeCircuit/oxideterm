// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::VecDeque, ops::Range, time::Duration};

use gpui::{Context, EventEmitter, Task, Timer};
use oxideterm_connections::{SaveConnectionRequest, SecretString};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_ssh::{
    HostKeyStatus, KeyboardInteractivePromptRequest, KeyboardInteractiveResponses, SshPromptError,
    UpstreamProxyConfig,
};
use tokio::sync::oneshot;

use super::{
    ConnectionFormState, HostKeyChallenge, KeyboardInteractiveChallenge,
    NativeSessionTreeConnectAction, NativeSessionTreeConnectPlan, NewConnectionField,
    SshConnectionIntent, SshConnectionWorkerResult, form_state::clear_connection_selection,
};
use crate::workspace::delivery;

/// Owns one proxy-chain connection attempt without duplicating authentication material.
pub(in crate::workspace) struct NativeProxyConnectRun {
    pub(in crate::workspace) generation: u64,
    pub(in crate::workspace) plan: NativeSessionTreeConnectPlan,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) intent: SshConnectionIntent,
    pub(in crate::workspace) save_after_open: Option<SaveConnectionRequest>,
    pub(in crate::workspace) upstream_proxy: Option<UpstreamProxyConfig>,
}

/// Moves the one upstream-proxy value through a single preflight worker.
pub(in crate::workspace) struct ProxyConnectPreflightContext {
    pub(in crate::workspace) generation: u64,
    pub(in crate::workspace) step_index: usize,
    pub(in crate::workspace) upstream_proxy: Option<UpstreamProxyConfig>,
}

/// Contains only non-secret values needed to render the host-key dialog.
pub(in crate::workspace) struct HostKeyDialogSnapshot {
    pub(in crate::workspace) visible: bool,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: u16,
    pub(in crate::workspace) status: HostKeyStatus,
}

/// Owns connection-flow state that must survive independently of root rendering.
pub(in crate::workspace) struct ConnectionFlowEntity {
    pub(in crate::workspace) form: ConnectionFormState,
    ssh_worker_tx: delivery::ActiveDeliverySender<SshConnectionWorkerResult>,
    ssh_worker_rx: std::sync::mpsc::Receiver<SshConnectionWorkerResult>,
    ssh_worker_results: VecDeque<SshConnectionWorkerResult>,
    active_proxy_connect_run: Option<NativeProxyConnectRun>,
    cancelled_proxy_connect_runs: VecDeque<NativeProxyConnectRun>,
    next_proxy_connect_generation: u64,
    path_picker_task: Option<Task<()>>,
    password_load_task: Option<Task<()>>,
    connection_form_exit_task: Option<Task<()>>,
    jump_server_exit_task: Option<Task<()>>,
    host_key_challenge: Option<HostKeyChallenge>,
    host_key_exit_task: Option<Task<()>>,
    keyboard_interactive_challenge: Option<KeyboardInteractiveChallenge>,
    keyboard_interactive_timer_generation: u64,
    keyboard_interactive_timer_task: Option<Task<()>>,
    keyboard_interactive_exit_task: Option<Task<()>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ConnectionFlowEvent {
    ConnectionFormClosed,
    WorkerResultsReady,
}

impl EventEmitter<ConnectionFlowEvent> for ConnectionFlowEntity {}

pub(in crate::workspace) enum KeyboardInteractiveKeyAction {
    NotHandled,
    Handled,
    Paste,
    Submit,
    Cancel,
}

pub(in crate::workspace) enum KeyboardInteractiveSubmitResult {
    Missing,
    Blocked,
    Submitted,
}

impl ConnectionFlowEntity {
    pub(in crate::workspace) fn new(cx: &mut Context<Self>) -> Self {
        let delivery_wake = delivery::ActiveDeliveryWake::default();
        let (ssh_worker_tx, ssh_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // External SSH tasks may finish after the window owner is released.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |connection_flow, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = connection_flow
                        .update(cx, |connection_flow, cx| {
                            connection_flow.drain_worker_results(cx)
                        })
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();

        Self {
            form: ConnectionFormState::new(),
            ssh_worker_tx,
            ssh_worker_rx,
            ssh_worker_results: VecDeque::new(),
            active_proxy_connect_run: None,
            cancelled_proxy_connect_runs: VecDeque::new(),
            next_proxy_connect_generation: 0,
            path_picker_task: None,
            password_load_task: None,
            connection_form_exit_task: None,
            jump_server_exit_task: None,
            host_key_challenge: None,
            host_key_exit_task: None,
            keyboard_interactive_challenge: None,
            keyboard_interactive_timer_generation: 0,
            keyboard_interactive_timer_task: None,
            keyboard_interactive_exit_task: None,
        }
    }

    pub(in crate::workspace) fn ssh_worker_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<SshConnectionWorkerResult> {
        // Worker tasks receive only a shallow delivery endpoint.
        self.ssh_worker_tx.clone()
    }

    pub(in crate::workspace) fn take_worker_results(
        &mut self,
    ) -> VecDeque<SshConnectionWorkerResult> {
        std::mem::take(&mut self.ssh_worker_results)
    }

    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.ssh_worker_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        if !delivery_batch.items.is_empty() {
            self.ssh_worker_results.extend(delivery_batch.items);
            cx.emit(ConnectionFlowEvent::WorkerResultsReady);
        }
        delivery_batch.outcome.backlog_remaining
    }

    pub(in crate::workspace) fn has_active_proxy_connect_run(&self) -> bool {
        self.active_proxy_connect_run.is_some()
    }

    pub(in crate::workspace) fn start_proxy_connect_run(
        &mut self,
        mut run: NativeProxyConnectRun,
        cx: &mut Context<Self>,
    ) -> Result<(), NativeProxyConnectRun> {
        if self.active_proxy_connect_run.is_some() {
            return Err(run);
        }
        self.next_proxy_connect_generation = self.next_proxy_connect_generation.wrapping_add(1);
        run.generation = self.next_proxy_connect_generation;
        self.active_proxy_connect_run = Some(run);
        cx.notify();
        Ok(())
    }

    pub(in crate::workspace) fn proxy_connect_next_action(
        &self,
    ) -> Option<NativeSessionTreeConnectAction> {
        self.active_proxy_connect_run
            .as_ref()
            .map(|run| run.plan.next_action())
    }

    pub(in crate::workspace) fn take_proxy_connect_preflight_context(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<ProxyConnectPreflightContext> {
        let run = self.active_proxy_connect_run.as_mut()?;
        let context = ProxyConnectPreflightContext {
            generation: run.generation,
            step_index: run.plan.current_index,
            upstream_proxy: run.upstream_proxy.take(),
        };
        cx.notify();
        Some(context)
    }

    pub(in crate::workspace) fn restore_proxy_connect_preflight_context(
        &mut self,
        context: ProxyConnectPreflightContext,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return false;
        };
        if run.generation != context.generation || run.plan.current_index != context.step_index {
            return false;
        }
        run.upstream_proxy = context.upstream_proxy;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn open_active_proxy_host_key_challenge(
        &mut self,
        status: HostKeyStatus,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(run) = self.active_proxy_connect_run.as_ref() else {
            return false;
        };
        let Some(step) = run.plan.steps.get(run.plan.current_index) else {
            return false;
        };
        let challenge = HostKeyChallenge {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            config: oxideterm_ssh::SshConfig::default(),
            title: run.title.clone(),
            status,
            intent: run.intent.clone(),
            session_tree_challenge: true,
            host: step.host.clone(),
            port: step.port,
        };
        self.open_host_key_challenge(challenge, cx);
        true
    }

    pub(in crate::workspace) fn mark_current_proxy_connect_step_verified(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return Ok(());
        };
        let result = run.plan.mark_current_preflight_verified();
        cx.notify();
        result
    }

    pub(in crate::workspace) fn accept_active_proxy_connect_host_key(
        &mut self,
        persist: bool,
        fingerprint: String,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return Ok(());
        };
        let result = run.plan.accept_current_host_key(persist, fingerprint);
        cx.notify();
        result
    }

    pub(in crate::workspace) fn active_proxy_connect_waits_for_node(
        &self,
        node_id: &oxideterm_ssh::NodeId,
    ) -> bool {
        self.active_proxy_connect_run
            .as_ref()
            .and_then(|run| run.plan.steps.get(run.plan.current_index))
            .is_some_and(|step| &step.node_id == node_id)
    }

    pub(in crate::workspace) fn advance_active_proxy_connect_after_node_connected(
        &mut self,
        node_id: &oxideterm_ssh::NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.active_proxy_connect_waits_for_node(node_id) {
            return false;
        }
        if let Some(run) = self.active_proxy_connect_run.as_mut() {
            run.plan.advance_after_connected_step();
            cx.notify();
        }
        true
    }

    pub(in crate::workspace) fn take_active_proxy_upstream_proxy_for_node(
        &mut self,
        node_id: &oxideterm_ssh::NodeId,
        cx: &mut Context<Self>,
    ) -> Option<UpstreamProxyConfig> {
        if !self.active_proxy_connect_waits_for_node(node_id) {
            return None;
        }
        let upstream_proxy = self
            .active_proxy_connect_run
            .as_mut()
            .and_then(|run| run.upstream_proxy.take());
        if upstream_proxy.is_some() {
            cx.notify();
        }
        upstream_proxy
    }

    pub(in crate::workspace) fn take_active_proxy_connect_run(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<NativeProxyConnectRun> {
        let run = self.active_proxy_connect_run.take();
        if run.is_some() {
            cx.notify();
        }
        run
    }

    pub(in crate::workspace) fn take_cancelled_proxy_connect_runs(
        &mut self,
    ) -> VecDeque<NativeProxyConnectRun> {
        std::mem::take(&mut self.cancelled_proxy_connect_runs)
    }

    pub(in crate::workspace) fn begin_connection_form_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.form.presence.begin_exit() else {
            return false;
        };
        self.form.close_select();
        self.connection_form_exit_task = None;
        if delay.is_zero() {
            self.finish_connection_form_exit(generation, cx);
            return true;
        }
        self.connection_form_exit_task = Some(cx.spawn(async move |connection_flow, cx| {
            Timer::after(delay).await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.finish_connection_form_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    pub(in crate::workspace) fn set_form_feedback(
        &mut self,
        pending: Option<bool>,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.form.form.as_mut() else {
            return false;
        };
        if let Some(pending) = pending {
            form.pending = pending;
        }
        form.error = error;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn start_path_picker(
        &mut self,
        field: NewConnectionField,
        selection: impl std::future::Future<Output = Option<String>> + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.path_picker_task.is_some() {
            return false;
        }
        // Retain picker completion with the form owner so it cannot update a released root.
        self.path_picker_task = Some(cx.spawn(async move |connection_flow, cx| {
            let selected_path = selection.await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.path_picker_task = None;
                let Some(path) = selected_path else {
                    return;
                };
                let Some(form) = connection_flow.form.form.as_mut() else {
                    return;
                };
                match field {
                    NewConnectionField::KeyPath => form.key_path = path,
                    NewConnectionField::CertPath => form.cert_path = path,
                    NewConnectionField::JumpKeyPath => {
                        let Some(jump_form) = form.jump_server_form.as_mut() else {
                            return;
                        };
                        jump_form.key_path = path;
                    }
                    NewConnectionField::JumpCertPath => {
                        let Some(jump_form) = form.jump_server_form.as_mut() else {
                            return;
                        };
                        jump_form.cert_path = path;
                    }
                    _ => return,
                }
                form.focused_field = field;
                form.field_focused = true;
                form.error = None;
                clear_connection_selection(form);
                cx.notify();
            });
        }));
        true
    }

    pub(in crate::workspace) fn start_password_load(
        &mut self,
        load: impl std::future::Future<Output = anyhow::Result<SecretString>> + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.password_load_task.is_some() {
            return false;
        }
        // The loaded allocation moves into the UI draft; no plaintext copy is created.
        self.password_load_task = Some(cx.spawn(async move |connection_flow, cx| {
            let result = load.await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.password_load_task = None;
                let Some(form) = connection_flow.form.form.as_mut() else {
                    return;
                };
                if !form.password_loading {
                    return;
                }
                form.password_loading = false;
                match result {
                    Ok(password) => {
                        let mut password = password.into_zeroizing();
                        zeroize::Zeroize::zeroize(&mut form.password);
                        form.password = std::mem::take(&mut *password);
                        form.password_loaded = true;
                        form.password_visible = true;
                        form.password_error = None;
                        form.focused_field = NewConnectionField::Password;
                        form.field_focused = true;
                        clear_connection_selection(form);
                    }
                    Err(error) => {
                        form.password_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
        true
    }

    fn finish_connection_form_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.form.presence.finish_exit(generation) {
            return false;
        }
        self.connection_form_exit_task = None;
        self.jump_server_exit_task = None;
        self.path_picker_task = None;
        self.password_load_task = None;
        if let Some(run) = self.active_proxy_connect_run.take() {
            // Window/runtime cleanup is emitted as a typed effect after state ownership is cleared.
            self.cancelled_proxy_connect_runs.push_back(run);
        }
        self.form.clear();
        self.form.presence.reopen();
        self.clear_host_key_challenge(cx);
        self.cancel_keyboard_interactive_challenge(Duration::ZERO, cx);
        cx.emit(ConnectionFlowEvent::ConnectionFormClosed);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn begin_jump_server_form_exit(
        &mut self,
        commit: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.form.jump_server_presence.begin_exit() else {
            return false;
        };
        self.form.jump_server_exit_commits = commit;
        if let Some(form) = self.form.form.as_mut() {
            form.field_focused = false;
            form.selected_field = None;
        }
        self.jump_server_exit_task = None;
        if delay.is_zero() {
            self.finish_jump_server_form_exit(generation, cx);
            return true;
        }
        self.jump_server_exit_task = Some(cx.spawn(async move |connection_flow, cx| {
            Timer::after(delay).await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.finish_jump_server_form_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_jump_server_form_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.form.jump_server_presence.finish_exit(generation) {
            return false;
        }
        self.jump_server_exit_task = None;
        let commit = std::mem::take(&mut self.form.jump_server_exit_commits);
        if let Some(form) = self.form.form.as_mut() {
            if commit {
                if let Some(jump_server) = form.jump_server_form.take() {
                    form.proxy_hops.push(jump_server);
                    if form.auth_tab == super::SshAuthTab::TwoFactor {
                        form.auth_tab = super::SshAuthTab::Password;
                        form.focused_field = super::NewConnectionField::Password;
                    }
                    form.proxy_chain_expanded = true;
                    form.field_focused = false;
                    form.selected_field = None;
                    form.error = None;
                }
            } else {
                form.jump_server_form = None;
            }
        }
        self.form.jump_server_presence.reopen();
        cx.notify();
        true
    }

    pub(in crate::workspace) fn has_host_key_challenge(&self) -> bool {
        self.host_key_challenge.is_some()
    }

    pub(in crate::workspace) fn host_key_dialog_snapshot(&self) -> Option<HostKeyDialogSnapshot> {
        let challenge = self.host_key_challenge.as_ref()?;
        Some(HostKeyDialogSnapshot {
            visible: challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible,
            host: challenge.host.clone(),
            port: challenge.port,
            status: challenge.status.clone(),
        })
    }

    pub(in crate::workspace) fn open_host_key_challenge(
        &mut self,
        challenge: HostKeyChallenge,
        cx: &mut Context<Self>,
    ) {
        // Replacing a challenge drops the previous config without duplicating its auth material.
        self.host_key_exit_task = None;
        self.host_key_challenge = Some(challenge);
        cx.notify();
    }

    pub(in crate::workspace) fn clear_host_key_challenge(&mut self, cx: &mut Context<Self>) {
        self.host_key_exit_task = None;
        if self.host_key_challenge.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn take_host_key_challenge(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<HostKeyChallenge> {
        self.host_key_exit_task = None;
        let challenge = self.host_key_challenge.take();
        if challenge.is_some() {
            cx.notify();
        }
        challenge
    }

    pub(in crate::workspace) fn restore_host_key_challenge(
        &mut self,
        challenge: HostKeyChallenge,
        cx: &mut Context<Self>,
    ) {
        self.host_key_exit_task = None;
        self.host_key_challenge = Some(challenge);
        cx.notify();
    }

    pub(in crate::workspace) fn begin_host_key_challenge_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.host_key_challenge.as_mut() else {
            return false;
        };
        let Some(generation) = challenge.presence.begin_exit() else {
            return false;
        };
        self.host_key_exit_task = None;
        if delay.is_zero() {
            self.finish_host_key_challenge_exit(generation, cx);
            return true;
        }
        self.host_key_exit_task = Some(cx.spawn(async move |connection_flow, cx| {
            Timer::after(delay).await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.finish_host_key_challenge_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_host_key_challenge_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self
            .host_key_challenge
            .as_ref()
            .is_some_and(|challenge| challenge.presence.finish_exit(generation))
        {
            return false;
        }
        self.host_key_challenge = None;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn has_keyboard_interactive_challenge(&self) -> bool {
        self.keyboard_interactive_challenge.is_some()
    }

    pub(in crate::workspace) fn keyboard_interactive_challenge(
        &self,
    ) -> Option<&KeyboardInteractiveChallenge> {
        self.keyboard_interactive_challenge.as_ref()
    }

    pub(in crate::workspace) fn focused_keyboard_interactive_prompt(&self) -> Option<usize> {
        self.keyboard_interactive_challenge
            .as_ref()
            .map(|challenge| challenge.focused_prompt)
    }

    pub(in crate::workspace) fn keyboard_interactive_response(&self, index: usize) -> Option<&str> {
        // The IME adapter builds a masked platform projection while this
        // Entity remains the only owner of the authentication response.
        self.keyboard_interactive_challenge
            .as_ref()?
            .responses
            .get(index)
            .map(String::as_str)
    }

    pub(in crate::workspace) fn open_keyboard_interactive_challenge(
        &mut self,
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(existing) = self.keyboard_interactive_challenge.as_ref()
            && existing.request.flow_id != request.flow_id
        {
            // Keep the active auth flow as the only owner of the protected dialog.
            let _ = response_tx.send(Err(SshPromptError::Cancelled));
            return false;
        }
        self.keyboard_interactive_timer_task = None;
        self.keyboard_interactive_exit_task = None;
        if let Some(mut existing) = self.keyboard_interactive_challenge.take()
            && let Some(existing_tx) = existing.response_tx.take()
        {
            // Reject the replaced oneshot so no transport waits for stale input.
            let _ = existing_tx.send(Err(SshPromptError::Cancelled));
        }
        self.keyboard_interactive_challenge =
            Some(KeyboardInteractiveChallenge::new(request, response_tx));
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.schedule_keyboard_interactive_timer(self.keyboard_interactive_timer_generation, cx);
        cx.notify();
        true
    }

    fn schedule_keyboard_interactive_timer(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.keyboard_interactive_timer_task = Some(cx.spawn(async move |connection_flow, cx| {
            loop {
                Timer::after(Duration::from_secs(1)).await;
                let keep_ticking = connection_flow
                    .update(cx, |connection_flow, cx| {
                        let Some(challenge) =
                            connection_flow.keyboard_interactive_challenge.as_ref()
                        else {
                            return false;
                        };
                        if connection_flow.keyboard_interactive_timer_generation != generation {
                            return false;
                        }
                        cx.notify();
                        !challenge.timed_out()
                    })
                    .unwrap_or(false);
                if !keep_ticking {
                    break;
                }
            }
        }));
    }

    pub(in crate::workspace) fn handle_keyboard_interactive_key(
        &mut self,
        key: &str,
        shift: bool,
        uses_text_edit_modifier: bool,
        cx: &mut Context<Self>,
    ) -> KeyboardInteractiveKeyAction {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return KeyboardInteractiveKeyAction::NotHandled;
        };
        if challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
            return KeyboardInteractiveKeyAction::Handled;
        }
        if uses_text_edit_modifier {
            return if key == "v" {
                KeyboardInteractiveKeyAction::Paste
            } else {
                KeyboardInteractiveKeyAction::Handled
            };
        }

        match key {
            "escape" => KeyboardInteractiveKeyAction::Cancel,
            "enter" if !challenge.timed_out() && challenge.all_responses_filled() => {
                KeyboardInteractiveKeyAction::Submit
            }
            "tab" => {
                if !challenge.responses.is_empty() {
                    if shift {
                        challenge.focused_prompt = challenge
                            .focused_prompt
                            .saturating_sub(1)
                            .min(challenge.responses.len() - 1);
                    } else {
                        challenge.focused_prompt =
                            (challenge.focused_prompt + 1).min(challenge.responses.len() - 1);
                    }
                }
                cx.notify();
                KeyboardInteractiveKeyAction::Handled
            }
            "backspace" => {
                if !challenge.timed_out()
                    && let Some(response) = challenge.responses.get_mut(challenge.focused_prompt)
                {
                    response.pop();
                    cx.notify();
                }
                KeyboardInteractiveKeyAction::Handled
            }
            _ => KeyboardInteractiveKeyAction::Handled,
        }
    }

    pub(in crate::workspace) fn focus_keyboard_interactive_prompt(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return;
        };
        challenge.focused_prompt = index.min(challenge.responses.len().saturating_sub(1));
        cx.notify();
    }

    pub(in crate::workspace) fn paste_keyboard_interactive_response(
        &mut self,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return false;
        };
        if challenge.timed_out() {
            return false;
        }
        let Some(response) = challenge.responses.get_mut(challenge.focused_prompt) else {
            return false;
        };
        response.push_str(text);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn replace_keyboard_interactive_response(
        &mut self,
        index: usize,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return false;
        };
        if challenge.timed_out() {
            return false;
        }
        let Some(response) = challenge.responses.get_mut(index) else {
            return false;
        };
        replace_utf16(response, replacement_range, text);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn submit_keyboard_interactive_challenge(
        &mut self,
        cx: &mut Context<Self>,
    ) -> KeyboardInteractiveSubmitResult {
        let Some(mut challenge) = self.keyboard_interactive_challenge.take() else {
            return KeyboardInteractiveSubmitResult::Missing;
        };
        if challenge.timed_out() || !challenge.all_responses_filled() {
            self.keyboard_interactive_challenge = Some(challenge);
            cx.notify();
            return KeyboardInteractiveSubmitResult::Blocked;
        }
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.keyboard_interactive_timer_task = None;
        if let Some(response_tx) = challenge.response_tx.take() {
            // Move the Zeroizing response owner directly to the SSH prompt waiter.
            let _ = response_tx.send(Ok(challenge.responses));
        }
        cx.notify();
        KeyboardInteractiveSubmitResult::Submitted
    }

    pub(in crate::workspace) fn cancel_keyboard_interactive_challenge(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return false;
        };
        let Some(generation) = challenge.presence.begin_exit() else {
            return false;
        };
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.keyboard_interactive_timer_task = None;
        self.keyboard_interactive_exit_task = None;
        if let Some(response_tx) = challenge.response_tx.take() {
            let _ = response_tx.send(Err(SshPromptError::Cancelled));
        }
        if delay.is_zero() {
            self.finish_keyboard_interactive_exit(generation, cx);
            return true;
        }
        self.keyboard_interactive_exit_task = Some(cx.spawn(async move |connection_flow, cx| {
            Timer::after(delay).await;
            let _ = connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.finish_keyboard_interactive_exit(generation, cx);
            });
        }));
        cx.notify();
        true
    }

    fn finish_keyboard_interactive_exit(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .keyboard_interactive_challenge
            .as_ref()
            .is_some_and(|challenge| challenge.presence.finish_exit(generation))
        {
            return false;
        }
        // Dropping the retained Zeroizing payload scrubs every secret answer.
        self.keyboard_interactive_challenge = None;
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

    use gpui::{AppContext, TestAppContext};
    use oxideterm_connections::SecretString;
    use oxideterm_ssh::{
        HostKeyStatus, KeyboardInteractivePrompt, KeyboardInteractivePromptRequest, NodeId,
        SshConfig, SshPromptError,
    };
    use tokio::sync::oneshot;

    use super::{ConnectionFlowEntity, ConnectionFlowEvent, NativeProxyConnectRun};
    use crate::workspace::new_connection::{
        HostKeyChallenge, NativeSessionTreeConnectAction, NativeSessionTreeConnectPlan,
        NewConnectionField, NewConnectionForm, NewConnectionProxyHop, SavedConnectionPromptAction,
        SshConnectionIntent, SshConnectionWorkerResult,
    };
    use oxideterm_ssh::NativeSessionTreeConnectStep;

    fn unknown_host_key_challenge() -> HostKeyChallenge {
        HostKeyChallenge {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            config: SshConfig::default(),
            title: "Test".to_string(),
            status: HostKeyStatus::Unknown {
                fingerprint: "SHA256:test".to_string(),
                key_type: "ssh-ed25519".to_string(),
            },
            intent: SshConnectionIntent::Test,
            session_tree_challenge: false,
            host: "example.test".to_string(),
            port: 22,
        }
    }

    fn keyboard_interactive_request(flow_id: &str) -> KeyboardInteractivePromptRequest {
        KeyboardInteractivePromptRequest {
            flow_id: flow_id.to_string(),
            name: "Authentication".to_string(),
            instructions: String::new(),
            prompts: vec![KeyboardInteractivePrompt {
                prompt: "Password".to_string(),
                echo: false,
            }],
            chained: false,
        }
    }

    fn proxy_connect_run() -> NativeProxyConnectRun {
        NativeProxyConnectRun {
            generation: 0,
            plan: NativeSessionTreeConnectPlan {
                target_node_id: NodeId::new("target"),
                cleanup_node_id: Some(NodeId::new("target")),
                steps: vec![NativeSessionTreeConnectStep {
                    node_id: NodeId::new("target"),
                    host: "target.example.test".to_string(),
                    port: 22,
                    trust_host_key: None,
                    expected_host_key_fingerprint: None,
                    preflight_verified: false,
                }],
                current_index: 0,
            },
            title: "Target".to_string(),
            intent: SshConnectionIntent::Connect,
            save_after_open: None,
            upstream_proxy: None,
        }
    }

    #[gpui::test]
    fn ssh_worker_delivery_and_release_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);
        let worker_results_ready = Arc::new(AtomicBool::new(false));
        let event_flag = worker_results_ready.clone();
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &ConnectionFlowEvent, _cx| {
                if *event == ConnectionFlowEvent::WorkerResultsReady {
                    event_flag.store(true, Ordering::Release);
                }
            })
        });
        let sender = cx.read(|cx| entity.read(cx).ssh_worker_sender());
        let wake = sender.wake();

        sender
            .send(SshConnectionWorkerResult::Test { result: Ok(()) })
            .expect("SSH worker delivery");
        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(matches!(
                entity.take_worker_results().front(),
                Some(SshConnectionWorkerResult::Test { result: Ok(()) })
            ));
        });
        assert!(worker_results_ready.load(Ordering::Acquire));

        drop(entity);
        cx.update(|_cx| {});
        assert!(wake.is_stopped());
    }

    #[gpui::test]
    fn form_close_moves_proxy_run_to_typed_cleanup_queue(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);

        entity.update(cx, |entity, cx| {
            entity
                .form
                .replace_with_new_form(NewConnectionForm::default());
            assert!(
                entity
                    .start_proxy_connect_run(proxy_connect_run(), cx)
                    .is_ok()
            );
            assert!(entity.has_active_proxy_connect_run());

            assert!(entity.begin_connection_form_exit(Duration::ZERO, cx));
            assert!(!entity.has_active_proxy_connect_run());
            let cancelled = entity.take_cancelled_proxy_connect_runs();
            assert_eq!(cancelled.len(), 1);
            assert_eq!(cancelled[0].generation, 1);
        });
    }

    #[gpui::test]
    fn proxy_run_transitions_and_host_key_state_stay_inside_entity(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);
        let target_node_id = NodeId::new("target");

        entity.update(cx, |entity, cx| {
            assert!(
                entity
                    .start_proxy_connect_run(proxy_connect_run(), cx)
                    .is_ok()
            );
            assert!(matches!(
                entity.proxy_connect_next_action(),
                Some(NativeSessionTreeConnectAction::Preflight { .. })
            ));

            let context = entity
                .take_proxy_connect_preflight_context(cx)
                .expect("preflight context");
            assert_eq!(context.generation, 1);
            assert_eq!(context.step_index, 0);
            assert!(entity.restore_proxy_connect_preflight_context(context, cx));

            assert!(entity.open_active_proxy_host_key_challenge(
                HostKeyStatus::Unknown {
                    fingerprint: "SHA256:test".to_string(),
                    key_type: "ssh-ed25519".to_string(),
                },
                cx,
            ));
            let challenge = entity
                .take_host_key_challenge(cx)
                .expect("proxy host-key challenge");
            assert!(challenge.session_tree_challenge);

            entity
                .accept_active_proxy_connect_host_key(false, "SHA256:test".to_string(), cx)
                .expect("accepted host key");
            assert!(matches!(
                entity.proxy_connect_next_action(),
                Some(NativeSessionTreeConnectAction::Connect { .. })
            ));
            assert!(entity.advance_active_proxy_connect_after_node_connected(&target_node_id, cx));
            assert!(matches!(
                entity.proxy_connect_next_action(),
                Some(NativeSessionTreeConnectAction::Complete { .. })
            ));
        });
    }

    #[gpui::test]
    fn host_key_state_and_exit_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);

        entity.update(cx, |entity, cx| {
            entity.open_host_key_challenge(unknown_host_key_challenge(), cx);
            let snapshot = entity
                .host_key_dialog_snapshot()
                .expect("host-key render snapshot");
            assert!(snapshot.visible);
            assert_eq!(snapshot.host, "example.test");
            assert!(entity.begin_host_key_challenge_exit(Duration::ZERO, cx));
            assert!(!entity.has_host_key_challenge());
        });
    }

    #[gpui::test]
    fn taking_and_restoring_host_key_challenge_preserves_single_ownership(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);

        entity.update(cx, |entity, cx| {
            entity.open_host_key_challenge(unknown_host_key_challenge(), cx);
            let challenge = entity
                .take_host_key_challenge(cx)
                .expect("owned host-key challenge");
            assert!(!entity.has_host_key_challenge());
            entity.restore_host_key_challenge(challenge, cx);
            assert!(entity.has_host_key_challenge());
        });
    }

    #[gpui::test]
    fn keyboard_interactive_responses_move_once_to_the_prompt_waiter(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);
        let (response_tx, mut response_rx) = oneshot::channel();

        entity.update(cx, |entity, cx| {
            assert!(entity.open_keyboard_interactive_challenge(
                keyboard_interactive_request("flow-a"),
                response_tx,
                cx,
            ));
            assert!(entity.replace_keyboard_interactive_response(0, None, "secret", cx));
            assert!(matches!(
                entity.submit_keyboard_interactive_challenge(cx),
                super::KeyboardInteractiveSubmitResult::Submitted
            ));
            assert!(!entity.has_keyboard_interactive_challenge());
        });

        let responses = response_rx
            .try_recv()
            .expect("prompt response delivery")
            .expect("submitted responses");
        assert_eq!(responses.as_slice(), ["secret"]);
    }

    #[gpui::test]
    fn competing_keyboard_interactive_flow_is_cancelled_without_replacing_owner(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(ConnectionFlowEntity::new);
        let (first_tx, _first_rx) = oneshot::channel();
        let (second_tx, mut second_rx) = oneshot::channel();

        entity.update(cx, |entity, cx| {
            assert!(entity.open_keyboard_interactive_challenge(
                keyboard_interactive_request("flow-a"),
                first_tx,
                cx,
            ));
            assert!(!entity.open_keyboard_interactive_challenge(
                keyboard_interactive_request("flow-b"),
                second_tx,
                cx,
            ));
            assert!(entity.has_keyboard_interactive_challenge());
        });

        assert!(matches!(
            second_rx.try_recv(),
            Ok(Err(SshPromptError::Cancelled))
        ));
    }

    #[gpui::test]
    fn cancelling_keyboard_interactive_challenge_rejects_waiter_and_drops_answers(
        cx: &mut TestAppContext,
    ) {
        let entity = cx.new(ConnectionFlowEntity::new);
        let (response_tx, mut response_rx) = oneshot::channel();

        entity.update(cx, |entity, cx| {
            assert!(entity.open_keyboard_interactive_challenge(
                keyboard_interactive_request("flow-a"),
                response_tx,
                cx,
            ));
            assert!(entity.replace_keyboard_interactive_response(0, None, "secret", cx));
            assert!(entity.cancel_keyboard_interactive_challenge(Duration::ZERO, cx));
            assert!(!entity.has_keyboard_interactive_challenge());
        });

        assert!(matches!(
            response_rx.try_recv(),
            Ok(Err(SshPromptError::Cancelled))
        ));
    }

    #[gpui::test]
    fn connection_form_close_clears_secret_owner_and_mode_metadata(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);

        entity.update(cx, |entity, cx| {
            let mut form = NewConnectionForm::default();
            form.password = "secret".to_string();
            entity.form.replace_with_new_form(form);
            entity.form.editing_saved_connection_id = Some("saved-id".to_string());
            entity.form.saved_connection_prompt_action = Some(SavedConnectionPromptAction::Connect);

            assert!(entity.begin_connection_form_exit(Duration::ZERO, cx));
            assert!(entity.form.form.is_none());
            assert!(entity.form.editing_saved_connection_id.is_none());
            assert!(entity.form.saved_connection_prompt_action.is_none());
        });
    }

    #[gpui::test]
    fn loaded_password_allocation_moves_into_entity_owned_form(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);
        let password = SecretString::from("loaded-secret-marker");
        let password_pointer = password.expose_secret().as_ptr();

        entity.update(cx, |entity, cx| {
            let mut form = NewConnectionForm::default();
            form.password_loading = true;
            entity.form.replace_with_new_form(form);
            assert!(entity.start_password_load(async move { Ok(password) }, cx));
        });
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(entity.password_load_task.is_none());
            let form = entity.form.form.as_ref().expect("retained form");
            assert_eq!(form.password, "loaded-secret-marker");
            assert_eq!(form.password.as_ptr(), password_pointer);
            assert!(!form.password_loading);
            assert!(form.password_loaded);
        });
    }

    #[gpui::test]
    fn closing_form_cancels_entity_owned_picker_and_password_tasks(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);
        let (_path_tx, path_rx) = oneshot::channel::<Option<String>>();
        let (_password_tx, password_rx) = oneshot::channel::<anyhow::Result<SecretString>>();

        entity.update(cx, |entity, cx| {
            let mut form = NewConnectionForm::default();
            form.password_loading = true;
            entity.form.replace_with_new_form(form);
            assert!(entity.start_path_picker(
                NewConnectionField::KeyPath,
                async move { path_rx.await.ok().flatten() },
                cx,
            ));
            assert!(entity.start_password_load(
                async move {
                    password_rx
                        .await
                        .unwrap_or_else(|_| anyhow::bail!("password load cancelled"))
                },
                cx,
            ));
            assert!(entity.path_picker_task.is_some());
            assert!(entity.password_load_task.is_some());

            assert!(entity.begin_connection_form_exit(Duration::ZERO, cx));
            assert!(entity.path_picker_task.is_none());
            assert!(entity.password_load_task.is_none());
            assert!(entity.form.form.is_none());
        });
    }

    #[gpui::test]
    fn jump_server_exit_commits_once_inside_the_connection_entity(cx: &mut TestAppContext) {
        let entity = cx.new(ConnectionFlowEntity::new);

        entity.update(cx, |entity, cx| {
            let mut form = NewConnectionForm::default();
            let mut jump_server = NewConnectionProxyHop::new();
            jump_server.host = "jump.example.test".to_string();
            jump_server.username = "alice".to_string();
            form.jump_server_form = Some(jump_server);
            entity.form.replace_with_new_form(form);

            assert!(entity.begin_jump_server_form_exit(true, Duration::ZERO, cx));
            let form = entity.form.form.as_ref().expect("retained connection form");
            assert!(form.jump_server_form.is_none());
            assert_eq!(form.proxy_hops.len(), 1);
            assert_eq!(form.proxy_hops[0].host, "jump.example.test");
        });
    }
}
