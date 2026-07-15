use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    env,
    hash::{Hash, Hasher},
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use chrono::Timelike;
use gpui::{
    Bounds, ClipboardItem, Context, EventEmitter, FocusHandle, PathPromptOptions, Pixels, Point,
    SharedString, Subscription, Timer, Window, px,
};
use oxideterm_ssh::SshConnectionHandle;
use oxideterm_terminal::{
    GraphicsOptions, LocalPtyConfig, SerialControlLine, SerialControlState, SerialDisplayMode,
    SerialLineEnding, SerialRuntimeOptions, SerialSendMode, SerialSessionConfig,
    ShellIntegrationLifecycleState, ShellIntegrationStatus, SshSessionConfig, TelnetSessionConfig,
    TermMode, TerminalCommandMark, TerminalCommandMarkClosedBy, TerminalCommandMarkConfidence,
    TerminalCommandMarkDetectionSource, TerminalCommandMarkEvent,
    TerminalCwdIntegrationLaunchState, TerminalDrainBudget, TerminalDrainReport, TerminalEvent,
    TerminalLifecycle, TerminalOutputProcessor, TerminalProcessInfo, TerminalProcessProbe,
    TerminalRow, TerminalSession, TerminalSessionKind, TerminalSnapshot, TrzszTransferDirection,
    TrzszTransferSelection, serial_list_ports,
};
use oxideterm_trzsz::TrzszState;
use parking_lot::Mutex;
use zeroize::Zeroizing;

use crate::background_cache::BackgroundImageRenderCache;
use crate::command_facts::{
    CommandFactLedger, TerminalAiCommandRecord, TerminalAutosuggestCommandRecord,
    TerminalAutosuggestInputState, TerminalCommandFact,
};
use crate::privilege_prompt::{
    PrivilegeInputObservation, PrivilegePromptSnapshot, PrivilegePromptTracker,
};
use crate::terminal_ui::*;
use crate::terminal_view::*;
use oxideterm_terminal_recording::{
    TerminalRecorder, TerminalRecordingOptions, TerminalRecordingStatus, TerminalRecordingTheme,
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

pub type SharedTerminalSession = Arc<Mutex<TerminalSession>>;
pub type TerminalInputInterceptor =
    Arc<dyn Fn(&[u8]) -> TerminalInputInterceptorResult + Send + Sync>;
const PRIVILEGE_PROMPT_DEBUG_ENV: &str = "OXIDETERM_PRIVILEGE_DEBUG";
const ACTIVE_TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(16);
const BACKGROUND_TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(64);
const IDLE_TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DRAIN_BOOST_POLL_INTERVAL: Duration = Duration::from_millis(8);
const RECENT_TERMINAL_ACTIVITY_WINDOW: Duration = Duration::from_millis(600);
const RECENT_TERMINAL_INPUT_WINDOW: Duration = Duration::from_millis(220);
const ACTIVE_PROCESS_INFO_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalPaneEvent {
    Exited { exit_code: Option<i32> },
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

fn terminal_poll_interval(
    focused: bool,
    drain_budget_exhausted: bool,
    time_since_input: Duration,
    time_since_activity: Duration,
) -> Duration {
    if drain_budget_exhausted {
        return DRAIN_BOOST_POLL_INTERVAL;
    }
    if focused {
        return ACTIVE_TERMINAL_POLL_INTERVAL;
    }
    if time_since_input <= RECENT_TERMINAL_INPUT_WINDOW
        || time_since_activity <= RECENT_TERMINAL_ACTIVITY_WINDOW
    {
        return BACKGROUND_TERMINAL_POLL_INTERVAL;
    }
    IDLE_TERMINAL_POLL_INTERVAL
}

pub struct TerminalPane {
    terminal: Arc<Mutex<TerminalSession>>,
    serial_reconnect_config: Option<SerialSessionConfig>,
    serial_port_available: Option<bool>,
    focus_handle: FocusHandle,
    preferences: TerminalUiPreferences,
    settings: TerminalUiSettings,
    theme: TerminalUiTheme,
    snapshot: TerminalSnapshot,
    snapshot_dirty: bool,
    snapshot_generation: u64,
    terminal_timestamps_enabled: bool,
    // Visual-only metadata keyed by terminal absolute line; never write this
    // into the PTY buffer, copied text, or search/indexed terminal content.
    row_timestamps: HashMap<i64, String>,
    // Tracks the last painted content signature for each stamped row so the
    // gutter follows line modification time instead of first-seen time.
    row_timestamp_signatures: HashMap<i64, u64>,
    metrics: TerminalMetrics,
    selection: Option<TerminalSelection>,
    pending_paste: Option<String>,
    context_menu: Option<TerminalContextMenu>,
    context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence,
    context_action_requested: Option<TerminalContextAction>,
    plugin_input_interceptor: Option<TerminalInputInterceptor>,
    input_locked: bool,
    marked_text: Option<String>,
    privilege_prompt_inline_hint: Option<String>,
    privilege_prompt_submit_requested: bool,
    search_query: Option<String>,
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
    command_marks: Vec<TerminalCommandMark>,
    selected_command_mark_id: Option<String>,
    command_mark_id_aliases: HashMap<String, String>,
    input_tracker: TerminalInputTracker,
    privilege_prompt_tracker: PrivilegePromptTracker,
    command_fact_ledger: CommandFactLedger,
    recorder: Option<TerminalRecorder>,
    bell_flash: bool,
    terminal_exited: bool,
    scroll_remainder_px: Pixels,
    smooth_scroll_animation_active: bool,
    scrollbar_drag: Option<ScrollbarDrag>,
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
    process_info_refresh_in_flight: bool,
    last_process_info_refresh_requested: Instant,
    render_stats: TerminalRenderStats,
    render_stats_window_start: Instant,
    render_stats_window_writes: usize,
    image_cache: ImageRenderCache,
    layout_cache: Arc<Mutex<TerminalLayoutCache>>,
    background_image_cache: BackgroundImageRenderCache,
    bounds: Option<Bounds<Pixels>>,
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
    pub replace_current_command: bool,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalContextAction {
    SendSelectionToAi,
    FillCommandBarFromSelection,
    OpenSearch,
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

const PTY_RESIZE_DEBOUNCE: Duration = Duration::from_millis(100);
const PENDING_CWD_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_COMMAND_MARKS_PER_PANE: usize = 2000;
const COMMAND_MARK_DEDUP_WINDOW_MS: u64 = 2000;
const COMMAND_MARK_DEDUP_LINE_DISTANCE: usize = 2;
static NEXT_TRZSZ_OWNER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_COMMAND_MARK_ID: AtomicU64 = AtomicU64::new(1);

include!("app_recording.rs");
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
        let terminal = Arc::new(Mutex::new(
            TerminalSession::telnet_with_graphics_and_encoding(
                config,
                DEFAULT_COLS,
                DEFAULT_ROWS,
                graphics_options_from_preferences(&preferences),
                preferences.terminal_encoding,
                preferences.scrollback_lines,
            ),
        ));
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_serial_with_preferences(
        config: SerialSessionConfig,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let reconnect_config = config.clone();
        let terminal = Arc::new(Mutex::new(
            TerminalSession::serial_with_graphics_and_encoding(
                config,
                DEFAULT_COLS,
                DEFAULT_ROWS,
                graphics_options_from_preferences(&preferences),
                preferences.terminal_encoding,
                preferences.scrollback_lines,
            )?,
        ));
        let mut pane = Self::from_session(terminal, preferences, window, cx)?;
        pane.serial_reconnect_config = Some(reconnect_config);
        Ok(pane)
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
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
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
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let (snapshot, session_kind, cwd_integration_launch_state) = {
            let terminal = terminal.lock();
            (
                terminal.snapshot().with_generation(1),
                terminal.kind(),
                terminal.cwd_integration_launch_state(),
            )
        };
        let cwd_shell_integration_status = initial_cwd_shell_integration_status(
            preferences.current_directory_awareness_enabled,
            session_kind,
            cwd_integration_launch_state,
        );
        let focus_handle = cx.focus_handle();
        let metrics = TerminalMetrics::measure_with_preferences(window, &preferences);
        window.focus(&focus_handle);
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

        cx.spawn(async move |weak, cx| {
            let mut poll_interval = ACTIVE_TERMINAL_POLL_INTERVAL;
            loop {
                Timer::after(poll_interval).await;
                let Ok(next_poll_interval) = weak.update(cx, |this, cx| {
                    this.tick(cx);
                    this.next_poll_interval()
                }) else {
                    break;
                };
                poll_interval = next_poll_interval;
            }
        })
        .detach();

        Ok(Self {
            terminal,
            serial_reconnect_config: None,
            serial_port_available: None,
            focus_handle,
            preferences: preferences.clone(),
            settings: TerminalUiSettings::from_preferences(&preferences),
            theme: preferences.theme.clone(),
            snapshot,
            snapshot_dirty: false,
            snapshot_generation: 1,
            terminal_timestamps_enabled: false,
            row_timestamps: HashMap::new(),
            row_timestamp_signatures: HashMap::new(),
            metrics,
            selection: None,
            pending_paste: None,
            context_menu: None,
            context_menu_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            context_action_requested: None,
            plugin_input_interceptor: None,
            input_locked: false,
            marked_text: None,
            privilege_prompt_inline_hint: None,
            privilege_prompt_submit_requested: false,
            search_query: None,
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
            command_marks: Vec::new(),
            selected_command_mark_id: None,
            command_mark_id_aliases: HashMap::new(),
            input_tracker: TerminalInputTracker::default(),
            privilege_prompt_tracker: PrivilegePromptTracker::default(),
            command_fact_ledger: CommandFactLedger::default(),
            recorder: None,
            bell_flash: false,
            terminal_exited: false,
            scroll_remainder_px: px(0.0),
            smooth_scroll_animation_active: false,
            scrollbar_drag: None,
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
            process_info_refresh_in_flight: false,
            last_process_info_refresh_requested: Instant::now()
                .checked_sub(ACTIVE_PROCESS_INFO_REFRESH_INTERVAL)
                .unwrap_or_else(Instant::now),
            render_stats: TerminalRenderStats::default(),
            render_stats_window_start: Instant::now(),
            render_stats_window_writes: 0,
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
            bounds: None,
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
            _subscriptions: vec![focus_in, focus_out],
        })
    }

    pub fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn stamp_snapshot(&mut self, mut snapshot: TerminalSnapshot) -> TerminalSnapshot {
        self.record_snapshot_row_timestamps(&snapshot);
        // Raw backend snapshots are stateless; the pane owns frame generation
        // so future render caches can invalidate without changing backends.
        snapshot.reuse_unchanged_rows_from(&self.snapshot);
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
            &mut self.row_timestamps,
            &mut self.row_timestamp_signatures,
            snapshot,
            &label,
        );
        self.trim_row_timestamps(snapshot);
    }

    fn trim_row_timestamps(&mut self, snapshot: &TerminalSnapshot) {
        let Some(max_line) = snapshot.lines.iter().map(|row| row.absolute_line).max() else {
            self.row_timestamps.clear();
            self.row_timestamp_signatures.clear();
            return;
        };
        let retained_rows = self
            .preferences
            .scrollback_lines
            .saturating_add(snapshot.rows)
            .saturating_add(1024)
            .max(2048) as i64;
        let min_line = max_line.saturating_sub(retained_rows);
        self.row_timestamps.retain(|line, _| *line >= min_line);
        self.row_timestamp_signatures
            .retain(|line, _| *line >= min_line);
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

    fn terminal_ghost_text(&self) -> Option<String> {
        self.privilege_prompt_inline_hint
            .clone()
            .or_else(|| self.autosuggest_ghost_text())
    }

    pub fn privilege_prompt_snapshot(&self) -> Option<PrivilegePromptSnapshot> {
        self.privilege_prompt_tracker.snapshot(Instant::now())
    }

    pub fn privilege_prompt_fallback_suppressed(&self) -> bool {
        self.privilege_prompt_tracker
            .suppresses_fallback_prompt_detection(Instant::now())
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
        let next_settings = TerminalUiSettings::from_preferences(&preferences);
        if !next_settings.command_marks_enabled {
            self.command_marks.clear();
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
        self.image_cache
            .set_byte_limit(preferences.render_policy.image_cache_bytes);
        self.background_image_cache
            .set_byte_limit(preferences.render_policy.image_cache_bytes);
        self.preferences = preferences;
        self.last_pty_resize = None;
        self.pending_pty_resize = None;
        self.reset_cursor_blink();
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
    }

    pub fn shutdown(&mut self) {
        self.terminal.lock().shutdown();
    }

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.terminal.lock().lifecycle()
    }

    pub fn is_serial_transport(&self) -> bool {
        self.serial_reconnect_config.is_some()
    }

    pub fn serial_status(&self) -> Option<TerminalSerialStatus> {
        let config = self.serial_reconnect_config.clone()?;
        let terminal = self.terminal.lock();
        Some(TerminalSerialStatus {
            config,
            lifecycle: terminal.lifecycle(),
            control_state: terminal.serial_control_state().unwrap_or_default(),
            runtime_options: terminal.serial_runtime_options().unwrap_or_default(),
            port_available: self.serial_port_available,
            can_reconnect: self.can_reconnect_serial(),
        })
    }

    fn can_reconnect_serial(&self) -> bool {
        self.serial_reconnect_config.is_some() && self.terminal_exited
    }

    fn reconnect_serial(&mut self, cx: &mut Context<Self>) {
        if !self.can_reconnect_serial() {
            return;
        }
        let Some(config) = self.serial_reconnect_config.clone() else {
            return;
        };

        let resize = self
            .last_pty_resize
            .unwrap_or((DEFAULT_COLS, DEFAULT_ROWS, 0, 0));
        let runtime_options = self
            .terminal
            .lock()
            .serial_runtime_options()
            .unwrap_or_default();
        self.terminal.lock().shutdown();

        let mut terminal = match TerminalSession::serial_with_graphics_and_encoding(
            config.clone(),
            resize.0,
            resize.1,
            graphics_options_from_preferences(&self.preferences),
            self.preferences.terminal_encoding,
            self.preferences.scrollback_lines,
        ) {
            Ok(terminal) => terminal,
            Err(error) => {
                self.title = SharedString::from(format!(
                    "{}: {error}",
                    self.preferences.serial_control_labels.reconnect_failed
                ));
                cx.notify();
                return;
            }
        };
        let _ = terminal.set_serial_runtime_options(runtime_options);
        if resize.2 > 0 && resize.3 > 0 {
            let _ = terminal.resize_with_cell_size(resize.0, resize.1, resize.2, resize.3);
        }
        let _ = terminal.set_focused(self.focused);
        let snapshot = terminal.snapshot();

        // Preserve the pane identity while replacing the transport-owned serial handle.
        self.terminal = Arc::new(Mutex::new(terminal));
        self.serial_reconnect_config = Some(config);
        self.serial_port_available = Some(true);
        self.snapshot = self.stamp_snapshot(snapshot);
        self.terminal_exited = false;
        self.input_locked = false;
        self.title = SharedString::from("OxideTerm");
        self.selection = None;
        self.pending_paste = None;
        self.context_menu = None;
        self.context_action_requested = None;
        self.marked_text = None;
        self.privilege_prompt_inline_hint = None;
        self.privilege_prompt_submit_requested = false;
        self.search_query = None;
        self.selected_search_match = None;
        self.hovered_link = None;
        self.hovered_command_mark_id = None;
        self.selecting = false;
        self.last_mouse_report_point = None;
        self.command_marks.clear();
        self.selected_command_mark_id = None;
        self.command_mark_id_aliases.clear();
        self.input_tracker.reset();
        self.privilege_prompt_tracker = PrivilegePromptTracker::default();
        self.command_fact_ledger = CommandFactLedger::default();
        self.last_pty_resize = Some(resize);
        self.pending_pty_resize = None;
        self.last_drain_budget_exhausted = false;
        self.clear_smooth_scroll_remainder();
        self.reset_cursor_blink();
        cx.notify();
    }

    fn refresh_serial_port_presence(&mut self, cx: &mut Context<Self>) {
        let Some(config) = self.serial_reconnect_config.as_ref() else {
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
        self.search_query
            .as_deref()
            .map(|query| self.terminal.lock().search_matches(query).len())
            .unwrap_or_default()
    }

    pub fn copy_to_clipboard(&mut self, cx: &mut Context<Self>) {
        self.copy_from_platform_shortcut(cx);
    }

    pub fn has_selection(&self) -> bool {
        self.selection
            .is_some_and(|selection| !selection.is_empty())
    }

    pub fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }
        let Some(bytes) = self.apply_plugin_input_interceptor(text.as_bytes()) else {
            return;
        };
        let mode = self.terminal.lock().mode();
        self.delete_free_type_selection_if_active(mode, cx);
        let now = Instant::now();
        // Pasted terminal input can include the sudo command while the later
        // prompt is a bare `Password:`. Feed it through the privilege tracker
        // without recording the paste as command history or exposing content.
        self.observe_privilege_input("paste", &bytes, now, cx);
        // Preserve bracketed paste encoding when hook output is still text;
        // binary hook output falls back to raw protocol bytes.
        let result = match std::str::from_utf8(&bytes) {
            Ok(text) => self.terminal.lock().paste_text(text),
            Err(_) => self.terminal.lock().write_protocol_bytes(&bytes),
        };
        if result.is_ok() {
            self.input_tracker.reset();
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
        }
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
        self.send_user_protocol_bytes(bytes, cx);
    }

    pub fn send_privilege_secret_input_bytes(
        &mut self,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) -> bool {
        if bytes.is_empty() || !self.terminal_accepts_input() {
            return false;
        }

        // Privilege Prompt Helper writes an explicitly user-confirmed secret
        // directly to the PTY. It must not pass through plugin interception,
        // autosuggest/history observation, AI context, or terminal recording.
        if self.terminal.lock().write_protocol_bytes(bytes).is_ok() {
            self.privilege_prompt_tracker
                .mark_secret_filled(Instant::now());
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
        self.selection = None;
        self.search_query = None;
        self.selected_search_match = None;
        self.mark_open_command_marks_stale_for_terminal_reset();
        cx.notify();
    }

    pub fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
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
            cx.notify();
            return;
        }
        self.paste_text(&text, cx);
    }

    pub(crate) fn confirm_pending_paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.pending_paste.take() else {
            return;
        };
        self.paste_text(&text, cx);
        cx.notify();
    }

    pub(crate) fn cancel_pending_paste(&mut self, cx: &mut Context<Self>) {
        if self.pending_paste.take().is_some() {
            cx.notify();
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
        }
        let render_stats_changed = self.update_render_stats(&report, now);

        let mut event_effect = TerminalEventEffect::default();
        for event in events {
            event_effect.combine(self.handle_terminal_event(event, cx));
        }

        let cleared_command_mark_selection = self.clear_command_mark_selection_for_tui_mode(mode);
        let mut needs_notify = event_effect.needs_notify || report.changed;
        if (self.preferences.show_performance_overlay && render_stats_changed)
            || cleared_command_mark_selection
        {
            needs_notify = true;
        }
        if self.advance_smooth_scroll_animation() {
            needs_notify = true;
        }
        if self.expire_pending_terminal_cwd(now) {
            needs_notify = true;
        }
        if needs_notify {
            cx.notify();
        }

        self.update_cursor_blink(cx);
        self.request_active_process_info_refresh(cx);
    }

    fn next_poll_interval(&self) -> Duration {
        terminal_poll_interval(
            self.focused,
            self.last_drain_budget_exhausted,
            self.last_terminal_input.elapsed(),
            self.last_terminal_activity.elapsed(),
        )
    }

    fn request_active_process_info_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.focused
            || !self.settings.current_directory_awareness_enabled
            || self.cwd_shell_integration_status == TerminalCwdShellIntegrationStatus::Active
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
        let probe_task = cx
            .background_executor()
            .spawn(async move { probe.collect_current_directory() });
        cx.spawn(async move |weak, cx| {
            let info = probe_task.await;
            let _ = weak.update(cx, |this, cx| {
                this.process_info_refresh_in_flight = false;
                if this.apply_process_info(info) {
                    cx.notify();
                }
            });
        })
        .detach();
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

    fn advance_smooth_scroll_animation(&mut self) -> bool {
        if !self.smooth_scroll_animation_active {
            return false;
        }

        let current = f32::from(self.scroll_remainder_px);
        if current.abs() <= f32::EPSILON {
            self.smooth_scroll_animation_active = false;
            return false;
        }

        // Keep the interpolation short and deterministic. The 16 ms tick loop
        // gives this roughly six frames, enough to reveal clipped text without
        // making wheel scrolling feel laggy.
        let step = (self.metrics.line_height_f32() / 6.0).max(1.0);
        let next = if current > 0.0 {
            (current - step).max(0.0)
        } else {
            (current + step).min(0.0)
        };
        self.scroll_remainder_px = px(next);
        self.smooth_scroll_animation_active = next.abs() > f32::EPSILON;
        true
    }

    fn clear_command_mark_selection_for_tui_mode(&mut self, mode: TermMode) -> bool {
        if self.selected_command_mark_id.is_none() && self.hovered_command_mark_id.is_none()
            || !(mode.contains(TermMode::ALT_SCREEN) || mode.intersects(TermMode::MOUSE_MODE))
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
        let display_offset =
            self.snapshot.display_offset as f32 + f32::from(self.scroll_remainder_px) / line_height;
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
        Self::apply_render_stats_sample(
            &mut self.render_stats,
            tier,
            report.pending_bytes,
            published_writes_per_sec,
        )
    }

    fn apply_render_stats_sample(
        stats: &mut TerminalRenderStats,
        tier: TerminalRenderTier,
        pending_bytes: usize,
        published_writes_per_sec: Option<u32>,
    ) -> bool {
        let previous_stats = *stats;
        stats.tier = tier;
        stats.pending_bytes = pending_bytes;
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
                self.privilege_prompt_tracker
                    .observe_output_bytes(&bytes, Instant::now());
                if let Some(recorder) = self.recorder.as_mut() {
                    recorder.record_output(&bytes);
                }
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
                    Timer::after(Duration::from_millis(180)).await;
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
            TerminalEvent::ShellIntegration(event) => {
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
                    self.command_marks.clear();
                    self.selected_command_mark_id = None;
                    self.hovered_command_mark_id = None;
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
                                self.privilege_prompt_tracker
                                    .observe_submitted_command(command, Instant::now());
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
                TerminalEventEffect::notify()
            }
            TerminalEvent::CwdChanged { cwd, host } => {
                self.cwd = Some(cwd);
                self.cwd_source = Some(TerminalWorkingDirectorySource::ShellIntegration);
                // A prepared startup profile becomes active only after the
                // terminal parser receives a valid directory report.
                self.cwd_shell_integration_status = TerminalCwdShellIntegrationStatus::Active;
                self.pending_cwd = None;
                self.cwd_host = host;
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
        cx.notify();
    }

    fn send_protocol_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }

        if self.terminal.lock().write_protocol_bytes(bytes).is_ok() {
            if let Some(recorder) = self.recorder.as_mut() {
                recorder.record_input(&String::from_utf8_lossy(bytes));
            }
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
        }
    }

    pub(crate) fn send_user_protocol_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }
        let Some(bytes) = self.apply_plugin_input_interceptor(bytes) else {
            return;
        };
        self.observe_user_input("protocol", &bytes, cx);
        self.send_protocol_bytes(&bytes, cx);
    }

    fn send_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }

        if self.terminal.lock().write_text(text).is_ok() {
            if let Some(recorder) = self.recorder.as_mut() {
                recorder.record_input(text);
            }
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
        }
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
        // The autosuggest input tracker owns the current editable command line.
        // Arm sudo/su detection from its completed command on Enter so bare
        // prompts such as macOS `Password:` do not depend on viewport parsing.
        self.privilege_prompt_tracker
            .observe_submitted_command(&command, now);
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
        let observation = self
            .privilege_prompt_tracker
            .observe_user_input_bytes(bytes, now);
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
        let command = self.input_tracker.apply_bytes(bytes)?;
        self.command_fact_ledger
            .record_runtime_autosuggest_command(&command);
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
        !self.input_locked && !self.terminal_exited && self.terminal.lock().is_interactive()
    }

    fn commit_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.marked_text = None;
        if !self.terminal_accepts_input() {
            return;
        }
        let Some(bytes) = self.apply_plugin_input_interceptor(text.as_bytes()) else {
            return;
        };
        let mode = self.terminal.lock().mode();
        self.delete_free_type_selection_if_active(mode, cx);
        self.observe_user_input("text", &bytes, cx);
        self.send_protocol_bytes(&bytes, cx);
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
            Timer::after(PTY_RESIZE_DEBOUNCE).await;
            let _ = weak.update(cx, |view, cx| {
                view.flush_pending_pty_resize(generation, cx);
            });
        })
        .detach();
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
    }
}

fn current_terminal_timestamp_label() -> String {
    let now = chrono::Local::now();
    format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
}

fn record_timestampable_snapshot_rows(
    row_timestamps: &mut HashMap<i64, String>,
    row_timestamp_signatures: &mut HashMap<i64, u64>,
    snapshot: &TerminalSnapshot,
    label: &str,
) {
    for row in &snapshot.lines {
        if terminal_row_has_timestamp_content(row) {
            let timestamp_signature = terminal_row_timestamp_signature(row);
            let line_changed = row_timestamp_signatures.get(&row.absolute_line).copied()
                != Some(timestamp_signature);
            if line_changed {
                row_timestamps.insert(row.absolute_line, label.to_string());
                row_timestamp_signatures.insert(row.absolute_line, timestamp_signature);
            }
        } else {
            // Blank viewport rows are recycled later. Removing their metadata
            // prevents new output from inheriting a stale line-modification time.
            row_timestamps.remove(&row.absolute_line);
            row_timestamp_signatures.remove(&row.absolute_line);
        }
    }
}

fn terminal_row_timestamp_signature(row: &TerminalRow) -> u64 {
    let mut hasher = DefaultHasher::new();
    row.wrapped.hash(&mut hasher);
    for cell in row.cells.iter() {
        cell.ch.hash(&mut hasher);
        cell.zerowidth.hash(&mut hasher);
        cell.wide.hash(&mut hasher);
        cell.fg.hash(&mut hasher);
        cell.bg.hash(&mut hasher);
        cell.attrs.hash(&mut hasher);
        cell.hyperlink.hash(&mut hasher);
    }
    hasher.finish()
}

fn terminal_row_has_timestamp_content(row: &TerminalRow) -> bool {
    row.cells
        .iter()
        .any(|cell| !cell.ch.is_whitespace() || !cell.zerowidth.is_empty())
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

    use oxideterm_terminal::{TerminalAttrs, TerminalCell, TerminalColor, TerminalCursorShape};

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
    fn terminal_polling_keeps_focused_panes_responsive() {
        assert_eq!(
            terminal_poll_interval(
                true,
                false,
                Duration::from_secs(10),
                Duration::from_secs(10),
            ),
            ACTIVE_TERMINAL_POLL_INTERVAL
        );
    }

    #[test]
    fn terminal_polling_reduces_background_and_idle_work() {
        assert_eq!(
            terminal_poll_interval(
                false,
                false,
                Duration::from_secs(10),
                Duration::from_millis(20),
            ),
            BACKGROUND_TERMINAL_POLL_INTERVAL
        );
        assert_eq!(
            terminal_poll_interval(
                false,
                false,
                Duration::from_secs(10),
                Duration::from_secs(10),
            ),
            IDLE_TERMINAL_POLL_INTERVAL
        );
    }

    #[test]
    fn terminal_polling_drains_backpressure_before_other_tiers() {
        assert_eq!(
            terminal_poll_interval(
                false,
                true,
                Duration::from_secs(10),
                Duration::from_secs(10),
            ),
            DRAIN_BOOST_POLL_INTERVAL
        );
    }

    #[test]
    fn osc52_clipboard_read_does_not_touch_clipboard_when_denied() {
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
    }

    #[test]
    fn osc52_clipboard_read_formats_response_when_allowed() {
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
            zerowidth: String::new(),
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            attrs: TerminalAttrs::default(),
            hyperlink: None,
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
    fn terminal_grid_span_reserves_scrollbar_gutter() {
        let cell_width = 10.0;
        let grid_span = terminal_grid_span_for_viewport(px(120.0), cell_width, 0.0);
        let cols = whole_cells_in_span(grid_span, cell_width);
        let scrollbar_right =
            f32::from(terminal_scrollbar_x_for_viewport(px(120.0))) + SCROLLBAR_WIDTH;

        assert_eq!(cols, 11);
        assert!(scrollbar_right <= 120.0);
        assert_eq!(scrollbar_right, 120.0);
    }

    #[test]
    fn terminal_grid_span_keeps_timestamp_gutter_paint_only() {
        let cell_width = 10.0;
        let grid_span = terminal_grid_span_for_viewport(px(160.0), cell_width, 0.0);
        let cols = whole_cells_in_span(grid_span, cell_width);

        assert_eq!(cols, 15);
    }

    #[test]
    fn performance_overlay_requests_redraw_only_when_published_stats_change() {
        let mut stats = TerminalRenderStats::default();

        assert!(!TerminalPane::apply_render_stats_sample(
            &mut stats,
            TerminalRenderTier::Normal,
            0,
            None,
        ));
        assert!(TerminalPane::apply_render_stats_sample(
            &mut stats,
            TerminalRenderTier::Idle,
            0,
            Some(7),
        ));
        assert!(!TerminalPane::apply_render_stats_sample(
            &mut stats,
            TerminalRenderTier::Idle,
            0,
            None,
        ));
    }

    #[test]
    fn row_timestamps_track_last_modified_nonblank_content() {
        let mut row_timestamps = HashMap::new();
        let mut row_timestamp_signatures = HashMap::new();
        let blank_snapshot = timestamp_test_snapshot(timestamp_test_row(42, "   "));
        record_timestampable_snapshot_rows(
            &mut row_timestamps,
            &mut row_timestamp_signatures,
            &blank_snapshot,
            "10:00:00",
        );

        assert!(!row_timestamps.contains_key(&42));
        assert!(!row_timestamp_signatures.contains_key(&42));

        let content_snapshot = timestamp_test_snapshot(timestamp_test_row(42, "ls"));
        record_timestampable_snapshot_rows(
            &mut row_timestamps,
            &mut row_timestamp_signatures,
            &content_snapshot,
            "10:00:01",
        );

        assert_eq!(
            row_timestamps.get(&42).map(String::as_str),
            Some("10:00:01")
        );

        let unchanged_snapshot =
            timestamp_test_snapshot(timestamp_test_row_with_cursor(42, "ls", Some(1), true));
        record_timestampable_snapshot_rows(
            &mut row_timestamps,
            &mut row_timestamp_signatures,
            &unchanged_snapshot,
            "10:00:02",
        );
        assert_eq!(
            row_timestamps.get(&42).map(String::as_str),
            Some("10:00:01")
        );

        let changed_snapshot = timestamp_test_snapshot(timestamp_test_row(42, "pwd"));
        record_timestampable_snapshot_rows(
            &mut row_timestamps,
            &mut row_timestamp_signatures,
            &changed_snapshot,
            "10:00:03",
        );
        assert_eq!(
            row_timestamps.get(&42).map(String::as_str),
            Some("10:00:03")
        );

        let cleared_snapshot = timestamp_test_snapshot(timestamp_test_row(42, ""));
        record_timestampable_snapshot_rows(
            &mut row_timestamps,
            &mut row_timestamp_signatures,
            &cleared_snapshot,
            "10:00:04",
        );

        assert!(!row_timestamps.contains_key(&42));
        assert!(!row_timestamp_signatures.contains_key(&42));
    }
}
