mod acp_workspace;
mod actions;
mod ai_background_tasks;
mod ai_lazy;
mod ai_runtime_context;
mod ai_state;
mod app_lock;
mod breadcrumb_scroll;
mod browser_behavior;
mod cloud_sync;
mod command_palette;
mod connection_monitor;
mod delivery;
mod detached_tab_window;
mod file_manager;
mod forwards;
mod graphics;
mod graphics_vnc;
mod ide;
mod ime;
mod local_shell_launcher;
mod local_terminal_background;
mod new_connection;
mod notification_center;
mod onboarding;
mod overlay;
mod pane_tree;
mod path_completion;
mod plugin_entity;
mod plugin_lifecycle;
mod plugin_manager;
mod plugin_ui;
mod public_mcp;
mod quick_commands;
mod remote_desktop;
mod runtime_entity;
mod root {
    pub(super) mod background;
    pub(super) mod helpers;
    pub(super) mod host_tools;
    pub(super) mod init;
    pub(super) mod modal_owner;
    pub(super) mod render;
    pub(super) mod state;
    #[cfg(test)]
    pub(super) mod tests;
    pub(super) mod window_state;
}
mod selectable_text;
mod selection_motion;
mod session_icons;
mod session_manager;
mod settings;
mod sftp;
mod sidebar;
mod standalone_connections;
mod tabs;
mod terminal_cast;
mod terminal_command_bar;
mod terminal_command_sender;
mod terminal_context_actions;
mod terminal_cwd;
mod terminal_entity;
mod terminal_git;
mod terminal_project;
mod terminal_triggers_runtime;
mod version_migration;
mod virtual_list;
mod window_intent;
mod window_registry;
mod window_shell;

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
    breadcrumb_scroll::scroll_breadcrumb_by_wheel,
    path_completion::{
        PathCompletionCandidate, PathCompletionOwner, PathCompletionState,
        local_path_completion_request, remote_path_completion_request,
    },
    sidebar::{ContextSidebarPanel, ContextSidebarTool},
    version_migration::VersionMigrationState,
};
use anyhow::Result;
use gpui::{
    AnchoredPositionMode, Animation, AnimationExt, AnyElement, AnyWindowHandle, App, Bounds,
    ClipboardEntry, ClipboardItem, Context, Corner, CursorStyle, Entity, FocusHandle, Focusable,
    FollowMode, Image, ImageFormat, IntoColor, IntoElement, KeyDownEvent, KeyUpEvent,
    ListAlignment, ListState, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, ParentElement, PathPromptOptions, Pixels, Point, Render, RenderImage,
    Rgba, ScrollHandle, ScrollWheelEvent, SharedString, Styled, StyledImage, Subscription, Task,
    TextLayout, Timer, UniformListScrollHandle, Window, anchored, canvas, deferred, div,
    prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_connection_monitor::{
    CompactMonitorRow, ConnectionPoolEntryState, ConnectionPoolEntrySummary,
    ConnectionPoolMonitorStats, DockerActionKind, FilesystemCommandCapability,
    FilesystemEntrySeverity, FilesystemFilter, GpuDevice, GpuProvider, GpuSamplingTask,
    GpuSnapshot, GpuSnapshotStatus, GpuUpdate, LogCommandCapability, LogPreset, MetricsSource,
    MonitorMetricKind, MonitorSectionKind, MonitorValueLevel, PackageCommandCapability,
    PackageFilter, PortCommandCapability, PortFilter, ProcessActionKind, ProcessCommandCapability,
    ProcessFilter, ProcessSort, ProfilerRegistry, ProfilerUpdate, ResourceDockerContainer,
    ResourceDockerStatus, ResourceFilesystemEntry, ResourceFilesystemSnapshot,
    ResourceFilesystemStatus, ResourceLogEntry, ResourceLogSnapshot, ResourceLogStatus,
    ResourceMetrics, ResourcePackageEntry, ResourcePackageSnapshot, ResourcePackageStatus,
    ResourcePortEntry, ResourcePortSnapshot, ResourcePortStatus, ResourceScheduledTask,
    ResourceScheduledTaskSnapshot, ResourceScheduledTaskStatus, ResourceService,
    ResourceServiceStatus, ResourceTmuxPane, ResourceTmuxSession, ResourceTmuxSnapshot,
    ResourceTmuxStatus, ResourceTmuxWindow, ResourceTopProcess, ScheduledTaskActionKind,
    ScheduledTaskCapability, ScheduledTaskFilter, ServiceActionKind, ServiceCommandCapability,
    TmuxActionKind, TmuxCommandCapability, build_docker_action_command,
    build_docker_exec_shell_command, build_docker_follow_logs_command, build_docker_logs_command,
    build_filesystem_diagnostic_command, build_filesystem_snapshot_command,
    build_log_follow_command, build_log_snapshot_command, build_package_inspect_command,
    build_package_snapshot_command, build_port_diagnostic_command, build_port_snapshot_command,
    build_process_action_command, build_scheduled_task_action_command,
    build_scheduled_task_diagnostic_command, build_scheduled_task_logs_command,
    build_scheduled_task_snapshot_command, build_service_action_command,
    build_service_follow_logs_command, build_service_logs_command, build_tmux_action_command,
    build_tmux_attach_command, build_tmux_new_session_command, build_tmux_rename_session_command,
    build_tmux_rename_window_command, build_tmux_send_pane_command, build_tmux_snapshot_command,
    compact_monitor_row_signature, compact_monitor_rows, docker_action_succeeded,
    docker_row_signature, docker_state_label_key, filesystem_attention_label_keys,
    filesystem_entry_severity, filesystem_filter_label_key, filesystem_kind_label_key,
    filesystem_read_only_label_key, filesystem_row_signature, format_bytes,
    gpu_device_row_signature, log_level_label_key, log_preset_label_key, log_row_signature,
    package_filter_label_key, package_row_signature, package_status_label_key, parse_log_snapshot,
    parse_package_snapshot, parse_port_snapshot, percent_level, port_endpoint,
    port_filter_label_key, port_is_risky_exposure, port_row_signature, port_state_label_key,
    process_display_command, process_display_name, process_row_signature, process_state_label_key,
    scheduled_task_active_label_key, scheduled_task_enabled_label_key,
    scheduled_task_filter_label_key, scheduled_task_row_signature, scheduled_task_source_label_key,
    service_action_succeeded, service_enabled_label_key, service_row_signature,
    service_state_label_key, start_gpu_sampling_on, tmux_session_row_signature,
    visible_docker_rows, visible_filesystem_rows, visible_log_rows, visible_package_rows,
    visible_port_rows, visible_process_rows, visible_scheduled_task_rows, visible_service_rows,
    visible_tmux_session_rows,
};
use oxideterm_connections::{
    ConnectionStore, ConnectionTerminalOptions, ConnectionTerminalSessionLogPolicy,
    MoshIpFamily as SavedMoshIpFamily, MoshPredictionMode,
    MoshUdpPortSelection as SavedMoshUdpPortSelection, PrivilegeCredentialKind,
    SaveConnectionRequest, SavedPrivilegeCredential, SshConfigSyncService,
};
use oxideterm_forwarding::{
    ForwardEventDeliverySender, ForwardStatus, ForwardingRegistry, SavedForwardStore,
};
use oxideterm_gpui_platform::{
    rendering::detect_graphics,
    vibrancy::{NativeVibrancyMode, VibrancySupport, apply_window_vibrancy},
    window_opacity::{apply_window_opacity, normalized_window_opacity},
};
use oxideterm_gpui_terminal::{
    BackgroundImageRenderCache, PrivilegePromptMatch, SemanticShellDialect,
    SharedTerminalCommandHistory, SharedTerminalSession, TerminalAutosuggestLabels,
    TerminalBackgroundFit, TerminalBackgroundPreferences, TerminalBroadcastInputKind,
    TerminalCommandSelectionLabels, TerminalContextAction, TerminalHighlightMatchScope,
    TerminalHighlightRenderMode, TerminalHighlightRule as UiHighlightRule,
    TerminalHighlightRuleSetOverride, TerminalInputBroadcaster, TerminalInputInterceptor,
    TerminalInputInterceptorResult, TerminalKittyFileTransmissionLabels, TerminalModemLabels,
    TerminalNotice, TerminalNoticeVariant, TerminalOutputProcessor, TerminalPane,
    TerminalPaneEvent, TerminalPasteLabels, TerminalRecordingState, TerminalRecordingStatus,
    TerminalSearchStatus, TerminalSerialControlLabels, TerminalSessionLogContext,
    TerminalSessionLogLabels, TerminalSessionLogOptions, TerminalSessionLogState,
    TerminalSessionLogStatus, TerminalTmuxLabels, TerminalTrzszLabels,
    TerminalUiPreferenceOverrides, TerminalUiPreferences, TerminalUiTheme,
    TerminalWorkingDirectorySource, detect_custom_privilege_prompt, prune_terminal_session_logs,
    resolved_terminal_semantic_scheme,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_gpui_ui::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView, checkbox,
    modal::{popover_backdrop, set_tauri_backdrop_blur_allowed},
    text_input::{TextInputView, text_input},
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
use oxideterm_plugin_host_api::runtime as plugin_runtime;
use oxideterm_plugin_registry as plugin_host;
use oxideterm_render_policy::{
    DetectedGraphics, EffectiveRenderPolicy, RenderProfile, compute_render_policy,
};
use oxideterm_session_adapter::{
    managed_key_resolver_from_store, reconnect_max_attempts_from_settings,
    reconnect_timing_from_settings, sftp_runtime_settings_from_settings,
    terminal_backspace_sequence_from_connection, terminal_delete_sequence_from_connection,
    terminal_encoding_from_connection,
    terminal_encoding_from_settings as session_terminal_encoding,
};
use oxideterm_settings::{
    AI_SIDEBAR_ABSOLUTE_MAX_WIDTH, AI_SIDEBAR_ABSOLUTE_MIN_WIDTH, BackgroundFit, BackgroundScope,
    CursorStyle as SettingsCursorStyle, FontFamily, FrostedGlassMode, GLOBAL_HIGHLIGHT_RULE_SET_ID,
    HighlightRule, HighlightRuleMatchScope, HighlightRuleRenderMode, Language,
    MAX_TERMINAL_BACKGROUND_OPACITY, MAX_WINDOW_OPACITY, MIN_TERMINAL_BACKGROUND_OPACITY,
    MIN_WINDOW_OPACITY, PersistedSettings, SettingsStore, background_images_directory,
    default_settings_path, ensure_bundled_background_image, list_background_images,
};
use oxideterm_settings_model::{
    AiMcpServerDraft, AiProviderKeyStatusDelivery, SettingsNavigationLayout,
};
use oxideterm_sftp::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, LazyProgressStore, ProgressStore, RemoteRelayDisposition,
    SftpTransferGuard, SftpTransferManager, StoredTransferProgress, TarTransferOptions,
    TransferStrategy, profile_local_directory, tar_download_directory, tar_upload_directory,
};
use oxideterm_ssh::{
    AuthMethod, ConnectionConsumer, ConnectionPoolConfig, ConnectionProgressReporter,
    ConnectionState, ConnectionTraceEvent, ConnectionTraceMode, ConnectionTracePlan,
    ConnectionTraceStage, ConnectionTraceState, ConnectionTraceStatus, DedicatedConnectionLease,
    MAX_RETAINED_RECONNECT_JOBS, ManagedKeyResolver, NodeEventReceiver, NodeEventSubscription,
    NodeId, NodeOrigin, NodeReadiness, NodeRouter, NodeRuntimeStore, NodeState, NodeStateEvent,
    NodeTreeExpansion, NodeTreePersistenceSnapshot, NodeTreeSnapshot, NodeTreeSnapshotNode,
    PhaseResult, ProbeConnectionStatus, ProxyHopConfig, ReconnectForwardRuleSnapshot,
    ReconnectNodeConnectionSnapshot, ReconnectNodeTerminalSnapshot, ReconnectNodeTransferSnapshot,
    ReconnectOrchestratorStore, ReconnectPhase, ReconnectProgress, ReconnectSnapshot,
    SshAlgorithmDiagnosticKind, SshConfig, SshConnectionHandle, SshConnectionRegistry,
    SshPromptHandler, SshTransportClient, TerminalEndpoint,
};
use oxideterm_ssh_launch::{
    NativeConnectionLaunch, TemporaryMoshLaunch, TemporarySshLaunch, TemporaryTelnetLaunch,
};
use oxideterm_terminal::{
    LocalPtyConfig, MoshTerminalConfig, RemoteShellIntegrationStatus, SerialSessionConfig,
    ShellInfo, SshSessionConfig, TelnetSessionConfig, TerminalCommandMarkDetectionSource,
    TerminalCursorShape, TerminalLifecycle, load_local_shell_history_commands, scan_shells,
};
use oxideterm_theme::{
    AppUiColors, TerminalTheme, ThemeTokens, UiDensityProfile, UiMotionProfile, UiRadii,
    theme_by_id,
};
use oxideterm_workspace::{
    ActiveSessionNode, ActiveSessionReadiness, ActiveSessionStatus, MAX_PANES_PER_TAB, PaneId,
    PaneNode, SplitDirection, Tab, TabId, TabKind, TabTitleSource, TerminalSessionId,
    adjusted_split_sizes,
};

use self::actions::SearchBarState;
use self::connection_monitor::{
    ConnectionRuntimeSection, HostToolsEntity, HostToolsEvent, HostToolsMessages,
    HostToolsWindowIntent, HostToolsWindowRequest,
};
use self::file_manager::{FileManagerState, FileManagerWorkspaceEvent};
use self::graphics::GraphicsWorkspaceEntity;
use self::ime::{
    HostToolsPlainTextImeFrame, TextInputAnchorStore, WorkspaceImeDragSelection,
    WorkspaceImeElement, WorkspaceImeSelection, WorkspaceImeTarget,
    active_ime_should_defer_input_key, workspace_ime_target_for_plain_host_tools_input,
};
use self::new_connection::{
    ConnectionFlowEntity, ConnectionFlowEvent, NativeSshPromptHandler, NewConnectionField,
    NewConnectionForm, SavedConnectionPromptAction, SshAuthTab, SshConnectionIntent,
};
use self::onboarding::OnboardingState;
use self::overlay::{
    WorkspaceOverlayConfirmEffect, WorkspaceOverlayConfirmKeyAction, WorkspaceOverlayConfirmKind,
    WorkspaceOverlayEntity, WorkspaceOverlayIntent,
};
use self::pane_tree::SplitDrag;
pub(crate) use self::root::helpers::tokens_from_settings as portable_bootstrap_tokens_from_settings;
use self::root::state::{ReconnectWorkerResult, WorkspaceSshNode, WorkspaceSshNodeEndpoint};
use self::root::{background::*, helpers::*};
use self::session_manager::{SessionManagerState, SessionManagerWorkspaceEvent};
use self::sidebar::AiInlinePanelState;
#[cfg(test)]
use self::sidebar::AiStreamDeliveryEvent;
use self::sidebar::{ActiveSessionSidebarViewMode, SidebarSection};
use self::sidebar::{
    AiCompactionDelivery, AiCompactionDeliverySender, AiStreamDelivery, AiStreamDeliverySender,
    ai_now_ms,
};
use self::tabs::{TabRemovalTransition, TerminalLocation};
use self::terminal_entity::{WorkspaceTerminalEntity, WorkspaceTerminalEvent};
use self::window_intent::WorkspaceWindowIntentEntity;
use crate::{
    CloseOtherTabs, ClosePane, CloseSearch, CloseTab, CommandPalette, Copy, Cut, Find, FindNext,
    FindPrev, FontDecrease, FontIncrease, FontReset, GoToTab1, GoToTab2, GoToTab3, GoToTab4,
    GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewConnection, NewTerminal, NextTab,
    OpenSettings, PaletteAiSidebar, PaletteBroadcast, PaletteCancelReconnect, PaletteCleanupDead,
    PaletteDetachTerminal, PaletteDisconnectAll, PaletteEventLog, PaletteHealthCheck,
    PaletteReconnectAll, PaletteResetPanes, Paste, PrevTab, ShellLauncher, ShowShortcuts,
    SplitHorizontal, SplitNavLeft, SplitNavRight, SplitVertical, SwitchLocaleChinese,
    SwitchLocaleEnglish, SwitchLocaleFrench, SwitchLocaleGerman, SwitchLocaleItalian,
    SwitchLocaleJapanese, SwitchLocaleKorean, SwitchLocalePortugueseBrazil, SwitchLocaleSpanish,
    SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese, TerminalAiPanel, TerminalClearScreen,
    TerminalFreeTypeMode, TerminalRecording, ToggleFullscreen, ToggleSidebar, ZenMode,
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
use oxideterm_gpui_ui::text_input::TextInputAnchor;
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
const SETTINGS_PERCENT_SCALE: f64 = 100.0;
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
const QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT: usize = 0;
const QUICK_COMMAND_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const QUICK_COMMAND_LIST_OVERSCAN: usize = 6;
const DETACHED_LOCAL_TERMINAL_LIST_INITIAL_ITEM_COUNT: usize = 0;
const DETACHED_LOCAL_TERMINAL_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const DETACHED_LOCAL_TERMINAL_LIST_OVERSCAN: usize = 4;
const ACTIVE_SESSION_SIDEBAR_LIST_INITIAL_ITEM_COUNT: usize = 0;
const ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT: f32 = 40.0;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

const CONFIRM_DIALOG_FOOTER_ACTIONS: [ConfirmDialogAction; 2] =
    [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];

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
    TrimNotice { count: usize },
    Message { index: usize, last_assistant: bool },
    BottomSpacer,
}

#[derive(Default)]
struct AiChatMessageSignatureCache {
    conversation_id: Option<String>,
    signatures: HashMap<String, u64>,
}

impl AiChatMessageSignatureCache {
    fn select_conversation(&mut self, conversation_id: &str) {
        if self.conversation_id.as_deref() == Some(conversation_id) {
            return;
        }
        self.conversation_id = Some(conversation_id.to_string());
        self.signatures.clear();
    }

    fn signature_for(&mut self, message_id: &str, compute: impl FnOnce() -> u64) -> u64 {
        if let Some(signature) = self.signatures.get(message_id) {
            return *signature;
        }
        let signature = compute();
        self.signatures.insert(message_id.to_string(), signature);
        signature
    }

    fn invalidate_message(&mut self, message_id: &str) {
        self.signatures.remove(message_id);
    }

    fn invalidate_all(&mut self) {
        self.signatures.clear();
    }

    fn needs_prune(&self, retained_count: usize) -> bool {
        self.signatures.len() > retained_count.saturating_add(32)
    }

    fn prune(&mut self, retained_message_ids: &HashSet<&str>) {
        self.signatures
            .retain(|message_id, _| retained_message_ids.contains(message_id.as_str()));
    }
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
    request_configuration_fingerprint: u64,
}

#[derive(Default)]
struct AiContextTokenBreakdownCache {
    key: Option<AiContextTokenBreakdownKey>,
    breakdown_without_draft: Option<AiContextTokenBreakdown>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AiPreparedPromptUsage {
    conversation_id: String,
    last_user_message_id: Option<String>,
    provider_id: String,
    model: String,
    breakdown: AiContextTokenBreakdown,
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct TabRenameDialog {
    // The dialog edits display metadata for one canonical terminal tab only.
    tab_id: TabId,
    draft: String,
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
pub(in crate::workspace) enum TabCloseConfirm {
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
    drag: Option<TabDragState>,
    context_menu: Option<TabContextMenu>,
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
            drag: None,
            context_menu: None,
            exiting_tabs: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            scrollbar_drag: None,
            scrollbar_hovered: false,
        }
    }
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
    main_window_tabs: WorkspaceWindowTabState,
    tab_rename_dialog: Option<TabRenameDialog>,
    detached_tab_return_drag: Option<DetachedTabReturnDrag>,
    detached_tab_return_handoff: Option<DetachedTabReturnHandoff>,
    next_tab_window_handoff_generation: u64,
    main_window_tabbar_drop_bounds: Option<Bounds<Pixels>>,
    pending_auto_close_terminal_sessions: HashSet<TerminalSessionId>,
    auto_close_terminal_sessions_scheduled: bool,
    tab_host: Entity<tabs::WorkspaceTabHostEntity>,
    _tab_host_subscription: Subscription,
    search: SearchBarState,
    terminal_recording_menu_open: bool,
    terminal_highlight_popover_open: bool,
    // Settings keep the source pane stable while editing session-only trigger overrides.
    terminal_trigger_settings_pane: Option<PaneId>,
    terminal_trigger_shell_confirmation_pending: bool,
    terminal_triggers: settings::TerminalTriggersSettingsState,
    terminal_trigger_runtime: terminal_triggers_runtime::TerminalTriggerRuntimeState,
    // Runtime panes share one stable saved-profile identity index across triggers and broadcasts.
    terminal_saved_connection_refs:
        HashMap<TerminalSessionId, oxideterm_terminal_triggers::SavedConnectionRef>,
    terminal_semantic_highlight_section_expanded: bool,
    terminal_rule_highlight_section_expanded: bool,
    terminal_command_context_highlight_section_expanded: bool,
    terminal_command_sender: Entity<terminal_command_sender::TerminalCommandSenderEntity>,
    _terminal_command_sender_observation: Subscription,
    local_terminal_command_history: SharedTerminalCommandHistory,
    // Runtime history follows NodeRouter identity without owning or prolonging the transport.
    ssh_terminal_command_histories: HashMap<NodeId, SharedTerminalCommandHistory>,
    detached_local_terminals: HashMap<TerminalSessionId, DetachedLocalTerminalSession>,
    detached_local_terminal_order: Vec<TerminalSessionId>,
    serial_terminal_configs: HashMap<TerminalSessionId, SerialSessionConfig>,
    // A Telnet pane keeps only the stable profile owner needed for toolbar persistence.
    telnet_terminal_profile_ids: HashMap<TerminalSessionId, String>,
    // Non-SSH connection records outlive their current terminal or desktop surface.
    standalone_connections: standalone_connections::StandaloneConnectionRegistry,
    detached_local_terminals_popover_open: bool,
    command_palette: Entity<command_palette::CommandPaletteEntity>,
    _command_palette_observation: Subscription,
    version_migration: VersionMigrationState,
    onboarding: OnboardingState,
    shortcuts_modal: ShortcutsModalState,
    settings_workspace: Entity<settings::SettingsWorkspaceEntity>,
    _settings_workspace_observation: Subscription,
    _settings_workspace_subscription: Subscription,
    segmented_control_user_motion: selection_motion::UserSegmentedControlMotionState,
    // Prompt and memory documents are edited outside the virtual settings list.
    ai_text_editor_dialog: Option<settings::AiTextEditorDialog>,
    ai_text_editor: Option<Entity<oxideterm_gpui_editor::TextEditorView>>,
    detached_local_terminal_list_state: ListState,
    detached_local_terminal_list_cache: RefCell<VirtualListSignatureCache>,
    plugin_entity: Entity<plugin_entity::PluginWorkspaceEntity>,
    _plugin_entity_subscription: Subscription,
    split_drag: Option<SplitDrag>,
    sidebar_resizing: bool,
    embedded_sftp_sidebar_resizing: bool,
    sidebar_resize_hotzone_hovered: bool,
    sidebar_collapsed: bool,
    sidebar_rendered: bool,
    sidebar_motion_generation: u64,
    sidebar_width: f32,
    context_sidebar_rendered: bool,
    context_sidebar_motion_generation: u64,
    ai_entity: Entity<ai_state::AiWorkspaceEntity>,
    acp_entity: Entity<acp_workspace::AcpWorkspaceEntity>,
    skill_registry: std::sync::Arc<parking_lot::RwLock<oxideterm_skills::SkillRegistry>>,
    skill_workspace_root: Option<std::path::PathBuf>,
    loaded_conversation_skills: HashMap<String, HashMap<String, String>>,
    ai_background_tasks: Entity<ai_background_tasks::AiBackgroundTaskEntity>,
    _ai_background_tasks_subscription: Subscription,
    ai_runtime_context: Entity<ai_runtime_context::AiRuntimeContextEntity>,
    _ai_entity_subscription: Subscription,
    _acp_entity_subscription: Subscription,
    active_context_sidebar_panel: ContextSidebarPanel,
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
    standard_confirm_focused_action: Option<ConfirmDialogAction>,
    skip_future_ssh_close_confirmations: bool,
    select_anchors: HashMap<SelectAnchorId, OverlayAnchor>,
    text_input_anchors: TextInputAnchorStore,
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
    workspace_input: Entity<ime::WorkspaceInputEntity>,
    _workspace_input_observation: Subscription,
    input_caret: ime::WorkspaceCaretVisibility,
    native_update_notification_open: bool,
    native_update_notification_presence: oxideterm_gpui_ui::motion::ExitPresence,
    native_update_release_notes_scroll: MarkdownVirtualListScrollHandle,
    settings_legal_notice_scroll: MarkdownVirtualListScrollHandle,
    _window_intents: Entity<WorkspaceWindowIntentEntity>,
    _window_intent_subscription: Subscription,
    _window_button_layout_subscription: Subscription,
    window_registry: window_registry::WorkspaceWindowRegistry,
    window_effect_delivery_scheduled: bool,
    connection_flow: Entity<ConnectionFlowEntity>,
    _connection_flow_observation: Subscription,
    _connection_flow_subscription: Subscription,
    workspace_runtime: Entity<runtime_entity::WorkspaceRuntimeEntity>,
    _workspace_runtime_subscription: Subscription,
    public_mcp: public_mcp::PublicMcpWorkspaceBridge,
    ssh_registry: SshConnectionRegistry,
    forwarding_service: forwards::ForwardingRuntimeService,
    forwarding_runtime: Arc<tokio::runtime::Runtime>,
    sftp_transfer_manager: Arc<SftpTransferManager>,
    sftp_progress_store: Arc<dyn ProgressStore>,
    node_router: NodeRouter,
    notification_center: NotificationCenterState,
    notification_sidebar_list_state: ListState,
    notification_sidebar_list_cache: RefCell<VirtualListSignatureCache>,
    event_log_sidebar_scroll_handle: UniformListScrollHandle,
    ssh_nodes: HashMap<NodeId, WorkspaceSshNode>,
    saved_ssh_nodes: HashMap<String, NodeId>,
    expanded_ssh_nodes: HashSet<NodeId>,
    active_ssh_node_id: Option<NodeId>,
    next_ssh_node_id: u64,
    forwarding: Entity<forwards::ForwardingWorkspaceEntity>,
    _forwarding_subscriptions: Vec<Subscription>,
    file_manager: Entity<FileManagerState>,
    _file_manager_observation: Subscription,
    _file_manager_subscription: Subscription,
    sftp_tab_nodes: HashMap<TabId, NodeId>,
    standalone_sftp_tabs: HashMap<TabId, sftp::StandaloneSftpTabBinding>,
    standalone_sftp_sessions: HashMap<String, sftp::StandaloneSftpRuntime>,
    dedicated_sftp_connections:
        Arc<parking_lot::Mutex<HashMap<NodeId, sftp::DedicatedSftpConnectionSlot>>>,
    ssh_consumer_prompt_handler: Arc<dyn SshPromptHandler>,
    ssh_consumer_managed_key_resolver: ManagedKeyResolver,
    pending_standalone_sftp_pair_launches:
        HashMap<String, new_connection::PendingStandaloneSftpPairLaunch>,
    embedded_sftp_node_id: Option<NodeId>,
    sftp_presentation_request: Option<sftp::SftpPresentationRequest>,
    ide_workspace: Entity<ide::IdeWorkspaceEntity>,
    _ide_workspace_subscription: Subscription,
    sftp_view: Entity<sftp::SftpWorkspaceEntity>,
    _sftp_observation: Subscription,
    _sftp_subscription: Subscription,
    graphics: Entity<GraphicsWorkspaceEntity>,
    _graphics_observation: Subscription,
    _graphics_subscription: Subscription,
    host_tools: Entity<HostToolsEntity>,
    _host_tools_subscription: Subscription,
    cloud_sync: Entity<cloud_sync::CloudSyncWorkspaceEntity>,
    _cloud_sync_observation: Subscription,
    _cloud_sync_subscription: Subscription,
    i18n: I18n,
    tokens: ThemeTokens,
    detected_graphics: DetectedGraphics,
    render_profile_override: Option<RenderProfile>,
    render_policy: EffectiveRenderPolicy,
    vibrancy_support: VibrancySupport,
    app_lock: app_lock::AppLockState,
    settings_store: SettingsStore,
    pending_window_ui_state: Option<oxideterm_settings::WindowUiState>,
    window_state_save_task: Option<Task<()>>,
    connection_store: ConnectionStore,
    // The connection-layer worker owns SSH config parsing and persistence.
    ssh_config_sync_service: Option<SshConfigSyncService>,
    session_manager: Entity<SessionManagerState>,
    _session_manager_observation: Subscription,
    _session_manager_subscription: Subscription,
    remote_desktop: Entity<remote_desktop::RemoteDesktopWorkspaceEntity>,
    remote_desktop_resize_menu_tab_id: Option<TabId>,
    local_shells: Vec<ShellInfo>,
    local_shell_launcher_open: bool,
    local_shell_launcher_selected_id: Option<String>,
    terminal: Entity<WorkspaceTerminalEntity>,
    _terminal_subscription: Subscription,
    overlay: Entity<WorkspaceOverlayEntity>,
    _overlay_observation: Subscription,
}

impl Drop for WorkspaceApp {
    fn drop(&mut self) {
        // App Lock and Cloud Sync move the focused secret into this window IME
        // adapter, so window destruction must zeroize it even without a blur event.
        zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
        // WorkspaceApp owns the shared session runtime. Window, tab, and page
        // release must not stop transfers or tunnels; final owner drop must.
        self.shutdown_final_session_services();
    }
}

pub(crate) use window_shell::WorkspaceWindowShell;

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

// Suggestion values can contain credentials embedded in commands, so diagnostics must not format them.
#[allow(dead_code)]
#[derive(Clone)]
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

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSuggestionKind {
    History,
    Command,
    Subcommand,
    Option,
    File,
    Directory,
}

#[derive(Clone)]
pub(crate) struct AiRuntimeCommandRecord {
    pub(crate) command_id: String,
    pub(crate) command: String,
    pub(crate) cwd: Option<String>,
    pub(crate) source: String,
    pub(crate) status: String,
    pub(crate) exit_code: Option<i64>,
    pub(crate) started_at: i64,
    pub(crate) finished_at: Option<i64>,
    pub(crate) approval_mode: Option<String>,
    pub(crate) risk: String,
}

impl std::fmt::Debug for AiRuntimeCommandRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Command text and working directories can contain credentials.
        formatter
            .debug_struct("AiRuntimeCommandRecord")
            .field("command_id", &self.command_id)
            .field("command", &"[redacted]")
            .field("cwd", &self.cwd.as_ref().map(|_| "[redacted]"))
            .field("source", &self.source)
            .field("status", &self.status)
            .field("exit_code", &self.exit_code)
            .field("started_at", &self.started_at)
            .field("finished_at", &self.finished_at)
            .field("approval_mode", &self.approval_mode)
            .field("risk", &self.risk)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AiToolExecutionRecord {
    pub(crate) record_id: String,
    pub(crate) conversation_id: String,
    pub(crate) assistant_message_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) tool_name: String,
    pub(crate) argument_summary: String,
    pub(crate) resource_kind: Option<oxideterm_ai::StableResourceKind>,
    pub(crate) target_kind: Option<String>,
    pub(crate) risk: String,
    pub(crate) approval_source: Option<String>,
    pub(crate) execution_surface: String,
    pub(crate) visible_in_terminal: Option<bool>,
    pub(crate) status: String,
    pub(crate) success: Option<bool>,
    pub(crate) error_code: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) started_at: i64,
    pub(crate) finished_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub(crate) struct AiToolResultFact {
    pub(crate) fact_id: String,
    pub(crate) conversation_id: String,
    pub(crate) assistant_message_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) tool_name: String,
    pub(crate) source_kind: String,
    pub(crate) summary: String,
    pub(crate) created_at: i64,
}

#[derive(Clone)]
pub(crate) struct AiCliAgentSession {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) status: String,
    pub(crate) live_terminal: bool,
    pub(crate) started_at: i64,
    pub(crate) updated_at: i64,
}

impl std::fmt::Debug for AiCliAgentSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Agent launch commands share the same secret-bearing boundary.
        formatter
            .debug_struct("AiCliAgentSession")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("label", &self.label)
            .field("status", &self.status)
            .field("live_terminal", &self.live_terminal)
            .field("started_at", &self.started_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

#[derive(Clone)]
struct DetachedLocalTerminalSession {
    session_id: TerminalSessionId,
    title: String,
    session: SharedTerminalSession,
    detached_at: Instant,
    buffer_lines: usize,
}

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_SESSION_TREE_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
