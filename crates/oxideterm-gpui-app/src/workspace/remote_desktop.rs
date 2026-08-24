use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use super::forwards;

use oxideterm_gpui_remote_desktop::{
    RemoteDesktopFrameApplyStats, RemoteDesktopMappedPoint, RemoteDesktopViewState,
    SharedRemoteDesktopGeometry, remote_desktop_surface_with_geometry,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions, ToolbarButtonOptions,
};
use oxideterm_gpui_ui::{
    context_menu::{
        context_menu_event_boundary, context_menu_item_height_estimate,
        context_menu_separator_height_estimate,
    },
    dropdown_menu::{
        DropdownMenuItemKind, dropdown_menu_content, dropdown_menu_item, dropdown_menu_label,
        dropdown_menu_separator,
    },
    modal::{
        dialog_overlay, modal_body, modal_container, modal_footer, modal_header,
        overlay_content_boundary,
    },
};
use oxideterm_remote_desktop::{
    NegotiatedCapabilities, NegotiatedCapabilityStatus, RemoteDesktopClipboardData,
    RemoteDesktopClipboardFormat, RemoteDesktopConnectionProfile, RemoteDesktopEndpoint,
    RemoteDesktopErrorCategory, RemoteDesktopFileConflictPolicy,
    RemoteDesktopFileTransferFailureKind, RemoteDesktopFrameDeliverySlot, RemoteDesktopHelperEvent,
    RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState, RemoteDesktopLockKeys,
    RemoteDesktopMonitor, RemoteDesktopMonitorLayout, RemoteDesktopMonitorOrientation,
    RemoteDesktopMouseButton, RemoteDesktopMouseButtonState, RemoteDesktopProtocol,
    RemoteDesktopProviderManifest, RemoteDesktopRemoteFileEntry, RemoteDesktopRemoteFileKind,
    RemoteDesktopSecret, RemoteDesktopSessionStatus, RemoteDesktopSize, RemoteDesktopWheelDelta,
    builtin_preview_provider_registry, builtin_provider_registry,
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};
use tokio::sync::Notify;
use zeroize::Zeroizing;

use super::*;

mod certificate;
mod clipboard;
mod input;
mod interaction;
mod public_mcp;
mod session;
mod vendor_files;
mod view;
mod worker;

pub(in crate::workspace) use public_mcp::RemoteDesktopPublicClipboardSnapshot;

use certificate::*;
use clipboard::*;
use input::*;
use vendor_files::*;
use worker::*;

const REMOTE_DESKTOP_INITIAL_WIDTH: u32 = 1280;
const REMOTE_DESKTOP_INITIAL_HEIGHT: u32 = 720;
const REMOTE_DESKTOP_SCROLL_LINE: f32 = 38.0;
const REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_INTERVAL: Duration = Duration::from_millis(16);
const REMOTE_DESKTOP_INITIAL_LAYOUT_PROBE_TICKS: usize = 120;
const REMOTE_DESKTOP_RESIZE_DEBOUNCE: Duration = Duration::from_millis(120);
const REMOTE_DESKTOP_RESIZE_DELTA_THRESHOLD: u32 = 16;
const REMOTE_DESKTOP_RESIZE_MENU_WIDTH: f32 = 240.0;
const REMOTE_DESKTOP_RESIZE_MENU_GAP: f32 = 4.0;
const REMOTE_DESKTOP_RESIZE_MENU_VIEWPORT_PADDING: f32 = 8.0;
const REMOTE_DESKTOP_COMMON_RESOLUTIONS: [RemoteDesktopSize; 5] = [
    RemoteDesktopSize {
        width: 1280,
        height: 720,
    },
    RemoteDesktopSize {
        width: 1366,
        height: 768,
    },
    RemoteDesktopSize {
        width: 1920,
        height: 1080,
    },
    RemoteDesktopSize {
        width: 2560,
        height: 1440,
    },
    RemoteDesktopSize {
        width: 3840,
        height: 2160,
    },
];
const REMOTE_DESKTOP_DEFAULT_SCALE_FACTOR_PERCENT: u32 = 100;
const REMOTE_DESKTOP_MIN_SCALE_FACTOR_PERCENT: u32 = 100;
const REMOTE_DESKTOP_MAX_SCALE_FACTOR_PERCENT: u32 = 500;
const REMOTE_DESKTOP_SCALE_PERCENT_MULTIPLIER: f32 = 100.0;
const REMOTE_DESKTOP_SCROLL_PIXEL_STEP: f32 = 120.0;
const REMOTE_DESKTOP_DELIVERY_BUDGET: delivery::DeliveryBudget =
    delivery::DeliveryBudget::new(64, Duration::from_millis(4));
const REMOTE_DESKTOP_FRAME_READY_DRAIN_LIMIT: usize = 32;
const REMOTE_DESKTOP_FRAME_READY_DRAIN_BUDGET: Duration = Duration::from_millis(6);
const REMOTE_DESKTOP_DIAGNOSTICS_ENV: &str = "OXIDETERM_REMOTE_DESKTOP_DIAGNOSTICS";
const REMOTE_DESKTOP_AUTOMATIC_RECONNECT_DELAYS: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
];

fn remote_desktop_automatic_reconnect_delay(attempt: usize) -> Duration {
    REMOTE_DESKTOP_AUTOMATIC_RECONNECT_DELAYS
        .get(attempt)
        .copied()
        .unwrap_or_else(|| {
            REMOTE_DESKTOP_AUTOMATIC_RECONNECT_DELAYS
                .last()
                .copied()
                .unwrap_or(Duration::ZERO)
        })
}

fn remote_desktop_network_failure_allows_automatic_reconnect(
    has_connected: bool,
    category: Option<RemoteDesktopErrorCategory>,
) -> bool {
    // Initial setup, authentication, protocol, and dependency failures need a
    // user decision. Only a network failure after a proven session is retried.
    has_connected && category == Some(RemoteDesktopErrorCategory::Network)
}

fn remote_desktop_tab_visible(main_tab_visible: bool, detached_tab_visible: bool) -> bool {
    main_tab_visible || detached_tab_visible
}

fn remote_desktop_resolution_label(size: RemoteDesktopSize) -> String {
    format!("{} × {}", size.width, size.height)
}

#[derive(Debug)]
pub(super) enum RemoteDesktopWorkerDelivery {
    FrameReady {
        tab_id: TabId,
        generation: u64,
    },
    FrameRecoveryRequired {
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

pub(super) enum RemoteDesktopDeliveryIntent {
    ClipboardTransferFailed,
    VncFileTransferCompleted,
    VncFileTransferFailed(RemoteDesktopFileTransferFailureKind),
}

pub(super) struct RemoteDesktopDeliveryOutcome {
    changed: bool,
    backlog_remaining: bool,
    intents: Vec<RemoteDesktopDeliveryIntent>,
}

#[derive(Clone)]
struct RemoteDesktopWorkerWake {
    pending: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    notification: Arc<Notify>,
}

impl Default for RemoteDesktopWorkerWake {
    fn default() -> Self {
        Self {
            pending: Arc::new(AtomicBool::new(false)),
            stopped: Arc::new(AtomicBool::new(false)),
            notification: Arc::new(Notify::new()),
        }
    }
}

impl RemoteDesktopWorkerWake {
    fn mark(&self) {
        // Worker threads cannot touch GPUI state directly. Notify stores one
        // permit when the foreground task has not started waiting yet.
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

#[cfg(test)]
mod visibility_tests {
    use super::{
        REMOTE_DESKTOP_AUTOMATIC_RECONNECT_DELAYS, remote_desktop_automatic_reconnect_delay,
        remote_desktop_network_failure_allows_automatic_reconnect,
    };
    use oxideterm_remote_desktop::RemoteDesktopErrorCategory;

    #[test]
    fn automatic_reconnect_backoff_caps_at_the_last_delay() {
        assert_eq!(
            remote_desktop_automatic_reconnect_delay(0),
            REMOTE_DESKTOP_AUTOMATIC_RECONNECT_DELAYS[0]
        );
        assert_eq!(
            remote_desktop_automatic_reconnect_delay(usize::MAX),
            *REMOTE_DESKTOP_AUTOMATIC_RECONNECT_DELAYS.last().unwrap()
        );
    }

    #[test]
    fn automatic_reconnect_requires_an_established_network_session() {
        assert!(remote_desktop_network_failure_allows_automatic_reconnect(
            true,
            Some(RemoteDesktopErrorCategory::Network)
        ));
        assert!(!remote_desktop_network_failure_allows_automatic_reconnect(
            false,
            Some(RemoteDesktopErrorCategory::Network)
        ));
        assert!(!remote_desktop_network_failure_allows_automatic_reconnect(
            true,
            Some(RemoteDesktopErrorCategory::Authentication)
        ));
        assert!(!remote_desktop_network_failure_allows_automatic_reconnect(
            true,
            Some(RemoteDesktopErrorCategory::Unknown)
        ));
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RemoteDesktopRenderDiagnostics {
    batches: u64,
    events_drained: u64,
    drain_budget_hits: u64,
    full_frames: u64,
    frame_updates: u64,
    dirty_updates_applied: u64,
    dirty_updates_rejected: u64,
    full_update_recoveries: u64,
    corrupted_frames: u64,
    first_trace_id: Option<u64>,
    last_trace_id: Option<u64>,
    dirty_rect_pixels: u64,
    dirty_frame_pixels: u64,
    pending_texture_updates: u64,
    pending_texture_upload_bytes: u64,
    dirty_tiles_refreshed: u64,
    frame_tiles_created: u64,
    retired_images: u64,
    total_apply_micros: u64,
    max_apply_micros: u64,
}

impl RemoteDesktopRenderDiagnostics {
    fn record_batch(
        &mut self,
        drained_events: usize,
        budget_hit: bool,
        apply_elapsed: Duration,
        apply_stats: RemoteDesktopFrameApplyStats,
        retired_images: usize,
    ) {
        self.batches = self.batches.saturating_add(1);
        self.events_drained = self.events_drained.saturating_add(drained_events as u64);
        if budget_hit {
            self.drain_budget_hits = self.drain_budget_hits.saturating_add(1);
        }
        self.full_frames = self
            .full_frames
            .saturating_add(apply_stats.full_frames as u64);
        self.frame_updates = self
            .frame_updates
            .saturating_add(apply_stats.frame_updates as u64);
        self.dirty_updates_applied = self
            .dirty_updates_applied
            .saturating_add(apply_stats.dirty_updates_applied as u64);
        self.dirty_updates_rejected = self
            .dirty_updates_rejected
            .saturating_add(apply_stats.dirty_updates_rejected as u64);
        self.full_update_recoveries = self
            .full_update_recoveries
            .saturating_add(apply_stats.full_update_recoveries as u64);
        self.corrupted_frames = self
            .corrupted_frames
            .saturating_add(apply_stats.corrupted_frames as u64);
        if self.first_trace_id.is_none() {
            self.first_trace_id = apply_stats.first_trace_id;
        }
        if apply_stats.last_trace_id.is_some() {
            self.last_trace_id = apply_stats.last_trace_id;
        }
        self.dirty_rect_pixels = self
            .dirty_rect_pixels
            .saturating_add(apply_stats.dirty_rect_pixels);
        self.dirty_frame_pixels = self
            .dirty_frame_pixels
            .saturating_add(apply_stats.dirty_frame_pixels);
        self.pending_texture_updates = apply_stats.pending_texture_updates as u64;
        self.pending_texture_upload_bytes = apply_stats.pending_texture_upload_bytes as u64;
        self.dirty_tiles_refreshed = self
            .dirty_tiles_refreshed
            .saturating_add(apply_stats.dirty_tiles_refreshed as u64);
        self.frame_tiles_created = self
            .frame_tiles_created
            .saturating_add(apply_stats.frame_tiles_created as u64);
        self.retired_images = self.retired_images.saturating_add(retired_images as u64);
        let apply_micros = duration_micros_u64(apply_elapsed);
        self.total_apply_micros = self.total_apply_micros.saturating_add(apply_micros);
        self.max_apply_micros = self.max_apply_micros.max(apply_micros);
    }
}

fn send_remote_desktop_worker_delivery(
    delivery_tx: &mpsc::Sender<RemoteDesktopWorkerDelivery>,
    worker_wake: &RemoteDesktopWorkerWake,
    delivery: RemoteDesktopWorkerDelivery,
) {
    // Publish before waking so the foreground task cannot observe an empty queue.
    if delivery_tx.send(delivery).is_ok() {
        worker_wake.mark();
    }
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

struct RemoteDesktopWorkerOwner {
    request_tx: Option<mpsc::Sender<RemoteDesktopHelperRequest>>,
    worker_thread: Option<thread::JoinHandle<()>>,
}

impl RemoteDesktopWorkerOwner {
    fn new(
        request_tx: mpsc::Sender<RemoteDesktopHelperRequest>,
        worker_thread: thread::JoinHandle<()>,
    ) -> Self {
        Self {
            request_tx: Some(request_tx),
            worker_thread: Some(worker_thread),
        }
    }

    fn request_sender(&self) -> Option<&mpsc::Sender<RemoteDesktopHelperRequest>> {
        self.request_tx.as_ref()
    }

    fn request_sender_cloned(&self) -> Option<mpsc::Sender<RemoteDesktopHelperRequest>> {
        self.request_tx.clone()
    }

    fn send(&self, request: RemoteDesktopHelperRequest) {
        if let Some(request_tx) = self.request_tx.as_ref() {
            let _ = request_tx.send(request);
        }
    }

    fn shutdown(&mut self) {
        if let Some(request_tx) = self.request_tx.take() {
            // Session shutdown and helper replacement release input state before
            // asking the helper to exit cooperatively.
            let _ = request_tx.send(RemoteDesktopHelperRequest::ReleaseAllInputs);
            let _ = request_tx.send(RemoteDesktopHelperRequest::Close);
        }
        self.retire_worker_thread();
    }

    fn retire_worker_thread(&mut self) {
        let Some(worker_thread) = self.worker_thread.take() else {
            return;
        };
        if worker_thread.is_finished() {
            let _ = worker_thread.join();
            return;
        }

        let reaper_name = worker_thread
            .thread()
            .name()
            .map(|name| format!("{name}-reaper"))
            .unwrap_or_else(|| "remote-desktop-worker-reaper".to_string());
        // The protocol worker has its own helper timeout, so this reaper owns a
        // bounded join without blocking the GPUI thread during session teardown.
        let _ = thread::Builder::new().name(reaper_name).spawn(move || {
            let _ = worker_thread.join();
        });
    }
}

impl Drop for RemoteDesktopWorkerOwner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

enum RemoteDesktopPublicClipboard {
    Text(Zeroizing<String>),
    Image {
        format: RemoteDesktopClipboardFormat,
        bytes: Zeroizing<Vec<u8>>,
    },
}

pub(in crate::workspace) struct RemoteDesktopSshTunnelLease {
    lease_id: String,
    forwarding_service: forwards::ForwardingRuntimeService,
}

pub(in crate::workspace) struct PendingRemoteDesktopSshTunnel {
    lease_id: Option<String>,
    forwarding_service: forwards::ForwardingRuntimeService,
    worker: Option<tokio::task::JoinHandle<Result<RemoteDesktopEndpoint, String>>>,
}

impl PendingRemoteDesktopSshTunnel {
    pub(in crate::workspace) fn new(
        lease_id: String,
        forwarding_service: forwards::ForwardingRuntimeService,
        worker: tokio::task::JoinHandle<Result<RemoteDesktopEndpoint, String>>,
    ) -> Self {
        Self {
            lease_id: Some(lease_id),
            forwarding_service,
            worker: Some(worker),
        }
    }

    pub(in crate::workspace) async fn finish(
        mut self,
    ) -> Result<(RemoteDesktopEndpoint, RemoteDesktopSshTunnelLease), String> {
        let worker = self
            .worker
            .take()
            .expect("pending remote desktop tunnel owns one worker");
        let endpoint = worker.await.map_err(|error| error.to_string())??;
        let lease_id = self
            .lease_id
            .take()
            .expect("pending remote desktop tunnel owns one lease id");
        let lease = RemoteDesktopSshTunnelLease::new(lease_id, self.forwarding_service.clone());
        Ok((endpoint, lease))
    }
}

impl Drop for PendingRemoteDesktopSshTunnel {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
        // Cancellation may race with listener creation, so the shared lease
        // registry remains the authoritative cleanup boundary.
        if let Some(lease_id) = self.lease_id.take() {
            self.forwarding_service
                .close_remote_desktop_tunnel(lease_id);
        }
    }
}

impl RemoteDesktopSshTunnelLease {
    pub(in crate::workspace) fn new(
        lease_id: String,
        forwarding_service: forwards::ForwardingRuntimeService,
    ) -> Self {
        Self {
            lease_id,
            forwarding_service,
        }
    }
}

impl Drop for RemoteDesktopSshTunnelLease {
    fn drop(&mut self) {
        // The lease stops only its hidden listener; NodeRouter retains the
        // physical SSH node for every other registered consumer.
        self.forwarding_service
            .close_remote_desktop_tunnel(self.lease_id.clone());
    }
}

pub(in crate::workspace) struct RemoteDesktopSessionEntity {
    tab_id: TabId,
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    password: Option<RemoteDesktopSecret>,
    certificate_store_path: PathBuf,
    certificate_challenge: Option<RemoteDesktopCertificateChallengeState>,
    session_trusted_certificate_fingerprint: Option<String>,
    state: RemoteDesktopViewState,
    geometry: SharedRemoteDesktopGeometry,
    frame_slot: RemoteDesktopFrameDeliverySlot,
    ui_frame_visible: bool,
    public_mcp_frame_observers: usize,
    public_mcp_clipboard: Option<RemoteDesktopPublicClipboard>,
    ssh_tunnel: Option<RemoteDesktopSshTunnelLease>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
    delivery_rx: mpsc::Receiver<RemoteDesktopWorkerDelivery>,
    worker: Option<RemoteDesktopWorkerOwner>,
    worker_wake: Option<RemoteDesktopWorkerWake>,
    worker_generation: u64,
    has_connected: bool,
    automatic_reconnect_attempt: usize,
    automatic_reconnect_worker_generation: Option<u64>,
    automatic_reconnect_task: Option<Task<()>>,
    window_handle: AnyWindowHandle,
    last_viewport_size: Option<RemoteDesktopSize>,
    last_sent_resize: Option<RemoteDesktopResizeRequestState>,
    last_viewport_scale_factor: Option<u32>,
    last_monitor_layout: RemoteDesktopMonitorLayout,
    resize_generation: Arc<AtomicU64>,
    follow_window_size: bool,
    last_input_modifiers: RemoteDesktopModifierState,
    last_lock_keys: Option<RemoteDesktopLockKeys>,
    pressed_mouse_buttons: HashSet<RemoteDesktopMouseButton>,
    wheel_pixel_remainder: RemoteDesktopWheelDelta,
    render_diagnostics: RemoteDesktopRenderDiagnostics,
    vnc_files: RemoteDesktopVncFileBrowserState,
}

impl RemoteDesktopSessionEntity {
    pub(in crate::workspace) fn active_session_protocol(&self) -> RemoteDesktopProtocol {
        self.profile.protocol
    }

    pub(in crate::workspace) fn active_session_status(&self) -> RemoteDesktopSessionStatus {
        // The protocol entity remains the authoritative source for sidebar liveness.
        self.state.snapshot().status
    }

    pub(in crate::workspace) fn ai_can_disconnect(&self) -> bool {
        matches!(
            self.state.snapshot().status,
            RemoteDesktopSessionStatus::Connecting
                | RemoteDesktopSessionStatus::Connected
                | RemoteDesktopSessionStatus::Reconnecting
        )
    }

    pub(in crate::workspace) fn ai_can_reconnect(&self) -> bool {
        remote_desktop_reconnect_mode(self.state.snapshot().status).is_some()
    }

    fn new(
        tab_id: TabId,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        password: Option<RemoteDesktopSecret>,
        certificate_store_path: PathBuf,
        frame_slot: RemoteDesktopFrameDeliverySlot,
        window_handle: AnyWindowHandle,
    ) -> Self {
        let (delivery_tx, delivery_rx) = mpsc::channel();
        let mut state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
            .with_read_only(profile.read_only);
        state.apply_event(RemoteDesktopHelperEvent::Status {
            status: RemoteDesktopSessionStatus::Connecting,
            message: None,
        });
        Self {
            tab_id,
            profile,
            provider,
            // The tab retains one zeroizing credential owner so a reconnect
            // can answer a fresh certificate-gated authentication request.
            password,
            certificate_store_path,
            certificate_challenge: None,
            session_trusted_certificate_fingerprint: None,
            state,
            geometry: SharedRemoteDesktopGeometry::default(),
            frame_slot,
            ui_frame_visible: false,
            public_mcp_frame_observers: 0,
            public_mcp_clipboard: None,
            ssh_tunnel: None,
            // Each tab owns its delivery mailbox. A wake for one detached window
            // must never drain another tab's lifecycle events or frame notices.
            delivery_tx,
            delivery_rx,
            worker: None,
            worker_wake: None,
            worker_generation: 0,
            has_connected: false,
            automatic_reconnect_attempt: 0,
            automatic_reconnect_worker_generation: None,
            automatic_reconnect_task: None,
            window_handle,
            last_viewport_size: None,
            last_sent_resize: None,
            last_viewport_scale_factor: None,
            last_monitor_layout: RemoteDesktopMonitorLayout::default(),
            resize_generation: Arc::new(AtomicU64::new(0)),
            follow_window_size: true,
            last_input_modifiers: RemoteDesktopModifierState::default(),
            last_lock_keys: None,
            pressed_mouse_buttons: HashSet::new(),
            wheel_pixel_remainder: remote_desktop_empty_wheel_delta(),
            render_diagnostics: RemoteDesktopRenderDiagnostics::default(),
            vnc_files: RemoteDesktopVncFileBrowserState::default(),
        }
    }

    fn schedule_worker_wake(
        &self,
        generation: u64,
        worker_wake: RemoteDesktopWorkerWake,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |session, cx| {
            loop {
                worker_wake.wait().await;
                let should_deliver = worker_wake.take();
                let stopped = worker_wake.is_stopped();
                if should_deliver {
                    let delivery_available = session
                        .update(cx, |current, cx| {
                            if current.worker_generation != generation {
                                return false;
                            }
                            cx.emit(RemoteDesktopSessionEvent::DeliveryReady { generation });
                            true
                        })
                        .unwrap_or(false);
                    if !delivery_available {
                        break;
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn schedule_frame_apply(&self, generation: u64, delay: Duration, cx: &mut Context<Self>) {
        cx.spawn(async move |session, cx| {
            if !delay.is_zero() {
                Timer::after(delay).await;
            }
            let _ = session.update(cx, |current, cx| {
                if current.worker_generation == generation {
                    cx.emit(RemoteDesktopSessionEvent::FrameApplyReady { generation });
                }
            });
        })
        .detach();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RemoteDesktopSessionEvent {
    DeliveryReady { generation: u64 },
    FrameApplyReady { generation: u64 },
    ClipboardTransferFailed,
    VncFileTransferCompleted,
    VncFileTransferFailed(RemoteDesktopFileTransferFailureKind),
}

impl gpui::EventEmitter<RemoteDesktopSessionEvent> for RemoteDesktopSessionEntity {}

/// Owns all remote desktop sessions independently from the workspace window shell.
pub(in crate::workspace) struct RemoteDesktopWorkspaceEntity {
    sessions: HashMap<TabId, Entity<RemoteDesktopSessionEntity>>,
    session_subscriptions: HashMap<TabId, Vec<Subscription>>,
}

impl RemoteDesktopWorkspaceEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            session_subscriptions: HashMap::new(),
        }
    }

    pub(in crate::workspace) fn session(
        &self,
        tab_id: TabId,
    ) -> Option<Entity<RemoteDesktopSessionEntity>> {
        self.sessions.get(&tab_id).cloned()
    }

    pub(in crate::workspace) fn insert(
        &mut self,
        tab_id: TabId,
        session: Entity<RemoteDesktopSessionEntity>,
        subscriptions: Vec<Subscription>,
    ) {
        self.sessions.insert(tab_id, session);
        self.session_subscriptions.insert(tab_id, subscriptions);
    }

    pub(in crate::workspace) fn remove(
        &mut self,
        tab_id: TabId,
    ) -> Option<Entity<RemoteDesktopSessionEntity>> {
        self.session_subscriptions.remove(&tab_id);
        self.sessions.remove(&tab_id)
    }

    pub(in crate::workspace) fn ai_snapshot(&self, cx: &App) -> serde_json::Value {
        let sessions = self
            .sessions
            .iter()
            .map(|(tab_id, session)| {
                let session = session.read(cx);
                let snapshot = session.state.snapshot();
                serde_json::json!({
                    "tabId": tab_id.0.to_string(),
                    "profileId": session.profile.id,
                    "label": snapshot.title,
                    "protocol": format!("{:?}", snapshot.protocol).to_lowercase(),
                    "host": session.profile.endpoint.host,
                    "port": session.profile.endpoint.port,
                    "status": format!("{:?}", snapshot.status).to_lowercase(),
                    "readOnly": snapshot.read_only,
                    "hasFrame": snapshot.has_frame,
                    "size": snapshot.size.map(|size| serde_json::json!({
                        "width": size.width,
                        "height": size.height,
                    })),
                    "errorCategory": snapshot.error_category
                        .map(|category| format!("{category:?}").to_lowercase()),
                    "negotiatedCapabilities": snapshot.negotiated_capabilities,
                    "canDisconnect": session.ai_can_disconnect(),
                    "canReconnect": remote_desktop_reconnect_mode(snapshot.status).is_some(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({ "sessions": sessions })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RemoteDesktopResizeRequestState {
    size: RemoteDesktopSize,
    scale_factor: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    struct RemoteDesktopTestRoot;

    impl Render for RemoteDesktopTestRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[test]
    fn worker_wake_uses_event_notification_and_stops_explicitly() {
        let wake = RemoteDesktopWorkerWake::default();
        let runtime = tokio::runtime::Runtime::new().unwrap();

        wake.mark();
        runtime.block_on(wake.wait());
        assert!(wake.take());

        wake.stop();
        runtime.block_on(wake.wait());
        assert!(wake.is_stopped());
    }

    #[test]
    fn worker_delivery_enqueues_before_marking_the_wake() {
        let (sender, receiver) = mpsc::channel();
        let wake = RemoteDesktopWorkerWake::default();
        let tab_id = TabId(7);

        send_remote_desktop_worker_delivery(
            &sender,
            &wake,
            RemoteDesktopWorkerDelivery::FrameReady {
                tab_id,
                generation: 3,
            },
        );

        assert!(wake.take());
        assert!(matches!(
            receiver.try_recv(),
            Ok(RemoteDesktopWorkerDelivery::FrameReady {
                tab_id: received_tab_id,
                generation: 3,
            }) if received_tab_id == tab_id
        ));
    }

    #[test]
    fn worker_deliveries_remain_isolated_by_session_mailbox() {
        let (first_sender, first_receiver) = mpsc::channel();
        let (second_sender, second_receiver) = mpsc::channel();
        let first_wake = RemoteDesktopWorkerWake::default();
        let second_wake = RemoteDesktopWorkerWake::default();

        send_remote_desktop_worker_delivery(
            &first_sender,
            &first_wake,
            RemoteDesktopWorkerDelivery::FrameReady {
                tab_id: TabId(1),
                generation: 4,
            },
        );

        assert!(first_wake.take());
        assert!(!second_wake.take());
        assert!(first_receiver.try_recv().is_ok());
        assert!(second_receiver.try_recv().is_err());

        // A second tab owns a distinct sender, receiver and wake permit.
        send_remote_desktop_worker_delivery(
            &second_sender,
            &second_wake,
            RemoteDesktopWorkerDelivery::FrameReady {
                tab_id: TabId(2),
                generation: 7,
            },
        );
        assert!(second_wake.take());
        assert!(second_receiver.try_recv().is_ok());
        assert!(first_receiver.try_recv().is_err());
    }

    #[gpui::test]
    fn session_release_stops_waiter_and_closes_only_its_helper(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, _cx| RemoteDesktopTestRoot);
        let protocol = RemoteDesktopProtocol::Rdp;
        let profile = preview_remote_desktop_profile(protocol);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let worker_wake = RemoteDesktopWorkerWake::default();
        let observed_wake = worker_wake.clone();
        let (request_tx, request_rx) = mpsc::channel();
        let session = cx.new(|cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                TabId(9),
                profile,
                provider,
                Some(RemoteDesktopSecret::from("release-test-secret")),
                std::env::temp_dir().join("oxideterm-release-test-certificates.json"),
                RemoteDesktopFrameDeliverySlot::new(),
                window.into(),
            );
            session.worker_wake = Some(worker_wake);
            session.worker = Some(RemoteDesktopWorkerOwner::new(
                request_tx,
                thread::spawn(|| {}),
            ));
            session.install_release_handler(cx);
            session
        });

        drop(session);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(observed_wake.is_stopped());
        assert!(matches!(
            request_rx.recv().unwrap(),
            RemoteDesktopHelperRequest::ReleaseAllInputs
        ));
        assert!(matches!(
            request_rx.recv().unwrap(),
            RemoteDesktopHelperRequest::Close
        ));
        assert!(request_rx.try_recv().is_err());
    }

    #[gpui::test]
    fn session_window_handoff_resumes_delivery_without_stopping_runtime(cx: &mut TestAppContext) {
        let first_window = cx.add_window(|_window, _cx| RemoteDesktopTestRoot);
        let second_window = cx.add_window(|_window, _cx| RemoteDesktopTestRoot);
        let protocol = RemoteDesktopProtocol::Rdp;
        let profile = preview_remote_desktop_profile(protocol);
        let provider = builtin_preview_provider_registry()
            .unwrap()
            .get_for_protocol(protocol)
            .cloned()
            .unwrap();
        let worker_wake = RemoteDesktopWorkerWake::default();
        let observed_wake = worker_wake.clone();
        let session = cx.new(|_cx| {
            let mut session = RemoteDesktopSessionEntity::new(
                TabId(10),
                profile,
                provider,
                None,
                std::env::temp_dir().join("oxideterm-handoff-test-certificates.json"),
                RemoteDesktopFrameDeliverySlot::new(),
                first_window.into(),
            );
            session.worker_wake = Some(worker_wake);
            session
        });

        session.update(cx, |session, _cx| {
            session.bind_window(second_window.into());
        });

        assert!(observed_wake.take());
        assert!(!observed_wake.is_stopped());
    }

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
    fn clipboard_images_map_between_gpui_and_remote_desktop_formats() {
        let item = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3]));

        let data = remote_desktop_clipboard_data_from_item(&item).unwrap();

        assert_eq!(data.format, RemoteDesktopClipboardFormat::ImagePng);
        assert_eq!(data.bytes, vec![1, 2, 3]);

        let data =
            RemoteDesktopClipboardData::new(RemoteDesktopClipboardFormat::ImageJpeg, vec![4, 5, 6]);

        let item = remote_desktop_clipboard_item_from_data(data).unwrap();

        assert!(matches!(
            item.entries(),
            [ClipboardEntry::Image(image)]
                if image.format == ImageFormat::Jpeg && image.bytes == vec![4, 5, 6]
        ));
    }

    #[test]
    fn wheel_delta_handles_pixel_accumulation_direction_changes_and_lines() {
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

        remainder = RemoteDesktopWheelDelta { x: 80.0, y: 40.0 };

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
    fn modifier_sync_emits_only_changed_modifier_states() {
        let pressed = RemoteDesktopModifierState {
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
        };
        assert_eq!(
            remote_desktop_modifier_sync_requests(RemoteDesktopModifierState::default(), pressed,),
            vec![
                modifier_request("ShiftLeft", RemoteDesktopKeyState::Pressed),
                modifier_request("ControlLeft", RemoteDesktopKeyState::Pressed),
            ]
        );

        let previous = RemoteDesktopModifierState {
            shift: false,
            ctrl: true,
            alt: false,
            meta: true,
        };

        assert_eq!(
            remote_desktop_modifier_sync_requests(previous, RemoteDesktopModifierState::default()),
            vec![
                modifier_request("ControlLeft", RemoteDesktopKeyState::Released),
                modifier_request("MetaLeft", RemoteDesktopKeyState::Released),
            ]
        );
    }

    #[test]
    fn lock_key_state_tracks_capslock_and_preserves_estimates() {
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
            viewport,
            None,
        ));
    }

    #[test]
    fn resize_request_does_not_duplicate_initial_scaled_connect() {
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };
        let request_size = RemoteDesktopSize {
            width: 3200,
            height: 1800,
        };

        assert!(!remote_desktop_resize_request_needed(
            Some(request_size),
            None,
            Some(viewport),
            None,
            viewport,
            request_size,
            Some(200),
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
            viewport,
            Some(125),
        ));
        assert!(!remote_desktop_resize_request_needed(
            Some(viewport),
            None,
            Some(viewport),
            Some(resize_state(viewport, Some(125))),
            viewport,
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
            viewport,
            Some(125),
        ));
    }

    #[test]
    fn resize_request_is_blocked_without_negotiated_resize_support() {
        let current = RemoteDesktopSize {
            width: 1280,
            height: 720,
        };
        let viewport = RemoteDesktopSize {
            width: 1600,
            height: 900,
        };

        assert!(!remote_desktop_resize_request_needed_for_capability(
            false,
            Some(current),
            None,
            Some(current),
            None,
            viewport,
            viewport,
            Some(100),
        ));
        assert!(remote_desktop_resize_request_needed_for_capability(
            true,
            Some(current),
            None,
            Some(current),
            None,
            viewport,
            viewport,
            Some(100),
        ));
    }

    fn resize_state(
        size: RemoteDesktopSize,
        scale_factor: Option<u32>,
    ) -> RemoteDesktopResizeRequestState {
        RemoteDesktopResizeRequestState { size, scale_factor }
    }
}
