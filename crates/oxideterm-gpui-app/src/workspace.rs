mod actions;
mod command_palette;
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
use oxideterm_ide_fs::{NodeAgentIdeFileSystem, NodeAgentRpcError};
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
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, DummyProgressStore, ProgressStore, RedbProgressStore,
    SftpTransferGuard, SftpTransferManager, SftpTransferRuntimeSettings, StoredTransferProgress,
    TransferStrategy, probe_tar_compression, probe_tar_support, tar_download_directory,
    tar_upload_directory,
};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, ConnectionPoolConfig, ConnectionState,
    MAX_RETAINED_RECONNECT_JOBS, NodeId, NodeOrigin, NodeReadiness, NodeRouter, NodeRuntimeStore,
    NodeState, NodeStateEvent, NodeTreeExpansion, NodeTreeSnapshot, NodeTreeSnapshotNode,
    PhaseResult, ProbeConnectionStatus, ProxyHopConfig, ReconnectForwardRule,
    ReconnectForwardRuleSnapshot, ReconnectJob, ReconnectNodeConnectionSnapshot,
    ReconnectNodeTerminalSnapshot, ReconnectNodeTransferSnapshot, ReconnectOrchestratorStore,
    ReconnectPhase, ReconnectSnapshot, ReconnectTiming, SshConfig, SshConnectionHandle,
    SshConnectionRegistry, SshTransportClient, TerminalEndpoint,
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
use self::settings::AiModelRefreshDelivery;
use self::settings::ThemeEditorState;
use self::sidebar::SidebarSection;
use self::sidebar::{
    AiCompactionDelivery, AiModelSelectorProbeDelivery, AiPendingChatStream, AiStreamDelivery,
};
use self::terminal_cast::TerminalCastPlayerState;
use crate::assets::LucideIcon;
use crate::{
    CloseOtherTabs, ClosePane, CloseSearch, CloseTab, CommandPalette, Copy, Find, FindNext,
    FindPrev, FontDecrease, FontIncrease, FontReset, GoToTab1, GoToTab2, GoToTab3, GoToTab4,
    GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewConnection, NewTerminal, NextTab,
    OpenSettings, PaletteAiSidebar, PaletteBroadcast, PaletteEventLog, Paste, PrevTab,
    ShellLauncher, ShowShortcuts, SplitHorizontal, SplitNavLeft, SplitNavRight, SplitVertical,
    SwitchLocaleChinese, SwitchLocaleEnglish, SwitchLocaleFrench, SwitchLocaleGerman,
    SwitchLocaleItalian, SwitchLocaleJapanese, SwitchLocaleKorean, SwitchLocalePortugueseBrazil,
    SwitchLocaleSpanish, SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese, TerminalAiPanel,
    TerminalRecording, ToggleSidebar, ZenMode,
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

#[derive(Clone, Debug)]
struct AiMcpServerDraft {
    name: String,
    transport: oxideterm_ai::McpTransport,
    command: String,
    args: String,
    env: Vec<(String, String)>,
    url: String,
    auth_header_name: String,
    auth_header_mode: oxideterm_ai::McpAuthHeaderMode,
    auth_token: String,
    headers: Vec<(String, String)>,
    retry_on_disconnect: bool,
    show_auth_token: bool,
}

#[derive(Clone, Debug)]
enum KnowledgeDeleteTarget {
    Collection,
    Document,
}

#[derive(Clone, Debug)]
struct KnowledgeDeleteConfirm {
    target: KnowledgeDeleteTarget,
    id: String,
    name: String,
}

#[derive(Clone, Debug)]
struct KnowledgeExternalEdit {
    doc_id: String,
    path: PathBuf,
    version: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeybindingScopeFilter {
    All,
    Scope(crate::keybindings::ActionScope),
}

impl KeybindingScopeFilter {
    fn matches(self, scope: crate::keybindings::ActionScope) -> bool {
        match self {
            Self::All => true,
            Self::Scope(candidate) => candidate == scope,
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::All => "settings_view.keybindings.scope_all",
            Self::Scope(scope) => scope.label_key(),
        }
    }
}

enum KnowledgeReindexDelivery {
    Progress { current: usize, total: usize },
    Finished(Result<usize, String>),
}

#[derive(Clone, Debug)]
struct CommandPaletteState {
    open: bool,
    query: String,
    selected_index: usize,
}

#[derive(Clone, Debug)]
struct ShortcutsModalState {
    open: bool,
    query: String,
}

impl Default for AiMcpServerDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            transport: oxideterm_ai::McpTransport::Stdio,
            command: String::new(),
            args: String::new(),
            env: Vec::new(),
            url: String::new(),
            auth_header_name: "Authorization".to_string(),
            auth_header_mode: oxideterm_ai::McpAuthHeaderMode::Bearer,
            auth_token: String::new(),
            headers: Vec::new(),
            retry_on_disconnect: false,
            show_auth_token: false,
        }
    }
}

pub(crate) struct WorkspaceApp {
    focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    active_tab_id: Option<TabId>,
    tab_navigation_history: Vec<TabId>,
    tab_navigation_index: Option<usize>,
    tab_navigation_replaying: bool,
    tab_navigation_observed_tab: Option<TabId>,
    panes: HashMap<PaneId, gpui::Entity<TerminalPane>>,
    tab_scroll_x: f32,
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    search: SearchBarState,
    terminal_command_bar_focused: bool,
    terminal_command_bar_draft: String,
    terminal_command_suggestions_open: bool,
    terminal_command_suggestion_highlighted: Option<usize>,
    terminal_broadcast_enabled: bool,
    terminal_broadcast_targets: HashSet<PaneId>,
    terminal_broadcast_menu_open: bool,
    terminal_quick_commands_open: bool,
    terminal_quick_command_pending: Option<String>,
    terminal_cast_player: Option<TerminalCastPlayerState>,
    terminal_cast_seek_dragging: bool,
    command_palette: CommandPaletteState,
    shortcuts_modal: ShortcutsModalState,
    quick_commands: QuickCommandsState,
    split_drag: Option<SplitDrag>,
    sidebar_resizing: bool,
    sidebar_collapsed: bool,
    sidebar_width: f32,
    ai_sidebar_resizing: bool,
    ai_sidebar_width: f32,
    ai_overlay_window_size: Option<(f32, f32)>,
    ai_overlay_window_bounds_subscription: Option<Subscription>,
    knowledge_window_activation_subscription: Option<Subscription>,
    needs_active_pane_focus: bool,
    active_sidebar_section: SidebarSection,
    active_surface: ActiveSurface,
    active_settings_tab: SettingsTab,
    terminal_settings_page: TerminalSettingsPage,
    open_settings_select: Option<SettingsSelect>,
    ai_new_provider_type: String,
    ai_provider_settings_expanded: bool,
    ai_tool_use_expanded: bool,
    ai_context_windows_expanded: bool,
    ai_model_reasoning_expanded: bool,
    expanded_ai_providers: HashSet<String>,
    expanded_ai_provider_models: HashSet<String>,
    expanded_ai_context_providers: HashSet<String>,
    expanded_ai_model_reasoning_providers: HashSet<String>,
    ai_model_selector_open: bool,
    ai_model_selector_search_focused: bool,
    ai_model_selector_search_query: String,
    ai_model_selector_expanded_providers: HashSet<String>,
    ai_model_selector_provider_online: HashMap<String, bool>,
    ai_model_selector_probe_generations: HashMap<String, u64>,
    ai_chat: oxideterm_ai::AiChatState,
    ai_chat_store: oxideterm_ai::AiChatPersistenceStore,
    ai_runtime_epoch: String,
    ai_command_record_sequence: u64,
    ai_command_records: VecDeque<AiRuntimeCommandRecord>,
    ai_cli_agent_sessions: HashMap<String, AiCliAgentSession>,
    ai_conversation_list_open: bool,
    ai_chat_menu_open: bool,
    ai_profile_selector_open: bool,
    ai_safety_menu_open: bool,
    ai_safety_confirm_open: bool,
    ai_summarize_confirm_open: bool,
    ai_clear_all_confirm_open: bool,
    ai_delete_message_confirm: Option<String>,
    ai_safety_bypass_conversations: HashSet<String>,
    ai_chat_draft: String,
    ai_chat_input_focused: bool,
    ai_editing_message_id: Option<String>,
    ai_editing_message_draft: String,
    ai_editing_message_focused: bool,
    ai_thinking_expansion_state: HashMap<String, bool>,
    ai_tool_call_expansion_state: HashSet<String>,
    ai_chat_autocomplete_index: usize,
    ai_chat_autocomplete_suppressed: bool,
    ai_context_popover_open: bool,
    ai_model_switch_warning_percentage: Option<usize>,
    ai_context_trim_notice_count: Option<usize>,
    ai_context_trim_notice_sequence: u64,
    ai_chat_include_context: bool,
    ai_chat_include_all_panes: bool,
    ai_chat_loading: bool,
    ai_chat_stream_generation: u64,
    ai_chat_stream_task: Option<tokio::task::JoinHandle<()>>,
    ai_chat_stream_rx: Option<std::sync::mpsc::Receiver<AiStreamDelivery>>,
    ai_chat_stream_polling: bool,
    ai_pending_tool_approvals: HashMap<String, tokio::sync::oneshot::Sender<bool>>,
    ai_agent_fs: NodeAgentIdeFileSystem,
    ai_mcp_registry: oxideterm_ai::McpRegistry,
    ai_rag_store: Arc<oxideterm_ai::RagStore>,
    ai_mcp_add_dialog: Option<AiMcpServerDraft>,
    knowledge_selected_collection_id: Option<String>,
    knowledge_create_dialog_open: bool,
    knowledge_new_document_dialog_open: bool,
    knowledge_embedding_config_expanded: bool,
    knowledge_new_collection_name: String,
    knowledge_new_document_title: String,
    knowledge_new_document_format: String,
    knowledge_import_progress: Option<(usize, usize)>,
    knowledge_embedding_progress: Option<(usize, usize)>,
    knowledge_reindex_progress: Option<(usize, usize)>,
    knowledge_reindex_cancel: Option<Arc<AtomicBool>>,
    knowledge_reindex_rx: Option<std::sync::mpsc::Receiver<KnowledgeReindexDelivery>>,
    knowledge_reindex_polling: bool,
    knowledge_delete_confirm: Option<KnowledgeDeleteConfirm>,
    knowledge_external_edit: Option<KnowledgeExternalEdit>,
    knowledge_error: Option<String>,
    ai_compaction_rx: Option<std::sync::mpsc::Receiver<AiCompactionDelivery>>,
    ai_compaction_polling: bool,
    ai_compacting_conversations: HashSet<String>,
    ai_pending_chat_after_compaction: Option<AiPendingChatStream>,
    next_ai_chat_sequence: u64,
    ai_key_store: oxideterm_ai::AiProviderKeyStore,
    ai_provider_key_status: HashMap<String, bool>,
    ai_model_refresh_generations: HashMap<String, u64>,
    ai_model_refreshing: HashSet<String>,
    ai_model_refresh_tx: Option<std::sync::mpsc::Sender<AiModelRefreshDelivery>>,
    ai_model_refresh_rx: Option<std::sync::mpsc::Receiver<AiModelRefreshDelivery>>,
    ai_model_refresh_polling: bool,
    ai_model_refresh_pending: usize,
    next_ai_model_refresh_generation: u64,
    next_ai_model_selector_probe_generation: u64,
    ai_model_selector_probe_rx: Option<std::sync::mpsc::Receiver<AiModelSelectorProbeDelivery>>,
    ai_model_selector_probe_tx: Option<std::sync::mpsc::Sender<AiModelSelectorProbeDelivery>>,
    ai_model_selector_probe_polling: bool,
    ai_model_selector_probe_pending: usize,
    show_ai_enable_confirm: bool,
    ai_provider_key_remove_confirm: Option<(usize, String)>,
    select_anchors: HashMap<SelectAnchorId, OverlayAnchor>,
    text_input_anchors: HashMap<TextInputAnchorId, TextInputAnchor>,
    ime_marked_text: Option<String>,
    focused_settings_input: Option<SettingsInput>,
    settings_input_draft: String,
    settings_slider_drag: Option<SettingsSlider>,
    keybinding_recording_action_id: Option<String>,
    keybinding_recording_combo: Option<crate::keybindings::KeyCombo>,
    keybinding_conflict_action_ids: Vec<String>,
    keybinding_search_query: String,
    keybinding_scope_filter: KeybindingScopeFilter,
    keybinding_reset_all_confirm_open: bool,
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
struct TerminalCommandSuggestion {
    kind: TerminalCommandSuggestionKind,
    label: String,
    insert_text: String,
    description: Option<String>,
    executable: bool,
    replacement: std::ops::Range<usize>,
    group_label_key: &'static str,
    source_label_key: &'static str,
    score: f64,
    risk: Option<&'static str>,
    inline_safe: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSuggestionKind {
    History,
    Command,
    Subcommand,
    Option,
    File,
    Directory,
    QuickCommand,
}

#[derive(Clone, Debug)]
pub(crate) struct AiRuntimeCommandRecord {
    pub(crate) command_id: String,
    pub(crate) target_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) command: String,
    pub(crate) cwd: Option<String>,
    pub(crate) source: String,
    pub(crate) status: String,
    pub(crate) exit_code: Option<i64>,
    pub(crate) started_at: i64,
    pub(crate) finished_at: Option<i64>,
    pub(crate) runtime_epoch: String,
    pub(crate) risk: String,
}

#[derive(Clone, Debug)]
pub(crate) struct AiCliAgentSession {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) status: String,
    pub(crate) target_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) command: String,
    pub(crate) started_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) runtime_epoch: String,
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
