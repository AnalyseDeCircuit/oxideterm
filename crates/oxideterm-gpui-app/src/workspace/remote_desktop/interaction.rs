// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl RemoteDesktopSessionEntity {
    pub(super) fn send_request(&mut self, request: RemoteDesktopHelperRequest) {
        if matches!(request, RemoteDesktopHelperRequest::Resize { .. })
            && !self.provider.capabilities.resize
        {
            return;
        }
        if let RemoteDesktopHelperRequest::Resize { size, .. } = &request {
            self.state.mark_resize_requested(*size);
        }
        if let Some(worker) = self.worker.as_ref() {
            worker.send(request);
        } else if matches!(request, RemoteDesktopHelperRequest::Close) {
            self.state
                .apply_event(RemoteDesktopHelperEvent::Disconnected { reason: None });
        }
    }

    fn map_pointer_position(
        &mut self,
        position: Point<Pixels>,
    ) -> Option<RemoteDesktopMappedPoint> {
        let point = self.geometry.map_window_point(position)?;
        // Servers do not always echo pointer moves. Keep the custom cursor
        // responsive without waiting for a round trip.
        self.state.apply_event(RemoteDesktopHelperEvent::Cursor {
            x: point.x,
            y: point.y,
            width: 0,
            height: 0,
        });
        Some(point)
    }

    fn handle_mouse_move(&mut self, position: Point<Pixels>) -> bool {
        let Some(point) = self.map_pointer_position(position) else {
            return false;
        };
        self.send_request(RemoteDesktopHelperRequest::MouseMove {
            x: point.x,
            y: point.y,
        });
        true
    }

    fn handle_mouse_button(
        &mut self,
        position: Point<Pixels>,
        button: RemoteDesktopMouseButton,
        state: RemoteDesktopMouseButtonState,
    ) -> bool {
        let Some(point) = self.map_pointer_position(position) else {
            return false;
        };
        match state {
            RemoteDesktopMouseButtonState::Pressed => {
                self.pressed_mouse_buttons.insert(button);
            }
            RemoteDesktopMouseButtonState::Released => {
                self.pressed_mouse_buttons.remove(&button);
            }
        }
        self.send_request(RemoteDesktopHelperRequest::MouseMove {
            x: point.x,
            y: point.y,
        });
        self.send_request(RemoteDesktopHelperRequest::MouseButton { button, state });
        true
    }

    fn release_mouse_button_out(&mut self, button: RemoteDesktopMouseButton) -> bool {
        if !self.pressed_mouse_buttons.remove(&button) {
            return false;
        }
        // Releases outside the framebuffer must still reach the server.
        self.send_request(RemoteDesktopHelperRequest::MouseButton {
            button,
            state: RemoteDesktopMouseButtonState::Released,
        });
        true
    }

    fn handle_wheel(&mut self, position: Point<Pixels>, delta: &gpui::ScrollDelta) -> bool {
        let Some(point) = self.map_pointer_position(position) else {
            return false;
        };
        let wheel_delta =
            remote_desktop_wheel_delta_from_scroll(delta, &mut self.wheel_pixel_remainder);
        self.send_request(RemoteDesktopHelperRequest::MouseMove {
            x: point.x,
            y: point.y,
        });
        if let Some(delta) = wheel_delta {
            self.send_request(RemoteDesktopHelperRequest::Wheel { delta });
        }
        true
    }

    fn handle_key(&mut self, keystroke: &gpui::Keystroke, state: RemoteDesktopKeyState) {
        let modifiers = keystroke.modifiers;
        self.sync_modifiers(modifiers);
        self.send_request(RemoteDesktopHelperRequest::Key {
            key: RemoteDesktopKey {
                code: keystroke.key.clone(),
                text: keystroke.key_char.clone(),
                alt: modifiers.alt,
                ctrl: modifiers.control,
                shift: modifiers.shift,
                meta: modifiers.platform,
            },
            state,
        });
    }

    fn sync_modifiers(&mut self, modifiers: gpui::Modifiers) {
        let next = RemoteDesktopModifierState::from_gpui(modifiers);
        let previous = std::mem::replace(&mut self.last_input_modifiers, next);
        if previous == next {
            return;
        }
        for request in remote_desktop_modifier_sync_requests(previous, next) {
            self.send_request(request);
        }
    }

    fn sync_lock_keys(&mut self, capslock: gpui::Capslock) {
        let previous = self.last_lock_keys;
        let next = remote_desktop_lock_keys_with_capslock(previous, capslock);
        self.last_lock_keys = Some(next);
        if let Some(request) = remote_desktop_lock_key_sync_request(previous, next) {
            self.send_request(request);
        }
    }

    fn sync_lock_key_press(&mut self, keystroke: &gpui::Keystroke) {
        let previous = self.last_lock_keys;
        let Some(next) = remote_desktop_lock_keys_after_pressed_code(previous, &keystroke.key)
        else {
            return;
        };
        self.last_lock_keys = Some(next);
        if let Some(request) = remote_desktop_lock_key_sync_request(previous, next) {
            self.send_request(request);
        }
    }

    pub(super) fn release_inputs(&mut self) {
        self.last_input_modifiers = RemoteDesktopModifierState::default();
        self.last_lock_keys = None;
        self.pressed_mouse_buttons.clear();
        self.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
        self.send_request(RemoteDesktopHelperRequest::ReleaseAllInputs);
    }

    fn release_shortcut_modifiers(&mut self, keystroke: &gpui::Keystroke) {
        let modifiers = keystroke.modifiers;
        if modifiers.control {
            self.last_input_modifiers.ctrl = false;
        }
        if modifiers.platform {
            self.last_input_modifiers.meta = false;
        }
        if modifiers.shift {
            self.last_input_modifiers.shift = false;
        }
        for code in remote_desktop_shortcut_modifier_release_codes(keystroke) {
            self.send_request(RemoteDesktopHelperRequest::Key {
                key: RemoteDesktopKey {
                    code: code.to_string(),
                    text: None,
                    alt: false,
                    ctrl: false,
                    shift: false,
                    meta: false,
                },
                state: RemoteDesktopKeyState::Released,
            });
        }
    }

    fn send_control_shortcut(&mut self, code: &str) {
        let key = RemoteDesktopKey {
            code: code.to_string(),
            text: Some(code.to_string()),
            alt: false,
            ctrl: true,
            shift: false,
            meta: false,
        };
        self.send_request(RemoteDesktopHelperRequest::Key {
            key: key.clone(),
            state: RemoteDesktopKeyState::Pressed,
        });
        self.send_request(RemoteDesktopHelperRequest::Key {
            key,
            state: RemoteDesktopKeyState::Released,
        });
    }

    fn paste_clipboard(&mut self, item: ClipboardItem) {
        if let Some(paths) = remote_desktop_clipboard_paths_from_item(&item) {
            let files_enabled = self.provider.capabilities.clipboard_files
                && self.profile.session_options.clipboard.files
                && (self.profile.protocol != RemoteDesktopProtocol::Vnc
                    || self
                        .state
                        .snapshot()
                        .negotiated_capabilities
                        .as_ref()
                        .is_some_and(|capabilities| {
                            capabilities.vendor_file_upload == NegotiatedCapabilityStatus::Supported
                        }));
            if files_enabled {
                self.send_request(RemoteDesktopHelperRequest::ClipboardFiles {
                    transfer_id: uuid::Uuid::new_v4().to_string(),
                    paths,
                });
            }
            // External paths cannot fall through to text injection because
            // that would bypass the file-redirection consent boundary.
            return;
        }

        let binary_clipboard_enabled = self.provider.capabilities.clipboard_data
            && self.profile.session_options.clipboard.images
            && (self.profile.protocol != RemoteDesktopProtocol::Vnc
                || self
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
                    }));
        if binary_clipboard_enabled
            && let Some(data) = remote_desktop_clipboard_data_from_item(&item)
        {
            self.send_request(RemoteDesktopHelperRequest::ClipboardData { data });
            return;
        }

        if !self.provider.capabilities.clipboard_text
            || !self.profile.session_options.clipboard.text
        {
            return;
        }
        let Some(text) = item.text() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        // Some pre-login fields do not honor CLIPRDR, so retain the explicit
        // text injection fallback after updating the remote clipboard.
        self.send_request(RemoteDesktopHelperRequest::ClipboardText { text: text.clone() });
        self.send_request(RemoteDesktopHelperRequest::Text { text });
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_remote_desktop_mouse_move(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.remote_desktop_session_entity(tab_id, cx)
            .is_some_and(|session| {
                session.update(cx, |session, _cx| session.handle_mouse_move(position))
            })
    }

    pub(in crate::workspace) fn handle_remote_desktop_mouse_button(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        button: RemoteDesktopMouseButton,
        state: RemoteDesktopMouseButtonState,
        cx: &mut Context<Self>,
    ) -> bool {
        self.remote_desktop_session_entity(tab_id, cx)
            .is_some_and(|session| {
                session.update(cx, |session, _cx| {
                    session.handle_mouse_button(position, button, state)
                })
            })
    }

    pub(in crate::workspace) fn handle_remote_desktop_gpui_mouse_button(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        button: gpui::MouseButton,
        state: RemoteDesktopMouseButtonState,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(button) = remote_desktop_mouse_button_from_gpui(button) else {
            return false;
        };
        self.handle_remote_desktop_mouse_button(tab_id, position, button, state, cx)
    }

    pub(in crate::workspace) fn handle_remote_desktop_mouse_button_release_out(
        &mut self,
        tab_id: TabId,
        button: RemoteDesktopMouseButton,
        cx: &mut Context<Self>,
    ) -> bool {
        self.remote_desktop_session_entity(tab_id, cx)
            .is_some_and(|session| {
                session.update(cx, |session, _cx| session.release_mouse_button_out(button))
            })
    }

    pub(in crate::workspace) fn handle_remote_desktop_wheel(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        delta: &gpui::ScrollDelta,
        cx: &mut Context<Self>,
    ) -> bool {
        self.remote_desktop_session_entity(tab_id, cx)
            .is_some_and(|session| {
                session.update(cx, |session, _cx| session.handle_wheel(position, delta))
            })
    }

    pub(in crate::workspace) fn handle_remote_desktop_key(
        &mut self,
        tab_id: TabId,
        keystroke: &gpui::Keystroke,
        state: RemoteDesktopKeyState,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.handle_key(keystroke, state));
        }
    }

    pub(in crate::workspace) fn sync_remote_desktop_modifiers(
        &mut self,
        tab_id: TabId,
        modifiers: gpui::Modifiers,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.sync_modifiers(modifiers));
        }
    }

    pub(in crate::workspace) fn sync_remote_desktop_lock_keys(
        &mut self,
        tab_id: TabId,
        capslock: gpui::Capslock,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.sync_lock_keys(capslock));
        }
    }

    pub(in crate::workspace) fn sync_remote_desktop_lock_key_press(
        &mut self,
        tab_id: TabId,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| {
                session.sync_lock_key_press(keystroke);
            });
        }
    }

    pub(in crate::workspace) fn forward_remote_desktop_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        self.sync_remote_desktop_modifiers(tab_id, event.modifiers, cx);
        self.sync_remote_desktop_lock_keys(tab_id, event.capslock, cx);
        true
    }

    pub(in crate::workspace) fn forward_remote_desktop_key_from_capture(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
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
        self.handle_remote_desktop_key(
            tab_id,
            &event.keystroke,
            RemoteDesktopKeyState::Pressed,
            cx,
        );
        self.sync_remote_desktop_lock_key_press(tab_id, &event.keystroke, cx);
        true
    }

    pub(in crate::workspace) fn forward_remote_desktop_key_up(
        &mut self,
        event: &KeyUpEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        if remote_desktop_paste_shortcut(&event.keystroke)
            || remote_desktop_copy_shortcut(&event.keystroke)
        {
            return true;
        }
        self.handle_remote_desktop_key(
            tab_id,
            &event.keystroke,
            RemoteDesktopKeyState::Released,
            cx,
        );
        true
    }

    pub(in crate::workspace) fn copy_remote_desktop_from_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        self.release_remote_desktop_shortcut_modifiers(tab_id, keystroke, cx);
        self.copy_remote_desktop(cx)
    }

    pub(in crate::workspace) fn copy_remote_desktop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        self.send_remote_desktop_control_shortcut(tab_id, "c", cx);
        true
    }

    pub(in crate::workspace) fn paste_remote_desktop_from_keystroke(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        self.release_remote_desktop_shortcut_modifiers(tab_id, keystroke, cx);
        self.paste_remote_desktop(cx)
    }

    pub(in crate::workspace) fn paste_remote_desktop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        let Some(item) = cx.read_from_clipboard() else {
            return true;
        };
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.paste_clipboard(item));
        }
        true
    }

    pub(in crate::workspace) fn release_remote_desktop_shortcut_modifiers(
        &mut self,
        tab_id: TabId,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) {
        if let Some(session_entity) = self.remote_desktop_session_entity(tab_id, cx) {
            session_entity.update(cx, |session, _cx| {
                session.release_shortcut_modifiers(keystroke);
            });
        }
    }

    pub(in crate::workspace) fn send_remote_desktop_control_shortcut(
        &mut self,
        tab_id: TabId,
        code: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, _cx| session.send_control_shortcut(code));
        }
    }

    pub(in crate::workspace) fn active_remote_desktop_tab_id(&self, cx: &App) -> Option<TabId> {
        self.active_tab(cx)
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
