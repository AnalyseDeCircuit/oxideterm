use std::time::Duration;

use gpui::{Rgba, rgb, rgba};
use oxideterm_gpui_ui::motion::ExitPresence;
use oxideterm_ssh::SshCommandOutput;
use oxideterm_topology::TopologyViewStatus;

use super::*;

pub(super) const HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(super) const HOST_GPU_LIST_ESTIMATED_ROW_HEIGHT: f32 = 72.0;
pub(super) const HOST_GPU_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_GPU_UTILIZATION_COLUMN_WIDTH: f32 = 58.0;
pub(super) const HOST_GPU_MEMORY_COLUMN_WIDTH: f32 = 92.0;
pub(super) const HOST_PROCESS_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_PROCESS_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(super) const HOST_PROCESS_USER_COLUMN_WIDTH: f32 = 64.0;
pub(super) const HOST_PROCESS_PID_COLUMN_WIDTH: f32 = 54.0;
pub(super) const HOST_PROCESS_CPU_COLUMN_WIDTH: f32 = 44.0;
pub(super) const HOST_PROCESS_MEMORY_COLUMN_WIDTH: f32 = 48.0;
pub(super) const HOST_PROCESS_SEPARATE_USER_COLUMN_MIN_WIDTH: f32 = 620.0;
pub(super) const HOST_PROCESS_TABLE_HEADER_TEXT_SIZE: f32 = 10.0;
pub(super) const HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE: f32 = 12.0;
pub(super) const HOST_PROCESS_TABLE_META_TEXT_SIZE: f32 = 10.0;
pub(super) const HOST_PROCESS_TABLE_VALUE_TEXT_SIZE: f32 = 11.0;
pub(super) const HOST_PROCESS_DETAIL_TEXT_SIZE: f32 = 11.0;
pub(super) const HOST_PROCESS_ACTION_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const HOST_PROCESS_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(super) const HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(super) const HOST_DOCKER_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_DOCKER_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(super) const HOST_DOCKER_STATE_COLUMN_WIDTH: f32 = 72.0;
pub(super) const HOST_DOCKER_PORTS_COLUMN_MIN_WIDTH: f32 = 92.0;
pub(super) const HOST_DOCKER_ACTION_TIMEOUT: Duration = Duration::from_secs(12);
pub(super) const HOST_DOCKER_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(super) const HOST_DOCKER_LOGS_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const HOST_DOCKER_LOGS_MAX_OUTPUT_SIZE: usize = 128 * 1024;
pub(super) const HOST_DOCKER_LOGS_DIALOG_WIDTH: f32 = 760.0;
pub(super) const HOST_DOCKER_LOGS_DIALOG_MAX_HEIGHT: f32 = 520.0;
pub(super) const HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(super) const HOST_SERVICE_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_SERVICE_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(super) const HOST_SERVICE_STATE_COLUMN_WIDTH: f32 = 78.0;
pub(super) const HOST_SERVICE_ENABLED_COLUMN_WIDTH: f32 = 70.0;
pub(super) const HOST_SERVICE_PID_COLUMN_WIDTH: f32 = 54.0;
pub(super) const HOST_SERVICE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(12);
pub(super) const HOST_SERVICE_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(super) const HOST_SERVICE_LOGS_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const HOST_SERVICE_ACTION_TIMEOUT: Duration = Duration::from_secs(15);
pub(super) const HOST_SERVICE_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(super) const HOST_SERVICE_LOGS_MAX_OUTPUT_SIZE: usize = 128 * 1024;
pub(super) const HOST_SERVICE_LOGS_DIALOG_WIDTH: f32 = 760.0;
pub(super) const HOST_SERVICE_LOGS_DIALOG_MAX_HEIGHT: f32 = 520.0;
pub(super) const HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT: f32 = 56.0;
pub(super) const HOST_LOG_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_LOG_TIME_COLUMN_WIDTH: f32 = 92.0;
pub(super) const HOST_LOG_LEVEL_COLUMN_WIDTH: f32 = 58.0;
pub(super) const HOST_LOG_SOURCE_COLUMN_WIDTH: f32 = 96.0;
pub(super) const HOST_LOG_UNIT_COLUMN_WIDTH: f32 = 96.0;
pub(super) const HOST_LOG_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 680.0;
pub(super) const HOST_LOG_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const HOST_LOG_SNAPSHOT_LIMIT: usize = 300;
pub(super) const HOST_LOG_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(super) const HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(super) const HOST_TMUX_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_TMUX_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(super) const HOST_TMUX_ATTACHED_COLUMN_WIDTH: f32 = 74.0;
pub(super) const HOST_TMUX_WINDOWS_COLUMN_WIDTH: f32 = 58.0;
pub(super) const HOST_TMUX_PANES_COLUMN_WIDTH: f32 = 48.0;
pub(super) const HOST_TMUX_ACTIVITY_COLUMN_WIDTH: f32 = 92.0;
pub(super) const HOST_TMUX_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 620.0;
pub(super) const HOST_TMUX_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const HOST_TMUX_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 128 * 1024;
pub(super) const HOST_TMUX_ACTION_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const HOST_TMUX_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(super) const HOST_TMUX_INPUT_DIALOG_WIDTH: f32 = 460.0;
pub(super) const HOST_PORT_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(super) const HOST_PORT_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_PORT_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(super) const HOST_PORT_PROTOCOL_COLUMN_WIDTH: f32 = 46.0;
pub(super) const HOST_PORT_STATE_COLUMN_WIDTH: f32 = 78.0;
pub(super) const HOST_PORT_PID_COLUMN_WIDTH: f32 = 58.0;
pub(super) const HOST_PORT_PROCESS_COLUMN_WIDTH: f32 = 96.0;
pub(super) const HOST_PORT_REMOTE_COLUMN_WIDTH: f32 = 132.0;
pub(super) const HOST_PORT_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 680.0;
pub(super) const HOST_PORT_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const HOST_PORT_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(super) const HOST_SCHEDULE_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(super) const HOST_SCHEDULE_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_SCHEDULE_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(super) const HOST_SCHEDULE_SOURCE_COLUMN_WIDTH: f32 = 74.0;
pub(super) const HOST_SCHEDULE_STATE_COLUMN_WIDTH: f32 = 78.0;
pub(super) const HOST_SCHEDULE_ENABLED_COLUMN_WIDTH: f32 = 72.0;
pub(super) const HOST_SCHEDULE_NEXT_COLUMN_WIDTH: f32 = 112.0;
pub(super) const HOST_SCHEDULE_LAST_COLUMN_WIDTH: f32 = 112.0;
pub(super) const HOST_SCHEDULE_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 720.0;
pub(super) const HOST_SCHEDULE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const HOST_SCHEDULE_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 256 * 1024;
pub(super) const HOST_SCHEDULE_ACTION_TIMEOUT: Duration = Duration::from_secs(12);
pub(super) const HOST_SCHEDULE_ACTION_MAX_OUTPUT_SIZE: usize = 4096;
pub(super) const HOST_SCHEDULE_LOGS_TIMEOUT: Duration = Duration::from_secs(8);
pub(super) const HOST_SCHEDULE_LOGS_MAX_OUTPUT_SIZE: usize = 128 * 1024;
pub(super) const HOST_SCHEDULE_LOGS_DIALOG_WIDTH: f32 = 760.0;
pub(super) const HOST_SCHEDULE_LOGS_DIALOG_MAX_HEIGHT: f32 = 520.0;
pub(super) const HOST_FILESYSTEM_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(super) const HOST_FILESYSTEM_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_FILESYSTEM_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(super) const HOST_FILESYSTEM_KIND_COLUMN_WIDTH: f32 = 74.0;
pub(super) const HOST_FILESYSTEM_USAGE_COLUMN_WIDTH: f32 = 70.0;
pub(super) const HOST_FILESYSTEM_INODE_COLUMN_WIDTH: f32 = 64.0;
pub(super) const HOST_FILESYSTEM_FS_COLUMN_WIDTH: f32 = 74.0;
pub(super) const HOST_FILESYSTEM_RO_COLUMN_WIDTH: f32 = 48.0;
pub(super) const HOST_FILESYSTEM_SIZE_COLUMN_WIDTH: f32 = 104.0;
pub(super) const HOST_FILESYSTEM_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 720.0;
pub(super) const HOST_FILESYSTEM_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(15);
pub(super) const HOST_FILESYSTEM_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 512 * 1024;
pub(super) const HOST_PACKAGE_LIST_ESTIMATED_ROW_HEIGHT: f32 = 64.0;
pub(super) const HOST_PACKAGE_TABLE_HEADER_HEIGHT: f32 = 28.0;
pub(super) const HOST_PACKAGE_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
pub(super) const HOST_PACKAGE_STATUS_COLUMN_WIDTH: f32 = 84.0;
pub(super) const HOST_PACKAGE_VERSION_COLUMN_WIDTH: f32 = 116.0;
pub(super) const HOST_PACKAGE_MANAGER_COLUMN_WIDTH: f32 = 66.0;
pub(super) const HOST_PACKAGE_SERVICE_COLUMN_WIDTH: f32 = 108.0;
pub(super) const HOST_PACKAGE_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 720.0;
pub(super) const HOST_PACKAGE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(18);
pub(super) const HOST_PACKAGE_SNAPSHOT_MAX_OUTPUT_SIZE: usize = 512 * 1024;

pub(super) const MONITOR_POOL_REFRESH_INTERVAL: Duration = Duration::from_millis(2000);
pub(super) const MONITOR_SPARKLINE_POINTS: usize = 12;
// The compact sidebar must stay on GPUI List scrolling; ordinary Div overflow
// repaints too much of the Host Tools panel during trackpad scrolling.
pub(super) const COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT: f32 = 34.0;
pub(super) const COMPACT_MONITOR_LIST_OVERSCAN: usize = 8;
pub(super) const COMPACT_MONITOR_METRIC_ROW_HEIGHT: f32 = 32.0;
// Network rows use the extra height only when their values must stack.
pub(super) const COMPACT_MONITOR_STACKED_ROW_HEIGHT: f32 = 52.0;
pub(super) const COMPACT_MONITOR_SECTION_ROW_HEIGHT: f32 = 32.0;
pub(super) const COMPACT_MONITOR_DETAIL_ROW_HEIGHT: f32 = 28.0;
pub(super) const COMPACT_MONITOR_RETRY_ROW_HEIGHT: f32 = 44.0;
// Compact monitoring rows use the same space-efficient inset at every width.
pub(super) const COMPACT_MONITOR_ROW_SIDE_PADDING: f32 = 12.0;
pub(super) const COMPACT_MONITOR_STACKED_LAYOUT_MAX_WIDTH: f32 = 360.0;
pub(super) const COMPACT_MONITOR_VALUE_MAX_WIDTH_RATIO: f32 = 0.58;
pub(super) const COMPACT_MONITOR_DETAIL_VALUE_MAX_WIDTH_RATIO: f32 = 0.55;
pub(super) const COMPACT_MONITOR_DETAIL_INDENT: f32 = 22.0;
pub(super) const MONITOR_PAGE_PADDING: f32 = 32.0;
pub(super) const MONITOR_SECTION_GAP: f32 = 32.0;
pub(super) const MONITOR_SPARKLINE_HEIGHT: f32 = 28.0;
pub(super) const MONITOR_SPARKLINE_STROKE_WIDTH: f32 = 1.5;
pub(super) const MONITOR_SPARKLINE_STROKE_ALPHA: u32 = 0x99;
pub(super) const MONITOR_BORDER_ALPHA: u32 = 0x80;
pub(super) const MONITOR_SOURCE_ALPHA: u32 = 0x80;
pub(super) const MONITOR_TINT_ALPHA: u32 = 0x1a;
pub(super) const MONITOR_EMERALD: u32 = 0x34d399;
pub(super) const MONITOR_EMERALD_DARK: u32 = 0x10b981;
pub(super) const MONITOR_AMBER: u32 = 0xf59e0b;
pub(super) const MONITOR_RED: u32 = 0xef4444;
pub(super) const MONITOR_BLUE: u32 = 0x3b82f6;
pub(super) const TOPOLOGY_BG_GRID_STEP: f32 = 40.0;
pub(super) const TOPOLOGY_BG_GRID_ALPHA: u32 = 0x1a;
pub(super) const TOPOLOGY_PANEL_BG_ALPHA_20: u32 = 0x33;
pub(super) const TOPOLOGY_PANEL_BORDER_ALPHA_50: u32 = 0x80;
pub(super) const TOPOLOGY_MUTED_TEXT_ALPHA_70: u32 = 0xb3;
pub(super) const TOPOLOGY_INSTRUCTION_ALPHA_60: u32 = 0x99;
pub(super) const TOPOLOGY_LINE_INACTIVE_ALPHA: u32 = 0x66;
pub(super) const TOPOLOGY_LINE_GLOW_ALPHA: u32 = 0x26;
pub(super) const TOPOLOGY_CONNECTED: u32 = 0x22c55e;
pub(super) const TOPOLOGY_CONNECTING: u32 = 0xeab308;
pub(super) const TOPOLOGY_FAILED: u32 = 0xef4444;
pub(super) const TOPOLOGY_DISCONNECTED: u32 = 0x71717a;
pub(super) const TOPOLOGY_PENDING: u32 = 0xf59e0b;
pub(super) const TOPOLOGY_ZOOM_INITIAL: f32 = 0.9;
pub(super) const TOPOLOGY_ZOOM_MIN: f32 = 0.3;
pub(super) const TOPOLOGY_ZOOM_MAX: f32 = 3.0;
pub(super) const TOPOLOGY_PAN_INITIAL_X: f32 = 0.0;
pub(super) const TOPOLOGY_PAN_INITIAL_Y: f32 = 50.0;
pub(super) const TOPOLOGY_MENU_WIDTH: f32 = 180.0;
pub(super) const TOPOLOGY_MENU_MAX_HEIGHT: f32 = 250.0;

pub(super) fn connection_monitor_surface_bg(theme_bg: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba(0x00000000)
    } else {
        rgb(theme_bg)
    }
}

#[derive(Clone, Copy)]
pub(super) struct TopologyTransform {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) k: f32,
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
pub(super) struct TopologyDragState {
    pub(super) last_x: f32,
    pub(super) last_y: f32,
}

#[derive(Clone, Copy)]
pub(super) struct HostToolsTabScrollbarDragState {
    pub(super) grab_offset_x: f32,
}

#[derive(Clone)]
pub(super) struct TopologyNodeMenuState {
    pub(super) node_id: Option<NodeId>,
    pub(super) name: String,
    pub(super) host: String,
    pub(super) view_status: TopologyViewStatus,
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MonitorConnectionOption {
    // Sidebar monitoring only needs selector/header fields; avoid cloning the
    // full registry connection payload on every scroll-driven render.
    pub(super) connection_id: String,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) username: String,
}

impl MonitorConnectionOption {
    pub(super) fn from_connection_info(connection: oxideterm_ssh::ConnectionInfo) -> Self {
        Self {
            connection_id: connection.connection_id,
            host: connection.host,
            port: connection.port,
            username: connection.username,
        }
    }

    pub(super) fn from_pool_summary(summary: &ConnectionPoolEntrySummary) -> Self {
        Self {
            connection_id: summary.id.clone(),
            host: summary.host.clone(),
            port: summary.port,
            username: summary.username.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct HostProcessActionRequest {
    pub(super) connection_id: String,
    pub(super) pid: String,
    // Process names can contain credential-like command fragments. All UI
    // clones share one buffer that is cleared when the confirmation closes.
    pub(super) display_command: Arc<zeroize::Zeroizing<String>>,
    pub(super) action: ProcessActionKind,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostProcessActionRun {
    pub(super) connection_id: String,
    pub(super) pid: String,
    pub(super) action: ProcessActionKind,
}

pub(super) struct HostProcessActionDelivery {
    pub(super) request: HostProcessActionRun,
    pub(super) result: Result<bool, ()>,
}

pub(super) struct HostProcessActionsState {
    pub(super) pending_confirm: Option<HostToolConfirmState<HostProcessActionRequest>>,
    pub(super) running: Option<HostProcessActionRun>,
}

impl HostProcessActionsState {
    pub(super) fn new() -> Self {
        Self {
            pending_confirm: None,
            running: None,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostDockerActionRequest {
    pub(super) connection_id: String,
    pub(super) container_id: String,
    pub(super) container_name: String,
    pub(super) action: DockerActionKind,
}

pub(super) struct HostDockerActionDelivery {
    pub(super) request: HostDockerActionRequest,
    pub(super) result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostDockerLogsRequest {
    pub(super) connection_id: String,
    pub(super) container_id: String,
    pub(super) container_name: String,
    pub(super) failure_fallback: String,
    pub(super) empty_fallback: String,
}

pub(super) struct HostDockerLogsDelivery {
    pub(super) request: HostDockerLogsRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostDockerLogsDialog {
    pub(super) request: HostDockerLogsRequest,
    // Docker output stays in one shared zeroizing buffer while rendered.
    pub(super) output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub(super) error: Option<String>,
    pub(super) loading: bool,
}

pub(super) struct HostDockerOperationsState {
    pub(super) pending_confirm: Option<HostToolConfirmState<HostDockerActionRequest>>,
    pub(super) action_running: Option<HostDockerActionRequest>,
    pub(super) logs_dialog: Option<HostDockerLogsDialog>,
}

impl HostDockerOperationsState {
    pub(super) fn new() -> Self {
        Self {
            pending_confirm: None,
            action_running: None,
            logs_dialog: None,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostServiceSnapshotRequest {
    pub(super) connection_id: String,
    pub(super) connection_fallback: String,
    pub(super) failure_fallback: String,
}

pub(super) struct HostServiceSnapshotDelivery {
    pub(super) request: HostServiceSnapshotRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

pub(super) struct HostServiceSnapshotPending {
    pub(super) request: HostServiceSnapshotRequest,
    pub(super) runtime: tokio::runtime::Handle,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostServiceActionRequest {
    pub(super) connection_id: String,
    pub(super) service_id: String,
    pub(super) description: String,
    pub(super) action: ServiceActionKind,
}

pub(super) struct HostServiceActionDelivery {
    pub(super) request: HostServiceActionRequest,
    pub(super) result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostServiceLogsRequest {
    pub(super) connection_id: String,
    pub(super) service_id: String,
    pub(super) description: String,
    pub(super) failure_fallback: String,
    pub(super) empty_fallback: String,
}

pub(super) struct HostServiceLogsDelivery {
    pub(super) request: HostServiceLogsRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostServiceLogsDialog {
    pub(super) request: HostServiceLogsRequest,
    // Service output stays in one shared zeroizing buffer while rendered.
    pub(super) output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub(super) error: Option<String>,
    pub(super) loading: bool,
}

pub(super) struct HostServicesState {
    pub(super) snapshot_connection_id: Option<String>,
    pub(super) snapshot: Option<oxideterm_connection_monitor::ResourceServiceSnapshot>,
    pub(super) snapshot_running: Option<HostServiceSnapshotRequest>,
    pub(super) snapshot_pending: Option<HostServiceSnapshotPending>,
    pub(super) snapshot_polling: bool,
    pub(super) pending_confirm: Option<HostToolConfirmState<HostServiceActionRequest>>,
    pub(super) action_running: Option<HostServiceActionRequest>,
    pub(super) logs_dialog: Option<HostServiceLogsDialog>,
}

impl HostServicesState {
    pub(super) fn new() -> Self {
        Self {
            snapshot_connection_id: None,
            snapshot: None,
            snapshot_running: None,
            snapshot_pending: None,
            snapshot_polling: false,
            pending_confirm: None,
            action_running: None,
            logs_dialog: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostSnapshotFeedback {
    Silent,
    Toast,
}

impl HostSnapshotFeedback {
    pub(super) fn should_toast(self) -> bool {
        matches!(self, Self::Toast)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HostLogSnapshotRequest {
    pub(super) connection_id: String,
    pub(super) preset: LogPreset,
    pub(super) limit: usize,
    pub(super) feedback: HostSnapshotFeedback,
    pub(super) failure_fallback: String,
}

pub(super) struct HostLogSnapshotDelivery {
    pub(super) request: HostLogSnapshotRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

pub(super) struct HostLogsState {
    pub(super) expanded_index: Option<usize>,
    pub(super) preset: LogPreset,
    pub(super) snapshot_connection_id: Option<String>,
    pub(super) snapshot: Option<ResourceLogSnapshot>,
    pub(super) running: Option<HostLogSnapshotRequest>,
    pub(super) polling: bool,
    pub(super) list_state: ListState,
    pub(super) list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostLogsState {
    pub(super) fn new() -> Self {
        Self {
            expanded_index: None,
            preset: LogPreset::All,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            polling: false,
            list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_LOG_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HostTmuxSnapshotRequest {
    pub(super) connection_id: String,
    pub(super) feedback: HostSnapshotFeedback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HostTmuxSnapshotDelivery {
    pub(super) request: HostTmuxSnapshotRequest,
    pub(super) result: Result<SshCommandOutput, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HostTmuxActionRequest {
    pub(super) connection_id: String,
    pub(super) session_id: String,
    pub(super) session_name: String,
    pub(super) target_label: String,
    pub(super) action: TmuxActionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HostTmuxActionDelivery {
    pub(super) request: HostTmuxActionRequest,
    pub(super) result: Result<SshCommandOutput, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HostPortSnapshotRequest {
    pub(super) connection_id: String,
    pub(super) feedback: HostSnapshotFeedback,
    pub(super) failure_fallback: String,
}

pub(super) struct HostPortSnapshotDelivery {
    pub(super) request: HostPortSnapshotRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

pub(super) struct HostPortsState {
    pub(super) filter: PortFilter,
    pub(super) expanded_index: Option<usize>,
    pub(super) snapshot_connection_id: Option<String>,
    pub(super) snapshot: Option<ResourcePortSnapshot>,
    pub(super) running: Option<HostPortSnapshotRequest>,
    pub(super) polling: bool,
    pub(super) list_state: ListState,
    pub(super) list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostPortsState {
    pub(super) fn new() -> Self {
        Self {
            filter: PortFilter::All,
            expanded_index: None,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            polling: false,
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
pub(super) struct HostScheduleSnapshotRequest {
    pub(super) connection_id: String,
    pub(super) feedback: HostSnapshotFeedback,
    pub(super) failure_fallback: String,
}

pub(super) struct HostScheduleSnapshotDelivery {
    pub(super) request: HostScheduleSnapshotRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

pub(super) struct HostSchedulesState {
    pub(super) filter: ScheduledTaskFilter,
    pub(super) expanded_index: Option<usize>,
    pub(super) snapshot_connection_id: Option<String>,
    pub(super) snapshot: Option<ResourceScheduledTaskSnapshot>,
    pub(super) running: Option<HostScheduleSnapshotRequest>,
    pub(super) polling: bool,
    pub(super) list_state: ListState,
    pub(super) list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) pending_confirm: Option<HostToolConfirmState<HostScheduleActionRequest>>,
    pub(super) action_running: Option<HostScheduleActionRequest>,
    pub(super) logs_dialog: Option<HostScheduleLogsDialog>,
}

impl HostSchedulesState {
    pub(super) fn new() -> Self {
        Self {
            filter: ScheduledTaskFilter::All,
            expanded_index: None,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            polling: false,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HostFilesystemSnapshotRequest {
    pub(super) connection_id: String,
    pub(super) feedback: HostSnapshotFeedback,
    pub(super) failure_fallback: String,
}

pub(super) struct HostFilesystemSnapshotDelivery {
    pub(super) request: HostFilesystemSnapshotRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

pub(super) struct HostFilesystemsState {
    pub(super) filter: FilesystemFilter,
    pub(super) expanded_index: Option<usize>,
    pub(super) snapshot_connection_id: Option<String>,
    pub(super) snapshot: Option<ResourceFilesystemSnapshot>,
    pub(super) running: Option<HostFilesystemSnapshotRequest>,
    pub(super) polling: bool,
    pub(super) list_state: ListState,
    pub(super) list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostFilesystemsState {
    pub(super) fn new() -> Self {
        Self {
            filter: FilesystemFilter::All,
            expanded_index: None,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            polling: false,
            list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_FILESYSTEM_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HostPackageSnapshotRequest {
    pub(super) connection_id: String,
    pub(super) feedback: HostSnapshotFeedback,
    pub(super) failure_fallback: String,
}

pub(super) struct HostPackageSnapshotDelivery {
    pub(super) request: HostPackageSnapshotRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

pub(super) struct HostPackagesState {
    pub(super) filter: PackageFilter,
    pub(super) expanded_index: Option<usize>,
    pub(super) snapshot_connection_id: Option<String>,
    pub(super) snapshot: Option<ResourcePackageSnapshot>,
    pub(super) running: Option<HostPackageSnapshotRequest>,
    pub(super) polling: bool,
    pub(super) list_state: ListState,
    pub(super) list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostPackagesState {
    pub(super) fn new() -> Self {
        Self {
            filter: PackageFilter::All,
            expanded_index: None,
            snapshot_connection_id: None,
            snapshot: None,
            running: None,
            polling: false,
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
pub(super) struct HostScheduleActionRequest {
    pub(super) connection_id: String,
    pub(super) task_id: String,
    pub(super) task_name: String,
    pub(super) unit: String,
    pub(super) action: ScheduledTaskActionKind,
}

pub(super) struct HostScheduleActionDelivery {
    pub(super) request: HostScheduleActionRequest,
    pub(super) result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostScheduleLogsRequest {
    pub(super) connection_id: String,
    // Keep sampled commands out of asynchronous deliveries; logs need only
    // the public task identity used by the dialog and follow action.
    pub(super) task_id: String,
    pub(super) task_name: String,
    pub(super) task_source: String,
    pub(super) task_unit: String,
    pub(super) failure_fallback: String,
    pub(super) empty_fallback: String,
}

pub(super) struct HostScheduleLogsDelivery {
    pub(super) request: HostScheduleLogsRequest,
    pub(super) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct HostScheduleLogsDialog {
    pub(super) request: HostScheduleLogsRequest,
    // The Entity and render tree share one zeroizing capture buffer.
    pub(super) output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub(super) error: Option<String>,
    pub(super) loading: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum HostTmuxInputDialogKind {
    RenameSession { target: String },
    RenameWindow { target: String },
    SendPaneCommand { target: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct HostTmuxInputDialog {
    pub(super) connection_id: String,
    pub(super) session_id: String,
    pub(super) session_name: String,
    pub(super) target_label: String,
    pub(in crate::workspace) value: String,
    pub(in crate::workspace) focused: bool,
    pub(super) kind: HostTmuxInputDialogKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ConnectionRuntimeSection {
    Overview,
    Health,
    Topology,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CompactMonitorLayout {
    Inline,
    Stacked,
}

/// Keeps a standard Host Tools confirmation payload alive while its exit motion runs.
pub(super) struct HostToolConfirmState<T> {
    pub(super) request: T,
    pub(super) presence: ExitPresence,
}

pub(super) struct HostGpuViewState {
    pub(super) update_tx: tokio::sync::mpsc::UnboundedSender<GpuUpdate>,
    pub(super) sampling_task: Option<GpuSamplingTask>,
    pub(super) snapshot_connection_id: Option<String>,
    pub(super) snapshot: Option<GpuSnapshot>,
    pub(super) expanded_uuid: Option<String>,
    pub(super) list_state: ListState,
    pub(super) list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostGpuViewState {
    pub(super) fn new(update_tx: tokio::sync::mpsc::UnboundedSender<GpuUpdate>) -> Self {
        Self {
            update_tx,
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
    pub(super) fn new(request: T) -> Self {
        Self {
            request,
            presence: ExitPresence::visible(),
        }
    }

    /// Reuses the generation so a stale timer cannot close a replacement request.
    pub(super) fn open(slot: &mut Option<Self>, request: T) {
        if let Some(state) = slot.as_mut() {
            state.request = request;
            state.presence.reopen();
        } else {
            *slot = Some(Self::new(request));
        }
    }
}

pub(in crate::workspace) struct ConnectionMonitorState {
    // Reliable Host Tools action results remain view-owned until their pages
    // move into HostToolsEntity in the following Phase 3A slices.
    pub(super) delivery_wake: crate::workspace::delivery::ActiveDeliveryWake,
    pub(super) delivery_cursor: usize,
    pub(in crate::workspace) host_process_search_query: String,
    pub(in crate::workspace) host_process_search_focused: bool,
    pub(super) host_process_filter: ProcessFilter,
    pub(super) host_process_sort: ProcessSort,
    pub(super) host_process_sort_descending: bool,
    pub(in crate::workspace) host_process_expanded_pid: Option<String>,
    pub(super) host_process_list_state: ListState,
    pub(super) host_process_list_cache: RefCell<VirtualListSignatureCache>,
    pub(in crate::workspace) host_process_renice_value: String,
    pub(in crate::workspace) host_process_renice_focused: bool,
    pub(in crate::workspace) host_docker_search_query: String,
    pub(in crate::workspace) host_docker_search_focused: bool,
    pub(in crate::workspace) host_docker_expanded_id: Option<String>,
    pub(super) host_docker_list_state: ListState,
    pub(super) host_docker_list_cache: RefCell<VirtualListSignatureCache>,
    pub(in crate::workspace) host_service_search_query: String,
    pub(in crate::workspace) host_service_search_focused: bool,
    pub(in crate::workspace) host_service_expanded_id: Option<String>,
    pub(super) host_service_list_state: ListState,
    pub(super) host_service_list_cache: RefCell<VirtualListSignatureCache>,
    pub(in crate::workspace) host_log_search_query: String,
    pub(in crate::workspace) host_log_search_focused: bool,
    pub(in crate::workspace) host_tmux_search_query: String,
    pub(in crate::workspace) host_tmux_search_focused: bool,
    pub(in crate::workspace) host_tmux_expanded_session_id: Option<String>,
    pub(in crate::workspace) host_tmux_expanded_window_id: Option<String>,
    pub(super) host_tmux_snapshot_connection_id: Option<String>,
    pub(super) host_tmux_snapshot: Option<ResourceTmuxSnapshot>,
    pub(super) host_tmux_snapshot_rx: Option<std::sync::mpsc::Receiver<HostTmuxSnapshotDelivery>>,
    pub(super) host_tmux_snapshot_running: Option<HostTmuxSnapshotRequest>,
    pub(super) host_tmux_snapshot_polling: bool,
    pub(super) host_tmux_last_error: Option<String>,
    pub(super) host_tmux_pending_confirm: Option<HostToolConfirmState<HostTmuxActionRequest>>,
    pub(in crate::workspace) host_tmux_input_dialog: Option<HostTmuxInputDialog>,
    pub(super) host_tmux_action_running: Option<HostTmuxActionRequest>,
    pub(super) host_tmux_action_rx: Option<std::sync::mpsc::Receiver<HostTmuxActionDelivery>>,
    pub(super) host_tmux_action_polling: bool,
    pub(super) host_tmux_list_state: ListState,
    pub(super) host_tmux_list_cache: RefCell<VirtualListSignatureCache>,
    pub(in crate::workspace) host_port_search_query: String,
    pub(in crate::workspace) host_port_search_focused: bool,
    pub(in crate::workspace) host_schedule_search_query: String,
    pub(in crate::workspace) host_schedule_search_focused: bool,
    pub(in crate::workspace) host_filesystem_search_query: String,
    pub(in crate::workspace) host_filesystem_search_focused: bool,
    pub(in crate::workspace) host_package_search_query: String,
    pub(in crate::workspace) host_package_search_focused: bool,
}

impl ConnectionMonitorState {
    pub(in crate::workspace) fn new() -> Self {
        let delivery_wake = crate::workspace::delivery::ActiveDeliveryWake::default();
        Self {
            delivery_wake,
            delivery_cursor: 0,
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
            host_docker_search_query: String::new(),
            host_docker_search_focused: false,
            host_docker_expanded_id: None,
            host_docker_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_docker_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_service_search_query: String::new(),
            host_service_search_focused: false,
            host_service_expanded_id: None,
            host_service_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_service_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_log_search_query: String::new(),
            host_log_search_focused: false,
            host_tmux_search_query: String::new(),
            host_tmux_search_focused: false,
            host_tmux_expanded_session_id: None,
            host_tmux_expanded_window_id: None,
            host_tmux_snapshot_connection_id: None,
            host_tmux_snapshot: None,
            host_tmux_snapshot_rx: None,
            host_tmux_snapshot_running: None,
            host_tmux_snapshot_polling: false,
            host_tmux_last_error: None,
            host_tmux_pending_confirm: None,
            host_tmux_input_dialog: None,
            host_tmux_action_running: None,
            host_tmux_action_rx: None,
            host_tmux_action_polling: false,
            host_tmux_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_tmux_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_port_search_query: String::new(),
            host_port_search_focused: false,
            host_schedule_search_query: String::new(),
            host_schedule_search_focused: false,
            host_filesystem_search_query: String::new(),
            host_filesystem_search_focused: false,
            host_package_search_query: String::new(),
            host_package_search_focused: false,
        }
    }
}
