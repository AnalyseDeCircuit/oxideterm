mod acp_workspace;
mod actions;
use oxideterm_gpui_ai::background as ai_background_tasks;
mod ai_runtime_context;
use oxideterm_gpui_ai::state as ai_state;
use ai_views::AiWorkspaceViews;
mod ai_views;
mod app_lock;
mod audit;
mod background_tasks;
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
mod history_quit;
pub(crate) use history_quit::request_app_quit;
mod acp_plugins;
mod ide;
mod ime;
mod knowledge;
mod local_sessions;
mod local_shell_launcher;
mod local_terminal_background;
mod new_connection;
mod notification_center;
mod onboarding;
mod overlay;
mod pane_tree;
mod path_completion;
mod plugin_connections;
mod plugin_entity;
mod plugin_lifecycle;
mod plugin_manager;
mod plugin_preview;
mod plugin_toolbar;
mod plugin_ui;
mod public_mcp;
mod quick_commands;
mod remote_desktop;
mod runtime_entity;
mod root {
    pub(super) mod runtime_settings;
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
mod disclosure_motion;
mod selectable_text;
mod selection_motion;
mod session_icons;
mod session_manager;
mod settings;
mod sftp;
mod sidebar;
mod ssh_tunnels;
mod standalone_connections;
mod tabs;
mod terminal_archive;
mod terminal_cast;
mod terminal_command_bar;
mod terminal_command_sender;
mod terminal_context_actions;
mod terminal_cwd;
mod terminal_entity;
mod terminal_git;
mod terminal_project;
mod terminal_sync_groups;
mod terminal_triggers_runtime;
mod version_migration;
mod virtual_list;
mod window_intent;
mod window_registry;
mod window_shell;
mod zen_exit;

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
    ClipboardItem, Context, Corner, CursorStyle, Entity, FocusHandle, Focusable, FollowMode,
    FontWeight, Image, IntoElement, KeyDownEvent, KeyUpEvent, ListAlignment, ListState,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    ParentElement, PathPromptOptions, Pixels, Point, Render, RenderImage, Rgba, ScrollEvent,
    ScrollHandle, SharedString, Styled, StyledImage, Subscription, Task, TextLayout, Timer,
    UniformListScrollHandle, Window, anchored, canvas, deferred, div, prelude::*, px, relative,
    rgb, rgba, svg,
};
use oxideterm_connection_monitor::{ConnectionPoolEntryState, ConnectionPoolEntrySummary};
use oxideterm_connections::{
    ConnectionStore, ConnectionTerminalOptions, ConnectionTerminalSessionLogPolicy,
    MoshIpFamily as SavedMoshIpFamily, MoshPredictionMode,
    MoshUdpPortSelection as SavedMoshUdpPortSelection, PrivilegePromptType, PrivilegeResponseMode,
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
    TerminalBackgroundPreferences, TerminalBroadcastInputKind,
    TerminalCommandSelectionLabels, TerminalContextAction, TerminalHighlightMatchScope,
    TerminalHighlightRenderMode, TerminalHighlightRule as UiHighlightRule,
    TerminalHighlightRuleSetOverride, TerminalHistoryStore, TerminalInputBroadcaster,
    TerminalInputInterceptor, TerminalInputInterceptorResult, TerminalKittyFileTransmissionLabels,
    TerminalModemLabels, TerminalNotice, TerminalNoticeVariant, TerminalOutputProcessor,
    TerminalPane, TerminalPaneEvent, TerminalPasteLabels, TerminalRecordingState,
    TerminalRecordingStatus, TerminalSearchStatus, TerminalSerialControlLabels,
    TerminalSessionLogContext, TerminalSessionLogLabels, TerminalSessionLogOptions,
    TerminalSessionLogState, TerminalSessionLogStatus, TerminalTmuxLabels,
    TerminalToolbarActionSpec, TerminalTrzszLabels, TerminalUiPreferenceOverrides,
    TerminalUiPreferences, TerminalUiTheme, TerminalWorkingDirectorySource,
    detect_custom_privilege_prompt, prune_terminal_session_logs, resolved_terminal_semantic_scheme,
};
use oxideterm_gpui_ui::scroll::ScrollableElement;
use oxideterm_gpui_ui::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView, MaterialRole, checkbox,
    material_surface,
    modal::{popover_backdrop, set_tauri_backdrop_blur_allowed},
    text_input::TextInputView,
    toast::{ToastVariant, ToastView, toast_close},
    toaster::toaster,
    tooltip::tooltip_surface,
};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_ide_fs::NodeAgentIdeFileSystem;
use oxideterm_notification_center::{
    ActivityView as WorkspaceActivityView, EventCategory as WorkspaceEventCategory,
    EventSeverity as WorkspaceEventSeverity, NotificationCenterState,
    NotificationEntry as WorkspaceNotificationEntry, NotificationKind as WorkspaceNotificationKind,
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
    AI_SIDEBAR_ABSOLUTE_MAX_WIDTH, AI_SIDEBAR_ABSOLUTE_MIN_WIDTH, BackgroundScope,
    CursorStyle as SettingsCursorStyle, FrostedGlassMode, GLOBAL_HIGHLIGHT_RULE_SET_ID,
    HighlightRule, HighlightRuleMatchScope, HighlightRuleRenderMode, Language,
    PersistedSettings, SettingsStore, background_images_directory,
    default_settings_path, ensure_bundled_background_image, list_background_images,
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
    BleGattHandle, BleSessionConfig, LocalPtyConfig, MoshTerminalConfig,
    RemoteShellIntegrationStatus, SerialSessionConfig, ShellInfo, SshSessionConfig,
    TelnetSessionConfig, TerminalCommandMarkDetectionSource, TerminalCursorShape,
    TerminalLifecycle, load_local_shell_history_commands, scan_shells,
};
use oxideterm_theme::{
    AppUiColors, ThemeTokens, UiDensityProfile, UiMotionProfile, UiRadii,
};
use oxideterm_workspace::{
    ActiveSessionNode, ActiveSessionReadiness, ActiveSessionStatus, MAX_PANES_PER_TAB, PaneId,
    PaneNode, SplitDirection, Tab, TabId, TabKind, TabTitleSource, TerminalSessionId,
    adjusted_split_sizes,
};

use self::connection_monitor::{
    ConnectionRuntimeSection, HostToolsEntity, HostToolsEvent, HostToolsMessages,
    HostToolsWindowIntent, HostToolsWindowRequest,
};
use oxideterm_gpui_file_manager::FileManagerViews;
use self::graphics::GraphicsWorkspaceEntity;
use self::ime::{
    TextInputAnchorStore, WorkspaceImeDragSelection, WorkspaceImeElement, WorkspaceImeSelection,
    WorkspaceImeTarget, active_ime_should_defer_input_key,
    workspace_ime_target_for_plain_host_tools_input,
};
use self::knowledge::KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT;
use self::new_connection::{
    ConnectionFlowEvent, NativeSshPromptHandler, NewConnectionField,
    NewConnectionForm, SavedConnectionPromptAction, SshConnectionIntent,
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
use self::session_manager::{
    SessionManagerInput, SessionManagerWorkspaceEvent,
};
use self::sidebar::{ActiveSessionSidebarViewMode, SidebarSection};
use self::tabs::{TabRemovalTransition, TerminalLocation};
use self::terminal_entity::{WorkspaceTerminalEntity, WorkspaceTerminalEvent};
use self::window_intent::WorkspaceWindowIntentEntity;
use crate::{
    CloseOtherTabs, ClosePane, CloseSearch, CloseTab, CommandPalette, Copy, Cut, Find, FindNext,
    FindPrev, FontDecrease, FontIncrease, FontReset, GoToTab1, GoToTab2, GoToTab3, GoToTab4,
    GoToTab5, GoToTab6, GoToTab7, GoToTab8, GoToTab9, NewConnection, NewTerminal, NextTab,
    OpenSettings, PaletteAiSidebar, PaletteBroadcast, PaletteCancelReconnect, PaletteCleanupDead,
    PaletteDetachTerminal, PaletteDisconnectAll, PaletteEventLog, PaletteHealthCheck,
    PaletteReconnectAll, PaletteResetPanes, Paste, PrevTab, Quit, ShellLauncher, ShowShortcuts,
    SplitHorizontal, SplitNavLeft, SplitNavRight, SplitVertical, SwitchLocaleChinese,
    SwitchLocaleEnglish, SwitchLocaleFrench, SwitchLocaleGerman, SwitchLocaleItalian,
    SwitchLocaleJapanese, SwitchLocaleKorean, SwitchLocalePortugueseBrazil, SwitchLocaleSpanish,
    SwitchLocaleTraditionalChinese, SwitchLocaleVietnamese, TerminalAiPanel, TerminalClearScreen,
    TerminalFreeTypeMode, TerminalRecording, ToggleFullscreen, ToggleSidebar, ZenMode,
};
use crate::{assets::LucideIcon, bundled_fonts};
use oxideterm_gpui_markdown::{
    MarkdownCodeBlockActions, MarkdownDocument, MarkdownMermaidZoomHandler,
    MarkdownOptions, MarkdownVirtualListScrollHandle,
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
use oxideterm_gpui_ui::typography::tauri_ui_font_family as settings_ui_font_family;
pub(super) use selectable_text::{
    SelectableTextRole, SelectableTextScrollExt,
};
pub(super) use virtual_list::{
    TauriVirtualListSpec, TauriVirtualScrollAlign, scroll_tauri_virtual_list_to_index,
    tauri_virtual_list, tauri_virtual_list_state, tauri_virtual_uniform_list,
    uniform_list_edge_autoscroll,
};
use virtual_list::{
    VirtualListSignatureCache, sync_tauri_variable_list_state_by_signatures,
};

const SETTINGS_SCROLL_CARET_PAUSE_MS: u64 = 700;
const QUICK_COMMAND_LIST_INITIAL_ITEM_COUNT: usize = 0;
const QUICK_COMMAND_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const QUICK_COMMAND_LIST_OVERSCAN: usize = 6;
const DETACHED_LOCAL_TERMINAL_LIST_INITIAL_ITEM_COUNT: usize = 0;
const DETACHED_LOCAL_TERMINAL_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const DETACHED_LOCAL_TERMINAL_LIST_OVERSCAN: usize = 4;
const ACTIVE_SESSION_SIDEBAR_LIST_INITIAL_ITEM_COUNT: usize = 0;
const ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT: f32 = 40.0;
const ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN: usize = 8;

use oxideterm_gpui_ai::state::AiChatFooterAction;

// AI composer footer uses the same explicit action list as dialog footers so
// keyboard focus order stays centralized even though it is not a modal trap.
const AI_CHAT_FOOTER_ACTIONS: [AiChatFooterAction; 1] = [AiChatFooterAction::Submit];

const CONFIRM_DIALOG_FOOTER_ACTIONS: [ConfirmDialogAction; 2] =
    [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];

use oxideterm_gpui_markdown::MarkdownPreviewCache;

// Tauri NotificationsPanel uses variable-height grouped rows. Keep the native
// estimate/overscan as a virtual-list spec instead of a raw overdraw number so
// notification/event-log surfaces share the same browser virtualizer contract.
const NOTIFICATION_SIDEBAR_ROW_HEIGHT_ESTIMATE: f32 = 72.0;
const NOTIFICATION_SIDEBAR_VIRTUAL_OVERSCAN: usize = 10;
use oxideterm_gpui_ai::state::AiChatMessageSignatureCache;

use oxideterm_gpui_ai::state::{AiContextTokenBreakdown, AiContextTokenBreakdownKey, AiPreparedPromptUsage};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConfirmKeyboardAction {
    Cancel,
    Confirm,
    Handled,
}

#[derive(Debug)]
struct ShortcutsModalState {
    open: bool,
    presence: oxideterm_gpui_ui::motion::ExitPresence,
    motion_generation: usize,
    exit_task: Option<Task<()>>,
    query: String,
    scroll_handle: UniformListScrollHandle,
}

use oxideterm_gpui_ai::state::AiModelSelectorScope;

#[derive(Clone, Debug, Eq, PartialEq)]
enum TabDragMode {
    Pending,
    Reorder,
    Detach,
    Content,
}

#[derive(Clone, Debug)]
struct TabDragState {
    tab_id: TabId,
    from_index: usize,
    destination_tab: Option<TabId>,
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
    tab_widths: Vec<f32>,
    last_edge_scroll_frame: Option<Instant>,
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
    started_at: Instant,
    duration: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TabCloseConfirm {
    ApplicationQuit,
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
    tab_text_width_cache: RefCell<HashMap<(String, u32, String), f32>>,
}

#[derive(Clone, Copy, Debug)]
struct TabbarScrollbarDragState {
    // Preserve the pointer's position inside the thumb to prevent a jump on drag start.
    grab_offset_x: f32,
}

#[derive(Clone, Copy, Debug)]
struct DetachedTabReturnDrag {
    tab_id: TabId,
    source_bounds: Bounds<Pixels>,
    native_window_move: bool,
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

impl WorkspaceWindowTabState {
    fn new() -> Self {
        Self {
            drag: None,
            context_menu: None,
            exiting_tabs: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            scrollbar_drag: None,
            scrollbar_hovered: false,
            tab_text_width_cache: RefCell::new(HashMap::new()),
        }
    }
}

#[derive(Clone)]
pub(super) struct SelectableTextFragmentState {
    pub join_previous: bool,
    pub group_id: u64,
    pub order: usize,
    pub generation: u64,
    pub text: String,
    pub layout: TextLayout,
    pub anchor: TextInputAnchor,
}

#[derive(Clone, Copy)]
pub(super) struct ThemeTransitionState {
    pub from: ThemeTokens,
    pub to: ThemeTokens,
    pub started: std::time::Instant,
    pub duration: Duration,
}

pub(crate) struct WorkspaceApp {
    focus_handle: FocusHandle,
    main_window_tabs: WorkspaceWindowTabState,
    tab_rename_dialog: Option<TabRenameDialog>,
    detached_tab_return_drag: Option<DetachedTabReturnDrag>,
    detached_tab_return_handoff: Option<DetachedTabReturnHandoff>,
    next_tab_window_handoff_generation: u64,
    main_window_tabbar_drop_bounds: Option<Bounds<Pixels>>,
    split_drop_regions: Rc<RefCell<Vec<tabs::split_drop::SplitDropRegion>>>,
    split_drop_target: Option<tabs::split_drop::SplitDropTarget>,
    pending_auto_close_terminal_sessions: HashSet<TerminalSessionId>,
    auto_close_terminal_sessions_scheduled: bool,
    tab_host: Entity<tabs::WorkspaceTabHostEntity>,
    _tab_host_subscription: Subscription,
    search: actions::TerminalSearchState,
    terminal_recording_menu_open: bool,
    privilege_credential_picker: Option<terminal_command_bar::PrivilegeCredentialPicker>,
    pending_privilege_totp: Option<terminal_command_bar::PendingPrivilegeTotp>,
    privilege_prompt_sync_task: Option<Task<()>>,
    terminal_highlight_popover_open: bool,
    // Settings keep the source pane stable while editing session-only trigger overrides.
    terminal_trigger_runtime: terminal_triggers_runtime::TerminalTriggerRuntimeState,
    // Runtime panes share one stable saved-profile identity index across triggers and broadcasts.
    terminal_saved_connection_refs:
        HashMap<TerminalSessionId, oxideterm_terminal_triggers::SavedConnectionRef>,
    terminal_semantic_highlight_section_expanded: bool,
    terminal_rule_highlight_section_expanded: bool,
    terminal_command_context_highlight_section_expanded: bool,
    terminal_selection_highlight_section_expanded: bool,
    terminal_command_sender: Entity<terminal_command_sender::TerminalCommandSenderEntity>,
    _terminal_command_sender_observation: Subscription,
    local_terminal_command_history: SharedTerminalCommandHistory,
    terminal_history_store: TerminalHistoryStore,
    _terminal_history_updates: Task<()>,
    terminal_history_all_connections: bool,
    // Active nodes retain history handles without owning or prolonging the transport.
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
    onboarding_view: Entity<oxideterm_gpui_onboarding::OnboardingView>,
    shortcuts_modal: ShortcutsModalState,
    settings_workspace: Entity<settings::SettingsWorkspaceEntity>,
    settings_views: oxideterm_gpui_settings_view::views::SettingsViews,
    _settings_workspace_subscription: Subscription,
    segmented_control_user_motion: selection_motion::UserSegmentedControlMotionState,
    // Prompt and memory documents are edited outside the virtual settings list.
    detached_local_terminal_list_state: ListState,
    detached_local_terminal_list_cache: RefCell<VirtualListSignatureCache>,
    plugin_entity: Entity<plugin_entity::PluginWorkspaceEntity>,
    // Shared behind a mutex so plugin host-call resolvers resolve handles per
    // call instead of holding a frozen activation-time snapshot.
    ble_gatt_handles: Arc<std::sync::Mutex<HashMap<TerminalSessionId, BleGattHandle>>>,
    ble_notification_router: plugin_lifecycle::BleNotificationRouter,
    _plugin_entity_subscription: Subscription,
    split_drag: Option<SplitDrag>,
    disclosure_motions: disclosure_motion::DisclosureMotions,
    sidebar_resizing: bool,
    embedded_sftp_sidebar_resizing: bool,
    zen_exit: zen_exit::ZenExitAffordance,
    sidebar_collapsed: bool,
    sidebar_rendered: bool,
    sidebar_motion_generation: u64,
    sidebar_section_motion_generation: u64,
    sidebar_section_transition: Option<(SidebarSection, u64)>,
    sidebar_motion: oxideterm_gpui_ui::motion::SidebarMotion,
    sidebar_width: f32,
    context_sidebar_rendered: bool,
    context_sidebar_motion_generation: u64,
    context_sidebar_motion: oxideterm_gpui_ui::motion::SidebarMotion,
    ai: AiWorkspaceViews,
    mosh_plugin_sessions: Arc<oxideterm_mosh::MoshPluginSessions>,
    // Panes whose plugin toolbar state currently declares `overlay`; one
    // member per pane, in-memory only, never persisted. The single physical
    // card binds to whichever member is the active pane; the binding is
    // derived from focus, never stored. The host owns the binding
    // transitions, the render gate, and the card chrome, while the plugin
    // owns the card content and refresh semantics.
    plugin_overlay_members: HashMap<PaneId, plugin_toolbar::PluginOverlayAnchor>,
    // Monotonic counter feeding each claimed anchor's generation so retired
    // poll loops can detect that a newer claim replaced theirs.
    plugin_overlay_generation: u64,
    // Authoritative per-pane toolbar state for plugin-contributed chips:
    // (plugin id, action id, pane id) -> state id. A missing entry means the
    // action's declared default state; the pane's toolbar specs are only a
    // render mirror pushed from here. Entries exist for open panes only.
    plugin_toolbar_states: HashMap<(String, String, PaneId), String>,
    active_context_sidebar_panel: ContextSidebarPanel,
    needs_active_pane_focus: bool,
    active_sidebar_section: SidebarSection,
    active_surface: ActiveSurface,
    active_session_sidebar_view_mode: ActiveSessionSidebarViewMode,
    session_sort_menu_open: bool,
    context_tools_menu: Option<ContextSidebarTool>,
    context_tools_menu_scroll: ScrollHandle,
    session_search_open: bool,
    session_search_query: String,
    // Session manager sidebar search disclosure and sort menu open state.
    // These are sidebar-only flags so the tab's toolbar menus don't conflict.
    session_manager_search_open: bool,
    session_manager_sidebar_sort_open: bool,
    // Set only across the tabbar's "show in sidebar" handoff: it closes the
    // Session Manager tab and the sidebar form takes over, so
    // finish_tab_removal must keep the panel on Connections instead of
    // parking the sidebar on Active Sessions.
    session_manager_handoff_to_sidebar: bool,
    active_session_sidebar_list_state: ListState,
    active_session_sidebar_list_cache: RefCell<VirtualListSignatureCache>,
    open_settings_select: Option<SettingsSelect>,
    // A root-mounted select portal must only use the trigger geometry from the
    // native window that opened it.
    open_settings_select_owner_window_id: Option<gpui::WindowId>,
    settings_select_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    standard_confirm_focused_action: Option<ConfirmDialogAction>,
    skip_future_ssh_close_confirmations: bool,
    select_anchors: HashMap<SelectAnchorId, OverlayAnchor>,
    settings_select_anchors: HashMap<(gpui::WindowId, SelectAnchorId), OverlayAnchor>,
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
    // The large command-spec document and highlight/semantic rule editors live
    // in workspace modals so the settings virtual list stays compact and single-scroll.
    workspace_input: Entity<ime::WorkspaceInputEntity>,
    _workspace_input_observation: Subscription,
    input_caret: ime::WorkspaceCaretVisibility,
    _window_intents: Entity<WorkspaceWindowIntentEntity>,
    _window_intent_subscription: Subscription,
    _window_button_layout_subscription: Subscription,
    window_registry: window_registry::WorkspaceWindowRegistry,
    window_effect_delivery_scheduled: bool,
    connections: oxideterm_gpui_connections::views::ConnectionsViews,
    _connection_flow_subscription: Subscription,
    workspace_runtime: Entity<runtime_entity::WorkspaceRuntimeEntity>,
    _workspace_runtime_subscription: Subscription,
    public_mcp: public_mcp::PublicMcpWorkspaceBridge,
    ssh_registry: SshConnectionRegistry,
    forwarding_service: forwards::ForwardingRuntimeService,
    forwarding_runtime: Arc<tokio::runtime::Runtime>,
    sftp_transfer_manager: Arc<SftpTransferManager>,
    background_transfers: Vec<background_tasks::TransferSummary>,
    _background_transfer_observation: Task<()>,
    sftp_progress_store: Arc<dyn ProgressStore>,
    node_router: NodeRouter,
    notification_center: NotificationCenterState,
    notification_sidebar_list_state: ListState,
    notification_sidebar_list_cache: RefCell<VirtualListSignatureCache>,
    ssh_nodes: HashMap<NodeId, WorkspaceSshNode>,
    saved_ssh_nodes: HashMap<String, NodeId>,
    expanded_ssh_nodes: HashSet<NodeId>,
    /// Host view disclosure state keyed by synthetic group row ids. Host headers
    /// default to expanded so the grouping is visible immediately; user rows
    /// default to collapsed so their option stacks stay out of the way.
    session_group_expanded: HashMap<NodeId, bool>,
    expanded_standalone_connections: HashSet<String>,
    local_session_group_expanded: bool,
    active_ssh_node_id: Option<NodeId>,
    next_ssh_node_id: u64,
    forwarding: Entity<forwards::ForwardingWorkspaceEntity>,
    _forwarding_subscription: Subscription,
    file_manager: FileManagerViews,
    sftp_tab_nodes: HashMap<TabId, NodeId>,
    standalone_sftp_tabs: HashMap<TabId, sftp::StandaloneSftpTabBinding>,
    standalone_sftp_sessions: HashMap<String, sftp::StandaloneSftpRuntime>,
    ftp_sessions: HashMap<String, Arc<sftp::ftp::FtpRuntime>>,
    dedicated_sftp_connections:
        Arc<parking_lot::Mutex<HashMap<NodeId, sftp::DedicatedSftpConnectionSlot>>>,
    ssh_consumer_prompt_handler: Arc<dyn SshPromptHandler>,
    ssh_consumer_managed_key_resolver: ManagedKeyResolver,
    embedded_sftp_node_id: Option<NodeId>,
    embedded_sftp_pinned: bool,
    sftp_presentation_request: Option<sftp::SftpPresentationRequest>,
    ide_workspace: Entity<ide::IdeWorkspaceEntity>,
    _ide_workspace_subscription: Subscription,
    knowledge_workspace: oxideterm_gpui_ai::knowledge::workspace::KnowledgeViews,
    sftp_view: Entity<sftp::SftpWorkspaceEntity>,
    sftp_pages: HashMap<TabId, sftp::views::SftpPage>,
    sftp_dispatch_surface: Rc<Cell<Option<sftp::SftpSurfaceId>>>,
    sftp_focused_surface: sftp::SftpSurfaceId,
    sftp_path_bookmarks: Vec<sftp::SftpPathBookmark>,
    sftp_path_bookmarks_loaded: bool,
    _sftp_observation: Subscription,
    _sftp_subscription: Subscription,
    graphics: Entity<GraphicsWorkspaceEntity>,
    graphics_surface: Entity<graphics::GraphicsSurfaceView>,
    _graphics_subscription: Subscription,
    host_tools: Entity<HostToolsEntity>,
    _host_tools_subscription: Subscription,
    cloud_sync: Entity<cloud_sync::CloudSyncWorkspaceEntity>,
    cloud_sync_panel: Entity<oxideterm_gpui_cloud_sync::panel::CloudSyncPanelView>,
    cloud_sync_confirm: Entity<cloud_sync::CloudSyncConfirmPortal>,
    _cloud_sync_subscription: Subscription,
    i18n: I18n,
    tokens: ThemeTokens,
    theme_transition: Option<ThemeTransitionState>,
    system_dark: bool,
    active_background: oxideterm_settings::BackgroundStyle,
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
    _session_manager_subscription: Subscription,
    remote_desktop: Entity<remote_desktop::RemoteDesktopWorkspaceEntity>,
    local_shells: Vec<ShellInfo>,
    local_shell_launcher_open: bool,
    local_shell_launcher_selected_id: Option<String>,
    archive_command: Option<terminal_archive::ArchiveCommandPreview>,
    ssh_tunnels: ssh_tunnels::SshTunnels,
    terminal: Entity<WorkspaceTerminalEntity>,
    _terminal_subscription: Subscription,
    overlay: Entity<WorkspaceOverlayEntity>,
    _overlay_observation: Subscription,
    // Producers release before the writer, so their final cancellation records can drain.
    audit: audit::AuditState,
}

impl Drop for WorkspaceApp {
    fn drop(&mut self) {
        self.archive_command = None;
        // App Lock and Cloud Sync move the focused secret into this window IME
        // adapter, so window destruction must zeroize it even without a blur event.
        zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
        // WorkspaceApp owns the shared session runtime. Window, tab, and page
        // release must not stop transfers or tunnels; final owner drop must.
        self.shutdown_final_session_services();
    }
}

pub(crate) use window_shell::WorkspaceWindowShell;

struct MermaidZoomState {
    window_id: gpui::WindowId,
    source: String,
    image: Arc<Image>,
    width: f32,
    height: f32,
    render_error: Option<String>,
    _render_task: Option<Task<()>>,
}

impl WorkspaceApp {
    fn localized_markdown_options(&self) -> MarkdownOptions {
        let mut options = MarkdownOptions::from_theme(&self.tokens);
        options.mermaid_error_prefix = self.i18n.t("markdown.mermaid_unsupported");
        options.mermaid_expand_label = self.i18n.t("markdown.mermaid_expand");
        options.mermaid_loading_label = self.i18n.t("markdown.mermaid_loading");
        options.html_details_label = self.i18n.t("markdown.html_details");
        options
    }

    fn mermaid_zoom_handler(&self, cx: &mut Context<Self>) -> MarkdownMermaidZoomHandler {
        Self::mermaid_zoom_handler_for_workspace(cx.entity())
    }

    fn mermaid_zoom_handler_for_workspace(workspace: Entity<Self>) -> MarkdownMermaidZoomHandler {
        Arc::new(move |source, image, width, height, window, cx| {
            let window_id = window.window_handle().window_id();
            let workspace = workspace.clone();
            window.defer(cx, move |_window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    let request = oxideterm_gpui_markdown::mermaid::MermaidRenderRequest::new(
                        &source,
                        &this.tokens,
                        &this.localized_markdown_options(),
                        MERMAID_MODAL_RASTER_SCALE,
                    );
                    let task = cx.spawn(async move |workspace, cx| {
                        let result = cx
                            .background_executor()
                            .spawn(async move { request.render() })
                            .await;
                        let _ = workspace.update(cx, |workspace, cx| {
                            if let Some(state) = workspace.mermaid_zoom.as_mut() {
                                match result {
                                    Ok(rendered) => {
                                        state.image = rendered.image;
                                        state.width = rendered.display_width;
                                        state.height = rendered.display_height;
                                    }
                                    Err(error) => state.render_error = Some(error),
                                }
                                state._render_task = None;
                                cx.notify();
                            }
                        });
                    });
                    this.mermaid_zoom = Some(MermaidZoomState {
                        window_id,
                        source,
                        image,
                        width,
                        height,
                        render_error: None,
                        _render_task: Some(task),
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
