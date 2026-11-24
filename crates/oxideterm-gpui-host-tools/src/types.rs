use std::time::Duration;

use gpui::{Rgba, rgb, rgba};
use oxideterm_gpui_ui::motion::ExitPresence;

use oxideterm_topology::TopologyViewStatus;
use zeroize::Zeroize;

use super::*;
use oxideterm_connection_monitor::multiplexer::{Multiplexer, MuxNode, MuxVerb};

pub(crate) const HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(crate) const HOST_GPU_LIST_ESTIMATED_ROW_HEIGHT: f32 = 72.0;
pub(crate) const HOST_GPU_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_GPU_UTILIZATION_COLUMN_WIDTH: f32 = 78.0;
pub(crate) const HOST_GPU_MEMORY_COLUMN_WIDTH: f32 = 92.0;
pub(crate) const HOST_PROCESS_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_PROCESS_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(crate) const HOST_PROCESS_USER_COLUMN_WIDTH: f32 = 64.0;
pub(crate) const HOST_PROCESS_PID_COLUMN_WIDTH: f32 = 54.0;
pub(crate) const HOST_PROCESS_CPU_COLUMN_WIDTH: f32 = 44.0;
pub(crate) const HOST_PROCESS_MEMORY_COLUMN_WIDTH: f32 = 64.0;
pub(crate) const HOST_PROCESS_SEPARATE_USER_COLUMN_MIN_WIDTH: f32 = 620.0;
pub(crate) const HOST_PROCESS_TABLE_HEADER_TEXT_SIZE: f32 = 10.0;
pub(crate) const HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE: f32 = 12.0;
pub(crate) const HOST_PROCESS_TABLE_META_TEXT_SIZE: f32 = 10.0;
pub(crate) const HOST_PROCESS_TABLE_VALUE_TEXT_SIZE: f32 = 11.0;
pub(crate) const HOST_PROCESS_DETAIL_TEXT_SIZE: f32 = 11.0;
pub(crate) const HOST_PROCESS_ACTION_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const HOST_PROCESS_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(crate) const HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(crate) const HOST_DOCKER_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_DOCKER_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(crate) const HOST_DOCKER_STATE_COLUMN_WIDTH: f32 = 72.0;
pub(crate) const HOST_DOCKER_PORTS_COLUMN_MIN_WIDTH: f32 = 92.0;
pub(crate) const HOST_DOCKER_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 560.0;
pub(crate) const HOST_DOCKER_ACTION_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const HOST_DOCKER_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(crate) const HOST_DOCKER_LOGS_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const HOST_DOCKER_LOGS_MAX_OUTPUT_SIZE: usize = 128 * 1024;
pub(crate) const HOST_DOCKER_LOGS_DIALOG_WIDTH: f32 = 760.0;
pub(crate) const HOST_DOCKER_LOGS_DIALOG_MAX_HEIGHT: f32 = 520.0;
pub(crate) const HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(crate) const HOST_SERVICE_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_SERVICE_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(crate) const HOST_SERVICE_STATE_COLUMN_WIDTH: f32 = 78.0;
pub(crate) const HOST_SERVICE_ENABLED_COLUMN_WIDTH: f32 = 70.0;
pub(crate) const HOST_SERVICE_PID_COLUMN_WIDTH: f32 = 54.0;
pub(crate) const HOST_SERVICE_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 560.0;
pub(crate) const HOST_SERVICE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const HOST_SERVICE_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(crate) const HOST_SERVICE_LOGS_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const HOST_SERVICE_ACTION_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const HOST_SERVICE_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(crate) const HOST_SERVICE_LOGS_MAX_OUTPUT_SIZE: usize = 128 * 1024;
pub(crate) const HOST_SERVICE_LOGS_DIALOG_WIDTH: f32 = 760.0;
pub(crate) const HOST_SERVICE_LOGS_DIALOG_MAX_HEIGHT: f32 = 520.0;
// The PING probe self-bounds with `redis-cli -t 3`; the runner timeout only
// covers a hung connection setup on top of it.
pub(crate) const HOST_REDIS_PROBE_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const HOST_REDIS_PROBE_MAX_OUTPUT_SIZE: usize = 64 * 1024;
// Browsing runs one bounded SCAN per database, so it needs room for several
// databases worth of keys and a longer runner timeout than the PING probe.
pub(crate) const HOST_REDIS_KEYS_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const HOST_REDIS_KEYS_MAX_OUTPUT_SIZE: usize = 512 * 1024;
pub(crate) const HOST_REDIS_TREE_ROW_HEIGHT: f32 = 24.0;
pub(crate) const HOST_REDIS_ENDPOINT_CHIP_HEIGHT: f32 = 26.0;
// A value read runs TYPE plus one bounded range/scan on top of the password
// handshake, so it needs more room than the key listing.
pub(crate) const HOST_REDIS_VALUE_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const HOST_REDIS_VALUE_MAX_OUTPUT_SIZE: usize = 512 * 1024;
pub(crate) const HOST_REDIS_ACTION_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const HOST_REDIS_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(crate) const HOST_REDIS_VALUE_DIALOG_WIDTH: f32 = 720.0;
pub(crate) const HOST_REDIS_VALUE_DIALOG_DEFAULT_HEIGHT: f32 = 420.0;
pub(crate) const HOST_REDIS_VALUE_DIALOG_MIN_WIDTH: f32 = 420.0;
pub(crate) const HOST_REDIS_VALUE_DIALOG_MIN_HEIGHT: f32 = 200.0;
pub(crate) const HOST_REDIS_VALUE_DIALOG_MAX_WIDTH: f32 = 1200.0;
pub(crate) const HOST_REDIS_VALUE_DIALOG_MAX_HEIGHT: f32 = 800.0;
/// One virtual row for the value viewer; long lines never wrap. The dialog's
/// default body height is derived from this so resizing stays on row boundaries
/// only by convention, not by measurement.
pub const HOST_REDIS_VALUE_ROW_HEIGHT: f32 = 18.0;
pub const HOST_REDIS_VALUE_ROW_OVERSCAN: usize = 8;
// String/JSON values are longer than a key name; the editor gets room to show
// the document instead of a single 32px line.
pub const HOST_REDIS_VALUE_INPUT_HEIGHT: f32 = 96.0;
/// Mono cell the value rows and the editor draw with. The horizontal band is
/// measured with the same size, so scrolling never clips a rendered line.
pub const HOST_REDIS_VALUE_TEXT_SIZE: f32 = 11.0;
/// Space between the two columns of collection rows.
pub const HOST_REDIS_VALUE_ROW_GAP: f32 = 8.0;
/// Trailing slack on a scroll band so the last glyph clears the scrollbar.
pub const HOST_REDIS_VALUE_SCROLL_SLACK: f32 = 16.0;
/// Window width the value dialog keeps free so its borders stay on screen.
pub const HOST_REDIS_VALUE_DIALOG_VIEWPORT_MARGIN: f32 = 48.0;
/// Header, footer and window margins around the value dialog body; the body
/// band is clamped against the window height with this reserve.
pub const HOST_REDIS_VALUE_DIALOG_CHROME_HEIGHT: f32 = 152.0;
// The dialog is a viewer over a bounded capture; per-frame line output stops
// here instead of building thousands of GPUI elements.
pub(crate) const HOST_REDIS_VALUE_MAX_LINES: usize = 400;
pub(crate) const HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT: f32 = 56.0;
pub(crate) const HOST_LOG_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_LOG_TIME_COLUMN_WIDTH: f32 = 92.0;
pub(crate) const HOST_LOG_LEVEL_COLUMN_WIDTH: f32 = 58.0;
pub(crate) const HOST_LOG_SOURCE_COLUMN_WIDTH: f32 = 96.0;
pub(crate) const HOST_LOG_UNIT_COLUMN_WIDTH: f32 = 96.0;
pub const HOST_LOG_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 680.0;
pub(crate) const HOST_LOG_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const HOST_LOG_SNAPSHOT_LIMIT: usize = 300;
pub(crate) const HOST_LOG_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(crate) const HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(crate) const HOST_TMUX_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_TMUX_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(crate) const HOST_TMUX_ATTACHED_COLUMN_WIDTH: f32 = 78.0;
pub(crate) const HOST_TMUX_WINDOWS_COLUMN_WIDTH: f32 = 70.0;
pub(crate) const HOST_TMUX_PANES_COLUMN_WIDTH: f32 = 48.0;
pub(crate) const HOST_TMUX_ACTIVITY_COLUMN_WIDTH: f32 = 92.0;
pub(crate) const HOST_TMUX_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 620.0;
pub(crate) const HOST_TMUX_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const HOST_TMUX_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 128 * 1024;
pub(crate) const HOST_TMUX_ACTION_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const HOST_TMUX_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(crate) const HOST_TMUX_INPUT_DIALOG_WIDTH: f32 = 460.0;
pub(crate) const HOST_MUX_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const HOST_MUX_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 512 * 1024;
pub(crate) const HOST_MUX_DETAIL_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const HOST_MUX_DETAIL_MAX_OUTPUT_SIZE: usize = 512 * 1024;
pub(crate) const HOST_MUX_ACTION_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const HOST_MUX_ACTION_MAX_OUTPUT_SIZE: usize = 64 * 1024;
pub(crate) const HOST_MUX_READ_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const HOST_MUX_READ_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(crate) const HOST_PORT_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(crate) const HOST_PORT_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_PORT_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(crate) const HOST_PORT_PROTOCOL_COLUMN_WIDTH: f32 = 64.0;
pub(crate) const HOST_PORT_STATE_COLUMN_WIDTH: f32 = 78.0;
pub(crate) const HOST_PORT_PID_COLUMN_WIDTH: f32 = 58.0;
pub(crate) const HOST_PORT_PROCESS_COLUMN_WIDTH: f32 = 96.0;
pub(crate) const HOST_PORT_REMOTE_COLUMN_WIDTH: f32 = 132.0;
pub(crate) const HOST_PORT_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 680.0;
pub(crate) const HOST_PORT_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const HOST_PORT_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(crate) const HOST_SCHEDULE_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(crate) const HOST_SCHEDULE_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_SCHEDULE_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(crate) const HOST_SCHEDULE_SOURCE_COLUMN_WIDTH: f32 = 74.0;
pub(crate) const HOST_SCHEDULE_STATE_COLUMN_WIDTH: f32 = 78.0;
pub(crate) const HOST_SCHEDULE_ENABLED_COLUMN_WIDTH: f32 = 72.0;
pub(crate) const HOST_SCHEDULE_NEXT_COLUMN_WIDTH: f32 = 112.0;
pub(crate) const HOST_SCHEDULE_LAST_COLUMN_WIDTH: f32 = 112.0;
pub(crate) const HOST_SCHEDULE_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 720.0;
pub(crate) const HOST_SCHEDULE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const HOST_SCHEDULE_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(crate) const HOST_SCHEDULE_ACTION_TIMEOUT: Duration = Duration::from_secs(12);
pub(crate) const HOST_SCHEDULE_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(crate) const HOST_SCHEDULE_LOGS_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const HOST_SCHEDULE_LOGS_MAX_OUTPUT_SIZE: usize = 128 * 1024;
pub(crate) const HOST_SCHEDULE_LOGS_DIALOG_WIDTH: f32 = 760.0;
pub(crate) const HOST_SCHEDULE_LOGS_DIALOG_MAX_HEIGHT: f32 = 520.0;
pub(crate) const HOST_FILESYSTEM_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const HOST_FILESYSTEM_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 512 * 1024;
pub(crate) const HOST_PACKAGE_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(crate) const HOST_PACKAGE_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const HOST_PACKAGE_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(crate) const HOST_PACKAGE_STATUS_COLUMN_WIDTH: f32 = 84.0;
pub(crate) const HOST_PACKAGE_VERSION_COLUMN_WIDTH: f32 = 116.0;
pub(crate) const HOST_PACKAGE_MANAGER_COLUMN_WIDTH: f32 = 92.0;
pub(crate) const HOST_PACKAGE_SERVICE_COLUMN_WIDTH: f32 = 108.0;
pub(crate) const HOST_PACKAGE_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 720.0;
pub(crate) const HOST_PACKAGE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(18);
pub(crate) const HOST_PACKAGE_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 512 * 1024;

pub const MONITOR_POOL_REFRESH_INTERVAL: Duration = Duration::from_millis(2000);
// The compact sidebar must stay on GPUI List scrolling; ordinary Div overflow
// repaints too much of the Host Tools panel during trackpad scrolling.
pub const COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT: f32 = 38.0;
pub const COMPACT_MONITOR_LIST_OVERSCAN: usize = 8;
pub const COMPACT_MONITOR_USAGE_ROW_HEIGHT: f32 = 44.0;
pub const COMPACT_MONITOR_CORE_PAIR_ROW_HEIGHT: f32 = 24.0;
pub const COMPACT_MONITOR_SPARKLINE_ROW_HEIGHT: f32 = 36.0;
pub const COMPACT_MONITOR_METRIC_ROW_HEIGHT: f32 = 28.0;
pub const COMPACT_MONITOR_SECTION_ROW_HEIGHT: f32 = 30.0;
pub const COMPACT_MONITOR_NETWORK_SUMMARY_ROW_HEIGHT: f32 = 32.0;
pub const COMPACT_MONITOR_STACKED_ROW_HEIGHT: f32 = 46.0;
pub const COMPACT_MONITOR_INTERFACE_ROW_HEIGHT: f32 = 28.0;
pub const COMPACT_MONITOR_PROCESS_ROW_HEIGHT: f32 = 26.0;
pub const COMPACT_MONITOR_RETRY_ROW_HEIGHT: f32 = 40.0;
pub const COMPACT_MONITOR_ROW_SIDE_PADDING: f32 = 12.0;
// Only fall back to two-line bandwidth rows when the sidebar is very narrow.
pub const COMPACT_MONITOR_STACKED_LAYOUT_MAX_WIDTH: f32 = 240.0;
pub const MONITOR_BORDER_ALPHA: u32 = 0x80;
pub const MONITOR_TINT_ALPHA: u32 = 0x1a;
pub const MONITOR_EMERALD: u32 = 0x34d399;
pub const MONITOR_EMERALD_DARK: u32 = 0x10b981;
pub const MONITOR_AMBER: u32 = 0xf59e0b;
pub const MONITOR_RED: u32 = 0xef4444;
pub const MONITOR_BLUE: u32 = 0x3b82f6;
pub const TOPOLOGY_BG_GRID_STEP: f32 = 40.0;
pub const TOPOLOGY_BG_GRID_ALPHA: u32 = 0x1a;
pub const TOPOLOGY_PANEL_BG_ALPHA_20: u32 = 0x33;
pub const TOPOLOGY_PANEL_BORDER_ALPHA_50: u32 = 0x80;
pub const TOPOLOGY_MUTED_TEXT_ALPHA_70: u32 = 0xb3;
pub const TOPOLOGY_INSTRUCTION_ALPHA_60: u32 = 0x99;
pub const TOPOLOGY_LINE_INACTIVE_ALPHA: u32 = 0x66;
pub const TOPOLOGY_LINE_GLOW_ALPHA: u32 = 0x26;
pub const TOPOLOGY_CONNECTED: u32 = 0x22c55e;
pub const TOPOLOGY_CONNECTING: u32 = 0xeab308;
pub const TOPOLOGY_FAILED: u32 = 0xef4444;
pub const TOPOLOGY_DISCONNECTED: u32 = 0x71717a;
pub const TOPOLOGY_PENDING: u32 = 0xf59e0b;
pub const TOPOLOGY_ZOOM_INITIAL: f32 = 0.9;
pub const TOPOLOGY_ZOOM_MIN: f32 = 0.3;
pub const TOPOLOGY_ZOOM_MAX: f32 = 3.0;
pub const TOPOLOGY_PAN_INITIAL_X: f32 = 0.0;
pub const TOPOLOGY_PAN_INITIAL_Y: f32 = 50.0;
pub const TOPOLOGY_MENU_WIDTH: f32 = 180.0;
pub const TOPOLOGY_MENU_MAX_HEIGHT: f32 = 250.0;

pub fn connection_monitor_surface_bg(theme_bg: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba(0x00000000)
    } else {
        rgb(theme_bg)
    }
}

#[derive(Clone, Copy)]
pub struct TopologyTransform {
    pub x: f32,
    pub y: f32,
    pub k: f32,
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
pub struct TopologyDragState {
    pub last_x: f32,
    pub last_y: f32,
}

#[derive(Clone)]
pub struct TopologyNodeMenuState {
    pub node_id: Option<NodeId>,
    pub name: String,
    pub host: String,
    pub view_status: TopologyViewStatus,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorConnectionOption {
    // Sidebar monitoring only needs selector/header fields; avoid cloning the
    // full registry connection payload on every scroll-driven render.
    pub connection_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    // The local target has no endpoint, so it carries its localized label.
    pub label: Option<String>,
}

impl MonitorConnectionOption {
    #[cfg(not(target_family = "wasm"))]
    pub fn from_connection_info(connection: oxideterm_ssh::ConnectionInfo) -> Self {
        Self {
            connection_id: connection.connection_id,
            host: connection.host,
            port: connection.port,
            username: connection.username,
            label: None,
        }
    }

    pub fn from_pool_summary(summary: &ConnectionPoolEntrySummary) -> Self {
        Self {
            connection_id: summary.id.clone(),
            host: summary.host.clone(),
            port: summary.port,
            username: summary.username.clone(),
            label: None,
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn local(label: String) -> Self {
        Self {
            connection_id: LOCAL_HOST_CONNECTION_ID.to_string(),
            host: String::new(),
            port: 0,
            username: String::new(),
            label: Some(label),
        }
    }
}

#[derive(Clone)]
pub struct HostProcessActionRequest {
    pub connection_id: String,
    pub pid: String,
    // Process names can contain credential-like command fragments. All UI
    // clones share one buffer that is cleared when the confirmation closes.
    pub display_command: Arc<zeroize::Zeroizing<String>>,
    pub action: ProcessActionKind,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostProcessActionRun {
    pub connection_id: String,
    pub pid: String,
    pub action: ProcessActionKind,
}

pub struct HostProcessActionDelivery {
    pub request: HostProcessActionRun,
    pub result: Result<bool, ()>,
}

pub struct HostProcessActionsState {
    pub pending_confirm: Option<HostToolConfirmState<HostProcessActionRequest>>,
    pub running: Option<HostProcessActionRun>,
}

impl HostProcessActionsState {
    pub fn new() -> Self {
        Self {
            pending_confirm: None,
            running: None,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostDockerActionRequest {
    pub connection_id: String,
    pub container_id: String,
    pub container_name: String,
    pub action: DockerActionKind,
}

pub struct HostDockerActionDelivery {
    pub request: HostDockerActionRequest,
    pub result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostDockerLogsRequest {
    pub connection_id: String,
    pub container_id: String,
    pub container_name: String,
    pub failure_fallback: String,
    pub empty_fallback: String,
}

pub struct HostDockerLogsDelivery {
    pub request: HostDockerLogsRequest,
    pub result: Result<HostCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostDockerLogsDialog {
    pub request: HostDockerLogsRequest,
    // Docker output stays in one shared zeroizing buffer while rendered.
    pub output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub error: Option<String>,
    pub loading: bool,
}

pub struct HostDockerOperationsState {
    pub pending_confirm: Option<HostToolConfirmState<HostDockerActionRequest>>,
    pub action_running: Option<HostDockerActionRequest>,
    pub logs_dialog: Option<HostDockerLogsDialog>,
}

impl HostDockerOperationsState {
    pub fn new() -> Self {
        Self {
            pending_confirm: None,
            action_running: None,
            logs_dialog: None,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostServiceSnapshotRequest {
    pub connection_id: String,
    pub connection_fallback: String,
    pub failure_fallback: String,
}

pub struct HostServiceSnapshotDelivery {
    pub request: HostServiceSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

pub struct HostServiceSnapshotPending {
    pub request: HostServiceSnapshotRequest,
    pub runtime: HostToolsRuntime,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostServiceActionRequest {
    pub connection_id: String,
    pub service_id: String,
    pub description: String,
    pub action: ServiceActionKind,
}

pub struct HostServiceActionDelivery {
    pub request: HostServiceActionRequest,
    pub result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostServiceLogsRequest {
    pub connection_id: String,
    pub service_id: String,
    pub description: String,
    pub failure_fallback: String,
    pub empty_fallback: String,
}

pub struct HostServiceLogsDelivery {
    pub request: HostServiceLogsRequest,
    pub result: Result<HostCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostServiceLogsDialog {
    pub request: HostServiceLogsRequest,
    // Service output stays in one shared zeroizing buffer while rendered.
    pub output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub error: Option<String>,
    pub loading: bool,
}

pub struct HostServicesState {
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<oxideterm_connection_monitor::ResourceServiceSnapshot>,
    pub snapshot_running: Option<HostServiceSnapshotRequest>,
    pub snapshot_pending: Option<HostServiceSnapshotPending>,
    pub snapshot_in_flight: bool,
    pub pending_confirm: Option<HostToolConfirmState<HostServiceActionRequest>>,
    pub action_running: Option<HostServiceActionRequest>,
    pub logs_dialog: Option<HostServiceLogsDialog>,
}

impl HostServicesState {
    pub fn new() -> Self {
        Self {
            snapshot_connection_id: None,
            snapshot: None,
            snapshot_running: None,
            snapshot_pending: None,
            snapshot_in_flight: false,
            pending_confirm: None,
            action_running: None,
            logs_dialog: None,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostRedisSnapshotRequest {
    pub connection_id: String,
    pub connection_fallback: String,
    pub failure_fallback: String,
}

pub struct HostRedisSnapshotDelivery {
    pub request: HostRedisSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

pub struct HostRedisSnapshotPending {
    pub request: HostRedisSnapshotRequest,
    pub runtime: HostToolsRuntime,
}

/// One key-browsing request for a single endpoint.
///
/// The password lives only in this request and in the focused input buffer: it
/// is never formatted, audited, or written to a snapshot.
#[derive(Clone)]
pub struct HostRedisKeysRequest {
    pub connection_id: String,
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    pub password: zeroize::Zeroizing<String>,
    /// `SCAN MATCH` glob when the key search is active.
    pub pattern: Option<String>,
    pub failure_fallback: String,
}

impl HostRedisKeysRequest {
    /// Request identity ignores the credential so a re-typed password for the
    /// same endpoint still replaces the in-flight result.
    pub fn same_target(&self, other: &Self) -> bool {
        self.connection_id == other.connection_id
            && self.endpoint == other.endpoint
            && self.pattern == other.pattern
    }

    /// A queued read belongs to the host and endpoint that were selected when
    /// it was made. Once either moved on, starting it would read the old host
    /// and render the result under the new one.
    pub fn is_stale_for(
        &self,
        connection: Option<&str>,
        endpoint: Option<&oxideterm_connection_monitor::ResourceRedisEndpoint>,
    ) -> bool {
        connection != Some(self.connection_id.as_str()) || endpoint != Some(&self.endpoint)
    }
}

pub struct HostRedisKeysDelivery {
    pub request: HostRedisKeysRequest,
    pub result: Result<HostSecretCommandOutput, ()>,
}

pub struct HostRedisKeysPending {
    pub request: HostRedisKeysRequest,
    pub runtime: HostToolsRuntime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRedisValuePurpose {
    /// The dialog is open and renders the value.
    Open,
    /// The row menu copies the value without opening the dialog.
    Copy,
}

#[derive(Clone)]
pub struct HostRedisValueRequest {
    pub connection_id: String,
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    /// Database the key lives in; a key name alone is ambiguous across dbs.
    pub database: u32,
    pub key: String,
    pub password: zeroize::Zeroizing<String>,
    pub purpose: HostRedisValuePurpose,
    pub failure_fallback: String,
}

impl HostRedisValueRequest {
    pub fn same_target(&self, other: &Self) -> bool {
        self.connection_id == other.connection_id
            && self.endpoint == other.endpoint
            && self.database == other.database
            && self.key == other.key
            && self.purpose == other.purpose
    }
}

pub struct HostRedisValueDelivery {
    pub request: HostRedisValueRequest,
    pub result: Result<HostSecretCommandOutput, ()>,
}

pub struct HostRedisValuePending {
    pub request: HostRedisValueRequest,
    pub runtime: HostToolsRuntime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRedisMutationKind {
    Delete,
    /// Deletes every key in `keys` in one exec (prefix deletion).
    DeleteMany,
    Rename,
    /// Sets an expiry in seconds (`ttl_seconds`).
    Expire,
    /// Removes the expiry.
    Persist,
}

#[derive(Clone)]
pub struct HostRedisMutationRequest {
    pub connection_id: String,
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    pub database: u32,
    pub kind: HostRedisMutationKind,
    pub key: String,
    /// Targets for `DeleteMany`; single mutations leave this empty.
    pub keys: Vec<String>,
    pub new_key: Option<String>,
    /// Expiry for `Expire`; unused by the other mutations.
    pub ttl_seconds: Option<u64>,
    pub password: zeroize::Zeroizing<String>,
    pub failure_fallback: String,
}

pub struct HostRedisMutationDelivery {
    pub request: HostRedisMutationRequest,
    pub result: Result<HostSecretCommandOutput, ()>,
}

pub struct HostRedisValueDialog {
    pub connection_id: String,
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    pub database: u32,
    pub key: String,
    pub value: Option<oxideterm_connection_monitor::ResourceRedisValue>,
    pub loading: bool,
    pub error: Option<String>,
    pub json_view: bool,
}

impl HostRedisValueDialog {
    /// Whether a delivery still belongs to this dialog's target. A key name
    /// alone is not unique: the same name can live on another host, endpoint,
    /// or database, so every part of the target has to match before a value is
    /// installed. The read purpose is deliberately not part of the identity —
    /// callers branch on it before installing.
    /// Mutation results are matched the same way, ignoring the mutation kind:
    /// only the target decides which dialog owns the answer.
    pub fn matches_mutation(&self, request: &HostRedisMutationRequest) -> bool {
        self.connection_id == request.connection_id
            && self.endpoint == request.endpoint
            && self.database == request.database
            && self.key == request.key
    }

    pub fn matches_request(&self, request: &HostRedisValueRequest) -> bool {
        self.connection_id == request.connection_id
            && self.endpoint == request.endpoint
            && self.database == request.database
            && self.key == request.key
    }

    /// Installs a freshly read value, wiping the previous body first: the body
    /// is the secret itself and must not linger in the freed allocation.
    pub fn set_value(&mut self, value: Option<oxideterm_connection_monitor::ResourceRedisValue>) {
        self.clear_value();
        self.value = value;
    }

    pub fn clear_value(&mut self) {
        if let Some(mut value) = self.value.take() {
            zeroize_redis_value(&mut value);
        }
    }
}

impl Drop for HostRedisValueDialog {
    /// Closing or replacing the dialog drops the fetched body; wipe it instead
    /// of handing a freed-but-readable copy back to the allocator.
    fn drop(&mut self) {
        self.clear_value();
    }
}

/// Wipes the secret-bearing parts of a read Redis value. `body` holds the
/// display projection and `elements` the exact reply bytes, so both go.
pub fn zeroize_redis_value(value: &mut oxideterm_connection_monitor::ResourceRedisValue) {
    use zeroize::Zeroize;
    value.body.zeroize();
    value.elements.zeroize();
    if let Some(raw_type) = value.raw_type.as_mut() {
        raw_type.zeroize();
    }
}

pub struct HostRedisDeleteConfirm {
    pub connection_id: String,
    /// Endpoint captured when the confirmation opened; the mutation must
    /// target it even if the user switches endpoints before confirming.
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    pub database: u32,
    /// Display name: the key name, or the prefix for a subtree deletion.
    pub key: String,
    /// Every key the confirmation removes; a single key for leaf deletes.
    pub keys: Vec<String>,
    pub presence: ExitPresence,
}

pub struct HostRedisCreateKeyDialog {
    pub connection_id: String,
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    pub database: u32,
    /// New key name; zeroized when the dialog closes.
    pub name: zeroize::Zeroizing<String>,
    pub kind: oxideterm_connection_monitor::RedisCreateKind,
    /// When true the key is created without an expiry and the TTL field is hidden.
    pub ttl_never: bool,
    pub ttl: zeroize::Zeroizing<String>,
    /// Value for `string`/`json`, or the pending element while adding entries.
    pub value: zeroize::Zeroizing<String>,
    /// Pending hash field while adding a pair.
    pub field: zeroize::Zeroizing<String>,
    /// Committed elements: `field<TAB>value` for hash, `member<TAB>score` for a
    /// sorted set, plain values otherwise.
    pub entries: Vec<zeroize::Zeroizing<String>>,
    pub error: Option<String>,
    pub loading: bool,
}

#[derive(Clone)]
pub struct HostRedisCreateKeyRequest {
    pub connection_id: String,
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    pub database: u32,
    pub key: String,
    pub password: zeroize::Zeroizing<String>,
    pub value: zeroize::Zeroizing<String>,
    pub failure_fallback: String,
}

pub struct HostRedisCreateKeyDelivery {
    pub request: HostRedisCreateKeyRequest,
    pub result: Result<HostSecretCommandOutput, ()>,
}

pub struct HostRedisSetValueDialog {
    pub connection_id: String,
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    pub database: u32,
    pub key: String,
    /// Shape being edited, taken from the fetched value.
    pub kind: RedisValueWriteKind,
    /// The server compares these bytes with the live key before publishing a replacement.
    pub base: Option<oxideterm_connection_monitor::ResourceRedisValue>,
    /// Replacement value for string/JSON, or the pending element.
    pub value: zeroize::Zeroizing<String>,
    /// Pending hash field while adding a pair.
    pub field: zeroize::Zeroizing<String>,
    /// Committed rows: `field<TAB>value`, `member<TAB>score`, or elements.
    pub entries: Vec<zeroize::Zeroizing<String>>,
    pub error: Option<String>,
    pub loading: bool,
}

impl Drop for HostRedisSetValueDialog {
    /// The baseline copy holds the same secret bytes as the fetched value, so
    /// closing the editor wipes it instead of leaving it to the allocator.
    fn drop(&mut self) {
        if let Some(base) = self.base.as_mut() {
            zeroize_redis_value(base);
        }
    }
}

#[derive(Clone)]
pub struct HostRedisSetValueRequest {
    pub connection_id: String,
    pub endpoint: oxideterm_connection_monitor::ResourceRedisEndpoint,
    pub database: u32,
    pub key: String,
    pub password: zeroize::Zeroizing<String>,
    pub value: zeroize::Zeroizing<String>,
    pub failure_fallback: String,
}

pub struct HostRedisSetValueDelivery {
    pub request: HostRedisSetValueRequest,
    pub result: Result<HostSecretCommandOutput, ()>,
}

pub struct HostRedisState {
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<oxideterm_connection_monitor::ResourceRedisSnapshot>,
    pub snapshot_running: Option<HostRedisSnapshotRequest>,
    pub snapshot_pending: Option<HostRedisSnapshotPending>,
    pub snapshot_in_flight: bool,
    pub keys_connection_id: Option<String>,
    pub keys_endpoint: Option<oxideterm_connection_monitor::ResourceRedisEndpoint>,
    /// Endpoint the cached key list was read from. A cached tree is only valid
    /// for the endpoint it came from; anything else is a stale projection.
    pub keys_cache_endpoint: Option<oxideterm_connection_monitor::ResourceRedisEndpoint>,
    pub keys: Option<oxideterm_connection_monitor::ResourceRedisKeys>,
    pub keys_running: Option<HostRedisKeysRequest>,
    pub keys_pending: Option<HostRedisKeysPending>,
    pub keys_in_flight: bool,
    pub value_dialog: Option<HostRedisValueDialog>,
    /// Value viewer geometry; dragging the corner keeps it across openings.
    pub value_dialog_size: (f32, f32),
    pub value_dialog_resize: Option<HostToolsDialogResizeDrag>,
    pub value_dialog_list: ListState,
    pub value_dialog_list_cache: std::cell::RefCell<VirtualListSignatureCache>,
    /// Horizontal band of the viewer body; rows can be wider than the dialog.
    pub value_dialog_h_scroll: gpui::ScrollHandle,
    /// Vertical/horizontal band of the inline value editor.
    pub value_dialog_editor_scroll: gpui::ScrollHandle,
    pub value_running: Option<HostRedisValueRequest>,
    pub value_pending: Option<HostRedisValuePending>,
    pub value_in_flight: bool,
    pub mutation_running: Option<HostRedisMutationRequest>,
    pub set_value_running: Option<HostRedisSetValueRequest>,
    pub create_key_running: Option<HostRedisCreateKeyRequest>,
    /// Key awaiting a delete confirmation.
    pub delete_confirm: Option<HostRedisDeleteConfirm>,
    /// Databases the user collapsed. Databases start expanded.
    pub collapsed_databases: std::collections::BTreeSet<String>,
    /// Key-tree segments the user expanded. Segments start collapsed, so a
    /// freshly browsed database shows only its first-level prefixes.
    pub expanded_segments: std::collections::BTreeSet<String>,
    pub list_state: ListState,
    pub list_cache: std::cell::RefCell<VirtualListSignatureCache>,
}

impl HostRedisState {
    pub fn new() -> Self {
        Self {
            snapshot_connection_id: None,
            snapshot: None,
            snapshot_running: None,
            snapshot_pending: None,
            snapshot_in_flight: false,
            keys_connection_id: None,
            keys_endpoint: None,
            keys_cache_endpoint: None,
            keys: None,
            keys_running: None,
            keys_pending: None,
            keys_in_flight: false,
            value_dialog: None,
            value_dialog_size: (
                HOST_REDIS_VALUE_DIALOG_WIDTH,
                HOST_REDIS_VALUE_DIALOG_DEFAULT_HEIGHT,
            ),
            value_dialog_resize: None,
            value_dialog_list: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_REDIS_VALUE_ROW_HEIGHT), 11),
            ),
            value_dialog_list_cache: std::cell::RefCell::new(VirtualListSignatureCache::default()),
            value_dialog_h_scroll: gpui::ScrollHandle::new(),
            value_dialog_editor_scroll: gpui::ScrollHandle::new(),
            value_running: None,
            value_pending: None,
            value_in_flight: false,
            mutation_running: None,
            set_value_running: None,
            create_key_running: None,
            delete_confirm: None,
            collapsed_databases: std::collections::BTreeSet::new(),
            expanded_segments: std::collections::BTreeSet::new(),
            list_state: tauri_virtual_list_state(
                0,
                gpui::ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_REDIS_TREE_ROW_HEIGHT), 8),
            ),
            list_cache: std::cell::RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

/// Container tool inventory tabs. Pods only exist on Podman, so the panel
/// hides that tab instead of rendering an always-empty page.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HostContainerTab {
    #[default]
    Containers,
    Images,
    Volumes,
    Networks,
    Pods,
    Registries,
}

impl HostContainerTab {
    /// Engine capability gates inventory tabs; a switch must not strand the
    /// panel on a tab the new engine cannot fill.
    pub fn supported_by(
        self,
        engine: oxideterm_connection_monitor::container::ContainerEngine,
    ) -> bool {
        match self {
            Self::Pods => engine.supports_pods(),
            Self::Registries => engine.supports_registry_auth(),
            Self::Containers | Self::Images | Self::Volumes | Self::Networks => true,
        }
    }
}

/// Column the Containers tab orders by. The cycle is ascending, descending,
/// then the engine's own order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerSortKey {
    Cpu,
    Memory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainerSort {
    pub key: ContainerSortKey,
    pub descending: bool,
}

pub const HOST_CONTAINER_CAPTURE_DIALOG_DEFAULT_WIDTH: f32 = 760.0;
pub const HOST_CONTAINER_CAPTURE_DIALOG_DEFAULT_HEIGHT: f32 = 420.0;
pub const HOST_CONTAINER_CAPTURE_DIALOG_MIN_WIDTH: f32 = 420.0;
pub const HOST_CONTAINER_CAPTURE_DIALOG_MIN_HEIGHT: f32 = 260.0;
pub const HOST_CONTAINER_CAPTURE_DIALOG_MAX_WIDTH: f32 = 1100.0;
pub const HOST_CONTAINER_CAPTURE_DIALOG_MAX_HEIGHT: f32 = 720.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostContainerConfirmKind {
    /// Prunes stopped containers on the selected engine.
    ContainerPrune,
    Container(oxideterm_connection_monitor::container::ContainerAction),
    Image(oxideterm_connection_monitor::container::ImageAction),
    Volume(oxideterm_connection_monitor::container::VolumeAction),
    Network(oxideterm_connection_monitor::container::NetworkAction),
    Pod(oxideterm_connection_monitor::container::PodAction),
    /// A command assembled from an input dialog; never shown as a confirm.
    Raw,
}

/// A confirmed mutation stays here until its command delivery resolves.
#[derive(Clone)]
pub struct HostContainerConfirmRequest {
    pub connection_id: String,
    pub instance: oxideterm_connection_monitor::container::ContainerInstance,
    pub target_id: String,
    pub label: String,
    pub kind: HostContainerConfirmKind,
}

pub struct HostContainerConfirmDelivery {
    pub request: HostContainerConfirmRequest,
    pub result: Result<HostCommandOutput, ()>,
}

/// Text-input dialogs for the container tool. Free-form arguments stay raw by
/// design; identifiers are validated before they reach a command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostContainerInputKind {
    Run,
    Pull,
    Push,
    Build,
    Tag { image_id: String },
    Save { image_id: String },
    Load,
    Import,
    Export { container_id: String },
    Copy { container_id: String },
    CreateVolume,
    CreateNetwork,
    CreatePod,
    Rename { container_id: String },
    Login,
}

/// Structured fields for the image `run` dialog. Defaults keep the dialog
/// usable without touching any field: `-d` plus the image entrypoint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostContainerRunParams {
    pub name: String,
    pub detached: bool,
    pub interactive: bool,
    pub auto_remove: bool,
    /// Comma-separated `-p` specs.
    pub ports: String,
    /// Comma-separated `-v` specs.
    pub volumes: String,
    /// Program and arguments run inside the container (e.g. `/bin/sh`).
    pub command: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostContainerRunFlag {
    Detached,
    Interactive,
    AutoRemove,
}

#[derive(Clone)]
pub struct HostContainerInputDialog {
    pub kind: HostContainerInputKind,
    /// Registry typed or picked for pull/push. Empty means Docker Hub, which
    /// is why the field carries a default hint instead of a value.
    pub registry: String,
    pub value: String,
    /// Fields for `HostContainerInputKind::Run`; unused by the other kinds.
    pub run: HostContainerRunParams,
}

#[derive(Clone)]
pub enum HostContainerCaptureKind {
    /// Engine detection; also refreshes the instance switcher.
    Discovery,
    Snapshot(oxideterm_connection_monitor::container::ContainerInstance),
    /// One-shot container resource usage.
    Stats,
    /// Authenticated registries read from the engine auth file.
    Registries,
    /// Read-only output rendered in the shared text dialog.
    Text {
        capture: oxideterm_connection_monitor::container::ContainerCapture,
        title: String,
    },
}

#[derive(Clone)]
pub struct HostContainerCaptureRequest {
    pub connection_id: String,
    pub kind: HostContainerCaptureKind,
    /// Engine the capture was built for; a delivery for another engine than
    /// the selected one must not replace the panel's copy.
    pub instance_id: Option<String>,
    pub failure_fallback: String,
}

pub struct HostContainerCaptureDelivery {
    pub request: HostContainerCaptureRequest,
    pub result: Result<HostCommandOutput, ()>,
}

pub struct HostContainerTextDialog {
    pub title: String,
    pub output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub error: Option<String>,
    pub loading: bool,
    pub scroll_handle: gpui::ScrollHandle,
}

#[derive(Clone, Copy)]
pub struct HostToolsDialogResizeDrag {
    pub start_cursor: gpui::Point<gpui::Pixels>,
    pub start_width: f32,
    pub start_height: f32,
}

pub struct HostContainerState {
    pub discovery: Option<oxideterm_connection_monitor::container::ContainerDiscovery>,
    pub discovery_connection_id: Option<String>,
    pub discovery_running: bool,
    /// Handle of the running discovery probe. Aborting it cancels the capture,
    /// so a superseding connection does not wait out the old host's 40s probe.
    pub discovery_task: Option<HostCaptureTask>,
    /// Engine selection survives refreshes and is dropped only when the engine
    /// disappears from discovery.
    pub selected_instance_id: Option<String>,
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<oxideterm_connection_monitor::container::ContainerSnapshot>,
    pub snapshot_running: bool,
    pub tab: HostContainerTab,
    pub confirm: Option<HostToolConfirmState<HostContainerConfirmRequest>>,
    pub action_running: bool,
    pub text_dialog: Option<HostContainerTextDialog>,
    pub text_dialog_running: Option<HostContainerCaptureRequest>,
    /// Dialog epoch. Bumped whenever a capture dialog opens or closes so a
    /// delivery spawned for an earlier dialog cannot fill a newer one that
    /// asks for the very same capture.
    pub text_dialog_generation: u64,
    /// Epoch the running capture was spawned under; it has to equal
    /// `text_dialog_generation` for its delivery to land.
    pub text_dialog_running_generation: Option<u64>,
    /// Task for the running capture; aborted when the dialog closes so a
    /// dismissed inspect/log read stops holding a channel until its timeout.
    pub text_dialog_task: Option<HostCaptureTask>,
    /// User-chosen capture dialog size, kept across openings.
    pub text_dialog_size: (f32, f32),
    pub text_dialog_resize: Option<HostToolsDialogResizeDrag>,
    /// In-flight image search; results land in the open input dialog.
    pub stats: Option<Vec<oxideterm_connection_monitor::container::ContainerStat>>,
    pub stats_running: bool,
    /// Read failure for the stats tab, kept apart from the table so an error
    /// does not render as an empty result.
    pub stats_error: Option<String>,
    pub registries: Option<Vec<oxideterm_connection_monitor::container::ContainerRegistry>>,
    pub registries_running: bool,
    /// Read failure for the registries tab.
    pub registries_error: Option<String>,
    /// Row whose action strip is expanded; only one row shows actions at a time.
    pub selected_row: Option<String>,
    /// Containers tab ordering; `None` keeps the engine's order.
    pub container_sort: Option<ContainerSort>,
    /// Images tab ordering; `None` keeps the engine's own order.
    pub image_sort: Option<oxideterm_connection_monitor::container::ImageSort>,
    pub image_sort_descending: bool,
    /// One virtual list serves every inventory tab.
    pub list_state: ListState,
    pub list_cache: std::cell::RefCell<VirtualListSignatureCache>,
}

impl HostContainerState {
    pub fn new() -> Self {
        Self {
            discovery: None,
            discovery_connection_id: None,
            discovery_running: false,
            discovery_task: None,
            selected_instance_id: None,
            snapshot_connection_id: None,
            snapshot: None,
            snapshot_running: false,
            tab: HostContainerTab::Containers,
            confirm: None,
            action_running: false,
            text_dialog: None,
            text_dialog_running: None,
            text_dialog_generation: 0,
            text_dialog_running_generation: None,
            text_dialog_task: None,
            text_dialog_size: (
                HOST_CONTAINER_CAPTURE_DIALOG_DEFAULT_WIDTH,
                HOST_CONTAINER_CAPTURE_DIALOG_DEFAULT_HEIGHT,
            ),
            text_dialog_resize: None,
            stats: None,
            stats_running: false,
            stats_error: None,
            registries: None,
            registries_running: false,
            registries_error: None,
            selected_row: None,
            container_sort: None,
            image_sort: None,
            image_sort_descending: true,
            list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(88.0), 6),
            ),
            list_cache: std::cell::RefCell::new(VirtualListSignatureCache::default()),
        }
    }

    /// Opens a new capture-dialog epoch. A capture spawned under an earlier
    /// epoch keeps that older value, so its delivery can no longer land.
    pub fn begin_container_text_dialog(&mut self) -> u64 {
        self.text_dialog_generation = self.text_dialog_generation.wrapping_add(1);
        self.text_dialog_generation
    }

    /// Closes the capture dialog and cancels its request. The spawn itself
    /// cannot be aborted, so the epoch bump is what makes its delivery inert.
    /// Returns whether a dialog was open.
    pub fn invalidate_container_text_dialog(&mut self) -> bool {
        self.text_dialog_generation = self.text_dialog_generation.wrapping_add(1);
        self.text_dialog_running = None;
        self.text_dialog_running_generation = None;
        if let Some(task) = self.text_dialog_task.take() {
            task.abort();
        }
        let was_open = self.text_dialog.take().is_some();
        self.text_dialog_resize = None;
        was_open
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostSnapshotFeedback {
    Silent,
    Toast,
}

/// Identifies the top Host Tools portal using the same order as root rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostToolsWindowModalSnapshot {
    ProcessConfirm(oxideterm_gpui_ui::motion::ExitPhase),
    DockerConfirm(oxideterm_gpui_ui::motion::ExitPhase),
    DockerLogs,
    ContainerConfirm(oxideterm_gpui_ui::motion::ExitPhase),
    ContainerText,
    ContainerInput,
    ServiceConfirm(oxideterm_gpui_ui::motion::ExitPhase),
    ServiceLogs,
    TmuxConfirm(oxideterm_gpui_ui::motion::ExitPhase),
    TmuxInput,
    MuxConfirm(oxideterm_gpui_ui::motion::ExitPhase),
    MuxInput,
    MuxRead,
    ScheduleConfirm(oxideterm_gpui_ui::motion::ExitPhase),
    ScheduleLogs,
    PackageConfirm(oxideterm_gpui_ui::motion::ExitPhase),
}

impl HostSnapshotFeedback {
    pub fn should_toast(self) -> bool {
        matches!(self, Self::Toast)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostLogSnapshotRequest {
    pub connection_id: String,
    pub preset: LogPreset,
    pub limit: usize,
    pub feedback: HostSnapshotFeedback,
    pub failure_fallback: String,
}

pub struct HostLogSnapshotDelivery {
    pub request: HostLogSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

pub fn zeroize_host_snapshot_output(output: &mut HostCommandOutput) {
    // Host inspection commands may return credentials embedded in logs or
    // diagnostics. Clear both raw streams once parsing or classification ends.
    output.stdout.zeroize();
    output.stderr.zeroize();
}

#[cfg(test)]
mod snapshot_output_zeroize_tests {
    use super::*;

    #[test]
    fn host_snapshot_output_zeroizes_both_raw_streams() {
        let mut output = HostCommandOutput {
            stdout: "Authorization: secret-output".to_string(),
            stderr: "Proxy-Authorization: secret-error".to_string(),
            exit_code: Some(1),
            truncated: true,
        };

        zeroize_host_snapshot_output(&mut output);

        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert_eq!(output.exit_code, Some(1));
        assert!(output.truncated);
    }
}

pub struct HostLogsState {
    pub expanded_index: Option<usize>,
    pub preset: LogPreset,
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<ResourceLogSnapshot>,
    pub running: Option<HostLogSnapshotRequest>,
    pub snapshot_in_flight: bool,
    pub list_state: ListState,
    pub list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostLogsState {
    pub fn new() -> Self {
        Self {
            expanded_index: None,
            preset: LogPreset::All,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            snapshot_in_flight: false,
            list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostTmuxSnapshotRequest {
    pub connection_id: String,
    pub feedback: HostSnapshotFeedback,
    pub search_query: String,
    pub failure_fallback: String,
    pub unavailable_fallback: String,
}

pub struct HostTmuxSnapshotDelivery {
    pub request: HostTmuxSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub enum HostTmuxDestructiveAction {
    KillSession { target: String },
    KillWindow { target: String },
    KillPane { target: String },
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostTmuxActionRequest {
    pub connection_id: String,
    pub session_id: String,
    pub session_name: String,
    pub target_label: String,
    // Confirm state accepts only destructive actions, so secret-bearing rename
    // and send-command values can never enter its cloneable request type.
    pub action: HostTmuxDestructiveAction,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostTmuxActionRun {
    pub connection_id: String,
    pub session_id: String,
    pub session_name: String,
    pub target_label: String,
}

pub struct HostTmuxActionDelivery {
    pub request: HostTmuxActionRun,
    pub result: Result<bool, ()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostPortSnapshotRequest {
    pub connection_id: String,
    pub feedback: HostSnapshotFeedback,
    pub failure_fallback: String,
}

pub struct HostPortSnapshotDelivery {
    pub request: HostPortSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

pub struct HostPortsState {
    pub filter: PortFilter,
    pub expanded_index: Option<usize>,
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<ResourcePortSnapshot>,
    pub running: Option<HostPortSnapshotRequest>,
    pub snapshot_in_flight: bool,
    pub list_state: ListState,
    pub list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostPortsState {
    pub fn new() -> Self {
        Self {
            filter: PortFilter::All,
            expanded_index: None,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            snapshot_in_flight: false,
            list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_PORT_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostScheduleSnapshotRequest {
    pub connection_id: String,
    pub feedback: HostSnapshotFeedback,
    pub failure_fallback: String,
}

pub struct HostScheduleSnapshotDelivery {
    pub request: HostScheduleSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

pub struct HostSchedulesState {
    pub filter: ScheduledTaskFilter,
    pub expanded_index: Option<usize>,
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<ResourceScheduledTaskSnapshot>,
    pub running: Option<HostScheduleSnapshotRequest>,
    pub snapshot_in_flight: bool,
    pub list_state: ListState,
    pub list_cache: RefCell<VirtualListSignatureCache>,
    pub pending_confirm: Option<HostToolConfirmState<HostScheduleActionRequest>>,
    pub action_running: Option<HostScheduleActionRequest>,
    pub logs_dialog: Option<HostScheduleLogsDialog>,
}

impl HostSchedulesState {
    pub fn new() -> Self {
        Self {
            filter: ScheduledTaskFilter::All,
            expanded_index: None,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            snapshot_in_flight: false,
            list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_SCHEDULE_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
            pending_confirm: None,
            action_running: None,
            logs_dialog: None,
        }
    }
}

pub struct HostFilesystemSnapshotDelivery {
    pub request: FilesystemSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostPackageSnapshotRequest {
    pub connection_id: String,
    pub feedback: HostSnapshotFeedback,
    pub failure_fallback: String,
}

pub struct HostPackageSnapshotDelivery {
    pub request: HostPackageSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

/// Packages uninstall through a confirmation because the terminal command
/// removes installed software as soon as the shell receives it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostPackageUninstallRequest {
    pub connection_id: String,
    pub manager: String,
    pub package_id: String,
    pub package_name: String,
}

pub struct HostPackagesState {
    pub filter: PackageFilter,
    pub expanded_index: Option<usize>,
    pub pending_confirm: Option<HostToolConfirmState<HostPackageUninstallRequest>>,
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<ResourcePackageSnapshot>,
    pub running: Option<HostPackageSnapshotRequest>,
    pub snapshot_in_flight: bool,
    pub list_state: ListState,
    pub list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostPackagesState {
    pub fn new() -> Self {
        Self {
            filter: PackageFilter::All,
            expanded_index: None,
            pending_confirm: None,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            snapshot_in_flight: false,
            list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_PACKAGE_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostScheduleActionRequest {
    pub connection_id: String,
    pub task_id: String,
    pub task_name: String,
    pub unit: String,
    pub action: ScheduledTaskActionKind,
}

pub struct HostScheduleActionDelivery {
    pub request: HostScheduleActionRequest,
    pub result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostScheduleLogsRequest {
    pub connection_id: String,
    // Keep sampled commands out of asynchronous deliveries; logs need only
    // the public task identity used by the dialog and follow action.
    pub task_id: String,
    pub task_name: String,
    pub task_source: String,
    pub task_unit: String,
    pub failure_fallback: String,
    pub empty_fallback: String,
}

pub struct HostScheduleLogsDelivery {
    pub request: HostScheduleLogsRequest,
    pub result: Result<HostCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostScheduleLogsDialog {
    pub request: HostScheduleLogsRequest,
    // The Entity and render tree share one zeroizing capture buffer.
    pub output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub error: Option<String>,
    pub loading: bool,
}

#[derive(Clone, Eq, PartialEq)]
pub enum HostTmuxInputDialogKind {
    RenameSession { target: String },
    RenameWindow { target: String },
    SendPaneCommand { target: String },
}

pub struct HostTmuxInputDialog {
    pub connection_id: String,
    pub session_id: String,
    pub session_name: String,
    pub target_label: String,
    // User commands may contain secrets; the dialog clears the only retained
    // input buffer when it closes or hands the value to command construction.
    pub value: zeroize::Zeroizing<String>,
    pub kind: HostTmuxInputDialogKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostToolsTextInput {
    ProcessSearch,
    ProcessRenice,
    DockerSearch,
    ServiceSearch,
    LogSearch,
    TmuxSearch,
    TmuxDialog,
    MuxDialog,
    PortSearch,
    ScheduleSearch,
    FilesystemSearch,
    PackageSearch,
    ContainerSearch,
    ContainerDialog,
    ContainerDialogRegistry,
    ContainerRunName,
    ContainerRunPorts,
    ContainerRunVolumes,
    ContainerRunCommand,
    RedisPassword,
    RedisRename,
    RedisSetValue,
    RedisSetValueField,
    RedisCreateKeyName,
    RedisCreateKeyValue,
    RedisCreateKeyField,
    RedisCreateKeyTtl,
    RedisTtl,
    RedisSearch,
}

pub struct HostTmuxState {
    pub(crate) mux: super::multiplexer::MuxPanel,
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<ResourceTmuxSnapshot>,
    pub snapshot_running: Option<HostTmuxSnapshotRequest>,
    pub snapshot_in_flight: bool,
    pub last_error: Option<String>,
    pub pending_confirm: Option<HostToolConfirmState<HostTmuxActionRequest>>,
    pub action_running: Option<HostTmuxActionRun>,
}

impl HostTmuxState {
    pub fn new() -> Self {
        Self {
            mux: Default::default(),
            snapshot_connection_id: None,
            snapshot: None,
            snapshot_running: None,
            snapshot_in_flight: false,
            last_error: None,
            pending_confirm: None,
            action_running: None,
        }
    }
}

/// One capture of the session list. `generation` distinguishes successive
/// captures of the same target: a response is applied only while the request
/// that started it is still current, so switching hosts or providers and back
/// cannot accept an older remote answer as the newest one.
#[derive(Clone, Eq, PartialEq)]
pub struct MuxSnapshotRequest {
    pub provider: Multiplexer,
    pub connection_id: String,
    pub feedback: HostSnapshotFeedback,
    pub generation: u64,
}

impl MuxSnapshotRequest {
    /// The identity repeat clicks and automatic refreshes coalesce on,
    /// deliberately ignoring the generation that makes each capture unique.
    pub fn same_target(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.connection_id == other.connection_id
            && self.feedback == other.feedback
    }
}

pub struct MuxSnapshotDelivery {
    pub request: MuxSnapshotRequest,
    pub result: Result<HostCommandOutput, ()>,
}

/// One capture of the subtree beneath one session; same generation rule as
/// [`MuxSnapshotRequest`].
#[derive(Clone, Eq, PartialEq)]
pub struct MuxDetailRequest {
    pub provider: Multiplexer,
    pub connection_id: String,
    pub session: String,
    pub generation: u64,
}

impl MuxDetailRequest {
    pub fn same_target(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.connection_id == other.connection_id
            && self.session == other.session
    }
}

pub struct MuxDetailDelivery {
    pub request: MuxDetailRequest,
    pub result: Result<HostCommandOutput, ()>,
}

/// The verb the user confirmed against one row. `node` is `None` for a
/// session-level verb; the frozen model builds every object command from the
/// node itself, so the request carries it rather than an id it could not
/// turn back into a row.
#[derive(Clone, Eq, PartialEq)]
pub struct MuxActionRequest {
    pub provider: Multiplexer,
    pub connection_id: String,
    /// Session that owns the workspace, tab or pane being mutated.
    pub session: String,
    pub target_label: String,
    pub node: Option<MuxNode>,
    pub verb: MuxVerb,
}

/// The one mutating slot a confirmed action reserves. It carries the whole
/// request identity because a target label alone is not unique: after a
/// provider switch an older action can finish with the same label as a newer
/// one and would otherwise clear that action's slot and report its result.
#[derive(Clone, Eq, PartialEq)]
pub struct MuxActionRun {
    pub provider: Multiplexer,
    pub connection_id: String,
    pub session: String,
    pub target_label: String,
    pub generation: u64,
}

pub struct MuxActionDelivery {
    pub request: MuxActionRun,
    // Action output is reduced to a success bit before it crosses delivery.
    pub result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub enum MuxInputDialogKind {
    NewSession,
    NewWorkspace,
    RenameWorkspace { target: MuxNode },
    RenameTab { target: MuxNode },
    RenamePane { target: MuxNode },
    RunPane { target: MuxNode },
}

/// The dialog is bound to the host and session it was opened for. A lifecycle
/// fallback can select another connection while the dialog stays open, and
/// submitting against that host would mutate a machine the user never chose,
/// so submission re-checks both before the verb is built.
pub struct MuxInputDialog {
    pub kind: MuxInputDialogKind,
    /// Session the dialog's action targets; providers address workspaces, tabs
    /// and panes through their session.
    pub session: String,
    pub provider: Multiplexer,
    pub connection_id: String,
    // User commands may contain secrets; the dialog clears the only retained
    // input buffer when it closes or hands the value to command construction.
    pub value: zeroize::Zeroizing<String>,
    /// Localized reason the previous submission was refused. The typed value
    /// stays so the user corrects it instead of typing it again.
    pub error: Option<&'static str>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct MuxReadRequest {
    pub provider: Multiplexer,
    pub connection_id: String,
    pub session: String,
    pub target_id: String,
    pub target_label: String,
    /// Reopening the same pane starts a new capture, so an older answer cannot
    /// overwrite the buffer the user is now reading.
    pub generation: u64,
}

pub struct MuxReadDelivery {
    pub request: MuxReadRequest,
    pub result: Result<HostCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct MuxReadDialog {
    pub request: MuxReadRequest,
    // The Entity and render tree share one zeroizing capture buffer.
    pub output: Option<Arc<zeroize::Zeroizing<String>>>,
    /// A failed capture renders the localized fallback; remote output never
    /// becomes the error text.
    pub failed: bool,
    pub loading: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionRuntimeSection {
    Overview,
    Topology,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostToolsVisibility {
    Hidden,
    VisibleMainTab,
    VisibleSidebar,
    VisibleDetachedWindow,
    VisibleMultiple {
        main_tab: bool,
        sidebar: bool,
        detached_window: bool,
    },
    Dropped,
}

impl HostToolsVisibility {
    pub fn from_mounts(main_tab: bool, sidebar: bool, detached_window: bool) -> Self {
        match (
            usize::from(main_tab) + usize::from(sidebar) + usize::from(detached_window),
            main_tab,
            sidebar,
            detached_window,
        ) {
            (0, _, _, _) => Self::Hidden,
            (1, true, _, _) => Self::VisibleMainTab,
            (1, _, true, _) => Self::VisibleSidebar,
            (1, _, _, true) => Self::VisibleDetachedWindow,
            _ => Self::VisibleMultiple {
                main_tab,
                sidebar,
                detached_window,
            },
        }
    }

    pub fn is_visible(self) -> bool {
        !matches!(self, Self::Hidden | Self::Dropped)
    }

    pub fn sidebar_is_visible(self) -> bool {
        matches!(
            self,
            Self::VisibleSidebar | Self::VisibleMultiple { sidebar: true, .. }
        )
    }

    pub fn main_window_is_visible(self) -> bool {
        matches!(
            self,
            Self::VisibleMainTab
                | Self::VisibleSidebar
                | Self::VisibleMultiple { main_tab: true, .. }
                | Self::VisibleMultiple { sidebar: true, .. }
        )
    }
}

#[derive(Clone)]
pub struct HostToolsMessages {
    pub service_connection_missing: String,
    pub service_action_failed: String,
    pub log_unknown_error: String,
    pub port_unknown_error: String,
    pub filesystem_unknown_error: String,
    pub package_unknown_error: String,
    pub schedule_unknown_error: String,
    pub tmux_unknown_error: String,
    pub tmux_unavailable: String,
    pub local_connection_label: String,
    pub redis_connection_missing: String,
    pub redis_probe_failed: String,
    pub redis_key_action_failed: String,
    pub redis_rename_invalid: String,
    pub redis_auth_required: String,
    pub redis_key_missing: String,
    pub redis_value_set_invalid_json: String,
    /// Shown when a value holds bytes or line breaks the editor would rewrite.
    pub redis_value_edit_unsupported: String,
    /// Shown when the server copy moved since the editor opened.
    pub redis_value_edit_conflict: String,
    pub redis_create_key_exists: String,
    pub redis_create_key_invalid_name: String,
    pub redis_create_key_invalid_ttl: String,
    pub redis_create_key_empty_entries: String,
    pub redis_create_key_invalid_field: String,
    pub redis_create_key_invalid_score: String,
    pub container_capture_failed: String,
    pub container_terminal_opened: String,
    pub container_terminal_missing: String,
}

impl Default for HostToolsMessages {
    fn default() -> Self {
        Self {
            service_connection_missing: String::new(),
            service_action_failed: String::new(),
            log_unknown_error: String::new(),
            port_unknown_error: String::new(),
            filesystem_unknown_error: String::new(),
            package_unknown_error: String::new(),
            schedule_unknown_error: String::new(),
            tmux_unknown_error: String::new(),
            tmux_unavailable: String::new(),
            local_connection_label: String::new(),
            redis_connection_missing: String::new(),
            redis_probe_failed: String::new(),
            redis_key_action_failed: String::new(),
            redis_rename_invalid: String::new(),
            redis_auth_required: String::new(),
            redis_key_missing: String::new(),
            redis_value_set_invalid_json: String::new(),
            redis_value_edit_unsupported: String::new(),
            redis_value_edit_conflict: String::new(),
            redis_create_key_exists: String::new(),
            redis_create_key_invalid_name: String::new(),
            redis_create_key_invalid_ttl: String::new(),
            redis_create_key_empty_entries: String::new(),
            redis_create_key_invalid_field: String::new(),
            redis_create_key_invalid_score: String::new(),
            container_capture_failed: String::new(),
            container_terminal_opened: String::new(),
            container_terminal_missing: String::new(),
        }
    }
}

impl HostToolsMessages {
    pub fn from_i18n(i18n: &I18n) -> Self {
        Self {
            service_connection_missing: i18n.t("sidebar.host_services.toast.connection_missing"),
            service_action_failed: i18n.t("sidebar.host_services.toast.action_failed"),
            log_unknown_error: i18n.t("sidebar.host_logs.toast.unknown_error"),
            port_unknown_error: i18n.t("sidebar.host_ports.toast.unknown_error"),
            filesystem_unknown_error: i18n.t("sidebar.host_filesystems.toast.unknown_error"),
            package_unknown_error: i18n.t("sidebar.host_packages.toast.unknown_error"),
            schedule_unknown_error: i18n.t("sidebar.host_schedules.toast.unknown_error"),
            tmux_unknown_error: i18n.t("sidebar.host_tmux.toast.unknown_error"),
            tmux_unavailable: i18n.t("sidebar.host_tmux.unavailable"),
            local_connection_label: i18n.t("sidebar.host_connection.local_machine"),
            redis_connection_missing: i18n.t("sidebar.host_redis.toast.connection_missing"),
            redis_probe_failed: i18n.t("sidebar.host_redis.toast.probe_failed"),
            // Consumers wrap this text into their own notice, so the fallback
            // must not carry the notice's own prefix or a placeholder.
            redis_key_action_failed: i18n.t("sidebar.host_redis.toast.action_failed"),
            redis_rename_invalid: i18n.t("sidebar.host_redis.rename.invalid"),
            redis_auth_required: i18n.t("sidebar.host_redis.auth_required"),
            redis_key_missing: i18n.t("sidebar.host_redis.value.ttl_missing"),
            redis_value_set_invalid_json: i18n.t("sidebar.host_redis.set_value.invalid_json"),
            redis_value_edit_unsupported: i18n.t("sidebar.host_redis.value.edit_unsupported"),
            redis_value_edit_conflict: i18n.t("sidebar.host_redis.value.edit_conflict"),
            redis_create_key_exists: i18n.t("sidebar.host_redis.create_key.exists"),
            redis_create_key_invalid_name: i18n.t("sidebar.host_redis.create_key.invalid_name"),
            redis_create_key_invalid_ttl: i18n.t("sidebar.host_redis.create_key.invalid_ttl"),
            redis_create_key_empty_entries: i18n.t("sidebar.host_redis.create_key.empty_entries"),
            redis_create_key_invalid_field: i18n.t("sidebar.host_redis.create_key.invalid_field"),
            redis_create_key_invalid_score: i18n.t("sidebar.host_redis.create_key.invalid_score"),
            container_capture_failed: i18n.t("sidebar.host_container.captures.failed"),
            container_terminal_opened: i18n.t("sidebar.host_container.toast.terminal_opened"),
            container_terminal_missing: i18n.t("sidebar.host_container.toast.terminal_missing"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactMonitorLayout {
    Inline,
    Stacked,
}

/// Keeps a standard Host Tools confirmation payload alive while its exit motion runs.
pub struct HostToolConfirmState<T> {
    pub request: T,
    pub presence: ExitPresence,
}

pub struct HostGpuViewState {
    #[cfg(not(target_family = "wasm"))]
    pub update_tx: tokio::sync::mpsc::UnboundedSender<GpuUpdate>,
    #[cfg(not(target_family = "wasm"))]
    pub sampling_task: Option<GpuSamplingTask>,
    pub snapshot_connection_id: Option<String>,
    pub snapshot: Option<GpuSnapshot>,
    pub expanded_uuid: Option<String>,
    pub list_state: ListState,
    pub list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostGpuViewState {
    pub fn new(
        #[cfg(not(target_family = "wasm"))] update_tx: tokio::sync::mpsc::UnboundedSender<
            GpuUpdate,
        >,
    ) -> Self {
        Self {
            #[cfg(not(target_family = "wasm"))]
            update_tx,
            #[cfg(not(target_family = "wasm"))]
            sampling_task: None,
            snapshot_connection_id: None,
            snapshot: None,
            expanded_uuid: None,
            list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_GPU_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

impl<T> HostToolConfirmState<T> {
    pub fn new(request: T) -> Self {
        Self {
            request,
            presence: ExitPresence::visible(),
        }
    }

    /// Reuses the generation so a stale timer cannot close a replacement request.
    pub fn open(slot: &mut Option<Self>, request: T) {
        if let Some(state) = slot.as_mut() {
            state.request = request;
            state.presence.reopen();
        } else {
            *slot = Some(Self::new(request));
        }
    }
}

/// Owns Host Tools input, selection, expansion, and virtual-list presentation state.
pub struct HostToolsUiState {
    pub focused_input: Option<HostToolsTextInput>,
    /// Container search text lives with the other Host Tools inputs so the
    /// shared IME plumbing can read and replace it.
    pub host_container_search_query: String,
    pub host_container_input_dialog: Option<HostContainerInputDialog>,
    pub host_process_search_query: String,
    pub host_process_filter: ProcessFilter,
    pub host_process_sort: ProcessSort,
    pub host_process_sort_descending: bool,
    pub host_process_expanded_pid: Option<String>,
    pub host_process_list_state: ListState,
    pub host_process_list_cache: RefCell<VirtualListSignatureCache>,
    pub host_process_renice_value: String,
    pub host_docker_search_query: String,
    pub host_docker_expanded_id: Option<String>,
    pub host_docker_list_state: ListState,
    pub host_docker_list_cache: RefCell<VirtualListSignatureCache>,
    pub host_service_search_query: String,
    pub host_service_expanded_id: Option<String>,
    pub host_service_list_state: ListState,
    pub host_service_list_cache: RefCell<VirtualListSignatureCache>,
    pub host_log_search_query: String,
    pub host_tmux_search_query: String,
    pub host_tmux_expanded_session_id: Option<String>,
    pub host_tmux_expanded_window_id: Option<String>,
    pub host_tmux_input_dialog: Option<HostTmuxInputDialog>,
    pub host_mux_input_dialog: Option<MuxInputDialog>,
    /// Inline rename buffer for the key title; `Some` means the editor is open.
    pub host_redis_rename_value: Option<zeroize::Zeroizing<String>>,
    pub host_redis_rename_error: Option<String>,
    /// Key search in the Redis tree; an empty query shows everything loaded.
    pub host_redis_search_query: String,
    pub host_redis_ttl_value: Option<zeroize::Zeroizing<String>>,
    pub host_redis_ttl_error: Option<String>,
    pub host_redis_set_value_dialog: Option<HostRedisSetValueDialog>,
    pub host_redis_create_key_dialog: Option<HostRedisCreateKeyDialog>,
    pub host_tmux_list_state: ListState,
    pub host_tmux_list_cache: RefCell<VirtualListSignatureCache>,
    pub host_port_search_query: String,
    pub host_schedule_search_query: String,
    pub host_package_search_query: String,
    /// Redis password buffer; zeroized when the field or panel is dropped.
    pub host_redis_password: zeroize::Zeroizing<String>,
    /// When false the tree hides databases that hold no keys.
    pub host_redis_show_all_databases: bool,
}

impl HostToolsUiState {
    pub fn new() -> Self {
        Self {
            focused_input: None,
            host_container_search_query: String::new(),
            host_container_input_dialog: None,
            host_process_search_query: String::new(),
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
            host_docker_search_query: String::new(),
            host_docker_expanded_id: None,
            host_docker_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_docker_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_service_search_query: String::new(),
            host_service_expanded_id: None,
            host_service_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_service_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_log_search_query: String::new(),
            host_tmux_search_query: String::new(),
            host_tmux_expanded_session_id: None,
            host_tmux_expanded_window_id: None,
            host_tmux_input_dialog: None,
            host_mux_input_dialog: None,
            host_redis_rename_value: None,
            host_redis_rename_error: None,
            host_redis_search_query: String::new(),
            host_redis_ttl_value: None,
            host_redis_ttl_error: None,
            host_redis_set_value_dialog: None,
            host_redis_create_key_dialog: None,
            host_tmux_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_tmux_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_port_search_query: String::new(),
            host_schedule_search_query: String::new(),
            host_package_search_query: String::new(),
            host_redis_password: zeroize::Zeroizing::new(String::new()),
            host_redis_show_all_databases: false,
        }
    }

    pub fn focus_input(&mut self, input: HostToolsTextInput) {
        self.focused_input = Some(input);
    }

    pub fn clear_input_focus(&mut self) {
        self.focused_input = None;
    }

    pub fn retain_input_focus_for_tool(&mut self, tool: ContextSidebarTool) {
        let belongs_to_tool = matches!(
            (self.focused_input, tool),
            (
                Some(HostToolsTextInput::ProcessSearch | HostToolsTextInput::ProcessRenice),
                ContextSidebarTool::Processes
            ) | (
                Some(HostToolsTextInput::DockerSearch),
                ContextSidebarTool::Docker
            ) | (
                Some(HostToolsTextInput::ServiceSearch),
                ContextSidebarTool::Services
            ) | (
                Some(HostToolsTextInput::LogSearch),
                ContextSidebarTool::Logs
            ) | (
                Some(
                    HostToolsTextInput::TmuxSearch
                        | HostToolsTextInput::TmuxDialog
                        | HostToolsTextInput::MuxDialog,
                ),
                ContextSidebarTool::Tmux
            ) | (
                Some(HostToolsTextInput::PortSearch),
                ContextSidebarTool::Ports
            ) | (
                Some(HostToolsTextInput::ScheduleSearch),
                ContextSidebarTool::Schedules
            ) | (
                Some(HostToolsTextInput::FilesystemSearch),
                ContextSidebarTool::Filesystems
            ) | (
                Some(HostToolsTextInput::PackageSearch),
                ContextSidebarTool::Packages
            ) | (
                Some(
                    HostToolsTextInput::ContainerSearch
                        | HostToolsTextInput::ContainerDialog
                        | HostToolsTextInput::ContainerDialogRegistry
                ),
                ContextSidebarTool::Container
            ) | (
                Some(
                    HostToolsTextInput::RedisPassword
                        | HostToolsTextInput::RedisRename
                        | HostToolsTextInput::RedisSetValue
                        | HostToolsTextInput::RedisSetValueField
                        | HostToolsTextInput::RedisCreateKeyName
                        | HostToolsTextInput::RedisCreateKeyValue
                        | HostToolsTextInput::RedisCreateKeyField
                        | HostToolsTextInput::RedisCreateKeyTtl
                ),
                ContextSidebarTool::Redis
            )
        );
        if !belongs_to_tool {
            self.clear_input_focus();
        }
    }

    pub fn input_is_focused(&self, input: HostToolsTextInput) -> bool {
        self.focused_input == Some(input)
    }

    pub fn input_value(&self, input: HostToolsTextInput) -> Option<&str> {
        if !self.input_is_focused(input) {
            return None;
        }
        match input {
            HostToolsTextInput::ProcessSearch => Some(&self.host_process_search_query),
            HostToolsTextInput::ProcessRenice => Some(&self.host_process_renice_value),
            HostToolsTextInput::DockerSearch => Some(&self.host_docker_search_query),
            HostToolsTextInput::ServiceSearch => Some(&self.host_service_search_query),
            HostToolsTextInput::LogSearch => Some(&self.host_log_search_query),
            HostToolsTextInput::TmuxSearch => Some(&self.host_tmux_search_query),
            HostToolsTextInput::TmuxDialog => self
                .host_tmux_input_dialog
                .as_ref()
                .map(|dialog| dialog.value.as_str()),
            HostToolsTextInput::MuxDialog => self
                .host_mux_input_dialog
                .as_ref()
                .map(|dialog| dialog.value.as_str()),
            HostToolsTextInput::PortSearch => Some(&self.host_port_search_query),
            HostToolsTextInput::ScheduleSearch => Some(&self.host_schedule_search_query),
            HostToolsTextInput::FilesystemSearch => None,
            HostToolsTextInput::PackageSearch => Some(&self.host_package_search_query),
            HostToolsTextInput::ContainerSearch => Some(&self.host_container_search_query),
            HostToolsTextInput::ContainerDialog => self
                .host_container_input_dialog
                .as_ref()
                .map(|dialog| dialog.value.as_str()),
            HostToolsTextInput::ContainerDialogRegistry => self
                .host_container_input_dialog
                .as_ref()
                .map(|dialog| dialog.registry.as_str()),
            HostToolsTextInput::ContainerRunName => self
                .host_container_input_dialog
                .as_ref()
                .map(|dialog| dialog.run.name.as_str()),
            HostToolsTextInput::ContainerRunPorts => self
                .host_container_input_dialog
                .as_ref()
                .map(|dialog| dialog.run.ports.as_str()),
            HostToolsTextInput::ContainerRunVolumes => self
                .host_container_input_dialog
                .as_ref()
                .map(|dialog| dialog.run.volumes.as_str()),
            HostToolsTextInput::ContainerRunCommand => self
                .host_container_input_dialog
                .as_ref()
                .map(|dialog| dialog.run.command.as_str()),
            HostToolsTextInput::RedisPassword => Some(&self.host_redis_password),
            HostToolsTextInput::RedisRename => self.host_redis_rename_value(),
            HostToolsTextInput::RedisSetValue => self.host_redis_set_value(),
            HostToolsTextInput::RedisSetValueField => self.host_redis_set_value_field(),
            HostToolsTextInput::RedisCreateKeyName => self.host_redis_create_key_name(),
            HostToolsTextInput::RedisCreateKeyValue => self.host_redis_create_key_value(),
            HostToolsTextInput::RedisCreateKeyField => self.host_redis_create_key_field(),
            HostToolsTextInput::RedisCreateKeyTtl => self.host_redis_create_key_ttl(),
            HostToolsTextInput::RedisTtl => self.host_redis_ttl_value(),
            HostToolsTextInput::RedisSearch => Some(&self.host_redis_search_query),
        }
    }

    pub fn host_redis_create_key_name(&self) -> Option<&str> {
        self.host_redis_create_key_dialog
            .as_ref()
            .map(|dialog| dialog.name.as_str())
    }

    pub fn host_redis_create_key_name_mut(&mut self) -> Option<&mut String> {
        self.host_redis_create_key_dialog
            .as_mut()
            .map(|dialog| &mut *dialog.name)
    }

    pub fn host_redis_create_key_field(&self) -> Option<&str> {
        self.host_redis_create_key_dialog
            .as_ref()
            .map(|dialog| dialog.field.as_str())
    }

    pub fn host_redis_create_key_field_mut(&mut self) -> Option<&mut String> {
        self.host_redis_create_key_dialog
            .as_mut()
            .map(|dialog| &mut *dialog.field)
    }

    pub fn host_redis_create_key_ttl(&self) -> Option<&str> {
        self.host_redis_create_key_dialog
            .as_ref()
            .map(|dialog| dialog.ttl.as_str())
    }

    pub fn host_redis_create_key_ttl_mut(&mut self) -> Option<&mut String> {
        self.host_redis_create_key_dialog
            .as_mut()
            .map(|dialog| &mut *dialog.ttl)
    }

    pub fn host_redis_create_key_value(&self) -> Option<&str> {
        self.host_redis_create_key_dialog
            .as_ref()
            .map(|dialog| dialog.value.as_str())
    }

    pub fn host_redis_create_key_value_mut(&mut self) -> Option<&mut String> {
        self.host_redis_create_key_dialog
            .as_mut()
            .map(|dialog| &mut *dialog.value)
    }

    pub fn host_redis_set_value_field(&self) -> Option<&str> {
        self.host_redis_set_value_dialog
            .as_ref()
            .map(|dialog| dialog.field.as_str())
    }

    pub fn host_redis_set_value_field_mut(&mut self) -> Option<&mut String> {
        self.host_redis_set_value_dialog
            .as_mut()
            .map(|dialog| &mut *dialog.field)
    }

    pub fn host_redis_set_value(&self) -> Option<&str> {
        self.host_redis_set_value_dialog
            .as_ref()
            .map(|dialog| dialog.value.as_str())
    }

    pub fn host_redis_set_value_mut(&mut self) -> Option<&mut String> {
        self.host_redis_set_value_dialog
            .as_mut()
            .map(|dialog| &mut *dialog.value)
    }

    pub fn host_redis_rename_value(&self) -> Option<&str> {
        self.host_redis_rename_value.as_deref().map(String::as_str)
    }

    pub fn host_redis_ttl_value(&self) -> Option<&str> {
        self.host_redis_ttl_value.as_deref().map(String::as_str)
    }

    pub fn host_redis_ttl_value_mut(&mut self) -> Option<&mut String> {
        self.host_redis_ttl_value.as_mut().map(|value| &mut **value)
    }

    pub fn host_redis_rename_value_mut(&mut self) -> Option<&mut String> {
        self.host_redis_rename_value
            .as_mut()
            .map(|value| &mut **value)
    }

    pub fn input_value_mut(&mut self, input: HostToolsTextInput) -> Option<&mut String> {
        if !self.input_is_focused(input) {
            return None;
        }
        match input {
            HostToolsTextInput::ProcessSearch => Some(&mut self.host_process_search_query),
            HostToolsTextInput::ProcessRenice => Some(&mut self.host_process_renice_value),
            HostToolsTextInput::DockerSearch => Some(&mut self.host_docker_search_query),
            HostToolsTextInput::ServiceSearch => Some(&mut self.host_service_search_query),
            HostToolsTextInput::LogSearch => Some(&mut self.host_log_search_query),
            HostToolsTextInput::TmuxSearch => Some(&mut self.host_tmux_search_query),
            HostToolsTextInput::TmuxDialog => self
                .host_tmux_input_dialog
                .as_mut()
                .map(|dialog| &mut *dialog.value),
            HostToolsTextInput::MuxDialog => self
                .host_mux_input_dialog
                .as_mut()
                .map(|dialog| &mut *dialog.value),
            HostToolsTextInput::PortSearch => Some(&mut self.host_port_search_query),
            HostToolsTextInput::ScheduleSearch => Some(&mut self.host_schedule_search_query),
            HostToolsTextInput::FilesystemSearch => None,
            HostToolsTextInput::PackageSearch => Some(&mut self.host_package_search_query),
            HostToolsTextInput::ContainerSearch => Some(&mut self.host_container_search_query),
            HostToolsTextInput::ContainerDialog => self
                .host_container_input_dialog
                .as_mut()
                .map(|dialog| &mut dialog.value),
            HostToolsTextInput::ContainerDialogRegistry => self
                .host_container_input_dialog
                .as_mut()
                .map(|dialog| &mut dialog.registry),
            HostToolsTextInput::ContainerRunName => self
                .host_container_input_dialog
                .as_mut()
                .map(|dialog| &mut dialog.run.name),
            HostToolsTextInput::ContainerRunPorts => self
                .host_container_input_dialog
                .as_mut()
                .map(|dialog| &mut dialog.run.ports),
            HostToolsTextInput::ContainerRunVolumes => self
                .host_container_input_dialog
                .as_mut()
                .map(|dialog| &mut dialog.run.volumes),
            HostToolsTextInput::ContainerRunCommand => self
                .host_container_input_dialog
                .as_mut()
                .map(|dialog| &mut dialog.run.command),
            HostToolsTextInput::RedisPassword => Some(&mut self.host_redis_password),
            HostToolsTextInput::RedisRename => self.host_redis_rename_value_mut(),
            HostToolsTextInput::RedisSetValue => self.host_redis_set_value_mut(),
            HostToolsTextInput::RedisSetValueField => self.host_redis_set_value_field_mut(),
            HostToolsTextInput::RedisCreateKeyName => self.host_redis_create_key_name_mut(),
            HostToolsTextInput::RedisCreateKeyValue => self.host_redis_create_key_value_mut(),
            HostToolsTextInput::RedisCreateKeyField => self.host_redis_create_key_field_mut(),
            HostToolsTextInput::RedisCreateKeyTtl => self.host_redis_create_key_ttl_mut(),
            HostToolsTextInput::RedisTtl => self.host_redis_ttl_value_mut(),
            HostToolsTextInput::RedisSearch => Some(&mut self.host_redis_search_query),
        }
    }
}
