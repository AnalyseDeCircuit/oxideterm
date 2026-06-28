use std::{
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use oxideterm_gpui_remote_desktop::{
    RemoteDesktopViewState, SharedRemoteDesktopGeometry, remote_desktop_surface_with_geometry,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
};
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopFakeBackend,
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState,
    RemoteDesktopMouseButton, RemoteDesktopMouseButtonState, RemoteDesktopProtocol,
    RemoteDesktopProviderManifest, RemoteDesktopSecret, RemoteDesktopSessionStatus,
    RemoteDesktopSize, RemoteDesktopWheelDelta, builtin_preview_provider_registry,
    builtin_provider_registry, read_event_line, write_request_line,
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};

use super::*;

const REMOTE_DESKTOP_INITIAL_WIDTH: u32 = 1280;
const REMOTE_DESKTOP_INITIAL_HEIGHT: u32 = 720;
const REMOTE_DESKTOP_SCROLL_LINE: f32 = 38.0;
const REMOTE_DESKTOP_RESIZE_DEBOUNCE: Duration = Duration::from_millis(120);

#[derive(Debug)]
pub(super) enum RemoteDesktopWorkerDelivery {
    FrameReady {
        tab_id: TabId,
    },
    Event {
        tab_id: TabId,
        event: RemoteDesktopHelperEvent,
    },
    TransportFailed {
        tab_id: TabId,
        message: String,
    },
}

#[derive(Clone, Default)]
struct RemoteDesktopFrameDeliverySlot {
    frame: Arc<Mutex<Option<RemoteDesktopHelperEvent>>>,
    queued: Arc<AtomicBool>,
}

impl RemoteDesktopFrameDeliverySlot {
    fn push(
        &self,
        tab_id: TabId,
        event: RemoteDesktopHelperEvent,
        delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    ) {
        {
            let Ok(mut frame) = self.frame.lock() else {
                return;
            };
            if let Some(existing) = frame.as_mut() {
                merge_remote_desktop_frame_event(existing, event);
            } else {
                *frame = Some(event);
            }
        }

        // A single queued marker is enough; newer frames replace the slot until
        // the UI thread catches up and acknowledges delivery.
        if !self.queued.swap(true, Ordering::AcqRel) {
            let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::FrameReady { tab_id });
        }
    }

    fn take(&self) -> Option<RemoteDesktopHelperEvent> {
        self.frame.lock().ok()?.take()
    }

    fn complete_delivery(
        &self,
        tab_id: TabId,
        delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    ) {
        self.queued.store(false, Ordering::Release);
        let has_pending_frame = self
            .frame
            .lock()
            .map(|frame| frame.is_some())
            .unwrap_or(false);
        if has_pending_frame && !self.queued.swap(true, Ordering::AcqRel) {
            let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::FrameReady { tab_id });
        }
    }
}

pub(super) struct RemoteDesktopSession {
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    state: RemoteDesktopViewState,
    geometry: SharedRemoteDesktopGeometry,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    request_tx: mpsc::Sender<RemoteDesktopHelperRequest>,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopSize>,
    resize_generation: Arc<AtomicU64>,
}

impl RemoteDesktopSession {
    fn new(
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        frame_slot: RemoteDesktopFrameDeliverySlot,
        request_tx: mpsc::Sender<RemoteDesktopHelperRequest>,
    ) -> Self {
        let state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
            .with_read_only(profile.read_only);
        Self {
            profile,
            provider,
            state,
            geometry: SharedRemoteDesktopGeometry::default(),
            frame_slot,
            request_tx,
            last_viewport_size: None,
            last_sent_resize: None,
            resize_generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_remote_desktop_preview_tab(
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

    pub(super) fn open_remote_desktop_connection_tab(
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

    fn open_remote_desktop_tab(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        title: String,
        password: Option<RemoteDesktopSecret>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = self.alloc_tab_id();
        let frame_slot = RemoteDesktopFrameDeliverySlot::default();
        let request_tx = self.spawn_remote_desktop_worker(
            tab_id,
            profile.clone(),
            provider.clone(),
            password,
            frame_slot.clone(),
        );
        let session = RemoteDesktopSession::new(profile, provider, frame_slot, request_tx);

        self.remote_desktop_sessions.insert(tab_id, session);
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
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn render_remote_desktop_surface(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.remote_desktop_sessions.get(&tab_id) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("remote_desktop.session_missing"))
                .into_any_element();
        };

        let geometry = session.geometry.clone();
        div()
            .size_full()
            .relative()
            .child(remote_desktop_surface_with_geometry(
                &self.tokens,
                &session.state,
                Some(geometry),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_remote_desktop_mouse_button(
                        tab_id,
                        event.position,
                        RemoteDesktopMouseButton::Left,
                        RemoteDesktopMouseButtonState::Pressed,
                    );
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_remote_desktop_mouse_button(
                        tab_id,
                        event.position,
                        RemoteDesktopMouseButton::Right,
                        RemoteDesktopMouseButtonState::Pressed,
                    );
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_remote_desktop_mouse_button(
                        tab_id,
                        event.position,
                        RemoteDesktopMouseButton::Middle,
                        RemoteDesktopMouseButtonState::Pressed,
                    );
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                    this.handle_remote_desktop_mouse_button(
                        tab_id,
                        event.position,
                        RemoteDesktopMouseButton::Left,
                        RemoteDesktopMouseButtonState::Released,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                    this.handle_remote_desktop_mouse_button(
                        tab_id,
                        event.position,
                        RemoteDesktopMouseButton::Right,
                        RemoteDesktopMouseButtonState::Released,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, event: &MouseUpEvent, _window, cx| {
                    this.handle_remote_desktop_mouse_button(
                        tab_id,
                        event.position,
                        RemoteDesktopMouseButton::Middle,
                        RemoteDesktopMouseButtonState::Released,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    this.handle_remote_desktop_mouse_move(tab_id, event.position);
                    cx.stop_propagation();
                }),
            )
            .on_scroll_wheel(
                cx.listener(move |this, event: &ScrollWheelEvent, _window, cx| {
                    this.handle_remote_desktop_wheel(tab_id, event.position, &event.delta);
                    cx.stop_propagation();
                }),
            )
            .child(self.render_remote_desktop_toolbar(tab_id, cx))
            .into_any_element()
    }

    pub(super) fn poll_remote_desktop_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut changed = self.schedule_remote_desktop_viewport_resizes();
        while let Ok(delivery) = self.remote_desktop_worker_rx.try_recv() {
            match delivery {
                RemoteDesktopWorkerDelivery::FrameReady { tab_id } => {
                    if self.apply_remote_desktop_frame_ready(tab_id) {
                        changed = true;
                    }
                }
                RemoteDesktopWorkerDelivery::Event { tab_id, event } => {
                    if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
                        if let RemoteDesktopHelperEvent::ClipboardText { text } = &event {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        }
                        session.state.apply_event(event);
                        changed = true;
                    }
                }
                RemoteDesktopWorkerDelivery::TransportFailed { tab_id, message } => {
                    if self.apply_remote_desktop_frame_ready(tab_id) {
                        changed = true;
                    }
                    if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
                        session
                            .state
                            .apply_event(RemoteDesktopHelperEvent::ConnectionFailure { message });
                        changed = true;
                    }
                }
            }
        }

        if changed {
            cx.notify();
        }
    }

    pub(super) fn close_remote_desktop_tab(&mut self, tab_id: TabId) {
        if let Some(session) = self.remote_desktop_sessions.remove(&tab_id) {
            // The helper owns external resources. Always send a protocol-level
            // close before dropping the channel so real helpers can disconnect.
            let _ = session.request_tx.send(RemoteDesktopHelperRequest::Close);
        }
    }

    fn spawn_remote_desktop_worker(
        &self,
        tab_id: TabId,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password: Option<RemoteDesktopSecret>,
        frame_slot: RemoteDesktopFrameDeliverySlot,
    ) -> mpsc::Sender<RemoteDesktopHelperRequest> {
        let (request_tx, request_rx) = mpsc::channel();
        let delivery_tx = self.remote_desktop_worker_tx.clone();
        thread::Builder::new()
            .name(format!("remote-desktop-{}", tab_id.0))
            .spawn(move || {
                run_remote_desktop_worker(
                    tab_id,
                    profile,
                    provider,
                    password,
                    frame_slot,
                    request_rx,
                    delivery_tx,
                );
            })
            .expect("failed to start remote desktop worker");
        request_tx
    }

    fn render_remote_desktop_toolbar(&self, tab_id: TabId, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.remote_desktop_sessions.get(&tab_id) else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let status = session.state.snapshot().status;
        let reconnect_disabled = matches!(status, RemoteDesktopSessionStatus::Connecting);
        let label = format!(
            "{} · {}:{}",
            session.provider.name, session.profile.endpoint.host, session.profile.endpoint.port
        );

        div()
            .absolute()
            .top(px(14.0))
            .right(px(14.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.two))
            .py(px(self.tokens.spacing.one))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_panel << 8) | 0xdd))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .child(
                div()
                    .max_w(px(360.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("remote_desktop.reconnect"),
                None,
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: reconnect_disabled,
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(move |this, _event, _window, cx| {
                    this.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Reconnect);
                    cx.notify();
                }),
            ))
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("remote_desktop.disconnect"),
                None,
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Destructive,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(move |this, _event, _window, cx| {
                    this.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Close);
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    fn send_remote_desktop_request(&mut self, tab_id: TabId, request: RemoteDesktopHelperRequest) {
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            if let RemoteDesktopHelperRequest::Resize { size } = request {
                session.state.mark_resize_requested(size);
            }
            let _ = session.request_tx.send(request);
        }
    }

    fn schedule_remote_desktop_viewport_resizes(&mut self) -> bool {
        let mut changed = false;
        for session in self.remote_desktop_sessions.values_mut() {
            let snapshot = session.state.snapshot();
            if snapshot.status != RemoteDesktopSessionStatus::Connected {
                continue;
            }
            let Some(viewport_size) = session.geometry.viewport_size() else {
                continue;
            };
            let size = RemoteDesktopSize::clamped(viewport_size.width, viewport_size.height);
            if Some(size) == session.last_viewport_size {
                continue;
            }
            session.last_viewport_size = Some(size);
            if Some(size) == snapshot.size || Some(size) == session.last_sent_resize {
                continue;
            }

            session.last_sent_resize = Some(size);
            session.state.mark_resize_requested(size);
            changed = true;

            let generation = session.resize_generation.fetch_add(1, Ordering::Relaxed) + 1;
            let resize_generation = session.resize_generation.clone();
            let request_tx = session.request_tx.clone();
            thread::Builder::new()
                .name("remote-desktop-resize-debounce".to_string())
                .spawn(move || {
                    thread::sleep(REMOTE_DESKTOP_RESIZE_DEBOUNCE);
                    if resize_generation.load(Ordering::Relaxed) == generation {
                        let _ = request_tx.send(RemoteDesktopHelperRequest::Resize { size });
                    }
                })
                .ok();
        }
        changed
    }

    fn apply_remote_desktop_frame_ready(&mut self, tab_id: TabId) -> bool {
        let delivery_tx = self.remote_desktop_worker_tx.clone();
        let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) else {
            return false;
        };
        let frame_slot = session.frame_slot.clone();
        let mut changed = false;
        if let Some(event) = frame_slot.take() {
            session.state.apply_event(event);
            changed = true;
        }
        frame_slot.complete_delivery(tab_id, &delivery_tx);
        changed
    }

    fn handle_remote_desktop_mouse_move(&mut self, tab_id: TabId, position: Point<Pixels>) {
        let Some(point) = self
            .remote_desktop_sessions
            .get(&tab_id)
            .and_then(|session| session.geometry.map_window_point(position))
        else {
            return;
        };
        self.send_remote_desktop_request(
            tab_id,
            RemoteDesktopHelperRequest::MouseMove {
                x: point.x,
                y: point.y,
            },
        );
    }

    fn handle_remote_desktop_mouse_button(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        button: RemoteDesktopMouseButton,
        state: RemoteDesktopMouseButtonState,
    ) {
        let Some(point) = self
            .remote_desktop_sessions
            .get(&tab_id)
            .and_then(|session| session.geometry.map_window_point(position))
        else {
            return;
        };
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
    }

    fn handle_remote_desktop_wheel(
        &mut self,
        tab_id: TabId,
        position: Point<Pixels>,
        delta: &gpui::ScrollDelta,
    ) {
        if self
            .remote_desktop_sessions
            .get(&tab_id)
            .and_then(|session| session.geometry.map_window_point(position))
            .is_none()
        {
            return;
        }

        let delta = match delta {
            gpui::ScrollDelta::Pixels(point) => RemoteDesktopWheelDelta {
                x: f32::from(point.x),
                y: f32::from(point.y),
            },
            gpui::ScrollDelta::Lines(point) => RemoteDesktopWheelDelta {
                x: point.x * REMOTE_DESKTOP_SCROLL_LINE,
                y: point.y * REMOTE_DESKTOP_SCROLL_LINE,
            },
        };
        self.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Wheel { delta });
    }

    fn handle_remote_desktop_key(
        &mut self,
        tab_id: TabId,
        keystroke: &gpui::Keystroke,
        state: RemoteDesktopKeyState,
    ) {
        let modifiers = keystroke.modifiers;
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

    pub(super) fn forward_remote_desktop_key_from_capture(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        if remote_desktop_paste_shortcut(&event.keystroke) {
            self.paste_remote_desktop(cx);
            return true;
        }
        if remote_desktop_copy_shortcut(&event.keystroke) {
            self.copy_remote_desktop(cx);
            return true;
        }
        self.handle_remote_desktop_key(tab_id, &event.keystroke, RemoteDesktopKeyState::Pressed);
        true
    }

    pub(super) fn forward_remote_desktop_key_up(&mut self, event: &KeyUpEvent) -> bool {
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

    pub(super) fn copy_remote_desktop(&mut self, _cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        self.send_remote_desktop_control_shortcut(tab_id, "c");
        true
    }

    pub(super) fn paste_remote_desktop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
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

    fn send_remote_desktop_control_shortcut(&mut self, tab_id: TabId, code: &str) {
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

    fn active_remote_desktop_tab_id(&self) -> Option<TabId> {
        self.active_tab()
            .filter(|tab| tab.kind == TabKind::RemoteDesktop)
            .map(|tab| tab.id)
    }

    fn remote_desktop_preview_tab_title(&self, protocol: RemoteDesktopProtocol) -> String {
        match protocol {
            RemoteDesktopProtocol::Rdp => self.i18n.t("remote_desktop.rdp_preview_title"),
            RemoteDesktopProtocol::Vnc => self.i18n.t("remote_desktop.vnc_preview_title"),
        }
    }
}

fn remote_desktop_paste_shortcut(keystroke: &gpui::Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    keystroke.key.eq_ignore_ascii_case("v")
        && !modifiers.alt
        && (modifiers.platform || modifiers.control)
}

fn remote_desktop_copy_shortcut(keystroke: &gpui::Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    keystroke.key.eq_ignore_ascii_case("c")
        && !modifiers.alt
        && (modifiers.platform || modifiers.control)
}

fn is_remote_desktop_frame_event(event: &RemoteDesktopHelperEvent) -> bool {
    matches!(
        event,
        RemoteDesktopHelperEvent::Frame { .. } | RemoteDesktopHelperEvent::FrameUpdate { .. }
    )
}

fn merge_remote_desktop_frame_event(
    existing: &mut RemoteDesktopHelperEvent,
    incoming: RemoteDesktopHelperEvent,
) {
    match existing {
        RemoteDesktopHelperEvent::Frame { frame } => match incoming {
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                if !frame.apply_update(&update) {
                    *existing = RemoteDesktopHelperEvent::FrameUpdate { update };
                }
            }
            incoming => {
                *existing = incoming;
            }
        },
        RemoteDesktopHelperEvent::FrameUpdate { update } => match incoming {
            RemoteDesktopHelperEvent::FrameUpdate {
                update: incoming_update,
            } => {
                if !update.merge(&incoming_update) {
                    *existing = RemoteDesktopHelperEvent::FrameUpdate {
                        update: incoming_update,
                    };
                }
            }
            incoming => {
                *existing = incoming;
            }
        },
        slot => {
            *slot = incoming;
        }
    }
}

fn preview_remote_desktop_profile(
    protocol: RemoteDesktopProtocol,
) -> RemoteDesktopConnectionProfile {
    let label = match protocol {
        RemoteDesktopProtocol::Rdp => "RDP Preview",
        RemoteDesktopProtocol::Vnc => "VNC Preview",
    };

    RemoteDesktopConnectionProfile {
        id: format!("preview-{}", protocol.provider_id()),
        label: label.to_string(),
        protocol,
        endpoint: RemoteDesktopEndpoint::for_protocol("preview.local", protocol),
        username: None,
        domain: None,
        credential_ref: None,
        read_only: false,
    }
}

fn run_remote_desktop_worker(
    tab_id: TabId,
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    password: Option<RemoteDesktopSecret>,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    if let Ok((mut child, mut stdin)) = spawn_remote_desktop_helper(&provider) {
        let stdout = child.stdout.take();
        let connect = connect_request(&profile, password);
        if let Err(error) = write_request_line(&mut stdin, &connect) {
            let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::TransportFailed {
                tab_id,
                message: error.to_string(),
            });
            return;
        }
        if let Some(stdout) = stdout {
            let reader_tx = delivery_tx.clone();
            let reader_frame_slot = frame_slot.clone();
            thread::Builder::new()
                .name(format!("remote-desktop-reader-{}", tab_id.0))
                .spawn(move || {
                    read_remote_desktop_events(tab_id, stdout, reader_tx, reader_frame_slot)
                })
                .ok();
        }

        run_remote_desktop_writer(tab_id, &mut stdin, request_rx, delivery_tx.clone());
        let exit_code = child.wait().ok().and_then(|status| status.code());
        let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::Event {
            tab_id,
            event: RemoteDesktopHelperEvent::Terminated { exit_code },
        });
        return;
    }

    run_in_process_fake_remote_desktop(tab_id, profile, frame_slot, request_rx, delivery_tx);
}

fn spawn_remote_desktop_helper(
    provider: &RemoteDesktopProviderManifest,
) -> Result<(Child, ChildStdin), std::io::Error> {
    let resolved = resolve_remote_desktop_helper_command(&provider.entry.command);
    let mut command = Command::new(&resolved.command);
    command
        .args(&resolved.prefix_args)
        .args(&provider.entry.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(working_dir) = provider.entry.working_dir.as_ref() {
        command.current_dir(working_dir);
    } else if let Some(working_dir) = resolved.working_dir.as_ref() {
        command.current_dir(working_dir);
    }
    let mut child = command.spawn()?;
    let stdin = child.stdin.take().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "remote desktop helper stdin is unavailable",
        )
    })?;
    Ok((child, stdin))
}

struct ResolvedRemoteDesktopHelper {
    command: PathBuf,
    prefix_args: Vec<String>,
    working_dir: Option<PathBuf>,
}

fn resolve_remote_desktop_helper_command(command: &str) -> ResolvedRemoteDesktopHelper {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 || command_path.is_absolute() {
        return ResolvedRemoteDesktopHelper {
            command: command_path.to_path_buf(),
            prefix_args: Vec::new(),
            working_dir: None,
        };
    }

    if let Some(resolved) = development_remote_desktop_helper_command(command) {
        return resolved;
    }

    for candidate in bundled_remote_desktop_helper_candidates(command) {
        if candidate.exists() {
            return ResolvedRemoteDesktopHelper {
                command: candidate,
                prefix_args: Vec::new(),
                working_dir: None,
            };
        }
    }

    ResolvedRemoteDesktopHelper {
        command: PathBuf::from(command),
        prefix_args: Vec::new(),
        working_dir: None,
    }
}

fn development_remote_desktop_helper_command(command: &str) -> Option<ResolvedRemoteDesktopHelper> {
    if !cfg!(debug_assertions)
        || !matches!(command, "oxideterm-rdp-helper" | "oxideterm-vnc-helper")
    {
        return None;
    }

    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)?
        .to_path_buf();
    if !workspace_root
        .join("crates")
        .join(command)
        .join("Cargo.toml")
        .exists()
    {
        return None;
    }

    let mut prefix_args = vec![
        "run".to_string(),
        "--quiet".to_string(),
        "-p".to_string(),
        command.to_string(),
    ];
    if command == "oxideterm-rdp-helper" && development_legacy_rdp_feature_available() {
        prefix_args.extend(["--features".to_string(), "legacy-freerdp".to_string()]);
    }
    prefix_args.push("--".to_string());

    // Debug app runs should execute the current helper source, not a stale
    // helper binary left next to the app from an earlier build.
    Some(ResolvedRemoteDesktopHelper {
        command: std::env::var_os("CARGO")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("cargo")),
        prefix_args,
        working_dir: Some(workspace_root),
    })
}

fn development_legacy_rdp_feature_available() -> bool {
    std::process::Command::new("pkg-config")
        .args(["--exists", "freerdp-client2 >= 2.4"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn bundled_remote_desktop_helper_candidates(command: &str) -> Vec<PathBuf> {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return Vec::new();
    };
    let helper_name = platform_helper_binary_name(command);
    let target_dirs = helper_target_resource_dirs();
    let mut roots = vec![
        exe_dir.join("resources"),
        exe_dir.join("..").join("Resources"),
    ];

    // Development builds keep helper binaries next to the app under target/*.
    roots.push(exe_dir.clone());

    let mut candidates = Vec::new();
    for root in roots {
        for target_dir in target_dirs {
            candidates.push(root.join("helpers").join(target_dir).join(&helper_name));
        }
        candidates.push(root.join("helpers").join(&helper_name));
        candidates.push(root.join(&helper_name));
    }
    candidates
}

fn platform_helper_binary_name(command: &str) -> String {
    if cfg!(target_os = "windows") && !command.ends_with(".exe") {
        format!("{command}.exe")
    } else {
        command.to_string()
    }
}

fn helper_target_resource_dirs() -> &'static [&'static str] {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        // Release packaging stores helpers under Cargo target triples. The
        // shorthand names remain fallbacks for older preview resource layouts.
        ("macos", "x86_64") => &["x86_64-apple-darwin", "macos_x64"],
        ("macos", "aarch64") => &["aarch64-apple-darwin", "macos_arm64"],
        ("windows", "x86_64") => &["x86_64-pc-windows-msvc", "windows_x64"],
        ("windows", "aarch64") => &["aarch64-pc-windows-msvc", "windows_arm64"],
        ("linux", "x86_64") => &["x86_64-unknown-linux-gnu", "linux_x64"],
        ("linux", "aarch64") => &["aarch64-unknown-linux-gnu", "linux_arm64"],
        _ => &[std::env::consts::ARCH],
    }
}

fn read_remote_desktop_events(
    tab_id: TabId,
    stdout: impl std::io::Read,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
    frame_slot: RemoteDesktopFrameDeliverySlot,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_event_line(&mut reader) {
            Ok(Some(event)) => {
                deliver_remote_desktop_worker_event(tab_id, event, &delivery_tx, &frame_slot);
            }
            Ok(None) => break,
            Err(error) => {
                let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    message: error.to_string(),
                });
                break;
            }
        }
    }
}

fn deliver_remote_desktop_worker_event(
    tab_id: TabId,
    event: RemoteDesktopHelperEvent,
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    frame_slot: &RemoteDesktopFrameDeliverySlot,
) {
    if is_remote_desktop_frame_event(&event) {
        frame_slot.push(tab_id, event, delivery_tx);
    } else {
        let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::Event { tab_id, event });
    }
}

fn run_remote_desktop_writer(
    tab_id: TabId,
    stdin: &mut impl Write,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    for request in request_rx {
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        if let Err(error) = write_request_line(stdin, &request) {
            let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::TransportFailed {
                tab_id,
                message: error.to_string(),
            });
            return;
        }
        if should_close {
            return;
        }
    }
}

fn run_in_process_fake_remote_desktop(
    tab_id: TabId,
    profile: RemoteDesktopConnectionProfile,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    let mut backend = RemoteDesktopFakeBackend::new(profile.protocol);
    for event in backend.handle_request(connect_request(&profile, None)) {
        deliver_remote_desktop_worker_event(tab_id, event, &delivery_tx, &frame_slot);
    }

    for request in request_rx {
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        for event in backend.handle_request(request) {
            deliver_remote_desktop_worker_event(tab_id, event, &delivery_tx, &frame_slot);
        }
        if should_close {
            break;
        }
    }
}

fn connect_request(
    profile: &RemoteDesktopConnectionProfile,
    password: Option<RemoteDesktopSecret>,
) -> RemoteDesktopHelperRequest {
    RemoteDesktopHelperRequest::Connect {
        protocol: profile.protocol,
        endpoint: profile.endpoint.clone(),
        username: profile.username.clone(),
        // Runtime-only credentials cross the UI/backend boundary here. They
        // are sent to the helper process and never stored in the profile model.
        password,
        domain: profile.domain.clone(),
        size: RemoteDesktopSize::clamped(
            REMOTE_DESKTOP_INITIAL_WIDTH,
            REMOTE_DESKTOP_INITIAL_HEIGHT,
        ),
        read_only: profile.read_only,
    }
}
