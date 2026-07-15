mod actions;
mod ai_lazy;
mod ai_state;
mod browser_behavior;
mod cloud_sync;
mod command_palette;
mod connection_monitor;
mod desktop_presence;
mod detached_tab_window;
mod file_manager;
mod forwards;
mod graphics;
mod graphics_vnc;
mod ide;
mod ime;
mod launcher;
mod local_shell_launcher;
mod local_terminal_background;
mod new_connection;
mod notification_center;
mod onboarding;
mod pane_tree;
mod plugin_host;
mod plugin_lifecycle;
mod plugin_manager;
mod plugin_runtime;
mod plugin_settings_store;
mod plugin_ui;
mod quick_commands;
mod remote_desktop;
mod root {
    pub(super) mod background;
    pub(super) mod helpers;
    pub(super) mod init;
    pub(super) mod render;
    pub(super) mod state;
    #[cfg(test)]
    pub(super) mod tests;
}
mod selectable_text;
mod selection_motion;
mod session_icons;
mod session_manager;
mod settings;
mod sftp;
mod sidebar;
mod single_instance;
mod tabs;
mod terminal_cast;
mod terminal_command_bar;
mod terminal_context_actions;
mod terminal_cwd;
mod terminal_git;
mod terminal_project;
mod version_migration;
mod virtual_list;

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use self::{
    ai_lazy::LazyAiRagStore,
    settings::SettingsManagedKeyDialog,
    sidebar::{ContextSidebarPanel, ContextSidebarTool},
    version_migration::VersionMigrationState,
};
use anyhow::Result;
use gpui::{
    AnchoredPositionMode, Animation, AnimationExt, AnyElement, App, Bounds, ClipboardEntry,
    ClipboardItem, Context, Corner, CursorStyle, Entity, FocusHandle, Focusable, Image,
    ImageFormat, IntoElement, KeyDownEvent, KeyUpEvent, ListAlignment, ListState,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    ParentElement, PathPromptOptions, Pixels, Point, Render, RenderImage, Rgba, ScrollHandle,
    ScrollWheelEvent, SharedString, Styled, StyledImage, Subscription, TextLayout, Timer,
    UniformListScrollHandle, WeakEntity, Window, anchored, deferred, div, prelude::*, px, relative,
    rgb, rgba, svg,
};
use oxideterm_connection_monitor::{
    CompactMonitorRow, ConnectionPoolEntryState, ConnectionPoolEntrySummary,
    ConnectionPoolMonitorStats, DockerActionKind, FilesystemCommandCapability,
    FilesystemEntrySeverity, FilesystemFilter, HostToolActionOutcome, LogCommandCapability,
    LogPreset, MetricsSource, MonitorListRow, MonitorMetricKind, MonitorSectionKind,
    MonitorValueLevel, PackageCommandCapability, PackageFilter, PortCommandCapability, PortFilter,
    ProcessActionKind, ProcessCommandCapability, ProcessFilter, ProcessSort, ProfilerRegistry,
    ProfilerUpdate, ResourceDockerContainer, ResourceDockerStatus, ResourceFilesystemEntry,
    ResourceFilesystemSnapshot, ResourceFilesystemStatus, ResourceLogEntry, ResourceLogSnapshot,
    ResourceLogStatus, ResourceMetrics, ResourcePackageEntry, ResourcePackageSnapshot,
    ResourcePackageStatus, ResourcePortEntry, ResourcePortSnapshot, ResourcePortStatus,
    ResourceScheduledTask, ResourceScheduledTaskSnapshot, ResourceScheduledTaskStatus,
    ResourceService, ResourceServiceStatus, ResourceTmuxPane, ResourceTmuxSession,
    ResourceTmuxSnapshot, ResourceTmuxStatus, ResourceTmuxWindow, ResourceTopProcess,
    ScheduledTaskActionKind, ScheduledTaskCapability, ScheduledTaskFilter, ServiceActionKind,
    ServiceCommandCapability, TmuxActionKind, TmuxCommandCapability, build_docker_action_command,
    build_docker_exec_shell_command, build_docker_follow_logs_command, build_docker_logs_command,
    build_filesystem_diagnostic_command, build_filesystem_snapshot_command,
    build_log_follow_command, build_log_snapshot_command, build_package_inspect_command,
    build_package_snapshot_command, build_port_diagnostic_command, build_port_snapshot_command,
    build_process_action_command, build_scheduled_task_action_command,
    build_scheduled_task_diagnostic_command, build_scheduled_task_logs_command,
    build_scheduled_task_snapshot_command, build_service_action_command,
    build_service_follow_logs_command, build_service_logs_command, build_tmux_action_command,
    build_tmux_attach_command, build_tmux_new_session_command, build_tmux_snapshot_command,
    compact_monitor_row_signature, compact_monitor_rows, disk_list_rows,
    docker_action_failure_message, docker_action_succeeded, docker_row_signature,
    docker_state_label_key, filesystem_attention_label_keys, filesystem_entry_severity,
    filesystem_filter_label_key, filesystem_kind_label_key, filesystem_read_only_label_key,
    filesystem_row_signature, format_bytes, format_rate, gpu_list_rows, gpu_memory_percent,
    gpu_memory_summary, gpu_utilization_percent, host_tool_capture_failure_message,
    interface_list_rows, interpret_docker_action_output, interpret_process_action_output,
    interpret_scheduled_task_action_output, interpret_service_action_output,
    interpret_tmux_action_output, log_level_label_key, log_preset_label_key, log_row_signature,
    metrics_source_label_key, package_filter_label_key, package_row_signature,
    package_status_label_key, parse_log_snapshot, parse_package_snapshot, parse_port_snapshot,
    percent_level, port_endpoint, port_filter_label_key, port_is_risky_exposure,
    port_row_signature, port_state_label_key, process_display_command, process_display_name,
    process_row_signature, process_state_label_key, resource_metrics_is_rtt_only, rtt_level,
    scheduled_task_active_label_key, scheduled_task_enabled_label_key,
    scheduled_task_filter_label_key, scheduled_task_row_signature, scheduled_task_source_label_key,
    service_action_failure_message, service_action_succeeded, service_enabled_label_key,
    service_row_signature, service_state_label_key, tmux_session_row_signature,
    top_process_list_rows, visible_docker_rows, visible_filesystem_rows, visible_log_rows,
    visible_package_rows, visible_port_rows, visible_process_rows, visible_scheduled_task_rows,
    visible_service_rows, visible_tmux_session_rows,
};
use oxideterm_connections::{
    ConnectionImportDuplicateStrategy, ConnectionImportPreview, ConnectionImportSource,
    ConnectionStore, PrivilegeCredentialKind, SaveConnectionRequest, SavedPrivilegeCredential,
};
use oxideterm_forwarding::{
    ForwardEvent, ForwardRule, ForwardStatus, ForwardType, ForwardingRegistry, SavedForwardStore,
};
use oxideterm_gpui_ide::IdeSurface;
use oxideterm_gpui_platform::{
    rendering::detect_graphics,
    vibrancy::{NativeVibrancyMode, VibrancySupport, apply_window_vibrancy},
};
use oxideterm_gpui_terminal::{
    BackgroundImageRenderCache, PrivilegePromptMatch, SharedTerminalSession, TerminalBackgroundFit,
    TerminalBackgroundPreferences, TerminalCommandSelectionLabels, TerminalContextAction,
    TerminalHighlightRenderMode, TerminalHighlightRule as UiHighlightRule,
    TerminalInputInterceptor, TerminalInputInterceptorResult, TerminalModemLabels, TerminalNotice,
    TerminalNoticeVariant, TerminalOutputProcessor, TerminalPane, TerminalPaneEvent,
    TerminalPasteLabels, TerminalRecordingState, TerminalRecordingStatus, TerminalSearchStatus,
    TerminalSerialControlLabels, TerminalTrzszLabels, TerminalUiPreferences, TerminalUiTheme,
    TerminalWorkingDirectorySource, detect_custom_privilege_prompt, detect_privilege_prompt,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_gpui_ui::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView,
    modal::{popover_backdrop, set_tauri_backdrop_blur_allowed},
    toast::{ToastVariant, ToastView, toast_action, toast_close},
    toaster::toaster,
    tooltip::tooltip_content,
};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_ide_fs::NodeAgentIdeFileSystem;
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
use oxideterm_session_adapter::{
    reconnect_max_attempts_from_settings, reconnect_timing_from_settings,
    sftp_runtime_settings_from_settings,
    terminal_encoding_from_settings as session_terminal_encoding,
};
use oxideterm_settings::{
    AI_SIDEBAR_MAX_WIDTH, AI_SIDEBAR_MIN_WIDTH, BackgroundFit, BackgroundScope,
    CursorStyle as SettingsCursorStyle, FontFamily, FrostedGlassMode, HighlightRuleRenderMode,
    Language, PersistedSettings, SettingsStore, default_settings_path,
};
use oxideterm_settings_model::{
    AiMcpServerDraft, AiModelRefreshDelivery, AiProviderKeyStatusDelivery, SettingsPageModel,
};
use oxideterm_sftp::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, LazyProgressStore, ProgressStore, SftpTransferGuard,
    SftpTransferManager, StoredTransferProgress, TransferStrategy, tar_download_directory,
    tar_upload_directory,
};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, ConnectionPoolConfig, ConnectionState, ConnectionTraceEvent,
    ConnectionTraceMode, ConnectionTracePlan, ConnectionTraceStage, ConnectionTraceState,
    ConnectionTraceStatus, MAX_RETAINED_RECONNECT_JOBS, NodeId, NodeOrigin, NodeReadiness,
    NodeRouter, NodeRuntimeStore, NodeState, NodeStateEvent, NodeTreeExpansion, NodeTreeSnapshot,
    NodeTreeSnapshotNode, PhaseResult, ProbeConnectionStatus, ProxyHopConfig, ReconnectForwardRule,
    ReconnectForwardRuleSnapshot, ReconnectJob, ReconnectNodeConnectionSnapshot,
    ReconnectNodeTerminalSnapshot, ReconnectNodeTransferSnapshot, ReconnectOrchestratorStore,
    ReconnectPhase, ReconnectSnapshot, SshAlgorithmDiagnosticKind, SshConfig,
    SshConnectionRegistry, SshTransportClient, TerminalEndpoint, UpstreamProxyConfig,
};
use oxideterm_ssh_launch::TemporarySshLaunch;
use oxideterm_terminal::{
    LocalPtyConfig, RawTcpSessionConfig, RawUdpSessionConfig, SerialSessionConfig, ShellInfo,
    SshSessionConfig, TelnetSessionConfig, TerminalCommandMarkDetectionSource, TerminalCursorShape,
    TerminalLifecycle, scan_shells,
};
use oxideterm_theme::{
    AppUiColors, TerminalTheme, ThemeTokens, UiDensityProfile, UiMotionProfile, UiRadii,
    derive_ui_colors_from_terminal, theme_by_id,
};
use oxideterm_workspace::{
    ActiveSessionNode, ActiveSessionReadiness, ActiveSessionStatus,
    CommandPaletteMode as PaletteMode, MAX_PANES_PER_TAB, PaneId, PaneNode, SplitDirection, Tab,
    TabId, TabKind, TabTitleSource, TerminalSessionId, adjusted_split_sizes, balanced_sizes,
};

use self::actions::SearchBarState;
use self::connection_monitor::{ConnectionMonitorState, ConnectionRuntimeSection};
use self::file_manager::FileManagerState;
use self::graphics::GraphicsState;
use self::ime::{
    WorkspaceImeDragSelection, WorkspaceImeElement, WorkspaceImeSelection, WorkspaceImeTarget,
    active_ime_should_defer_input_key,
};
use self::launcher::LauncherState;
use self::new_connection::{
    HostKeyChallenge, KeyboardInteractiveChallenge, NativeSessionTreeConnectPlan,
    NativeSshPromptHandler, NewConnectionField, NewConnectionForm, NewConnectionSelect,
    PrivilegeCredentialDraft, SavedConnectionPromptAction, SshAuthTab, SshConnectionIntent,
    SshConnectionWorkerResult,
};
use self::onboarding::OnboardingState;
use self::pane_tree::SplitDrag;
use self::quick_commands::QuickCommandsState;
use self::root::state::{PendingSshTerminalOpen, ReconnectWorkerResult, WorkspaceSshNode};
use self::root::{background::*, helpers::*};
use self::session_manager::SessionManagerState;
use self::sidebar::AiInlinePanelState;
use self::sidebar::{ActiveSessionSidebarViewMode, SidebarSection};
use self::sidebar::{
    AiCompactionDelivery, AiModelSelectorProbeDelivery, AiPendingChatStream, AiStreamDelivery,
};
use self::terminal_cast::TerminalCastPlayerState;
use crate::{
    CloseOtherTabs, ClosePane, CloseSearch, CloseTab, CommandPalette, Copy, Find, FindNext,
    FindPrev, FontDecrease, FontIncrease, FontReset, GoToTab1, GoToTab2, GoToTab3, GoToTab4,
    GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewConnection, NewTerminal, NextTab,
    OpenSettings, PaletteAiSidebar, PaletteBroadcast, PaletteCancelReconnect, PaletteCleanupDead,
    PaletteDetachTerminal, PaletteDisconnectAll, PaletteEventLog, PaletteHealthCheck,
    PaletteReconnectAll, PaletteResetPanes, Paste, PrevTab, ShellLauncher, ShowShortcuts,
    SplitHorizontal, SplitNavLeft, SplitNavRight, SplitVertical, SwitchLocaleChinese,
    SwitchLocaleEnglish, SwitchLocaleFrench, SwitchLocaleGerman, SwitchLocaleItalian,
    SwitchLocaleJapanese, SwitchLocaleKorean, SwitchLocalePortugueseBrazil, SwitchLocaleSpanish,
    SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese, TerminalAiPanel, TerminalRecording,
    ToggleSidebar, ZenMode,
};
use crate::{assets::LucideIcon, bundled_fonts};
use oxideterm_gpui_markdown::{
    MarkdownBlockLayout, MarkdownCodeBlockActions, MarkdownDocument, MarkdownMermaidZoomHandler,
    MarkdownOptions, MarkdownVirtualListScrollHandle, markdown_virtual_with_code_actions,
};

const MERMAID_MODAL_RASTER_SCALE: f32 = 3.0;

pub(crate) fn locale_from_settings(language: Language) -> Locale {
    root_locale_from_settings(language)
}

use oxideterm_gpui_settings_view::{
    ActiveSurface, SettingsInput, SettingsSelect, SettingsSlider, SettingsTab,
};
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId, select_anchor_probe};
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputAnchorId};
use oxideterm_gpui_ui::typography::{
    css_font_family_head as settings_css_font_family_head, gpui_font_family_name,
    tauri_ui_font_family as settings_ui_font_family,
};
pub(super) use selectable_text::{
    SelectableTextRole, SelectableTextScrollExt, selectable_vertical_scrollbar_layer,
};
pub(super) use virtual_list::{
    TauriVirtualListSpec, TauriVirtualScrollAlign, scroll_tauri_virtual_list_to_index,
    tauri_virtual_list, tauri_virtual_list_is_near_bottom, tauri_virtual_list_state,
    tauri_virtual_uniform_list, uniform_list_edge_autoscroll,
};
use virtual_list::{
    VirtualListSignatureCache, sync_tauri_variable_list_state_by_signatures,
    sync_tauri_virtual_list_state_by_signatures,
};

const SETTINGS_SECTION_LIST_INITIAL_ITEM_COUNT: usize = 4;
const SETTINGS_SECTION_LIST_ESTIMATED_HEIGHT: f32 = 260.0;
const SETTINGS_SECTION_LIST_OVERSCAN: usize = 2;
const SETTINGS_SCROLL_CARET_PAUSE_MS: u64 = 700;
const AI_SETTINGS_SECTION_ESTIMATED_HEIGHT: f32 = 360.0;
const AI_PROVIDER_MODEL_ROW_LIST_INITIAL_ITEM_COUNT: usize = 0;
const AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT: f32 = 48.0;
const AI_PROVIDER_MODEL_ROW_LIST_OVERSCAN: usize = 6;
const AI_PROVIDER_MODEL_CHIP_LIST_INITIAL_ROW_COUNT: usize = 0;
const AI_PROVIDER_MODEL_CHIPS_PER_VIRTUAL_ROW: usize = 4;
const AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT: f32 = 28.0;
const AI_PROVIDER_MODEL_CHIP_ROW_OVERSCAN: usize = 6;
const AI_PROVIDER_CARD_LIST_INITIAL_ITEM_COUNT: usize = 0;
const AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT: f32 = 220.0;
const AI_PROVIDER_CARD_LIST_OVERSCAN: usize = 3;
const AI_MCP_SERVER_LIST_INITIAL_ITEM_COUNT: usize = 0;
const AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT: f32 = 156.0;
const AI_MCP_SERVER_LIST_OVERSCAN: usize = 4;
const CLOUD_SYNC_SECTION_LIST_INITIAL_ITEM_COUNT: usize = 7;
const CLOUD_SYNC_SECTION_LIST_ESTIMATED_HEIGHT: f32 = 240.0;
const CLOUD_SYNC_SECTION_LIST_OVERSCAN: usize = 1;
const FORWARDS_SECTION_LIST_INITIAL_ITEM_COUNT: usize = 5;
const FORWARDS_SECTION_LIST_ESTIMATED_HEIGHT: f32 = 180.0;
const FORWARDS_SECTION_LIST_OVERSCAN: usize = 2;
const FORWARDS_TABLE_ROW_LIST_INITIAL_ITEM_COUNT: usize = 0;
const FORWARDS_TABLE_ROW_LIST_ESTIMATED_HEIGHT: f32 = 42.0;
const FORWARDS_TABLE_ROW_LIST_OVERSCAN: usize = 8;
const CONNECTION_MONITOR_SECTION_LIST_ITEM_COUNT: usize = 2;
const CONNECTION_MONITOR_SECTION_LIST_ESTIMATED_HEIGHT: f32 = 280.0;
const CONNECTION_MONITOR_SECTION_LIST_OVERSCAN: usize = 1;
const LAUNCHER_WSL_LIST_INITIAL_ITEM_COUNT: usize = 0;
const LAUNCHER_WSL_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const LAUNCHER_WSL_LIST_OVERSCAN: usize = 6;
const LAUNCHER_APP_GRID_INITIAL_ROW_COUNT: usize = 0;
const LAUNCHER_APP_GRID_ESTIMATED_ROW_HEIGHT: f32 = 104.0;
const LAUNCHER_APP_GRID_OVERSCAN: usize = 4;
const QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT: usize = 0;
const QUICK_COMMAND_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const QUICK_COMMAND_LIST_OVERSCAN: usize = 6;
const DETACHED_LOCAL_TERMINAL_LIST_INITIAL_ITEM_COUNT: usize = 0;
const DETACHED_LOCAL_TERMINAL_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const DETACHED_LOCAL_TERMINAL_LIST_OVERSCAN: usize = 4;
const ACTIVE_SESSION_SIDEBAR_LIST_INITIAL_ITEM_COUNT: usize = 0;
const ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT: f32 = 48.0;
const ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN: usize = 8;
const ACTIVE_SESSION_FOCUS_LIST_ESTIMATED_HEIGHT: f32 = 76.0;
const OXIDE_EXPORT_CONNECTION_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_EXPORT_CONNECTION_LIST_ESTIMATED_HEIGHT: f32 = 58.0;
const OXIDE_EXPORT_CONNECTION_LIST_OVERSCAN: usize = 8;
const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_ESTIMATED_HEIGHT: f32 = 22.0;
const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_OVERSCAN: usize = 8;
const OXIDE_EXPORT_FORWARD_GROUP_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_EXPORT_FORWARD_GROUP_LIST_ESTIMATED_HEIGHT: f32 = 84.0;
const OXIDE_EXPORT_FORWARD_GROUP_LIST_OVERSCAN: usize = 4;
const OXIDE_EXPORT_SUMMARY_LINE_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT: f32 = 18.0;
const OXIDE_EXPORT_SUMMARY_LINE_LIST_OVERSCAN: usize = 6;
const OXIDE_IMPORT_FORWARD_DETAIL_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_IMPORT_FORWARD_DETAIL_LIST_ESTIMATED_HEIGHT: f32 = 36.0;
const OXIDE_IMPORT_FORWARD_DETAIL_LIST_OVERSCAN: usize = 6;
const OXIDE_IMPORT_NAME_GROUP_LIST_INITIAL_ITEM_COUNT: usize = 0;
const OXIDE_IMPORT_NAME_GROUP_LIST_ESTIMATED_HEIGHT: f32 = 28.0;
const OXIDE_IMPORT_NAME_GROUP_LIST_OVERSCAN: usize = 6;
const CLOUD_SYNC_ROLLBACK_BACKUP_LIST_INITIAL_ITEM_COUNT: usize = 0;
const CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT: f32 = 72.0;
const CLOUD_SYNC_ROLLBACK_BACKUP_LIST_OVERSCAN: usize = 4;
const CLOUD_SYNC_HISTORY_LIST_INITIAL_ITEM_COUNT: usize = 0;
const CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT: f32 = 72.0;
const CLOUD_SYNC_HISTORY_LIST_OVERSCAN: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
enum AiCompactionNoticePhase {
    Running,
    Done,
}

#[derive(Clone, Debug)]
struct AiCompactionNotice {
    conversation_id: String,
    phase: AiCompactionNoticePhase,
    compacted_count: Option<usize>,
    timestamp_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AiChatInitializationError {
    message_key: &'static str,
    can_retry: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AiChatFooterAction {
    Submit,
}

// AI composer footer uses the same explicit action list as dialog footers so
// keyboard focus order stays centralized even though it is not a modal trap.
const AI_CHAT_FOOTER_ACTIONS: [AiChatFooterAction; 1] = [AiChatFooterAction::Submit];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeybindingRecordingFooterAction {
    Confirm,
    Cancel,
}

const CONFIRM_DIALOG_FOOTER_ACTIONS: [ConfirmDialogAction; 2] =
    [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];
const KEYBINDING_RECORDING_FOOTER_ACTIONS: [KeybindingRecordingFooterAction; 2] = [
    KeybindingRecordingFooterAction::Confirm,
    KeybindingRecordingFooterAction::Cancel,
];

enum KnowledgeReindexDelivery {
    Progress { current: usize, total: usize },
    Finished(Result<usize, String>),
}

#[derive(Default)]
struct AiMarkdownDocumentCache {
    documents: HashMap<String, AiCachedMarkdownDocument>,
    insertion_order: VecDeque<String>,
}

#[derive(Clone)]
struct AiCachedMarkdownDocument {
    document: MarkdownDocument,
    layout: MarkdownBlockLayout,
}

const AI_MARKDOWN_DOCUMENT_CACHE_MAX_ENTRIES: usize = 128;
const AI_CHAT_LIST_ROW_HEIGHT_ESTIMATE: f32 = 80.0;
const AI_CHAT_LIST_VIRTUAL_OVERSCAN: usize = 8;

fn ai_chat_virtual_list_spec() -> TauriVirtualListSpec {
    // Tauri AI chat is a browser scroll container, while native uses GPUI List
    // for message virtualization. Keep the estimate/overscan explicit so this
    // variable-height list follows the same shared virtual-list contract as
    // tables, file panes, notifications, and event logs.
    TauriVirtualListSpec::new(
        px(AI_CHAT_LIST_ROW_HEIGHT_ESTIMATE),
        AI_CHAT_LIST_VIRTUAL_OVERSCAN,
    )
}

// Tauri NotificationsPanel uses variable-height grouped rows. Keep the native
// estimate/overscan as a virtual-list spec instead of a raw overdraw number so
// notification/event-log surfaces share the same browser virtualizer contract.
const NOTIFICATION_SIDEBAR_ROW_HEIGHT_ESTIMATE: f32 = 72.0;
const NOTIFICATION_SIDEBAR_VIRTUAL_OVERSCAN: usize = 10;
const AI_MARKDOWN_WINDOW_OVERDRAW_PX: f32 = 720.0;
const AI_MARKDOWN_CONTENT_OFFSET_PX: f32 = 56.0;

#[derive(Clone, Debug)]
enum AiChatListItem {
    TrimNotice { sequence: u64, count: usize },
    Message { id: String },
    BottomSpacer,
}

#[derive(Clone, Copy, Debug)]
struct AiMessageViewport {
    top: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct AiChatListViewportSnapshot {
    item_ix: usize,
    offset_in_item: f32,
    height: f32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AiContextTokenBreakdown {
    system_instructions: usize,
    tool_definitions: usize,
    reserved_output: usize,
    messages: usize,
    tool_results: usize,
    total: usize,
    max_tokens: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AiContextTokenBreakdownKey {
    conversation_id: Option<String>,
    conversation_fingerprint: u64,
    provider_id: String,
    model: String,
    max_tokens: usize,
    system_prompt_fingerprint: u64,
    tool_use_enabled: bool,
}

#[derive(Default)]
struct AiContextTokenBreakdownCache {
    key: Option<AiContextTokenBreakdownKey>,
    breakdown_without_draft: Option<AiContextTokenBreakdown>,
}

#[derive(Clone, Debug)]
struct CommandPaletteState {
    open: bool,
    raw_query: String,
    mode: PaletteMode,
    selected_index: usize,
    scroll_handle: UniformListScrollHandle,
    ssh_config_hosts: Vec<oxideterm_connections::SshConfigHost>,
    ssh_config_hosts_loading: bool,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConfirmKeyboardAction {
    Cancel,
    Confirm,
    Handled,
}

#[derive(Clone, Debug)]
struct ShortcutsModalState {
    open: bool,
    query: String,
    scroll_handle: UniformListScrollHandle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AiModelSelectorScope {
    Sidebar,
    TerminalInline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TabDragMode {
    Pending,
    Reorder,
    Detach,
}

#[derive(Clone, Debug)]
struct TabDragState {
    tab_id: TabId,
    from_index: usize,
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
    tab_widths: Vec<f32>,
    active: bool,
    mode: TabDragMode,
    drop_target_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabContextMenu {
    tab_id: TabId,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug)]
struct ExitingTabVisual {
    tab_id: TabId,
    kind: TabKind,
    title: String,
    width: f32,
    visual_index: usize,
    was_active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TabCloseConfirm {
    Single { tab_id: TabId },
    LocalChildProcess { tab_id: TabId },
    LocalChildProcessBatch { tab_ids: Vec<TabId> },
    Other { tab_ids: Vec<TabId> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LocalTerminalCloseCheck {
    Single { tab_id: TabId },
    Batch { tab_ids: Vec<TabId> },
}

impl LocalTerminalCloseCheck {
    fn tab_ids(&self) -> Vec<TabId> {
        match self {
            Self::Single { tab_id } => vec![*tab_id],
            Self::Batch { tab_ids } => tab_ids.clone(),
        }
    }
}

struct WorkspaceWindowTabState {
    active_tab_id: Option<TabId>,
    navigation_history: Vec<TabId>,
    navigation_index: Option<usize>,
    navigation_replaying: bool,
    navigation_observed_tab: Option<TabId>,
    drag: Option<TabDragState>,
    context_menu: Option<TabContextMenu>,
    close_confirm: Option<TabCloseConfirm>,
    process_close_check_generation: u64,
    exiting_tabs: Vec<ExitingTabVisual>,
    scroll_handle: ScrollHandle,
    scrollbar_drag: Option<TabbarScrollbarDragState>,
    scrollbar_hovered: bool,
}

#[derive(Clone, Copy, Debug)]
struct TabbarScrollbarDragState {
    // Preserve the pointer's position inside the thumb to prevent a jump on drag start.
    grab_offset_x: f32,
}

#[derive(Clone, Copy, Debug)]
struct DetachedTabReturnDrag {
    tab_id: TabId,
    start_screen_x: f32,
    start_screen_y: f32,
    current_screen_x: f32,
    current_screen_y: f32,
    active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabWindowHandoffOrigin {
    screen_left: f32,
    screen_top: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DetachedTabReturnHandoff {
    tab_id: TabId,
    origin: TabWindowHandoffOrigin,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DetachedTabReturnPlaceholder {
    tab_id: TabId,
    visible_index: usize,
}

impl WorkspaceWindowTabState {
    fn new() -> Self {
        Self {
            active_tab_id: None,
            navigation_history: Vec::new(),
            navigation_index: None,
            navigation_replaying: false,
            navigation_observed_tab: None,
            drag: None,
            context_menu: None,
            close_confirm: None,
            process_close_check_generation: 0,
            exiting_tabs: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            scrollbar_drag: None,
            scrollbar_hovered: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeDisconnectConfirm {
    node_id: NodeId,
    display_name: String,
}

#[derive(Clone, Copy)]
enum SimpleConfirmExitTarget {
    AiClearAll,
    AiDeleteMessage,
    NodeDisconnect,
    TabClose,
    SettingsDataDirectory,
    KeybindingResetAll,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DataDirectoryConfirm {
    Conflict {
        path: PathBuf,
        files_found: Vec<String>,
    },
    Reset,
}

#[derive(Clone)]
pub(super) struct SelectableTextFragmentState {
    pub group_id: u64,
    pub order: usize,
    pub generation: u64,
    pub text: String,
    pub layout: TextLayout,
    pub anchor: TextInputAnchor,
}

pub(crate) struct WorkspaceApp {
    focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    main_window_tabs: WorkspaceWindowTabState,
    detached_tabs: HashSet<TabId>,
    detached_tab_return_drag: Option<DetachedTabReturnDrag>,
    detached_tab_return_handoff: Option<DetachedTabReturnHandoff>,
    next_tab_window_handoff_generation: u64,
    main_window_tabbar_drop_bounds: Option<Bounds<Pixels>>,
    node_disconnect_confirm: Option<NodeDisconnectConfirm>,
    node_disconnect_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    panes: HashMap<PaneId, gpui::Entity<TerminalPane>>,
    terminal_pane_subscriptions: HashMap<PaneId, Subscription>,
    pending_auto_close_terminal_sessions: HashSet<TerminalSessionId>,
    auto_close_terminal_sessions_scheduled: bool,
    host_tools_tab_scroll_handle: ScrollHandle,
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    search: SearchBarState,
    terminal_command_bar_focused: bool,
    terminal_command_input_collapsed: bool,
    terminal_command_bar_draft: String,
    terminal_command_suggestions_open: bool,
    terminal_command_suggestion_highlighted: Option<usize>,
    terminal_broadcast_enabled: bool,
    terminal_broadcast_targets: HashSet<PaneId>,
    terminal_broadcast_menu_open: bool,
    terminal_quick_commands_open: bool,
    terminal_quick_commands_pinned: bool,
    terminal_quick_command_pending: Option<String>,
    terminal_cwd_tx: std::sync::mpsc::Sender<terminal_cwd::TerminalCwdDelivery>,
    terminal_cwd_rx: std::sync::mpsc::Receiver<terminal_cwd::TerminalCwdDelivery>,
    terminal_cwd_picker: terminal_cwd::TerminalCwdPickerState,
    terminal_git_store: oxideterm_environment::GitStatusStore,
    terminal_git_tx: std::sync::mpsc::Sender<terminal_git::TerminalGitDelivery>,
    terminal_git_rx: std::sync::mpsc::Receiver<terminal_git::TerminalGitDelivery>,
    terminal_git_branch_picker: terminal_git::TerminalGitBranchPickerState,
    terminal_project_store: oxideterm_environment::ProjectStatusStore,
    terminal_project_tx: std::sync::mpsc::Sender<terminal_project::TerminalProjectDelivery>,
    terminal_project_rx: std::sync::mpsc::Receiver<terminal_project::TerminalProjectDelivery>,
    terminal_project_panel: terminal_project::TerminalProjectPanelState,
    detached_local_terminals: HashMap<TerminalSessionId, DetachedLocalTerminalSession>,
    serial_terminal_configs: HashMap<TerminalSessionId, SerialSessionConfig>,
    raw_tcp_terminal_configs: HashMap<TerminalSessionId, RawTcpSessionConfig>,
    raw_udp_terminal_configs: HashMap<TerminalSessionId, RawUdpSessionConfig>,
    detached_local_terminals_popover_open: bool,
    terminal_cast_player: Option<TerminalCastPlayerState>,
    terminal_cast_seek_dragging: bool,
    command_palette: CommandPaletteState,
    version_migration: VersionMigrationState,
    onboarding: OnboardingState,
    shortcuts_modal: ShortcutsModalState,
    settings_page: SettingsPageModel,
    segmented_control_user_motion: selection_motion::UserSegmentedControlMotionState,
    theme_editor_presence: oxideterm_gpui_ui::motion::ExitPresence,
    knowledge_create_presence: oxideterm_gpui_ui::motion::ExitPresence,
    knowledge_document_presence: oxideterm_gpui_ui::motion::ExitPresence,
    ssh_config_import_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    ai_mcp_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    managed_key_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    portable_settings_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    help_legal_notice_presence: oxideterm_gpui_ui::motion::ExitPresence,
    ai_settings_dialog_presence: oxideterm_gpui_ui::motion::ExitPresence,
    // Prompt and memory documents are edited outside the virtual settings list.
    ai_text_editor_dialog: Option<settings::AiTextEditorDialog>,
    ai_text_editor: Option<Entity<oxideterm_gpui_editor::TextEditorView>>,
    settings_managed_key_dialog: Option<SettingsManagedKeyDialog>,
    settings_managed_key_status: Option<String>,
    settings_managed_key_file_path: String,
    settings_managed_key_file_name: String,
    settings_managed_key_file_passphrase: String,
    settings_managed_key_paste_name: String,
    settings_managed_key_paste_private_key: String,
    settings_managed_key_paste_passphrase: String,
    settings_managed_key_rename_name: String,
    settings_connection_import_source: ConnectionImportSource,
    settings_connection_import_paths: Vec<String>,
    settings_connection_import_preview: Option<ConnectionImportPreview>,
    settings_selected_connection_import_drafts: HashSet<String>,
    settings_connection_import_duplicate_strategy: ConnectionImportDuplicateStrategy,
    settings_connection_import_target_group: String,
    settings_network_proxy_password_status: Option<String>,
    settings_network_proxy_test_host: String,
    settings_network_proxy_test_port: String,
    settings_network_proxy_test_pending: bool,
    settings_network_proxy_test_status: Option<String>,
    settings_local_privilege_draft: PrivilegeCredentialDraft,
    settings_local_privilege_error: Option<String>,
    quick_commands: QuickCommandsState,
    quick_command_list_state: ListState,
    quick_command_list_cache: RefCell<VirtualListSignatureCache>,
    detached_local_terminal_list_state: ListState,
    detached_local_terminal_list_cache: RefCell<VirtualListSignatureCache>,
    native_plugin_manager: plugin_manager::NativePluginManagerState,
    split_drag: Option<SplitDrag>,
    sidebar_resizing: bool,
    sidebar_collapsed: bool,
    sidebar_rendered: bool,
    sidebar_motion_generation: u64,
    sidebar_width: f32,
    context_sidebar_rendered: bool,
    context_sidebar_motion_generation: u64,
    ai: ai_state::AiWorkspaceState,
    active_context_sidebar_panel: ContextSidebarPanel,
    active_context_sidebar_tool: ContextSidebarTool,
    needs_active_pane_focus: bool,
    active_sidebar_section: SidebarSection,
    active_surface: ActiveSurface,
    active_session_sidebar_view_mode: ActiveSessionSidebarViewMode,
    active_session_sidebar_focused_node_id: Option<NodeId>,
    active_session_sidebar_list_state: ListState,
    active_session_sidebar_list_cache: RefCell<VirtualListSignatureCache>,
    open_settings_select: Option<SettingsSelect>,
    settings_select_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    settings_section_list_state: ListState,
    settings_section_list_cache: RefCell<VirtualListSignatureCache>,
    settings_data_directory_confirm: Option<DataDirectoryConfirm>,
    settings_data_directory_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    standard_confirm_focused_action: Option<ConfirmDialogAction>,
    settings_reset_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    keybinding_reset_all_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    ai_clear_all_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    ai_delete_message_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    tab_close_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    select_anchors: HashMap<SelectAnchorId, OverlayAnchor>,
    text_input_anchors: HashMap<TextInputAnchorId, TextInputAnchor>,
    selectable_text_values: HashMap<u64, String>,
    selectable_text_layouts: HashMap<u64, TextLayout>,
    selectable_text_fragments: HashMap<u64, SelectableTextFragmentState>,
    selectable_text_generation: u64,
    selectable_text_pending_updates: Rc<RefCell<selectable_text::SelectableTextFrameUpdates>>,
    selectable_text_flush_scheduled: Rc<Cell<bool>>,
    selectable_text_autoscroll_position: Option<Point<Pixels>>,
    selectable_text_autoscroll_scheduled: bool,
    selectable_text_scroll_handles: RefCell<HashMap<String, ScrollHandle>>,
    mermaid_zoom: Option<MermaidZoomState>,
    ime_marked_text: Option<ime::WorkspaceImeMarkedText>,
    pending_platform_text_commit: Option<ime::PendingPlatformTextCommit>,
    next_platform_text_commit_generation: u64,
    selected_ime_target: Option<WorkspaceImeTarget>,
    selected_ime_range: Option<WorkspaceImeSelection>,
    ime_drag_selection: Option<WorkspaceImeDragSelection>,
    focused_settings_input: Option<SettingsInput>,
    settings_input_draft: String,
    // The large command-spec document is edited in a workspace modal so the
    // settings virtual list remains the only scroll owner behind it.
    terminal_command_specs_editor_open: bool,
    settings_slider_drag: Option<SettingsSlider>,
    settings_caret_blink_pause_until: Option<Instant>,
    keybinding_recording_combo: Option<crate::keybindings::KeyCombo>,
    keybinding_recording_footer_focus: Option<KeybindingRecordingFooterAction>,
    portable_settings_dialog: Option<settings::PortableSettingsDialog>,
    portable_settings_action_pending: Option<settings::PortableSettingsAction>,
    portable_settings_action_error: Option<String>,
    portable_status_snapshot: Option<oxideterm_portable_runtime::PortableStatusSnapshot>,
    portable_status_error: Option<String>,
    portable_exportable_secret_count: Option<usize>,
    portable_settings_refresh_pending: bool,
    native_update_state: settings::NativeUpdateUiState,
    native_update_rx: Option<std::sync::mpsc::Receiver<settings::NativeUpdateDelivery>>,
    native_update_polling: bool,
    native_update_cancel: Option<Arc<AtomicBool>>,
    native_update_package: Option<oxideterm_update::NativeUpdatePackage>,
    native_update_notification_open: bool,
    native_update_notification_presence: oxideterm_gpui_ui::motion::ExitPresence,
    native_update_release_notes_open: bool,
    native_update_release_notes_presence: oxideterm_gpui_ui::motion::ExitPresence,
    native_update_release_notes_scroll: MarkdownVirtualListScrollHandle,
    settings_legal_notice_scroll: MarkdownVirtualListScrollHandle,
    desktop_presence_rx: Option<oxideterm_desktop_presence::DesktopPresenceReceiver>,
    desktop_presence_polling: bool,
    single_instance_rx: Option<crate::single_instance::SingleInstanceReceiver>,
    single_instance_polling: bool,
    portable_current_password: String,
    portable_new_password: String,
    portable_confirm_password: String,
    new_connection_form: Option<NewConnectionForm>,
    new_connection_form_presence: oxideterm_gpui_ui::motion::ExitPresence,
    jump_server_form_presence: oxideterm_gpui_ui::motion::ExitPresence,
    jump_server_exit_commits: bool,
    drill_down_parent_node_id: Option<NodeId>,
    editing_saved_connection_id: Option<String>,
    editing_saved_connection_connect_after_save_node_id: Option<NodeId>,
    editing_raw_tcp_profile_id: Option<String>,
    editing_raw_udp_profile_id: Option<String>,
    duplicating_saved_connection_id: Option<String>,
    saved_connection_prompt_action: Option<SavedConnectionPromptAction>,
    open_new_connection_select: Option<NewConnectionSelect>,
    new_connection_select_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    new_connection_caret_visible: bool,
    host_key_challenge: Option<HostKeyChallenge>,
    active_proxy_connect_run: Option<NativeProxyConnectRun>,
    keyboard_interactive_challenge: Option<KeyboardInteractiveChallenge>,
    keyboard_interactive_timer_generation: u64,
    ssh_worker_tx: std::sync::mpsc::Sender<SshConnectionWorkerResult>,
    ssh_worker_rx: std::sync::mpsc::Receiver<SshConnectionWorkerResult>,
    ssh_registry: SshConnectionRegistry,
    forwarding_registry: ForwardingRegistry,
    forwarding_runtime: Arc<tokio::runtime::Runtime>,
    wsl_graphics: Arc<oxideterm_wsl_graphics::WslGraphicsState>,
    forwarding_connection_consumers: HashMap<String, (String, ConnectionConsumer)>,
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
    notification_sidebar_list_state: ListState,
    notification_sidebar_list_cache: RefCell<VirtualListSignatureCache>,
    event_log_sidebar_scroll_handle: UniformListScrollHandle,
    terminal_endpoint_sessions: HashMap<TerminalSessionId, WorkspaceTerminalEndpointSession>,
    ssh_nodes: HashMap<NodeId, WorkspaceSshNode>,
    saved_ssh_nodes: HashMap<String, NodeId>,
    terminal_ssh_nodes: HashMap<TerminalSessionId, NodeId>,
    pending_ssh_terminal_opens: VecDeque<PendingSshTerminalOpen>,
    expanded_ssh_nodes: HashSet<NodeId>,
    active_ssh_node_id: Option<NodeId>,
    next_ssh_node_id: u64,
    forward_tab_nodes: HashMap<TabId, NodeId>,
    forwards_section_list_state: ListState,
    forwards_section_list_cache: RefCell<VirtualListSignatureCache>,
    forwards_table_row_list_state: ListState,
    forwards_table_row_list_cache: RefCell<VirtualListSignatureCache>,
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
    launcher_wsl_list_state: ListState,
    launcher_wsl_list_cache: RefCell<VirtualListSignatureCache>,
    launcher_app_grid_list_state: ListState,
    launcher_app_grid_list_cache: RefCell<VirtualListSignatureCache>,
    graphics: GraphicsState,
    connection_monitor: ConnectionMonitorState,
    active_connection_runtime_section: ConnectionRuntimeSection,
    previous_connection_runtime_section: ConnectionRuntimeSection,
    connection_monitor_section_list_state: ListState,
    connection_monitor_section_list_cache: RefCell<VirtualListSignatureCache>,
    cloud_sync: cloud_sync::CloudSyncWorkspaceState,
    sftp_worker_tx: tokio::sync::mpsc::UnboundedSender<sftp::SftpWorkerResult>,
    forwarding_worker_tx: std::sync::mpsc::Sender<forwards::ForwardingWorkerResult>,
    forwarding_worker_rx: std::sync::mpsc::Receiver<forwards::ForwardingWorkerResult>,
    forwarding_event_rx: std::sync::mpsc::Receiver<ForwardEvent>,
    i18n: I18n,
    tokens: ThemeTokens,
    detected_graphics: DetectedGraphics,
    render_profile_override: Option<RenderProfile>,
    render_policy: EffectiveRenderPolicy,
    applied_vibrancy_mode: NativeVibrancyMode,
    vibrancy_support: VibrancySupport,
    background_image_cache: BackgroundImageRenderCache,
    settings_store: SettingsStore,
    connection_store: ConnectionStore,
    settings_store_last_modified: Option<SystemTime>,
    connection_store_last_modified: Option<SystemTime>,
    native_plugin_runtime: plugin_lifecycle::NativePluginRuntimeState,
    session_manager: SessionManagerState,
    remote_desktop_sessions: HashMap<TabId, remote_desktop::RemoteDesktopSession>,
    remote_desktop_worker_tx: std::sync::mpsc::Sender<remote_desktop::RemoteDesktopWorkerDelivery>,
    remote_desktop_worker_rx:
        std::sync::mpsc::Receiver<remote_desktop::RemoteDesktopWorkerDelivery>,
    oxide_export_connection_list_state: ListState,
    oxide_export_connection_list_cache: RefCell<VirtualListSignatureCache>,
    oxide_import_connection_preview_list_state: ListState,
    oxide_import_connection_preview_list_cache: RefCell<VirtualListSignatureCache>,
    oxide_export_forward_group_list_state: ListState,
    oxide_export_forward_group_list_cache: RefCell<VirtualListSignatureCache>,
    oxide_export_summary_line_list_state: ListState,
    oxide_export_summary_line_list_cache: RefCell<VirtualListSignatureCache>,
    oxide_import_forward_detail_list_state: ListState,
    oxide_import_forward_detail_list_cache: RefCell<VirtualListSignatureCache>,
    oxide_import_name_group_list_states: RefCell<HashMap<String, ListState>>,
    oxide_import_name_group_list_caches: RefCell<HashMap<String, VirtualListSignatureCache>>,
    local_shells: Vec<ShellInfo>,
    local_shell_launcher_open: bool,
    local_shell_launcher_selected_id: Option<String>,
    terminal_notice_tx: std::sync::mpsc::Sender<TerminalNotice>,
    terminal_notice_rx: std::sync::mpsc::Receiver<TerminalNotice>,
    // Standard toasts need stable ids so the close button removes the rendered
    // toast, not whichever item later occupies the same list index.
    workspace_toast_next_id: u64,
    workspace_toasts: Vec<WorkspaceToast>,
    plugin_progress_toasts: HashMap<String, WorkspaceToast>,
    connection_trace_tx: std::sync::mpsc::Sender<ConnectionTraceEvent>,
    connection_trace_rx: std::sync::mpsc::Receiver<ConnectionTraceEvent>,
    connection_trace_toasts: HashMap<String, ActiveConnectionTrace>,
    connection_trace_state: ConnectionTraceState,
    zen_hint_expires_at: Option<Instant>,
    workspace_tooltip: Option<WorkspaceTooltip>,
    workspace_tooltip_pending: Option<WorkspaceTooltipPending>,
    workspace_tooltip_generation: u64,
}

#[derive(Clone)]
struct MermaidZoomState {
    source: String,
    image: Arc<Image>,
    width: f32,
    height: f32,
}

impl WorkspaceApp {
    fn localized_markdown_options(&self) -> MarkdownOptions {
        let mut options = MarkdownOptions::from_theme(&self.tokens);
        options.mermaid_error_prefix = self.i18n.t("markdown.mermaid_unsupported");
        options.mermaid_expand_label = self.i18n.t("markdown.mermaid_expand");
        options
    }

    fn mermaid_zoom_handler(&self, cx: &mut Context<Self>) -> MarkdownMermaidZoomHandler {
        let workspace = cx.entity();
        Arc::new(move |source, image, width, height, window, cx| {
            let workspace = workspace.clone();
            window.defer(cx, move |_window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    let rendered = oxideterm_gpui_markdown::mermaid::render_mermaid_svg_scaled(
                        &source,
                        &this.tokens,
                        &this.localized_markdown_options(),
                        MERMAID_MODAL_RASTER_SCALE,
                    )
                    .ok();
                    this.mermaid_zoom = Some(MermaidZoomState {
                        source,
                        image: rendered
                            .as_ref()
                            .map(|rendered| rendered.image.clone())
                            .unwrap_or(image),
                        width: rendered
                            .as_ref()
                            .map(|rendered| rendered.display_width)
                            .unwrap_or(width),
                        height: rendered
                            .as_ref()
                            .map(|rendered| rendered.display_height)
                            .unwrap_or(height),
                    });
                    cx.notify();
                });
            });
        })
    }

    fn markdown_mermaid_actions(&self, cx: &mut Context<Self>) -> MarkdownCodeBlockActions {
        MarkdownCodeBlockActions {
            on_run: None,
            on_mermaid_zoom: Some(self.mermaid_zoom_handler(cx)),
        }
    }
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
    pub(crate) approval_mode: Option<String>,
    pub(crate) risk: String,
}

#[derive(Clone, Debug)]
pub(crate) struct AiToolExecutionRecord {
    pub(crate) record_id: String,
    pub(crate) conversation_id: String,
    pub(crate) assistant_message_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) tool_name: String,
    pub(crate) argument_summary: String,
    pub(crate) target_id: Option<String>,
    pub(crate) target_kind: Option<String>,
    pub(crate) risk: String,
    pub(crate) approval_source: Option<String>,
    pub(crate) execution_surface: String,
    pub(crate) visible_in_terminal: Option<bool>,
    pub(crate) status: String,
    pub(crate) success: Option<bool>,
    pub(crate) error_code: Option<String>,
    pub(crate) result_summary: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) started_at: i64,
    pub(crate) finished_at: Option<i64>,
    pub(crate) runtime_epoch: String,
}

#[derive(Clone, Debug)]
pub(crate) struct AiToolResultFact {
    pub(crate) fact_id: String,
    pub(crate) conversation_id: String,
    pub(crate) assistant_message_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) tool_name: String,
    pub(crate) source_kind: String,
    pub(crate) text_hash: String,
    pub(crate) summary: String,
    pub(crate) output_preview: String,
    pub(crate) created_at: i64,
    pub(crate) runtime_epoch: String,
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
    id: u64,
    notice: TerminalNotice,
    expires_at: Instant,
    presence: oxideterm_gpui_ui::motion::ExitPresence,
}

#[derive(Clone, Debug)]
struct AcpAgentProbeDelivery {
    agent_id: String,
    result: AcpAgentProbeResult,
}

#[derive(Clone, Debug)]
struct AcpAgentProbeResult {
    runtime_state: oxideterm_settings::AcpAgentRuntimeState,
    auth_status: oxideterm_settings::AcpAgentAuthStatus,
    last_error_kind: Option<String>,
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
    presence: oxideterm_gpui_ui::motion::ExitPresence,
}

#[derive(Clone, Debug)]
struct ConnectionChainRun {
    node_ids: Vec<NodeId>,
    next_index: usize,
    trace_plan: ConnectionTracePlan,
}

#[derive(Clone, Debug)]
struct NativeProxyConnectRun {
    plan: NativeSessionTreeConnectPlan,
    title: String,
    intent: SshConnectionIntent,
    save_after_open: Option<SaveConnectionRequest>,
    upstream_proxy: Option<UpstreamProxyConfig>,
}

#[derive(Clone, Debug)]
struct WorkspaceTooltip {
    id: String,
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

#[derive(Clone)]
struct DetachedLocalTerminalSession {
    session_id: TerminalSessionId,
    title: String,
    session: SharedTerminalSession,
    detached_at: Instant,
    buffer_lines: usize,
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

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_SESSION_TREE_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
