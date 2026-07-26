// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
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
                );
                return;
            }
        };
        let title = profile.label.clone();

        self.open_remote_desktop_tab(profile, provider, title, password, window, cx);
    }

    pub(in crate::workspace) fn open_remote_desktop_tab(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        title: String,
        password: Option<RemoteDesktopSecret>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = self.alloc_tab_id();
        let frame_slot = RemoteDesktopFrameDeliverySlot::new();
        let certificate_store_path =
            oxideterm_remote_desktop::RemoteDesktopCertificateStore::path_next_to_settings(
                self.settings_store.path(),
            );
        let session = cx.new(|cx| {
            let session = RemoteDesktopSessionEntity::new(
                tab_id,
                profile,
                provider,
                password,
                certificate_store_path,
                frame_slot,
                window.window_handle(),
            );
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

        if let Some(previous_tab_id) = self.main_window_tabs.active_tab_id {
            self.release_remote_desktop_inputs_for_tab(previous_tab_id, cx);
        }
        self.remote_desktop.update(cx, |remote_desktop, _cx| {
            remote_desktop.insert(
                tab_id,
                session,
                vec![session_subscription, session_observation],
            );
        });
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::RemoteDesktop,
            title,
            title_source: TabTitleSource::Static,
            root_pane: None,
            active_pane_id: None,
        });
        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.focus_remote_desktop_keyboard(window, cx);
        self.reveal_active_tab(window);
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| {
                session.schedule_initial_layout_probe(cx);
            });
        }
        cx.notify();
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
        let worker_generation = session.worker_generation;
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
            .when_some(certificate_challenge, |surface, challenge| {
                surface.child(self.render_remote_desktop_certificate_dialog(
                    tab_id,
                    worker_generation,
                    challenge,
                    cx,
                ))
            })
            .into_any_element()
    }
}
