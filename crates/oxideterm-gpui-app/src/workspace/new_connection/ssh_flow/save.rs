// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::{
    WorkspaceNotificationKind, WorkspaceNotificationScope, WorkspaceNotificationSeverity,
    standalone_connections::{StandaloneConnectionKind, StandaloneConnectionLaunch},
};
use oxideterm_gpui_connections::profiles::{SavedSshOpen, prepare_saved_ssh_open};
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
        self.connections.report_connection_error(message, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_save_runtime_node_form(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_runtime_node_form(node_id, true, window, cx);
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
        let action = self.connections.flow.read(cx).default_submit_action();
        self.submit_new_connection_form_with_action(action, window, cx);
    }

    pub(in crate::workspace) fn submit_new_connection_form_with_action(
        &mut self,
        action: NewConnectionSubmitAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::submission::ConnectionSubmissionEffect as Effect;

        let Some(result) = self.connections.flow.update(cx, |flow, cx| {
            flow.submit_form(
                action,
                &mut self.connection_store,
                self.settings_store.settings(),
                &self.i18n,
                |parent| {
                    oxideterm_gpui_connections::conversion::runtime_proxy_hops_for_parent_path(
                        &self.node_router,
                        parent,
                        &self.i18n,
                    )
                },
                |id| match &self.standalone_connections.record(id)?.launch {
                    StandaloneConnectionLaunch::RestoredRemoteDesktop { profile } => Some(profile),
                    _ => None,
                },
                cx,
            )
        }) else {
            return;
        };
        if result.persisted {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        match result.effect {
            None => {}
            Some(Effect::Ftp) => self.submit_ftp_connection_form(action, cx),
            Some(Effect::LocalTerminal(plan)) => {
                self.open_local_terminal_from_form(plan, window, cx)
            }
            Some(Effect::Console(open)) => self.open_console_from_form(open, window, cx),
            Some(Effect::Ble(open)) => {
                self.start_ble_open_flow(open, cx);
                cx.notify();
            }
            Some(Effect::Mosh(mut open, reconnect_id)) => {
                if let Some(id) = reconnect_id {
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
            Some(Effect::Desktop(open)) => {
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
            Some(Effect::WslGraphics) => {
                self.close_new_connection_form(window, cx);
                self.open_graphics_tab(window, cx);
            }
            Some(Effect::SavedAuthentication(launch)) => {
                self.launch_saved_connection_prompt(launch, window, cx);
            }
            Some(Effect::SavedEdit { id, next }) => self.apply_saved_edit(id, next, window, cx),
            Some(Effect::Duplicate { status }) => {
                self.connections.show_catalog_status(status, cx);
                self.focus_active_pane(window, cx);
                cx.notify();
            }
            Some(Effect::Preflight((config, title, intent))) => {
                self.start_connection_preflight_plan(config, title, intent, false, window, cx);
            }
            Some(Effect::CloseForm) => self.close_new_connection_form(window, cx),
            Some(Effect::ReportSaveError(error)) => self.report_saved_next_hop_message(error, cx),
        }
    }

    fn open_console_from_form(
        &mut self,
        open: oxideterm_gpui_connections::submission::ConsoleFormOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::submission::{ConsoleFormOpen, ConsoleProfileAfterOpen};
        use oxideterm_terminal_triggers::SavedConnectionKind;
        let (session, profile, kind) = match open {
            ConsoleFormOpen::Serial(open) => (
                self.create_serial_terminal_tab(
                    open.config,
                    &open.title,
                    open.terminal,
                    window,
                    cx,
                ),
                ConsoleProfileAfterOpen::Serial(open.edited_profile),
                SavedConnectionKind::Serial,
            ),
            ConsoleFormOpen::Telnet(open) => (
                self.create_telnet_terminal_tab(open.config, open.proxy, open.terminal, window, cx),
                ConsoleProfileAfterOpen::Telnet(open.edited_profile, open.profile_id),
                SavedConnectionKind::Telnet,
            ),
        };
        let result = self.connections.finish_console_open(
            session.as_ref().map(|_| ()).map_err(ToString::to_string),
            profile,
            &mut self.connection_store,
            &self.i18n,
            cx,
        );
        if let (Ok(session), Some(result)) = (session, result) {
            if result.persisted {
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            if let Some(id) = result.open {
                if matches!(kind, SavedConnectionKind::Telnet) {
                    self.telnet_terminal_profile_ids.insert(session, id.clone());
                }
                self.register_terminal_saved_connection(session, kind, id, cx);
            }
            cx.notify();
        }
    }

    pub(in crate::workspace) fn start_new_connection_flow(
        &mut self,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let launch = self.connections.flow.update(cx, |flow, cx| {
            flow.prepare_ssh_launch(
                &self.connection_store,
                self.settings_store.settings(),
                &self.i18n,
                intent,
                cx,
            )
        });
        if let Some((config, title, intent)) = launch {
            self.start_connection_preflight_plan(config, title, intent, false, window, cx);
        }
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
        let Some((plan, open_sftp_on_connect)) = prepare_saved_ssh_open(
            &self.connection_store,
            self.settings_store.settings(),
            id,
            open_sftp_on_connect_override,
        ) else {
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
        match plan {
            SavedSshOpen::Authenticate(conn) => {
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
                    self.connections
                        .flow
                        .update(cx, |flow, cx| flow.enable_prompt_sftp(cx));
                }
            }
            SavedSshOpen::Connect { config, title } => self.start_saved_connection_flow(
                id.to_string(),
                config,
                title,
                None,
                open_sftp_on_connect,
                window,
                cx,
            ),
        }
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
        self.connections.flow.update(cx, |flow, cx| {
            flow.arm_reconnect_after_edit(id, node_id, cx);
        });
    }

    pub(in crate::workspace) fn open_runtime_node_reconnect_editor(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_runtime_node_form(node_id, false, window, cx);
    }

    fn open_runtime_node_form(
        &mut self,
        node_id: NodeId,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = self
            .ssh_nodes
            .get(&node_id)
            .ok_or_else(|| anyhow::anyhow!(self.i18n.t("ssh.form.runtime_node_missing")))
            .and_then(|node| {
                oxideterm_gpui_connections::conversion::form_for_runtime_node(
                    &self.node_router,
                    &node_id,
                    &node.title,
                    save,
                    &self.i18n,
                )
            });
        match draft {
            Ok(draft) => {
                self.prepare_modal_interaction_boundary(cx);
                self.connections
                    .flow
                    .update(cx, |flow, cx| flow.replace_form(draft, cx));
                self.focus_connection_form(window, cx);
            }
            Err(error) if save => self.set_connection_status(Some(error.to_string()), cx),
            Err(_) => {}
        }
    }

    fn launch_saved_connection_prompt(
        &mut self,
        launch: oxideterm_gpui_connections::submission::SavedPromptLaunch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    fn apply_saved_edit(
        &mut self,
        id: String,
        next: oxideterm_gpui_connections::submission::AfterSshEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::submission::AfterSshEdit;
        self.sync_saved_connection_node_title(&id);
        self.sync_saved_connection_x11_forwarding(&id);
        self.apply_saved_connection_terminal_preferences(&id, cx);
        match next {
            AfterSshEdit::Reconnect {
                node,
                config,
                title,
                open_sftp,
            } => {
                self.remove_inactive_session_tree_node(&node, window, cx);
                self.start_saved_connection_flow(id, config, title, None, open_sftp, window, cx);
            }
            AfterSshEdit::Prompt { message } => {
                self.open_saved_connection_prompt(
                    &id,
                    SavedConnectionPromptAction::Connect,
                    Some(message),
                    window,
                    cx,
                );
            }
            AfterSshEdit::FocusPane { status } => {
                self.connections.show_catalog_status(status, cx);
                self.focus_active_pane(window, cx);
            }
        }
        cx.notify();
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
        &mut self,
        id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::submission::SavedSftpOpen;
        let open = self.connections.flow.update(cx, |flow, _| {
            flow.prepare_saved_sftp_open(
                id,
                &self.connection_store,
                self.settings_store.settings(),
                |id| self.standalone_sftp_sessions.contains_key(id),
                &self.i18n,
            )
        });
        match open {
            Ok(None) => {}
            Err(error) => self.set_connection_status(Some(error), cx),
            Ok(Some(SavedSftpOpen::Existing {
                title,
                primary: (id, path),
                secondary,
            })) => {
                if let Some((secondary, secondary_path)) = secondary {
                    self.open_standalone_sftp_pair_tab_surface(
                        id,
                        secondary,
                        title,
                        path,
                        secondary_path,
                        cx,
                    );
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
        &mut self,
        id: &str,
        attempt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::submission::{SavedMoshOpen, prepare_saved_mosh_open};
        let reconnect = attempt
            .as_deref()
            .and_then(|id| self.standalone_connections.connection_id_for_attempt(id));
        let draft = match prepare_saved_mosh_open(
            &self.connection_store,
            id,
            attempt.clone(),
            reconnect,
            &self.i18n,
        ) {
            SavedMoshOpen::Connect(open) => {
                self.set_connection_status(Some(self.i18n.t("ssh.form.checking_host_key")), cx);
                self.start_ssh_preflight(
                    open.config,
                    open.title,
                    SshConnectionIntent::Mosh(open.options),
                    cx,
                );
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
            self.connections
                .flow
                .update(cx, |flow, cx| flow.replace_form(draft, cx));
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
