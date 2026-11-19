// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::{
    WorkspaceNotificationKind, WorkspaceNotificationScope, WorkspaceNotificationSeverity,
    standalone_connections::{StandaloneConnectionKind, StandaloneConnectionLaunch},
};
use oxideterm_gpui_connections::profiles::{
    saved_connection_for_open, saved_connection_requires_credentials,
};
use oxideterm_gpui_terminal::TerminalNoticeVariant;

impl WorkspaceApp {
    pub(super) fn report_saved_next_hop_error(&mut self, i18n_key: &str, cx: &mut Context<Self>) {
        self.report_saved_next_hop_message(self.i18n.t(i18n_key), cx);
    }

    pub(super) fn report_saved_next_hop_message(
        &mut self,
        message: String,
        cx: &mut Context<Self>,
    ) {
        let reported_to_form = self.connections.flow.update(cx, |connection_flow, cx| {
            connection_flow.set_form_feedback(Some(false), Some(message.clone()), cx)
        });
        if !reported_to_form {
            self.connections.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn open_save_runtime_node_form(&mut self, node_id: NodeId, window: &mut Window, cx: &mut Context<Self>) {
        self.open_runtime_node_form(node_id, true, window, cx);
    }

    fn runtime_proxy_hops_for_parent_path(&self, parent: &NodeId) -> anyhow::Result<Vec<NewConnectionProxyHop>> {
        oxideterm_gpui_connections::conversion::runtime_proxy_hops_for_parent_path(&self.node_router, parent, &self.i18n)
    }

    pub(in crate::workspace) fn close_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if !self.connections.flow.update(cx, |connection_flow, cx| {
            connection_flow.begin_connection_form_exit(delay, cx)
        }) {
            return;
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn submit_new_connection_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = if self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| form.standalone_connection_id.is_some())
        {
            NewConnectionSubmitAction::Connect
        } else {
            NewConnectionSubmitAction::SaveAndConnect
        };
        self.submit_new_connection_form_with_action(action, window, cx);
    }

    pub(in crate::workspace) fn submit_new_connection_form_with_action(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::submission::{FormSubmission, plan_form_submission};

        let plan = self.connections.flow.update(cx, |flow, cx| {
            let plan = plan_form_submission(&mut flow.form, action, &self.i18n);
            cx.notify();
            plan
        });
        match plan {
            None => {}
            Some(FormSubmission::Ftp) => self.submit_ftp_connection_form(action, cx),
            Some(FormSubmission::LocalTerminal) => {
                self.submit_local_terminal_form(action, window, cx)
            }
            Some(FormSubmission::Serial) => self.submit_serial_connection_form(action, window, cx),
            Some(FormSubmission::Telnet) => self.submit_telnet_connection_form(action, window, cx),
            Some(FormSubmission::Ble) => self.submit_ble_connection_form(action, window, cx),
            Some(FormSubmission::Mosh) => self.submit_mosh_connection_form(action, window, cx),
            Some(FormSubmission::StandaloneSftp) => {
                self.submit_standalone_sftp_connection_form(action, window, cx)
            }
            Some(FormSubmission::RemoteDesktop) => {
                self.submit_remote_desktop_connection_form(action, window, cx)
            }
            Some(FormSubmission::WslGraphics) => {
                self.close_new_connection_form(window, cx);
                self.open_graphics_tab(window, cx);
            }
            Some(FormSubmission::SavedAuthentication) => {
                self.submit_saved_connection_prompt(window, cx)
            }
            Some(FormSubmission::Edit) => self.save_editing_connection(window, cx),
            Some(FormSubmission::Duplicate) => self.save_duplicate_connection_template(window, cx),
            Some(FormSubmission::Connect(intent)) => {
                self.start_new_connection_flow(intent, window, cx)
            }
            Some(FormSubmission::Save {
                parent,
                connect: false,
            }) => {
                self.save_new_connection_without_connecting(parent.as_ref(), window, cx);
            }
            Some(FormSubmission::Save {
                parent,
                connect: true,
            }) => {
                if let Some(handoff) = self.save_current_connection_form(parent.as_ref(), cx) {
                    self.start_saved_form_connection_flow(handoff, parent, window, cx);
                }
            }
        }
    }

    pub(super) fn save_new_connection_without_connecting(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .save_current_connection_form(drill_down_parent_id, cx)
            .is_some()
        {
            self.close_new_connection_form(window, cx);
        }
    }

    pub(super) fn save_current_connection_form(
        &mut self,
        drill_down_parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) -> Option<SavedConnectionRuntimeHandoff> {
        let prefix = drill_down_parent_id.map_or_else(
            || Ok(Vec::new()),
            |id| self.runtime_proxy_hops_for_parent_path(id),
        );
        let store = &mut self.connection_store;
        let i18n = &self.i18n;
        let handoff = self.connections.flow.update(cx, |flow, cx| {
            let form = flow.form.form.as_mut()?;
            let handoff =
                oxideterm_gpui_connections::submission::persist_ssh_form(form, store, prefix, i18n);
            cx.notify();
            handoff
        });
        if handoff.is_some() {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        handoff
    }

    fn start_saved_form_connection_flow(
        &mut self,
        handoff: SavedConnectionRuntimeHandoff,
        parent: Option<NodeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent = match parent
            .map(|id| {
                self.runtime_proxy_hops_for_parent_path(&id)
                    .map(|hops| (id, hops.len()))
            })
            .transpose()
        {
            Ok(parent) => parent,
            Err(error) => return self.report_saved_next_hop_message(error.to_string(), cx),
        };
        let launch = self.connections.flow.update(cx, |flow, cx| {
            flow.launch_persisted_ssh_form(
                &self.connection_store,
                self.settings_store.settings(),
                &self.i18n,
                handoff,
                parent,
                cx,
            )
        });
        match launch {
            Ok((config, title, intent)) => {
                self.start_new_connection_config_flow(config, title, intent, window, cx)
            }
            Err(error) => self.report_saved_next_hop_message(error, cx),
        }
    }

    pub(super) fn submit_serial_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let save = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_serial(&mut self.connection_store, &self.i18n, action, cx)
        });
        let Some(open) = self.apply_profile_submission(save, cx) else {
            return;
        };
        let result =
            self.create_serial_terminal_tab(open.config, &open.title, open.terminal, window, cx);
        match result {
            Ok(session) => {
                if let Some(request) = open.edited_profile {
                    let save = self
                        .connection_store
                        .upsert_serial_profile(request)
                        .map(|profile| profile.id);
                    if let Some(id) = self.report_profile_update_after_open(
                        save,
                        "modals.new_connection.serial_save_failed",
                        cx,
                    ) {
                        self.register_terminal_saved_connection(
                            session,
                            oxideterm_terminal_triggers::SavedConnectionKind::Serial,
                            id,
                            cx,
                        );
                    }
                }
                self.update_connection_form_state(cx, ConnectionFormState::clear);
                cx.notify();
            }
            Err(error) => self.set_connection_form_launch_error(error.to_string(), cx),
        }
    }

    pub(super) fn submit_telnet_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let save = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_telnet(&mut self.connection_store, &self.i18n, action, cx)
        });
        let Some(open) = self.apply_profile_submission(save, cx) else {
            return;
        };
        let result =
            self.create_telnet_terminal_tab(open.config, open.proxy, open.terminal, window, cx);
        match result {
            Ok(session) => {
                let id = if let Some(request) = open.edited_profile {
                    let save = self
                        .connection_store
                        .upsert_telnet_profile(request)
                        .map(|profile| profile.id);
                    self.report_profile_update_after_open(
                        save,
                        "modals.new_connection.telnet_save_failed",
                        cx,
                    )
                    .or(open.profile_id)
                } else {
                    open.profile_id
                };
                if let Some(id) = id {
                    self.telnet_terminal_profile_ids.insert(session, id.clone());
                    self.register_terminal_saved_connection(
                        session,
                        oxideterm_terminal_triggers::SavedConnectionKind::Telnet,
                        id,
                        cx,
                    );
                }
                self.update_connection_form_state(cx, ConnectionFormState::clear);
                cx.notify();
            }
            Err(error) => self.set_connection_form_launch_error(error.to_string(), cx),
        }
    }

    fn report_profile_update_after_open(
        &mut self,
        result: Result<String, impl std::fmt::Display>,
        key: &str,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        match result {
            Ok(id) => {
                self.queue_cloud_sync_dirty_refresh(cx);
                Some(id)
            }
            Err(error) => {
                let message = format!("{}: {error}", self.i18n.t(key));
                self.connections.session_manager
                    .update(cx, |manager, cx| manager.set_status(Some(message), cx));
                None
            }
        }
    }

    fn set_connection_form_launch_error(&self, message: String, cx: &mut Context<Self>) {
        self.connections.flow.update(cx, |flow, cx| {
            flow.set_form_feedback(Some(false), Some(message), cx)
        });
    }

    pub(in crate::workspace::new_connection) fn apply_profile_submission<T>(
        &mut self,
        result: Option<oxideterm_gpui_connections::submission::ProfileSubmission<T>>,
        cx: &mut Context<Self>,
    ) -> Option<T> {
        let result = result?;
        if result.persisted {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        result.open
    }

    pub(super) fn submit_mosh_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let save = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_mosh(&mut self.connection_store, &self.i18n, action, cx)
        });
        let Some(mut open) = self.apply_profile_submission(save, cx) else {
            return;
        };
        if let Some(id) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| form.standalone_connection_id.clone())
        {
            let Some(attempt) = self.standalone_connections.begin_reconnect(&id) else {
                return;
            };
            open.options.runtime_connection_attempt_id = Some(attempt);
        }
        self.start_ssh_preflight(
            open.config,
            open.title,
            SshConnectionIntent::Mosh(open.options),
            cx,
        );
        cx.notify();
    }

    pub(super) fn submit_standalone_sftp_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action == NewConnectionSubmitAction::Connect {
            let initial_remote_path =
                self.connection_form_state(cx)
                    .form
                    .as_ref()
                    .and_then(|form| {
                        let path = form.sftp_initial_remote_path.trim();
                        (!path.is_empty()).then(|| path.to_string())
                    });
            self.start_new_connection_flow(
                SshConnectionIntent::StandaloneSftp {
                    saved_profile_id: None,
                    initial_remote_path,
                    pair_launch_token: None,
                },
                window,
                cx,
            );
            return;
        }
        let save = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_saved_sftp(
                &mut self.connection_store,
                self.settings_store.settings(),
                &self.i18n,
                action,
                cx,
            )
        });
        if let Some((config, title, intent)) = self.apply_profile_submission(save, cx) {
            self.start_new_connection_config_flow(config, title, intent, window, cx);
        }
    }

    pub(super) fn submit_remote_desktop_connection_form(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let reconnect = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .and_then(|form| form.standalone_connection_id.as_deref())
            .and_then(|id| self.standalone_connections.record(id))
            .and_then(|record| match &record.launch {
                StandaloneConnectionLaunch::RestoredRemoteDesktop { profile } => Some(profile),
                _ => None,
            });
        let save = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_desktop(
                &mut self.connection_store,
                self.settings_store.settings(),
                &self.i18n,
                action,
                reconnect,
                cx,
            )
        });
        let Some(open) = self.apply_profile_submission(save, cx) else {
            return;
        };
        let attempt = if let Some(id) = open.reconnect_id {
            let Some(attempt) = self.standalone_connections.begin_reconnect(&id) else {
                return;
            };
            Some(attempt)
        } else {
            None
        };
        let password = open
            .password
            .map(|secret| RemoteDesktopSecret::from(secret.into_zeroizing()));
        self.open_remote_desktop_connection_for_connection(
            open.profile,
            password,
            open.gateway,
            attempt,
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::workspace) fn start_new_connection_flow(
        &mut self,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = self.connections.flow.update(cx, |flow, cx| {
            flow.plan_form_ssh_preflight(
                &self.connection_store,
                self.settings_store.settings(),
                &self.i18n,
                &intent,
                cx,
            )
        });
        if let Some((config, title)) = config {
            self.start_new_connection_config_flow(config, title, intent, window, cx);
        }
    }

    fn start_new_connection_config_flow(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_connection_preflight_plan(config, title, intent, false, window, cx);
    }

    fn start_connection_preflight_plan(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        show_session_status: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::preflight::{RoutePreflight, SshPreflightPlan};
        let plan = self.connections.flow.update(cx, |flow, cx| {
            flow.plan_preflight(config, title, intent, !show_session_status, &self.i18n, cx)
        });
        let SshPreflightPlan {
            route,
            config,
            title,
            intent,
        } = match plan {
            Ok(plan) => plan,
            Err(message) => {
                if let Some(message) = message {
                    self.set_connection_status(Some(message), cx);
                }
                return;
            }
        };
        if show_session_status {
            self.set_connection_status(Some(self.i18n.t("ssh.form.checking_host_key")), cx);
        }
        match route {
            RoutePreflight::Test => return self.start_ssh_test_flow(config, title, cx),
            RoutePreflight::AlreadyVerified => {
                return self.continue_verified_ssh_flow(config, title, intent, window, cx);
            }
            RoutePreflight::Routed => {
                self.start_proxy_session_tree_connect(config, title, intent, None, window, cx)
            }
            RoutePreflight::Direct => self.start_ssh_preflight(config, title, intent, cx),
        }
        cx.notify();
    }

    pub(crate) fn open_saved_connection(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_connection_with_sftp(id, false, window, cx);
    }

    /// `open_sftp_on_connect_override` is the Ctrl-click escape hatch; saved
    /// connections otherwise follow their own per-connection setting.
    pub(crate) fn open_saved_connection_with_sftp(
        &mut self,
        id: &str,
        open_sftp_on_connect_override: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = saved_connection_for_open(&self.connection_store, id) else {
            // Saved rows can outlive an external store update. Report the
            // stale reference without exposing its identifier or connection data.
            tracing::warn!("Saved connection lookup failed before opening");
            let title = self.i18n.t("sessionManager.toast.connection_not_found");
            self.push_command_palette_toast(title.clone(), None, TerminalNoticeVariant::Error, cx);
            self.push_notification_entry(
                WorkspaceNotificationKind::Connection,
                WorkspaceNotificationSeverity::Error,
                title,
                None,
                WorkspaceNotificationScope::Global,
                Some("saved-connection-not-found".to_string()),
            );
            cx.notify();
            return;
        };
        let config = ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &conn,
        );
        let open_sftp_on_connect =
            open_sftp_on_connect_override || conn.options.open_sftp_on_connect;
        if saved_connection_requires_credentials(config.as_ref()) {
            if self.try_reuse_active_saved_connection_terminal(id, &conn, window, cx) {
                if open_sftp_on_connect
                    && let Some(node_id) = self.indexed_saved_ssh_node_for_connection(id, &conn)
                    && let Some(session_id) = self
                        .workspace_runtime
                        .read(cx)
                        .ssh_terminal_session_ids_for_node(&node_id)
                        .first()
                        .copied()
                {
                    self.maybe_open_sftp_after_terminal_open(
                        &node_id, session_id, true, window, cx,
                    );
                }
                return;
            }
            self.open_saved_connection_prompt(
                id,
                SavedConnectionPromptAction::Connect,
                Some(
                    self.i18n
                        .t("sessionManager.edit_properties.password_required"),
                ),
                window,
                cx,
            );
            if open_sftp_on_connect {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.open_sftp_on_connect = true;
                    }
                });
            }
            return;
        }
        let config = config.expect("credential form handles unavailable configurations");
        let title = conn.name.clone();
        self.start_saved_connection_flow(
            id.to_string(),
            config,
            title,
            None,
            open_sftp_on_connect,
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn open_saved_connection_prompt(
        &mut self,
        id: &str,
        action: SavedConnectionPromptAction,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_connection_form(id, Some(action), error, window, cx);
    }

    pub(in crate::workspace) fn open_saved_connection_editor(
        &mut self,
        id: &str,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_connection_form(id, None, error, window, cx);
    }

    fn open_saved_connection_form(
        &mut self,
        id: &str,
        prompt: Option<SavedConnectionPromptAction>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.connection_store.get(id).is_none() {
            return;
        }
        self.prepare_modal_interaction_boundary(cx);
        self.connections.flow.update(cx, |flow, cx| {
            flow.open_saved_form(&self.connection_store, id, prompt, error, cx)
        });
        self.focus_connection_form(window, cx);
    }

    pub(super) fn focus_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_saved_connection_reconnect_editor(
        &mut self,
        node_id: NodeId,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_connection_editor(id, None, window, cx);
        if self
            .connection_form_state(cx)
            .editing_saved_connection_id
            .as_deref()
            == Some(id)
        {
            // This marker is consumed after a successful save so normal
            // connection edits keep their existing save-only behavior.
            self.update_connection_form_state(cx, |state| {
                state.editing_saved_connection_connect_after_save_node_id = Some(node_id);
            });
        }
    }

    pub(in crate::workspace) fn open_runtime_node_reconnect_editor(&mut self, node_id: NodeId, window: &mut Window, cx: &mut Context<Self>) {
        self.open_runtime_node_form(node_id, false, window, cx);
    }

    fn open_runtime_node_form(&mut self, node_id: NodeId, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self.ssh_nodes.get(&node_id)
            .ok_or_else(|| anyhow::anyhow!(self.i18n.t("ssh.form.runtime_node_missing")))
            .and_then(|node| oxideterm_gpui_connections::conversion::form_for_runtime_node(&self.node_router, &node_id, &node.title, save, &self.i18n));
        match draft {
            Ok(draft) => {
                self.prepare_modal_interaction_boundary(cx);
                self.update_connection_form_state(cx, |state| state.replace_with_new_form(draft));
                self.focus_connection_form(window, cx);
            }
            Err(error) if save => self.set_connection_status(Some(error.to_string()), cx),
            Err(_) => {}
        }
    }

    pub(super) fn submit_saved_connection_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let launch = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_saved_prompt(
                &self.connection_store,
                self.settings_store.settings(),
                &self.i18n,
                cx,
            )
        });
        let Some(launch) = launch else {
            return;
        };
        match launch.action {
            SavedConnectionPromptAction::Test => {
                self.start_ssh_test_flow(launch.config, launch.title, cx)
            }
            SavedConnectionPromptAction::Connect => {
                self.start_saved_connection_flow(
                    launch.id,
                    launch.config,
                    launch.title,
                    launch.auth_to_save,
                    launch.open_sftp,
                    window,
                    cx,
                );
            }
            SavedConnectionPromptAction::Tunnel => {
                let mut groups = launch.remaining_tunnels;
                if let Some((owner, ids)) = groups.pop_front()
                    && owner == launch.id
                {
                    if let Some(node) =
                        self.connect_ssh_tunnels_with_config(owner, ids, launch.config, cx)
                    {
                        self.arm_saved_auth_save(&node, launch.auth_to_save);
                    }
                    self.start_tunnel_groups(groups, false, window, cx);
                }
            }
        }
    }

    pub(super) fn sync_saved_connection_node_title(&mut self, saved_connection_id: &str) -> bool {
        let Some(title) = self
            .connection_store
            .get(saved_connection_id)
            .map(|connection| connection.name.clone())
        else {
            return false;
        };
        sync_saved_connection_node_title_for_nodes(&mut self.ssh_nodes, saved_connection_id, &title)
    }

    pub(super) fn sync_saved_connection_x11_forwarding(&self, saved_connection_id: &str) -> bool {
        let Some(options) = self
            .connection_store
            .get(saved_connection_id)
            .map(|connection| connection.options.x11_forwarding)
        else {
            return false;
        };
        // Only the non-secret policy is updated in place. Existing shells and
        // the registry-owned physical connection retain their current owners.
        self.node_router.update_saved_connection_x11_forwarding(
            saved_connection_id,
            x11_forward_policy(options),
        ) > 0
    }

    pub(super) fn save_editing_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let edit = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_ssh_edit(&mut self.connection_store, &self.i18n, false, cx)
        });
        let Some(edit) = edit else {
            return;
        };
        self.sync_saved_connection_node_title(&edit.id);
        self.sync_saved_connection_x11_forwarding(&edit.id);
        self.apply_saved_connection_terminal_preferences(&edit.id, cx);
        self.queue_cloud_sync_dirty_refresh(cx);
        if let Some(node_id) = edit.reconnect_node {
            if let Some(connection) = self.connection_store.get(&edit.id)
                && let Some(config) = ssh_config_from_saved_connection(
                    &self.connection_store,
                    self.settings_store.settings(),
                    connection,
                )
            {
                let title = connection.name.clone();
                let open_sftp = connection.options.open_sftp_on_connect;
                self.remove_inactive_session_tree_node(&node_id, window, cx);
                self.start_saved_connection_flow(
                    edit.id, config, title, None, open_sftp, window, cx,
                );
            } else {
                let message = self
                    .i18n
                    .t("sessionManager.edit_properties.password_placeholder");
                self.open_saved_connection_prompt(
                    &edit.id,
                    SavedConnectionPromptAction::Connect,
                    Some(message),
                    window,
                    cx,
                );
            }
        } else {
            let message = self.i18n.t("sessionManager.edit_properties.save");
            self.connections.session_manager
                .update(cx, |manager, cx| manager.set_status(Some(message), cx));
            self.focus_active_pane(window, cx);
        }
        cx.notify();
    }

    pub(super) fn save_duplicate_connection_template(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .connections
            .flow
            .update(cx, |flow, cx| {
                flow.submit_ssh_edit(&mut self.connection_store, &self.i18n, true, cx)
            })
            .is_some()
        {
            let message = self.i18n.t("sessionManager.toast.connection_duplicated");
            self.connections.session_manager
                .update(cx, |manager, cx| manager.set_status(Some(message), cx));
            self.queue_cloud_sync_dirty_refresh(cx);
            self.focus_active_pane(window, cx);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn start_saved_connection_flow(
        &mut self,
        id: String,
        config: SshConfig,
        title: String,
        auth_save_target: Option<super::super::password_prompt::SavedAuthSaveTarget>,
        open_sftp_on_connect: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_connection_preflight_plan(
            config,
            title,
            SshConnectionIntent::ConnectSaved {
                id,
                auth_save_target,
                open_sftp_on_connect,
            },
            true,
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn open_saved_standalone_sftp_profile(
        &mut self, id: &str, _window: &mut Window, cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::submission::SavedSftpOpen;
        let open = self.connections.flow.update(cx, |flow, _| flow.prepare_saved_sftp_open(
            id, &self.connection_store, self.settings_store.settings(),
            |id| self.standalone_sftp_sessions.contains_key(id), &self.i18n,
        ));
        match open {
            Ok(None) => {}
            Err(error) => self.set_connection_status(Some(error), cx),
            Ok(Some(SavedSftpOpen::Existing { title, primary: (id, path), secondary })) => {
                if let Some((secondary, secondary_path)) = secondary {
                    self.open_standalone_sftp_pair_tab_surface(id, secondary, title, path, secondary_path, cx);
                } else {
                    self.open_standalone_sftp_tab_surface(id, title, path, cx);
                }
            }
            Ok(Some(SavedSftpOpen::Preflight((config, title, intent)))) => {
                self.set_connection_status(Some(self.i18n.t("ssh.form.checking_host_key")), cx);
                self.start_ssh_preflight(config, title, intent, cx);
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn open_saved_mosh_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_saved_mosh_profile_for_connection(id, None, window, cx);
    }

    pub(in crate::workspace) fn open_saved_mosh_profile_for_connection(
        &mut self, id: &str, attempt: Option<String>, window: &mut Window, cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::submission::{SavedMoshOpen, prepare_saved_mosh_open};
        let reconnect = attempt.as_deref().and_then(|id| self.standalone_connections.connection_id_for_attempt(id));
        let draft = match prepare_saved_mosh_open(&self.connection_store, id, attempt.clone(), reconnect, &self.i18n) {
            SavedMoshOpen::Connect(open) => {
                self.set_connection_status(Some(self.i18n.t("ssh.form.checking_host_key")), cx);
                self.start_ssh_preflight(open.config, open.title, SshConnectionIntent::Mosh(open.options), cx);
                cx.notify();
                return;
            }
            SavedMoshOpen::Missing => None,
            SavedMoshOpen::SupplyCredentials(draft) => Some(draft),
        };
        if let Some(attempt) = attempt {
            self.standalone_connections.mark_attempt_error(&attempt);
        }
        if let Some(draft) = draft {
            self.open_new_connection_form(window, cx);
            self.update_connection_form_state(cx, |state| state.replace_with_new_form(draft));
        }
    }

    pub(in crate::workspace) fn start_ssh_preflight(
        &mut self,
        mut config: SshConfig,
        title: String,
        mut intent: SshConnectionIntent,
        cx: &App,
    ) {
        if let SshConnectionIntent::Mosh(options) = &mut intent {
            let mut reconnect_options = options.clone();
            // A later manual retry must not reuse an automation request correlation token.
            reconnect_options.public_mcp_open_token = None;
            let reconnect_launch = options.saved_profile_id.as_ref().map_or_else(
                || StandaloneConnectionLaunch::MoshPreflight {
                    config: config.clone(),
                    options: reconnect_options,
                },
                |profile_id| StandaloneConnectionLaunch::SavedMosh {
                    profile_id: profile_id.clone(),
                },
            );
            if let Some(attempt) = &options.runtime_connection_attempt_id {
                self.standalone_connections.replace_launch_for_attempt(
                    attempt,
                    title.clone(),
                    reconnect_launch,
                );
            } else {
                options.runtime_connection_attempt_id =
                    Some(self.standalone_connections.insert_pending(
                        StandaloneConnectionKind::Mosh,
                        title.clone(),
                        reconnect_launch,
                    ));
            }
        }
        let tx = self.ssh_worker_sender(cx);
        if matches!(intent, SshConnectionIntent::Mosh(_))
            && self.plugin_entity.read(cx).mosh_executable().is_none()
        {
            let host = config.host.clone();
            let port = config.port;
            let upstream_proxy = config.upstream_proxy.take();
            let _ = tx.send(SshConnectionWorkerResult::Preflight {
                config,
                upstream_proxy,
                title,
                intent,
                host,
                port,
                status: HostKeyStatus::Error {
                    message: self.plugin_requirement_message(
                        "com.oxideterm.terminal.mosh",
                        "Mosh",
                        cx,
                    ),
                },
            });
            return;
        }
        oxideterm_gpui_connections::workers::preflight(
            config,
            title,
            intent,
            tx,
            managed_key_resolver_from_store(&self.connection_store),
            self.i18n.clone(),
        );
    }
}
