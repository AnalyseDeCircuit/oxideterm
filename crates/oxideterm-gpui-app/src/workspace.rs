mod actions;
mod forwards;
mod ime;
mod new_connection;
mod pane_tree;
mod session_manager;
mod settings;
mod sftp;
mod sidebar;
mod tabs;

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use gpui::{
    AnyElement, App, ClipboardItem, Context, CursorStyle, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    ParentElement, Pixels, Render, RenderImage, Rgba, ScrollWheelEvent, SharedString, Styled,
    StyledImage, Timer, Window, div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_connections::ConnectionStore;
use oxideterm_forwarding::{ForwardEvent, ForwardingRegistry, SavedForwardStore};
use oxideterm_gpui_platform::{
    rendering::detect_graphics,
    vibrancy::{NativeVibrancyMode, apply_window_vibrancy},
};
use oxideterm_gpui_terminal::{
    BackgroundImageRenderCache, SharedTerminalSession, TerminalBackgroundFit,
    TerminalBackgroundPreferences, TerminalHighlightRenderMode,
    TerminalHighlightRule as UiHighlightRule, TerminalNotice, TerminalNoticeVariant, TerminalPane,
    TerminalPasteLabels, TerminalTrzszLabels, TerminalUiPreferences, TerminalUiTheme,
};
use oxideterm_gpui_ui::{
    toast::{ToastVariant, ToastView},
    toaster::toaster,
};
use oxideterm_i18n::{I18n, Locale};
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
    AuthMethod, ConnectionConsumer, ConnectionPoolConfig, ConnectionState, NodeId, NodeOrigin,
    NodeReadiness, NodeRouter, NodeRuntimeStore, NodeState, NodeStateEvent, NodeTreeSnapshot,
    NodeTreeSnapshotNode, PhaseResult, ProbeConnectionStatus, ReconnectNodeTerminalSnapshot,
    ReconnectNodeTransferSnapshot, ReconnectOrchestratorStore, ReconnectPhase, ReconnectSnapshot,
    SshConfig, SshConnectionRegistry, SshTransportClient, TerminalEndpoint,
};
use oxideterm_terminal::{
    LocalPtyConfig, ShellInfo, SshSessionConfig, TerminalCursorShape,
    TerminalEncoding as SessionTerminalEncoding, TerminalLifecycle, scan_shells,
};
use oxideterm_theme::{ThemeTokens, UiRadii, theme_by_id};
use oxideterm_workspace::{
    ActiveSessionNode, ActiveSessionReadiness, ActiveSessionStatus, MAX_PANES_PER_TAB, PaneId,
    PaneNode, SplitDirection, Tab, TabId, TabKind, TabTitleSource, TerminalSessionId,
    adjusted_split_sizes, balanced_sizes, sort_active_session_nodes,
};

use self::actions::SearchBarState;
use self::ime::{WorkspaceImeElement, keystroke_commits_platform_text};
use self::new_connection::{
    HostKeyChallenge, KeyboardInteractiveChallenge, NativeSshPromptHandler, NewConnectionForm,
    NewConnectionSelect, SavedConnectionPromptAction, SshAuthTab, SshConnectionWorkerResult,
};
use self::pane_tree::SplitDrag;
use self::session_manager::SessionManagerState;
use self::sidebar::SidebarSection;
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
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId};
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputAnchorId};
use oxideterm_gpui_ui::typography::tauri_ui_font_family as settings_ui_font_family;

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
    background_blur_preview: Option<i64>,
    background_blur_commit_generation: u64,
    background_cache_poll_scheduled: bool,
    new_connection_form: Option<NewConnectionForm>,
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
    pending_reconnect_transfer_resumes: HashMap<NodeId, HashSet<String>>,
    reconnect_transfer_resume_totals: HashMap<NodeId, usize>,
    terminal_endpoint_sessions: HashMap<TerminalSessionId, WorkspaceTerminalEndpointSession>,
    ssh_nodes: HashMap<NodeId, WorkspaceSshNode>,
    saved_ssh_nodes: HashMap<String, NodeId>,
    terminal_ssh_nodes: HashMap<TerminalSessionId, NodeId>,
    expanded_ssh_nodes: HashSet<NodeId>,
    active_ssh_node_id: Option<NodeId>,
    next_ssh_node_id: u64,
    forward_tab_nodes: HashMap<TabId, NodeId>,
    forwarding_view: forwards::ForwardsViewState,
    sftp_tab_nodes: HashMap<TabId, NodeId>,
    sftp_view: sftp::SftpViewState,
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
    settings_connection_new_group: String,
    settings_selected_ssh_hosts: HashSet<String>,
    settings_connection_status: Option<String>,
    local_shells: Vec<ShellInfo>,
    terminal_notice_tx: std::sync::mpsc::Sender<TerminalNotice>,
    terminal_notice_rx: std::sync::mpsc::Receiver<TerminalNotice>,
    workspace_toasts: Vec<WorkspaceToast>,
}

#[derive(Clone, Debug)]
struct WorkspaceToast {
    notice: TerminalNotice,
    expires_at: Instant,
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
