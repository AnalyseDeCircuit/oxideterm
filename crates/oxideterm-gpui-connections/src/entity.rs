// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    ops::Range,
    rc::Rc,
    time::Duration,
};

use super::{
    BleConnectionWorkerResult, ConnectionFormState, HostKeyChallenge, KeyboardInteractiveChallenge,
    NewConnectionField, SshConnectionIntent, SshConnectionWorkerResult,
    form_state::clear_connection_selection, saved_auth::SavedPasswordLoadError,
};
use crate::launch::PendingStandaloneSftpPairLaunch;
use gpui::{Context, EventEmitter, Task, Timer};
use oxideterm_connections::{SaveConnectionRequest, SecretString};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId};
use oxideterm_ssh::{
    HostKeyStatus, KeyboardInteractivePromptRequest, NativeSessionTreeConnectAction,
    NativeSessionTreeConnectPlan, SshConfig, SshPromptError, UpstreamProxyConfig,
};
use oxideterm_workspace::delivery;

/// Owns one proxy-chain connection attempt without duplicating authentication material.
pub struct NativeProxyConnectRun {
    pub generation: u64,
    pub plan: NativeSessionTreeConnectPlan,
    pub title: String,
    pub intent: SshConnectionIntent,
    pub save_after_open: Option<SaveConnectionRequest>,
    pub upstream_proxy: Option<UpstreamProxyConfig>,
}

/// Moves the one upstream-proxy value through a single preflight worker.
pub struct ProxyConnectPreflightContext {
    pub generation: u64,
    pub step_index: usize,
    pub upstream_proxy: Option<UpstreamProxyConfig>,
}

/// Contains only non-secret values needed to render the host-key dialog.
pub struct HostKeyDialogSnapshot {
    pub visible: bool,
    pub host: String,
    pub port: u16,
    pub status: HostKeyStatus,
}

/// Shares layout-only select anchors without routing every prepaint through WorkspaceApp.
#[derive(Clone, Default)]
pub struct ConnectionSelectAnchorStore {
    anchors: Rc<RefCell<HashMap<SelectAnchorId, OverlayAnchor>>>,
}

impl ConnectionSelectAnchorStore {
    pub fn get(&self, id: SelectAnchorId) -> Option<OverlayAnchor> {
        self.anchors.borrow().get(&id).copied()
    }

    pub fn update(&self, anchor: OverlayAnchor) -> bool {
        if self.get(anchor.id) == Some(anchor) {
            return false;
        }
        self.anchors.borrow_mut().insert(anchor.id, anchor);
        true
    }

    pub fn clear(&self) {
        self.anchors.borrow_mut().clear();
    }
}

/// Owns connection-flow state that must survive independently of root rendering.
pub struct ConnectionFlowEntity {
    pub form: ConnectionFormState,
    select_anchors: ConnectionSelectAnchorStore,
    ssh_worker_tx: delivery::ActiveDeliverySender<SshConnectionWorkerResult>,
    ssh_worker_rx: std::sync::mpsc::Receiver<SshConnectionWorkerResult>,
    ssh_worker_results: VecDeque<SshConnectionWorkerResult>,
    ble_worker_tx: delivery::ActiveDeliverySender<BleConnectionWorkerResult>,
    ble_worker_rx: std::sync::mpsc::Receiver<BleConnectionWorkerResult>,
    ble_worker_results: VecDeque<BleConnectionWorkerResult>,
    active_proxy_connect_run: Option<NativeProxyConnectRun>,
    cancelled_proxy_connect_runs: VecDeque<NativeProxyConnectRun>,
    pending_sftp_pair_launches: HashMap<String, PendingStandaloneSftpPairLaunch>,
    next_proxy_connect_generation: u64,
    path_picker_task: Option<Task<()>>,
    password_load_task: Option<Task<()>>,
    next_password_load_id: u64,
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
pub enum ConnectionFlowEvent {
    ConnectionFormClosed,
    WorkerResultsReady,
    ModalBoundaryChanged,
}

impl EventEmitter<ConnectionFlowEvent> for ConnectionFlowEntity {}

pub(super) struct AuthenticationChanged;

impl EventEmitter<AuthenticationChanged> for ConnectionFlowEntity {}

pub enum KeyboardInteractiveKeyAction {
    NotHandled,
    Handled,
    Paste,
    Submit,
    Cancel,
}

pub enum KeyboardInteractiveSubmitResult {
    Missing,
    Blocked,
    Submitted,
}

impl ConnectionFlowEntity {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let delivery_wake = delivery::ActiveDeliveryWake::default();
        let (ssh_worker_tx, ssh_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let (ble_worker_tx, ble_worker_rx) =
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
            form: ConnectionFormState::new(cx.focus_handle()),
            select_anchors: ConnectionSelectAnchorStore::default(),
            ssh_worker_tx,
            ssh_worker_rx,
            ssh_worker_results: VecDeque::new(),
            ble_worker_tx,
            ble_worker_rx,
            ble_worker_results: VecDeque::new(),
            active_proxy_connect_run: None,
            cancelled_proxy_connect_runs: VecDeque::new(),
            pending_sftp_pair_launches: HashMap::new(),
            next_proxy_connect_generation: 0,
            path_picker_task: None,
            password_load_task: None,
            next_password_load_id: 0,
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

    pub fn ssh_worker_sender(&self) -> delivery::ActiveDeliverySender<SshConnectionWorkerResult> {
        // Worker tasks receive only a shallow delivery endpoint.
        self.ssh_worker_tx.clone()
    }

    pub fn ble_worker_sender(&self) -> delivery::ActiveDeliverySender<BleConnectionWorkerResult> {
        self.ble_worker_tx.clone()
    }

    pub fn select_anchor_store(&self) -> ConnectionSelectAnchorStore {
        self.select_anchors.clone()
    }

    pub fn take_worker_results(&mut self) -> VecDeque<SshConnectionWorkerResult> {
        std::mem::take(&mut self.ssh_worker_results)
    }

    pub fn take_ble_worker_results(&mut self) -> VecDeque<BleConnectionWorkerResult> {
        std::mem::take(&mut self.ble_worker_results)
    }

    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let ssh_batch =
            delivery::drain_channel(&self.ssh_worker_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let ble_batch =
            delivery::drain_channel(&self.ble_worker_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let had_results = !ssh_batch.items.is_empty() || !ble_batch.items.is_empty();
        if !ssh_batch.items.is_empty() {
            self.ssh_worker_results.extend(ssh_batch.items);
        }
        if !ble_batch.items.is_empty() {
            self.ble_worker_results.extend(ble_batch.items);
        }
        if had_results {
            cx.emit(ConnectionFlowEvent::WorkerResultsReady);
        }
        ssh_batch.outcome.backlog_remaining || ble_batch.outcome.backlog_remaining
    }

    pub fn has_active_proxy_connect_run(&self) -> bool {
        self.active_proxy_connect_run.is_some()
    }

    pub fn start_proxy_connect_run(
        &mut self,
        mut run: NativeProxyConnectRun,
        cx: &mut Context<Self>,
    ) -> Result<(), Box<NativeProxyConnectRun>> {
        if self.active_proxy_connect_run.is_some() {
            return Err(Box::new(run));
        }
        self.next_proxy_connect_generation = self.next_proxy_connect_generation.wrapping_add(1);
        run.generation = self.next_proxy_connect_generation;
        self.active_proxy_connect_run = Some(run);
        cx.notify();
        Ok(())
    }

    pub fn proxy_connect_next_action(&self) -> Option<NativeSessionTreeConnectAction> {
        self.active_proxy_connect_run
            .as_ref()
            .map(|run| run.plan.next_action())
    }

    pub fn take_proxy_connect_preflight_context(
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

    pub fn restore_proxy_connect_preflight_context(
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

    pub fn open_active_proxy_host_key_challenge(
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

    pub fn mark_current_proxy_connect_step_verified(
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

    pub fn accept_active_proxy_connect_host_key(
        &mut self,
        persist: bool,
        fingerprint: String,
        replacement: Option<oxideterm_ssh::HostKeyReplacement>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return Ok(());
        };
        let result = run.plan.accept_current_host_key(persist, fingerprint);
        if result.is_ok() {
            run.plan.steps[run.plan.current_index].host_key_replacement = replacement;
        }
        cx.notify();
        result
    }

    pub fn active_proxy_connect_waits_for_node(&self, node_id: &oxideterm_ssh::NodeId) -> bool {
        self.active_proxy_connect_run
            .as_ref()
            .and_then(|run| run.plan.steps.get(run.plan.current_index))
            .is_some_and(|step| &step.node_id == node_id)
    }

    pub fn advance_active_proxy_connect_after_node_connected(
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

    pub fn take_active_proxy_upstream_proxy_for_node(
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

    pub fn take_active_proxy_connect_run(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<NativeProxyConnectRun> {
        let run = self.active_proxy_connect_run.take();
        if run.is_some() {
            cx.notify();
        }
        run
    }

    pub fn take_cancelled_proxy_connect_runs(&mut self) -> VecDeque<NativeProxyConnectRun> {
        std::mem::take(&mut self.cancelled_proxy_connect_runs)
    }

    pub(crate) fn ensure_kerberos_credentials_availability(
        &mut self,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.form.form.as_mut() else {
            return;
        };
        let kerberos_enabled = form.gssapi_enabled
            || form.standalone_sftp_secondary.gssapi_enabled
            || form
                .jump_server_form
                .as_ref()
                .is_some_and(|hop| hop.gssapi_enabled);
        if !kerberos_enabled
            || form.gssapi_credentials_available.is_some()
            || form.gssapi_credentials_check_pending
        {
            return;
        }
        form.gssapi_credentials_check_pending = true;
        let instance_id = form.ble.instance_id;
        cx.spawn(async move |flow, cx| {
            let available = runtime
                .spawn_blocking(oxideterm_ssh::kerberos_credentials_available)
                .await
                .unwrap_or(false);
            let _ = flow.update(cx, |flow, cx| {
                if let Some(form) = flow.form.form.as_mut()
                    && form.ble.instance_id == instance_id
                {
                    form.gssapi_credentials_check_pending = false;
                    form.gssapi_credentials_available = Some(available);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn form_input_value(&self, field: NewConnectionField) -> Option<&str> {
        crate::input::field_value(self.form.form.as_ref()?, field)
    }

    pub fn replace_form_input(
        &mut self,
        field: NewConnectionField,
        replacement_range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !crate::input::replace_text(&mut self.form, field, replacement_range, text) {
            return false;
        }
        cx.notify();
        true
    }

    pub fn stage_sftp_pair_launch(&mut self, launch: PendingStandaloneSftpPairLaunch) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        self.pending_sftp_pair_launches
            .insert(token.clone(), launch);
        token
    }

    pub fn accept_sftp_pair_primary(
        &mut self,
        token: &str,
        config: SshConfig,
    ) -> Option<SshConfig> {
        let launch = self.pending_sftp_pair_launches.get_mut(token)?;
        launch.primary_config = Some(config);
        launch.secondary_config.take()
    }

    pub fn take_sftp_pair_launch(
        &mut self,
        token: &str,
    ) -> Option<PendingStandaloneSftpPairLaunch> {
        self.pending_sftp_pair_launches.remove(token)
    }

    pub fn begin_connection_form_exit(&mut self, delay: Duration, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self.form.presence.begin_exit() else {
            return false;
        };
        // Pending endpoint configs still own credentials until the SFTP runtime adopts them.
        self.pending_sftp_pair_launches.clear();
        self.form.close_select();
        self.password_load_task = None;
        if let Some(form) = self.form.form.as_mut() {
            form.password_load_id = None;
            if form.password_from_store {
                zeroize::Zeroize::zeroize(&mut form.password);
                form.password_from_store = false;
                form.password_loaded = false;
                form.password_visible = false;
            }
        }
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

    pub fn replace_form(
        &mut self,
        form: crate::form_state::NewConnectionForm,
        cx: &mut Context<Self>,
    ) {
        self.form.replace_with_new_form(form);
        cx.notify();
    }

    pub fn focus_form_input(&mut self, field: crate::NewConnectionField, cx: &mut Context<Self>) {
        if let Some(form) = self.form.form.as_mut() {
            form.field_focused = true;
            form.focused_field = field;
            crate::form_state::clear_connection_selection(form);
        }
        self.form.close_select();
        cx.notify();
    }

    pub(crate) fn clear_form_after_submit(&mut self, cx: &mut Context<Self>) {
        let was_open = self.form.form.is_some();
        self.form.clear();
        if was_open {
            cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
        }
        cx.notify();
    }

    pub fn set_form_feedback(
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
        form.success_feedback_message = None;
        form.error = error;
        cx.notify();
        true
    }

    pub fn set_form_success_feedback(&mut self, message: String, cx: &mut Context<Self>) -> bool {
        let Some(form) = self.form.form.as_mut() else {
            return false;
        };
        form.pending = false;
        form.error = Some(message.clone());
        form.success_feedback_message = Some(message);
        cx.notify();
        true
    }

    pub fn start_path_picker(
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

    pub fn start_password_load(
        &mut self,
        load: impl std::future::Future<Output = Result<SecretString, SavedPasswordLoadError>> + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.form.form.as_mut() else {
            return false;
        };
        if form.password_loaded
            || form.password_load_id.is_some()
            || self.form.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting
        {
            return false;
        }
        let Some(keychain_id) = form.saved_password_keychain_id.clone() else {
            return false;
        };
        self.next_password_load_id = self.next_password_load_id.wrapping_add(1);
        let load_id = self.next_password_load_id;
        form.password_load_id = Some(load_id);
        form.password_load_error = None;
        // The form owner cancels the waiter on close or another reveal. A late result
        // owns a zeroizing value and cannot enter a replacement form or edited draft.
        self.password_load_task = Some(cx.spawn(async move |entity, cx| {
            let result = load.await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.next_password_load_id != load_id {
                    return;
                }
                entity.password_load_task = None;
                let Some(form) = entity.form.form.as_mut() else {
                    return;
                };
                if form.password_load_id != Some(load_id) {
                    return;
                }
                form.password_load_id = None;
                if form.password_loaded
                    || form.saved_password_keychain_id.as_ref() != Some(&keychain_id)
                    || form.auth_tab != super::SshAuthTab::Password
                    || entity.form.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(password) => {
                        let mut password = password.into_zeroizing();
                        zeroize::Zeroize::zeroize(&mut form.password);
                        form.empty_password = password.is_empty();
                        form.password = std::mem::take(&mut *password);
                        form.password_loaded = true;
                        form.password_from_store = true;
                        form.password_visible = true;
                        if form.focused_field == NewConnectionField::Password {
                            clear_connection_selection(form);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(category = ?error, "Saved SSH password could not be read");
                        form.password_load_error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
        true
    }

    fn finish_connection_form_exit(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.form.presence.finish_exit(generation) {
            return false;
        }
        self.connection_form_exit_task = None;
        self.jump_server_exit_task = None;
        self.path_picker_task = None;
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

    pub fn begin_jump_server_form_exit(
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
            if let Some(jump_server) = form.jump_server_form.take() {
                let edit_index = form.jump_server_edit_index.take();
                let proxy_hops = match form.jump_server_target {
                    super::ConnectionRouteTarget::Primary => {
                        form.proxy_chain_expanded = true;
                        &mut form.proxy_hops
                    }
                    super::ConnectionRouteTarget::StandaloneSftpSecondary => {
                        form.standalone_sftp_secondary.proxy_chain_expanded = true;
                        &mut form.standalone_sftp_secondary.proxy_hops
                    }
                };
                if let Some(index) = edit_index {
                    // The existing hop is moved into the modal, so both save and cancel
                    // restore it at its original position without duplicating secrets.
                    proxy_hops.insert(index.min(proxy_hops.len()), jump_server);
                } else if commit {
                    proxy_hops.push(jump_server);
                }
                if commit {
                    if form.auth_tab == super::SshAuthTab::TwoFactor {
                        form.auth_tab = super::SshAuthTab::Password;
                        form.focused_field = super::NewConnectionField::Password;
                    }
                    form.field_focused = false;
                    form.selected_field = None;
                    form.error = None;
                }
            }
        }
        self.form.jump_server_presence.reopen();
        cx.notify();
        true
    }

    pub fn has_host_key_challenge(&self) -> bool {
        self.host_key_challenge.is_some()
    }

    pub fn host_key_challenge_intent(&self) -> Option<SshConnectionIntent> {
        self.host_key_challenge
            .as_ref()
            .map(|challenge| challenge.intent.clone())
    }

    pub fn host_key_dialog_snapshot(&self) -> Option<HostKeyDialogSnapshot> {
        let challenge = self.host_key_challenge.as_ref()?;
        Some(HostKeyDialogSnapshot {
            visible: challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible,
            host: challenge.host.clone(),
            port: challenge.port,
            status: challenge.status.clone(),
        })
    }

    pub fn open_host_key_challenge(&mut self, challenge: HostKeyChallenge, cx: &mut Context<Self>) {
        // Replacing a challenge drops the previous config without duplicating its auth material.
        self.host_key_exit_task = None;
        if self.host_key_challenge.is_none() {
            cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
        }
        self.host_key_challenge = Some(challenge);
        cx.emit(AuthenticationChanged);
    }

    pub fn clear_host_key_challenge(&mut self, cx: &mut Context<Self>) {
        self.host_key_exit_task = None;
        if self.host_key_challenge.take().is_some() {
            cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
            cx.emit(AuthenticationChanged);
        }
    }

    pub fn take_host_key_challenge(&mut self, cx: &mut Context<Self>) -> Option<HostKeyChallenge> {
        self.host_key_exit_task = None;
        let challenge = self.host_key_challenge.take();
        if challenge.is_some() {
            cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
            cx.emit(AuthenticationChanged);
        }
        challenge
    }

    pub fn begin_host_key_challenge_exit(
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
        cx.emit(AuthenticationChanged);
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
        cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
        cx.emit(AuthenticationChanged);
        true
    }

    pub fn has_keyboard_interactive_challenge(&self) -> bool {
        self.keyboard_interactive_challenge.is_some()
    }

    pub fn keyboard_interactive_challenge(&self) -> Option<&KeyboardInteractiveChallenge> {
        self.keyboard_interactive_challenge.as_ref()
    }

    pub fn focused_keyboard_interactive_prompt(&self) -> Option<usize> {
        self.keyboard_interactive_challenge
            .as_ref()
            .map(|challenge| challenge.focused_prompt)
    }

    pub fn keyboard_interactive_response(&self, index: usize) -> Option<&str> {
        // The IME adapter builds a masked platform projection while this
        // Entity remains the only owner of the authentication response.
        self.keyboard_interactive_challenge
            .as_ref()?
            .responses
            .get(index)
            .map(String::as_str)
    }

    pub fn open_keyboard_interactive_challenge(
        &mut self,
        request: KeyboardInteractivePromptRequest,
        response_tx: impl Into<crate::prompts::NativeSshPromptSender>,
        cx: &mut Context<Self>,
    ) -> bool {
        let response_tx = response_tx.into();
        self.clear_inactive_keyboard_interactive_challenge(cx);
        if response_tx.is_closed() {
            return false;
        }
        if let Some(existing) = self.keyboard_interactive_challenge.as_ref()
            && existing.request.flow_id != request.flow_id
            && existing
                .response_tx
                .as_ref()
                .is_some_and(|tx| !tx.is_closed())
        {
            // Keep the active auth flow as the only owner of the protected dialog.
            let _ = response_tx.send(Err(SshPromptError::Cancelled));
            return false;
        }
        self.keyboard_interactive_timer_task = None;
        self.keyboard_interactive_exit_task = None;
        if let Some(mut existing) = self.keyboard_interactive_challenge.take() {
            if let Some(existing_tx) = existing.response_tx.take() {
                // Reject the replaced oneshot so no transport waits for stale input.
                let _ = existing_tx.send(Err(SshPromptError::Cancelled));
            }
        } else {
            cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
        }
        self.keyboard_interactive_challenge =
            Some(KeyboardInteractiveChallenge::new(request, response_tx));
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.schedule_keyboard_interactive_timer(self.keyboard_interactive_timer_generation, cx);
        cx.emit(AuthenticationChanged);
        true
    }

    fn clear_inactive_keyboard_interactive_challenge(&mut self, cx: &mut Context<Self>) {
        let Some(challenge) = self.keyboard_interactive_challenge.as_ref() else {
            return;
        };
        // An exiting dialog retains its payload only until its existing animation completes.
        if challenge.presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Exiting {
            return;
        }
        let timed_out = challenge.timed_out();
        let waiter_closed = challenge
            .response_tx
            .as_ref()
            .is_none_or(|tx| tx.is_closed());
        if !timed_out && !waiter_closed {
            return;
        }
        if let Some(mut challenge) = self.keyboard_interactive_challenge.take()
            && timed_out
            && let Some(response_tx) = challenge.response_tx.take()
        {
            let _ = response_tx.send(Err(SshPromptError::Timeout));
        }
        // The transport waiter and the UI answer buffer must end together, including retries.
        cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
        cx.emit(AuthenticationChanged);
    }

    pub fn wait_for_security_key_touch(&mut self, flow_id: &str, cx: &mut Context<Self>) {
        if let Some(challenge) = self.keyboard_interactive_challenge.as_mut()
            && challenge.request.flow_id == flow_id
        {
            challenge.external_wait = true;
            cx.emit(AuthenticationChanged);
        }
    }

    fn schedule_keyboard_interactive_timer(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.keyboard_interactive_timer_task = Some(cx.spawn(async move |connection_flow, cx| {
            loop {
                Timer::after(Duration::from_secs(1)).await;
                let keep_ticking = connection_flow
                    .update(cx, |connection_flow, cx| {
                        if connection_flow.keyboard_interactive_timer_generation != generation {
                            return false;
                        }
                        connection_flow.clear_inactive_keyboard_interactive_challenge(cx);
                        cx.emit(AuthenticationChanged);
                        connection_flow.has_keyboard_interactive_challenge()
                    })
                    .unwrap_or(false);
                if !keep_ticking {
                    break;
                }
            }
        }));
    }

    pub fn handle_keyboard_interactive_key(
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
                cx.emit(AuthenticationChanged);
                KeyboardInteractiveKeyAction::Handled
            }
            "backspace" => {
                if !challenge.timed_out()
                    && let Some(response) = challenge.responses.get_mut(challenge.focused_prompt)
                {
                    response.pop();
                    cx.emit(AuthenticationChanged);
                }
                KeyboardInteractiveKeyAction::Handled
            }
            _ => KeyboardInteractiveKeyAction::Handled,
        }
    }

    pub fn focus_keyboard_interactive_prompt(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(challenge) = self.keyboard_interactive_challenge.as_mut() else {
            return;
        };
        challenge.focused_prompt = index.min(challenge.responses.len().saturating_sub(1));
        cx.emit(AuthenticationChanged);
    }

    pub fn paste_keyboard_interactive_response(
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
        cx.emit(AuthenticationChanged);
        true
    }

    pub fn replace_keyboard_interactive_response(
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
        cx.emit(AuthenticationChanged);
        true
    }

    pub fn submit_keyboard_interactive_challenge(
        &mut self,
        cx: &mut Context<Self>,
    ) -> KeyboardInteractiveSubmitResult {
        let Some(mut challenge) = self.keyboard_interactive_challenge.take() else {
            return KeyboardInteractiveSubmitResult::Missing;
        };
        if challenge.timed_out() || !challenge.all_responses_filled() {
            self.keyboard_interactive_challenge = Some(challenge);
            cx.emit(AuthenticationChanged);
            return KeyboardInteractiveSubmitResult::Blocked;
        }
        self.keyboard_interactive_timer_generation =
            self.keyboard_interactive_timer_generation.wrapping_add(1);
        self.keyboard_interactive_timer_task = None;
        if let Some(response_tx) = challenge.response_tx.take() {
            // Move the Zeroizing response owner directly to the SSH prompt waiter.
            let _ = response_tx
                .send_with_remember(Ok(challenge.responses), challenge.remember_password);
        }
        cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
        cx.emit(AuthenticationChanged);
        KeyboardInteractiveSubmitResult::Submitted
    }

    pub fn toggle_prompt_password_remember(&mut self, cx: &mut Context<Self>) {
        if let Some(challenge) = self.keyboard_interactive_challenge.as_mut()
            && !challenge.timed_out()
            && challenge
                .response_tx
                .as_ref()
                .is_some_and(|sender| sender.can_remember())
        {
            challenge.remember_password = !challenge.remember_password;
            cx.emit(AuthenticationChanged);
        }
    }

    pub fn dismiss_keyboard_interactive_challenge(
        &mut self,
        delay: Duration,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.cancel_keyboard_interactive_challenge(delay, cx) {
            return false;
        }
        if let Some(form) = self.form.form.as_mut() {
            form.pending = false;
            form.error = Some(i18n.t("ssh.kbi.cancelled"));
            cx.notify();
        }
        true
    }

    pub fn cancel_keyboard_interactive_challenge(
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
        cx.emit(AuthenticationChanged);
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
        cx.emit(ConnectionFlowEvent::ModalBoundaryChanged);
        cx.emit(AuthenticationChanged);
        true
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod saved_password_tests;
