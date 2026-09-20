// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl RemoteDesktopSessionEntity {
    pub(super) fn send_request(&mut self, request: RemoteDesktopHelperRequest) {
        if self.profile.protocol == RemoteDesktopProtocol::Spice
            && self.profile.read_only
            && !matches!(
                request,
                RemoteDesktopHelperRequest::Close
                    | RemoteDesktopHelperRequest::ReleaseAllInputs
                    | RemoteDesktopHelperRequest::RequestFrame
            )
        {
            return;
        }
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
        if self.profile.protocol == RemoteDesktopProtocol::Spice
            && self.spice.mouse_mode == Some(oxideterm_spice::SpiceMouseMode::Server)
            && !self
                .spice_mouse_capture
                .as_ref()
                .is_some_and(|capture| capture.is_active())
        {
            return None;
        }
        let point = self.geometry.map_window_point(position)?;
        // Servers do not always echo pointer moves. Keep the custom cursor
        // responsive without waiting for a round trip.
        if self.profile.protocol != RemoteDesktopProtocol::Spice
            || self.spice.mouse_mode == Some(oxideterm_spice::SpiceMouseMode::Client)
        {
            self.state.apply_event(RemoteDesktopHelperEvent::Cursor {
                x: point.x,
                y: point.y,
                width: 0,
                height: 0,
            });
        }
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
        self.spice_mouse_capture.take();
        self.spice_motion_remainder = point(px(0.0), px(0.0));
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
        let shortcut_modifiers = RemoteDesktopModifierState {
            ctrl: true,
            ..Default::default()
        };
        // SPICE sends physical scan codes and does not synthesize modifiers from
        // key metadata. Bracket the shortcut while preserving held keys.
        if self.profile.protocol == RemoteDesktopProtocol::Spice {
            for request in
                remote_desktop_modifier_sync_requests(self.last_input_modifiers, shortcut_modifiers)
            {
                self.send_request(request);
            }
        }
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
        if self.profile.protocol == RemoteDesktopProtocol::Spice {
            for request in
                remote_desktop_modifier_sync_requests(shortcut_modifiers, self.last_input_modifiers)
            {
                self.send_request(request);
            }
        }
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
        if self.profile.protocol == RemoteDesktopProtocol::Rdp {
            self.send_request(RemoteDesktopHelperRequest::PasteText { text: text.into() });
        } else {
            self.send_request(RemoteDesktopHelperRequest::ClipboardText { text: text.clone() });
            self.send_request(RemoteDesktopHelperRequest::Text { text });
        }
    }
}

impl WorkspaceApp {
    pub(super) fn capture_spice_mouse(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.remote_desktop_session_entity(tab_id, cx) else {
            return true;
        };
        let state = session.read(cx);
        if state.profile.protocol != RemoteDesktopProtocol::Spice
            || state.spice.mouse_mode != Some(oxideterm_spice::SpiceMouseMode::Server)
        {
            return true;
        }
        if state.profile.read_only {
            return false;
        }
        if state
            .spice_mouse_capture
            .as_ref()
            .is_some_and(|capture| capture.is_active())
        {
            return true;
        }
        session.update(cx, |session, _cx| session.release_inputs());
        match window.capture_relative_mouse() {
            Ok(capture) => {
                session.update(cx, |session, _cx| {
                    session.spice_mouse_capture = Some(capture)
                });
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.spice_mouse_release"),
                    None,
                    TerminalNoticeVariant::Default,
                    cx,
                );
            }
            Err(_) => self.push_command_palette_toast(
                self.i18n.t("remote_desktop.spice_mouse_unavailable"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            ),
        }
        // Acquiring the pointer must not click at the guest's previous cursor position.
        false
    }

    pub(super) fn handle_spice_relative_motion(
        &mut self,
        tab_id: TabId,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(entity) = self.remote_desktop_session_entity(tab_id, cx) else {
            return false;
        };
        entity.update(cx, |session, _cx| {
            if session.profile.protocol != RemoteDesktopProtocol::Spice
                || session.spice.mouse_mode != Some(oxideterm_spice::SpiceMouseMode::Server)
            {
                return false;
            }
            if !session
                .spice_mouse_capture
                .as_ref()
                .is_some_and(|capture| capture.is_active())
            {
                // Native focus loss may have released the guard already. Send
                // releases once, rather than on every uncaptured pointer move.
                if session.spice_mouse_capture.is_some() {
                    session.release_inputs();
                }
                return true;
            }
            if let Some(delta) = event.relative_delta {
                session.spice_motion_remainder.x += delta.x;
                session.spice_motion_remainder.y += delta.y;
                let dx = f32::from(session.spice_motion_remainder.x).trunc() as i32;
                let dy = f32::from(session.spice_motion_remainder.y).trunc() as i32;
                session.spice_motion_remainder.x -= px(dx as f32);
                session.spice_motion_remainder.y -= px(dy as f32);
                if dx != 0 || dy != 0 {
                    session.send_spice_request(SpiceWorkerRequest::PointerMotion {
                        dx,
                        dy,
                        buttons: 0,
                    });
                }
            }
            true
        })
    }

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
        if event.keystroke.key == "escape"
            && event.keystroke.modifiers.control
            && event.keystroke.modifiers.alt
            && let Some(session) = self.remote_desktop_session_entity(tab_id, cx)
            && session.read(cx).spice_mouse_capture.is_some()
        {
            session.update(cx, |session, _cx| session.release_inputs());
            return true;
        }
        if remote_desktop_paste_shortcut(
            &event.keystroke,
            &self.settings_store.settings().keybindings.overrides,
        ) {
            self.paste_remote_desktop_from_keystroke(&event.keystroke, cx);
            return true;
        }
        if remote_desktop_copy_shortcut(
            &event.keystroke,
            &self.settings_store.settings().keybindings.overrides,
        ) {
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
        if remote_desktop_paste_shortcut(
            &event.keystroke,
            &self.settings_store.settings().keybindings.overrides,
        ) || remote_desktop_copy_shortcut(
            &event.keystroke,
            &self.settings_store.settings().keybindings.overrides,
        ) {
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
            RemoteDesktopProtocol::Spice => self.i18n.t("remote_desktop.spice_preview_title"),
        }
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;
    use gpui::TestAppContext;

    struct ClipboardTestWindow;
    impl Render for ClipboardTestWindow {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[gpui::test]
    fn spice_copy_sends_control_edges_and_preserves_held_control(cx: &mut TestAppContext) {
        use oxideterm_spice::SpiceKeyState::{Pressed, Released};
        let window = cx.add_window(|_window, _cx| ClipboardTestWindow);
        let provider = builtin_provider_registry()
            .unwrap()
            .get_for_protocol(RemoteDesktopProtocol::Spice)
            .cloned()
            .unwrap();
        let mut session = RemoteDesktopSessionEntity::new(
            TabId(72),
            preview_remote_desktop_profile(RemoteDesktopProtocol::Spice),
            provider,
            None,
            std::path::PathBuf::new(),
            RemoteDesktopFrameDeliverySlot::new(),
            window.into(),
        );
        let (tx, rx) = mpsc::channel();
        session.worker = Some(RemoteDesktopWorkerOwner {
            spice_request_tx: None,
            request_tx: Some(tx),
            worker_thread: None,
        });
        let mut adapter = SpiceRemoteDesktopAdapter::new(
            RemoteDesktopSize {
                width: 1024,
                height: 768,
            },
            RemoteDesktopMonitorLayout::default(),
            false,
        );
        for held in [false, true] {
            session.sync_modifiers(gpui::Modifiers {
                control: held,
                ..Default::default()
            });
            for request in rx.try_iter() {
                adapter.map_request(request);
            }
            session.send_control_shortcut("c");
            let events: Vec<_> = rx
                .try_iter()
                .flat_map(|request| adapter.map_request(request))
                .map(|event| match event {
                    SpiceWorkerRequest::KeyCode { code, state } => (code, state),
                    _ => panic!("expected keyboard event"),
                })
                .collect();
            let expected = if held {
                vec![(0x2e, Pressed), (0x2e, Released)]
            } else {
                vec![
                    (0x1d, Pressed),
                    (0x2e, Pressed),
                    (0x2e, Released),
                    (0x1d, Released),
                ]
            };
            assert_eq!(events, expected);
            assert_eq!(session.last_input_modifiers.ctrl, held);
        }
    }

    #[gpui::test]
    fn rdp_paste_sends_one_clipboard_transaction_and_respects_text_permission(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|_window, _cx| ClipboardTestWindow);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(RemoteDesktopProtocol::Rdp)
            .cloned()
            .unwrap();
        let mut session = RemoteDesktopSessionEntity::new(
            TabId(71),
            preview_remote_desktop_profile(RemoteDesktopProtocol::Rdp),
            provider,
            None,
            std::path::PathBuf::new(),
            RemoteDesktopFrameDeliverySlot::new(),
            window.into(),
        );
        let (tx, rx) = mpsc::channel();
        session.worker = Some(RemoteDesktopWorkerOwner {
            spice_request_tx: None,
            request_tx: Some(tx),
            worker_thread: None,
        });
        let text = "code\n\t中文🦀\r\n".repeat(512);
        session.paste_clipboard(ClipboardItem::new_string(text.clone()));
        match rx.try_recv().unwrap() {
            RemoteDesktopHelperRequest::PasteText { text: received } => {
                assert_eq!(received.expose_secret(), text)
            }
            _ => panic!("RDP paste must use a single clipboard transaction"),
        }
        assert!(
            rx.try_recv().is_err(),
            "paste must not also inject keyboard text"
        );
        session.profile.session_options.clipboard.text = false;
        session.paste_clipboard(ClipboardItem::new_string("blocked".to_string()));
        assert!(
            rx.try_recv().is_err(),
            "disabled clipboard text must not be sent"
        );
    }
}
