use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher},
    env,
    hash::{Hash, Hasher},
    ops::Range,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use async_channel::Sender;
use chrono::Timelike;
use futures::future::{Either, pending, select};
use gpui::{
    App, Bounds, ClipboardItem, Context, EventEmitter, FocusHandle, PathPromptOptions, Pixels,
    Point, SharedString, Subscription, Timer, Window, px,
};
use oxideterm_ssh::SshConnectionHandle;
use oxideterm_terminal::{
    GraphicsOptions, KittyFileTransmissionControl, LocalPtyConfig, SerialControlLine,
    SerialControlState, SerialDisplayMode, SerialLineEnding, SerialRuntimeOptions, SerialSendMode,
    SerialSessionConfig, ShellIntegrationLifecycleState, ShellIntegrationStatus, SshSessionConfig,
    TelnetSessionConfig, TermMode, TerminalCommandMark, TerminalCommandMarkClosedBy,
    TerminalCommandMarkConfidence, TerminalCommandMarkDetectionSource, TerminalCommandMarkEvent,
    TerminalCwdIntegrationLaunchState, TerminalDrainBudget, TerminalDrainReport,
    TerminalEditorApplication, TerminalEditorClipboardOperation, TerminalEditorIntegrationEvent,
    TerminalEvent, TerminalLifecycle, TerminalOutputProcessor, TerminalProcessInfo,
    TerminalProcessProbe, TerminalRow, TerminalSearchMatch, TerminalSession, TerminalSessionKind,
    TerminalSnapshot, TmuxSeparator, TmuxSeparatorDirection, TrzszTransferDirection,
    TrzszTransferSelection, serial_list_ports,
};
use oxideterm_trzsz::TrzszState;
use parking_lot::Mutex;
use zeroize::Zeroizing;

use crate::background_cache::BackgroundImageRenderCache;
use crate::command_facts::{
    CommandFactLedger, SharedTerminalCommandHistory, TerminalAiCommandRecord,
    TerminalAutosuggestCandidate, TerminalAutosuggestCommandRecord, TerminalAutosuggestInputState,
    TerminalCommandFact,
};
use crate::privilege_prompt::{
    PrivilegeInputObservation, PrivilegePromptMatch, PrivilegePromptSnapshot,
    PrivilegePromptTracker,
};
use crate::session_log::TerminalSessionLog;
use crate::terminal_ui::*;
use crate::terminal_view::*;
use oxideterm_terminal_recording::{
    TerminalRecorder, TerminalRecordingOptions, TerminalRecordingState, TerminalRecordingStatus,
    TerminalRecordingTheme,
};

mod image_cache;
mod ime;
mod interactions;
mod render;
mod scrollbar;

use crate::modem_worker::{
    ModemPromptSelection, ModemWorkerEvent, ModemWorkerJob, ModemWorkerProgress,
    format_modem_bytes, run_modem_worker_job,
};
use crate::trzsz_worker::{
    TrzszPromptRequest, TrzszPromptSelection, TrzszWorkerEvent, TrzszWorkerJob,
    run_trzsz_worker_job,
};
use image_cache::ImageRenderCache;
pub(crate) use image_cache::TerminalRenderedImage;
pub(crate) use ime::TerminalInputHandler;
use scrollbar::{ScrollbarDrag, ScrollbarGeometry};

#[derive(Clone, Debug)]
enum TmuxPromptKind {
    RenameSession(u64),
    RenameWindow(u64),
    Command,
}

#[derive(Clone, Debug)]
struct TmuxPromptState {
    kind: TmuxPromptKind,
    value: String,
}

#[derive(Clone, Copy, Debug)]
struct TmuxSeparatorDrag {
    separator: TmuxSeparator,
    last_point: TerminalPoint,
}

pub type SharedTerminalSession = Arc<Mutex<TerminalSession>>;
pub type TerminalInputInterceptor =
    Arc<dyn Fn(&[u8]) -> TerminalInputInterceptorResult + Send + Sync>;
pub type TerminalInputBroadcaster = Rc<dyn Fn(TerminalBroadcastInputKind, &[u8], &mut App)>;
const PRIVILEGE_PROMPT_DEBUG_ENV: &str = "OXIDETERM_PRIVILEGE_DEBUG";
const TERMINAL_ANIMATION_INTERVAL: Duration = Duration::from_millis(16);
const SMOOTH_SCROLL_ANIMATION_DURATION: Duration = Duration::from_millis(125);
const DRAIN_BOOST_POLL_INTERVAL: Duration = Duration::from_millis(8);
const SSH_DRAIN_BOOST_POLL_INTERVAL: Duration = Duration::from_millis(1);
const MAX_UI_TERMINAL_DRAIN_DURATION: Duration = Duration::from_millis(2);
const RECENT_TERMINAL_ACTIVITY_WINDOW: Duration = Duration::from_millis(600);
const TERMINAL_PERFORMANCE_SAMPLE_CAPACITY: usize = 256;
const RECENT_TERMINAL_INPUT_WINDOW: Duration = Duration::from_millis(220);
const ACTIVE_PROCESS_INFO_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const EDITOR_INTEGRATION_HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(2500);
const EDITOR_CLIPBOARD_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const TERMINAL_SEARCH_DEBOUNCE: Duration = Duration::from_millis(24);
const BACKGROUND_IMAGE_COMPLETION_POLL_INTERVAL: Duration = Duration::from_millis(32);
const TERMINAL_AUTOSUGGEST_MAX_CANDIDATES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalPaneEvent {
    Exited { exit_code: Option<i32> },
    // Output contents stay pane-owned; consumers only learn that the visible buffer changed.
    OutputActivity,
    // CWD payloads stay pane-owned; Workspace only recomputes the active metadata key.
    CurrentDirectoryChanged,
    // Recording contents stay pane-owned; consumers only reschedule visible elapsed chrome.
    RecordingStatusChanged,
    // Session log contents and paths stay pane-owned; consumers only refresh log controls.
    SessionLogStatusChanged,
    // Prompt text and credentials stay pane-owned; consumers only recompute the active hint.
    PrivilegePromptStateChanged,
    // The event carries intent only; Workspace resolves any credential in the active scope.
    PrivilegePromptSubmitRequested,
    // The requested action remains pane-owned until the active Workspace consumes it.
    ContextActionRequested,
    // Match payloads remain pane-owned; Workspace drains them using the source pane identity.
    TriggerMatchesAvailable,
    // Search completion is asynchronous; Workspace reads the latest pane-owned status.
    SearchStatusChanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalBroadcastInputKind {
    Protocol,
    Text,
    Paste,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalWorkingDirectorySource {
    ShellIntegration,
    SessionDefault,
    VisibleCommand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalCwdShellIntegrationStatus {
    NotAttempted,
    Installing,
    Active,
    Failed,
    Disabled,
}

#[derive(Clone, Copy)]
struct ActiveTerminalEditorIntegration {
    state: TerminalEditorIntegrationEvent,
    last_seen: Instant,
}

#[derive(Clone, Copy)]
struct PendingTerminalEditorClipboard {
    application: TerminalEditorApplication,
    operation: TerminalEditorClipboardOperation,
    requested_at: Instant,
}

fn editor_integration_is_usable(
    free_type_mode: bool,
    terminal_mode: TermMode,
    integration: TerminalEditorIntegrationEvent,
    heartbeat_age: Duration,
    foreground_command: Option<&str>,
) -> bool {
    free_type_mode
        && terminal_mode.contains(TermMode::ALT_SCREEN)
        && integration.active
        && heartbeat_age <= EDITOR_INTEGRATION_HEARTBEAT_TIMEOUT
        && foreground_command
            .is_none_or(|command| integration.application.matches_process_command(command))
}

fn initial_cwd_shell_integration_status(
    enabled: bool,
    session_kind: TerminalSessionKind,
    launch_state: TerminalCwdIntegrationLaunchState,
) -> TerminalCwdShellIntegrationStatus {
    if !enabled {
        return TerminalCwdShellIntegrationStatus::Disabled;
    }
    if session_kind != TerminalSessionKind::LocalPty {
        return TerminalCwdShellIntegrationStatus::NotAttempted;
    }
    match launch_state {
        TerminalCwdIntegrationLaunchState::Prepared => {
            TerminalCwdShellIntegrationStatus::Installing
        }
        TerminalCwdIntegrationLaunchState::Unavailable => TerminalCwdShellIntegrationStatus::Failed,
        TerminalCwdIntegrationLaunchState::NotRequested => {
            TerminalCwdShellIntegrationStatus::NotAttempted
        }
    }
}

fn log_privilege_prompt_terminal_pane(args: std::fmt::Arguments<'_>) {
    if env::var_os(PRIVILEGE_PROMPT_DEBUG_ENV).is_some() {
        eprintln!("[oxideterm:privilege] {args}");
    }
}

fn privilege_input_observation_name(observation: PrivilegeInputObservation) -> &'static str {
    match observation {
        PrivilegeInputObservation::Normal => "normal",
        PrivilegeInputObservation::SecretEntry => "secret-entry",
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalCursorAnchor {
    pub x: f32,
    pub y: f32,
    pub line_height: f32,
    pub char_width: f32,
    pub container_width: f32,
    pub container_height: f32,
}

pub enum TerminalInputInterceptorResult {
    Continue(Vec<u8>),
    Suppress,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerminalSearchStatus {
    pub query: Option<String>,
    pub active_match: Option<usize>,
    pub match_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalSerialStatus {
    pub config: SerialSessionConfig,
    pub lifecycle: TerminalLifecycle,
    pub control_state: SerialControlState,
    pub runtime_options: SerialRuntimeOptions,
    pub port_available: Option<bool>,
    pub can_reconnect: bool,
}

/// A serial operation requested by another application-owned surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSerialAction {
    RefreshPortPresence,
    SendBreak,
    SetDataTerminalReady(bool),
    SetRequestToSend(bool),
    SetLocalEcho(bool),
    SetLineEnding(SerialLineEnding),
    SetDisplayMode(SerialDisplayMode),
    SetSendMode(SerialSendMode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Actions that must execute through the entity owning the live Telnet session.
pub enum TerminalTelnetAction {
    SendControl(oxideterm_terminal::TelnetControlCommand),
}

#[derive(Clone, Copy, Debug, Default)]
struct TerminalEventEffect {
    needs_notify: bool,
}

impl TerminalEventEffect {
    fn notify() -> Self {
        Self { needs_notify: true }
    }

    fn combine(&mut self, effect: Self) {
        self.needs_notify |= effect.needs_notify;
    }
}

fn terminal_maintenance_interval(
    startup_pending: bool,
    drain_budget_exhausted: bool,
    drain_boost_interval: Duration,
    smooth_scroll_active: bool,
    cursor_blink_remaining: Option<Duration>,
    process_refresh_remaining: Option<Duration>,
    pending_cwd_remaining: Option<Duration>,
    editor_expiry_remaining: Option<Duration>,
) -> Option<Duration> {
    if drain_budget_exhausted {
        return Some(drain_boost_interval);
    }
    [
        startup_pending.then_some(TERMINAL_ANIMATION_INTERVAL),
        smooth_scroll_active.then_some(TERMINAL_ANIMATION_INTERVAL),
        cursor_blink_remaining,
        process_refresh_remaining,
        pending_cwd_remaining,
        editor_expiry_remaining,
    ]
    .into_iter()
    .flatten()
    .min()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalSchedulerWake {
    BackendActivity,
    BackendClosed,
    PaneActivity,
    PaneClosed,
    Maintenance,
}

fn viewport_needs_live_output_restore(
    display_offset: usize,
    smooth_scroll_offset_px: Pixels,
    smooth_scroll_animation_active: bool,
) -> bool {
    display_offset > 0
        || f32::from(smooth_scroll_offset_px).abs() > f32::EPSILON
        || smooth_scroll_animation_active
}

fn populate_default_session_log_context(
    context: &mut crate::session_log::TerminalSessionLogContext,
    session_kind: TerminalSessionKind,
) {
    let protocol = match session_kind {
        TerminalSessionKind::LocalPty => "local",
        TerminalSessionKind::SshPty => "ssh",
        TerminalSessionKind::Telnet => "telnet",
        TerminalSessionKind::Mosh => "mosh",
        TerminalSessionKind::Serial => "serial",
    };
    if context.protocol.is_empty() {
        context.protocol = protocol.to_string();
    }
    if context.session.is_empty() {
        context.session = protocol.to_string();
    }
    if context.host.is_empty() {
        context.host = if session_kind == TerminalSessionKind::LocalPty {
            "localhost".to_string()
        } else {
            "unknown".to_string()
        };
    }
    if context.username.is_empty() {
        // Environment-derived identity is metadata only and never includes authentication input.
        context.username = if session_kind == TerminalSessionKind::LocalPty {
            env::var("USER")
                .or_else(|_| env::var("USERNAME"))
                .unwrap_or_else(|_| "unknown".to_string())
        } else {
            "unknown".to_string()
        };
    }
}

fn terminal_latency_percentiles(samples: &VecDeque<u64>) -> (u64, u64, u64) {
    if samples.is_empty() {
        return (0, 0, 0);
    }
    let mut sorted = samples.iter().copied().collect::<Vec<_>>();
    sorted.sort_unstable();
    let percentile = |numerator: usize| {
        let rank = (sorted.len() * numerator).div_ceil(100).max(1);
        sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
    };
    (percentile(50), percentile(95), percentile(99))
}

pub struct TerminalPane {
    terminal: Arc<Mutex<TerminalSession>>,
    // The backend kind is immutable for the pane's full lifetime.
    session_kind: TerminalSessionKind,
    serial_session_config: Option<SerialSessionConfig>,
    serial_port_available: Option<bool>,
    focus_handle: FocusHandle,
    preference_overrides: TerminalUiPreferenceOverrides,
    // The pane owns only its live-session highlight choice. Saved connection
    // defaults remain connection-store data and never affect node ownership.
    session_highlight_override: Option<TerminalHighlightRuleSetOverride>,
    // This optional override belongs only to the pane and never mutates the
    // application setting or the lifetime of its backing terminal session.
    session_semantic_coloring_override: Option<bool>,
    // Command-derived query highlighting is session-only and never becomes a
    // persisted keyword rule or shared backend state.
    command_context_highlighting_enabled: bool,
    preferences: TerminalUiPreferences,
    settings: TerminalUiSettings,
    theme: TerminalUiTheme,
    snapshot: TerminalSnapshot,
    snapshot_dirty: bool,
    snapshot_generation: u64,
    next_snapshot_line_id: u64,
    terminal_timestamps_enabled: bool,
    // Visual-only metadata keyed by stable snapshot line identity; never write this
    // into the PTY buffer, copied text, or search/indexed terminal content.
    row_timestamps: Arc<HashMap<u64, TerminalRowTimestamp>>,
    row_timestamp_retained_min_line: Option<u64>,
    metrics: TerminalMetrics,
    metrics_dirty: bool,
    selection: Option<TerminalSelection>,
    pending_paste: Option<String>,
    pending_paste_prefix: Option<Vec<u8>>,
    // The pane observes only its session's capability and never stores the sandbox path.
    kitty_file_transmission: Option<KittyFileTransmissionControl>,
    kitty_file_transmission_confirm_open: bool,
    // Control-mode prompts stay pane-owned so text never reaches the hosted shell.
    tmux_prompt: Option<TmuxPromptState>,
    dismissed_tmux_message_generation: u64,
    context_menu: Option<TerminalContextMenu>,
    context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence,
    context_action_requested: Option<TerminalContextAction>,
    pending_trigger_matches: VecDeque<oxideterm_terminal_triggers::TriggerMatched>,
    plugin_input_interceptor: Option<TerminalInputInterceptor>,
    input_broadcaster: Option<TerminalInputBroadcaster>,
    #[cfg(test)]
    test_accepts_input: bool,
    input_locked: bool,
    marked_text: Option<String>,
    privilege_prompt_inline_hint: Option<String>,
    privilege_prompt_submit_requested: bool,
    search_query: Option<String>,
    terminal_content_revision: u64,
    search_cache: Option<TerminalSearchCache>,
    search_generation: Arc<AtomicU64>,
    search_task: Option<gpui::Task<()>>,
    selected_search_match: Option<usize>,
    hovered_link: Option<TerminalLinkRange>,
    hovered_command_mark_id: Option<String>,
    selecting: bool,
    free_type_drag: Option<FreeTypeDragState>,
    last_mouse_report_point: Option<TerminalPoint>,
    title: SharedString,
    cwd: Option<String>,
    cwd_source: Option<TerminalWorkingDirectorySource>,
    pending_cwd: Option<PendingTerminalCwd>,
    cwd_host: Option<String>,
    cwd_shell_integration_status: TerminalCwdShellIntegrationStatus,
    shell_integration_status: ShellIntegrationStatus,
    editor_integration: Option<ActiveTerminalEditorIntegration>,
    pending_editor_clipboard: Option<PendingTerminalEditorClipboard>,
    command_marks: Vec<TerminalCommandMark>,
    command_marks_render_cache: Arc<[TerminalCommandMark]>,
    command_marks_render_cache_dirty: bool,
    selected_command_mark_id: Option<String>,
    command_mark_id_aliases: HashMap<String, String>,
    input_tracker: TerminalInputTracker,
    command_history: SharedTerminalCommandHistory,
    // Shell integration opens this boundary at a prompt and command submission closes it.
    autosuggest_prompt_active: bool,
    autosuggest_selected_index: Option<usize>,
    autosuggest_dismissed_query: Option<String>,
    privilege_prompt_tracker: PrivilegePromptTracker,
    privilege_prompt_expiry_generation: u64,
    privilege_prompt_expiry_task: Option<gpui::Task<()>>,
    command_fact_ledger: CommandFactLedger,
    recorder: Option<TerminalRecorder>,
    session_log: Option<TerminalSessionLog>,
    last_session_log_path: Option<std::path::PathBuf>,
    bell_flash: bool,
    terminal_exited: bool,
    scroll_input_remainder_px: Pixels,
    smooth_scroll_offset_px: Pixels,
    smooth_scroll_animation: Option<SmoothScrollAnimation>,
    smooth_scroll_snapshot_cache: Option<SmoothScrollSnapshotCache>,
    scrollbar_drag: Option<ScrollbarDrag>,
    tmux_separator_drag: Option<TmuxSeparatorDrag>,
    selection_autoscroll_position: Option<Point<Pixels>>,
    selection_autoscroll_scheduled: bool,
    copy_on_select_generation: u64,
    focused: bool,
    cursor_visible: bool,
    cursor_blink_terminal_enabled: bool,
    last_cursor_blink: Instant,
    last_terminal_input: Instant,
    last_terminal_activity: Instant,
    last_drain_budget_exhausted: bool,
    scheduler_wake_sender: Sender<()>,
    process_info_refresh_in_flight: bool,
    last_process_info_refresh_requested: Instant,
    render_stats: TerminalRenderStats,
    #[cfg(feature = "bench")]
    benchmark_performance_metrics_enabled: bool,
    #[cfg(feature = "bench")]
    benchmark_backend_snapshot_micros: u64,
    #[cfg(feature = "bench")]
    benchmark_snapshot_state_micros: u64,
    render_stats_window_start: Instant,
    render_stats_window_writes: usize,
    drain_duration_samples_micros: VecDeque<u64>,
    input_latency_samples_micros: VecDeque<u64>,
    last_latency_sampled_input: Option<Instant>,
    image_cache: ImageRenderCache,
    layout_cache: Arc<Mutex<TerminalLayoutCache>>,
    background_image_cache: BackgroundImageRenderCache,
    background_image_poll_active: bool,
    bounds: Option<Bounds<Pixels>>,
    viewport_scale_factor_bits: Option<u32>,
    last_pty_resize: Option<(usize, usize, u16, u16)>,
    pending_pty_resize: Option<(usize, usize, u16, u16)>,
    pty_resize_generation: u64,
    trzsz_state: Arc<TrzszState>,
    trzsz_owner_id: String,
    trzsz_prompt_active: bool,
    trzsz_connection_lost: bool,
    modem_prompt_active: bool,
    modem_connection_lost: bool,
    modem_progress: Option<ModemProgressState>,
    modem_transfer: Option<oxideterm_modem_transfer::ModemTransfer>,
    modem_worker: Option<std::thread::JoinHandle<()>>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Debug)]
pub(crate) struct TerminalContextMenu {
    pub x: f32,
    pub y: f32,
    pub modem_submenu_open: bool,
    pub target: TerminalPoint,
    pub has_selection: bool,
    pub reference_line: usize,
    pub command_mark_id: Option<String>,
    pub has_previous_command: bool,
    pub has_next_command: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FreeTypeDragState {
    pub start_position: Point<Pixels>,
    pub text: String,
    pub source_selection: Option<TerminalSelection>,
    pub action: FreeTypeDragAction,
    pub active: bool,
}

/// Describes the remote editing intent chosen for a Free Type drag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FreeTypeDragAction {
    MoveSelection,
    CopySelection,
    ReplaceCommand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalContextAction {
    SendSelectionToAi,
    FillCommandBarFromSelection,
    OpenSearch,
    OpenSessionTriggers,
}

#[derive(Clone, Debug)]
pub(crate) struct ModemProgressState {
    pub file_name: Option<String>,
    pub transferred_text: String,
    pub total_text: Option<String>,
    pub percent: Option<f32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalCommandNavigationDirection {
    Previous,
    Next,
}

#[derive(Clone, Debug)]
struct PendingTerminalCwd {
    path: String,
    command: String,
    created_at: Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct TerminalRowTimestamp {
    pub(crate) label: String,
    signature: u64,
    source_signature: u64,
}

#[derive(Clone)]
struct TerminalSearchCache {
    query: String,
    content_revision: u64,
    matches: Arc<[oxideterm_terminal::TerminalSearchMatch]>,
}

struct SmoothScrollSnapshotCache {
    source_generation: u64,
    display_offset: usize,
    rows: usize,
    snapshot: TerminalSnapshot,
}

#[derive(Clone, Copy)]
struct SmoothScrollAnimation {
    started_at: Instant,
    start_offset_px: Pixels,
}

impl SmoothScrollAnimation {
    fn offset_at(self, now: Instant) -> (Pixels, bool) {
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed >= SMOOTH_SCROLL_ANIMATION_DURATION {
            return (px(0.0), false);
        }
        let progress = elapsed.as_secs_f32() / SMOOTH_SCROLL_ANIMATION_DURATION.as_secs_f32();
        let remaining = 1.0 - progress.clamp(0.0, 1.0);
        // Ease out toward the target so delayed frames catch up without extending the animation.
        let offset = f32::from(self.start_offset_px) * remaining * remaining * remaining;
        (px(offset), true)
    }
}

impl TerminalSearchCache {
    fn is_current(&self, query: &str, content_revision: u64) -> bool {
        self.query == query && self.content_revision == content_revision
    }
}

const PTY_RESIZE_DEBOUNCE: Duration = Duration::from_millis(100);
const PENDING_CWD_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_COMMAND_MARKS_PER_PANE: usize = 2000;
const COMMAND_MARK_DEDUP_WINDOW_MS: u64 = 2000;
const COMMAND_MARK_DEDUP_LINE_DISTANCE: usize = 2;
static NEXT_TRZSZ_OWNER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_COMMAND_MARK_ID: AtomicU64 = AtomicU64::new(1);

fn command_mark_ui_available(enabled: bool, mode: TermMode) -> bool {
    // Command marks describe normal-screen scrollback. A full-screen application or terminal
    // mouse protocol owns the active grid, so stale shell ranges must not remain interactive.
    enabled && !mode.contains(TermMode::ALT_SCREEN) && !mode.intersects(TermMode::MOUSE_MODE)
}

fn privilege_prompt_input_tracking_available(mode: TermMode) -> bool {
    // Full-screen applications own input; their navigation is not shell history.
    !mode.contains(TermMode::ALT_SCREEN)
}

fn take_snapshot_line_id(next_line_id: &mut u64) -> u64 {
    let line_id = (*next_line_id).max(1);
    *next_line_id = line_id.wrapping_add(1).max(1);
    line_id
}

fn assign_initial_snapshot_line_ids(snapshot: &mut TerminalSnapshot, next_line_id: &mut u64) {
    for row in &mut snapshot.lines {
        row.line_id = take_snapshot_line_id(next_line_id);
    }
}

fn reconcile_snapshot_line_ids(
    snapshot: &mut TerminalSnapshot,
    previous: &TerminalSnapshot,
    next_line_id: &mut u64,
) {
    if snapshot.lines.iter().all(|row| row.line_id != 0) {
        return;
    }

    let previous_by_source = previous
        .lines
        .iter()
        .filter(|row| row.source_id != 0 && row.line_id != 0)
        .map(|row| (row.source_id, row))
        .collect::<HashMap<_, _>>();
    let previous_by_grid_line = previous
        .lines
        .iter()
        .filter(|row| row.line_id != 0)
        .map(|row| (row.absolute_line, row))
        .collect::<HashMap<_, _>>();
    let mut used_line_ids = snapshot
        .lines
        .iter()
        .filter_map(|row| (row.line_id != 0).then_some(row.line_id))
        .collect::<HashSet<_>>();

    for row in &mut snapshot.lines {
        if row.line_id != 0 {
            continue;
        }

        // A backing row moving toward a lower grid line is retained output. Rows recycled into
        // the bottom move in the opposite direction and therefore receive a new identity.
        let moved_line_id = previous_by_source
            .get(&row.source_id)
            .filter(|previous_row| previous_row.absolute_line > row.absolute_line)
            .map(|previous_row| previous_row.line_id);
        let stationary_line_id = previous_by_grid_line
            .get(&row.absolute_line)
            .filter(|previous_row| previous_row.source_id == row.source_id)
            .map(|previous_row| previous_row.line_id);
        let inherited_line_id = moved_line_id
            .or(stationary_line_id)
            .filter(|line_id| !used_line_ids.contains(line_id));
        row.line_id = inherited_line_id.unwrap_or_else(|| take_snapshot_line_id(next_line_id));
        used_line_ids.insert(row.line_id);
    }
}

include!("app_recording.rs");
include!("app_session_log.rs");
include!("app_command_marks.rs");
include!("app_modem.rs");
include!("app_trzsz.rs");

impl TerminalPane {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        Self::new_with_preferences(TerminalUiPreferences::default(), window, cx)
    }

    pub fn new_with_preferences(
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let config = LocalPtyConfig {
            current_directory_shell_integration: preferences.current_directory_awareness_enabled,
            ..LocalPtyConfig::default()
        };
        let terminal = Arc::new(Mutex::new(
            TerminalSession::local_with_config_graphics_and_encoding(
                DEFAULT_COLS,
                DEFAULT_ROWS,
                config,
                graphics_options_from_preferences(&preferences),
                preferences.terminal_encoding,
                preferences.scrollback_lines,
            )?,
        ));
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_local_with_config_and_preferences(
        mut config: LocalPtyConfig,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        config.current_directory_shell_integration =
            preferences.current_directory_awareness_enabled;
        let terminal = Arc::new(Mutex::new(
            TerminalSession::local_with_config_graphics_and_encoding(
                DEFAULT_COLS,
                DEFAULT_ROWS,
                config,
                graphics_options_from_preferences(&preferences),
                preferences.terminal_encoding,
                preferences.scrollback_lines,
            )?,
        ));
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_ssh(
        config: SshSessionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        Self::new_ssh_with_preferences(config, TerminalUiPreferences::default(), window, cx)
    }

    pub fn new_ssh_with_preferences(
        config: SshSessionConfig,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let terminal = Self::ssh_shared_session(config, &preferences);
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn ssh_shared_session(
        config: SshSessionConfig,
        preferences: &TerminalUiPreferences,
    ) -> SharedTerminalSession {
        Arc::new(Mutex::new(TerminalSession::ssh_with_graphics_and_encoding(
            config,
            DEFAULT_COLS,
            DEFAULT_ROWS,
            graphics_options_from_preferences(preferences),
            preferences.terminal_encoding,
            preferences.scrollback_lines,
        )))
    }

    pub fn new_telnet_with_preferences(
        config: TelnetSessionConfig,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        Self::new_telnet_with_login_preferences(config, None, preferences, window, cx)
    }

    pub fn new_telnet_with_login_preferences(
        config: TelnetSessionConfig,
        login: Option<oxideterm_terminal::TelnetLoginCredentials>,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let terminal = Arc::new(Mutex::new(TerminalSession::telnet_with_login_and_encoding(
            config,
            login,
            DEFAULT_COLS,
            DEFAULT_ROWS,
            graphics_options_from_preferences(&preferences),
            preferences.terminal_encoding,
            preferences.scrollback_lines,
        )));
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_mosh_with_preferences(
        config: oxideterm_terminal::MoshTerminalConfig,
        mut preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        // Mosh state is always UTF-8 regardless of the application default.
        preferences.terminal_encoding = oxideterm_terminal::TerminalEncoding::Utf8;
        let terminal = Arc::new(Mutex::new(TerminalSession::mosh_with_graphics(
            config,
            DEFAULT_COLS,
            DEFAULT_ROWS,
            graphics_options_from_preferences(&preferences),
            preferences.scrollback_lines,
        )));
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_serial_with_preferences(
        config: SerialSessionConfig,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let session_config = config.clone();
        let terminal = Self::open_serial_session_with_preferences(config, &preferences)?;
        let mut pane = Self::from_session(terminal, preferences, window, cx)?;
        pane.serial_session_config = Some(session_config);
        Ok(pane)
    }

    pub fn open_serial_session_with_preferences(
        config: SerialSessionConfig,
        preferences: &TerminalUiPreferences,
    ) -> Result<SharedTerminalSession> {
        // Opening the device is fallible and must happen before GPUI allocates
        // the pane entity so callers can surface missing or busy ports.
        Ok(Arc::new(Mutex::new(
            TerminalSession::serial_with_graphics_and_encoding(
                config,
                DEFAULT_COLS,
                DEFAULT_ROWS,
                graphics_options_from_preferences(preferences),
                preferences.terminal_encoding,
                preferences.scrollback_lines,
            )?,
        )))
    }

    pub fn from_shared_session(
        terminal: SharedTerminalSession,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_recording_playback(
        cols: usize,
        rows: usize,
        mut preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        // Playback is not a live terminal connection and must never create an automatic audit log.
        preferences.session_log_automatic = false;
        preferences.session_log_options = None;
        let terminal = Arc::new(Mutex::new(TerminalSession::recording_playback(
            cols,
            rows,
            graphics_options_from_preferences(&preferences),
            preferences.scrollback_lines,
        )));
        Self::from_session(terminal, preferences, window, cx)
    }

    fn from_session(
        terminal: SharedTerminalSession,
        mut preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let (mut snapshot, session_kind, cwd_integration_launch_state) = {
            let terminal = terminal.lock();
            (
                terminal.snapshot().with_generation(1),
                terminal.kind(),
                terminal.cwd_integration_launch_state(),
            )
        };
        let kitty_file_transmission = terminal.lock().kitty_file_transmission_control();
        let mut next_snapshot_line_id = 1;
        assign_initial_snapshot_line_ids(&mut snapshot, &mut next_snapshot_line_id);
        let cwd_shell_integration_status = initial_cwd_shell_integration_status(
            preferences.current_directory_awareness_enabled,
            session_kind,
            cwd_integration_launch_state,
        );
        if let Some(options) = preferences.session_log_options.as_mut() {
            populate_default_session_log_context(&mut options.context, session_kind);
        }
        let focus_handle = cx.focus_handle();
        let metrics = TerminalMetrics::measure_with_preferences(window, &preferences);
        window.focus(&focus_handle, cx);
        terminal.lock().set_focused(true)?;
        let trzsz_owner_id = format!(
            "gpui-terminal-{}",
            NEXT_TRZSZ_OWNER_ID.fetch_add(1, Ordering::Relaxed)
        );

        let focus_in = cx.on_focus_in(&focus_handle, window, |this, _window, cx| {
            this.handle_focus_change(true, cx);
        });
        let focus_out = cx.on_focus_out(&focus_handle, window, |this, _event, _window, cx| {
            this.handle_focus_change(false, cx);
        });

        // GPUI unit tests use a deterministic single-thread scheduler, so real PTY threads must
        // not wake its local executor directly. Production builds keep the activity-driven path.
        let backend_activity_enabled = !cfg!(test);
        let terminal_activity =
            backend_activity_enabled.then(|| terminal.lock().activity_receiver());
        let (scheduler_wake_sender, scheduler_wake_receiver) = async_channel::bounded(1);
        // Backend output drives steady-state work. The startup deadline keeps GPUI layout and
        // deferred PTY sizing moving before a remote session can emit its first activity edge.
        cx.spawn(async move |weak, cx| {
            let mut activity_receiver = terminal_activity;
            let mut maintenance_interval = Some(TERMINAL_ANIMATION_INTERVAL);
            let mut backend_activity_closed = false;
            loop {
                let activity = Box::pin(async {
                    match activity_receiver.as_ref() {
                        Some(receiver) => receiver.notified().await,
                        None => pending::<bool>().await,
                    }
                });
                let pane_activity = Box::pin(scheduler_wake_receiver.recv());
                let terminal_wake = Box::pin(async {
                    match select(activity, pane_activity).await {
                        Either::Left((true, _)) => TerminalSchedulerWake::BackendActivity,
                        Either::Left((false, _)) => TerminalSchedulerWake::BackendClosed,
                        Either::Right((Ok(()), _)) => TerminalSchedulerWake::PaneActivity,
                        Either::Right((Err(_), _)) => TerminalSchedulerWake::PaneClosed,
                    }
                });
                let maintenance = Box::pin(async {
                    match maintenance_interval {
                        Some(interval) => cx.background_executor().timer(interval).await,
                        None => pending::<()>().await,
                    }
                });
                let wake = match select(terminal_wake, maintenance).await {
                    Either::Left((wake, _)) => wake,
                    Either::Right(((), _)) => TerminalSchedulerWake::Maintenance,
                };
                if wake == TerminalSchedulerWake::PaneClosed {
                    break;
                }
                backend_activity_closed = match wake {
                    TerminalSchedulerWake::BackendClosed => true,
                    TerminalSchedulerWake::PaneActivity => false,
                    _ => backend_activity_closed,
                };

                let Ok((next_maintenance_interval, next_activity_receiver)) =
                    weak.update(cx, |this, cx| {
                        this.tick(cx);
                        let listen_for_backend = backend_activity_enabled
                            && !backend_activity_closed
                            && !this.terminal_exited
                            && !this.last_drain_budget_exhausted;
                        (
                            this.next_maintenance_interval(),
                            listen_for_backend.then(|| this.terminal.lock().activity_receiver()),
                        )
                    })
                else {
                    break;
                };
                maintenance_interval = next_maintenance_interval;
                activity_receiver = next_activity_receiver;
            }
        })
        .detach();

        let session_log = if preferences.session_log_automatic {
            preferences.session_log_options.clone().and_then(|options| {
                match TerminalSessionLog::start(options) {
                    Ok(log) => Some(log),
                    Err(_) => {
                        if let Some(sink) = &preferences.notice_sink
                            && !preferences.session_log_labels.start_failed.is_empty()
                        {
                            sink(TerminalNotice {
                                title: preferences.session_log_labels.start_failed.clone(),
                                description: None,
                                status_text: None,
                                progress: None,
                                variant: TerminalNoticeVariant::Error,
                            });
                        }
                        None
                    }
                }
            })
        } else {
            None
        };

        let mut pane = Self {
            terminal,
            session_kind,
            serial_session_config: None,
            serial_port_available: None,
            focus_handle,
            preference_overrides: TerminalUiPreferenceOverrides::default(),
            session_highlight_override: None,
            session_semantic_coloring_override: None,
            command_context_highlighting_enabled: true,
            preferences: preferences.clone(),
            settings: TerminalUiSettings::from_preferences(&preferences),
            theme: preferences.theme.clone(),
            snapshot,
            snapshot_dirty: false,
            snapshot_generation: 1,
            next_snapshot_line_id,
            terminal_timestamps_enabled: false,
            row_timestamps: Arc::new(HashMap::new()),
            row_timestamp_retained_min_line: None,
            metrics,
            metrics_dirty: false,
            selection: None,
            pending_paste: None,
            pending_paste_prefix: None,
            kitty_file_transmission,
            kitty_file_transmission_confirm_open: false,
            tmux_prompt: None,
            dismissed_tmux_message_generation: 0,
            context_menu: None,
            context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            context_action_requested: None,
            pending_trigger_matches: VecDeque::new(),
            plugin_input_interceptor: None,
            input_broadcaster: None,
            #[cfg(test)]
            test_accepts_input: false,
            input_locked: false,
            marked_text: None,
            privilege_prompt_inline_hint: None,
            privilege_prompt_submit_requested: false,
            search_query: None,
            terminal_content_revision: 1,
            search_cache: None,
            search_generation: Arc::new(AtomicU64::new(0)),
            search_task: None,
            selected_search_match: None,
            hovered_link: None,
            hovered_command_mark_id: None,
            selecting: false,
            free_type_drag: None,
            last_mouse_report_point: None,
            title: SharedString::from("OxideTerm"),
            cwd: None,
            cwd_source: None,
            pending_cwd: None,
            cwd_host: None,
            cwd_shell_integration_status,
            shell_integration_status: ShellIntegrationStatus {
                detected: false,
                state: ShellIntegrationLifecycleState::Idle,
                integration_source: None,
                last_seen_at: None,
            },
            editor_integration: None,
            pending_editor_clipboard: None,
            command_marks: Vec::new(),
            command_marks_render_cache: Arc::from([]),
            command_marks_render_cache_dirty: false,
            selected_command_mark_id: None,
            command_mark_id_aliases: HashMap::new(),
            input_tracker: TerminalInputTracker::default(),
            command_history: preferences.command_history.clone(),
            autosuggest_prompt_active: false,
            autosuggest_selected_index: None,
            autosuggest_dismissed_query: None,
            privilege_prompt_tracker: PrivilegePromptTracker::default(),
            privilege_prompt_expiry_generation: 0,
            privilege_prompt_expiry_task: None,
            command_fact_ledger: CommandFactLedger::default(),
            recorder: None,
            session_log,
            last_session_log_path: None,
            bell_flash: false,
            terminal_exited: false,
            scroll_input_remainder_px: px(0.0),
            smooth_scroll_offset_px: px(0.0),
            smooth_scroll_animation: None,
            smooth_scroll_snapshot_cache: None,
            scrollbar_drag: None,
            tmux_separator_drag: None,
            selection_autoscroll_position: None,
            selection_autoscroll_scheduled: false,
            copy_on_select_generation: 0,
            focused: true,
            cursor_visible: true,
            cursor_blink_terminal_enabled: false,
            last_cursor_blink: Instant::now(),
            last_terminal_input: Instant::now(),
            last_terminal_activity: Instant::now(),
            last_drain_budget_exhausted: false,
            scheduler_wake_sender,
            process_info_refresh_in_flight: false,
            last_process_info_refresh_requested: Instant::now()
                .checked_sub(ACTIVE_PROCESS_INFO_REFRESH_INTERVAL)
                .unwrap_or_else(Instant::now),
            render_stats: TerminalRenderStats::default(),
            #[cfg(feature = "bench")]
            benchmark_performance_metrics_enabled: false,
            #[cfg(feature = "bench")]
            benchmark_backend_snapshot_micros: 0,
            #[cfg(feature = "bench")]
            benchmark_snapshot_state_micros: 0,
            render_stats_window_start: Instant::now(),
            render_stats_window_writes: 0,
            drain_duration_samples_micros: VecDeque::with_capacity(
                TERMINAL_PERFORMANCE_SAMPLE_CAPACITY,
            ),
            input_latency_samples_micros: VecDeque::with_capacity(
                TERMINAL_PERFORMANCE_SAMPLE_CAPACITY,
            ),
            last_latency_sampled_input: None,
            image_cache: {
                let mut cache = ImageRenderCache::default();
                cache.set_byte_limit(preferences.render_policy.image_cache_bytes);
                cache
            },
            layout_cache: Arc::new(Mutex::new(TerminalLayoutCache::default())),
            background_image_cache: {
                let mut cache = BackgroundImageRenderCache::default();
                cache.set_byte_limit(preferences.render_policy.image_cache_bytes);
                cache
            },
            background_image_poll_active: false,
            bounds: None,
            viewport_scale_factor_bits: None,
            last_pty_resize: None,
            pending_pty_resize: None,
            pty_resize_generation: 0,
            trzsz_state: TrzszState::new(),
            trzsz_owner_id,
            trzsz_prompt_active: false,
            trzsz_connection_lost: false,
            modem_prompt_active: false,
            modem_connection_lost: false,
            modem_progress: None,
            modem_transfer: None,
            modem_worker: None,
            _subscriptions: vec![focus_in, focus_out],
        };
        pane.sync_terminal_output_events_enabled();
        Ok(pane)
    }

    pub fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn stamp_snapshot(&mut self, mut snapshot: TerminalSnapshot) -> TerminalSnapshot {
        let backend_reused_rows = snapshot.lines.iter().any(|row| row.line_id != 0);
        reconcile_snapshot_line_ids(
            &mut snapshot,
            &self.snapshot,
            &mut self.next_snapshot_line_id,
        );
        // Raw backend snapshots are stateless; the pane owns frame generation
        // so future render caches can invalidate without changing backends.
        if !backend_reused_rows {
            // Incremental backends already carry shared cell buffers and line identities. Full
            // snapshots still receive the equality fallback used by reset and resize paths.
            snapshot.reuse_unchanged_rows_from(&self.snapshot);
        }
        self.record_snapshot_row_timestamps(&snapshot);
        self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
        if self.snapshot_generation == 0 {
            self.snapshot_generation = 1;
        }
        snapshot.with_generation(self.snapshot_generation)
    }

    fn record_snapshot_row_timestamps(&mut self, snapshot: &TerminalSnapshot) {
        // Match iTerm-style semantics: a row label is the time that row was
        // last modified, not the time it first became visible in the viewport.
        let label = current_terminal_timestamp_label();
        record_timestampable_snapshot_rows(
            Arc::make_mut(&mut self.row_timestamps),
            snapshot,
            &label,
        );
        self.trim_row_timestamps(snapshot);
    }

    fn trim_row_timestamps(&mut self, snapshot: &TerminalSnapshot) {
        let Some(max_line) = snapshot.lines.iter().map(|row| row.line_id).max() else {
            Arc::make_mut(&mut self.row_timestamps).clear();
            self.row_timestamp_retained_min_line = None;
            return;
        };
        let retained_rows = self
            .preferences
            .scrollback_lines
            .saturating_add(snapshot.rows)
            .saturating_add(1024)
            .max(2048) as u64;
        let min_line = max_line.saturating_sub(retained_rows);
        trim_row_timestamp_history(
            Arc::make_mut(&mut self.row_timestamps),
            &mut self.row_timestamp_retained_min_line,
            min_line,
        );
    }

    pub fn terminal_timestamps_enabled(&self) -> bool {
        self.terminal_timestamps_enabled
    }

    pub fn toggle_terminal_timestamps(&mut self, cx: &mut Context<Self>) {
        self.terminal_timestamps_enabled = !self.terminal_timestamps_enabled;
        // Timestamp visibility is paint-only. Do not restamp or resize here:
        // both would make old scrollback look like it was modified at toggle time.
        cx.notify();
    }

    pub fn shared_session(&self) -> SharedTerminalSession {
        self.terminal.clone()
    }

    pub fn process_info(&self) -> TerminalProcessInfo {
        self.terminal.lock().process_info()
    }

    fn active_editor_integration(&self, mode: TermMode) -> Option<TerminalEditorIntegrationEvent> {
        let integration = self.editor_integration?;
        let process_info = self.process_info();
        editor_integration_is_usable(
            self.settings.free_type_mode,
            mode,
            integration.state,
            integration.last_seen.elapsed(),
            process_info.command.as_deref(),
        )
        .then_some(integration.state)
    }

    pub fn process_info_probe(&self) -> Option<TerminalProcessProbe> {
        self.terminal.lock().process_info_probe()
    }

    pub fn apply_process_info(&mut self, info: TerminalProcessInfo) -> bool {
        self.terminal.lock().apply_process_info(info)
    }

    pub fn buffer_line_count(&self) -> usize {
        self.terminal.lock().buffer_line_count()
    }

    pub fn shell_integration_status(&self) -> ShellIntegrationStatus {
        self.shell_integration_status.clone()
    }

    pub fn current_working_directory(&self) -> Option<String> {
        self.pending_cwd
            .as_ref()
            .map(|pending| pending.path.clone())
            .or_else(|| self.cwd.clone())
    }

    pub fn current_working_directory_source(&self) -> Option<TerminalWorkingDirectorySource> {
        self.pending_cwd
            .as_ref()
            .map(|_| TerminalWorkingDirectorySource::VisibleCommand)
            .or(self.cwd_source)
    }

    pub fn current_working_directory_is_pending(&self) -> bool {
        self.pending_cwd.is_some()
    }

    pub fn set_current_working_directory_from_terminal_action(
        &mut self,
        cwd: String,
        cx: &mut Context<Self>,
    ) {
        let cwd = cwd.trim();
        if cwd.is_empty() || cwd.chars().any(char::is_control) {
            return;
        }
        // Workspace-owned directory actions only call this after selecting a
        // path that was already resolved by the active pane's directory scope.
        self.cwd = Some(cwd.to_string());
        self.cwd_source = Some(TerminalWorkingDirectorySource::VisibleCommand);
        self.pending_cwd = None;
        cx.emit(TerminalPaneEvent::CurrentDirectoryChanged);
        cx.notify();
    }

    pub fn set_current_working_directory_from_session_default(
        &mut self,
        cwd: &str,
        cx: &mut Context<Self>,
    ) {
        let cwd = cwd.trim();
        if cwd.is_empty() || cwd.chars().any(char::is_control) || self.cwd.is_some() {
            return;
        }
        // SSH does not expose the login shell cwd through the PTY protocol.
        // Seed the standard login default without writing probe bytes into the
        // shell; OSC 7 or a visible user `cd` will replace it when available.
        self.cwd = Some(cwd.to_string());
        self.cwd_source = Some(TerminalWorkingDirectorySource::SessionDefault);
        cx.emit(TerminalPaneEvent::CurrentDirectoryChanged);
        cx.notify();
    }

    pub fn set_pending_current_working_directory_from_terminal_action(
        &mut self,
        cwd: String,
        command: String,
        cx: &mut Context<Self>,
    ) {
        let cwd = cwd.trim();
        let command = command.trim();
        if cwd.is_empty()
            || command.is_empty()
            || cwd.chars().any(char::is_control)
            || command.chars().any(char::is_control)
        {
            return;
        }
        // The UI may follow a user-selected, listed directory immediately,
        // but the shell command mark remains the authority for success/failure.
        self.pending_cwd = Some(PendingTerminalCwd {
            path: cwd.to_string(),
            command: command.to_string(),
            created_at: Instant::now(),
        });
        self.wake_terminal_scheduler();
        cx.emit(TerminalPaneEvent::CurrentDirectoryChanged);
        cx.notify();
    }

    pub fn current_working_directory_host(&self) -> Option<String> {
        self.cwd_host.clone()
    }

    pub fn cwd_shell_integration_status(&self) -> TerminalCwdShellIntegrationStatus {
        if !self.settings.current_directory_awareness_enabled {
            return TerminalCwdShellIntegrationStatus::Disabled;
        }
        self.cwd_shell_integration_status
    }

    pub fn can_switch_working_directory_from_chrome(&self) -> bool {
        let mode = self.terminal.lock().mode();
        !mode.contains(TermMode::ALT_SCREEN) && !mode.intersects(TermMode::MOUSE_MODE)
    }

    pub fn command_marks(&self) -> Vec<TerminalCommandMark> {
        self.command_marks.clone()
    }

    pub fn command_facts(&self) -> Vec<TerminalCommandFact> {
        self.command_fact_ledger.facts()
    }

    pub fn ai_command_records(&self) -> Vec<TerminalAiCommandRecord> {
        self.command_fact_ledger.ai_records()
    }

    pub fn autosuggest_command_records(&self) -> Vec<TerminalAutosuggestCommandRecord> {
        self.command_fact_ledger.autosuggest_records()
    }

    pub fn autosuggest_input_state(&self) -> TerminalAutosuggestInputState {
        self.input_tracker.state()
    }

    pub fn autosuggest_ghost_text(&self) -> Option<String> {
        self.command_fact_ledger
            .autosuggest_ghost_text(&self.input_tracker.state())
    }

    pub fn history_ghost_text_for_input(&self, input: &str) -> Option<String> {
        self.command_history
            .ghost_text(&TerminalAutosuggestInputState {
                value: input.to_string(),
                cursor_index: input.len(),
                is_cursor_at_end: true,
            })
    }

    pub fn history_command_records(&self) -> Vec<TerminalAutosuggestCommandRecord> {
        self.command_history.records()
    }

    fn terminal_autosuggest_candidates(&self) -> Vec<TerminalAutosuggestCandidate> {
        let cursor_row_is_active_input = self
            .snapshot
            .lines
            .get(self.snapshot.cursor_row)
            .is_some_and(|row| row.active_input);
        if !self.autosuggest_prompt_active
            || self.marked_text.is_some()
            || self.tmux_prompt.is_some()
            || self.pending_paste.is_some()
            || self.context_menu.is_some()
            || self.privilege_prompt_inline_hint.is_some()
            || !cursor_row_is_active_input
            || self.snapshot.display_offset != 0
        {
            return Vec::new();
        }
        // Most terminal frames have no active suggestion prompt. Defer both the terminal lock and
        // input-state clone until the pane-local eligibility checks have passed.
        let (mode, terminal_interactive) = {
            let terminal = self.terminal.lock();
            (terminal.mode(), terminal.is_interactive())
        };
        if mode.contains(TermMode::ALT_SCREEN)
            || !self.terminal_accepts_input_with_interactive_state(terminal_interactive)
        {
            return Vec::new();
        }
        let state = self.input_tracker.state();
        if self.autosuggest_dismissed_query.as_deref() == Some(state.value.as_str()) {
            return Vec::new();
        }
        self.command_history
            .candidates(&state, TERMINAL_AUTOSUGGEST_MAX_CANDIDATES)
    }

    fn dismiss_terminal_autosuggest(&mut self, cx: &mut Context<Self>) {
        self.autosuggest_dismissed_query = Some(self.input_tracker.state().value);
        self.autosuggest_selected_index = None;
        cx.notify();
    }

    fn fill_terminal_autosuggest_command(
        &mut self,
        command: &str,
        append_enter: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let state = self.input_tracker.state();
        let Some(suffix) = command.strip_prefix(&state.value) else {
            return false;
        };
        let mut bytes =
            Zeroizing::new(Vec::with_capacity(suffix.len() + usize::from(append_enter)));
        bytes.extend_from_slice(suffix.as_bytes());
        if append_enter {
            bytes.push(b'\r');
        }
        self.autosuggest_selected_index = None;
        self.autosuggest_dismissed_query = Some(command.to_string());
        self.send_user_protocol_bytes(&bytes, cx);
        true
    }

    fn terminal_ghost_text(&self) -> Option<String> {
        // Keep ordinary terminal suggestions in the pane-owned list instead of painting ghost text.
        self.privilege_prompt_inline_hint.clone()
    }

    pub fn privilege_prompt_snapshot(&self) -> Option<PrivilegePromptSnapshot> {
        self.privilege_prompt_tracker.snapshot(Instant::now())
    }

    pub fn privilege_prompt_fallback_suppressed(&self) -> bool {
        self.privilege_prompt_tracker
            .suppresses_fallback_prompt_detection(Instant::now())
    }

    pub fn has_privilege_prompt_inline_hint(&self) -> bool {
        self.privilege_prompt_inline_hint.is_some()
    }

    pub(crate) fn sync_terminal_output_events_enabled(&mut self) {
        let recording_requires_output = self
            .recorder
            .as_ref()
            .is_some_and(|recorder| recorder.status().state == TerminalRecordingState::Recording);
        let session_log_requires_output = self.session_log.as_ref().is_some_and(|log| {
            log.status().state == crate::session_log::TerminalSessionLogState::Logging
        });
        // Privilege prompts use compact semantic events at the session output
        // boundary. Full decoded output is duplicated only for active file consumers.
        self.terminal
            .lock()
            .set_output_events_enabled(recording_requires_output || session_log_requires_output);
    }

    fn finish_privilege_prompt_tracker_update(
        &mut self,
        previous_state_generation: u64,
        cx: &mut Context<Self>,
    ) {
        if self.privilege_prompt_tracker.state_generation() == previous_state_generation {
            return;
        }
        self.schedule_privilege_prompt_expiry(cx);
        cx.emit(TerminalPaneEvent::PrivilegePromptStateChanged);
    }

    fn schedule_privilege_prompt_expiry(&mut self, cx: &mut Context<Self>) {
        self.privilege_prompt_expiry_generation =
            self.privilege_prompt_expiry_generation.wrapping_add(1);
        self.privilege_prompt_expiry_task = None;
        let Some(deadline) = self.privilege_prompt_tracker.next_expiry_deadline() else {
            return;
        };
        let generation = self.privilege_prompt_expiry_generation;
        let delay = deadline.saturating_duration_since(Instant::now());
        self.privilege_prompt_expiry_task = Some(cx.spawn(async move |pane, cx| {
            Timer::after(delay).await;
            let _ = pane.update(cx, |pane, cx| {
                if pane.privilege_prompt_expiry_generation != generation {
                    return;
                }
                if !pane.privilege_prompt_tracker.expire_at(Instant::now()) {
                    pane.schedule_privilege_prompt_expiry(cx);
                    return;
                }
                // Expiry carries no prompt payload. Workspace reads only the
                // active pane and clears any now-stale inline hint.
                cx.emit(TerminalPaneEvent::PrivilegePromptStateChanged);
                cx.notify();
            });
        }));
    }

    pub fn take_privilege_prompt_submit_request(&mut self) -> bool {
        let requested = self.privilege_prompt_submit_requested;
        self.privilege_prompt_submit_requested = false;
        requested
    }

    pub fn take_context_action_request(&mut self) -> Option<TerminalContextAction> {
        self.context_action_requested.take()
    }

    pub fn set_privilege_prompt_inline_hint(
        &mut self,
        hint: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.privilege_prompt_inline_hint == hint {
            return false;
        }
        self.privilege_prompt_inline_hint = hint;
        cx.notify();
        true
    }

    fn clear_privilege_prompt_inline_hint(&mut self) -> bool {
        self.privilege_prompt_inline_hint.take().is_some()
    }

    pub fn set_preferences(&mut self, preferences: TerminalUiPreferences, cx: &mut Context<Self>) {
        let mut preferences = preferences;
        self.preference_overrides.apply_to(&mut preferences);
        if preferences.session_log_options.is_none() && self.session_log.is_some() {
            // An explicit connection-level disable takes effect immediately for an active pane.
            let _ = self.stop_session_log(cx);
        }
        if let Some(highlight_override) = &self.session_highlight_override {
            preferences.highlight_rules = highlight_override.rules.clone();
        }
        if self.session_semantic_coloring_override == Some(preferences.semantic_coloring) {
            self.session_semantic_coloring_override = None;
        }
        if self.preferences.terminal_encoding != preferences.terminal_encoding {
            self.terminal
                .lock()
                .set_encoding(preferences.terminal_encoding);
        }
        if self.preferences.trzsz_policy != preferences.trzsz_policy {
            self.terminal
                .lock()
                .set_trzsz_policy(preferences.trzsz_policy.clone());
        }
        let metrics_changed = self.preferences.font_family != preferences.font_family
            || self.preferences.cjk_font_family != preferences.cjk_font_family
            || self.preferences.font_ligatures != preferences.font_ligatures
            || self.preferences.font_size.to_bits() != preferences.font_size.to_bits()
            || self.preferences.font_weight.to_bits() != preferences.font_weight.to_bits()
            || self.preferences.line_height.to_bits() != preferences.line_height.to_bits();
        let next_settings = TerminalUiSettings::from_preferences(&preferences);
        if !next_settings.command_marks_enabled {
            self.command_marks.clear();
            self.command_marks_render_cache_dirty = true;
            self.selected_command_mark_id = None;
            self.hovered_command_mark_id = None;
            self.command_mark_id_aliases.clear();
        }
        if !next_settings.current_directory_awareness_enabled {
            self.pending_cwd = None;
            self.cwd_shell_integration_status = TerminalCwdShellIntegrationStatus::Disabled;
        } else if !self.settings.current_directory_awareness_enabled {
            self.cwd_shell_integration_status = TerminalCwdShellIntegrationStatus::NotAttempted;
        }
        if !next_settings.smooth_scroll {
            self.clear_smooth_scroll_remainder();
        }
        self.settings = next_settings;
        self.theme = preferences.theme.clone();
        self.command_history = preferences.command_history.clone();
        self.image_cache
            .set_byte_limit(preferences.render_policy.image_cache_bytes);
        self.background_image_cache
            .set_byte_limit(preferences.render_policy.image_cache_bytes);
        self.preferences = preferences;
        // Font resolution is stable across output frames and changes only with typography
        // preferences, so defer the next measurement until the pane is rendered again.
        self.metrics_dirty |= metrics_changed;
        self.last_pty_resize = None;
        self.pending_pty_resize = None;
        self.viewport_scale_factor_bits = None;
        self.reset_cursor_blink();
        self.wake_terminal_scheduler();
        cx.notify();
    }

    pub fn with_preference_overrides(
        mut self,
        preference_overrides: TerminalUiPreferenceOverrides,
    ) -> Self {
        // Callers apply these overrides before constructing the backend. The
        // pane retains them only to preserve host behavior on later refreshes.
        self.preference_overrides = preference_overrides;
        self
    }

    pub fn with_serial_session_config(mut self, config: SerialSessionConfig) -> Self {
        // A pane built from a pre-opened session still owns the configuration
        // required by serial status and device-presence controls.
        self.serial_session_config = Some(config);
        self
    }

    pub fn preference_overrides_snapshot(&self) -> TerminalUiPreferenceOverrides {
        self.preference_overrides.clone()
    }

    pub fn session_highlight_rule_set_id(&self) -> Option<&str> {
        self.session_highlight_override
            .as_ref()
            .map(|highlight_override| highlight_override.id.as_str())
    }

    pub fn semantic_coloring_enabled(&self) -> bool {
        self.session_semantic_coloring_override
            .unwrap_or(self.preferences.semantic_coloring)
    }

    pub fn session_semantic_coloring_overridden(&self) -> bool {
        self.session_semantic_coloring_override.is_some()
    }

    pub fn set_session_semantic_coloring_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let semantic_override = (enabled != self.preferences.semantic_coloring).then_some(enabled);
        if self.session_semantic_coloring_override == semantic_override {
            return;
        }
        self.session_semantic_coloring_override = semantic_override;
        cx.notify();
    }

    pub fn command_context_highlighting_enabled(&self) -> bool {
        self.command_context_highlighting_enabled
    }

    pub fn set_command_context_highlighting_enabled(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.command_context_highlighting_enabled == enabled {
            return;
        }
        self.command_context_highlighting_enabled = enabled;
        cx.notify();
    }

    pub fn set_session_highlight_override(
        &mut self,
        highlight_override: Option<TerminalHighlightRuleSetOverride>,
        application_preferences: TerminalUiPreferences,
        cx: &mut Context<Self>,
    ) {
        // Session-only state is retained by this pane and is discarded with
        // the pane without mutating the shared SSH node or saved connection.
        self.session_highlight_override = highlight_override;
        self.set_preferences(application_preferences, cx);
    }

    pub fn set_preference_overrides(
        &mut self,
        preference_overrides: TerminalUiPreferenceOverrides,
        application_preferences: TerminalUiPreferences,
        cx: &mut Context<Self>,
    ) {
        // A saved host edit changes only this session-owned protocol behavior;
        // all unrelated visual preferences continue to come from the app.
        self.preference_overrides = preference_overrides;
        self.set_preferences(application_preferences, cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    pub fn shutdown(&mut self) {
        self.terminal.lock().shutdown();
    }

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.terminal.lock().lifecycle()
    }

    pub fn session_kind(&self) -> TerminalSessionKind {
        self.session_kind
    }

    pub fn is_tmux_control_mode(&self) -> bool {
        // Control mode changes the command target semantics while retaining the
        // transport session kind, so callers need an explicit runtime signal.
        self.terminal.lock().tmux_state().is_some()
    }

    pub fn is_serial_transport(&self) -> bool {
        self.serial_session_config.is_some()
    }

    pub fn serial_status(&self) -> Option<TerminalSerialStatus> {
        let config = self.serial_session_config.clone()?;
        let terminal = self.terminal.lock();
        Some(TerminalSerialStatus {
            config,
            lifecycle: terminal.lifecycle(),
            control_state: terminal.serial_control_state().unwrap_or_default(),
            runtime_options: terminal.serial_runtime_options().unwrap_or_default(),
            port_available: self.serial_port_available,
            // Reconnect is workspace-owned because it must allocate a fresh tab and pane.
            can_reconnect: false,
        })
    }

    pub fn apply_serial_action(
        &mut self,
        action: TerminalSerialAction,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.is_serial_transport() {
            return Err("The selected terminal is not a serial session.".to_string());
        }
        match action {
            TerminalSerialAction::RefreshPortPresence => {
                self.refresh_serial_port_presence(cx);
            }
            TerminalSerialAction::SendBreak => {
                self.terminal
                    .lock()
                    .send_serial_break()
                    .map_err(|error| error.to_string())?;
                cx.notify();
            }
            TerminalSerialAction::SetDataTerminalReady(asserted) => {
                self.terminal
                    .lock()
                    .set_serial_control_line(SerialControlLine::DataTerminalReady, asserted)
                    .map_err(|error| error.to_string())?;
                cx.notify();
            }
            TerminalSerialAction::SetRequestToSend(asserted) => {
                self.terminal
                    .lock()
                    .set_serial_control_line(SerialControlLine::RequestToSend, asserted)
                    .map_err(|error| error.to_string())?;
                cx.notify();
            }
            TerminalSerialAction::SetLocalEcho(enabled) => {
                let mut options = self
                    .terminal
                    .lock()
                    .serial_runtime_options()
                    .ok_or_else(|| "Serial runtime options are unavailable.".to_string())?;
                options.local_echo = enabled;
                self.terminal
                    .lock()
                    .set_serial_runtime_options(options)
                    .map_err(|error| error.to_string())?;
                cx.notify();
            }
            TerminalSerialAction::SetLineEnding(line_ending) => {
                let mut options = self
                    .terminal
                    .lock()
                    .serial_runtime_options()
                    .ok_or_else(|| "Serial runtime options are unavailable.".to_string())?;
                options.line_ending = line_ending;
                self.terminal
                    .lock()
                    .set_serial_runtime_options(options)
                    .map_err(|error| error.to_string())?;
                cx.notify();
            }
            TerminalSerialAction::SetDisplayMode(display_mode) => {
                let mut options = self
                    .terminal
                    .lock()
                    .serial_runtime_options()
                    .ok_or_else(|| "Serial runtime options are unavailable.".to_string())?;
                options.display_mode = display_mode;
                self.terminal
                    .lock()
                    .set_serial_runtime_options(options)
                    .map_err(|error| error.to_string())?;
                cx.notify();
            }
            TerminalSerialAction::SetSendMode(send_mode) => {
                let mut options = self
                    .terminal
                    .lock()
                    .serial_runtime_options()
                    .ok_or_else(|| "Serial runtime options are unavailable.".to_string())?;
                options.send_mode = send_mode;
                self.terminal
                    .lock()
                    .set_serial_runtime_options(options)
                    .map_err(|error| error.to_string())?;
                cx.notify();
            }
        }
        Ok(())
    }

    pub fn apply_telnet_action(
        &mut self,
        action: TerminalTelnetAction,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        // Reuse the pane-owned backend so AI control cannot create a parallel socket.
        if self.session_kind() != TerminalSessionKind::Telnet {
            return Err("The selected terminal is not a Telnet session.".to_string());
        }
        match action {
            TerminalTelnetAction::SendControl(command) => {
                self.terminal
                    .lock()
                    .send_telnet_control(command)
                    .map_err(|error| error.to_string())?;
            }
        }
        cx.notify();
        Ok(())
    }

    fn refresh_serial_port_presence(&mut self, cx: &mut Context<Self>) {
        let Some(config) = self.serial_session_config.as_ref() else {
            return;
        };
        let expected = config.port_path.trim().to_ascii_lowercase();
        self.serial_port_available = serial_list_ports().ok().map(|ports| {
            ports
                .iter()
                .any(|port| port.port_path.trim().to_ascii_lowercase() == expected)
        });
        cx.notify();
    }

    fn set_serial_control_line(
        &mut self,
        line: SerialControlLine,
        asserted: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .terminal
            .lock()
            .set_serial_control_line(line, asserted)
            .is_ok()
        {
            cx.notify();
        }
    }

    fn send_serial_break(&mut self, cx: &mut Context<Self>) {
        if self.terminal.lock().send_serial_break().is_ok() {
            cx.notify();
        }
    }

    fn set_serial_runtime_options(
        &mut self,
        options: SerialRuntimeOptions,
        cx: &mut Context<Self>,
    ) {
        if self
            .terminal
            .lock()
            .set_serial_runtime_options(options)
            .is_ok()
        {
            cx.notify();
        }
    }

    fn cycle_serial_send_mode(&mut self, cx: &mut Context<Self>) {
        let Some(mut options) = self.terminal.lock().serial_runtime_options() else {
            return;
        };
        options.send_mode = match options.send_mode {
            SerialSendMode::Text => SerialSendMode::Hex,
            SerialSendMode::Hex => SerialSendMode::Text,
        };
        self.set_serial_runtime_options(options, cx);
    }

    fn cycle_serial_display_mode(&mut self, cx: &mut Context<Self>) {
        let Some(mut options) = self.terminal.lock().serial_runtime_options() else {
            return;
        };
        options.display_mode = match options.display_mode {
            SerialDisplayMode::Text => SerialDisplayMode::Hex,
            SerialDisplayMode::Hex => SerialDisplayMode::Mixed,
            SerialDisplayMode::Mixed => SerialDisplayMode::Text,
        };
        self.set_serial_runtime_options(options, cx);
    }

    fn cycle_serial_line_ending(&mut self, cx: &mut Context<Self>) {
        let Some(mut options) = self.terminal.lock().serial_runtime_options() else {
            return;
        };
        options.line_ending = match options.line_ending {
            SerialLineEnding::None => SerialLineEnding::Lf,
            SerialLineEnding::Lf => SerialLineEnding::CrLf,
            SerialLineEnding::CrLf => SerialLineEnding::Cr,
            SerialLineEnding::Cr => SerialLineEnding::None,
        };
        self.set_serial_runtime_options(options, cx);
    }

    fn toggle_serial_local_echo(&mut self, cx: &mut Context<Self>) {
        let Some(mut options) = self.terminal.lock().serial_runtime_options() else {
            return;
        };
        options.local_echo = !options.local_echo;
        self.set_serial_runtime_options(options, cx);
    }

    pub fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.terminal.lock().ssh_connection_handle()
    }

    pub fn set_search_query(
        &mut self,
        query: Option<String>,
        selected_match: Option<usize>,
        cx: &mut Context<Self>,
    ) -> TerminalSearchStatus {
        self.search_query = query;
        self.search_cache = None;
        self.schedule_search_refresh(cx);
        let match_count = self.search_match_count();
        self.selected_search_match = if match_count == 0 {
            None
        } else {
            selected_match
                .or(Some(0))
                .filter(|index| *index < match_count)
        };
        if self.selected_search_match.is_some() {
            self.scroll_to_selected_search_match(cx);
        }
        cx.notify();
        self.search_status()
    }

    pub fn select_next_search_result(
        &mut self,
        forward: bool,
        cx: &mut Context<Self>,
    ) -> TerminalSearchStatus {
        self.select_next_search_match(forward, cx);
        self.search_status()
    }

    pub fn search_status(&self) -> TerminalSearchStatus {
        let match_count = self.search_match_count();
        TerminalSearchStatus {
            query: self.search_query.clone(),
            active_match: self
                .selected_search_match
                .filter(|index| *index < match_count),
            match_count,
        }
    }

    fn search_match_count(&self) -> usize {
        self.search_cache
            .as_ref()
            .filter(|cache| {
                self.search_query
                    .as_deref()
                    .is_some_and(|query| cache.is_current(query, self.terminal_content_revision))
            })
            .map(|cache| cache.matches.len())
            .unwrap_or_default()
    }

    fn mark_terminal_content_changed(&mut self, cx: &mut Context<Self>) {
        self.terminal_content_revision = self.terminal_content_revision.wrapping_add(1).max(1);
        self.search_cache = None;
        self.schedule_search_refresh(cx);
    }

    fn schedule_search_refresh(&mut self, cx: &mut Context<Self>) {
        let generation = self
            .search_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        self.search_task = None;
        let Some(query) = self
            .search_query
            .as_deref()
            .filter(|query| !query.is_empty())
            .map(str::to_owned)
        else {
            self.search_cache = None;
            self.selected_search_match = None;
            return;
        };
        let Some(search_source) = self.terminal.lock().search_source() else {
            return;
        };
        let cancellation = self.search_generation.clone();
        let content_revision = self.terminal_content_revision;
        self.search_task = Some(cx.spawn(async move |weak, cx| {
            cx.background_executor()
                .timer(TERMINAL_SEARCH_DEBOUNCE)
                .await;
            if cancellation.load(Ordering::Acquire) != generation {
                return;
            }
            let worker_cancellation = cancellation.clone();
            let worker_query = query.clone();
            let search_task = cx.background_executor().spawn(async move {
                let started = Instant::now();
                let is_cancelled = || worker_cancellation.load(Ordering::Acquire) != generation;
                let matches = search_source.search_matches(&worker_query, &is_cancelled);
                (matches, started.elapsed())
            });
            let (matches, elapsed) = search_task.await;
            let _ = weak.update(cx, |this, cx| {
                if this.search_generation.load(Ordering::Acquire) != generation
                    || this.terminal_content_revision != content_revision
                    || this.search_query.as_deref() != Some(query.as_str())
                {
                    return;
                }
                let matches: Arc<[TerminalSearchMatch]> = matches.into();
                this.search_cache = Some(TerminalSearchCache {
                    query,
                    content_revision,
                    matches: matches.clone(),
                });
                this.render_stats.search_micros =
                    elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
                this.selected_search_match = if matches.is_empty() {
                    None
                } else {
                    this.selected_search_match
                        .or(Some(0))
                        .filter(|index| *index < matches.len())
                };
                if this.selected_search_match.is_some() {
                    this.scroll_to_selected_search_match(cx);
                }
                cx.emit(TerminalPaneEvent::SearchStatusChanged);
                cx.notify();
            });
        }));
    }

    fn current_search_matches(&self) -> Arc<[TerminalSearchMatch]> {
        self.search_cache
            .as_ref()
            .filter(|cache| {
                self.search_query
                    .as_deref()
                    .is_some_and(|query| cache.is_current(query, self.terminal_content_revision))
            })
            .map(|cache| cache.matches.clone())
            .unwrap_or_else(|| Arc::from([]))
    }

    pub fn copy_to_clipboard(&mut self, cx: &mut Context<Self>) {
        self.copy_from_platform_shortcut(cx);
    }

    pub fn has_selection(&self) -> bool {
        self.selection
            .is_some_and(|selection| !selection.is_empty())
    }

    pub fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.paste_text_without_broadcast(text, cx) {
            self.broadcast_user_input(TerminalBroadcastInputKind::Paste, text.as_bytes(), cx);
        }
    }

    fn paste_text_without_broadcast(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if !self.terminal_accepts_input() {
            return false;
        }
        let Some(bytes) = self.apply_plugin_input_interceptor(text.as_bytes()) else {
            return false;
        };
        let bytes = Zeroizing::new(bytes);
        let mode = self.terminal.lock().mode();
        self.delete_free_type_selection_if_active(mode, cx);
        let now = Instant::now();
        // Privilege input classification runs before command tracking so a password answer never
        // becomes a command merely because it arrived through the clipboard path.
        let privilege_observation = self.observe_privilege_input("paste", &bytes, now, cx);
        let trackable_single_line_paste = std::str::from_utf8(&bytes)
            .ok()
            .filter(|text| !text.contains(['\r', '\n']));
        // Preserve bracketed paste encoding when hook output is still text;
        // binary hook output falls back to raw protocol bytes.
        let result = match std::str::from_utf8(&bytes) {
            Ok(text) => self.terminal.lock().paste_text(text),
            Err(_) => self.terminal.lock().write_protocol_bytes(&bytes),
        };
        if result.is_ok() {
            self.restore_live_output_after_user_input();
            if privilege_observation == PrivilegeInputObservation::SecretEntry {
                self.input_tracker.reset();
            } else if let Some(text) = trackable_single_line_paste {
                self.observe_autosuggest_input_bytes(text.as_bytes(), cx);
            } else {
                // Multi-line and binary pastes do not have one reliable shell submission boundary.
                self.input_tracker.reset();
            }
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
            return true;
        }
        false
    }

    pub fn send_command_line(&mut self, command: &str, cx: &mut Context<Self>) {
        if command.trim().is_empty() {
            return;
        }
        let mut input = command.replace("\r\n", "\r").replace('\n', "\r");
        input.push('\r');
        self.observe_privilege_input("command-line", input.as_bytes(), Instant::now(), cx);
        self.observe_autosuggest_input_bytes(input.as_bytes(), cx);
        self.send_text(&input, cx);
    }

    pub fn send_command_sender_line(&mut self, line: &str, cx: &mut Context<Self>) -> bool {
        let mut input = zeroize::Zeroizing::new(line.replace("\r\n", "\r").replace('\n', "\r"));
        input.push('\r');
        self.send_command_sender_text(&input, cx)
    }

    pub fn send_command_sender_text_chunk(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if text.is_empty() {
            return false;
        }
        self.send_command_sender_text(text, cx)
    }

    pub fn send_trigger_text(
        &mut self,
        text: &str,
        append_enter: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = terminal_trigger_input(text, append_enter) else {
            return false;
        };
        if !self.terminal_accepts_input() {
            return false;
        }

        // Trigger captures bypass plugin interception, broadcast, recording, and command
        // history so remote output cannot escape through an unrelated observer.
        let write_result = {
            let mut terminal = self.terminal.lock();
            if self.input_locked || self.terminal_exited || !terminal.is_interactive() {
                return false;
            }
            terminal.write_text(&input)
        };
        if write_result.is_err() {
            return false;
        }

        self.last_terminal_input = Instant::now();
        self.reset_cursor_blink();
        self.restore_live_output_after_user_input();
        cx.notify();
        true
    }

    pub fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        self.pending_trigger_matches.clear();
        self.terminal.lock().set_trigger_rules(rules);
    }

    pub fn take_trigger_matches(&mut self) -> Vec<oxideterm_terminal_triggers::TriggerMatched> {
        self.pending_trigger_matches.drain(..).collect()
    }

    pub fn send_command_sender_raw_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) -> bool {
        if bytes.is_empty() || !self.terminal_accepts_input() {
            return false;
        }
        let Some(bytes) = self.apply_plugin_input_interceptor(bytes) else {
            return false;
        };
        let bytes = zeroize::Zeroizing::new(bytes);
        // Hex input is an opaque protocol payload. Recheck lifecycle after the
        // plugin hook, then bypass text recording and command observation.
        let write_result = {
            let mut terminal = self.terminal.lock();
            if self.input_locked || self.terminal_exited || !terminal.is_interactive() {
                return false;
            }
            terminal.write_protocol_bytes(&bytes)
        };
        if write_result.is_err() {
            return false;
        }
        self.last_terminal_input = Instant::now();
        self.reset_cursor_blink();
        self.restore_live_output_after_user_input();
        cx.notify();
        true
    }

    fn send_command_sender_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if text.is_empty() || !self.terminal_accepts_input() {
            return false;
        }
        let Some(bytes) = self.apply_plugin_input_interceptor(text.as_bytes()) else {
            return false;
        };
        let bytes = zeroize::Zeroizing::new(bytes);
        // The plugin can run arbitrary code, so the lifecycle check and write
        // must share the same terminal-session lock after interception.
        let write_result = {
            let mut terminal = self.terminal.lock();
            if self.input_locked || self.terminal_exited || !terminal.is_interactive() {
                return false;
            }
            match std::str::from_utf8(&bytes) {
                Ok(text) => terminal.write_text(text),
                Err(_) => terminal.write_protocol_bytes(&bytes),
            }
        };
        if write_result.is_err() {
            return false;
        }

        // Scheduled input does not prove that the remote prompt accepted or
        // began a command. Keep it out of marks, AI facts, autosuggest, history,
        // and asciicast input; only update the privilege prompt state safely.
        self.observe_privilege_input("command-sender-text", &bytes, Instant::now(), cx);
        self.last_terminal_input = Instant::now();
        self.reset_cursor_blink();
        self.restore_live_output_after_user_input();
        cx.notify();
        true
    }

    pub fn send_internal_control_command_line(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if command.trim().is_empty() || !self.terminal_accepts_input() {
            return false;
        }

        let mut input = command.replace("\r\n", "\r").replace('\n', "\r");
        input.push('\r');
        // Internal control commands are terminal-owned probes. They must not be
        // learned as user history, autosuggest input, privilege commands, or AI
        // context, even though the shell may still echo the bytes visibly.
        if self.terminal.lock().write_text(&input).is_ok() {
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
            return true;
        }
        false
    }

    pub fn send_ai_input_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if bytes.is_empty() || !self.terminal_accepts_input() {
            return;
        }
        // AI input remains scoped to its selected pane even while the user has
        // interactive terminal broadcasting enabled.
        self.send_user_protocol_bytes_without_broadcast(bytes, cx);
    }

    pub fn send_privilege_secret_input_bytes(
        &mut self,
        bytes: &[u8],
        confirmed_prompt: PrivilegePromptMatch,
        cx: &mut Context<Self>,
    ) -> bool {
        if bytes.is_empty() || !self.terminal_accepts_input() {
            return false;
        }

        // Privilege Prompt Helper writes an explicitly user-confirmed secret
        // directly to the PTY. It must not pass through plugin interception,
        // autosuggest/history observation, AI context, or terminal recording.
        if self.terminal.lock().write_protocol_bytes(bytes).is_ok() {
            let previous_state_generation = self.privilege_prompt_tracker.state_generation();
            self.privilege_prompt_tracker
                .mark_confirmed_secret_filled(confirmed_prompt, Instant::now());
            self.finish_privilege_prompt_tracker_update(previous_state_generation, cx);
            self.clear_privilege_prompt_inline_hint();
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
            return true;
        }
        false
    }

    pub fn ai_accepts_input(&self) -> bool {
        // AI terminal tools mirror Tauri's readiness gate before reporting a
        // successful send, instead of letting a closed/non-interactive pane
        // silently drop input.
        self.terminal_accepts_input()
    }

    pub fn set_plugin_input_interceptor(&mut self, interceptor: Option<TerminalInputInterceptor>) {
        self.plugin_input_interceptor = interceptor;
    }

    pub fn set_input_broadcaster(&mut self, broadcaster: Option<TerminalInputBroadcaster>) {
        self.input_broadcaster = broadcaster;
    }

    pub fn send_broadcast_input(
        &mut self,
        kind: TerminalBroadcastInputKind,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) -> bool {
        // Mirrored input uses the target pane's normal interception and PTY
        // path, but never re-enters the broadcaster and creates a loop.
        match kind {
            TerminalBroadcastInputKind::Protocol => {
                self.send_user_protocol_bytes_without_broadcast(bytes, cx)
            }
            TerminalBroadcastInputKind::Text => std::str::from_utf8(bytes)
                .is_ok_and(|text| self.commit_text_without_broadcast(text, cx)),
            TerminalBroadcastInputKind::Paste => std::str::from_utf8(bytes)
                .is_ok_and(|text| self.paste_text_without_broadcast(text, cx)),
        }
    }

    pub fn set_input_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        if self.input_locked == locked {
            return;
        }
        // Tauri TerminalView drops user input while a node is link-down or
        // reconnecting. Keep that readiness gate before plugin hooks so plugins
        // cannot accidentally send input into a standby SSH transport.
        self.input_locked = locked;
        cx.notify();
    }

    pub fn set_plugin_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.terminal.lock().set_output_processor(processor);
    }

    pub fn clear_buffer(&mut self, cx: &mut Context<Self>) {
        // Plugin clearBuffer mirrors Tauri's host-side buffer reset: it must not
        // send Ctrl-L or other bytes to the running shell. The emulator and the
        // command fact ledger are both owned by this pane, so keep the mutation
        // on the GPUI entity thread.
        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.clear_buffer();
            terminal.snapshot()
        };
        self.clear_smooth_scroll_remainder();
        self.snapshot = self.stamp_snapshot(snapshot);
        self.mark_terminal_content_changed(cx);
        self.selection = None;
        self.search_query = None;
        self.selected_search_match = None;
        self.reset_command_marks_for_terminal_reset();
        cx.notify();
    }

    pub fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(prompt) = self.tmux_prompt.as_mut() {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                prompt
                    .value
                    .extend(text.chars().filter(|character| !character.is_control()));
                cx.notify();
            }
            return;
        }
        self.paste_from_clipboard_after(&[], cx);
    }

    fn paste_from_clipboard_after(&mut self, prefix: &[u8], cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        if !self.terminal_accepts_input() {
            return;
        }
        if self.settings.paste_protection && paste_needs_confirmation(&text) {
            self.pending_paste = Some(text);
            self.pending_paste_prefix = (!prefix.is_empty()).then(|| prefix.to_vec());
            cx.notify();
            return;
        }
        if !prefix.is_empty() {
            self.send_user_protocol_bytes(prefix, cx);
        }
        self.paste_text(&text, cx);
    }

    pub(crate) fn confirm_pending_paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.pending_paste.take() else {
            return;
        };
        if let Some(prefix) = self.pending_paste_prefix.take() {
            self.send_user_protocol_bytes(&prefix, cx);
        }
        self.paste_text(&text, cx);
        cx.notify();
    }

    pub(crate) fn cancel_pending_paste(&mut self, cx: &mut Context<Self>) {
        self.pending_paste_prefix = None;
        if self.pending_paste.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn confirm_kitty_file_transmission(&mut self, cx: &mut Context<Self>) {
        self.kitty_file_transmission_confirm_open = false;
        let labels = &self.preferences.kitty_file_transmission_labels;
        let result = self
            .kitty_file_transmission
            .as_ref()
            .ok_or_else(|| std::io::Error::other("Kitty file transmission is unavailable"))
            .and_then(KittyFileTransmissionControl::authorize_for_session);
        match result {
            Ok(sandbox_path) => {
                // Clipboard export is the explicit user-authorized capability boundary;
                // the path is never retained by pane state, logs, or persistence.
                cx.write_to_clipboard(ClipboardItem::new_string(
                    sandbox_path.to_string_lossy().into_owned(),
                ));
                self.emit_kitty_file_transmission_notice(
                    labels.allowed_title.clone(),
                    labels.allowed_description.clone(),
                    TerminalNoticeVariant::Success,
                );
            }
            Err(_) => self.emit_kitty_file_transmission_notice(
                labels.failed_title.clone(),
                labels.failed_description.clone(),
                TerminalNoticeVariant::Error,
            ),
        }
        cx.notify();
    }

    pub(crate) fn deny_kitty_file_transmission(&mut self, cx: &mut Context<Self>) {
        self.kitty_file_transmission_confirm_open = false;
        if let Some(control) = &self.kitty_file_transmission {
            control.deny_for_session();
        }
        cx.notify();
    }

    fn emit_kitty_file_transmission_notice(
        &self,
        title: String,
        description: String,
        variant: TerminalNoticeVariant,
    ) {
        if let Some(sink) = &self.preferences.notice_sink {
            sink(TerminalNotice {
                title,
                description: Some(description),
                status_text: None,
                progress: None,
                variant,
            });
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let budget = self.next_drain_budget();
        let (report, events, mode) = {
            let mut terminal = self.terminal.lock();
            let report = terminal.read_pending_with_budget(budget);
            let events = terminal.take_events();
            let mode = terminal.mode();
            (report, events, mode)
        };
        self.last_drain_budget_exhausted = report.budget_exhausted;
        if report.changed {
            self.last_terminal_activity = now;
            // Parsing stays current for every terminal, but the expensive immutable snapshot is
            // built only when GPUI actually renders this pane.
            self.snapshot_dirty = true;
            self.mark_terminal_content_changed(cx);
            cx.emit(TerminalPaneEvent::OutputActivity);
        }
        let render_stats_changed = self.update_render_stats(&report, now);

        let mut event_effect = TerminalEventEffect::default();
        for event in events {
            event_effect.combine(self.handle_terminal_event(event, cx));
        }

        let cleared_command_mark_selection = self.clear_command_mark_selection_for_tui_mode(mode);
        let cleared_privilege_prompt_hint = if mode.contains(TermMode::ALT_SCREEN) {
            // Full-screen applications own the alternate screen and Enter.
            // Clear terminal-local ghost text even when no tracker event fires
            // during the mode transition.
            self.clear_privilege_prompt_inline_hint()
        } else {
            false
        };
        let mut needs_notify = event_effect.needs_notify || report.changed;
        if self
            .kitty_file_transmission
            .as_ref()
            .is_some_and(KittyFileTransmissionControl::take_authorization_request)
            && !self.kitty_file_transmission_confirm_open
        {
            self.kitty_file_transmission_confirm_open = true;
            needs_notify = true;
        }
        if (self.preferences.show_performance_overlay && render_stats_changed)
            || cleared_command_mark_selection
            || cleared_privilege_prompt_hint
        {
            needs_notify = true;
        }
        if self.advance_smooth_scroll_animation(now) {
            needs_notify = true;
        }
        if self.expire_pending_terminal_cwd(now) {
            cx.emit(TerminalPaneEvent::CurrentDirectoryChanged);
            needs_notify = true;
        }
        if needs_notify {
            cx.notify();
        }

        self.update_cursor_blink(cx);
        self.request_active_process_info_refresh(cx);
        if self.expire_editor_integration(mode, now) {
            cx.notify();
        }
    }

    fn next_maintenance_interval(&self) -> Option<Duration> {
        let now = Instant::now();
        let cursor_blink_remaining = self.should_blink_cursor().then(|| {
            CURSOR_BLINK_INTERVAL
                .saturating_sub(now.saturating_duration_since(self.last_cursor_blink))
        });
        let (mode, ssh_still_connecting, process_refresh_supported) = {
            let terminal = self.terminal.lock();
            (
                terminal.mode(),
                terminal.kind() == TerminalSessionKind::SshPty && !terminal.is_interactive(),
                terminal.process_info_probe().is_some(),
            )
        };
        let needs_process_refresh = (self.settings.free_type_mode
            && mode.contains(TermMode::ALT_SCREEN))
            || (self.settings.current_directory_awareness_enabled
                && self.cwd_shell_integration_status != TerminalCwdShellIntegrationStatus::Active);
        let process_refresh_remaining = (self.focused
            && needs_process_refresh
            && process_refresh_supported
            && !self.process_info_refresh_in_flight)
            .then(|| {
                ACTIVE_PROCESS_INFO_REFRESH_INTERVAL.saturating_sub(
                    now.saturating_duration_since(self.last_process_info_refresh_requested),
                )
            });
        let pending_cwd_remaining = self.pending_cwd.as_ref().map(|pending| {
            PENDING_CWD_TIMEOUT.saturating_sub(now.saturating_duration_since(pending.created_at))
        });
        let editor_expiry_remaining = self.editor_integration.map(|integration| {
            EDITOR_INTEGRATION_HEARTBEAT_TIMEOUT
                .saturating_sub(now.saturating_duration_since(integration.last_seen))
        });
        terminal_maintenance_interval(
            !self.terminal_exited && (self.last_pty_resize.is_none() || ssh_still_connecting),
            self.last_drain_budget_exhausted,
            if self.session_kind == TerminalSessionKind::SshPty {
                SSH_DRAIN_BOOST_POLL_INTERVAL
            } else {
                DRAIN_BOOST_POLL_INTERVAL
            },
            self.smooth_scroll_animation.is_some(),
            cursor_blink_remaining,
            process_refresh_remaining,
            pending_cwd_remaining,
            editor_expiry_remaining,
        )
    }

    pub(super) fn wake_terminal_scheduler(&self) {
        // One queued edge is enough because every wake recomputes all maintenance deadlines.
        let _ = self.scheduler_wake_sender.try_send(());
    }

    fn request_active_process_info_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.focused {
            return;
        }
        let mode = self.terminal.lock().mode();
        let needs_editor_process =
            self.settings.free_type_mode && mode.contains(TermMode::ALT_SCREEN);
        let needs_current_directory = self.settings.current_directory_awareness_enabled
            && self.cwd_shell_integration_status != TerminalCwdShellIntegrationStatus::Active;
        if (!needs_editor_process && !needs_current_directory)
            || self.process_info_refresh_in_flight
            || self.last_process_info_refresh_requested.elapsed()
                < ACTIVE_PROCESS_INFO_REFRESH_INTERVAL
        {
            return;
        }
        let Some(probe) = self.process_info_probe() else {
            return;
        };

        self.process_info_refresh_in_flight = true;
        self.last_process_info_refresh_requested = Instant::now();
        let probe_task = cx.background_executor().spawn(async move {
            if needs_editor_process {
                // Full-screen editor routing needs the current foreground
                // executable; cwd-only probes deliberately preserve the
                // previous command and cannot establish that identity.
                probe.collect()
            } else {
                probe.collect_current_directory()
            }
        });
        cx.spawn(async move |weak, cx| {
            let info = probe_task.await;
            let _ = weak.update(cx, |this, cx| {
                this.process_info_refresh_in_flight = false;
                if this.apply_process_info(info) {
                    cx.notify();
                }
                this.wake_terminal_scheduler();
            });
        })
        .detach();
    }

    fn expire_editor_integration(&mut self, mode: TermMode, now: Instant) -> bool {
        let stale = self.editor_integration.is_some_and(|integration| {
            !mode.contains(TermMode::ALT_SCREEN)
                || now.saturating_duration_since(integration.last_seen)
                    > EDITOR_INTEGRATION_HEARTBEAT_TIMEOUT
        });
        if !stale {
            return false;
        }
        self.editor_integration = None;
        self.pending_editor_clipboard = None;
        true
    }

    fn expire_pending_terminal_cwd(&mut self, now: Instant) -> bool {
        let Some(pending) = self.pending_cwd.as_ref() else {
            return false;
        };
        if self.settings.current_directory_awareness_enabled
            && now.duration_since(pending.created_at) < PENDING_CWD_TIMEOUT
        {
            return false;
        }
        self.pending_cwd = None;
        true
    }

    fn advance_smooth_scroll_animation(&mut self, now: Instant) -> bool {
        let Some(animation) = self.smooth_scroll_animation else {
            return false;
        };
        let previous_offset = self.smooth_scroll_offset_px;
        let (next_offset, active) = animation.offset_at(now);
        self.smooth_scroll_offset_px = next_offset;
        if !active {
            self.smooth_scroll_animation = None;
        }
        self.smooth_scroll_offset_px != previous_offset
    }

    fn clear_command_mark_selection_for_tui_mode(&mut self, mode: TermMode) -> bool {
        if self.selected_command_mark_id.is_none() && self.hovered_command_mark_id.is_none()
            || command_mark_ui_available(self.settings.command_marks_enabled, mode)
        {
            return false;
        }

        // Command mark selection overlays belong to the normal scrollback UI.
        // TUI applications own the active screen and mouse surface instead.
        self.selected_command_mark_id = None;
        self.hovered_command_mark_id = None;
        true
    }

    fn smooth_scroll_display_offset(&self) -> f32 {
        if !self.settings.smooth_scroll {
            return self.snapshot.display_offset as f32;
        }

        let line_height = self.metrics.line_height_f32();
        if line_height <= f32::EPSILON {
            return self.snapshot.display_offset as f32;
        }

        // The terminal state still scrolls in whole rows. The paint layer keeps
        // the remaining wheel distance in pixels, so the scrollbar must use the
        // same fractional row offset to move with smooth-scrolling content.
        let display_offset = self.snapshot.display_offset as f32
            + f32::from(self.smooth_scroll_offset_px) / line_height;
        display_offset.clamp(0.0, self.snapshot.scrollback_lines as f32)
    }

    fn next_drain_budget(&self) -> TerminalDrainBudget {
        let drain = self.preferences.render_policy.drain;
        if self.last_drain_budget_exhausted {
            TerminalDrainBudget::new(drain.throughput_bytes, drain.max_events)
        } else if self.last_terminal_input.elapsed() <= RECENT_TERMINAL_INPUT_WINDOW {
            TerminalDrainBudget::new(drain.interactive_bytes, drain.max_events)
        } else {
            TerminalDrainBudget::new(drain.normal_bytes, drain.max_events)
        }
        .with_max_duration(MAX_UI_TERMINAL_DRAIN_DURATION)
        .with_performance_metrics(self.preferences.show_performance_overlay)
    }

    fn current_render_tier(&self) -> TerminalRenderTier {
        if self.last_drain_budget_exhausted {
            TerminalRenderTier::Boost
        } else if self.last_terminal_input.elapsed() <= RECENT_TERMINAL_INPUT_WINDOW
            || self.last_terminal_activity.elapsed() <= RECENT_TERMINAL_ACTIVITY_WINDOW
        {
            TerminalRenderTier::Normal
        } else {
            TerminalRenderTier::Idle
        }
    }

    fn update_render_stats(&mut self, report: &TerminalDrainReport, now: Instant) -> bool {
        let drain_micros = report.drain_duration.as_micros().min(u128::from(u64::MAX)) as u64;
        if self.preferences.show_performance_overlay
            && (report.events_drained > 0 || report.changed || report.pending_bytes > 0)
        {
            if self.drain_duration_samples_micros.len() == TERMINAL_PERFORMANCE_SAMPLE_CAPACITY {
                self.drain_duration_samples_micros.pop_front();
            }
            self.drain_duration_samples_micros.push_back(drain_micros);
        }
        let writes = report
            .events_drained
            .max(usize::from(report.changed && report.drained_bytes > 0));
        self.render_stats_window_writes = self.render_stats_window_writes.saturating_add(writes);
        let elapsed = now.saturating_duration_since(self.render_stats_window_start);
        let tier = self.current_render_tier();
        let published_writes_per_sec = if elapsed >= Duration::from_millis(500) {
            let seconds = elapsed.as_secs_f64().max(0.001);
            let writes_per_sec = (self.render_stats_window_writes as f64 / seconds).round() as u32;
            self.render_stats_window_start = now;
            self.render_stats_window_writes = 0;
            Some(writes_per_sec)
        } else {
            None
        };
        if report.changed
            && now.saturating_duration_since(self.last_terminal_input)
                <= RECENT_TERMINAL_ACTIVITY_WINDOW
            && self.last_latency_sampled_input != Some(self.last_terminal_input)
        {
            if self.input_latency_samples_micros.len() == TERMINAL_PERFORMANCE_SAMPLE_CAPACITY {
                self.input_latency_samples_micros.pop_front();
            }
            self.input_latency_samples_micros.push_back(
                now.saturating_duration_since(self.last_terminal_input)
                    .as_micros()
                    .min(u128::from(u64::MAX)) as u64,
            );
            self.last_latency_sampled_input = Some(self.last_terminal_input);
        }
        let drain_p95_micros = if self.preferences.show_performance_overlay {
            terminal_latency_percentiles(&self.drain_duration_samples_micros).1
        } else {
            self.render_stats.drain_p95_micros
        };
        let latency_percentiles = terminal_latency_percentiles(&self.input_latency_samples_micros);
        Self::apply_render_stats_sample(
            &mut self.render_stats,
            tier,
            report,
            drain_p95_micros,
            latency_percentiles,
            published_writes_per_sec,
        )
    }

    fn apply_render_stats_sample(
        stats: &mut TerminalRenderStats,
        tier: TerminalRenderTier,
        report: &TerminalDrainReport,
        drain_p95_micros: u64,
        latency_percentiles: (u64, u64, u64),
        published_writes_per_sec: Option<u32>,
    ) -> bool {
        let previous_stats = *stats;
        stats.tier = tier;
        stats.pending_bytes = report.pending_bytes;
        stats.drain_micros = report.drain_duration.as_micros().min(u128::from(u64::MAX)) as u64;
        stats.drain_p95_micros = drain_p95_micros;
        stats.drained_bytes = report.drained_bytes;
        stats.max_data_chunk_bytes = report.max_data_chunk_bytes;
        stats.output_processing_micros = report
            .output_processing_duration
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        stats.terminal_lock_wait_micros = report
            .terminal_lock_wait_duration
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        stats.input_latency_p50_micros = latency_percentiles.0;
        stats.input_latency_p95_micros = latency_percentiles.1;
        stats.input_latency_p99_micros = latency_percentiles.2;
        if let Some(writes_per_sec) = published_writes_per_sec {
            stats.writes_per_sec = writes_per_sec;
        }
        // The diagnostics overlay must never create a redraw loop merely to observe itself.
        *stats != previous_stats
    }

    fn handle_terminal_event(
        &mut self,
        event: TerminalEvent,
        cx: &mut Context<Self>,
    ) -> TerminalEventEffect {
        match event {
            TerminalEvent::Output(bytes) => {
                if let Some(recorder) = self.recorder.as_mut() {
                    recorder.record_output(&bytes);
                }
                let session_log_failed = self
                    .session_log
                    .as_mut()
                    .is_some_and(|log| log.write_output(bytes).is_err());
                if session_log_failed {
                    // Failure drops only this pane's file sink and leaves the terminal session alive.
                    self.last_session_log_path =
                        self.session_log.as_ref().and_then(|log| log.status().path);
                    self.session_log.take();
                    self.sync_terminal_output_events_enabled();
                    cx.emit(TerminalPaneEvent::SessionLogStatusChanged);
                    if let Some(sink) = &self.preferences.notice_sink
                        && !self.preferences.session_log_labels.write_failed.is_empty()
                    {
                        sink(TerminalNotice {
                            title: self.preferences.session_log_labels.write_failed.clone(),
                            description: None,
                            status_text: None,
                            progress: None,
                            variant: TerminalNoticeVariant::Error,
                        });
                    }
                }
                TerminalEventEffect::default()
            }
            TerminalEvent::TriggerMatched(matched) => {
                const MAX_PENDING_TRIGGER_MATCHES: usize = 128;
                let should_emit = self.pending_trigger_matches.is_empty();
                if self.pending_trigger_matches.len() < MAX_PENDING_TRIGGER_MATCHES {
                    self.pending_trigger_matches.push_back(matched);
                    if should_emit {
                        cx.emit(TerminalPaneEvent::TriggerMatchesAvailable);
                    }
                }
                TerminalEventEffect::default()
            }
            TerminalEvent::PrivilegePrompt(event) => {
                let previous_state_generation = self.privilege_prompt_tracker.state_generation();
                self.privilege_prompt_tracker
                    .observe_terminal_prompt_event(event, Instant::now());
                self.finish_privilege_prompt_tracker_update(previous_state_generation, cx);
                TerminalEventEffect::default()
            }
            TerminalEvent::TitleChanged(title) => {
                self.title = title.into();
                TerminalEventEffect::notify()
            }
            TerminalEvent::TitleReset => {
                self.title = SharedString::from("OxideTerm");
                TerminalEventEffect::notify()
            }
            TerminalEvent::Bell => {
                self.bell_flash = true;
                cx.spawn(async move |weak, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(180))
                        .await;
                    let _ = weak.update(cx, |this, cx| {
                        this.bell_flash = false;
                        cx.notify();
                    });
                })
                .detach();
                TerminalEventEffect::notify()
            }
            TerminalEvent::Wakeup => TerminalEventEffect::notify(),
            TerminalEvent::BlinkChanged(blinking) => {
                self.cursor_blink_terminal_enabled = blinking;
                self.reset_cursor_blink();
                TerminalEventEffect::notify()
            }
            TerminalEvent::ChildExited(code) => {
                self.notify_trzsz_connection_lost_if_active();
                self.notify_modem_connection_lost_if_active();
                let should_emit_exit = !self.terminal_exited;
                self.terminal_exited = true;
                self.title = match code {
                    Some(code) => format!("Process exited ({code})").into(),
                    None => "Process exited".into(),
                };
                if should_emit_exit {
                    cx.emit(TerminalPaneEvent::Exited { exit_code: code });
                }
                TerminalEventEffect::notify()
            }
            TerminalEvent::MagicDetected(kind) => {
                let _ = kind;
                TerminalEventEffect::default()
            }
            TerminalEvent::TrzszTransferPrompt {
                direction,
                selection,
                remote_is_windows,
            } => {
                self.handle_trzsz_transfer_prompt(
                    TrzszPromptRequest {
                        direction,
                        selection,
                        remote_is_windows,
                    },
                    cx,
                );
                TerminalEventEffect::notify()
            }
            TerminalEvent::ModemTransferPrompt { request, transfer } => {
                self.handle_modem_transfer_prompt(request, transfer, cx);
                TerminalEventEffect::notify()
            }
            TerminalEvent::EncodingHint(hint) => {
                let _ = hint;
                TerminalEventEffect::default()
            }
            TerminalEvent::EditorIntegration(event) => {
                if event.active {
                    if self
                        .editor_integration
                        .is_some_and(|current| current.state.application != event.application)
                    {
                        self.pending_editor_clipboard = None;
                    }
                    self.editor_integration = Some(ActiveTerminalEditorIntegration {
                        state: event,
                        last_seen: Instant::now(),
                    });
                } else if self
                    .editor_integration
                    .is_some_and(|current| current.state.application == event.application)
                {
                    self.editor_integration = None;
                    self.pending_editor_clipboard = None;
                }
                TerminalEventEffect::notify()
            }
            TerminalEvent::EditorClipboard(event) => {
                let Some(request) = self.pending_editor_clipboard.take() else {
                    return TerminalEventEffect::default();
                };
                let mode = self.terminal.lock().mode();
                let request_matches = request.requested_at.elapsed()
                    <= EDITOR_CLIPBOARD_REQUEST_TIMEOUT
                    && request.application == event.application
                    && request.operation == event.operation
                    && self
                        .active_editor_integration(mode)
                        .is_some_and(|state| state.application == event.application);
                if !request_matches {
                    return TerminalEventEffect::default();
                }

                // The editor payload is accepted only after a matching user
                // shortcut. GPUI owns the clipboard copy after this boundary;
                // the zeroizing event buffer is dropped immediately afterward.
                cx.write_to_clipboard(ClipboardItem::new_string(event.text.to_string()));
                TerminalEventEffect::default()
            }
            TerminalEvent::ShellIntegration(event) => {
                self.autosuggest_prompt_active = matches!(
                    event.kind,
                    oxideterm_terminal::ShellIntegrationEventKind::PromptStart
                        | oxideterm_terminal::ShellIntegrationEventKind::CommandStart
                );
                self.shell_integration_status = ShellIntegrationStatus {
                    detected: true,
                    state: match event.kind {
                        oxideterm_terminal::ShellIntegrationEventKind::PromptStart => {
                            ShellIntegrationLifecycleState::Prompt
                        }
                        oxideterm_terminal::ShellIntegrationEventKind::CommandStart => {
                            ShellIntegrationLifecycleState::Command
                        }
                        oxideterm_terminal::ShellIntegrationEventKind::OutputStart => {
                            ShellIntegrationLifecycleState::Output
                        }
                        oxideterm_terminal::ShellIntegrationEventKind::CommandEnd => {
                            ShellIntegrationLifecycleState::Closed
                        }
                    },
                    integration_source: Some(event.source),
                    last_seen_at: Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|duration| duration.as_millis() as u64)
                            .unwrap_or_default(),
                    ),
                };
                TerminalEventEffect::notify()
            }
            TerminalEvent::CommandMark(event) => {
                if let TerminalCommandMarkEvent::Closed(mark) = &event {
                    self.observe_terminal_cwd_action_from_closed_command_mark(mark, cx);
                }
                if !self.settings.command_marks_enabled {
                    self.clear_visual_command_marks();
                } else {
                    match event {
                        TerminalCommandMarkEvent::Created(mut mark) => {
                            if mark.detection_source
                                == TerminalCommandMarkDetectionSource::ShellIntegration
                                && let Some((index, submitted_by)) =
                                    self.shell_integration_dedup_candidate(&mark)
                            {
                                let shell_command_id = mark.command_id.clone();
                                let frontend_command_id =
                                    self.command_marks[index].command_id.clone();
                                mark.command_id = frontend_command_id.clone();
                                mark.submitted_by = Some(submitted_by);
                                self.command_marks.remove(index);
                                self.command_mark_id_aliases
                                    .insert(shell_command_id, frontend_command_id);
                            }
                            if let Some(command) = mark.command.as_deref() {
                                // Shell integration is the terminal-owned
                                // submitted-command source. Feed it to the
                                // privilege tracker so bare sudo prompts do not
                                // depend on lossy key/IME reconstruction.
                                let previous_state_generation =
                                    self.privilege_prompt_tracker.state_generation();
                                self.privilege_prompt_tracker
                                    .observe_submitted_command(command, Instant::now());
                                self.finish_privilege_prompt_tracker_update(
                                    previous_state_generation,
                                    cx,
                                );
                            }
                            self.command_fact_ledger.create_from_mark(&mark);
                            self.command_marks.push(mark);
                            self.trim_command_marks();
                        }
                        TerminalCommandMarkEvent::Closed(mut mark) => {
                            if let Some(frontend_command_id) =
                                self.command_mark_id_aliases.remove(&mark.command_id)
                            {
                                mark.command_id = frontend_command_id;
                            }
                            self.command_fact_ledger.close_from_mark(&mark);
                            if let Some(existing) = self
                                .command_marks
                                .iter_mut()
                                .find(|candidate| candidate.command_id == mark.command_id)
                            {
                                *existing = mark;
                            } else {
                                self.command_marks.push(mark);
                            }
                        }
                        TerminalCommandMarkEvent::Reset => {
                            self.clear_visual_command_marks();
                        }
                    }
                    if let Some(selected_id) = &self.selected_command_mark_id
                        && !self
                            .command_marks
                            .iter()
                            .any(|mark| mark.command_id == *selected_id)
                    {
                        self.selected_command_mark_id = None;
                    }
                    if let Some(hovered_id) = &self.hovered_command_mark_id
                        && !self
                            .command_marks
                            .iter()
                            .any(|mark| mark.command_id == *hovered_id)
                    {
                        self.hovered_command_mark_id = None;
                    }
                }
                self.command_marks_render_cache_dirty = true;
                TerminalEventEffect::notify()
            }
            TerminalEvent::CwdChanged { cwd, host } => {
                self.cwd = Some(cwd);
                self.cwd_source = Some(TerminalWorkingDirectorySource::ShellIntegration);
                // Managed OSC 7 hooks emit at the shell prompt, which establishes the minimum
                // reliable boundary for terminal-side history suggestions.
                self.autosuggest_prompt_active = true;
                // A prepared startup profile becomes active only after the
                // terminal parser receives a valid directory report.
                self.cwd_shell_integration_status = TerminalCwdShellIntegrationStatus::Active;
                self.pending_cwd = None;
                self.cwd_host = host;
                cx.emit(TerminalPaneEvent::CurrentDirectoryChanged);
                TerminalEventEffect::notify()
            }
            TerminalEvent::ClipboardStore(text) => {
                if self.settings.osc52_clipboard {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                TerminalEventEffect::default()
            }
            TerminalEvent::ClipboardLoad(formatter) => {
                if let Some(response) = build_osc52_clipboard_response(
                    self.settings.osc52_clipboard_read,
                    || cx.read_from_clipboard().and_then(|item| item.text()),
                    formatter.as_ref(),
                ) {
                    self.send_protocol_bytes(response.as_bytes(), cx);
                }
                TerminalEventEffect::default()
            }
        }
    }

    fn handle_focus_change(&mut self, focused: bool, cx: &mut Context<Self>) {
        self.focused = focused;
        let _ = self.terminal.lock().set_focused(focused);
        self.reset_cursor_blink();
        // Focus changes must consume already queued output instead of waiting for an old deadline.
        self.tick(cx);
        self.wake_terminal_scheduler();
        cx.notify();
    }

    fn send_protocol_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) -> bool {
        if !self.terminal_accepts_input() {
            return false;
        }

        if self.terminal.lock().write_protocol_bytes(bytes).is_ok() {
            if let Some(recorder) = self.recorder.as_mut() {
                recorder.record_input(&String::from_utf8_lossy(bytes));
            }
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn send_user_protocol_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.send_user_protocol_bytes_without_broadcast(bytes, cx) {
            self.broadcast_user_input(TerminalBroadcastInputKind::Protocol, bytes, cx);
        }
    }

    fn send_user_protocol_bytes_without_broadcast(
        &mut self,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal_accepts_input() {
            return false;
        }
        let Some(bytes) = self.apply_plugin_input_interceptor(bytes) else {
            return false;
        };
        let bytes = Zeroizing::new(bytes);
        self.observe_user_input("protocol", &bytes, cx);
        if self.send_protocol_bytes(&bytes, cx) {
            self.restore_live_output_after_user_input();
            return true;
        }
        false
    }

    fn broadcast_user_input(
        &self,
        kind: TerminalBroadcastInputKind,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) {
        let Some(broadcaster) = self.input_broadcaster.clone() else {
            return;
        };
        // The callback is synchronous and receives borrowed input so commands
        // are never retained in pane events, logs, or background tasks.
        broadcaster(kind, bytes, cx);
    }

    fn send_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }

        if self.terminal.lock().write_text(text).is_ok() {
            self.restore_live_output_after_user_input();
            if let Some(recorder) = self.recorder.as_mut() {
                recorder.record_input(text);
            }
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
        }
    }

    fn restore_live_output_after_user_input(&mut self) {
        if !viewport_needs_live_output_restore(
            self.snapshot.display_offset,
            self.smooth_scroll_offset_px,
            self.smooth_scroll_animation.is_some(),
        ) {
            return;
        }

        // User-originated input should reveal the live prompt without changing
        // the viewport for mouse reports or terminal-owned protocol responses.
        let snapshot = {
            let mut terminal = self.terminal.lock();
            terminal.scroll_to_bottom();
            terminal.snapshot()
        };
        self.clear_smooth_scroll_remainder();
        self.snapshot = self.stamp_snapshot(snapshot);
    }

    fn apply_plugin_input_interceptor(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        let Some(interceptor) = &self.plugin_input_interceptor else {
            return Some(bytes.to_vec());
        };
        // Plugin input hooks run before command tracking and shell writes so a
        // transformed or suppressed payload has the same boundary as Tauri.
        match interceptor(bytes) {
            TerminalInputInterceptorResult::Continue(bytes) => Some(bytes),
            TerminalInputInterceptorResult::Suppress => None,
        }
    }

    fn observe_user_input(&mut self, source: &'static str, bytes: &[u8], cx: &mut Context<Self>) {
        let now = Instant::now();
        if self.observe_privilege_input(source, bytes, now, cx)
            == PrivilegeInputObservation::SecretEntry
        {
            return;
        }
        let Some(command) = self.observe_autosuggest_input_bytes(bytes, cx) else {
            return;
        };
        self.observe_current_directory_submitted_command(&command, cx);
        if self.shell_integration_status.detected
            || !self.settings.command_marks_user_input_observed
        {
            return;
        }
        self.begin_command_mark(
            &command,
            TerminalCommandMarkDetectionSource::UserInputObserved,
            cx,
        );
    }

    fn observe_privilege_input(
        &mut self,
        source: &'static str,
        bytes: &[u8],
        now: Instant,
        cx: &mut Context<Self>,
    ) -> PrivilegeInputObservation {
        if !privilege_prompt_input_tracking_available(self.terminal.lock().mode()) {
            return PrivilegeInputObservation::Normal;
        }
        let previous_state_generation = self.privilege_prompt_tracker.state_generation();
        let observation = self
            .privilege_prompt_tracker
            .observe_user_input_bytes(bytes, now);
        self.finish_privilege_prompt_tracker_update(previous_state_generation, cx);
        log_privilege_prompt_terminal_pane(format_args!(
            "input observed: source={} has_cr={} has_lf={} observation={}",
            source,
            bytes.contains(&b'\r'),
            bytes.contains(&b'\n'),
            privilege_input_observation_name(observation)
        ));
        if observation == PrivilegeInputObservation::SecretEntry
            && self.clear_privilege_prompt_inline_hint()
        {
            cx.notify();
        }
        observation
    }

    fn observe_autosuggest_input_bytes(
        &mut self,
        bytes: &[u8],
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let previous_state = self.input_tracker.state();
        let command = self.input_tracker.apply_bytes(bytes);
        let next_state = self.input_tracker.state();
        if next_state != previous_state {
            self.autosuggest_selected_index = None;
            self.autosuggest_dismissed_query = None;
        }
        let command = command?;
        self.autosuggest_prompt_active = false;
        self.command_fact_ledger
            .record_runtime_autosuggest_command(&command);
        self.command_history.record(&command);
        Some(command)
    }

    fn observe_current_directory_submitted_command(
        &mut self,
        command: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.current_directory_awareness_enabled || self.cwd_is_shell_integrated() {
            return;
        }
        let cwd = self
            .pending_cwd
            .as_ref()
            .map(|pending| pending.path.as_str())
            .or(self.cwd.as_deref());
        if let Some(next_cwd) = cwd_after_simple_cd_command(command, cwd) {
            // The pending state lets the UI follow a submitted simple `cd`
            // immediately without treating terminal viewport text as evidence.
            self.set_pending_current_working_directory_from_terminal_action(
                next_cwd,
                command.to_string(),
                cx,
            );
        }
    }

    fn cwd_is_shell_integrated(&self) -> bool {
        self.cwd_source == Some(TerminalWorkingDirectorySource::ShellIntegration)
    }

    fn terminal_accepts_input(&self) -> bool {
        #[cfg(test)]
        if self.test_accepts_input {
            return !self.input_locked;
        }
        let terminal_interactive = self.terminal.lock().is_interactive();
        self.terminal_accepts_input_with_interactive_state(terminal_interactive)
    }

    fn terminal_accepts_input_with_interactive_state(&self, terminal_interactive: bool) -> bool {
        #[cfg(test)]
        if self.test_accepts_input {
            // Unit tests can exercise input routing without creating a live PTY.
            return !self.input_locked;
        }
        !self.input_locked && !self.terminal_exited && terminal_interactive
    }

    fn commit_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.marked_text = None;
        if let Some(prompt) = self.tmux_prompt.as_mut() {
            prompt
                .value
                .extend(text.chars().filter(|character| !character.is_control()));
            cx.notify();
            return;
        }
        if self.commit_text_without_broadcast(text, cx) {
            self.broadcast_user_input(TerminalBroadcastInputKind::Text, text.as_bytes(), cx);
        }
    }

    fn commit_text_without_broadcast(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if !self.terminal_accepts_input() {
            return false;
        }
        let Some(bytes) = self.apply_plugin_input_interceptor(text.as_bytes()) else {
            return false;
        };
        let bytes = Zeroizing::new(bytes);
        let mode = self.terminal.lock().mode();
        self.delete_free_type_selection_if_active(mode, cx);
        self.observe_user_input("text", &bytes, cx);
        if self.send_protocol_bytes(&bytes, cx) {
            self.restore_live_output_after_user_input();
            return true;
        }
        false
    }

    fn set_marked_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.marked_text = (!text.is_empty()).then(|| text.to_string());
        cx.notify();
    }

    fn clear_marked_text(&mut self, cx: &mut Context<Self>) {
        if self.marked_text.take().is_some() {
            cx.notify();
        }
    }

    fn marked_text_range(&self) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn should_blink_cursor(&self) -> bool {
        let alt_screen = self.terminal.lock().mode().contains(TermMode::ALT_SCREEN);
        should_blink_cursor_for_mode(
            self.settings.blink_mode,
            self.focused,
            self.cursor_blink_terminal_enabled,
            alt_screen,
            self.preferences.cursor_shape,
        )
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.last_cursor_blink = Instant::now();
    }

    fn update_cursor_blink(&mut self, cx: &mut Context<Self>) {
        if !self.should_blink_cursor() {
            if !self.cursor_visible {
                self.cursor_visible = true;
                cx.notify();
            }
            self.last_cursor_blink = Instant::now();
            return;
        }

        if self.last_cursor_blink.elapsed() >= CURSOR_BLINK_INTERVAL {
            self.cursor_visible = !self.cursor_visible;
            self.last_cursor_blink = Instant::now();
            cx.notify();
        }
    }

    pub fn apply_viewport_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut Context<Self>,
    ) {
        self.bounds = Some(bounds);
        self.viewport_scale_factor_bits = Some(scale_factor.to_bits());
        let cell_width = self.metrics.cell_width_f32();
        let line_height = self.metrics.line_height_f32();
        let width = terminal_grid_span_for_viewport(
            bounds.size.width,
            cell_width,
            self.command_mark_gutter_width(),
        );
        let height =
            (f32::from(bounds.size.height) - TERMINAL_CONTENT_PADDING * 2.0).max(line_height * 2.0);
        let cols = whole_cells_in_span(width, cell_width).max(2);
        let rows = whole_cells_in_span(height, line_height).max(2);
        let cell_width_px = (cell_width * scale_factor).ceil().max(1.0) as u16;
        let cell_height_px = (line_height * scale_factor).ceil().max(1.0) as u16;
        let resize = (cols, rows, cell_width_px, cell_height_px);

        if self.last_pty_resize == Some(resize) || self.pending_pty_resize == Some(resize) {
            return;
        }

        self.pending_pty_resize = Some(resize);
        self.pty_resize_generation = self.pty_resize_generation.wrapping_add(1);
        let generation = self.pty_resize_generation;
        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(PTY_RESIZE_DEBOUNCE).await;
            let _ = weak.update(cx, |view, cx| {
                view.flush_pending_pty_resize(generation, cx);
            });
        })
        .detach();
    }

    pub fn resize_grid(
        &mut self,
        cols: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        // Preserve the pane's measured cell size while changing only the requested grid.
        let (_, _, cell_width_px, cell_height_px) = self.last_pty_resize.unwrap_or_else(|| {
            (
                self.snapshot.cols,
                self.snapshot.rows,
                self.metrics.cell_width_f32().ceil().max(1.0) as u16,
                self.metrics.line_height_f32().ceil().max(1.0) as u16,
            )
        });
        let requested = (cols, rows, cell_width_px, cell_height_px);
        self.pending_pty_resize = Some(requested);
        self.pty_resize_generation = self.pty_resize_generation.wrapping_add(1);
        self.flush_pending_pty_resize(self.pty_resize_generation, cx);
        (self.last_pty_resize == Some(requested))
            .then_some(())
            .ok_or_else(|| "The terminal backend rejected the requested grid size.".to_string())
    }

    fn flush_pending_pty_resize(&mut self, generation: u64, cx: &mut Context<Self>) {
        if generation != self.pty_resize_generation {
            return;
        }
        let Some((cols, rows, cell_width_px, cell_height_px)) = self.pending_pty_resize.take()
        else {
            return;
        };
        let resize = (cols, rows, cell_width_px, cell_height_px);
        if self.last_pty_resize == Some(resize) {
            return;
        }
        let grid_changed = self.snapshot.cols != cols || self.snapshot.rows != rows;

        let next_snapshot = {
            let mut terminal = self.terminal.lock();
            terminal
                .resize_with_cell_size(cols, rows, cell_width_px, cell_height_px)
                .is_ok()
                .then(|| terminal.snapshot())
        };
        if let Some(snapshot) = next_snapshot {
            self.last_pty_resize = Some(resize);
            if let Some(recorder) = self.recorder.as_mut() {
                recorder.record_resize(cols, rows);
            }
            self.clear_smooth_scroll_remainder();
            self.snapshot = self.stamp_snapshot(snapshot);
            self.mark_terminal_content_changed(cx);
            if grid_changed {
                // The backend also resets its shell-integration state. Clear
                // immediately so stale hit regions cannot survive one UI frame.
                self.reset_command_marks_while_awaiting_backend_reset();
            }
            cx.notify();
        }
    }

    fn content_origin(&self) -> gpui::Point<Pixels> {
        self.bounds
            .map(|bounds| bounds.origin)
            .unwrap_or_else(|| gpui::point(px(0.0), px(0.0)))
    }

    fn timestamp_gutter_width(&self) -> f32 {
        terminal_timestamp_gutter_width(&self.metrics, self.terminal_timestamps_enabled)
    }

    fn terminal_content_padding_x(&self) -> f32 {
        TERMINAL_CONTENT_PADDING + self.timestamp_gutter_width() + self.command_mark_gutter_width()
    }

    fn command_mark_gutter_width(&self) -> f32 {
        if self.settings.command_marks_enabled {
            TERMINAL_COMMAND_MARK_GUTTER_WIDTH
        } else {
            0.0
        }
    }

    pub fn cursor_anchor(&self) -> Option<TerminalCursorAnchor> {
        let bounds = self.bounds?;
        let cursor_bounds = ime_cursor_bounds_for_snapshot(&self.snapshot, &self.metrics)?;
        // The app layer owns overlays such as inline AI chat, but only the
        // terminal pane knows the bidi-aware cursor visual column and measured
        // cell metrics. Expose pane-local facts rather than making workspace
        // code duplicate terminal layout math.
        Some(TerminalCursorAnchor {
            x: f32::from(cursor_bounds.origin.x) + self.terminal_content_padding_x(),
            y: f32::from(cursor_bounds.origin.y) + TERMINAL_CONTENT_PADDING,
            line_height: self.metrics.line_height_f32(),
            char_width: self.metrics.cell_width_f32(),
            container_width: f32::from(bounds.size.width),
            container_height: f32::from(bounds.size.height),
        })
    }
}

impl EventEmitter<TerminalPaneEvent> for TerminalPane {}

pub fn paste_needs_confirmation(text: &str) -> bool {
    const PASTE_LINE_THRESHOLD: usize = 1;
    const PASTE_CHAR_THRESHOLD: usize = 50;

    text.contains('\n')
        && (text.split('\n').count() > PASTE_LINE_THRESHOLD || text.len() > PASTE_CHAR_THRESHOLD)
}

fn terminal_trigger_input(text: &str, append_enter: bool) -> Option<Zeroizing<String>> {
    if text.is_empty() && !append_enter {
        return None;
    }
    let mut input = Zeroizing::new(text.to_string());
    if append_enter {
        input.push('\r');
    }
    Some(input)
}

fn graphics_options_from_preferences(preferences: &TerminalUiPreferences) -> GraphicsOptions {
    let graphics = preferences.render_policy.terminal_graphics;
    let storage_limit_mb = graphics.storage_limit_bytes.div_ceil(1024 * 1024);
    GraphicsOptions {
        enabled: true,
        sixel: true,
        iterm2_inline: true,
        kitty: true,
        pixel_limit: graphics.pixel_limit.min(u32::MAX as usize) as u32,
        storage_limit_mb: storage_limit_mb.min(u32::MAX as usize) as u32,
        show_placeholder: graphics.show_placeholders,
        kitty_file_transmission: KittyFileTransmissionControl::new(),
    }
}

fn current_terminal_timestamp_label() -> String {
    let now = chrono::Local::now();
    terminal_timestamp_label(
        now.hour(),
        now.minute(),
        now.second(),
        now.timestamp_subsec_millis(),
    )
}

fn terminal_timestamp_label(hour: u32, minute: u32, second: u32, millis: u32) -> String {
    // Bracketed fixed-width labels separate timestamp metadata from terminal
    // output while keeping the paint-only gutter stable.
    format!("[{hour:02}:{minute:02}:{second:02}.{millis:03}]")
}

fn record_timestampable_snapshot_rows(
    row_timestamps: &mut HashMap<u64, TerminalRowTimestamp>,
    snapshot: &TerminalSnapshot,
    label: &str,
) {
    for row in &snapshot.lines {
        // The snapshot signature is a cheap invalidation key. Cursor-only changes
        // still fall through to the content signature comparison below.
        if row_timestamps
            .get(&row.line_id)
            .is_some_and(|timestamp| timestamp.source_signature == row.signature)
        {
            continue;
        }

        if terminal_row_has_timestamp_content(row) {
            let timestamp_signature = terminal_row_timestamp_signature(row);
            if let Some(timestamp) = row_timestamps.get_mut(&row.line_id) {
                if timestamp.signature != timestamp_signature {
                    timestamp.label = label.to_string();
                    timestamp.signature = timestamp_signature;
                }
                timestamp.source_signature = row.signature;
            } else {
                row_timestamps.insert(
                    row.line_id,
                    TerminalRowTimestamp {
                        label: label.to_string(),
                        signature: timestamp_signature,
                        source_signature: row.signature,
                    },
                );
            }
        } else {
            // Blank viewport rows are recycled later. Removing their metadata
            // prevents new output from inheriting a stale line-modification time.
            row_timestamps.remove(&row.line_id);
        }
    }
}

fn trim_row_timestamp_history(
    row_timestamps: &mut HashMap<u64, TerminalRowTimestamp>,
    retained_min_line: &mut Option<u64>,
    min_line: u64,
) {
    let Some(previous_min_line) = *retained_min_line else {
        row_timestamps.retain(|line, _| *line >= min_line);
        *retained_min_line = Some(min_line);
        return;
    };

    if min_line <= previous_min_line {
        // Snapshot identity can restart when a pane is rebuilt. New rows may then be inserted
        // below the former boundary, so restart incremental trimming from this identity.
        *retained_min_line = Some(min_line);
        return;
    }

    let advanced_lines =
        usize::try_from(min_line.saturating_sub(previous_min_line)).unwrap_or(usize::MAX);
    if advanced_lines < row_timestamps.len() {
        // Normal scrolling advances by only a few rows. Removing those keys is
        // cheaper than scanning the entire retained timestamp history.
        for line in previous_min_line..min_line {
            row_timestamps.remove(&line);
        }
    } else {
        row_timestamps.retain(|line, _| *line >= min_line);
    }
    *retained_min_line = Some(min_line);
}

fn terminal_row_timestamp_signature(row: &TerminalRow) -> u64 {
    let mut hasher = DefaultHasher::new();
    row.wrapped.hash(&mut hasher);
    for cell in row.cells.iter() {
        cell.ch.hash(&mut hasher);
        cell.zerowidth().hash(&mut hasher);
        cell.wide.hash(&mut hasher);
        cell.fg.hash(&mut hasher);
        cell.bg.hash(&mut hasher);
        cell.attrs.hash(&mut hasher);
        cell.hyperlink().hash(&mut hasher);
    }
    hasher.finish()
}

fn terminal_row_has_timestamp_content(row: &TerminalRow) -> bool {
    row.cells
        .iter()
        .any(|cell| !cell.ch.is_whitespace() || !cell.zerowidth().is_empty())
}

fn hex_color(color: u32) -> String {
    format!("#{:06x}", color & 0x00ff_ffff)
}

fn build_osc52_clipboard_response(
    allowed: bool,
    read_clipboard: impl FnOnce() -> Option<String>,
    formatter: &(dyn Fn(&str) -> String + Send + Sync),
) -> Option<Zeroizing<String>> {
    if !allowed {
        return None;
    }
    // OSC 52 reads can expose arbitrary clipboard data, so clear both temporary UI and wire
    // copies immediately after the protocol response is submitted.
    let text = Zeroizing::new(read_clipboard()?);
    Some(Zeroizing::new(formatter(&text)))
}

fn whole_cells_in_span(span: f32, cell_span: f32) -> usize {
    let cells = span / cell_span;
    let nearest_integer = cells.round();
    if (cells - nearest_integer).abs() <= 0.0001 {
        nearest_integer.max(0.0) as usize
    } else {
        cells.floor().max(0.0) as usize
    }
}

fn terminal_grid_span_for_viewport(
    viewport_width: Pixels,
    cell_width: f32,
    left_gutter_width: f32,
) -> f32 {
    // Browser terminals reserve right-side scrollbar chrome outside the grid.
    // Keep that gutter stable even before scrollback exists so history growth
    // does not resize the PTY and push the scrollbar outside the viewport.
    // Timestamp labels are a visual overlay and must not change PTY columns;
    // toggling them should never reflow scrollback or restamp old rows.
    (f32::from(viewport_width)
        - TERMINAL_CONTENT_PADDING * 2.0
        - left_gutter_width
        - SCROLLBAR_RESERVED_WIDTH)
        .max(cell_width * 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, collections::HashMap, sync::Arc};

    use gpui::{AppContext, IntoElement, Render, TestAppContext, div};
    use oxideterm_terminal::{TerminalAttrs, TerminalCell, TerminalColor, TerminalCursorShape};

    #[test]
    fn idle_terminal_has_no_maintenance_deadline() {
        assert_eq!(
            terminal_maintenance_interval(
                false,
                false,
                Duration::from_millis(8),
                false,
                None,
                None,
                None,
                None,
            ),
            None
        );
    }

    struct TerminalTestRoot;

    struct TerminalBroadcastRecorder {
        delivered: Vec<(TerminalBroadcastInputKind, Vec<u8>)>,
    }

    impl Render for TerminalTestRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[test]
    fn missing_serial_port_is_reported_before_pane_construction() {
        const MISSING_SERIAL_PORT_PATH: &str = "oxideterm-test-missing-serial-port";
        let config = SerialSessionConfig {
            port_path: MISSING_SERIAL_PORT_PATH.to_string(),
            baud_rate: 115_200,
            data_bits: 8,
            stop_bits: 1,
            parity: oxideterm_terminal::SerialParity::None,
            flow_control: oxideterm_terminal::SerialFlowControl::None,
        };

        let result = TerminalPane::open_serial_session_with_preferences(
            config,
            &TerminalUiPreferences::default(),
        );
        let error = match result {
            Ok(_) => panic!("missing serial port must not open"),
            Err(error) => error,
        };
        let serial_error = error
            .downcast_ref::<oxideterm_terminal::SerialError>()
            .expect("serial backend error");
        assert_eq!(
            serial_error.code,
            oxideterm_terminal::SerialErrorCode::PortNotFound
        );
        assert_eq!(
            serial_error.port_path.as_deref(),
            Some(MISSING_SERIAL_PORT_PATH)
        );
    }

    #[test]
    fn trigger_input_appends_only_the_explicit_enter() {
        assert!(terminal_trigger_input("", false).is_none());
        assert_eq!(
            terminal_trigger_input("value", false)
                .as_ref()
                .map(|input| input.as_str()),
            Some("value")
        );
        assert_eq!(
            terminal_trigger_input("value\nnext", true)
                .as_ref()
                .map(|input| input.as_str()),
            Some("value\nnext\r")
        );
        assert_eq!(
            terminal_trigger_input("", true)
                .as_ref()
                .map(|input| input.as_str()),
            Some("\r")
        );
    }

    #[test]
    fn command_mark_ui_is_hidden_while_a_tui_owns_the_terminal_surface() {
        assert!(command_mark_ui_available(true, TermMode::empty()));
        assert!(!command_mark_ui_available(false, TermMode::empty()));
        assert!(!command_mark_ui_available(true, TermMode::ALT_SCREEN));
        assert!(!command_mark_ui_available(
            true,
            TermMode::MOUSE_REPORT_CLICK
        ));
    }

    #[test]
    fn privilege_prompt_input_tracking_ignores_full_screen_application_keys() {
        assert!(privilege_prompt_input_tracking_available(TermMode::empty()));
        assert!(!privilege_prompt_input_tracking_available(
            TermMode::ALT_SCREEN
        ));
    }

    #[test]
    fn local_cwd_integration_waits_for_first_report_before_becoming_active() {
        assert_eq!(
            initial_cwd_shell_integration_status(
                true,
                TerminalSessionKind::LocalPty,
                TerminalCwdIntegrationLaunchState::Prepared,
            ),
            TerminalCwdShellIntegrationStatus::Installing
        );
        assert_eq!(
            initial_cwd_shell_integration_status(
                true,
                TerminalSessionKind::LocalPty,
                TerminalCwdIntegrationLaunchState::Unavailable,
            ),
            TerminalCwdShellIntegrationStatus::Failed
        );
        assert_eq!(
            initial_cwd_shell_integration_status(
                false,
                TerminalSessionKind::LocalPty,
                TerminalCwdIntegrationLaunchState::Prepared,
            ),
            TerminalCwdShellIntegrationStatus::Disabled
        );
    }

    #[test]
    fn user_input_restores_only_non_live_viewports() {
        for (display_offset, smooth_offset, animation_active, expected) in [
            (3, px(0.0), false, true),
            (0, px(2.0), true, true),
            (0, px(0.0), false, false),
        ] {
            assert_eq!(
                viewport_needs_live_output_restore(display_offset, smooth_offset, animation_active),
                expected
            );
        }
    }

    #[gpui::test]
    fn direct_user_input_broadcasts_once_for_each_input_path(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_window, _cx| TerminalTestRoot);
        let pane = cx.update(|window, cx| {
            cx.new(|cx| {
                TerminalPane::new_recording_playback(
                    DEFAULT_COLS,
                    DEFAULT_ROWS,
                    TerminalUiPreferences::default(),
                    window,
                    cx,
                )
                .expect("test terminal pane")
            })
        });
        let recorder = cx.new(|_| TerminalBroadcastRecorder {
            delivered: Vec::new(),
        });
        let recorder_for_broadcaster = recorder.downgrade();
        pane.update(cx, |pane, _cx| {
            pane.test_accepts_input = true;
            pane.set_input_broadcaster(Some(Rc::new(move |kind, bytes, cx| {
                let _ = recorder_for_broadcaster.update(cx, |recorder, _cx| {
                    recorder.delivered.push((kind, bytes.to_vec()));
                });
            })));
        });

        pane.update(cx, |pane, cx| {
            pane.commit_text("x", cx);
            pane.send_user_protocol_bytes(b"\x1b[D", cx);
            pane.paste_text("y", cx);
        });
        let delivered = recorder.read_with(cx, |recorder, _cx| recorder.delivered.clone());
        assert_eq!(
            delivered.as_slice(),
            [
                (TerminalBroadcastInputKind::Text, b"x".to_vec()),
                (TerminalBroadcastInputKind::Protocol, b"\x1b[D".to_vec()),
                (TerminalBroadcastInputKind::Paste, b"y".to_vec()),
            ]
        );

        pane.update(cx, |pane, cx| {
            assert!(pane.send_broadcast_input(TerminalBroadcastInputKind::Text, b"z", cx));
        });
        assert_eq!(
            recorder.read_with(cx, |recorder, _cx| recorder.delivered.len()),
            3
        );
    }

    #[gpui::test]
    fn history_suggestions_follow_prompt_capability_without_requiring_direct_focus(
        cx: &mut TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_window, _cx| TerminalTestRoot);
        let pane = cx.update(|window, cx| {
            let mut preferences = TerminalUiPreferences::default();
            preferences.command_history =
                SharedTerminalCommandHistory::from_commands(vec!["docker ps".to_string()]);
            cx.new(|cx| {
                TerminalPane::new_recording_playback(
                    DEFAULT_COLS,
                    DEFAULT_ROWS,
                    preferences,
                    window,
                    cx,
                )
                .expect("test terminal pane")
            })
        });

        pane.update(cx, |pane, cx| {
            pane.test_accepts_input = true;
            pane.focused = false;
            pane.snapshot.lines[pane.snapshot.cursor_row].active_input = true;
            pane.observe_autosuggest_input_bytes(b"dock", cx);

            assert!(pane.terminal_autosuggest_candidates().is_empty());
            pane.autosuggest_prompt_active = true;
            assert_eq!(
                pane.terminal_autosuggest_candidates()
                    .into_iter()
                    .map(|candidate| candidate.command)
                    .collect::<Vec<_>>(),
                ["docker ps"]
            );

            pane.observe_autosuggest_input_bytes(b"\r", cx);
            assert!(!pane.autosuggest_prompt_active);
            assert!(pane.terminal_autosuggest_candidates().is_empty());
        });
    }

    #[test]
    fn osc52_clipboard_read_respects_permission_and_formats_allowed_content() {
        let read_called = Cell::new(false);

        let response = build_osc52_clipboard_response(
            false,
            || {
                read_called.set(true);
                Some("clipboard".to_string())
            },
            &|text| format!("response:{text}"),
        );

        assert!(response.is_none());
        assert!(!read_called.get());

        let response =
            build_osc52_clipboard_response(true, || Some("clipboard".to_string()), &|text| {
                format!("response:{text}")
            })
            .unwrap();

        assert_eq!(response.as_str(), "response:clipboard");
    }

    fn timestamp_test_cell(ch: char) -> TerminalCell {
        TerminalCell {
            ch,
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: Default::default(),
            attrs: TerminalAttrs::default(),
            extra: None,
            cursor: false,
        }
    }

    fn timestamp_test_row(absolute_line: i64, text: &str) -> TerminalRow {
        timestamp_test_row_with_cursor(absolute_line, text, None, false)
    }

    fn timestamp_test_row_with_cursor(
        absolute_line: i64,
        text: &str,
        cursor_col: Option<usize>,
        active_input: bool,
    ) -> TerminalRow {
        let mut cells = text.chars().map(timestamp_test_cell).collect::<Vec<_>>();
        if cells.is_empty() {
            cells.push(timestamp_test_cell(' '));
        }
        if let Some(cursor_col) = cursor_col
            && let Some(cell) = cells.get_mut(cursor_col)
        {
            cell.cursor = true;
        }
        let mut row = TerminalRow {
            line_id: absolute_line.max(0) as u64,
            source_id: 0,
            absolute_line,
            cells: Arc::new(cells),
            wrapped: false,
            active_input,
            signature: 0,
        };
        row.refresh_signature();
        row
    }

    fn timestamp_test_snapshot(row: TerminalRow) -> TerminalSnapshot {
        TerminalSnapshot {
            generation: 1,
            cols: row.cells.len().max(1),
            rows: 1,
            cursor_col: 0,
            cursor_row: 0,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 0,
            lines: vec![row],
            images: Vec::new(),
        }
    }

    #[test]
    fn snapshot_line_ids_follow_scrolled_sources_without_reusing_recycled_rows() {
        let mut previous_lines = (0..3)
            .map(|line| timestamp_test_row(line, &format!("line-{line}")))
            .collect::<Vec<_>>();
        for (index, row) in previous_lines.iter_mut().enumerate() {
            row.line_id = 10 + index as u64;
            row.source_id = 100 + index;
        }
        let previous = TerminalSnapshot {
            generation: 1,
            cols: 8,
            rows: 3,
            cursor_col: 0,
            cursor_row: 2,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 0,
            lines: previous_lines,
            images: Vec::new(),
        };
        let mut next = previous.clone();
        next.lines = vec![
            previous.lines[1].clone(),
            previous.lines[2].clone(),
            previous.lines[0].clone(),
        ];
        for (index, row) in next.lines.iter_mut().enumerate() {
            row.line_id = 0;
            row.absolute_line = index as i64;
        }
        next.lines[0].signature = 2_000;
        let mut next_line_id = 20;

        reconcile_snapshot_line_ids(&mut next, &previous, &mut next_line_id);

        assert_eq!(
            next.lines.iter().map(|row| row.line_id).collect::<Vec<_>>(),
            vec![11, 12, 20]
        );
        assert_eq!(next_line_id, 21);
    }

    #[test]
    fn row_timestamps_track_last_modified_nonblank_content() {
        let mut row_timestamps = HashMap::new();
        let blank_snapshot = timestamp_test_snapshot(timestamp_test_row(42, "   "));
        record_timestampable_snapshot_rows(&mut row_timestamps, &blank_snapshot, "10:00:00");

        assert!(!row_timestamps.contains_key(&42));

        let content_snapshot = timestamp_test_snapshot(timestamp_test_row(42, "ls"));
        record_timestampable_snapshot_rows(&mut row_timestamps, &content_snapshot, "10:00:01");

        assert_eq!(
            row_timestamps
                .get(&42)
                .map(|timestamp| timestamp.label.as_str()),
            Some("10:00:01")
        );

        let unchanged_snapshot =
            timestamp_test_snapshot(timestamp_test_row_with_cursor(42, "ls", Some(1), true));
        record_timestampable_snapshot_rows(&mut row_timestamps, &unchanged_snapshot, "10:00:02");
        let unchanged_timestamp = row_timestamps.get(&42).expect("timestamped row");
        assert_eq!(unchanged_timestamp.label, "10:00:01");
        assert_eq!(
            unchanged_timestamp.source_signature,
            unchanged_snapshot.lines[0].signature
        );

        let changed_snapshot = timestamp_test_snapshot(timestamp_test_row(42, "pwd"));
        record_timestampable_snapshot_rows(&mut row_timestamps, &changed_snapshot, "10:00:03");
        assert_eq!(
            row_timestamps
                .get(&42)
                .map(|timestamp| timestamp.label.as_str()),
            Some("10:00:03")
        );

        let label = terminal_timestamp_label(1, 2, 3, 4);
        assert_eq!(label, "[01:02:03.004]");
        assert_eq!(label.chars().count(), TERMINAL_TIMESTAMP_LABEL_CELLS);

        let cleared_snapshot = timestamp_test_snapshot(timestamp_test_row(42, ""));
        record_timestampable_snapshot_rows(&mut row_timestamps, &cleared_snapshot, "10:00:04");

        assert!(!row_timestamps.contains_key(&42));
    }

    #[test]
    fn row_timestamp_history_trims_incrementally_and_handles_rewind() {
        let timestamp = |line: u64| TerminalRowTimestamp {
            label: line.to_string(),
            signature: line,
            source_signature: line,
        };
        let mut row_timestamps = (0..6)
            .map(|line| (line, timestamp(line)))
            .collect::<HashMap<_, _>>();
        let mut retained_min_line = None;

        trim_row_timestamp_history(&mut row_timestamps, &mut retained_min_line, 2);
        assert_eq!(retained_min_line, Some(2));
        assert_eq!(row_timestamps.len(), 4);
        assert!(row_timestamps.keys().all(|line| *line >= 2));

        trim_row_timestamp_history(&mut row_timestamps, &mut retained_min_line, 4);
        assert_eq!(retained_min_line, Some(4));
        assert_eq!(row_timestamps.len(), 2);
        assert!(row_timestamps.keys().all(|line| *line >= 4));

        trim_row_timestamp_history(&mut row_timestamps, &mut retained_min_line, 1);
        row_timestamps.insert(1, timestamp(1));
        trim_row_timestamp_history(&mut row_timestamps, &mut retained_min_line, 3);
        assert_eq!(retained_min_line, Some(3));
        assert!(!row_timestamps.contains_key(&1));
    }

    #[gpui::test]
    fn session_highlight_override_wins_over_connection_and_application_rules(
        cx: &mut TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_window, _cx| TerminalTestRoot);
        // Highlight precedence is independent of a live shell and must not start a PTY.
        let pane = cx.update(|window, cx| {
            cx.new(|cx| {
                TerminalPane::new_recording_playback(
                    DEFAULT_COLS,
                    DEFAULT_ROWS,
                    TerminalUiPreferences::default(),
                    window,
                    cx,
                )
                .expect("test terminal pane")
            })
        });
        let rule = |id: &str| TerminalHighlightRule {
            id: id.to_string(),
            pattern: id.to_string(),
            enabled: true,
            ..TerminalHighlightRule::default()
        };
        pane.update(cx, |pane, cx| {
            let mut application = TerminalUiPreferences::default();
            application.highlight_rules = Arc::from([rule("application")]);
            pane.set_preference_overrides(
                TerminalUiPreferenceOverrides {
                    highlight_rules: Some(Arc::from([rule("connection")])),
                    highlight_rule_set_id: Some("connection-set".to_string()),
                    ..TerminalUiPreferenceOverrides::default()
                },
                application.clone(),
                cx,
            );
            pane.set_session_highlight_override(
                Some(TerminalHighlightRuleSetOverride {
                    id: "session-set".to_string(),
                    rules: Arc::from([rule("session")]),
                }),
                application,
                cx,
            );
        });

        pane.read_with(cx, |pane, _cx| {
            assert_eq!(pane.preferences.highlight_rules[0].id, "session");
            assert_eq!(pane.session_highlight_rule_set_id(), Some("session-set"));
        });
    }
}
