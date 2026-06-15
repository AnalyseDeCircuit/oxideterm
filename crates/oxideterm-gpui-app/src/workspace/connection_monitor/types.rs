use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui::{MouseButton, PathBuilder, canvas, fill, point, rgba};
use oxideterm_connection_monitor::{ProfilerState, ResourceSampler};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
};
use oxideterm_gpui_ui::context_menu::{ContextMenuActionableStyle, context_menu_event_boundary};
use oxideterm_gpui_ui::progress::progress;
use oxideterm_gpui_ui::select::{
    select_event_boundary, select_option_highlighted, select_option_action,
};
use oxideterm_topology::{
    ConnectionTopologyLayout, ConnectionTopologySnapshot, TOPOLOGY_NODE_HEIGHT,
    TOPOLOGY_NODE_WIDTH, TopologyLayoutNode, TopologyViewStatus,
};
use oxideterm_ssh::SshCommandOutput;

use super::*;

const HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
const HOST_PROCESS_TABLE_HEADER_HEIGHT: f32 = 28.0;
const HOST_PROCESS_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
const HOST_PROCESS_USER_COLUMN_WIDTH: f32 = 64.0;
const HOST_PROCESS_PID_COLUMN_WIDTH: f32 = 54.0;
const HOST_PROCESS_CPU_COLUMN_WIDTH: f32 = 44.0;
const HOST_PROCESS_MEMORY_COLUMN_WIDTH: f32 = 48.0;
const HOST_PROCESS_SEPARATE_USER_COLUMN_MIN_WIDTH: f32 = 620.0;
const HOST_PROCESS_TABLE_HEADER_TEXT_SIZE: f32 = 10.0;
const HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE: f32 = 12.0;
const HOST_PROCESS_TABLE_META_TEXT_SIZE: f32 = 10.0;
const HOST_PROCESS_TABLE_VALUE_TEXT_SIZE: f32 = 11.0;
const HOST_PROCESS_DETAIL_TEXT_SIZE: f32 = 11.0;
const HOST_PROCESS_ACTION_TIMEOUT: Duration = Duration::from_secs(8);
const HOST_PROCESS_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
const HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
const HOST_DOCKER_TABLE_HEADER_HEIGHT: f32 = 28.0;
const HOST_DOCKER_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
const HOST_DOCKER_STATE_COLUMN_WIDTH: f32 = 72.0;
const HOST_DOCKER_PORTS_COLUMN_MIN_WIDTH: f32 = 92.0;
const HOST_DOCKER_ACTION_TIMEOUT: Duration = Duration::from_secs(12);
const HOST_DOCKER_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
const HOST_DOCKER_LOGS_TIMEOUT: Duration = Duration::from_secs(8);
const HOST_DOCKER_LOGS_MAX_OUTPUT_SIZE: usize = 128 * 1024;
const HOST_DOCKER_LOGS_DIALOG_WIDTH: f32 = 760.0;
const HOST_DOCKER_LOGS_DIALOG_MAX_HEIGHT: f32 = 520.0;

const MONITOR_POOL_REFRESH_INTERVAL: Duration = Duration::from_millis(2000);
const MONITOR_SPARKLINE_POINTS: usize = 12;
// The compact sidebar must stay on GPUI List scrolling; ordinary Div overflow
// repaints too much of the Host Tools panel during trackpad scrolling.
const COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT: f32 = 34.0;
const COMPACT_MONITOR_LIST_OVERSCAN: usize = 8;
const COMPACT_MONITOR_METRIC_ROW_HEIGHT: f32 = 32.0;
const COMPACT_MONITOR_SECTION_ROW_HEIGHT: f32 = 32.0;
const COMPACT_MONITOR_DETAIL_ROW_HEIGHT: f32 = 28.0;
const COMPACT_MONITOR_RETRY_ROW_HEIGHT: f32 = 44.0;
const COMPACT_MONITOR_ROW_SIDE_PADDING: f32 = 24.0;
const COMPACT_MONITOR_VALUE_MAX_WIDTH_RATIO: f32 = 0.58;
const COMPACT_MONITOR_DETAIL_VALUE_MAX_WIDTH_RATIO: f32 = 0.55;
const COMPACT_MONITOR_DETAIL_INDENT: f32 = 22.0;
const MONITOR_CONTENT_MAX_WIDTH: f32 = 1024.0;
const MONITOR_PAGE_PADDING: f32 = 32.0;
const MONITOR_SECTION_GAP: f32 = 32.0;
const MONITOR_SPARKLINE_HEIGHT: f32 = 28.0;
const MONITOR_SPARKLINE_STROKE_WIDTH: f32 = 1.5;
const MONITOR_SPARKLINE_STROKE_ALPHA: u32 = 0x99;
const MONITOR_BORDER_ALPHA: u32 = 0x80;
const MONITOR_SOURCE_ALPHA: u32 = 0x80;
const MONITOR_TINT_ALPHA: u32 = 0x1a;
const MONITOR_EMERALD: u32 = 0x34d399;
const MONITOR_EMERALD_DARK: u32 = 0x10b981;
const MONITOR_AMBER: u32 = 0xf59e0b;
const MONITOR_RED: u32 = 0xef4444;
const MONITOR_BLUE: u32 = 0x3b82f6;
const CONNECTION_POOL_HEADER_X: f32 = 24.0;
const CONNECTION_POOL_HEADER_Y: f32 = 16.0;
const CONNECTION_POOL_BODY_PADDING: f32 = 24.0;
const CONNECTION_POOL_CARD_GAP: f32 = 16.0;
const CONNECTION_POOL_CARD_PADDING: f32 = 16.0;
const CONNECTION_POOL_EMPTY_Y: f32 = 64.0;
const CONNECTION_POOL_BUTTON_SIZE: f32 = 32.0;
const CONNECTION_POOL_ICON_SIZE_LG: f32 = 20.0;
const CONNECTION_POOL_ICON_SIZE_MD: f32 = 16.0;
const CONNECTION_POOL_ICON_SIZE_SM: f32 = 12.0;
const CONNECTION_POOL_GREEN_400: u32 = 0x4ade80;
const CONNECTION_POOL_YELLOW_400: u32 = 0xfacc15;
const CONNECTION_POOL_AMBER_400: u32 = 0xfbbf24;
const CONNECTION_POOL_ORANGE_400: u32 = 0xfb923c;
const CONNECTION_POOL_RED_400: u32 = 0xf87171;
const CONNECTION_POOL_PANEL_ALPHA_30: u32 = 0x4d;
const CONNECTION_POOL_EMPTY_ICON_ALPHA: u32 = 0x4d;
const CONNECTION_POOL_EMPTY_HINT_OPACITY: f32 = 0.7;
const CONNECTION_POOL_IDLE_BORDER_ALPHA_30: u32 = 0x4d;
const TOPOLOGY_BG_GRID_STEP: f32 = 40.0;
const TOPOLOGY_BG_GRID_ALPHA: u32 = 0x1a;
const TOPOLOGY_PANEL_BG_ALPHA_20: u32 = 0x33;
const TOPOLOGY_PANEL_BORDER_ALPHA_50: u32 = 0x80;
const TOPOLOGY_MUTED_TEXT_ALPHA_70: u32 = 0xb3;
const TOPOLOGY_INSTRUCTION_ALPHA_60: u32 = 0x99;
const TOPOLOGY_LINE_INACTIVE_ALPHA: u32 = 0x66;
const TOPOLOGY_LINE_GLOW_ALPHA: u32 = 0x26;
const TOPOLOGY_CONNECTED: u32 = 0x22c55e;
const TOPOLOGY_CONNECTING: u32 = 0xeab308;
const TOPOLOGY_FAILED: u32 = 0xef4444;
const TOPOLOGY_DISCONNECTED: u32 = 0x71717a;
const TOPOLOGY_PENDING: u32 = 0xf59e0b;
const TOPOLOGY_ZOOM_INITIAL: f32 = 0.9;
const TOPOLOGY_ZOOM_MIN: f32 = 0.3;
const TOPOLOGY_ZOOM_MAX: f32 = 3.0;
const TOPOLOGY_PAN_INITIAL_X: f32 = 0.0;
const TOPOLOGY_PAN_INITIAL_Y: f32 = 50.0;
const TOPOLOGY_MENU_WIDTH: f32 = 180.0;
const TOPOLOGY_MENU_MAX_HEIGHT: f32 = 250.0;

#[derive(Clone, Copy)]
struct TopologyTransform {
    x: f32,
    y: f32,
    k: f32,
}

impl Default for TopologyTransform {
    fn default() -> Self {
        Self {
            x: TOPOLOGY_PAN_INITIAL_X,
            y: TOPOLOGY_PAN_INITIAL_Y,
            k: TOPOLOGY_ZOOM_INITIAL,
        }
    }
}

#[derive(Clone, Copy)]
struct TopologyDragState {
    last_x: f32,
    last_y: f32,
}

#[derive(Clone)]
struct TopologyNodeMenuState {
    node_id: Option<NodeId>,
    name: String,
    host: String,
    view_status: TopologyViewStatus,
    x: f32,
    y: f32,
}

struct ConnectionPoolStateView {
    label: String,
    color: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MonitorConnectionOption {
    // Sidebar monitoring only needs selector/header fields; avoid cloning the
    // full registry connection payload on every scroll-driven render.
    connection_id: String,
    host: String,
    port: u16,
    username: String,
}

impl MonitorConnectionOption {
    fn from_connection_info(connection: oxideterm_ssh::ConnectionInfo) -> Self {
        Self {
            connection_id: connection.connection_id,
            host: connection.host,
            port: connection.port,
            username: connection.username,
        }
    }

    fn from_pool_summary(summary: &ConnectionPoolEntrySummary) -> Self {
        Self {
            connection_id: summary.id.clone(),
            host: summary.host.clone(),
            port: summary.port,
            username: summary.username.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HostProcessActionRequest {
    connection_id: String,
    pid: String,
    command: String,
    action: ProcessActionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HostProcessActionDelivery {
    request: HostProcessActionRequest,
    result: Result<SshCommandOutput, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HostDockerActionRequest {
    connection_id: String,
    container_id: String,
    container_name: String,
    action: DockerActionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HostDockerActionDelivery {
    request: HostDockerActionRequest,
    result: Result<SshCommandOutput, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HostDockerLogsRequest {
    connection_id: String,
    container_id: String,
    container_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HostDockerLogsDelivery {
    request: HostDockerLogsRequest,
    result: Result<SshCommandOutput, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HostDockerLogsDialog {
    request: HostDockerLogsRequest,
    output: Option<String>,
    error: Option<String>,
    loading: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConnectionRuntimeSection {
    Overview,
    Pool,
    Health,
    Topology,
}

pub(super) struct ConnectionMonitorState {
    pub(super) pool_stats: Option<ConnectionPoolMonitorStats>,
    pub(super) pool_summaries: Vec<ConnectionPoolEntrySummary>,
    pub(super) topology_snapshot: Option<ConnectionTopologySnapshot>,
    pub(super) pool_error: Option<String>,
    pub(super) last_pool_refresh: Option<Instant>,
    pub(super) selected_connection_id: Option<String>,
    pub(super) selector_open: bool,
    pub(super) selector_highlighted_index: Option<usize>,
    pub(super) selector_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    pub(super) disabled_profiler_connections: HashSet<String>,
    pub(super) profiler_registry: ProfilerRegistry,
    pub(super) profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
    pub(super) profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
    compact_monitor_list_state: ListState,
    compact_monitor_list_cache: RefCell<VirtualListSignatureCache>,
    pub(in crate::workspace) host_process_search_query: String,
    pub(in crate::workspace) host_process_search_focused: bool,
    host_process_filter: ProcessFilter,
    host_process_sort: ProcessSort,
    host_process_sort_descending: bool,
    pub(in crate::workspace) host_process_expanded_pid: Option<String>,
    host_process_list_state: ListState,
    host_process_list_cache: RefCell<VirtualListSignatureCache>,
    pub(in crate::workspace) host_process_renice_value: String,
    pub(in crate::workspace) host_process_renice_focused: bool,
    host_process_pending_confirm: Option<HostProcessActionRequest>,
    host_process_action_running: Option<HostProcessActionRequest>,
    host_process_action_rx: Option<std::sync::mpsc::Receiver<HostProcessActionDelivery>>,
    host_process_action_polling: bool,
    pub(in crate::workspace) host_docker_search_query: String,
    pub(in crate::workspace) host_docker_search_focused: bool,
    pub(in crate::workspace) host_docker_expanded_id: Option<String>,
    host_docker_list_state: ListState,
    host_docker_list_cache: RefCell<VirtualListSignatureCache>,
    host_docker_pending_confirm: Option<HostDockerActionRequest>,
    host_docker_action_running: Option<HostDockerActionRequest>,
    host_docker_action_rx: Option<std::sync::mpsc::Receiver<HostDockerActionDelivery>>,
    host_docker_action_polling: bool,
    host_docker_logs_dialog: Option<HostDockerLogsDialog>,
    host_docker_logs_rx: Option<std::sync::mpsc::Receiver<HostDockerLogsDelivery>>,
    host_docker_logs_polling: bool,
    topology_transform: TopologyTransform,
    topology_drag: Option<TopologyDragState>,
    topology_menu: Option<TopologyNodeMenuState>,
}

impl ConnectionMonitorState {
    pub(super) fn new(
        profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
        profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
    ) -> Self {
        Self {
            pool_stats: None,
            pool_summaries: Vec::new(),
            topology_snapshot: None,
            pool_error: None,
            last_pool_refresh: None,
            selected_connection_id: None,
            selector_open: false,
            selector_highlighted_index: None,
            selector_focus_origin: None,
            disabled_profiler_connections: HashSet::new(),
            profiler_registry: ProfilerRegistry::new(),
            profiler_update_tx,
            profiler_update_rx,
            compact_monitor_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT),
                    COMPACT_MONITOR_LIST_OVERSCAN,
                ),
            ),
            compact_monitor_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_process_search_query: String::new(),
            host_process_search_focused: false,
            host_process_filter: ProcessFilter::All,
            host_process_sort: ProcessSort::Memory,
            host_process_sort_descending: true,
            host_process_expanded_pid: None,
            host_process_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_process_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_process_renice_value: "0".to_string(),
            host_process_renice_focused: false,
            host_process_pending_confirm: None,
            host_process_action_running: None,
            host_process_action_rx: None,
            host_process_action_polling: false,
            host_docker_search_query: String::new(),
            host_docker_search_focused: false,
            host_docker_expanded_id: None,
            host_docker_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_docker_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_docker_pending_confirm: None,
            host_docker_action_running: None,
            host_docker_action_rx: None,
            host_docker_action_polling: false,
            host_docker_logs_dialog: None,
            host_docker_logs_rx: None,
            host_docker_logs_polling: false,
            topology_transform: TopologyTransform::default(),
            topology_drag: None,
            topology_menu: None,
        }
    }

    pub(in crate::workspace) fn dismiss_topology_menu(&mut self) -> bool {
        // Topology menu state owns a private node snapshot; expose only the
        // browser-style transient dismissal result to the workspace root.
        self.topology_menu.take().is_some()
    }
}
