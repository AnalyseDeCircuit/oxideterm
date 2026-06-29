use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, SystemTime},
};

use oxideterm_gpui_remote_desktop::{
    RemoteDesktopMappedPoint, RemoteDesktopViewState, SharedRemoteDesktopGeometry,
    remote_desktop_surface_with_geometry,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
};
use oxideterm_remote_desktop::{
    RemoteDesktopClipboardData, RemoteDesktopClipboardFormat, RemoteDesktopConnectionProfile,
    RemoteDesktopEndpoint, RemoteDesktopErrorCategory, RemoteDesktopFakeBackend,
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState,
    RemoteDesktopLockKeys, RemoteDesktopMouseButton, RemoteDesktopMouseButtonState,
    RemoteDesktopProtocol, RemoteDesktopProviderManifest, RemoteDesktopSecret,
    RemoteDesktopSessionStatus, RemoteDesktopSize, RemoteDesktopWheelDelta,
    builtin_preview_provider_registry, builtin_provider_registry, read_event_line,
    write_request_line,
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};

use super::*;

const REMOTE_DESKTOP_INITIAL_WIDTH: u32 = 1280;
const REMOTE_DESKTOP_INITIAL_HEIGHT: u32 = 720;
const REMOTE_DESKTOP_SCROLL_LINE: f32 = 38.0;
const REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_INTERVAL: Duration = Duration::from_millis(16);
const REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_TICKS: usize = 120;
const REMOTE_DESKTOP_WORKER_WAKE_POLL_INTERVAL: Duration = Duration::from_millis(8);
const REMOTE_DESKTOP_RESIZE_DEBOUNCE: Duration = Duration::from_millis(120);
const REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD: u32 = 16;
const REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT: u32 = 100;
const REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT: u32 = 100;
const REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT: u32 = 500;
const REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER: f32 = 100.0;
const REMOTE_DESKTOP_SCROLL_PIXEL_STEP: f32 = 120.0;

#[derive(Debug)]
pub(super) enum RemoteDesktopWorkerDelivery {
    FrameReady {
        tab_id: TabId,
        generation: u64,
    },
    Event {
        tab_id: TabId,
        generation: u64,
        event: RemoteDesktopHelperEvent,
    },
    TransportFailed {
        tab_id: TabId,
        generation: u64,
        message: String,
    },
}

#[derive(Clone, Default)]
struct RemoteDesktopWorkerWake {
    pending: Arc<AtomicBool>,
}

impl RemoteDesktopWorkerWake {
    fn mark(&self) {
        // Worker threads cannot touch GPUI state directly; this flag gives the
        // foreground task a cheap edge-triggered signal to request a repaint.
        self.pending.store(true, Ordering::Release);
    }

    fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }
}

#[derive(Clone, Default)]
struct RemoteDesktopFrameDeliverySlot {
    frames: Arc<Mutex<VecDeque<RemoteDesktopHelperEvent>>>,
    queued: Arc<AtomicBool>,
}

impl RemoteDesktopFrameDeliverySlot {
    fn push(
        &self,
        tab_id: TabId,
        generation: u64,
        event: RemoteDesktopHelperEvent,
        delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
        worker_wake: &RemoteDesktopWorkerWake,
    ) {
        {
            let Ok(mut frames) = self.frames.lock() else {
                return;
            };
            push_remote_desktop_frame_event(&mut frames, event);
        }

        // A single queued marker is enough because the slot preserves ordered
        // frame events until the UI thread catches up.
        if !self.queued.swap(true, Ordering::AcqRel) {
            send_remote_desktop_worker_delivery(
                delivery_tx,
                worker_wake,
                RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation },
            );
        }
    }

    fn take(&self) -> Option<RemoteDesktopHelperEvent> {
        self.frames.lock().ok()?.pop_front()
    }

    fn complete_delivery(
        &self,
        tab_id: TabId,
        generation: u64,
        delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    ) {
        self.queued.store(false, Ordering::Release);
        let has_pending_frame = self
            .frames
            .lock()
            .map(|frames| !frames.is_empty())
            .unwrap_or(false);
        if has_pending_frame && !self.queued.swap(true, Ordering::AcqRel) {
            let _ =
                delivery_tx.send(RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation });
        }
    }
}

fn send_remote_desktop_worker_delivery(
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    worker_wake: &RemoteDesktopWorkerWake,
    delivery: RemoteDesktopWorkerDelivery,
) {
    worker_wake.mark();
    let _ = delivery_tx.send(delivery);
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RemoteDesktopModifierState {
    // GPUI key events carry aggregate modifier state; mirror that state so the
    // helper can correct missed platform modifier key transitions.
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
}

impl RemoteDesktopModifierState {
    fn from_gpui(modifiers: gpui::Modifiers) -> Self {
        Self {
            shift: modifiers.shift,
            ctrl: modifiers.control,
            alt: modifiers.alt,
            meta: modifiers.platform,
        }
    }
}

pub(super) struct RemoteDesktopSession {
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    password: Option<RemoteDesktopSecret>,
    state: RemoteDesktopViewState,
    geometry: SharedRemoteDesktopGeometry,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    request_tx: Option<mpsc::Sender<RemoteDesktopHelperRequest>>,
    worker_generation: u64,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopResizeRequestState>,
    last_viewport_scale_factor: Option<u32>,
    resize_generation: Arc<AtomicU64>,
    last_input_modifiers: RemoteDesktopModifierState,
    last_lock_keys: Option<RemoteDesktopLockKeys>,
    pressed_mouse_buttons: HashSet<RemoteDesktopMouseButton>,
    wheel_pixel_remainder: RemoteDesktopWheelDelta,
}

impl RemoteDesktopSession {
    fn new(
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password: Option<RemoteDesktopSecret>,
        frame_slot: RemoteDesktopFrameDeliverySlot,
    ) -> Self {
        let mut state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
            .with_read_only(profile.read_only);
        state.apply_event(RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connecting,
            message: None,
        });
        Self {
            profile,
            provider,
            // Runtime credentials are kept only for this tab so a user-visible
            // reconnect can start a fresh helper after the previous one exits.
            password,
            state,
            geometry: SharedRemoteDesktopGeometry::default(),
            frame_slot,
            request_tx: None,
            worker_generation: 0,
            last_viewport_size: None,
            last_sent_resize: None,
            last_viewport_scale_factor: None,
            resize_generation: Arc::new(AtomicU64::new(0)),
            last_input_modifiers: RemoteDesktopModifierState::default(),
            last_lock_keys: None,
            pressed_mouse_buttons: HashSet::new(),
            wheel_pixel_remainder: remote_desktop_empty_wheel_delta(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RemoteDesktopResizeRequestState {
    size: RemoteDesktopSize,
    scale_factor: Option<u32>,
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
        let session = RemoteDesktopSession::new(profile, provider, password, frame_slot);

        if let Some(previous_tab_id) = self.main_window_tabs.active_tab_id {
            self.release_remote_desktop_inputs_for_tab(previous_tab_id);
        }
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
        self.schedule_remote_desktop_initial_layout_probe(tab_id, cx);
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
                    ) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                    if this.handle_remote_desktop_mouse_move(tab_id, event.position) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_scroll_wheel(
                cx.listener(move |this, event: &ScrollWheelEvent, _window, cx| {
                    if this.handle_remote_desktop_wheel(tab_id, event.position, &event.delta) {
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
            .into_any_element()
    }

    pub(super) fn poll_remote_desktop_worker_results(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let scale_factor = Some(remote_desktop_scale_factor_percent(window.scale_factor()));
        let mut changed = self.schedule_remote_desktop_viewport_resizes(scale_factor, cx);
        while let Ok(delivery) = self.remote_desktop_worker_rx.try_recv() {
            match delivery {
                RemoteDesktopWorkerDelivery::FrameReady { tab_id, generation } => {
                    if self.apply_remote_desktop_frame_ready(tab_id, generation) {
                        changed = true;
                    }
                }
                RemoteDesktopWorkerDelivery::Event {
                    tab_id,
                    generation,
                    event,
                } => {
                    if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
                        continue;
                    }
                    if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
                        match &event {
                            RemoteDesktopHelperEvent::ClipboardText { text } => {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            }
                            RemoteDesktopHelperEvent::ClipboardData { data } => {
                                if let Some(item) = remote_desktop_clipboard_item_from_data(data) {
                                    cx.write_to_clipboard(item);
                                }
                            }
                            _ => {}
                        }
                        session.state.apply_event(event);
                        changed = true;
                    }
                }
                RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation,
                    message,
                } => {
                    if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
                        continue;
                    }
                    if self.apply_remote_desktop_frame_ready(tab_id, generation) {
                        changed = true;
                    }
                    if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
                        session
                            .state
                            .apply_event(RemoteDesktopHelperEvent::ConnectionFailure {
                                message,
                                category: Some(RemoteDesktopErrorCategory::Unknown),
                            });
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
            if let Some(request_tx) = session.request_tx {
                let _ = request_tx.send(RemoteDesktopHelperRequest::ReleaseAllInputs);
                let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
            }
        }
    }

    pub(super) fn release_remote_desktop_inputs_for_tab(&mut self, tab_id: TabId) {
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            session.last_input_modifiers = RemoteDesktopModifierState::default();
            session.last_lock_keys = None;
            session.pressed_mouse_buttons.clear();
            session.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
        }
        self.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::ReleaseAllInputs);
    }

    pub(super) fn release_active_remote_desktop_inputs(&mut self) {
        if let Some(tab_id) = self.active_remote_desktop_tab_id() {
            self.release_remote_desktop_inputs_for_tab(tab_id);
        }
    }

    fn focus_remote_desktop_keyboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The remote surface stops root mouse propagation, so it must run the
        // same blur path that an outside workspace click would normally run.
        self.blur_text_inputs(cx);

        let mut changed = self.clear_ime_selection();
        changed |= self.ime_marked_text.take().is_some();
        changed |= self.pending_platform_text_commit.take().is_some();

        let ai_focus_changed = self.ai_chat_input_focused
            || self.ai_chat_footer_focus.is_some()
            || self.ai_model_selector_open
            || self.ai_model_selector_search_focused;
        self.clear_ai_sidebar_keyboard_focus();
        changed |= ai_focus_changed;

        if self.terminal_command_bar_focused
            || self.terminal_command_suggestions_open
            || self.terminal_command_suggestion_highlighted.is_some()
        {
            // Remote desktop clicks are a keyboard ownership boundary. Clear
            // Workspace-local text owners so Enter and IME control keys route
            // to the helper after the surface gains focus.
            self.terminal_command_bar_focused = false;
            self.terminal_command_suggestions_open = false;
            self.terminal_command_suggestion_highlighted = None;
            changed = true;
        }

        if let Some(tab_id) = self.active_remote_desktop_tab_id() {
            self.sync_remote_desktop_lock_keys(tab_id, window.capslock());
        }
        window.focus(&self.focus_handle);
        if changed {
            cx.notify();
        }
    }

    fn spawn_remote_desktop_worker(
        &self,
        tab_id: TabId,
        generation: u64,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password: Option<RemoteDesktopSecret>,
        frame_slot: RemoteDesktopFrameDeliverySlot,
        worker_wake: RemoteDesktopWorkerWake,
        initial_size: RemoteDesktopSize,
        scale_factor: Option<u32>,
    ) -> mpsc::Sender<RemoteDesktopHelperRequest> {
        let (request_tx, request_rx) = mpsc::channel();
        let delivery_tx = self.remote_desktop_worker_tx.clone();
        thread::Builder::new()
            .name(format!("remote-desktop-{}", tab_id.0))
            .spawn(move || {
                run_remote_desktop_worker(
                    tab_id,
                    generation,
                    profile,
                    provider,
                    password,
                    initial_size,
                    scale_factor,
                    frame_slot,
                    worker_wake,
                    request_rx,
                    delivery_tx,
                );
            })
            .expect("failed to start remote desktop worker");
        request_tx
    }

    fn schedule_remote_desktop_worker_wake_poll(
        &self,
        tab_id: TabId,
        generation: u64,
        worker_wake: RemoteDesktopWorkerWake,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |workspace, cx| {
            loop {
                Timer::after(REMOTE_DESKTOP_WORKER_WAKE_POLL_INTERVAL).await;
                let keep_running = workspace
                    .update(cx, |this, cx| {
                        if !this.remote_desktop_worker_generation_matches(tab_id, generation) {
                            return false;
                        }
                        if worker_wake.take() {
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    fn render_remote_desktop_footer(&self, tab_id: TabId, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.remote_desktop_sessions.get(&tab_id) else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let snapshot = session.state.snapshot();
        let status = snapshot.status;
        let status_color = remote_desktop_status_color(&self.tokens, status);
        let reconnect_disabled = remote_desktop_reconnect_mode(status).is_none();
        let label = format!(
            "{} · {}:{}",
            session.provider.name, session.profile.endpoint.host, session.profile.endpoint.port
        );

        div()
            .flex_none()
            .h(px(36.0))
            .px(px(14.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.one))
            .bg(rgb(theme.bg_panel))
            .border_t_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .child(remote_desktop_protocol_chip(
                &self.tokens,
                snapshot.protocol,
            ))
            .child(
                div()
                    .size(px(7.0))
                    .rounded_full()
                    .bg(rgb(status_color))
                    .flex_none(),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.one))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.reconnect"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::RefreshCw,
                            12.0,
                            rgb(theme.text_muted),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: reconnect_disabled,
                            },
                            height: Some(24.0),
                            padding_x: Some(8.0),
                            font_size: Some(self.tokens.metrics.ui_text_xs),
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, _window, cx| {
                            this.reconnect_remote_desktop(tab_id, cx);
                            cx.notify();
                        }),
                    ))
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("remote_desktop.disconnect"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::Power,
                            12.0,
                            rgb(theme.text_muted),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            height: Some(24.0),
                            padding_x: Some(8.0),
                            font_size: Some(self.tokens.metrics.ui_text_xs),
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(move |this, _event, _window, cx| {
                            this.release_remote_desktop_inputs_for_tab(tab_id);
                            this.send_remote_desktop_request(
                                tab_id,
                                RemoteDesktopHelperRequest::Close,
                            );
                            cx.notify();
                        }),
                    )),
            )
            .into_any_element()
    }

    fn send_remote_desktop_request(&mut self, tab_id: TabId, request: RemoteDesktopHelperRequest) {
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            if let RemoteDesktopHelperRequest::Resize { size, .. } = request {
                session.state.mark_resize_requested(size);
            }
            if let Some(request_tx) = session.request_tx.as_ref() {
                let _ = request_tx.send(request);
            } else if matches!(request, RemoteDesktopHelperRequest::Close) {
                session
                    .state
                    .apply_event(RemoteDesktopHelperEvent::Disconnected { reason: None });
            }
        }
    }

    fn reconnect_remote_desktop(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(status) = self
            .remote_desktop_sessions
            .get(&tab_id)
            .map(|session| session.state.snapshot().status)
        else {
            return;
        };

        match remote_desktop_reconnect_mode(status) {
            Some(RemoteDesktopReconnectMode::ProtocolRequest) => {
                self.release_remote_desktop_inputs_for_tab(tab_id);
                self.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Reconnect);
            }
            Some(RemoteDesktopReconnectMode::RestartHelper) => {
                self.release_remote_desktop_inputs_for_tab(tab_id);
                self.restart_remote_desktop_worker(tab_id, cx);
            }
            None => {}
        }
    }

    fn restart_remote_desktop_worker(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some((
            profile,
            provider,
            password,
            generation,
            initial_size,
            scale_factor,
            old_request_tx,
        )) = self.remote_desktop_sessions.get(&tab_id).map(|session| {
            (
                session.profile.clone(),
                session.provider.clone(),
                session.password.clone(),
                next_remote_desktop_worker_generation(session.worker_generation),
                initial_remote_desktop_size_for_session(session),
                session.last_viewport_scale_factor,
                session.request_tx.clone(),
            )
        })
        else {
            return;
        };
        if let Some(old_request_tx) = old_request_tx {
            let _ = old_request_tx.send(RemoteDesktopHelperRequest::Close);
        }

        let frame_slot = RemoteDesktopFrameDeliverySlot::default();
        let worker_wake = RemoteDesktopWorkerWake::default();
        let request_tx = self.spawn_remote_desktop_worker(
            tab_id,
            generation,
            profile.clone(),
            provider,
            password,
            frame_slot.clone(),
            worker_wake.clone(),
            initial_size,
            scale_factor,
        );
        self.schedule_remote_desktop_worker_wake_poll(tab_id, generation, worker_wake, cx);

        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            session.state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
                .with_read_only(profile.read_only);
            session.state.apply_event(RemoteDesktopHelperEvent::Status {
                status: RemoteDesktopSessionStatus::Reconnecting,
                message: None,
            });
            session.frame_slot = frame_slot;
            session.request_tx = Some(request_tx);
            session.worker_generation = generation;
            session.last_viewport_size = Some(initial_size);
            session.last_sent_resize = None;
            session.last_viewport_scale_factor = scale_factor;
            session.resize_generation = Arc::new(AtomicU64::new(0));
            session.last_lock_keys = None;
            session.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
        }
    }

    fn start_remote_desktop_worker_for_session(
        &mut self,
        tab_id: TabId,
        initial_size: RemoteDesktopSize,
        scale_factor: Option<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((profile, provider, password, frame_slot, generation)) = self
            .remote_desktop_sessions
            .get(&tab_id)
            .and_then(|session| {
                if session.request_tx.is_some() {
                    return None;
                }
                Some((
                    session.profile.clone(),
                    session.provider.clone(),
                    session.password.clone(),
                    session.frame_slot.clone(),
                    next_remote_desktop_worker_generation(session.worker_generation),
                ))
            })
        else {
            return false;
        };

        let worker_wake = RemoteDesktopWorkerWake::default();
        let request_tx = self.spawn_remote_desktop_worker(
            tab_id,
            generation,
            profile,
            provider,
            password,
            frame_slot,
            worker_wake.clone(),
            initial_size,
            scale_factor,
        );
        self.schedule_remote_desktop_worker_wake_poll(tab_id, generation, worker_wake, cx);

        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            session.request_tx = Some(request_tx);
            session.worker_generation = generation;
            session.last_viewport_size = Some(initial_size);
            session.last_sent_resize = None;
            session.last_viewport_scale_factor = scale_factor;
            session.last_lock_keys = None;
            session.wheel_pixel_remainder = remote_desktop_empty_wheel_delta();
            session.state.apply_event(RemoteDesktopHelperEvent::Status {
                status: RemoteDesktopSessionStatus::Connecting,
                message: None,
            });
            return true;
        }
        false
    }

    fn remote_desktop_worker_generation_matches(&self, tab_id: TabId, generation: u64) -> bool {
        self.remote_desktop_sessions
            .get(&tab_id)
            .is_some_and(|session| session.worker_generation == generation)
    }

    fn schedule_remote_desktop_viewport_resizes(
        &mut self,
        scale_factor: Option<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut changed = false;
        let mut pending_starts = Vec::new();
        for (tab_id, session) in self.remote_desktop_sessions.iter_mut() {
            let snapshot = session.state.snapshot();
            let Some(viewport_size) = session.geometry.viewport_size() else {
                continue;
            };
            let size = RemoteDesktopSize::clamped(viewport_size.width, viewport_size.height);
            if let Some(scale_factor) = scale_factor {
                session.last_viewport_scale_factor = Some(scale_factor);
            }
            let resize_request = RemoteDesktopResizeRequestState {
                size,
                scale_factor: session.last_viewport_scale_factor,
            };
            if session.request_tx.is_none() {
                if matches!(
                    snapshot.status,
                    RemoteDesktopSessionStatus::Idle
                        | RemoteDesktopSessionStatus::Connecting
                        | RemoteDesktopSessionStatus::Reconnecting
                ) {
                    pending_starts.push((*tab_id, size, session.last_viewport_scale_factor));
                }
                continue;
            }
            if snapshot.status != RemoteDesktopSessionStatus::Connected {
                continue;
            }
            let should_send_resize = remote_desktop_resize_request_needed(
                snapshot.size,
                snapshot.pending_resize,
                session.last_viewport_size,
                session.last_sent_resize,
                size,
                session.last_viewport_scale_factor,
            );
            if Some(size) == session.last_viewport_size && !should_send_resize {
                continue;
            }
            session.last_viewport_size = Some(size);
            if !should_send_resize {
                continue;
            }

            session.last_sent_resize = Some(resize_request);
            session.state.mark_resize_requested(size);
            changed = true;

            let generation = session.resize_generation.fetch_add(1, Ordering::Relaxed) + 1;
            let resize_generation = session.resize_generation.clone();
            let Some(request_tx) = session.request_tx.clone() else {
                continue;
            };
            thread::Builder::new()
                .name("remote-desktop-resize-debounce".to_string())
                .spawn(move || {
                    thread::sleep(REMOTE_DESKTOP_RESIZE_DEBOUNCE);
                    if resize_generation.load(Ordering::Relaxed) == generation {
                        let _ = request_tx.send(RemoteDesktopHelperRequest::Resize {
                            size: resize_request.size,
                            scale_factor: resize_request.scale_factor,
                        });
                    }
                })
                .ok();
        }
        for (tab_id, size, scale_factor) in pending_starts {
            changed |= self.start_remote_desktop_worker_for_session(tab_id, size, scale_factor, cx);
        }
        changed
    }

    fn schedule_remote_desktop_initial_layout_probe(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |workspace, cx| {
            for _ in 0..REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_TICKS {
                Timer::after(REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_INTERVAL).await;
                let done = workspace
                    .update(cx, |this, cx| {
                        let Some(session) = this.remote_desktop_sessions.get(&tab_id) else {
                            return true;
                        };
                        if session.request_tx.is_some() {
                            return true;
                        }

                        // The viewport probe runs during layout, after the
                        // render-time worker poll. Nudge the workspace briefly
                        // so a measured first viewport can start the helper
                        // without waiting for an unrelated repaint.
                        if this.schedule_remote_desktop_viewport_resizes(None, cx) {
                            cx.notify();
                        }

                        this.remote_desktop_sessions
                            .get(&tab_id)
                            .map(|session| session.request_tx.is_some())
                            .unwrap_or(true)
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_remote_desktop_frame_ready(&mut self, tab_id: TabId, generation: u64) -> bool {
        if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
            return false;
        }
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
        frame_slot.complete_delivery(tab_id, generation, &delivery_tx);
        changed
    }

    fn map_remote_desktop_pointer_position(
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

    fn handle_remote_desktop_mouse_move(&mut self, tab_id: TabId, position: Point<Pixels>) -> bool {
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

    fn handle_remote_desktop_mouse_button(
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

    fn handle_remote_desktop_gpui_mouse_button(
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

    fn handle_remote_desktop_mouse_button_release_out(
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

    fn handle_remote_desktop_wheel(
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

    fn handle_remote_desktop_key(
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

    fn sync_remote_desktop_modifiers(&mut self, tab_id: TabId, modifiers: gpui::Modifiers) {
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

    fn sync_remote_desktop_lock_keys(&mut self, tab_id: TabId, capslock: gpui::Capslock) {
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

    fn sync_remote_desktop_lock_key_press(&mut self, tab_id: TabId, keystroke: &gpui::Keystroke) {
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

    pub(super) fn forward_remote_desktop_modifiers_changed(
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

    pub(super) fn forward_remote_desktop_key_from_capture(
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

    fn copy_remote_desktop_from_keystroke(
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

    pub(super) fn copy_remote_desktop(&mut self, _cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        self.send_remote_desktop_control_shortcut(tab_id, "c");
        true
    }

    fn paste_remote_desktop_from_keystroke(
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

    pub(super) fn paste_remote_desktop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.active_remote_desktop_tab_id() else {
            return false;
        };
        let Some(item) = cx.read_from_clipboard() else {
            return true;
        };

        if let Some(session) = self.remote_desktop_sessions.get(&tab_id)
            && session.provider.capabilities.clipboard_data
            && let Some(data) = remote_desktop_clipboard_data_from_item(&item)
        {
            self.send_remote_desktop_request(
                tab_id,
                RemoteDesktopHelperRequest::ClipboardData { data },
            );
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

    fn release_remote_desktop_shortcut_modifiers(
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

fn remote_desktop_clipboard_data_from_item(
    item: &ClipboardItem,
) -> Option<RemoteDesktopClipboardData> {
    item.entries().iter().find_map(|entry| {
        let ClipboardEntry::Image(image) = entry else {
            return None;
        };
        if image.bytes.is_empty() {
            return None;
        }
        let format = remote_desktop_clipboard_format_from_gpui(image.format)?;
        Some(RemoteDesktopClipboardData::new(format, image.bytes.clone()))
    })
}

fn remote_desktop_clipboard_item_from_data(
    data: &RemoteDesktopClipboardData,
) -> Option<ClipboardItem> {
    if data.bytes.is_empty() {
        return None;
    }
    let format = gpui_image_format_from_remote_desktop(data.format)?;
    Some(ClipboardItem::new_image(&Image::from_bytes(
        format,
        data.bytes.clone(),
    )))
}

fn remote_desktop_clipboard_format_from_gpui(
    format: ImageFormat,
) -> Option<RemoteDesktopClipboardFormat> {
    Some(match format {
        ImageFormat::Png => RemoteDesktopClipboardFormat::ImagePng,
        ImageFormat::Jpeg => RemoteDesktopClipboardFormat::ImageJpeg,
        ImageFormat::Webp => RemoteDesktopClipboardFormat::ImageWebp,
        ImageFormat::Gif => RemoteDesktopClipboardFormat::ImageGif,
        ImageFormat::Svg => RemoteDesktopClipboardFormat::ImageSvg,
        ImageFormat::Bmp => RemoteDesktopClipboardFormat::ImageBmp,
        ImageFormat::Tiff => RemoteDesktopClipboardFormat::ImageTiff,
    })
}

fn gpui_image_format_from_remote_desktop(
    format: RemoteDesktopClipboardFormat,
) -> Option<ImageFormat> {
    Some(match format {
        RemoteDesktopClipboardFormat::ImagePng => ImageFormat::Png,
        RemoteDesktopClipboardFormat::ImageJpeg => ImageFormat::Jpeg,
        RemoteDesktopClipboardFormat::ImageWebp => ImageFormat::Webp,
        RemoteDesktopClipboardFormat::ImageGif => ImageFormat::Gif,
        RemoteDesktopClipboardFormat::ImageSvg => ImageFormat::Svg,
        RemoteDesktopClipboardFormat::ImageBmp => ImageFormat::Bmp,
        RemoteDesktopClipboardFormat::ImageTiff => ImageFormat::Tiff,
    })
}

fn remote_desktop_protocol_chip(
    tokens: &ThemeTokens,
    protocol: RemoteDesktopProtocol,
) -> gpui::Div {
    let label = match protocol {
        RemoteDesktopProtocol::Rdp => "RDP",
        RemoteDesktopProtocol::Vnc => "VNC",
    };

    div()
        .h(px(20.0))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.sm))
        .bg(rgba((tokens.ui.accent << 8) | 0x1f))
        .text_size(px(tokens.metrics.ui_text_xs))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(rgb(tokens.ui.accent))
        .child(label)
}

fn remote_desktop_status_color(tokens: &ThemeTokens, status: RemoteDesktopSessionStatus) -> u32 {
    // The footer uses a color-only status marker so the remote desktop title can
    // stay in the tab chrome without adding another always-visible label.
    match status {
        RemoteDesktopSessionStatus::Connected => tokens.ui.success,
        RemoteDesktopSessionStatus::Failed => tokens.ui.error,
        RemoteDesktopSessionStatus::Connecting | RemoteDesktopSessionStatus::Reconnecting => {
            tokens.ui.warning
        }
        RemoteDesktopSessionStatus::Idle | RemoteDesktopSessionStatus::Disconnected => {
            tokens.ui.text_muted
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RemoteDesktopReconnectMode {
    ProtocolRequest,
    RestartHelper,
}

fn remote_desktop_reconnect_mode(
    status: RemoteDesktopSessionStatus,
) -> Option<RemoteDesktopReconnectMode> {
    match status {
        RemoteDesktopSessionStatus::Connected => Some(RemoteDesktopReconnectMode::ProtocolRequest),
        RemoteDesktopSessionStatus::Idle
        | RemoteDesktopSessionStatus::Disconnected
        | RemoteDesktopSessionStatus::Failed => Some(RemoteDesktopReconnectMode::RestartHelper),
        RemoteDesktopSessionStatus::Connecting | RemoteDesktopSessionStatus::Reconnecting => None,
    }
}

fn remote_desktop_mouse_button_from_gpui(
    button: gpui::MouseButton,
) -> Option<RemoteDesktopMouseButton> {
    match button {
        gpui::MouseButton::Left => Some(RemoteDesktopMouseButton::Left),
        gpui::MouseButton::Middle => Some(RemoteDesktopMouseButton::Middle),
        gpui::MouseButton::Right => Some(RemoteDesktopMouseButton::Right),
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Back) => {
            Some(RemoteDesktopMouseButton::Back)
        }
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward) => {
            Some(RemoteDesktopMouseButton::Forward)
        }
    }
}

fn remote_desktop_empty_wheel_delta() -> RemoteDesktopWheelDelta {
    RemoteDesktopWheelDelta { x: 0.0, y: 0.0 }
}

fn remote_desktop_wheel_delta_from_scroll(
    delta: &gpui::ScrollDelta,
    pixel_remainder: &mut RemoteDesktopWheelDelta,
) -> Option<RemoteDesktopWheelDelta> {
    match delta {
        gpui::ScrollDelta::Pixels(point) => remote_desktop_pixel_wheel_delta(
            pixel_remainder,
            f32::from(point.x),
            f32::from(point.y),
        ),
        gpui::ScrollDelta::Lines(point) => {
            *pixel_remainder = remote_desktop_empty_wheel_delta();
            remote_desktop_nonzero_wheel_delta(RemoteDesktopWheelDelta {
                x: point.x * REMOTE_DESKTOP_SCROLL_LINE,
                y: point.y * REMOTE_DESKTOP_SCROLL_LINE,
            })
        }
    }
}

fn remote_desktop_pixel_wheel_delta(
    pixel_remainder: &mut RemoteDesktopWheelDelta,
    x: f32,
    y: f32,
) -> Option<RemoteDesktopWheelDelta> {
    let x = remote_desktop_pixel_wheel_axis(&mut pixel_remainder.x, x);
    let y = remote_desktop_pixel_wheel_axis(&mut pixel_remainder.y, y);
    remote_desktop_nonzero_wheel_delta(RemoteDesktopWheelDelta { x, y })
}

fn remote_desktop_pixel_wheel_axis(remainder: &mut f32, delta: f32) -> f32 {
    if delta.abs() < f32::EPSILON {
        return 0.0;
    }
    if remainder.signum() != delta.signum() {
        // A new gesture in the opposite direction should not pay off stale
        // sub-notch pixels from the previous direction.
        *remainder = 0.0;
    }
    *remainder += delta;
    let steps = (*remainder / REMOTE_DESKTOP_SCROLL_PIXEL_STEP).trunc();
    if steps.abs() < 1.0 {
        return 0.0;
    }
    let consumed = steps * REMOTE_DESKTOP_SCROLL_PIXEL_STEP;
    *remainder -= consumed;
    consumed
}

fn remote_desktop_nonzero_wheel_delta(
    delta: RemoteDesktopWheelDelta,
) -> Option<RemoteDesktopWheelDelta> {
    if delta.x.abs() < f32::EPSILON && delta.y.abs() < f32::EPSILON {
        None
    } else {
        Some(delta)
    }
}

fn next_remote_desktop_worker_generation(current: u64) -> u64 {
    current.saturating_add(1).max(1)
}

fn remote_desktop_paste_shortcut(keystroke: &gpui::Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    remote_desktop_key_matches(keystroke, "v")
        && !modifiers.alt
        && (modifiers.platform || modifiers.control)
}

fn remote_desktop_copy_shortcut(keystroke: &gpui::Keystroke) -> bool {
    let modifiers = keystroke.modifiers;
    remote_desktop_key_matches(keystroke, "c")
        && !modifiers.alt
        && (modifiers.platform || modifiers.control)
}

fn remote_desktop_key_matches(keystroke: &gpui::Keystroke, key: &str) -> bool {
    let event_key = keystroke.key.as_str();
    event_key.eq_ignore_ascii_case(key)
        || (event_key.len() == key.len() + "Key".len()
            && event_key
                .get(.."Key".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Key"))
            && event_key
                .get("Key".len()..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(key)))
}

fn remote_desktop_shortcut_modifier_release_codes(
    keystroke: &gpui::Keystroke,
) -> Vec<&'static str> {
    let mut codes = Vec::new();
    let modifiers = keystroke.modifiers;
    if modifiers.control {
        codes.push("control");
    }
    if modifiers.platform {
        codes.push("meta");
    }
    if modifiers.shift {
        codes.push("shift");
    }
    codes
}

fn remote_desktop_modifier_sync_requests(
    previous: RemoteDesktopModifierState,
    next: RemoteDesktopModifierState,
) -> Vec<RemoteDesktopHelperRequest> {
    let mut requests = Vec::new();
    push_remote_desktop_modifier_sync(&mut requests, "ShiftLeft", previous.shift, next.shift);
    push_remote_desktop_modifier_sync(&mut requests, "ControlLeft", previous.ctrl, next.ctrl);
    push_remote_desktop_modifier_sync(&mut requests, "AltLeft", previous.alt, next.alt);
    push_remote_desktop_modifier_sync(&mut requests, "MetaLeft", previous.meta, next.meta);
    requests
}

fn push_remote_desktop_modifier_sync(
    requests: &mut Vec<RemoteDesktopHelperRequest>,
    code: &'static str,
    previous: bool,
    next: bool,
) {
    if previous == next {
        return;
    }
    let state = if next {
        RemoteDesktopKeyState::Pressed
    } else {
        RemoteDesktopKeyState::Released
    };
    requests.push(RemoteDesktopHelperRequest::Key {
        key: RemoteDesktopKey {
            code: code.to_string(),
            text: None,
            alt: false,
            ctrl: false,
            shift: false,
            meta: false,
        },
        state,
    });
}

fn remote_desktop_lock_keys_with_capslock(
    previous: Option<RemoteDesktopLockKeys>,
    capslock: gpui::Capslock,
) -> RemoteDesktopLockKeys {
    // GPUI exposes CapsLock as a real platform snapshot. Preserve estimated
    // lock keys that GPUI does not expose, but let the platform own CapsLock.
    let mut keys = previous.unwrap_or_default();
    keys.caps_lock = capslock.on;
    keys
}

fn remote_desktop_lock_keys_after_pressed_code(
    previous: Option<RemoteDesktopLockKeys>,
    code: &str,
) -> Option<RemoteDesktopLockKeys> {
    let mut keys = previous.unwrap_or_default();
    match normalize_remote_desktop_key_code(code).as_str() {
        "numlock" => keys.num_lock = !keys.num_lock,
        "scrolllock" => keys.scroll_lock = !keys.scroll_lock,
        "kana" | "kanamode" | "kanalock" => keys.kana_lock = !keys.kana_lock,
        _ => return None,
    }
    Some(keys)
}

fn normalize_remote_desktop_key_code(code: &str) -> String {
    code.chars()
        .filter(|character| *character != '_' && *character != '-' && !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn remote_desktop_lock_key_sync_request(
    previous: Option<RemoteDesktopLockKeys>,
    next: RemoteDesktopLockKeys,
) -> Option<RemoteDesktopHelperRequest> {
    if previous == Some(next) {
        None
    } else {
        Some(RemoteDesktopHelperRequest::SynchronizeLockKeys { keys: next })
    }
}

fn is_remote_desktop_frame_event(event: &RemoteDesktopHelperEvent) -> bool {
    matches!(
        event,
        RemoteDesktopHelperEvent::Frame { .. } | RemoteDesktopHelperEvent::FrameUpdate { .. }
    )
}

fn push_remote_desktop_frame_event(
    frames: &mut VecDeque<RemoteDesktopHelperEvent>,
    event: RemoteDesktopHelperEvent,
) {
    if matches!(event, RemoteDesktopHelperEvent::Frame { .. }) {
        frames.clear();
        frames.push_back(event);
        return;
    }

    if let Some(existing) = frames.back_mut() {
        if let Err(incoming) = try_merge_remote_desktop_frame_event(existing, event) {
            frames.push_back(incoming);
        }
    } else {
        frames.push_back(event);
    }
}

fn try_merge_remote_desktop_frame_event(
    existing: &mut RemoteDesktopHelperEvent,
    incoming: RemoteDesktopHelperEvent,
) -> Result<(), RemoteDesktopHelperEvent> {
    match existing {
        RemoteDesktopHelperEvent::Frame { frame } => match incoming {
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                if !frame.apply_update(&update) {
                    return Err(RemoteDesktopHelperEvent::FrameUpdate { update });
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
                    return Err(RemoteDesktopHelperEvent::FrameUpdate {
                        update: incoming_update,
                    });
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
    Ok(())
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
    generation: u64,
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    password: Option<RemoteDesktopSecret>,
    initial_size: RemoteDesktopSize,
    _scale_factor: Option<u32>,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    worker_wake: RemoteDesktopWorkerWake,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    if let Ok((mut child, mut stdin)) = spawn_remote_desktop_helper(&provider) {
        let stdout = child.stdout.take();
        let connect = connect_request(&profile, password, initial_size);
        if let Err(error) = write_request_line(&mut stdin, &connect) {
            send_remote_desktop_worker_delivery(
                &delivery_tx,
                &worker_wake,
                RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation,
                    message: error.to_string(),
                },
            );
            return;
        }
        if let Some(stdout) = stdout {
            let reader_tx = delivery_tx.clone();
            let reader_frame_slot = frame_slot.clone();
            let reader_worker_wake = worker_wake.clone();
            thread::Builder::new()
                .name(format!("remote-desktop-reader-{}", tab_id.0))
                .spawn(move || {
                    read_remote_desktop_events(
                        tab_id,
                        generation,
                        stdout,
                        reader_tx,
                        reader_frame_slot,
                        reader_worker_wake,
                    )
                })
                .ok();
        }

        run_remote_desktop_writer(
            tab_id,
            generation,
            &mut stdin,
            request_rx,
            delivery_tx.clone(),
            worker_wake.clone(),
        );
        drop(stdin);
        let exit_code = child.wait().ok().and_then(|status| status.code());
        send_remote_desktop_worker_delivery(
            &delivery_tx,
            &worker_wake,
            RemoteDesktopWorkerDelivery::Event {
                tab_id,
                generation,
                event: RemoteDesktopHelperEvent::Terminated { exit_code },
            },
        );
        return;
    }

    run_in_process_fake_remote_desktop(
        tab_id,
        generation,
        profile,
        initial_size,
        _scale_factor,
        frame_slot,
        worker_wake,
        request_rx,
        delivery_tx,
    );
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

    if let Some(resolved) = fresh_development_helper_binary(&workspace_root, command) {
        return Some(resolved);
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

fn fresh_development_helper_binary(
    workspace_root: &Path,
    command: &str,
) -> Option<ResolvedRemoteDesktopHelper> {
    if command == "oxideterm-rdp-helper" && development_legacy_rdp_feature_available() {
        return None;
    }

    let candidate = workspace_root
        .join("target")
        .join("debug")
        .join(platform_helper_binary_name(command));
    let binary_modified = candidate.metadata().ok()?.modified().ok()?;
    let helper_crate = workspace_root.join("crates").join(command);
    let protocol_crate = workspace_root
        .join("crates")
        .join("oxideterm-remote-desktop");
    let cargo_lock = workspace_root.join("Cargo.lock");
    if path_modified_after(&helper_crate, binary_modified)
        || path_modified_after(&protocol_crate, binary_modified)
        || path_modified_after(&cargo_lock, binary_modified)
    {
        return None;
    }

    Some(ResolvedRemoteDesktopHelper {
        command: candidate,
        prefix_args: Vec::new(),
        working_dir: None,
    })
}

fn path_modified_after(path: &Path, cutoff: SystemTime) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if metadata
        .modified()
        .map(|modified| modified > cutoff)
        .unwrap_or(false)
    {
        return true;
    }
    if !metadata.is_dir() {
        return false;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let file_name = entry.file_name();
        if file_name == "target" {
            continue;
        }
        if path_modified_after(&entry_path, cutoff) {
            return true;
        }
    }
    false
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

fn default_remote_desktop_initial_size() -> RemoteDesktopSize {
    RemoteDesktopSize::clamped(REMOTE_DESKTOP_INITIAL_WIDTH, REMOTE_DESKTOP_INITIAL_HEIGHT)
}

fn initial_remote_desktop_size_for_session(session: &RemoteDesktopSession) -> RemoteDesktopSize {
    session
        .geometry
        .viewport_size()
        .or_else(|| session.state.snapshot().size)
        .unwrap_or_else(default_remote_desktop_initial_size)
}

fn remote_desktop_scale_factor_percent(scale_factor: f32) -> u32 {
    let percent = (scale_factor * REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER).round();
    if percent.is_finite() {
        let percent = percent as u32;
        if (REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT..=REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT)
            .contains(&percent)
        {
            return percent;
        }
    }
    REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT
}

fn remote_desktop_resize_request_needed(
    current_frame_size: Option<RemoteDesktopSize>,
    pending_resize: Option<RemoteDesktopSize>,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopResizeRequestState>,
    viewport_size: RemoteDesktopSize,
    viewport_scale_factor: Option<u32>,
) -> bool {
    let next_request = RemoteDesktopResizeRequestState {
        size: viewport_size,
        scale_factor: viewport_scale_factor,
    };
    if Some(next_request) == last_sent_resize {
        return false;
    }

    let frame_mismatch = remote_desktop_size_delta_is_meaningful(current_frame_size, viewport_size)
        && Some(viewport_size) != current_frame_size;
    let viewport_changed = Some(viewport_size) != last_viewport_size;
    let scale_changed = viewport_scale_factor.is_some()
        && last_sent_resize.is_none_or(|last_sent| last_sent.scale_factor != viewport_scale_factor);
    if !viewport_changed && !frame_mismatch && !scale_changed {
        return false;
    }
    if !frame_mismatch {
        return scale_changed;
    }
    if Some(viewport_size) == pending_resize {
        return scale_changed && last_sent_resize.is_some();
    }
    let last_sent_size = last_sent_resize.map(|last_sent| last_sent.size);
    if !remote_desktop_size_delta_is_meaningful(last_sent_size, viewport_size) && !scale_changed {
        return false;
    }
    true
}

fn remote_desktop_size_delta_is_meaningful(
    previous: Option<RemoteDesktopSize>,
    next: RemoteDesktopSize,
) -> bool {
    previous.is_none_or(|previous| {
        previous.width.abs_diff(next.width) >= REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD
            || previous.height.abs_diff(next.height) >= REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD
    })
}

fn read_remote_desktop_events(
    tab_id: TabId,
    generation: u64,
    stdout: impl std::io::Read,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    worker_wake: RemoteDesktopWorkerWake,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_event_line(&mut reader) {
            Ok(Some(event)) => {
                deliver_remote_desktop_worker_event(
                    tab_id,
                    generation,
                    event,
                    &delivery_tx,
                    &frame_slot,
                    &worker_wake,
                );
            }
            Ok(None) => break,
            Err(error) => {
                send_remote_desktop_worker_delivery(
                    &delivery_tx,
                    &worker_wake,
                    RemoteDesktopWorkerDelivery::TransportFailed {
                        tab_id,
                        generation,
                        message: error.to_string(),
                    },
                );
                break;
            }
        }
    }
}

fn deliver_remote_desktop_worker_event(
    tab_id: TabId,
    generation: u64,
    event: RemoteDesktopHelperEvent,
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    frame_slot: &RemoteDesktopFrameDeliverySlot,
    worker_wake: &RemoteDesktopWorkerWake,
) {
    if is_remote_desktop_frame_event(&event) {
        frame_slot.push(tab_id, generation, event, delivery_tx, worker_wake);
    } else {
        send_remote_desktop_worker_delivery(
            delivery_tx,
            worker_wake,
            RemoteDesktopWorkerDelivery::Event {
                tab_id,
                generation,
                event,
            },
        );
    }
}

fn run_remote_desktop_writer(
    tab_id: TabId,
    generation: u64,
    stdin: &mut impl Write,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
    worker_wake: RemoteDesktopWorkerWake,
) {
    for request in request_rx {
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        if let Err(error) = write_request_line(stdin, &request) {
            send_remote_desktop_worker_delivery(
                &delivery_tx,
                &worker_wake,
                RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    generation,
                    message: error.to_string(),
                },
            );
            return;
        }
        if should_close {
            return;
        }
    }
}

fn run_in_process_fake_remote_desktop(
    tab_id: TabId,
    generation: u64,
    profile: RemoteDesktopConnectionProfile,
    initial_size: RemoteDesktopSize,
    _scale_factor: Option<u32>,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    worker_wake: RemoteDesktopWorkerWake,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    let mut backend = RemoteDesktopFakeBackend::new(profile.protocol);
    for event in backend.handle_request(connect_request(&profile, None, initial_size)) {
        deliver_remote_desktop_worker_event(
            tab_id,
            generation,
            event,
            &delivery_tx,
            &frame_slot,
            &worker_wake,
        );
    }

    for request in request_rx {
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        for event in backend.handle_request(request) {
            deliver_remote_desktop_worker_event(
                tab_id,
                generation,
                event,
                &delivery_tx,
                &frame_slot,
                &worker_wake,
            );
        }
        if should_close {
            break;
        }
    }
}

fn connect_request(
    profile: &RemoteDesktopConnectionProfile,
    password: Option<RemoteDesktopSecret>,
    initial_size: RemoteDesktopSize,
) -> RemoteDesktopHelperRequest {
    RemoteDesktopHelperRequest::Connect {
        protocol: profile.protocol,
        endpoint: profile.endpoint.clone(),
        username: profile.username.clone(),
        // Runtime-only credentials cross the UI/backend boundary here. They
        // are sent to the helper process and never stored in the profile model.
        password,
        domain: profile.domain.clone(),
        size: RemoteDesktopSize::clamped(initial_size.width, initial_size.height),
        // Initial connections request the measured desktop size only. Runtime
        // resize events carry scale so high-DPI changes follow IronRDP's
        // display-control path instead of inflating the first desktop.
        scale_factor: None,
        read_only: profile.read_only,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_mode_restarts_helper_after_terminal_states() {
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Disconnected),
            Some(RemoteDesktopReconnectMode::RestartHelper)
        );
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Failed),
            Some(RemoteDesktopReconnectMode::RestartHelper)
        );
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Idle),
            Some(RemoteDesktopReconnectMode::RestartHelper)
        );
    }

    #[test]
    fn reconnect_mode_uses_live_helper_only_when_connected() {
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Connected),
            Some(RemoteDesktopReconnectMode::ProtocolRequest)
        );
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Connecting),
            None
        );
        assert_eq!(
            remote_desktop_reconnect_mode(RemoteDesktopSessionStatus::Reconnecting),
            None
        );
    }

    #[test]
    fn worker_generation_never_wraps_to_stale_zero() {
        assert_eq!(next_remote_desktop_worker_generation(0), 1);
        assert_eq!(next_remote_desktop_worker_generation(7), 8);
        assert_eq!(next_remote_desktop_worker_generation(u64::MAX), u64::MAX);
    }

    #[test]
    fn connect_request_uses_measured_initial_size() {
        let profile = preview_remote_desktop_profile(RemoteDesktopProtocol::Rdp);
        let initial_size = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        let request = connect_request(&profile, None, initial_size);

        assert!(matches!(
            request,
            RemoteDesktopHelperRequest::Connect {
                size: RemoteDesktopSize {
                    width: 1600,
                    height: 900
                },
                scale_factor: None,
                ..
            }
        ));
    }

    #[test]
    fn resize_delta_ignores_border_sized_differences() {
        let previous = Some(RemoteDesktopSize {
            width: 1600,
            height: 900,
        });

        assert!(!remote_desktop_size_delta_is_meaningful(
            previous,
            RemoteDesktopSize {
                width: 1598,
                height: 898
            },
        ));
        assert!(remote_desktop_size_delta_is_meaningful(
            previous,
            RemoteDesktopSize {
                width: 1500,
                height: 900
            },
        ));
    }

    #[test]
    fn resize_scale_factor_matches_window_percent() {
        assert_eq!(remote_desktop_scale_factor_percent(1.0), 100);
        assert_eq!(remote_desktop_scale_factor_percent(1.25), 125);
        assert_eq!(remote_desktop_scale_factor_percent(5.0), 500);
        assert_eq!(remote_desktop_scale_factor_percent(0.75), 100);
        assert_eq!(remote_desktop_scale_factor_percent(5.25), 100);
        assert_eq!(remote_desktop_scale_factor_percent(0.0), 100);
        assert_eq!(remote_desktop_scale_factor_percent(f32::NAN), 100);
    }

    #[test]
    fn clipboard_image_item_maps_to_remote_desktop_data() {
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3]));

        let data = remote_desktop_clipboard_data_from_item(&item).unwrap();

        assert_eq!(data.format, RemoteDesktopClipboardFormat::ImagePng);
        assert_eq!(data.bytes, vec![1, 2, 3]);
    }

    #[test]
    fn remote_desktop_clipboard_data_maps_to_image_item() {
        let data =
            RemoteDesktopClipboardData::new(RemoteDesktopClipboardFormat::ImageJpeg, vec![4, 5, 6]);

        let item = remote_desktop_clipboard_item_from_data(&data).unwrap();

        assert!(matches!(
            item.entries(),
            [ClipboardEntry::Image(image)]
                if image.format == ImageFormat::Jpeg && image.bytes == vec![4, 5, 6]
        ));
    }

    #[test]
    fn mouse_button_mapping_forwards_navigation_buttons() {
        assert_eq!(
            remote_desktop_mouse_button_from_gpui(gpui::MouseButton::Navigate(
                gpui::NavigationDirection::Back
            )),
            Some(RemoteDesktopMouseButton::Back)
        );
        assert_eq!(
            remote_desktop_mouse_button_from_gpui(gpui::MouseButton::Navigate(
                gpui::NavigationDirection::Forward
            )),
            Some(RemoteDesktopMouseButton::Forward)
        );
    }

    #[test]
    fn pixel_wheel_delta_accumulates_until_full_notch() {
        let mut remainder = remote_desktop_empty_wheel_delta();

        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Pixels(gpui::point(gpui::px(60.0), gpui::px(0.0))),
                &mut remainder,
            ),
            None
        );
        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Pixels(gpui::point(gpui::px(60.0), gpui::px(0.0))),
                &mut remainder,
            ),
            Some(RemoteDesktopWheelDelta { x: 120.0, y: 0.0 })
        );
        assert_eq!(remainder, remote_desktop_empty_wheel_delta());
    }

    #[test]
    fn pixel_wheel_delta_drops_opposite_direction_remainder() {
        let mut remainder = remote_desktop_empty_wheel_delta();

        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Pixels(gpui::point(gpui::px(80.0), gpui::px(0.0))),
                &mut remainder,
            ),
            None
        );
        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Pixels(gpui::point(gpui::px(-120.0), gpui::px(0.0))),
                &mut remainder,
            ),
            Some(RemoteDesktopWheelDelta { x: -120.0, y: 0.0 })
        );
        assert_eq!(remainder, remote_desktop_empty_wheel_delta());
    }

    #[test]
    fn line_wheel_delta_resets_pixel_remainder() {
        let mut remainder = RemoteDesktopWheelDelta { x: 80.0, y: 40.0 };

        assert_eq!(
            remote_desktop_wheel_delta_from_scroll(
                &gpui::ScrollDelta::Lines(gpui::point(0.0, 1.0)),
                &mut remainder,
            ),
            Some(RemoteDesktopWheelDelta {
                x: 0.0,
                y: REMOTE_DESKTOP_SCROLL_LINE,
            })
        );
        assert_eq!(remainder, remote_desktop_empty_wheel_delta());
    }

    #[test]
    fn resize_request_retries_when_initial_frame_size_differs_from_viewport() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(remote_desktop_resize_request_needed(
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            }),
            None,
            Some(viewport),
            None,
            viewport,
            Some(100),
        ));
    }

    #[test]
    fn resize_request_does_not_repeat_pending_retry() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed(
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            }),
            Some(viewport),
            Some(viewport),
            None,
            viewport,
            Some(100),
        ));
    }

    #[test]
    fn remote_desktop_clipboard_shortcuts_accept_physical_key_codes() {
        let mut modifiers = gpui::Modifiers::default();
        modifiers.control = true;

        assert!(remote_desktop_paste_shortcut(&gpui::Keystroke {
            modifiers,
            key: "KeyV".to_string(),
            key_char: Some("v".to_string()),
        }));
        assert!(remote_desktop_paste_shortcut(&gpui::Keystroke {
            modifiers,
            key: "keyv".to_string(),
            key_char: Some("v".to_string()),
        }));
        assert!(remote_desktop_copy_shortcut(&gpui::Keystroke {
            modifiers,
            key: "KeyC".to_string(),
            key_char: Some("c".to_string()),
        }));
    }

    #[test]
    fn remote_desktop_clipboard_shortcuts_release_forwarded_modifiers() {
        let mut modifiers = gpui::Modifiers::default();
        modifiers.control = true;
        modifiers.platform = true;
        modifiers.shift = true;

        let codes = remote_desktop_shortcut_modifier_release_codes(&gpui::Keystroke {
            modifiers,
            key: "KeyV".to_string(),
            key_char: Some("v".to_string()),
        });

        assert_eq!(codes, vec!["control", "meta", "shift"]);
    }

    #[test]
    fn modifier_sync_presses_new_modifier_state() {
        let next = RemoteDesktopModifierState {
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
        };

        let requests =
            remote_desktop_modifier_sync_requests(RemoteDesktopModifierState::default(), next);

        assert_eq!(
            requests,
            vec![
                modifier_request("ShiftLeft", RemoteDesktopKeyState::Pressed),
                modifier_request("ControlLeft", RemoteDesktopKeyState::Pressed),
            ]
        );
    }

    #[test]
    fn modifier_sync_releases_cleared_modifier_state() {
        let previous = RemoteDesktopModifierState {
            shift: false,
            ctrl: true,
            alt: false,
            meta: true,
        };

        let requests =
            remote_desktop_modifier_sync_requests(previous, RemoteDesktopModifierState::default());

        assert_eq!(
            requests,
            vec![
                modifier_request("ControlLeft", RemoteDesktopKeyState::Released),
                modifier_request("MetaLeft", RemoteDesktopKeyState::Released),
            ]
        );
    }

    #[test]
    fn capslock_state_maps_to_rdp_lock_key_sync() {
        let keys = remote_desktop_lock_keys_with_capslock(None, gpui::Capslock { on: true });

        assert_eq!(
            keys,
            RemoteDesktopLockKeys {
                scroll_lock: false,
                num_lock: false,
                caps_lock: true,
                kana_lock: false,
            }
        );
        assert_eq!(
            remote_desktop_lock_key_sync_request(None, keys),
            Some(RemoteDesktopHelperRequest::SynchronizeLockKeys { keys })
        );
        assert_eq!(remote_desktop_lock_key_sync_request(Some(keys), keys), None);
    }

    #[test]
    fn capslock_sync_preserves_estimated_lock_keys() {
        let previous = RemoteDesktopLockKeys {
            scroll_lock: true,
            num_lock: true,
            caps_lock: false,
            kana_lock: true,
        };

        let keys =
            remote_desktop_lock_keys_with_capslock(Some(previous), gpui::Capslock { on: true });

        assert_eq!(
            keys,
            RemoteDesktopLockKeys {
                scroll_lock: true,
                num_lock: true,
                caps_lock: true,
                kana_lock: true,
            }
        );
    }

    #[test]
    fn lock_key_press_toggles_estimated_non_caps_states() {
        let after_num_lock = remote_desktop_lock_keys_after_pressed_code(None, "NumLock").unwrap();
        assert_eq!(
            after_num_lock,
            RemoteDesktopLockKeys {
                num_lock: true,
                ..RemoteDesktopLockKeys::default()
            }
        );

        let after_scroll_lock =
            remote_desktop_lock_keys_after_pressed_code(Some(after_num_lock), "Scroll_Lock")
                .unwrap();
        assert_eq!(
            after_scroll_lock,
            RemoteDesktopLockKeys {
                scroll_lock: true,
                num_lock: true,
                ..RemoteDesktopLockKeys::default()
            }
        );

        let after_kana =
            remote_desktop_lock_keys_after_pressed_code(Some(after_scroll_lock), "KanaMode")
                .unwrap();
        assert_eq!(
            after_kana,
            RemoteDesktopLockKeys {
                scroll_lock: true,
                num_lock: true,
                kana_lock: true,
                ..RemoteDesktopLockKeys::default()
            }
        );
        assert_eq!(
            remote_desktop_lock_keys_after_pressed_code(Some(after_kana), "CapsLock"),
            None
        );
    }

    fn modifier_request(code: &str, state: RemoteDesktopKeyState) -> RemoteDesktopHelperRequest {
        RemoteDesktopHelperRequest::Key {
            key: RemoteDesktopKey {
                code: code.to_string(),
                text: None,
                alt: false,
                ctrl: false,
                shift: false,
                meta: false,
            },
            state,
        }
    }

    #[test]
    fn resize_request_does_not_repeat_ignored_retry() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed(
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            }),
            None,
            Some(viewport),
            Some(resize_state(viewport, Some(100))),
            viewport,
            Some(100),
        ));
    }

    #[test]
    fn resize_request_skips_when_frame_already_matches_viewport() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed(
            Some(viewport),
            None,
            Some(viewport),
            None,
            viewport,
            None,
        ));
    }

    #[test]
    fn resize_request_sends_scale_only_change_once() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(remote_desktop_resize_request_needed(
            Some(viewport),
            None,
            Some(viewport),
            Some(resize_state(viewport, Some(100))),
            viewport,
            Some(125),
        ));
        assert!(!remote_desktop_resize_request_needed(
            Some(viewport),
            None,
            Some(viewport),
            Some(resize_state(viewport, Some(125))),
            viewport,
            Some(125),
        ));
    }

    #[test]
    fn resize_request_can_replace_pending_scale_change() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(remote_desktop_resize_request_needed(
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            }),
            Some(viewport),
            Some(viewport),
            Some(resize_state(viewport, Some(100))),
            viewport,
            Some(125),
        ));
    }

    fn resize_state(
        size: RemoteDesktopSize,
        scale_factor: Option<u32>,
    ) -> RemoteDesktopResizeRequestState {
        RemoteDesktopResizeRequestState { size, scale_factor }
    }
}
