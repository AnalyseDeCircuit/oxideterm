// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(in crate::workspace) fn remote_desktop_keyboard_capture(
    root: gpui::Stateful<gpui::Div>,
    session: Entity<RemoteDesktopSessionEntity>,
    overrides: serde_json::Map<String, serde_json::Value>,
) -> gpui::Stateful<gpui::Div> {
    let key_down_session = session.clone();
    let key_up_session = session.clone();
    let key_down_overrides = overrides.clone();
    // Bind the native window directly to its mounted session, independently of the main tab.
    root.capture_key_down(move |event, window, cx| {
        key_down_session.update(cx, |session, cx| {
            session.forward_key_down(event, &key_down_overrides, cx);
        });
        window.prevent_default();
        cx.stop_propagation();
    })
    .on_key_up(move |event, _window, cx| {
        key_up_session.update(cx, |session, _cx| {
            session.forward_key_up(event, &overrides);
        });
        cx.stop_propagation();
    })
    .on_modifiers_changed(move |event, _window, cx| {
        session.update(cx, |session, _cx| {
            session.sync_modifiers(event.modifiers);
            session.sync_lock_keys(event.capslock);
        });
        cx.stop_propagation();
    })
}

impl RemoteDesktopSessionEntity {
    fn forward_key_down(
        &mut self,
        event: &KeyDownEvent,
        overrides: &serde_json::Map<String, serde_json::Value>,
        cx: &mut App,
    ) {
        if remote_desktop_paste_shortcut(&event.keystroke, overrides) {
            self.release_shortcut_modifiers(&event.keystroke);
            if let Some(item) = cx.read_from_clipboard() {
                self.paste_clipboard(item);
            }
        } else if remote_desktop_copy_shortcut(&event.keystroke, overrides) {
            self.release_shortcut_modifiers(&event.keystroke);
            self.send_control_shortcut("c");
        } else {
            self.handle_key(&event.keystroke, RemoteDesktopKeyState::Pressed);
            self.sync_lock_key_press(&event.keystroke);
        }
    }

    fn forward_key_up(
        &mut self,
        event: &KeyUpEvent,
        overrides: &serde_json::Map<String, serde_json::Value>,
    ) {
        if !remote_desktop_paste_shortcut(&event.keystroke, overrides)
            && !remote_desktop_copy_shortcut(&event.keystroke, overrides)
        {
            self.handle_key(&event.keystroke, RemoteDesktopKeyState::Released);
        }
    }

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
        if self.profile.protocol == RemoteDesktopProtocol::Rdp {
            self.send_request(RemoteDesktopHelperRequest::PasteText { text: text.into() });
        } else {
            self.send_request(RemoteDesktopHelperRequest::ClipboardText { text: text.clone() });
            self.send_request(RemoteDesktopHelperRequest::Text { text });
        }
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
        let Some(session) = self
            .active_remote_desktop_tab_id(cx)
            .and_then(|tab_id| self.remote_desktop_session_entity(tab_id, cx))
        else {
            return false;
        };
        let overrides = &self.settings_store.settings().keybindings.overrides;
        session.update(cx, |session, cx| {
            session.forward_key_down(event, overrides, cx)
        });
        true
    }

    pub(in crate::workspace) fn forward_remote_desktop_key_up(
        &mut self,
        event: &KeyUpEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self
            .active_remote_desktop_tab_id(cx)
            .and_then(|tab_id| self.remote_desktop_session_entity(tab_id, cx))
        else {
            return false;
        };
        let overrides = &self.settings_store.settings().keybindings.overrides;
        session.update(cx, |session, _cx| session.forward_key_up(event, overrides));
        true
    }

    pub(in crate::workspace) fn copy_remote_desktop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id(cx) else {
            return false;
        };
        self.send_remote_desktop_control_shortcut(tab_id, "c", cx);
        true
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

#[cfg(test)]
mod clipboard_tests {
    use super::*;
    use gpui::TestAppContext;

    struct RemoteKeyboardWindow {
        session: Entity<RemoteDesktopSessionEntity>,
        focus: FocusHandle,
        surface_focus: FocusHandle,
    }

    impl Render for RemoteKeyboardWindow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            remote_desktop_keyboard_capture(
                div()
                    .id("remote-keyboard-window")
                    .size_full()
                    .track_focus(&self.focus)
                    .child(div().size_full().track_focus(&self.surface_focus)),
                self.session.clone(),
                serde_json::Map::new(),
            )
        }
    }

    #[gpui::test]
    fn remote_window_routes_keyboard_and_clipboard_to_its_own_session(cx: &mut TestAppContext) {
        let mut windows = Vec::new();
        for protocol in [RemoteDesktopProtocol::Rdp, RemoteDesktopProtocol::Vnc] {
            let (tx, rx) = mpsc::channel();
            let handle = cx.add_window(|window, cx| {
                let provider = builtin_preview_provider_registry()
                    .unwrap()
                    .get_for_protocol(protocol)
                    .cloned()
                    .unwrap();
                let session = cx.new(|_| {
                    let mut session = RemoteDesktopSessionEntity::new(
                        TabId(if protocol == RemoteDesktopProtocol::Rdp {
                            71
                        } else {
                            72
                        }),
                        preview_remote_desktop_profile(protocol),
                        provider,
                        None,
                        std::path::PathBuf::new(),
                        RemoteDesktopFrameDeliverySlot::new(),
                        window.window_handle(),
                    );
                    session.worker = Some(RemoteDesktopWorkerOwner {
                        request_tx: Some(tx),
                        worker_thread: None,
                    });
                    session
                });
                let focus = cx.focus_handle();
                let surface_focus = cx.focus_handle();
                window.focus(&surface_focus, cx);
                RemoteKeyboardWindow {
                    session,
                    focus,
                    surface_focus,
                }
            });
            let window = gpui::VisualTestContext::from_window(handle.into(), cx);
            windows.push((window, rx, protocol));
        }
        for index in 0..windows.len() {
            let (window, rx, protocol) = &mut windows[index];
            let stroke = gpui::Keystroke::parse("a").unwrap();
            window.simulate_event(KeyDownEvent {
                keystroke: stroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            window.simulate_event(KeyUpEvent { keystroke: stroke });
            window.simulate_event(ModifiersChangedEvent {
                modifiers: gpui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
                ..Default::default()
            });
            window.simulate_event(ModifiersChangedEvent::default());
            let mut keys = Vec::new();
            let mut lock_states = Vec::new();
            for request in rx.try_iter() {
                match request {
                    RemoteDesktopHelperRequest::Key { key, state } => keys.push((key.code, state)),
                    RemoteDesktopHelperRequest::SynchronizeLockKeys { keys } => {
                        lock_states.push(keys.caps_lock)
                    }
                    _ => panic!("unexpected remote input request"),
                }
            }
            assert_eq!(
                keys,
                vec![
                    ("a".into(), RemoteDesktopKeyState::Pressed),
                    ("a".into(), RemoteDesktopKeyState::Released),
                    ("ShiftLeft".into(), RemoteDesktopKeyState::Pressed),
                    ("ShiftLeft".into(), RemoteDesktopKeyState::Released),
                ]
            );
            assert_eq!(lock_states, vec![false]);
            window.update(|_, app| {
                app.write_to_clipboard(ClipboardItem::new_string("remote paste".into()))
            });
            let paste = gpui::Keystroke::parse(if cfg!(target_os = "macos") {
                "cmd-v"
            } else {
                "ctrl-v"
            })
            .unwrap();
            window.simulate_event(KeyDownEvent {
                keystroke: paste.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            window.simulate_event(KeyUpEvent { keystroke: paste });
            let mut expected = vec![RemoteDesktopHelperRequest::Key {
                key: RemoteDesktopKey {
                    code: if cfg!(target_os = "macos") {
                        "meta"
                    } else {
                        "control"
                    }
                    .into(),
                    text: None,
                    alt: false,
                    ctrl: false,
                    shift: false,
                    meta: false,
                },
                state: RemoteDesktopKeyState::Released,
            }];
            match protocol {
                RemoteDesktopProtocol::Rdp => {
                    expected.push(RemoteDesktopHelperRequest::PasteText {
                        text: "remote paste".into(),
                    })
                }
                RemoteDesktopProtocol::Vnc => {
                    expected.push(RemoteDesktopHelperRequest::ClipboardText {
                        text: "remote paste".into(),
                    });
                    expected.push(RemoteDesktopHelperRequest::Text {
                        text: "remote paste".into(),
                    });
                }
            }
            assert!(
                rx.try_iter().collect::<Vec<_>>() == expected,
                "unexpected {protocol:?} paste requests"
            );
            assert!(
                windows[1 - index].1.try_recv().is_err(),
                "input leaked to another window"
            );
        }
    }

    struct ClipboardTestWindow;
    impl Render for ClipboardTestWindow {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
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
