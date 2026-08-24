// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_session_adapter::ssh_config_from_saved_connection;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_remote_desktop_connection_with_gateway(
        &mut self,
        mut profile: RemoteDesktopConnectionProfile,
        password: Option<RemoteDesktopSecret>,
        ssh_gateway_connection_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ssh_gateway_connection_id) = ssh_gateway_connection_id else {
            self.open_remote_desktop_connection_tab(profile, password, window, cx);
            return;
        };
        let pending_tunnel = match self.start_remote_desktop_ssh_tunnel(
            ssh_gateway_connection_id,
            profile.endpoint.clone(),
            cx,
        ) {
            Ok(pending_tunnel) => pending_tunnel,
            Err(error) => {
                self.report_remote_desktop_ssh_gateway_error(error, cx);
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
                            workspace.open_remote_desktop_connection_tab_with_tunnel(
                                profile,
                                password,
                                Some(lease),
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
                    });
                }
            }
        })
        .detach();
    }

    pub(in crate::workspace) fn start_remote_desktop_ssh_tunnel(
        &mut self,
        ssh_gateway_connection_id: String,
        target_endpoint: RemoteDesktopEndpoint,
        cx: &mut Context<Self>,
    ) -> Result<PendingRemoteDesktopSshTunnel, String> {
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
                gateway.name.clone(),
            ) {
                Ok(expansion) => expansion.target_node_id,
                Err(error) => {
                    return Err(error.to_string());
                }
            }
        } else {
            self.materialize_ssh_root_node(
                config,
                gateway.name.clone(),
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
        let worker = self.forwarding_runtime.spawn(async move {
            worker_service
                .open_remote_desktop_tunnel(
                    worker_lease_id,
                    &node_id,
                    &ssh_gateway_connection_id,
                    target_endpoint.host,
                    target_endpoint.port,
                )
                .await
        });
        Ok(PendingRemoteDesktopSshTunnel::new(
            lease_id,
            forwarding_service,
            worker,
        ))
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
        let provider = match builtin_provider_registry()
            .ok()
            .and_then(|registry| registry.get_for_protocol(profile.protocol).cloned())
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
        let title = profile.label.clone();

        self.open_remote_desktop_tab_with_tunnel(
            profile, provider, title, password, ssh_tunnel, window, cx,
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
        let tab_id = self.alloc_tab_id(cx);
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
            session.ssh_tunnel = ssh_tunnel;
            session.install_release_handler(cx);
            session
        });
        let session_subscription = cx.subscribe(
            &session,
            move |workspace, session, event: &RemoteDesktopSessionEvent, cx| {
                workspace.handle_remote_desktop_session_event(tab_id, &session, event, cx);
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
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            },
            cx,
        );
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.focus_remote_desktop_keyboard(window, cx);
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
        // Rendering is the authoritative mount boundary for both the main
        // workspace and detached windows.
        self.bind_remote_desktop_window(tab_id, window.window_handle(), cx);
        let Some(session_entity) = self.remote_desktop.read(cx).session(tab_id) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("remote_desktop.session_missing"))
                .into_any_element();
        };

        let session = session_entity.read(cx);
        let geometry = session.geometry.clone();
        let certificate_challenge = session.certificate_challenge.clone();
        let vnc_file_browser = session.vnc_files.open.then(|| session.vnc_files.clone());
        let worker_generation = session.worker_generation;
        let resize_menu_open = self.remote_desktop_resize_menu_tab_id == Some(tab_id);
        let resize_session = session_entity.clone();
        window.on_next_frame(move |window, cx| {
            let scale_factor = Some(remote_desktop_scale_factor_percent(window.scale_factor()));
            let _ = resize_session.update(cx, |session, cx| {
                // Canvas geometry is final only after layout. Replaying it on
                // the next frame makes local window/sidebar/tab changes drive
                // resize independently from incoming remote desktop frames.
                if session.schedule_viewport_resize(scale_factor, cx) {
                    cx.notify();
                }
            });
        });
        let desktop_surface = div()
            .min_h(px(0.0))
            .flex_1()
            .relative()
            .child(remote_desktop_surface_with_geometry(
                &self.tokens,
                &session.state,
                Some(geometry),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Pressed,
                        cx,
                    ) {
                        cx.notify();
                    }
                    this.focus_remote_desktop_keyboard(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Pressed,
                        cx,
                    ) {
                        cx.notify();
                    }
                    this.focus_remote_desktop_keyboard(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Pressed,
                        cx,
                    ) {
                        cx.notify();
                    }
                    this.focus_remote_desktop_keyboard(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(gpui::NavigationDirection::Back),
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Pressed,
                        cx,
                    ) {
                        cx.notify();
                    }
                    this.focus_remote_desktop_keyboard(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(gpui::NavigationDirection::Forward),
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Pressed,
                        cx,
                    ) {
                        cx.notify();
                    }
                    this.focus_remote_desktop_keyboard(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Released,
                        cx,
                    ) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_mouse_button_release_out(
                        tab_id,
                        RemoteDesktopMouseButton::Left,
                        cx,
                    ) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Released,
                        cx,
                    ) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Right,
                cx.listener(move |this, _event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_mouse_button_release_out(
                        tab_id,
                        RemoteDesktopMouseButton::Right,
                        cx,
                    ) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Released,
                        cx,
                    ) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_mouse_button_release_out(
                        tab_id,
                        RemoteDesktopMouseButton::Middle,
                        cx,
                    ) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Navigate(gpui::NavigationDirection::Back),
                cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Released,
                        cx,
                    ) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Navigate(gpui::NavigationDirection::Back),
                cx.listener(move |this, _event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_mouse_button_release_out(
                        tab_id,
                        RemoteDesktopMouseButton::Back,
                        cx,
                    ) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Navigate(gpui::NavigationDirection::Forward),
                cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_gpui_mouse_button(
                        tab_id,
                        event.position,
                        event.button,
                        RemoteDesktopMouseButtonState::Released,
                        cx,
                    ) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Navigate(gpui::NavigationDirection::Forward),
                cx.listener(move |this, _event: &MouseUpEvent, _window, cx| {
                    if this.handle_remote_desktop_mouse_button_release_out(
                        tab_id,
                        RemoteDesktopMouseButton::Forward,
                        cx,
                    ) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    if this.handle_remote_desktop_mouse_move(tab_id, event.position, cx) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_scroll_wheel(
                cx.listener(move |this, event: &ScrollWheelEvent, _window, cx| {
                    if this.handle_remote_desktop_wheel(tab_id, event.position, &event.delta, cx) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            );

        div()
            .size_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(desktop_surface)
            .child(self.render_remote_desktop_footer(tab_id, cx))
            .when(resize_menu_open, |surface| {
                surface.child(self.workspace_context_menu_backdrop(
                    self.render_remote_desktop_resize_menu(tab_id, window, cx),
                    cx,
                ))
            })
            .when_some(certificate_challenge, |surface, challenge| {
                surface.child(self.render_remote_desktop_certificate_dialog(
                    tab_id,
                    worker_generation,
                    challenge,
                    cx,
                ))
            })
            .when_some(vnc_file_browser, |surface, browser| {
                surface.child(self.render_vnc_file_browser(tab_id, browser, cx))
            })
            .into_any_element()
    }
}
