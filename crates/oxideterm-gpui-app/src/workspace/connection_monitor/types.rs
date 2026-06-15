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
    select_trigger_with_focus_visible,
};
use oxideterm_topology::{
    ConnectionTopologyLayout, ConnectionTopologySnapshot, TOPOLOGY_NODE_HEIGHT,
    TOPOLOGY_NODE_WIDTH, TopologyLayoutNode, TopologyViewStatus,
};

use super::*;

const MONITOR_POOL_REFRESH_INTERVAL: Duration = Duration::from_millis(2000);
const MONITOR_SPARKLINE_POINTS: usize = 12;
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
