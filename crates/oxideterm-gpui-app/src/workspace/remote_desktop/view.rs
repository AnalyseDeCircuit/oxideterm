// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_session_adapter::ssh_config_from_saved_connection;

impl WorkspaceApp {
    pub(in crate::workspace) fn show_missing_remote_desktop_plugin(
        &self,
        protocol: RemoteDesktopProtocol,
        cx: &mut Context<Self>,
    ) {
        self.show_required_plugin_notice(
            &format!("com.oxideterm.remote-desktop.{}", protocol.provider_id()),
            &protocol.provider_id().to_uppercase(),
            cx,
        );
    }
    pub(in crate::workspace) fn remote_desktop_provider(
        &self,
        protocol: RemoteDesktopProtocol,
        cx: &App,
    ) -> Option<RemoteDesktopProviderManifest> {
        self.plugin_entity
            .read(cx)
            .remote_desktop_providers()
            .into_iter()
            .find(|provider| provider.protocol == protocol)
    }

    pub(in crate::workspace) fn open_remote_desktop_connection_for_connection(
        &mut self,
        mut profile: RemoteDesktopConnectionProfile,
        password: Option<RemoteDesktopSecret>,
        ssh_gateway_connection_id: Option<String>,
        mut connection_attempt_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.remote_desktop_provider(profile.protocol, cx).is_none() {
            if let Some(attempt) = connection_attempt_id.as_deref() {
                self.standalone_connections.mark_attempt_error(attempt);
            }
            self.show_missing_remote_desktop_plugin(profile.protocol, cx);
            return;
        }
        if let Some(attempt) = connection_attempt_id.as_deref() {
            let launch = if self
                .connection_store
                .get_remote_desktop_profile(&profile.id)
                .is_some()
            {
                standalone_connections::StandaloneConnectionLaunch::SavedRemoteDesktop {
                    profile_id: profile.id.clone(),
                }
            } else if let Some(provider) = self.remote_desktop_provider(profile.protocol, cx) {
                standalone_connections::StandaloneConnectionLaunch::RemoteDesktop {
                    profile: profile.clone(),
                    provider,
                    password: password
                        .as_ref()
                        .map(RemoteDesktopSecret::duplicate_for_reauthentication),
                    ssh_gateway_connection_id: ssh_gateway_connection_id.clone(),
                }
            } else {
                self.standalone_connections.mark_attempt_error(attempt);
                self.show_missing_remote_desktop_plugin(profile.protocol, cx);
                return;
            };
            self.standalone_connections.replace_launch_for_attempt(
                attempt,
                profile.label.clone(),
                launch,
            );
        }
        let Some(ssh_gateway_connection_id) = ssh_gateway_connection_id else {
            self.open_remote_desktop_connection_tab_for_connection(
                profile,
                password,
                None,
                None,
                connection_attempt_id,
                window,
                cx,
            );
            return;
        };
        // Gateway transport has one route owner; never proxy the loopback tunnel again.
        profile.socks_proxy = None;
        if connection_attempt_id.is_none() {
            let saved_profile_id = self
                .connection_store
                .get_remote_desktop_profile(&profile.id)
                .map(|saved| saved.id.clone());
            let kind = match profile.protocol {
                RemoteDesktopProtocol::Rdp => standalone_connections::StandaloneConnectionKind::Rdp,
                RemoteDesktopProtocol::Vnc => standalone_connections::StandaloneConnectionKind::Vnc,
            };
            let launch = if let Some(profile_id) = saved_profile_id {
                standalone_connections::StandaloneConnectionLaunch::SavedRemoteDesktop {
                    profile_id,
                }
            } else {
                let Some(provider) = self.remote_desktop_provider(profile.protocol, cx) else {
                    self.show_missing_remote_desktop_plugin(profile.protocol, cx);
                    return;
                };
                standalone_connections::StandaloneConnectionLaunch::RemoteDesktop {
                    profile: profile.clone(),
                    provider,
                    password: password
                        .as_ref()
                        .map(RemoteDesktopSecret::duplicate_for_reauthentication),
                    ssh_gateway_connection_id: Some(ssh_gateway_connection_id.clone()),
                }
            };
            connection_attempt_id = Some(self.standalone_connections.insert_pending(
                kind,
                profile.label.clone(),
                launch,
            ));
        }
        let reconnect_gateway_connection_id = ssh_gateway_connection_id.clone();
        let pending_tunnel =
            match self.start_remote_desktop_ssh_tunnel(ssh_gateway_connection_id, &profile, cx) {
                Ok(pending_tunnel) => pending_tunnel,
                Err(error) => {
                    self.report_remote_desktop_ssh_gateway_error(error, cx);
                    if let Some(connection_attempt_id) = connection_attempt_id.as_deref() {
                        self.standalone_connections
                            .mark_attempt_error(connection_attempt_id);
                    }
                    return;
                }
            };
        let window_handle = window.window_handle();
        cx.spawn(async move |workspace, cx| {
            match pending_tunnel.finish().await {
                Ok((transport_endpoint, lease)) => {
                    profile.transport_endpoint = Some(transport_endpoint);
                    let _ = cx.update_window(window_handle, move |_, window, cx| {
                        let _ = workspace.update(cx, |workspace, cx| {
                            if connection_attempt_id.as_deref().is_some_and(|attempt_id| {
                                !workspace
                                    .standalone_connections
                                    .is_connecting_attempt(attempt_id)
                            }) {
                                // A disconnect during tunnel setup must not resurrect the session.
                                return;
                            }
                            workspace.open_remote_desktop_connection_tab_for_connection(
                                profile,
                                password,
                                Some(lease),
                                Some(reconnect_gateway_connection_id),
                                connection_attempt_id,
                                window,
                                cx,
                            );
                        });
                    });
                    // If the target window vanished, dropping the captured lease
                    // above tears down the listener without exposing the secret.
                }
                Err(error) => {
                    let _ = workspace.update(cx, |workspace, cx| {
                        workspace.report_remote_desktop_ssh_gateway_error(error, cx);
                        if let Some(connection_attempt_id) = connection_attempt_id.as_deref() {
                            workspace
                                .standalone_connections
                                .mark_attempt_error(connection_attempt_id);
                        }
                    });
                }
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn start_remote_desktop_ssh_tunnel(
        &mut self,
        ssh_gateway_connection_id: String,
        profile: &RemoteDesktopConnectionProfile,
        cx: &mut Context<Self>,
    ) -> Result<PendingRemoteDesktopSshTunnel, String> {
        let audit_context = remote_desktop_audit_context(profile);
        let mut connect_audit =
            desktop_connection_audit(audit_context.as_ref(), "desktop_connect", None);
        let target_endpoint = profile.endpoint.clone();
        let result = (|| {
            let Some(gateway) = self
                .connection_store
                .get(&ssh_gateway_connection_id)
                .cloned()
            else {
                return Err(self
                    .i18n
                    .t("modals.new_connection.remote_desktop_ssh_gateway_missing"));
            };
            let Some(config) = ssh_config_from_saved_connection(
                &self.connection_store,
                self.settings_store.settings(),
                &gateway,
            ) else {
                return Err(self
                    .i18n
                    .t("modals.new_connection.remote_desktop_ssh_gateway_credentials_missing"));
            };
            let node_id = if config
                .proxy_chain
                .as_ref()
                .is_some_and(|chain| !chain.is_empty())
            {
                match self.expand_saved_connection_tree(
                    &ssh_gateway_connection_id,
                    config,
                    gateway.name,
                ) {
                    Ok(expansion) => expansion.target_node_id,
                    Err(error) => {
                        return Err(error.to_string());
                    }
                }
            } else {
                self.materialize_ssh_root_node(
                    config,
                    gateway.name,
                    Some(ssh_gateway_connection_id.clone()),
                )
            };
            if !self.ensure_node_connection_started(&node_id, cx) {
                return Err(self
                    .i18n
                    .t("modals.new_connection.remote_desktop_ssh_gateway_connect_failed"));
            }
            let _ = self.connection_store.mark_used(&ssh_gateway_connection_id);

            let lease_id = uuid::Uuid::new_v4().to_string();
            let forwarding_service = self.forwarding_service.clone();
            let worker_service = forwarding_service.clone();
            let worker_lease_id = lease_id.clone();
            let tunnel_context = audit_context.clone().map(|mut context| {
                context.parent_id = connect_audit
                    .as_ref()
                    .and_then(|audit| audit.id())
                    .map(str::to_owned);
                context
            });
            let worker =
                self.forwarding_runtime
                    .spawn(oxideterm_audit::AuditContext::scope_optional(
                        tunnel_context,
                        async move {
                            worker_service
                                .open_remote_desktop_tunnel(
                                    worker_lease_id,
                                    &node_id,
                                    &ssh_gateway_connection_id,
                                    target_endpoint.host,
                                    target_endpoint.port,
                                )
                                .await
                        },
                    ));
            Ok(PendingRemoteDesktopSshTunnel::new(
                move || forwarding_service.close_remote_desktop_tunnel(lease_id),
                worker,
                audit_context,
                connect_audit.take(),
            ))
        })();
        if result.is_err() {
            if let Some(audit) = connect_audit.take() {
                audit.finish(
                    oxideterm_audit::AuditOutcome::Failed,
                    oxideterm_audit::AuditEvidence::Request,
                    None,
                    None,
                );
            }
        }
        result
    }

    fn report_remote_desktop_ssh_gateway_error(&mut self, detail: String, cx: &mut Context<Self>) {
        let title = self
            .i18n
            .t("modals.new_connection.remote_desktop_ssh_gateway_connect_failed");
        self.push_command_palette_toast(
            title,
            Some(detail.clone()),
            TerminalNoticeVariant::Error,
            cx,
        );
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.set_status(Some(detail), cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn open_remote_desktop_preview_tab(
        &mut self,
        protocol: RemoteDesktopProtocol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let profile = preview_remote_desktop_profile(protocol);
        let provider = match builtin_preview_provider_registry()
            .ok()
            .and_then(|registry| registry.get_for_protocol(protocol).cloned())
        {
            Some(provider) => provider,
            None => {
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.provider_missing"),
                    None,
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        };
        let title = self.remote_desktop_preview_tab_title(protocol);

        self.open_remote_desktop_tab(profile, provider, title, None, window, cx);
    }

    pub(in crate::workspace) fn open_remote_desktop_connection_tab(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        password: Option<RemoteDesktopSecret>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_remote_desktop_connection_tab_with_tunnel(profile, password, None, window, cx);
    }

    pub(in crate::workspace) fn open_remote_desktop_connection_tab_with_tunnel(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        password: Option<RemoteDesktopSecret>,
        ssh_tunnel: Option<RemoteDesktopSshTunnelLease>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_remote_desktop_connection_tab_for_connection(
            profile, password, ssh_tunnel, None, None, window, cx,
        );
    }

    pub(in crate::workspace) fn open_remote_desktop_connection_tab_for_connection(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        password: Option<RemoteDesktopSecret>,
        ssh_tunnel: Option<RemoteDesktopSshTunnelLease>,
        ssh_gateway_connection_id: Option<String>,
        connection_attempt_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let provider = match self.remote_desktop_provider(profile.protocol, cx) {
            Some(provider) => provider,
            None => {
                self.show_missing_remote_desktop_plugin(profile.protocol, cx);
                if let Some(connection_attempt_id) = connection_attempt_id.as_deref() {
                    self.standalone_connections
                        .mark_attempt_error(connection_attempt_id);
                }
                return;
            }
        };
        let title = profile.label.clone();

        self.open_remote_desktop_tab_for_connection(
            profile,
            provider,
            title,
            password,
            ssh_tunnel,
            ssh_gateway_connection_id,
            connection_attempt_id,
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn open_remote_desktop_tab(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        title: String,
        password: Option<RemoteDesktopSecret>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TabId {
        self.open_remote_desktop_tab_with_tunnel(
            profile, provider, title, password, None, window, cx,
        )
    }

    pub(in crate::workspace) fn open_remote_desktop_tab_with_tunnel(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        title: String,
        password: Option<RemoteDesktopSecret>,
        ssh_tunnel: Option<RemoteDesktopSshTunnelLease>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TabId {
        self.open_remote_desktop_tab_for_connection(
            profile, provider, title, password, ssh_tunnel, None, None, window, cx,
        )
    }

    pub(in crate::workspace) fn open_remote_desktop_tab_for_connection(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        title: String,
        password: Option<RemoteDesktopSecret>,
        ssh_tunnel: Option<RemoteDesktopSshTunnelLease>,
        ssh_gateway_connection_id: Option<String>,
        connection_attempt_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TabId {
        let tab_id = self.alloc_tab_id(cx);
        let saved_profile_id = self
            .connection_store
            .get_remote_desktop_profile(&profile.id)
            .map(|saved| saved.id.clone());
        let reconnect_profile = profile.clone();
        let reconnect_provider = provider.clone();
        let reconnect_password = saved_profile_id
            .is_none()
            .then(|| {
                password
                    .as_ref()
                    .map(RemoteDesktopSecret::duplicate_for_reauthentication)
            })
            .flatten();
        let frame_slot = RemoteDesktopFrameDeliverySlot::new();
        let certificate_store_path =
            oxideterm_remote_desktop::RemoteDesktopCertificateStore::path_next_to_settings(
                self.settings_store.path(),
            );
        let session = cx.new(|cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                tab_id,
                profile,
                provider,
                password,
                certificate_store_path,
                frame_slot,
                window.window_handle(),
            );
            let plugins = self.plugin_entity.downgrade();
            session.use_plugin_provider(move |id, cx| {
                plugins.upgrade().and_then(|plugins| {
                    plugins
                        .read(cx)
                        .remote_desktop_providers()
                        .into_iter()
                        .find(|provider| provider.id == id)
                })
            });
            session.attach_tunnel(ssh_tunnel);
            session.install_release_handler(cx);
            session
        });
        let session_subscription = cx.subscribe(
            &session,
            move |workspace, session, event: &RemoteDesktopSessionEvent, cx| {
                workspace.handle_remote_desktop_session_event(&session, event, cx);
            },
        );
        let session_observation = cx.observe(&session, |_workspace, _session, cx| {
            // Session-owned state changes repaint every mounted workspace view,
            // including a detached window, without copying state back to root.
            cx.notify();
        });

        if let Some(previous_tab_id) = self.active_tab_id(cx) {
            self.release_remote_desktop_inputs_for_tab(previous_tab_id, cx);
        }
        self.remote_desktop.update(cx, |remote_desktop, _cx| {
            remote_desktop.insert(
                tab_id,
                session,
                vec![session_subscription, session_observation],
            );
        });
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::RemoteDesktop,
                title: title.clone(),
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            },
            cx,
        );
        let surface = standalone_connections::StandaloneConnectionSurface::RemoteDesktop(tab_id);
        if let Some(connection_attempt_id) = connection_attempt_id {
            self.standalone_connections
                .bind_surface_for_attempt(&connection_attempt_id, surface);
        } else {
            let kind = match reconnect_profile.protocol {
                RemoteDesktopProtocol::Rdp => standalone_connections::StandaloneConnectionKind::Rdp,
                RemoteDesktopProtocol::Vnc => standalone_connections::StandaloneConnectionKind::Vnc,
            };
            let launch = if let Some(profile_id) = saved_profile_id {
                standalone_connections::StandaloneConnectionLaunch::SavedRemoteDesktop {
                    profile_id,
                }
            } else {
                standalone_connections::StandaloneConnectionLaunch::RemoteDesktop {
                    profile: reconnect_profile,
                    provider: reconnect_provider,
                    password: reconnect_password,
                    ssh_gateway_connection_id,
                }
            };
            self.standalone_connections
                .insert(kind, title, launch, surface);
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.focus_remote_desktop_keyboard(tab_id, window, cx);
        self.reveal_active_tab(window, cx);
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            let initial_scale_factor = remote_desktop_scale_factor_percent(window.scale_factor());
            session.update(cx, |session, cx| {
                session.schedule_initial_layout_probe(initial_scale_factor, cx);
            });
        }
        cx.notify();
        tab_id
    }

    pub(in crate::workspace) fn render_remote_desktop_surface(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.bind_remote_desktop_window(tab_id, window.window_handle(), cx);
        self.sync_remote_desktop_frame_visibility(tab_id, cx);
        let Some(session) = self.remote_desktop_session_entity(tab_id, cx) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("remote_desktop.session_missing"))
                .into_any_element();
        };
        let certificate_dialog =
            session
                .read(cx)
                .certificate_prompt()
                .map(|(generation, challenge)| {
                    self.render_remote_desktop_certificate_dialog(tab_id, generation, challenge, cx)
                });
        let owner = cx.entity().downgrade();
        render_remote_desktop_panel(
            &session,
            &self.remote_desktop,
            &self.tokens,
            &self.i18n,
            window,
            cx,
            certificate_dialog,
            move |action, window, cx| {
                let _ = owner.update(cx, |app, cx| match action {
                    DesktopWindowAction::FocusKeyboard(tab_id) => {
                        app.focus_remote_desktop_keyboard(tab_id, window, cx)
                    }
                    DesktopWindowAction::Reconnect(tab_id) => {
                        app.reconnect_remote_desktop(tab_id, window, cx);
                        cx.notify();
                    }
                    DesktopWindowAction::Disconnect(tab_id) => {
                        app.release_remote_desktop_inputs_for_tab(tab_id, cx);
                        app.disconnect_remote_desktop(tab_id, window, cx);
                        cx.notify();
                    }
                    DesktopWindowAction::DismissOverlays => {
                        app.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                        cx.stop_propagation();
                    }
                    DesktopWindowAction::ShowCapabilityTooltip { text, position } => {
                        app.queue_workspace_tooltip(
                            "remote-desktop-vnc-capabilities",
                            text,
                            f32::from(position.x) + 12.0,
                            f32::from(position.y) + 16.0,
                            cx,
                        );
                    }
                    DesktopWindowAction::ClearCapabilityTooltip => {
                        app.clear_workspace_tooltip("remote-desktop-vnc-capabilities", cx);
                    }
                });
            },
        )
    }
}
