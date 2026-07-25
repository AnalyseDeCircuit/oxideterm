// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn map_remote_desktop_pointer_position(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
    ) -> Option<RemoteDesktopMappedPoint> {
        let point = self
            .remote_desktop_sessions
            .get(&tab_id)
            .and_then(|session| session.geometry.map_window_point(position))?;
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            // Servers do not always echo pointer-position updates for client
            // moves. Update the local cursor state immediately so custom
            // remote cursors track the pointer without waiting for a roundtrip.
            session.state.apply_event(RemoteDesktopHelperEvent::Cursor {
                x: point.x,
                y: point.y,
                width: 0,
                height: 0,
            });
        }
        Some(point)
    }

    pub(in crate::workspace) fn handle_remote_desktop_mouse_move(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
    ) -> bool {
        let Some(point) = self.map_remote_desktop_pointer_position(tab_id, position) else {
            return false;
        };
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::MouseMove {
                x: point.x,
                y: point.y,
            },
        );
        true
    }

    pub(in crate::workspace) fn handle_remote_desktop_mouse_button(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        button: RemoteDesktopMouseButton,
        state: RemoteDesktopMouseButtonState,
    ) -> bool {
        let Some(point) = self.map_remote_desktop_pointer_position(tab_id, position) else {
            return false;
        };
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            match state {
                RemoteDesktopMouseButtonState::Pressed => {
                    session.pressed_mouse_buttons.insert(button);
                }
                RemoteDesktopMouseButtonState::Released => {
                    session.pressed_mouse_buttons.remove(&button);
                }
            }
        }
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::MouseMove {
                x: point.x,
                y: point.y,
            },
        );
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::MouseButton { button, state },
        );
        true
    }

    pub(in crate::workspace) fn handle_remote_desktop_gpui_mouse_button(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        button: gpui::MouseButton,
        state: RemoteDesktopMouseButtonState,
    ) -> bool {
        let Some(button) = remote_desktop_mouse_button_from_gpui(button) else {
            return false;
        };
        self.handle_remote_desktop_mouse_button(tab_id, position, button, state)
    }

    pub(in crate::workspace) fn handle_remote_desktop_mouse_button_release_out(
        &mut self,
        tab_id: TabId,
        button: RemoteDesktopMouseButton,
    ) -> bool {
        let should_release = self
            .remote_desktop_sessions
            .get_mut(&tab_id)
            .is_some_and(|session| session.pressed_mouse_buttons.remove(&button));
        if !should_release {
            return false;
        }
        // A drag can start on the framebuffer and end outside it. The release
        // edge must still reach the remote session or the remote button state
        // remains pressed until a later release happens inside the image.
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::MouseButton {
                button,
                state: RemoteDesktopMouseButtonState::Released,
            },
        );
        true
    }

    pub(in crate::workspace) fn handle_remote_desktop_wheel(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        delta: &gpui::ScrollDelta,
    ) -> bool {
        let Some(point) = self.map_remote_desktop_pointer_position(tab_id, position) else {
            return false;
        };

        let delta = self
            .remote_desktop_sessions
            .get_mut(&tab_id)
            .and_then(|session| {
                remote_desktop_wheel_delta_from_scroll(delta, &mut session.wheel_pixel_remainder)
            });
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::MouseMove {
                x: point.x,
                y: point.y,
            },
        );
        if let Some(delta) = delta {
            self.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Wheel { delta });
        }
        true
    }

    pub(in crate::workspace) fn handle_remote_desktop_key(
        &mut self,
        tab_id: TabId,
        keystroke: &gpui::Keystroke,
        state: RemoteDesktopKeyState,
    ) {
        let modifiers = keystroke.modifiers;
        self.sync_remote_desktop_modifiers(tab_id, modifiers);
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::Key {
                key: RemoteDesktopKey {
                    code: keystroke.key.clone(),
                    text: keystroke.key_char.clone(),
                    alt: modifiers.alt,
                    ctrl: modifiers.control,
                    shift: modifiers.shift,
                    meta: modifiers.platform,
                },
                state,
            },
        );
    }

    pub(in crate::workspace) fn sync_remote_desktop_modifiers(
        &mut self,
        tab_id: TabId,
        modifiers: gpui::Modifiers,
    ) {
        let next = RemoteDesktopModifierState::from_gpui(modifiers);
        let Some(previous) = self
            .remote_desktop_sessions
            .get_mut(&tab_id)
            .map(|session| {
                let previous = session.last_input_modifiers;
                session.last_input_modifiers = next;
                previous
            })
        else {
            return;
        };
        if previous == next {
            return;
        }
        for request in remote_desktop_modifier_sync_requests(previous, next) {
            self.send_remote_desktop_request(tab_id, request);
        }
    }

    pub(in crate::workspace) fn sync_remote_desktop_lock_keys(
        &mut self,
        tab_id: TabId,
        capslock: gpui::Capslock,
    ) {
        let Some((previous, next)) = self
            .remote_desktop_sessions
            .get_mut(&tab_id)
            .map(|session| {
                let previous = session.last_lock_keys;
                let next = remote_desktop_lock_keys_with_capslock(previous, capslock);
                session.last_lock_keys = Some(next);
                (previous, next)
            })
        else {
            return;
        };
        if let Some(request) = remote_desktop_lock_key_sync_request(previous, next) {
            self.send_remote_desktop_request(tab_id, request);
        }
    }

    pub(in crate::workspace) fn sync_remote_desktop_lock_key_press(
        &mut self,
        tab_id: TabId,
        keystroke: &gpui::Keystroke,
    ) {
        let Some((previous, next)) =
            self.remote_desktop_sessions
                .get_mut(&tab_id)
                .and_then(|session| {
                    let previous = session.last_lock_keys;
                    let next =
                        remote_desktop_lock_keys_after_pressed_code(previous, &keystroke.key)?;
                    session.last_lock_keys = Some(next);
                    Some((previous, next))
                })
        else {
            return;
        };
        if let Some(request) = remote_desktop_lock_key_sync_request(previous, next) {
            self.send_remote_desktop_request(tab_id, request);
        }
    }

    pub(in crate::workspace) fn forward_remote_desktop_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        self.sync_remote_desktop_modifiers(tab_id, event.modifiers);
        self.sync_remote_desktop_lock_keys(tab_id, event.capslock);
        true
    }

    pub(in crate::workspace) fn forward_remote_desktop_key_from_capture(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        if remote_desktop_paste_shortcut(&event.keystroke) {
            self.paste_remote_desktop_from_keystroke(&event.keystroke, cx);
            return true;
        }
        if remote_desktop_copy_shortcut(&event.keystroke) {
            self.copy_remote_desktop_from_keystroke(&event.keystroke, cx);
            return true;
        }
        self.handle_remote_desktop_key(tab_id, &event.keystroke, RemoteDesktopKeyState::Pressed);
        self.sync_remote_desktop_lock_key_press(tab_id, &event.keystroke);
        true
    }

    pub(in crate::workspace) fn forward_remote_desktop_key_up(
        &mut self,
        event: &KeyUpEvent,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        if remote_desktop_paste_shortcut(&event.keystroke)
            || remote_desktop_copy_shortcut(&event.keystroke)
        {
            return true;
        }
        self.handle_remote_desktop_key(tab_id, &event.keystroke, RemoteDesktopKeyState::Released);
        true
    }

    pub(in crate::workspace) fn copy_remote_desktop_from_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        self.release_remote_desktop_shortcut_modifiers(tab_id, keystroke);
        self.copy_remote_desktop(cx)
    }

    pub(in crate::workspace) fn copy_remote_desktop(&mut self, _cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        self.send_remote_desktop_control_shortcut(tab_id, "c");
        true
    }

    pub(in crate::workspace) fn paste_remote_desktop_from_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        self.release_remote_desktop_shortcut_modifiers(tab_id, keystroke);
        self.paste_remote_desktop(cx)
    }

    pub(in crate::workspace) fn paste_remote_desktop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        let Some(item) = cx.read_from_clipboard() else {
            return true;
        };

        if let Some(paths) = remote_desktop_clipboard_paths_from_item(&item) {
            let files_enabled = self
                .remote_desktop_sessions
                .get(&tab_id)
                .is_some_and(|session| {
                    session.provider.capabilities.clipboard_files
                        && session.profile.session_options.clipboard.files
                        && (session.profile.protocol != RemoteDesktopProtocol::Vnc
                            || session
                                .state
                                .snapshot()
                                .negotiated_capabilities
                                .as_ref()
                                .is_some_and(|capabilities| {
                                    capabilities.vendor_files
                                        == NegotiatedCapabilityStatus::Supported
                                }))
                });
            if files_enabled {
                self.send_remote_desktop_request(
                    tab_id,
                    RemoteDesktopHelperRequest::ClipboardFiles {
                        transfer_id: uuid::Uuid::new_v4().to_string(),
                        paths,
                    },
                );
            }
            // External paths must never fall through to text injection because
            // that would bypass the file-redirection consent boundary.
            return true;
        }

        // Provider support describes the helper boundary; VNC binary formats
        // additionally require server evidence from Extended Clipboard caps.
        if let Some(session) = self.remote_desktop_sessions.get(&tab_id)
            && session.provider.capabilities.clipboard_data
            && session.profile.session_options.clipboard.images
            && (session.profile.protocol != RemoteDesktopProtocol::Vnc
                || session
                    .state
                    .snapshot()
                    .negotiated_capabilities
                    .as_ref()
                    .is_some_and(|capabilities| {
                        capabilities.extended_clipboard == NegotiatedCapabilityStatus::Supported
                            && capabilities
                                .extended_clipboard_formats
                                .iter()
                                .any(|format| format == "dib-v5")
                    }))
            && let Some(data) = remote_desktop_clipboard_data_from_item(&item)
        {
            self.send_remote_desktop_request(
                tab_id,
                RemoteDesktopHelperRequest::ClipboardData { data },
            );
            return true;
        }

        let text_enabled = self
            .remote_desktop_sessions
            .get(&tab_id)
            .is_some_and(|session| {
                session.provider.capabilities.clipboard_text
                    && session.profile.session_options.clipboard.text
            });
        if !text_enabled {
            return true;
        }
        let Some(text) = item.text() else {
            return true;
        };
        if text.is_empty() {
            return true;
        }

        // Update the remote clipboard and also inject text for pre-login fields
        // that may not honor CLIPRDR until the desktop session is fully active.
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::ClipboardText { text: text.clone() },
        );
        self.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Text { text });
        true
    }

    pub(in crate::workspace) fn release_remote_desktop_shortcut_modifiers(
        &mut self,
        tab_id: TabId,
        keystroke: &gpui::Keystroke,
    ) {
        let modifiers = keystroke.modifiers;
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            if modifiers.control {
                session.last_input_modifiers.ctrl = false;
            }
            if modifiers.platform {
                session.last_input_modifiers.meta = false;
            }
            if modifiers.shift {
                session.last_input_modifiers.shift = false;
            }
        }
        for code in remote_desktop_shortcut_modifier_release_codes(keystroke) {
            self.send_remote_desktop_request(
                tab_id,
                RemoteDesktopHelperRequest::Key {
                    key: RemoteDesktopKey {
                        code: code.to_string(),
                        text: None,
                        alt: false,
                        ctrl: false,
                        shift: false,
                        meta: false,
                    },
                    state: RemoteDesktopKeyState::Released,
                },
            );
        }
    }

    pub(in crate::workspace) fn send_remote_desktop_control_shortcut(
        &mut self,
        tab_id: TabId,
        code: &str,
    ) {
        let key = RemoteDesktopKey {
            code: code.to_string(),
            text: Some(code.to_string()),
            alt: false,
            ctrl: true,
            shift: false,
            meta: false,
        };
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::Key {
                key: key.clone(),
                state: RemoteDesktopKeyState::Pressed,
            },
        );
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::Key {
                key,
                state: RemoteDesktopKeyState::Released,
            },
        );
    }

    pub(in crate::workspace) fn active_remote_desktop_tab_id(&self) -> Option<TabId> {
        self.active_tab()
            .filter(|tab| tab.kind == TabKind::RemoteDesktop)
            .map(|tab| tab.id)
    }

    pub(in crate::workspace) fn remote_desktop_preview_tab_title(
        &self,
        protocol: RemoteDesktopProtocol,
    ) -> String {
        match protocol {
            RemoteDesktopProtocol::Rdp => self.i18n.t("remote_desktop.rdp_preview_title"),
            RemoteDesktopProtocol::Vnc => self.i18n.t("remote_desktop.vnc_preview_title"),
        }
    }
}
