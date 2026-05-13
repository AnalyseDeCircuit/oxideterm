mod actions;
mod connection_monitor;
mod file_manager;
mod forwards;
mod graphics;
mod ide;
mod ime;
mod launcher;
mod new_connection;
mod notification_center;
mod pane_tree;
mod quick_commands;
mod session_manager;
mod settings;
mod sftp;
mod sidebar;
mod tabs;
mod terminal_cast;
mod terminal_command_bar;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use anyhow::Result;
use gpui::{
    AnchoredPositionMode, AnyElement, App, ClipboardItem, Context, Corner, CursorStyle,
    FocusHandle, Focusable, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, ParentElement, PathPromptOptions, Pixels, Render, RenderImage, Rgba,
    ScrollWheelEvent, SharedString, Styled, StyledImage, Subscription, Timer, Window, anchored,
    deferred, div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_backend_classification::{BackendErrorClass, classify_message};
use oxideterm_connection_monitor::{
    ConnectionPoolEntryState, ConnectionPoolEntrySummary, ConnectionPoolMonitorStats,
    MetricsSource, ProfilerRegistry, ProfilerUpdate, ResourceMetrics,
};
use oxideterm_connections::ConnectionStore;
use oxideterm_forwarding::{
    ForwardEvent, ForwardRule, ForwardStatus, ForwardType, ForwardingRegistry, SavedForwardStore,
};
use oxideterm_gpui_ide::IdeSurface;
use oxideterm_gpui_platform::{
    rendering::detect_graphics,
    vibrancy::{NativeVibrancyMode, apply_window_vibrancy},
};
use oxideterm_gpui_terminal::{
    BackgroundImageRenderCache, SharedTerminalSession, TerminalBackgroundFit,
    TerminalBackgroundPreferences, TerminalCommandSelectionLabels, TerminalHighlightRenderMode,
    TerminalHighlightRule as UiHighlightRule, TerminalNotice, TerminalNoticeVariant, TerminalPane,
    TerminalPasteLabels, TerminalRecordingState, TerminalRecordingStatus, TerminalTrzszLabels,
    TerminalUiPreferences, TerminalUiTheme,
};
use oxideterm_gpui_ui::{
    modal::popover_backdrop,
    toast::{ToastVariant, ToastView},
    toaster::toaster,
    tooltip::tooltip_content,
};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_notification_center::{
    ActivityView as WorkspaceActivityView, EventCategory as WorkspaceEventCategory,
    EventCategoryFilter as WorkspaceEventCategoryFilter, EventLogEntry as WorkspaceEventLogEntry,
    EventSeverity as WorkspaceEventSeverity, EventSeverityFilter as WorkspaceEventSeverityFilter,
    NotificationCenterState, NotificationEntry as WorkspaceNotificationEntry,
    NotificationKind as WorkspaceNotificationKind,
    NotificationKindFilter as WorkspaceNotificationKindFilter,
    NotificationScope as WorkspaceNotificationScope,
    NotificationSeverity as WorkspaceNotificationSeverity,
    NotificationSeverityFilter as WorkspaceNotificationSeverityFilter,
    NotificationStatus as WorkspaceNotificationStatus,
    NotificationStatusFilter as WorkspaceNotificationStatusFilter,
};
use oxideterm_render_policy::{
    DetectedGraphics, EffectiveRenderPolicy, RenderProfile, compute_render_policy,
};
use oxideterm_settings::{
    BackgroundFit, CursorStyle as SettingsCursorStyle, FrostedGlassMode, HighlightRuleRenderMode,
    Language, PersistedSettings, SettingsStore, TerminalEncoding as SettingsTerminalEncoding,
    default_settings_path,
};
use oxideterm_sftp::{
    DummyProgressStore, ProgressStore, RedbProgressStore, SftpTransferManager,
    SftpTransferRuntimeSettings, StoredTransferProgress,
};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, ConnectionPoolConfig, ConnectionState,
    MAX_RETAINED_RECONNECT_JOBS, NodeId, NodeOrigin, NodeReadiness, NodeRouter, NodeRuntimeStore,
    NodeState, NodeStateEvent, NodeTreeExpansion, NodeTreeSnapshot, NodeTreeSnapshotNode,
    PhaseResult, ProbeConnectionStatus, ProxyHopConfig, ReconnectForwardRule,
    ReconnectForwardRuleSnapshot, ReconnectJob, ReconnectNodeConnectionSnapshot,
    ReconnectNodeTerminalSnapshot, ReconnectNodeTransferSnapshot, ReconnectOrchestratorStore,
    ReconnectPhase, ReconnectSnapshot, ReconnectTiming, SshConfig, SshConnectionRegistry,
    SshTransportClient, TerminalEndpoint,
};
use oxideterm_terminal::TerminalCommandMarkDetectionSource;
use oxideterm_terminal::{
    LocalPtyConfig, ShellInfo, SshSessionConfig, TerminalCursorShape,
    TerminalEncoding as SessionTerminalEncoding, TerminalLifecycle, scan_shells,
};
use oxideterm_theme::{
    AppUiColors, TerminalTheme, ThemeTokens, UiRadii, derive_ui_colors_from_terminal, theme_by_id,
};
use oxideterm_workspace::{
    ActiveSessionNode, ActiveSessionReadiness, ActiveSessionStatus, MAX_PANES_PER_TAB, PaneId,
    PaneNode, SplitDirection, Tab, TabId, TabKind, TabTitleSource, TerminalSessionId,
    adjusted_split_sizes, balanced_sizes, sort_active_session_nodes,
};

use self::actions::SearchBarState;
use self::connection_monitor::ConnectionMonitorState;
use self::file_manager::FileManagerState;
use self::graphics::GraphicsState;
use self::ime::{WorkspaceImeElement, keystroke_commits_platform_text};
use self::launcher::LauncherState;
use self::new_connection::{
    HostKeyChallenge, KeyboardInteractiveChallenge, NativeSshPromptHandler, NewConnectionForm,
    NewConnectionSelect, SavedConnectionPromptAction, SshAuthTab, SshConnectionWorkerResult,
};
use self::pane_tree::SplitDrag;
use self::quick_commands::QuickCommandsState;
use self::session_manager::{AutoRouteModalState, SessionManagerState};
use self::settings::ThemeEditorState;
use self::sidebar::SidebarSection;
use self::terminal_cast::TerminalCastPlayerState;
use crate::assets::LucideIcon;
use crate::{
    ClosePane, CloseSearch, CloseTab, Copy, Find, FindNext, FindPrev, GoToTab1, GoToTab2, GoToTab3,
    GoToTab4, GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewTerminal, NextTab, OpenSettings,
    Paste, PrevTab, SplitHorizontal, SplitVertical, SwitchLocaleChinese, SwitchLocaleEnglish,
    SwitchLocaleFrench, SwitchLocaleGerman, SwitchLocaleItalian, SwitchLocaleJapanese,
    SwitchLocaleKorean, SwitchLocalePortugueseBrazil, SwitchLocaleSpanish,
    SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese,
};
use oxideterm_gpui_settings_view::{
    ActiveSurface, SettingsInput, SettingsSelect, SettingsSlider, SettingsTab, TerminalSettingsPage,
};
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId, select_anchor_probe};
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputAnchorId};
use oxideterm_gpui_ui::typography::{
    css_font_family_head as settings_css_font_family_head, gpui_font_family_name,
    tauri_ui_font_family as settings_ui_font_family,
};

pub(crate) struct WorkspaceApp {
    focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    active_tab_id: Option<TabId>,
    panes: HashMap<PaneId, gpui::Entity<TerminalPane>>,
    tab_scroll_x: f32,
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    search: SearchBarState,
    terminal_command_bar_focused: bool,
    terminal_command_bar_draft: String,
    terminal_broadcast_enabled: bool,
    terminal_broadcast_targets: HashSet<PaneId>,
    terminal_broadcast_menu_open: bool,
    terminal_quick_commands_open: bool,
    terminal_quick_command_pending: Option<String>,
    terminal_cast_player: Option<TerminalCastPlayerState>,
    terminal_cast_seek_dragging: bool,
    quick_commands: QuickCommandsState,
    split_drag: Option<SplitDrag>,
    sidebar_resizing: bool,
    sidebar_collapsed: bool,
    sidebar_width: f32,
    needs_active_pane_focus: bool,
    active_sidebar_section: SidebarSection,
    active_surface: ActiveSurface,
    active_settings_tab: SettingsTab,
    terminal_settings_page: TerminalSettingsPage,
    open_settings_select: Option<SettingsSelect>,
    select_anchors: HashMap<SelectAnchorId, OverlayAnchor>,
    text_input_anchors: HashMap<TextInputAnchorId, TextInputAnchor>,
    ime_marked_text: Option<String>,
    focused_settings_input: Option<SettingsInput>,
    settings_input_draft: String,
    settings_slider_drag: Option<SettingsSlider>,
    theme_editor: Option<ThemeEditorState>,
    background_blur_preview: Option<i64>,
    background_blur_commit_generation: u64,
    background_cache_poll_scheduled: bool,
    new_connection_form: Option<NewConnectionForm>,
    drill_down_parent_node_id: Option<NodeId>,
    editing_saved_connection_id: Option<String>,
    saved_connection_prompt_action: Option<SavedConnectionPromptAction>,
    open_new_connection_select: Option<NewConnectionSelect>,
    new_connection_caret_visible: bool,
    host_key_challenge: Option<HostKeyChallenge>,
    keyboard_interactive_challenge: Option<KeyboardInteractiveChallenge>,
    ssh_worker_tx: std::sync::mpsc::Sender<SshConnectionWorkerResult>,
    ssh_worker_rx: std::sync::mpsc::Receiver<SshConnectionWorkerResult>,
    ssh_registry: SshConnectionRegistry,
    forwarding_registry: ForwardingRegistry,
    forwarding_runtime: Arc<tokio::runtime::Runtime>,
    wsl_graphics: Arc<oxideterm_wsl_graphics::WslGraphicsState>,
    forwarding_connection_consumers: HashMap<String, (String, ConnectionConsumer)>,
    sftp_connection_consumers: HashMap<String, (String, ConnectionConsumer)>,
    sftp_transfer_manager: Arc<SftpTransferManager>,
    sftp_progress_store: Arc<dyn ProgressStore>,
    node_runtime_store: NodeRuntimeStore,
    node_router: NodeRouter,
    node_event_tx: std::sync::mpsc::Sender<NodeStateEvent>,
    node_event_rx: std::sync::mpsc::Receiver<NodeStateEvent>,
    node_event_generations: HashMap<NodeId, u64>,
    reconnect_orchestrator: ReconnectOrchestratorStore,
    reconnect_worker_tx: std::sync::mpsc::Sender<ReconnectWorkerResult>,
    reconnect_worker_rx: std::sync::mpsc::Receiver<ReconnectWorkerResult>,
    pending_reconnect_node_ids: HashSet<NodeId>,
    reconnect_debounce_scheduled: bool,
    reconnect_debounce_generation: u64,
    reconnect_pipeline_active_node: Option<NodeId>,
    reconnect_requeue_counts: HashMap<NodeId, u32>,
    active_connection_chain: Option<ConnectionChainRun>,
    connecting_node_locks: HashSet<NodeId>,
    pending_reconnect_cascade_nodes: VecDeque<NodeId>,
    last_ssh_active_probe_at: Option<Instant>,
    ssh_active_probe_in_flight: bool,
    pending_reconnect_transfer_resumes: HashMap<NodeId, HashSet<String>>,
    reconnect_transfer_resume_totals: HashMap<NodeId, usize>,
    reconnect_transfer_resume_successes: HashMap<NodeId, usize>,
    pending_ide_restore_transfer_counts: HashMap<NodeId, u32>,
    reconnect_forward_restore_totals: HashMap<NodeId, u32>,
    reconnect_forward_restore_tokens: HashMap<NodeId, Arc<AtomicBool>>,
    notification_center: NotificationCenterState,
    terminal_endpoint_sessions: HashMap<TerminalSessionId, WorkspaceTerminalEndpointSession>,
    ssh_nodes: HashMap<NodeId, WorkspaceSshNode>,
    saved_ssh_nodes: HashMap<String, NodeId>,
    terminal_ssh_nodes: HashMap<TerminalSessionId, NodeId>,
    pending_ssh_terminal_opens: VecDeque<PendingSshTerminalOpen>,
    expanded_ssh_nodes: HashSet<NodeId>,
    active_ssh_node_id: Option<NodeId>,
    next_ssh_node_id: u64,
    forward_tab_nodes: HashMap<TabId, NodeId>,
    forwarding_view: forwards::ForwardsViewState,
    forwarding_port_detection_by_node: HashMap<NodeId, forwards::PortDetectionViewState>,
    forwarding_port_profiler_nodes: HashSet<NodeId>,
    file_manager: FileManagerState,
    sftp_tab_nodes: HashMap<TabId, NodeId>,
    sftp_view_node: Option<NodeId>,
    sftp_local_path_memory: HashMap<NodeId, String>,
    sftp_path_memory: HashMap<NodeId, String>,
    sftp_remote_home_by_node: HashMap<NodeId, String>,
    ide_tab_surfaces: HashMap<TabId, gpui::Entity<IdeSurface>>,
    ide_surface_subscriptions: HashMap<TabId, Subscription>,
    ide_tab_nodes: HashMap<TabId, NodeId>,
    ide_last_closed_at_by_node: HashMap<NodeId, SystemTime>,
    sftp_view: sftp::SftpViewState,
    launcher: LauncherState,
    graphics: GraphicsState,
    connection_monitor: ConnectionMonitorState,
    sftp_worker_tx: std::sync::mpsc::Sender<sftp::SftpWorkerResult>,
    sftp_worker_rx: std::sync::mpsc::Receiver<sftp::SftpWorkerResult>,
    forwarding_worker_tx: std::sync::mpsc::Sender<forwards::ForwardingWorkerResult>,
    forwarding_worker_rx: std::sync::mpsc::Receiver<forwards::ForwardingWorkerResult>,
    forwarding_event_rx: std::sync::mpsc::Receiver<ForwardEvent>,
    i18n: I18n,
    tokens: ThemeTokens,
    detected_graphics: DetectedGraphics,
    render_profile_override: Option<RenderProfile>,
    render_policy: EffectiveRenderPolicy,
    applied_vibrancy_mode: NativeVibrancyMode,
    background_image_cache: BackgroundImageRenderCache,
    settings_store: SettingsStore,
    connection_store: ConnectionStore,
    session_manager: SessionManagerState,
    auto_route_modal: AutoRouteModalState,
    settings_connection_new_group: String,
    settings_selected_ssh_hosts: HashSet<String>,
    settings_connection_status: Option<String>,
    local_shells: Vec<ShellInfo>,
    terminal_notice_tx: std::sync::mpsc::Sender<TerminalNotice>,
    terminal_notice_rx: std::sync::mpsc::Receiver<TerminalNotice>,
    workspace_toasts: Vec<WorkspaceToast>,
    connection_trace_tx: std::sync::mpsc::Sender<ConnectionTraceEvent>,
    connection_trace_rx: std::sync::mpsc::Receiver<ConnectionTraceEvent>,
    connection_trace_toasts: HashMap<String, ActiveConnectionTrace>,
    connection_trace_nodes: HashMap<NodeId, ConnectionTraceNodeContext>,
    connection_trace_attempt_seq: u64,
    workspace_tooltip: Option<WorkspaceTooltip>,
    workspace_tooltip_pending: Option<WorkspaceTooltipPending>,
    workspace_tooltip_generation: u64,
}

#[derive(Clone, Debug)]
struct WorkspaceToast {
    notice: TerminalNotice,
    expires_at: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionTraceStage {
    Queued,
    Preparing,
    OpeningTransport,
    SshHandshake,
    HostKey,
    Authentication,
    Pty,
    ShellReady,
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionTraceStatus {
    Running,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionTraceMode {
    Connect,
    Reconnect,
}

#[derive(Clone, Debug)]
struct ConnectionTraceEvent {
    attempt_id: String,
    node_id: NodeId,
    stage: ConnectionTraceStage,
    status: ConnectionTraceStatus,
    progress: f32,
    elapsed_ms: u64,
    detail: Option<String>,
    label: Option<String>,
    step_index: Option<u32>,
    total_steps: Option<u32>,
    mode: ConnectionTraceMode,
}

#[derive(Clone, Debug)]
struct ActiveConnectionTrace {
    visible: bool,
    latest: ConnectionTraceEvent,
    displayed: Option<ConnectionTraceEvent>,
    started_at: Instant,
    show_generation: u64,
    flush_generation: u64,
    expires_at: Option<Instant>,
}

#[derive(Clone, Debug)]
struct ConnectionTraceNodeContext {
    attempt_id: String,
    label: Option<String>,
    step_index: Option<u32>,
    total_steps: Option<u32>,
    mode: ConnectionTraceMode,
}

#[derive(Clone, Debug)]
struct ConnectionTracePlan {
    attempt_id: String,
    mode: ConnectionTraceMode,
    node_ids: Vec<NodeId>,
}

#[derive(Clone, Debug)]
struct ConnectionChainRun {
    node_ids: Vec<NodeId>,
    next_index: usize,
    trace_plan: ConnectionTracePlan,
}

#[derive(Clone, Debug)]
struct WorkspaceTooltip {
    label: String,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug)]
struct WorkspaceTooltipPending {
    id: String,
    label: String,
    x: f32,
    y: f32,
    generation: u64,
}

#[derive(Clone)]
struct WorkspaceTerminalEndpointSession {
    endpoint: TerminalEndpoint,
    session: SharedTerminalSession,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedNodeTreeSnapshot {
    version: u32,
    exported_at_ms: u64,
    root_ids: Vec<NodeId>,
    nodes: Vec<PersistedNodeTreeNode>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedNodeTreeNode {
    id: NodeId,
    parent_id: Option<NodeId>,
    children_ids: Vec<NodeId>,
    depth: u32,
    origin: NodeOrigin,
    config: Option<SshConfig>,
    created_at_ms: u64,
    generation: u64,
}

// Root workspace pieces are included from here to preserve private access
// across the Tauri-port surface while shrinking the previous 1k-line module.
include!("workspace/root/state.rs");
include!("workspace/root/init.rs");
include!("workspace/root/helpers.rs");
include!("workspace/root/render.rs");
include!("workspace/root/background.rs");
#[cfg(test)]
include!("workspace/root/tests.rs");
