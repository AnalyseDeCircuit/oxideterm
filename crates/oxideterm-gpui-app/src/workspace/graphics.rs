use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use gpui::RenderImage;
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::{
    TextInputView,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions, button_with,
    },
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};
use oxideterm_wsl_graphics::{
    GraphicsSessionMode, WSL_GRAPHICS_UNAVAILABLE, WslDistro, WslGraphicsError, WslGraphicsSession,
    WslgStatus, wsl,
};
use tokio::sync::Notify;

use super::graphics_vnc::{
    GraphicsVncFrame, GraphicsVncInput, GraphicsVncWorkerEvent, SharedGraphicsVncGeometry,
    graphics_vnc_canvas, graphics_vnc_keysyms, run_graphics_vnc_worker, vnc_button_mask,
    vnc_scroll_masks,
};
use super::ime::WorkspaceImeTarget;
use super::*;

const GRAPHICS_SELECTOR_MAX_W: f32 = 384.0; // Tauri max-w-sm.
const GRAPHICS_SELECTOR_PADDING_X: f32 = 24.0; // Tauri px-6.
const GRAPHICS_SELECTOR_GAP: f32 = 16.0; // Tauri gap-4.
const GRAPHICS_WARNING_PADDING_X: f32 = 12.0; // Tauri px-3.
const GRAPHICS_WARNING_PADDING_Y: f32 = 10.0; // Tauri py-2.5.
const GRAPHICS_DISTRO_ROW_PADDING_X: f32 = 16.0; // Tauri px-4.
const GRAPHICS_DISTRO_ROW_PADDING_Y: f32 = 12.0; // Tauri py-3.
const GRAPHICS_DISTRO_ROW_GAP: f32 = 12.0; // Tauri gap-3.
const GRAPHICS_BADGE_TEXT_SIZE: f32 = 11.0; // Tauri text-xs.
const GRAPHICS_DOT_SIZE: f32 = 6.0; // Tauri w-1.5 h-1.5.
const GRAPHICS_INPUT_H: f32 = 36.0; // Tauri shadcn Input default h-9.
const GRAPHICS_TOOLBAR_TOP: f32 = 16.0; // Tauri top-4.
const GRAPHICS_TOOLBAR_PADDING_X: f32 = 12.0; // Tauri px-3.
const GRAPHICS_TOOLBAR_PADDING_Y: f32 = 8.0; // Tauri py-2.
const GRAPHICS_TOOLBAR_GAP: f32 = 8.0; // Tauri gap-2.
const GRAPHICS_STATUS_OVERLAY_BOTTOM: f32 = 16.0; // Tauri bottom-4.
const GRAPHICS_STATUS_OVERLAY_PADDING_X: f32 = 16.0; // Tauri px-4.
const GRAPHICS_STATUS_OVERLAY_PADDING_Y: f32 = 12.0; // Tauri py-3.
const GRAPHICS_COMMON_APP_COL_GAP: f32 = 8.0; // Tauri gap-2.
const GRAPHICS_AMBER_500: u32 = 0xf59e0b; // Tauri amber-500.
const GRAPHICS_GREEN_500: u32 = 0x22c55e; // Tauri green-500.
const GRAPHICS_RED_400: u32 = 0xf87171; // Tauri red-400.
const GRAPHICS_RED_500: u32 = 0xef4444; // Tauri destructive.
const GRAPHICS_ALPHA_10: u32 = 0x1a; // Tailwind /10.
const GRAPHICS_ALPHA_20: u32 = 0x33; // Tailwind /20.
const GRAPHICS_ALPHA_50: u32 = 0x80; // Tailwind /50.
const GRAPHICS_ALPHA_90: u32 = 0xe6; // Tailwind /90.
const GRAPHICS_WORKER_DELIVERY_BUDGET: delivery::DeliveryBudget =
    delivery::DeliveryBudget::new(32, Duration::from_millis(6));

const COMMON_APPS: &[(&str, &str)] = &[
    ("gedit", "gedit"),
    ("Firefox", "firefox"),
    ("Nautilus", "nautilus"),
    ("VS Code", "code"),
    ("xterm", "xterm"),
    ("GIMP", "gimp"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum GraphicsInput {
    AppCommand,
}

impl GraphicsInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::AppCommand => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GraphicsStatus {
    Idle,
    Starting,
    Active,
    Disconnected,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GraphicsLaunchMode {
    Desktop,
    App,
}

#[derive(Clone, Debug)]
pub(super) enum GraphicsWorkerResult {
    ListSessions {
        result: Result<Vec<WslGraphicsSession>, String>,
    },
    LoadDistros {
        generation: u64,
        result: Result<Vec<WslDistro>, String>,
    },
    DetectWslg {
        generation: u64,
        distro: String,
        result: Result<WslgStatus, String>,
    },
    Start {
        generation: u64,
        result: Result<WslGraphicsSession, String>,
    },
    StartApp {
        generation: u64,
        result: Result<WslGraphicsSession, String>,
    },
    Stop {
        generation: u64,
        session_id: String,
        result: Result<(), String>,
    },
    Reconnect {
        generation: u64,
        result: Result<WslGraphicsSession, String>,
    },
    VncEvent(GraphicsVncWorkerEvent),
}

fn coalesce_adjacent_graphics_frames(
    results: Vec<GraphicsWorkerResult>,
) -> Vec<GraphicsWorkerResult> {
    let mut coalesced = Vec::with_capacity(results.len());
    for result in results {
        match result {
            GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Frame { session_id, frame }) => {
                let latest_frame = GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Frame {
                    session_id,
                    frame,
                });
                let replaces_previous = matches!(
                    (coalesced.last(), &latest_frame),
                    (
                        Some(GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Frame {
                            session_id: previous_session_id,
                            ..
                        })),
                        GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Frame {
                            session_id: latest_session_id,
                            ..
                        })
                    ) if previous_session_id == latest_session_id
                );
                if replaces_previous {
                    // A full frame supersedes only an adjacent frame from the same session.
                    let previous_frame_index = coalesced.len() - 1;
                    coalesced[previous_frame_index] = latest_frame;
                } else {
                    coalesced.push(latest_frame);
                }
            }
            other => {
                // Lifecycle boundaries stay ordered and prevent frame coalescing across them.
                coalesced.push(other);
            }
        }
    }
    coalesced
}

#[derive(Clone)]
struct GraphicsWorkerWake {
    pending: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    notification: Arc<Notify>,
}

impl Default for GraphicsWorkerWake {
    fn default() -> Self {
        Self {
            pending: Arc::new(AtomicBool::new(false)),
            stopped: Arc::new(AtomicBool::new(false)),
            notification: Arc::new(Notify::new()),
        }
    }
}

impl GraphicsWorkerWake {
    fn mark(&self) {
        // Notify stores one permit, while pending coalesces any burst before the UI task runs.
        self.pending.store(true, Ordering::Release);
        self.notification.notify_one();
    }

    fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.notification.notify_one();
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    async fn wait(&self) {
        self.notification.notified().await;
    }
}

#[derive(Default)]
struct GraphicsFrameDeliveryState {
    visible: bool,
    latest: Option<(String, GraphicsVncFrame)>,
}

#[derive(Clone, Default)]
struct GraphicsFrameDeliverySlot {
    state: Arc<Mutex<GraphicsFrameDeliveryState>>,
}

impl GraphicsFrameDeliverySlot {
    fn push(&self, session_id: String, frame: GraphicsVncFrame) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        // Full VNC frames supersede one another before GPUI image allocation.
        state.latest = Some((session_id, frame));
        state.visible
    }

    fn set_visible(&self, visible: bool) -> Option<(String, GraphicsVncFrame)> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        state.visible = visible;
        if visible { state.latest.take() } else { None }
    }

    fn take_visible(&self) -> Option<(String, GraphicsVncFrame)> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        if state.visible {
            state.latest.take()
        } else {
            None
        }
    }

    fn clear(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.latest = None;
        }
    }

    fn clear_session(&self, session_id: &str) {
        if let Ok(mut state) = self.state.lock()
            && state
                .latest
                .as_ref()
                .is_some_and(|(pending_session_id, _)| pending_session_id == session_id)
        {
            state.latest = None;
        }
    }

    #[cfg(test)]
    fn has_pending_frame(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.latest.is_some())
            .unwrap_or(false)
    }
}

#[derive(Clone)]
struct GraphicsWorkerDelivery {
    sender: mpsc::Sender<GraphicsWorkerResult>,
    wake: GraphicsWorkerWake,
    frame_slot: GraphicsFrameDeliverySlot,
}

impl GraphicsWorkerDelivery {
    fn new(sender: mpsc::Sender<GraphicsWorkerResult>) -> Self {
        Self {
            sender,
            wake: GraphicsWorkerWake::default(),
            frame_slot: GraphicsFrameDeliverySlot::default(),
        }
    }

    fn send(&self, result: GraphicsWorkerResult) {
        match result {
            GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Frame { session_id, frame }) => {
                if self.frame_slot.push(session_id, frame) {
                    self.wake.mark();
                }
            }
            other => {
                if let GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Disconnected {
                    session_id,
                    ..
                }) = &other
                {
                    // A disconnect is a lifecycle boundary; never recover its preceding frame later.
                    self.frame_slot.clear_session(session_id);
                }
                // Publish before waking so the foreground task observes the queued result.
                if self.sender.send(other).is_ok() {
                    self.wake.mark();
                }
            }
        }
    }
}

/// Owns WSL graphics sessions, VNC input, workers, and delivery lifecycle.
pub(super) struct GraphicsWorkspaceEntity {
    distros: Vec<WslDistro>,
    sessions: Vec<WslGraphicsSession>,
    wslg_statuses: HashMap<String, WslgStatus>,
    selected_distro: Option<String>,
    launch_mode: GraphicsLaunchMode,
    app_command: String,
    status: GraphicsStatus,
    error: Option<String>,
    loading: bool,
    generation: u64,
    pub(super) focused_input: Option<GraphicsInput>,
    session: Option<WslGraphicsSession>,
    vnc_session_id: Option<String>,
    vnc_input: Option<tokio::sync::mpsc::UnboundedSender<GraphicsVncInput>>,
    vnc_stop: Option<tokio::sync::oneshot::Sender<()>>,
    vnc_frame: Option<GraphicsVncFrame>,
    vnc_render_image: Option<Arc<RenderImage>>,
    vnc_retired_images: Vec<Arc<RenderImage>>,
    vnc_geometry: SharedGraphicsVncGeometry,
    vnc_button_mask: u8,
    worker_delivery: GraphicsWorkerDelivery,
    worker_rx: mpsc::Receiver<GraphicsWorkerResult>,
    backend: Arc<oxideterm_wsl_graphics::WslGraphicsState>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GraphicsWorkspaceEvent {
    WorkerResultsReady,
}

impl GraphicsWorkspaceEntity {
    pub(super) fn new(
        backend: Arc<oxideterm_wsl_graphics::WslGraphicsState>,
        runtime: Arc<tokio::runtime::Runtime>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        let worker_delivery = GraphicsWorkerDelivery::new(worker_tx);
        let entity = Self {
            distros: Vec::new(),
            sessions: Vec::new(),
            wslg_statuses: HashMap::new(),
            selected_distro: None,
            launch_mode: GraphicsLaunchMode::Desktop,
            app_command: String::new(),
            status: GraphicsStatus::Idle,
            error: None,
            loading: false,
            generation: 0,
            focused_input: None,
            session: None,
            vnc_session_id: None,
            vnc_input: None,
            vnc_stop: None,
            vnc_frame: None,
            vnc_render_image: None,
            vnc_retired_images: Vec::new(),
            vnc_geometry: SharedGraphicsVncGeometry::default(),
            vnc_button_mask: 0,
            worker_delivery,
            worker_rx,
            backend,
            runtime,
        };
        entity.schedule_worker_delivery(cx);
        entity
    }

    pub(super) fn focused_input(&self) -> Option<GraphicsInput> {
        self.focused_input
    }

    pub(super) fn input_value(&self, input: GraphicsInput) -> &str {
        match input {
            GraphicsInput::AppCommand => &self.app_command,
        }
    }

    pub(super) fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(super) fn replace_input(
        &mut self,
        input: GraphicsInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focused_input != Some(input) {
            return false;
        }
        match input {
            GraphicsInput::AppCommand => {
                replace_utf16(&mut self.app_command, replacement_range, text);
            }
        }
        cx.notify();
        true
    }
}

impl Drop for GraphicsWorkspaceEntity {
    fn drop(&mut self) {
        // Stop the foreground waiter even if background producers still hold sender clones.
        self.worker_delivery.wake.stop();
    }
}

impl GraphicsWorkspaceEntity {
    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> Vec<Arc<RenderImage>> {
        let drain = delivery::drain_channel(&self.worker_rx, GRAPHICS_WORKER_DELIVERY_BUDGET);
        let mut changed = false;
        for result in coalesce_adjacent_graphics_frames(drain.items) {
            match result {
                GraphicsWorkerResult::ListSessions { result } => {
                    if let Ok(mut sessions) = result {
                        sessions.sort_by(|left, right| {
                            left.distro
                                .cmp(&right.distro)
                                .then_with(|| left.desktop_name.cmp(&right.desktop_name))
                                .then_with(|| left.id.cmp(&right.id))
                        });
                        self.sessions = sessions;
                        changed = true;
                    }
                }
                GraphicsWorkerResult::LoadDistros { generation, result } => {
                    if generation != self.generation {
                        continue;
                    }
                    self.loading = false;
                    match result {
                        Ok(distros) => {
                            self.error = None;
                            self.distros = distros;
                            if self.selected_distro.is_none() {
                                self.selected_distro = self
                                    .distros
                                    .iter()
                                    .find(|distro| distro.is_default)
                                    .or_else(|| self.distros.first())
                                    .map(|distro| distro.name.clone());
                            }
                            self.start_wslg_detection(generation);
                            self.load_sessions();
                        }
                        Err(error) => {
                            self.error = Some(normalize_graphics_error(error));
                            self.distros.clear();
                        }
                    }
                    changed = true;
                }
                GraphicsWorkerResult::DetectWslg {
                    generation,
                    distro,
                    result,
                } => {
                    if generation != self.generation {
                        continue;
                    }
                    if let Ok(status) = result {
                        self.wslg_statuses.insert(distro, status);
                        changed = true;
                    }
                }
                GraphicsWorkerResult::Start { generation, result }
                | GraphicsWorkerResult::StartApp { generation, result } => {
                    if generation != self.generation {
                        continue;
                    }
                    match result {
                        Ok(session) => {
                            self.reset_vnc_viewer(true);
                            self.error = None;
                            self.status = GraphicsStatus::Starting;
                            self.session = Some(session);
                            self.load_sessions();
                        }
                        Err(error) => {
                            self.reset_vnc_viewer(true);
                            self.status = GraphicsStatus::Error;
                            self.error = Some(normalize_graphics_error(error));
                            self.session = None;
                        }
                    }
                    changed = true;
                }
                GraphicsWorkerResult::Stop {
                    generation,
                    session_id,
                    result,
                } => {
                    if generation != self.generation {
                        continue;
                    }
                    if self
                        .session
                        .as_ref()
                        .is_none_or(|session| session.id == session_id)
                    {
                        self.session = None;
                        self.reset_vnc_viewer(true);
                        self.status = GraphicsStatus::Idle;
                        self.load_sessions();
                    }
                    if let Err(error) = result {
                        self.error = Some(normalize_graphics_error(error));
                    }
                    changed = true;
                }
                GraphicsWorkerResult::Reconnect { generation, result } => {
                    if generation != self.generation {
                        continue;
                    }
                    match result {
                        Ok(session) => {
                            self.reset_vnc_viewer(true);
                            self.error = None;
                            self.status = GraphicsStatus::Starting;
                            self.session = Some(session);
                            self.load_sessions();
                        }
                        Err(error) => {
                            self.reset_vnc_viewer(true);
                            self.status = GraphicsStatus::Error;
                            self.error = Some(normalize_graphics_error(error));
                        }
                    }
                    changed = true;
                }
                GraphicsWorkerResult::VncEvent(event) => {
                    changed |= self.apply_vnc_event(event);
                }
            }
        }
        if let Some((session_id, frame)) = self.worker_delivery.frame_slot.take_visible() {
            changed |= self.apply_vnc_frame(session_id, frame);
        }
        if changed {
            self.ensure_vnc_worker();
            cx.notify();
        }
        if drain.outcome.backlog_remaining {
            self.worker_delivery.wake.mark();
        }
        std::mem::take(&mut self.vnc_retired_images)
    }

    fn schedule_worker_delivery(&self, cx: &mut Context<Self>) {
        let worker_wake = self.worker_delivery.wake.clone();
        cx.spawn(async move |entity, cx| {
            loop {
                worker_wake.wait().await;
                if worker_wake.is_stopped() {
                    break;
                }
                if !worker_wake.take() {
                    continue;
                }
                if entity
                    .update(cx, |_graphics, cx| {
                        // The registry retains this typed notification until a
                        // current native window can release retired image entries.
                        cx.emit(GraphicsWorkspaceEvent::WorkerResultsReady);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}

impl gpui::EventEmitter<GraphicsWorkspaceEvent> for GraphicsWorkspaceEntity {}

impl WorkspaceApp {
    pub(in crate::workspace) fn apply_graphics_worker_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let retired_images = self
            .graphics
            .update(cx, |graphics, cx| graphics.drain_worker_results(cx));
        for image in retired_images {
            // Retired atlas entries must be dropped against a live window.
            cx.drop_image(image, Some(window));
        }
    }

    pub(super) fn open_graphics_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::Graphics)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Graphics,
                    title: self.i18n.t("graphics.tab_title"),
                    title_source: TabTitleSource::I18nKey("graphics.tab_title"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.graphics.update(cx, |graphics, cx| {
            graphics.start_graphics_load_if_needed(false);
            graphics.load_sessions();
            cx.notify();
        });
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window, cx);
        cx.notify();
    }

    pub(super) fn render_graphics_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.graphics.update(cx, |graphics, cx| {
            graphics.set_surface_visible(true, cx);
        });
        let graphics = self.graphics.read(cx);
        if graphics.session.is_some() || graphics.status != GraphicsStatus::Idle {
            return self.render_graphics_active_surface(window, cx);
        }
        self.render_graphics_distro_selector(cx)
    }

    pub(in crate::workspace) fn sync_graphics_surface_visibility(&self, cx: &mut App) {
        let visible = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::Graphics)
            .is_some_and(|tab| {
                let tab_host = self.tab_host.read(cx);
                (self.active_tab_id(cx) == Some(tab.id) && !tab_host.is_outside_main_window(tab.id))
                    || tab_host.is_detached(tab.id)
            });
        self.graphics.update(cx, |graphics, cx| {
            graphics.set_surface_visible(visible, cx);
        });
    }

    pub(super) fn handle_graphics_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.graphics.read(cx).session.is_some()
            && self.graphics.read(cx).focused_input.is_none()
            && let Some(keysym) =
                graphics_vnc_keysyms(&event.keystroke.key, event.keystroke.key_char.as_deref())
        {
            // VNC needs explicit press/release. GPUI routes graphics input
            // through KeyDown here, so send a short key tap for now.
            self.graphics.update(cx, |graphics, _cx| {
                graphics.send_vnc_input(GraphicsVncInput::Key { keysym, down: true });
                graphics.send_vnc_input(GraphicsVncInput::Key {
                    keysym,
                    down: false,
                });
            });
            return true;
        }
        if self.graphics.read(cx).focused_input != Some(GraphicsInput::AppCommand)
            || event.keystroke.modifiers.platform
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "enter" => {
                self.graphics
                    .update(cx, |graphics, cx| graphics.start_graphics_app(cx));
                true
            }
            "escape" => {
                self.graphics.update(cx, |graphics, cx| {
                    graphics.focused_input = None;
                    cx.notify();
                });
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "backspace" => {
                let command_changed = self.graphics.update(cx, |graphics, cx| {
                    let changed = graphics.app_command.pop().is_some();
                    if changed {
                        cx.notify();
                    }
                    changed
                });
                let changed = command_changed || self.ime_marked_text.take().is_some();
                if changed {
                    // Empty Backspace should not repaint unless it clears IME
                    // composition state.
                    cx.notify();
                }
                true
            }
            _ => true,
        }
    }

    fn render_graphics_distro_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let (loading, error, distros_empty, launch_mode, has_sessions) = {
            let graphics = self.graphics.read(cx);
            (
                graphics.loading,
                graphics.error.clone(),
                graphics.distros.is_empty(),
                graphics.launch_mode,
                !graphics.sessions.is_empty(),
            )
        };
        if loading {
            return self.render_graphics_center_state(
                LucideIcon::LoaderCircle,
                self.i18n.t("graphics.loading_distros"),
                theme.accent,
                None,
                cx,
            );
        }

        if let Some(error) = error.as_ref()
            && error == WSL_GRAPHICS_UNAVAILABLE
        {
            return self.render_graphics_not_available(cx);
        }

        if distros_empty && error.is_none() {
            return self.render_graphics_center_state(
                LucideIcon::Monitor,
                self.i18n.t("graphics.no_distros"),
                theme.text_muted,
                None,
                cx,
            );
        }

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .px(px(GRAPHICS_SELECTOR_PADDING_X))
            .bg(rgb(theme.bg))
            .child(
                div()
                    .w_full()
                    .max_w(px(GRAPHICS_SELECTOR_MAX_W))
                    .flex()
                    .flex_col()
                    .gap(px(GRAPHICS_SELECTOR_GAP))
                    .child(self.render_graphics_mode_tabs(cx))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t(match launch_mode {
                                GraphicsLaunchMode::Desktop => "graphics.select_distro",
                                GraphicsLaunchMode::App => "graphics.app_select_distro",
                            })),
                    )
                    .when_some(error.as_ref(), |panel, error| {
                        panel.child(self.render_graphics_error_box(error))
                    })
                    .child(self.render_graphics_launch_mode(cx))
                    .when(has_sessions, |panel| {
                        panel.child(self.render_graphics_session_list(cx))
                    }),
            )
            .into_any_element()
    }

    fn render_graphics_mode_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let graphics = self.graphics.read(cx);
        let desktop_active = graphics.launch_mode == GraphicsLaunchMode::Desktop;
        let app_active = graphics.launch_mode == GraphicsLaunchMode::App;
        div()
            .grid()
            .grid_cols(2)
            .gap(px(2.0))
            .p(px(2.0))
            .rounded(px(self.tokens.radii.md))
            .bg(rgb(theme.bg_panel))
            .child(self.render_graphics_mode_tab(
                "graphics.desktop_mode",
                desktop_active,
                GraphicsLaunchMode::Desktop,
                cx,
            ))
            .child(self.render_graphics_mode_tab(
                "graphics.app_mode",
                app_active,
                GraphicsLaunchMode::App,
                cx,
            ))
            .into_any_element()
    }

    fn render_graphics_mode_tab(
        &self,
        label_key: &'static str,
        active: bool,
        mode: GraphicsLaunchMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if active { theme.text } else { theme.text_muted }))
            .bg(if active {
                rgb(theme.bg)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .child(self.i18n.t(label_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.graphics.update(cx, |graphics, cx| {
                        graphics.launch_mode = mode;
                        graphics.focused_input =
                            (mode == GraphicsLaunchMode::App).then_some(GraphicsInput::AppCommand);
                        cx.notify();
                    });
                }),
            )
            .into_any_element()
    }

    fn render_graphics_launch_mode(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.graphics.read(cx).launch_mode {
            GraphicsLaunchMode::Desktop => self.render_graphics_desktop_mode(cx),
            GraphicsLaunchMode::App => self.render_graphics_app_mode(cx),
        }
    }

    fn render_graphics_desktop_mode(&self, cx: &mut Context<Self>) -> AnyElement {
        let distros = self.graphics.read(cx).distros.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_graphics_warning(self.i18n.t("graphics.desktop_experimental"), None))
            .children(
                distros
                    .into_iter()
                    .map(|distro| self.render_graphics_distro_row(distro, cx)),
            )
            .into_any_element()
    }

    fn render_graphics_app_mode(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let (selected_status, can_start) = {
            let graphics = self.graphics.read(cx);
            (
                graphics
                    .selected_distro
                    .as_ref()
                    .and_then(|name| graphics.wslg_statuses.get(name))
                    .cloned(),
                graphics.selected_distro.is_some()
                    && !graphics.app_command.trim().is_empty()
                    && graphics.status != GraphicsStatus::Starting,
            )
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_graphics_warning(
                self.i18n.t("graphics.desktop_experimental"),
                Some(self.i18n.t("graphics.app_experimental_note")),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.render_graphics_label("graphics.app_distro_label"))
                    .child(self.render_graphics_app_distro_selector(cx))
                    .when_some(selected_status.as_ref(), |field, status| {
                        field.child(self.render_graphics_wslg_badge(Some(status)))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.render_graphics_label("graphics.app_command_label"))
                    .child(self.render_graphics_app_command_input(cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("graphics.app_common_apps")),
                    )
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap(px(GRAPHICS_COMMON_APP_COL_GAP))
                            .children(COMMON_APPS.iter().map(|(label, command)| {
                                self.render_graphics_common_app_button(label, command, cx)
                            })),
                    ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("graphics.start_app"),
                    ButtonOptions {
                        variant: ButtonVariant::Default,
                        size: ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: !can_start,
                    },
                )
                .w_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        if this.graphics.read(cx).selected_distro.is_some()
                            && !this.graphics.read(cx).app_command.trim().is_empty()
                        {
                            this.graphics
                                .update(cx, |graphics, cx| graphics.start_graphics_app(cx));
                        }
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_graphics_session_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let sessions = self.graphics.read(cx).sessions.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("graphics.tab_title")),
            )
            .children(
                sessions
                    .into_iter()
                    .map(|session| self.render_graphics_session_row(session, cx)),
            )
            .into_any_element()
    }

    fn render_graphics_session_row(
        &self,
        session: WslGraphicsSession,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let reconnect_session_id = session.id.clone();
        let stop_session_id = session.id.clone();
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(graphics_session_title(&session)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(session.distro.clone()),
                    ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("graphics.reconnect"),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.graphics.update(cx, |graphics, cx| {
                            graphics
                                .reconnect_graphics_session_id(reconnect_session_id.clone(), cx);
                        });
                    }),
                ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("graphics.stop"),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.graphics.update(cx, |graphics, cx| {
                            graphics.stop_graphics_session_id(stop_session_id.clone(), cx);
                        });
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_graphics_label(&self, key: &'static str) -> AnyElement {
        div()
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(key))
            .into_any_element()
    }

    fn render_graphics_distro_row(&self, distro: WslDistro, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let graphics = self.graphics.read(cx);
        let status = graphics.wslg_statuses.get(&distro.name);
        let distro_name = distro.name.clone();
        div()
            .flex()
            .items_center()
            .gap(px(GRAPHICS_DISTRO_ROW_GAP))
            .px(px(GRAPHICS_DISTRO_ROW_PADDING_X))
            .py(px(GRAPHICS_DISTRO_ROW_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba(0x00000000))
            .cursor_pointer()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(div().truncate().child(distro.name.clone()))
                            .when(distro.is_default, |name_row| {
                                name_row.child(self.render_graphics_default_badge())
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(if distro.is_running {
                                self.i18n.t("graphics.distro_running")
                            } else {
                                self.i18n.t("graphics.distro_stopped")
                            })
                            .child(self.render_graphics_wslg_badge(status)),
                    ),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ChevronRight,
                16.0,
                rgb(theme.text_muted),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.graphics.update(cx, |graphics, cx| {
                        graphics.start_graphics_desktop(distro_name.clone(), cx);
                    });
                }),
            )
            .into_any_element()
    }

    fn render_graphics_app_distro_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .children(
                self.graphics
                    .read(cx)
                    .distros
                    .iter()
                    .cloned()
                    .map(|distro| {
                        let selected =
                            self.graphics.read(cx).selected_distro.as_deref() == Some(&distro.name);
                        let distro_name = distro.name.clone();
                        div()
                            .h(px(34.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgb(if selected { theme.accent } else { theme.border }))
                            .bg(if selected {
                                rgba((theme.accent << 8) | GRAPHICS_ALPHA_10)
                            } else {
                                rgb(theme.bg)
                            })
                            .text_size(px(13.0))
                            .text_color(rgb(theme.text))
                            .cursor_pointer()
                            .child(div().flex_1().truncate().child(format!(
                                "{}{}{}",
                                distro.name,
                                if distro.is_default { " (Default)" } else { "" },
                                if distro.is_running {
                                    String::new()
                                } else {
                                    format!(" - {}", self.i18n.t("graphics.distro_stopped"))
                                }
                            )))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Check,
                                14.0,
                                rgb(if selected { theme.accent } else { theme.bg }),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.graphics.update(cx, |graphics, cx| {
                                        graphics.selected_distro = Some(distro_name.clone());
                                        cx.notify();
                                    });
                                }),
                            )
                    }),
            )
            .into_any_element()
    }

    fn render_graphics_app_command_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let graphics = self.graphics.read(cx);
        let focused = graphics.focused_input == Some(GraphicsInput::AppCommand);
        let target = WorkspaceImeTarget::Graphics(GraphicsInput::AppCommand);
        let marked = self.marked_text_for_target(target, cx);
        div()
            .relative()
            .child(
                self.text_input_with_workspace_ime(
                    target,
                    oxideterm_gpui_ui::text_input(
                        &self.tokens,
                        TextInputView {
                            value: &graphics.app_command,
                            placeholder: self.i18n.t("graphics.app_command_placeholder"),
                            focused,
                            caret_visible: self.input_caret.visible(),
                            secret: false,
                            selected_all: false,
                            selected_range: self.ime_selected_range_for_target(target, cx),
                            marked_text: marked,
                        },
                    )
                    .h(px(GRAPHICS_INPUT_H))
                    .bg(rgb(theme.bg))
                    .border_color(rgb(theme.border)),
                    |this, cx| {
                        this.graphics.update(cx, |graphics, cx| {
                            graphics.focused_input = Some(GraphicsInput::AppCommand);
                            cx.notify();
                        });
                        this.show_active_input_caret(cx);
                    },
                    cx,
                ),
            )
            .into_any_element()
    }

    fn render_graphics_common_app_button(
        &self,
        label: &str,
        command: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let command = command.to_string();
        button_with(
            &self.tokens,
            label.to_string(),
            ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .justify_start()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.graphics.update(cx, |graphics, cx| {
                    graphics.app_command = command.clone();
                    graphics.focused_input = Some(GraphicsInput::AppCommand);
                    cx.notify();
                });
            }),
        )
        .into_any_element()
    }

    fn render_graphics_active_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.graphics
            .update(cx, |graphics, _cx| graphics.ensure_vnc_worker());
        let (frame_size, render_image, geometry, show_status_overlay) = {
            let graphics = self.graphics.read(cx);
            (
                graphics
                    .vnc_frame
                    .as_ref()
                    .map(|frame| (frame.width, frame.height)),
                graphics.vnc_render_image.clone(),
                graphics.vnc_geometry.clone(),
                graphics.status != GraphicsStatus::Active
                    || graphics.error.is_some()
                    || graphics.session.is_none(),
            )
        };
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(0x000000))
            .child(
                div()
                    .size_full()
                    .child(graphics_vnc_canvas(
                        frame_size,
                        render_image,
                        geometry,
                        0x000000,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.graphics.update(cx, |graphics, _cx| {
                                graphics.send_vnc_pointer(
                                    event.position,
                                    Some((MouseButton::Left, true)),
                                );
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.graphics.update(cx, |graphics, _cx| {
                                graphics.send_vnc_pointer(
                                    event.position,
                                    Some((MouseButton::Right, true)),
                                );
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.graphics.update(cx, |graphics, _cx| {
                                graphics.send_vnc_pointer(
                                    event.position,
                                    Some((MouseButton::Middle, true)),
                                );
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                            this.graphics.update(cx, |graphics, _cx| {
                                graphics.send_vnc_pointer(
                                    event.position,
                                    Some((MouseButton::Left, false)),
                                );
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                            this.graphics.update(cx, |graphics, _cx| {
                                graphics.send_vnc_pointer(
                                    event.position,
                                    Some((MouseButton::Right, false)),
                                );
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                            this.graphics.update(cx, |graphics, _cx| {
                                graphics.send_vnc_pointer(
                                    event.position,
                                    Some((MouseButton::Middle, false)),
                                );
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.graphics.update(cx, |graphics, _cx| {
                            graphics.send_vnc_pointer(event.position, None);
                        });
                        cx.stop_propagation();
                    }))
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                        this.graphics.update(cx, |graphics, _cx| {
                            graphics.send_vnc_scroll(event.position, &event.delta);
                        });
                        cx.stop_propagation();
                    })),
            )
            .child(self.render_graphics_toolbar(window, cx))
            .when(show_status_overlay, |surface| {
                surface.child(self.render_graphics_status_overlay(cx))
            })
            .child(
                div()
                    .absolute()
                    .right(px(16.0))
                    .bottom(px(16.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba((theme.bg_panel << 8) | GRAPHICS_ALPHA_90))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | GRAPHICS_ALPHA_50))
                    .text_size(px(11.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.graphics_canvas_diagnostics_text()),
            )
            .into_any_element()
    }

    fn render_graphics_toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let session = self.graphics.read(cx).session.clone();
        let session_missing = session.is_none();
        div()
            .absolute()
            .top(px(GRAPHICS_TOOLBAR_TOP))
            .left(px(GRAPHICS_TOOLBAR_TOP))
            .right(px(GRAPHICS_TOOLBAR_TOP))
            .flex()
            .items_center()
            .gap(px(GRAPHICS_TOOLBAR_GAP))
            .px(px(GRAPHICS_TOOLBAR_PADDING_X))
            .py(px(GRAPHICS_TOOLBAR_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_panel << 8) | GRAPHICS_ALPHA_90))
            .border_1()
            .border_color(rgba((theme.border << 8) | GRAPHICS_ALPHA_50))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .truncate()
                            .child(
                                session
                                    .as_ref()
                                    .map(graphics_session_title)
                                    .unwrap_or_default(),
                            ),
                    )
                    .when_some(session.as_ref(), |meta, session| {
                        meta.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(11.0))
                                .text_color(rgb(theme.text_muted))
                                .child(session.distro.clone())
                                .when(
                                    matches!(session.mode, GraphicsSessionMode::App { .. }),
                                    |row| row.child(self.i18n.t("graphics.app_mode")),
                                )
                                .child(self.i18n.t("graphics.desktop_experimental")),
                        )
                    }),
            )
            .child(self.render_graphics_toolbar_button(
                "graphics.reconnect",
                ButtonVariant::Secondary,
                session_missing,
                GraphicsToolbarAction::Reconnect,
                window,
                cx,
            ))
            .child(self.render_graphics_toolbar_button(
                "graphics.fullscreen",
                ButtonVariant::Secondary,
                false,
                GraphicsToolbarAction::Fullscreen,
                window,
                cx,
            ))
            .child(self.render_graphics_toolbar_button(
                "graphics.stop",
                ButtonVariant::Destructive,
                session_missing,
                GraphicsToolbarAction::Stop,
                window,
                cx,
            ))
            .into_any_element()
    }

    fn render_graphics_toolbar_button(
        &self,
        label_key: &'static str,
        variant: ButtonVariant,
        disabled: bool,
        action: GraphicsToolbarAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Graphics toolbar buttons are shadcn-style Tauri actions; route
        // disabled activation through the same workspace guard as other toolbars.
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                match action {
                    GraphicsToolbarAction::Reconnect => {
                        if this.graphics.read(cx).session.is_some() {
                            this.graphics.update(cx, |graphics, cx| {
                                graphics.reconnect_graphics_session(cx);
                            });
                        }
                    }
                    GraphicsToolbarAction::Fullscreen => window.toggle_fullscreen(),
                    GraphicsToolbarAction::Stop => {
                        this.graphics
                            .update(cx, |graphics, cx| graphics.stop_graphics_session(cx));
                    }
                }
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_graphics_status_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let graphics = self.graphics.read(cx);
        let (icon, text, color) = match graphics.status {
            GraphicsStatus::Starting => (
                LucideIcon::LoaderCircle,
                self.i18n.t("graphics.starting"),
                theme.accent,
            ),
            GraphicsStatus::Disconnected => (
                LucideIcon::WifiOff,
                self.i18n.t("graphics.disconnected"),
                GRAPHICS_AMBER_500,
            ),
            GraphicsStatus::Error => (
                LucideIcon::AlertCircle,
                graphics
                    .error
                    .clone()
                    .unwrap_or_else(|| self.i18n.t("graphics.error")),
                GRAPHICS_RED_400,
            ),
            _ => (
                LucideIcon::LoaderCircle,
                self.i18n.t("graphics.starting"),
                theme.accent,
            ),
        };
        div()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(GRAPHICS_STATUS_OVERLAY_BOTTOM))
            .flex()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(520.0))
                    .min_w(px(0.0))
                    .px(px(GRAPHICS_STATUS_OVERLAY_PADDING_X))
                    .py(px(GRAPHICS_STATUS_OVERLAY_PADDING_Y))
                    .rounded(px(self.tokens.radii.md))
                    .bg(rgba((theme.bg_panel << 8) | GRAPHICS_ALPHA_90))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | GRAPHICS_ALPHA_50))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(if matches!(icon, LucideIcon::LoaderCircle) {
                        self.render_loading_icon("graphics-status-loading", 16.0, rgb(color))
                    } else {
                        Self::render_lucide_icon(icon, 16.0, rgb(color))
                    })
                    .child(
                        div()
                            // Long prerequisite errors must shrink and wrap inside the overlay.
                            .min_w(px(0.0))
                            .flex_1()
                            .whitespace_normal()
                            .text_size(px(13.0))
                            .text_color(rgb(theme.text))
                            .child(text),
                    )
                    .when(
                        matches!(
                            graphics.status,
                            GraphicsStatus::Disconnected | GraphicsStatus::Error
                        ) && graphics.session.is_some(),
                        |overlay| {
                            overlay.child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("graphics.reconnect"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.graphics.update(cx, |graphics, cx| {
                                            graphics.reconnect_graphics_session(cx);
                                        });
                                    }),
                                ),
                            )
                        },
                    ),
            )
            .into_any_element()
    }

    fn render_graphics_center_state(
        &self,
        icon: LucideIcon,
        label: String,
        color: u32,
        action: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(self.tokens.ui.bg))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.0))
                    .text_align(gpui::TextAlign::Center)
                    .child(if matches!(icon, LucideIcon::LoaderCircle) {
                        self.render_loading_icon("graphics-center-loading", 28.0, rgb(color))
                    } else {
                        Self::render_lucide_icon(icon, 28.0, rgb(color))
                    })
                    .child(
                        div()
                            .max_w(px(GRAPHICS_SELECTOR_MAX_W))
                            .text_size(px(14.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(label),
                    )
                    .when_some(action, |panel, label| {
                        panel.child(
                            button_with(
                                &self.tokens,
                                label,
                                ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.graphics.update(cx, |graphics, cx| {
                                        graphics.start_graphics_load_if_needed(true);
                                        cx.notify();
                                    });
                                }),
                            ),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_graphics_not_available(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .px(px(GRAPHICS_SELECTOR_PADDING_X))
            .bg(rgb(theme.bg))
            .child(
                div()
                    .w_full()
                    .max_w(px(GRAPHICS_SELECTOR_MAX_W))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_20))
                    .bg(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_10))
                    .p(px(16.0))
                    .flex()
                    .gap(px(12.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::AlertTriangle,
                        20.0,
                        rgb(GRAPHICS_AMBER_500),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("graphics.not_available")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("graphics.no_distros")),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_graphics_error_box(&self, error: &str) -> AnyElement {
        div()
            .px(px(GRAPHICS_WARNING_PADDING_X))
            .py(px(GRAPHICS_WARNING_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((GRAPHICS_RED_500 << 8) | GRAPHICS_ALPHA_10))
            .text_color(rgb(GRAPHICS_RED_400))
            .text_size(px(13.0))
            .child(error.to_string())
            .into_any_element()
    }

    fn render_graphics_warning(&self, strong: String, detail: Option<String>) -> AnyElement {
        div()
            .px(px(GRAPHICS_WARNING_PADDING_X))
            .py(px(GRAPHICS_WARNING_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_10))
            .border_1()
            .border_color(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_20))
            .text_size(px(12.0))
            .text_color(rgb(GRAPHICS_AMBER_500))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(4.0))
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(strong))
                    .when_some(detail, |line, detail| {
                        // WSL diagnostics can be longer than the settings
                        // column, so allow the detail to wrap within the card.
                        line.child(div().min_w(px(0.0)).child(detail))
                    }),
            )
            .into_any_element()
    }

    fn render_graphics_default_badge(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .ml(px(8.0))
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((theme.accent << 8) | GRAPHICS_ALPHA_10))
            .text_size(px(11.0))
            .text_color(rgb(theme.accent))
            .child("Default")
            .into_any_element()
    }

    fn render_graphics_wslg_badge(&self, status: Option<&WslgStatus>) -> AnyElement {
        let Some(status) = status else {
            return div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(self.tokens.radii.sm))
                .bg(rgba((self.tokens.ui.text_muted << 8) | GRAPHICS_ALPHA_10))
                .text_size(px(GRAPHICS_BADGE_TEXT_SIZE))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child("WSLg N/A")
                .into_any_element();
        };
        if !status.available {
            return div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(self.tokens.radii.sm))
                .bg(rgba((self.tokens.ui.text_muted << 8) | GRAPHICS_ALPHA_10))
                .text_size(px(GRAPHICS_BADGE_TEXT_SIZE))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child("WSLg N/A")
                .into_any_element();
        }
        let label = match (status.wayland, status.x11) {
            (true, true) => "Wayland + X11",
            (true, false) => "Wayland",
            (false, true) => "X11",
            (false, false) => "WSLg",
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba((GRAPHICS_GREEN_500 << 8) | GRAPHICS_ALPHA_10))
                    .text_size(px(GRAPHICS_BADGE_TEXT_SIZE))
                    .text_color(rgb(GRAPHICS_GREEN_500))
                    .child(
                        div()
                            .size(px(GRAPHICS_DOT_SIZE))
                            .rounded_full()
                            .bg(rgb(GRAPHICS_GREEN_500)),
                    )
                    .child(label.to_string()),
            )
            .when(!status.has_openbox, |row| {
                row.child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_10))
                        .text_size(px(GRAPHICS_BADGE_TEXT_SIZE))
                        .text_color(rgb(GRAPHICS_AMBER_500))
                        .child(self.i18n.t("graphics.openbox_missing")),
                )
            })
            .into_any_element()
    }
}

impl GraphicsWorkspaceEntity {
    fn set_surface_visible(&mut self, visible: bool, cx: &mut Context<Self>) -> bool {
        let pending_frame = self.worker_delivery.frame_slot.set_visible(visible);
        let Some((session_id, frame)) = pending_frame else {
            return false;
        };
        // Visibility resumes only presentation; the session and VNC worker survive hiding.
        let changed = self.apply_vnc_frame(session_id, frame);
        if changed {
            cx.notify();
        }
        changed
    }

    fn start_graphics_load_if_needed(&mut self, force: bool) {
        if self.loading || (!force && !self.distros.is_empty()) {
            return;
        }
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        let delivery = self.worker_delivery.clone();
        self.runtime.spawn(async move {
            let result = wsl::list_distros().map_err(|error| error.to_string());
            delivery.send(GraphicsWorkerResult::LoadDistros { generation, result });
        });
    }

    fn load_sessions(&self) {
        let delivery = self.worker_delivery.clone();
        let backend = self.backend.clone();
        self.runtime.spawn(async move {
            let result = Ok::<_, String>(backend.list_sessions().await);
            delivery.send(GraphicsWorkerResult::ListSessions { result });
        });
    }

    fn start_wslg_detection(&self, generation: u64) {
        for distro in self.distros.iter().filter(|distro| distro.is_running) {
            let delivery = self.worker_delivery.clone();
            let backend = self.backend.clone();
            let distro_name = distro.name.clone();
            self.runtime.spawn(async move {
                let result = backend
                    .detect_wslg(&distro_name)
                    .await
                    .map_err(|error| error.to_string());
                delivery.send(GraphicsWorkerResult::DetectWslg {
                    generation,
                    distro: distro_name,
                    result,
                });
            });
        }
    }

    fn start_graphics_desktop(&mut self, distro: String, cx: &mut Context<Self>) {
        if self.status == GraphicsStatus::Starting {
            return;
        }
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        self.status = GraphicsStatus::Starting;
        self.error = None;
        self.session = None;
        self.reset_vnc_viewer(true);
        let delivery = self.worker_delivery.clone();
        let backend = self.backend.clone();
        self.runtime.spawn(async move {
            let result = backend
                .start_desktop(distro)
                .await
                .map_err(|error| error.to_string());
            delivery.send(GraphicsWorkerResult::Start { generation, result });
        });
        cx.notify();
    }

    fn start_graphics_app(&mut self, cx: &mut Context<Self>) {
        if self.status == GraphicsStatus::Starting {
            return;
        }
        let Some(distro) = self.selected_distro.clone() else {
            return;
        };
        let argv = split_graphics_app_command(&self.app_command);
        if argv.is_empty() {
            return;
        }
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        self.status = GraphicsStatus::Starting;
        self.error = None;
        self.session = None;
        self.reset_vnc_viewer(true);
        let delivery = self.worker_delivery.clone();
        let backend = self.backend.clone();
        self.runtime.spawn(async move {
            let result = backend
                .start_app(distro, argv, None, None)
                .await
                .map_err(|error| error.to_string());
            delivery.send(GraphicsWorkerResult::StartApp { generation, result });
        });
        cx.notify();
    }

    pub(super) fn shutdown_graphics_session(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session.as_ref().map(|session| session.id.clone()) else {
            self.reset_vnc_viewer(true);
            cx.notify();
            return;
        };
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        self.reset_vnc_viewer(true);
        let delivery = self.worker_delivery.clone();
        let backend = self.backend.clone();
        self.runtime.spawn(async move {
            let result = backend
                .stop(&session_id)
                .await
                .map_err(|error| error.to_string());
            delivery.send(GraphicsWorkerResult::Stop {
                generation,
                session_id,
                result,
            });
        });
        cx.notify();
    }

    fn stop_graphics_session(&mut self, cx: &mut Context<Self>) {
        if self.session.is_some() {
            self.shutdown_graphics_session(cx);
        } else {
            self.status = GraphicsStatus::Idle;
            self.error = None;
            self.reset_vnc_viewer(true);
            cx.notify();
        }
    }

    fn stop_graphics_session_id(&mut self, session_id: String, cx: &mut Context<Self>) {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.id == session_id)
        {
            self.shutdown_graphics_session(cx);
            return;
        }
        let delivery = self.worker_delivery.clone();
        let backend = self.backend.clone();
        self.runtime.spawn(async move {
            let result = backend
                .stop(&session_id)
                .await
                .map_err(|error| error.to_string());
            delivery.send(GraphicsWorkerResult::ListSessions {
                result: result.map(|_| Vec::new()),
            });
            let sessions = backend.list_sessions().await;
            delivery.send(GraphicsWorkerResult::ListSessions {
                result: Ok(sessions),
            });
        });
        cx.notify();
    }

    fn reconnect_graphics_session(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session.as_ref().map(|session| session.id.clone()) else {
            return;
        };
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        self.status = GraphicsStatus::Starting;
        self.error = None;
        self.reset_vnc_viewer(true);
        let delivery = self.worker_delivery.clone();
        let backend = self.backend.clone();
        self.runtime.spawn(async move {
            let result = backend
                .reconnect(&session_id)
                .await
                .map_err(|error| error.to_string());
            delivery.send(GraphicsWorkerResult::Reconnect { generation, result });
        });
        cx.notify();
    }

    fn reconnect_graphics_session_id(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        self.status = GraphicsStatus::Starting;
        self.error = None;
        self.reset_vnc_viewer(true);
        let delivery = self.worker_delivery.clone();
        let backend = self.backend.clone();
        self.runtime.spawn(async move {
            let result = backend
                .reconnect(&session_id)
                .await
                .map_err(|error| error.to_string());
            delivery.send(GraphicsWorkerResult::Reconnect { generation, result });
        });
        cx.notify();
    }

    fn apply_vnc_event(&mut self, event: GraphicsVncWorkerEvent) -> bool {
        match event {
            GraphicsVncWorkerEvent::Connected { session_id } => {
                if !self.session_matches(&session_id) {
                    return false;
                }
                self.status = GraphicsStatus::Active;
                self.error = None;
                true
            }
            GraphicsVncWorkerEvent::Frame { session_id, frame } => {
                self.apply_vnc_frame(session_id, frame)
            }
            GraphicsVncWorkerEvent::Disconnected { session_id, reason } => {
                if !self.session_matches(&session_id) {
                    return false;
                }
                self.vnc_session_id = None;
                self.vnc_input = None;
                self.vnc_stop = None;
                self.vnc_button_mask = 0;
                self.vnc_frame = None;
                self.worker_delivery.frame_slot.clear();
                let old_image = self.vnc_render_image.take();
                self.retire_vnc_image(old_image);
                if let Some(reason) = reason {
                    self.status = GraphicsStatus::Error;
                    self.error = Some(normalize_graphics_error(reason));
                } else {
                    self.status = GraphicsStatus::Disconnected;
                }
                true
            }
        }
    }

    fn apply_vnc_frame(&mut self, session_id: String, frame: GraphicsVncFrame) -> bool {
        if !self.session_matches(&session_id) {
            return false;
        }
        if let Some(render_image) = frame.render_image() {
            let old_image = self.vnc_render_image.replace(render_image);
            self.retire_vnc_image(old_image);
        }
        self.vnc_frame = Some(frame);
        self.status = GraphicsStatus::Active;
        self.error = None;
        true
    }

    fn retire_vnc_image(&mut self, image: Option<Arc<RenderImage>>) {
        if let Some(image) = image {
            self.vnc_retired_images.push(image);
            // Wake the entity-owned cleanup path even when retirement came from a UI action.
            self.worker_delivery.wake.mark();
        }
    }

    fn session_matches(&self, session_id: &str) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.id == session_id)
    }

    fn reset_vnc_viewer(&mut self, clear_frame: bool) {
        // The native VNC worker is a viewer concern: the WSL graphics crate owns
        // VNC/server processes, while GPUI owns the client connection and input
        // routing. Always stop the viewer before switching sessions.
        if let Some(stop) = self.vnc_stop.take() {
            let _ = stop.send(());
        }
        self.vnc_session_id = None;
        self.vnc_input = None;
        self.vnc_button_mask = 0;
        self.vnc_geometry.clear();
        if clear_frame {
            self.worker_delivery.frame_slot.clear();
            self.vnc_frame = None;
            let image = self.vnc_render_image.take();
            self.retire_vnc_image(image);
        }
    }

    fn ensure_vnc_worker(&mut self) {
        let Some(session) = self.session.clone() else {
            self.reset_vnc_viewer(true);
            return;
        };
        if self.vnc_session_id.as_deref() == Some(session.id.as_str()) && self.vnc_input.is_some() {
            return;
        }

        self.reset_vnc_viewer(true);
        let (input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let event_delivery = self.worker_delivery.clone();
        let session_id = session.id.clone();
        let vnc_port = session.vnc_port;
        self.vnc_session_id = Some(session_id.clone());
        self.vnc_input = Some(input_tx);
        self.vnc_stop = Some(stop_tx);
        self.runtime.spawn(async move {
            run_graphics_vnc_worker(session_id, vnc_port, input_rx, stop_rx, move |event| {
                event_delivery.send(GraphicsWorkerResult::VncEvent(event));
            })
            .await;
        });
    }

    fn send_vnc_input(&mut self, input: GraphicsVncInput) -> bool {
        self.vnc_input
            .as_ref()
            .is_some_and(|sender| sender.send(input).is_ok())
    }

    fn send_vnc_pointer(
        &mut self,
        position: Point<Pixels>,
        button_update: Option<(MouseButton, bool)>,
    ) -> bool {
        let Some((x, y)) = self.vnc_geometry.pointer(position) else {
            return false;
        };
        if let Some((button, pressed)) = button_update {
            let mask = vnc_button_mask(button);
            if pressed {
                self.vnc_button_mask |= mask;
            } else {
                self.vnc_button_mask &= !mask;
            }
        }
        self.send_vnc_input(GraphicsVncInput::Pointer {
            x,
            y,
            buttons: self.vnc_button_mask,
        })
    }

    fn send_vnc_scroll(&mut self, position: Point<Pixels>, delta: &gpui::ScrollDelta) {
        let Some((x, y)) = self.vnc_geometry.pointer(position) else {
            return;
        };
        for mask in vnc_scroll_masks(delta) {
            let _ = self.send_vnc_input(GraphicsVncInput::Pointer {
                x,
                y,
                buttons: self.vnc_button_mask | mask,
            });
            let _ = self.send_vnc_input(GraphicsVncInput::Pointer {
                x,
                y,
                buttons: self.vnc_button_mask,
            });
        }
    }
}

impl WorkspaceApp {
    fn graphics_canvas_diagnostics_text(&self) -> String {
        let backend = if self.detected_graphics.driver_name.is_empty() {
            format!("{:?}", self.detected_graphics.kind)
        } else {
            self.detected_graphics.driver_name.clone()
        };
        format!("{}: {backend}", self.i18n.t("graphics.gpu_canvas_backend"))
    }
}

#[derive(Clone, Copy)]
enum GraphicsToolbarAction {
    Reconnect,
    Fullscreen,
    Stop,
}

fn graphics_session_title(session: &WslGraphicsSession) -> String {
    match &session.mode {
        GraphicsSessionMode::Desktop => session.desktop_name.clone(),
        GraphicsSessionMode::App { title, argv } => title
            .clone()
            .or_else(|| argv.first().cloned())
            .unwrap_or_else(|| session.desktop_name.clone()),
    }
}

fn split_graphics_app_command(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_graphics_error(error: String) -> String {
    if error.contains("only available on Windows")
        || error == WslGraphicsError::UnsupportedPlatform.to_string()
    {
        WSL_GRAPHICS_UNAVAILABLE.to_string()
    } else {
        error
    }
}

#[cfg(test)]
mod delivery_tests {
    use gpui::TestAppContext;

    use super::*;

    fn test_graphics_session(session_id: &str) -> WslGraphicsSession {
        WslGraphicsSession {
            id: session_id.to_string(),
            vnc_port: 5900,
            distro: "test-distro".to_string(),
            desktop_name: "Test Desktop".to_string(),
            mode: GraphicsSessionMode::Desktop,
        }
    }

    fn test_graphics_frame(marker: u8) -> GraphicsVncFrame {
        GraphicsVncFrame {
            width: 1,
            height: 1,
            bgra: vec![marker, 0, 0, u8::MAX],
        }
    }

    #[test]
    fn worker_delivery_marks_wake_after_enqueue() {
        let (sender, receiver) = mpsc::channel();
        let delivery = GraphicsWorkerDelivery::new(sender);

        delivery.send(GraphicsWorkerResult::ListSessions {
            result: Ok(Vec::new()),
        });

        assert!(delivery.wake.take());
        assert!(matches!(
            receiver.try_recv(),
            Ok(GraphicsWorkerResult::ListSessions { result: Ok(sessions) }) if sessions.is_empty()
        ));
    }

    #[test]
    fn worker_wake_coalesces_bursts_and_stops_explicitly() {
        let wake = GraphicsWorkerWake::default();

        wake.mark();
        wake.mark();

        assert!(wake.take());
        assert!(!wake.take());
        wake.stop();
        assert!(wake.is_stopped());
    }

    #[test]
    fn adjacent_frames_coalesce_without_crossing_lifecycle_boundaries() {
        let session_id = "graphics-session".to_string();
        let results = vec![
            GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Connected {
                session_id: session_id.clone(),
            }),
            GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Frame {
                session_id: session_id.clone(),
                frame: GraphicsVncFrame {
                    width: 1,
                    height: 1,
                    bgra: vec![1, 0, 0, 0],
                },
            }),
            GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Frame {
                session_id: session_id.clone(),
                frame: GraphicsVncFrame {
                    width: 1,
                    height: 1,
                    bgra: vec![2, 0, 0, 0],
                },
            }),
            GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Disconnected {
                session_id,
                reason: None,
            }),
        ];

        let coalesced = coalesce_adjacent_graphics_frames(results);

        assert_eq!(coalesced.len(), 3);
        assert!(matches!(
            &coalesced[1],
            GraphicsWorkerResult::VncEvent(GraphicsVncWorkerEvent::Frame { frame, .. })
                if frame.bgra == vec![2, 0, 0, 0]
        ));
    }

    #[gpui::test]
    fn visible_hidden_frames_resume_only_the_latest_frame(cx: &mut TestAppContext) {
        let runtime = Arc::new(tokio::runtime::Runtime::new().expect("graphics test runtime"));
        let graphics = cx.new(|cx| {
            GraphicsWorkspaceEntity::new(
                Arc::new(oxideterm_wsl_graphics::WslGraphicsState::new()),
                runtime,
                cx,
            )
        });
        let (input_tx, _input_rx) = tokio::sync::mpsc::unbounded_channel();
        graphics.update(cx, |graphics, cx| {
            graphics.session = Some(test_graphics_session("graphics-session"));
            graphics.vnc_session_id = Some("graphics-session".to_string());
            graphics.vnc_input = Some(input_tx);
            graphics.set_surface_visible(true, cx);
            graphics
                .worker_delivery
                .send(GraphicsWorkerResult::VncEvent(
                    GraphicsVncWorkerEvent::Frame {
                        session_id: "graphics-session".to_string(),
                        frame: test_graphics_frame(1),
                    },
                ));
            assert!(graphics.worker_delivery.wake.take());
            let _ = graphics.drain_worker_results(cx);
        });
        let visible_image = graphics.read_with(cx, |graphics, _cx| {
            assert_eq!(
                graphics.vnc_frame.as_ref().map(|frame| frame.bgra[0]),
                Some(1)
            );
            graphics
                .vnc_render_image
                .clone()
                .expect("visible frame image")
        });

        graphics.update(cx, |graphics, cx| {
            graphics.set_surface_visible(false, cx);
            for marker in [2, 3] {
                graphics
                    .worker_delivery
                    .send(GraphicsWorkerResult::VncEvent(
                        GraphicsVncWorkerEvent::Frame {
                            session_id: "graphics-session".to_string(),
                            frame: test_graphics_frame(marker),
                        },
                    ));
            }
            assert!(!graphics.worker_delivery.wake.take());
            assert!(graphics.worker_delivery.frame_slot.has_pending_frame());
            assert_eq!(
                graphics.vnc_frame.as_ref().map(|frame| frame.bgra[0]),
                Some(1)
            );
            assert!(Arc::ptr_eq(
                graphics
                    .vnc_render_image
                    .as_ref()
                    .expect("retained visible image"),
                &visible_image
            ));

            assert!(graphics.set_surface_visible(true, cx));
            assert!(!graphics.worker_delivery.frame_slot.has_pending_frame());
            assert_eq!(
                graphics.vnc_frame.as_ref().map(|frame| frame.bgra[0]),
                Some(3)
            );
            assert!(!Arc::ptr_eq(
                graphics
                    .vnc_render_image
                    .as_ref()
                    .expect("resumed frame image"),
                &visible_image
            ));
            assert!(graphics.session.is_some());
            assert!(graphics.vnc_input.is_some());
        });
    }

    #[gpui::test]
    fn hidden_surface_delivery_and_release_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let runtime = Arc::new(tokio::runtime::Runtime::new().expect("graphics test runtime"));
        let graphics = cx.new(|cx| {
            GraphicsWorkspaceEntity::new(
                Arc::new(oxideterm_wsl_graphics::WslGraphicsState::new()),
                runtime,
                cx,
            )
        });
        let _subscription = graphics.update(cx, |_, cx| {
            cx.subscribe(
                &graphics,
                |graphics, _, event: &GraphicsWorkspaceEvent, cx| match event {
                    GraphicsWorkspaceEvent::WorkerResultsReady => {
                        let _ = graphics.drain_worker_results(cx);
                    }
                },
            )
        });
        let wake = graphics.update(cx, |graphics, _cx| graphics.worker_delivery.wake.clone());
        graphics.update(cx, |graphics, _cx| {
            graphics.generation = 9;
            graphics.worker_delivery.send(GraphicsWorkerResult::Start {
                generation: 9,
                result: Err("test graphics failure".to_string()),
            });
        });

        // No graphics page is rendered; the entity still applies reliable completion.
        cx.run_until_parked();
        graphics.read_with(cx, |graphics, _cx| {
            assert_eq!(graphics.status, GraphicsStatus::Error);
            assert_eq!(graphics.error.as_deref(), Some("test graphics failure"));
        });

        let (input_tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel();
        graphics.update(cx, |graphics, cx| {
            graphics.session = Some(test_graphics_session("hidden-session"));
            graphics.vnc_session_id = Some("hidden-session".to_string());
            graphics.vnc_input = Some(input_tx);
            graphics.status = GraphicsStatus::Starting;
            graphics.set_surface_visible(false, cx);
            graphics
                .worker_delivery
                .send(GraphicsWorkerResult::VncEvent(
                    GraphicsVncWorkerEvent::Connected {
                        session_id: "hidden-session".to_string(),
                    },
                ));
        });
        cx.run_until_parked();
        graphics.update(cx, |graphics, _cx| {
            assert_eq!(graphics.status, GraphicsStatus::Active);
            assert!(graphics.send_vnc_input(GraphicsVncInput::Key {
                keysym: 0x61,
                down: true,
            }));
            graphics
                .worker_delivery
                .send(GraphicsWorkerResult::VncEvent(
                    GraphicsVncWorkerEvent::Frame {
                        session_id: "hidden-session".to_string(),
                        frame: test_graphics_frame(7),
                    },
                ));
            assert!(!graphics.worker_delivery.wake.take());
            assert!(graphics.worker_delivery.frame_slot.has_pending_frame());
            graphics
                .worker_delivery
                .send(GraphicsWorkerResult::VncEvent(
                    GraphicsVncWorkerEvent::Disconnected {
                        session_id: "hidden-session".to_string(),
                        reason: Some("test disconnect".to_string()),
                    },
                ));
        });
        assert!(matches!(
            input_rx.try_recv(),
            Ok(GraphicsVncInput::Key {
                keysym: 0x61,
                down: true
            })
        ));
        cx.run_until_parked();
        graphics.read_with(cx, |graphics, _cx| {
            assert_eq!(graphics.status, GraphicsStatus::Error);
            assert_eq!(graphics.error.as_deref(), Some("test disconnect"));
            assert!(graphics.session.is_some());
            assert!(graphics.vnc_render_image.is_none());
            assert!(!graphics.worker_delivery.frame_slot.has_pending_frame());
            assert!(!wake.is_stopped());
        });

        drop(graphics);
        cx.update(|_cx| {});
        assert!(wake.is_stopped());
    }
}
