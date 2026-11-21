use std::time::Duration;

use gpui::{Rgba, rgb, rgba};
use oxideterm_gpui_ui::motion::ExitPresence;
use oxideterm_ssh::SshCommandOutput;
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
pub(crate) const COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT: f32 = 38.0;
pub(crate) const COMPACT_MONITOR_LIST_OVERSCAN: usize = 8;
pub(crate) const COMPACT_MONITOR_USAGE_ROW_HEIGHT: f32 = 44.0;
pub(crate) const COMPACT_MONITOR_CORE_PAIR_ROW_HEIGHT: f32 = 24.0;
pub(crate) const COMPACT_MONITOR_SPARKLINE_ROW_HEIGHT: f32 = 36.0;
pub(crate) const COMPACT_MONITOR_METRIC_ROW_HEIGHT: f32 = 28.0;
pub(crate) const COMPACT_MONITOR_SECTION_ROW_HEIGHT: f32 = 30.0;
pub(crate) const COMPACT_MONITOR_NETWORK_SUMMARY_ROW_HEIGHT: f32 = 32.0;
pub(crate) const COMPACT_MONITOR_STACKED_ROW_HEIGHT: f32 = 46.0;
pub(crate) const COMPACT_MONITOR_INTERFACE_ROW_HEIGHT: f32 = 28.0;
pub(crate) const COMPACT_MONITOR_PROCESS_ROW_HEIGHT: f32 = 26.0;
pub(crate) const COMPACT_MONITOR_RETRY_ROW_HEIGHT: f32 = 40.0;
pub(crate) const COMPACT_MONITOR_ROW_SIDE_PADDING: f32 = 12.0;
// Only fall back to two-line bandwidth rows when the sidebar is very narrow.
pub(crate) const COMPACT_MONITOR_STACKED_LAYOUT_MAX_WIDTH: f32 = 240.0;
pub const MONITOR_BORDER_ALPHA: u32 = 0x80;
pub(crate) const MONITOR_TINT_ALPHA: u32 = 0x1a;
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
pub(crate) const TOPOLOGY_CONNECTED: u32 = 0x22c55e;
pub(crate) const TOPOLOGY_CONNECTING: u32 = 0xeab308;
pub const TOPOLOGY_FAILED: u32 = 0xef4444;
pub(crate) const TOPOLOGY_DISCONNECTED: u32 = 0x71717a;
pub(crate) const TOPOLOGY_PENDING: u32 = 0xf59e0b;
pub(crate) const TOPOLOGY_ZOOM_INITIAL: f32 = 0.9;
pub(crate) const TOPOLOGY_ZOOM_MIN: f32 = 0.3;
pub(crate) const TOPOLOGY_ZOOM_MAX: f32 = 3.0;
pub(crate) const TOPOLOGY_PAN_INITIAL_X: f32 = 0.0;
pub(crate) const TOPOLOGY_PAN_INITIAL_Y: f32 = 50.0;
pub const TOPOLOGY_MENU_WIDTH: f32 = 180.0;
pub(crate) const TOPOLOGY_MENU_MAX_HEIGHT: f32 = 250.0;

pub fn connection_monitor_surface_bg(theme_bg: u32, has_background: bool) -> Rgba {
    if has_background {
        rgba(0x00000000)
    } else {
        rgb(theme_bg)
    }
}

#[derive(Clone, Copy)]
pub struct TopologyTransform {
    pub(crate) x: f32,
    pub(crate) y: f32,
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
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
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
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    // The local target has no endpoint, so it carries its localized label.
    pub(crate) label: Option<String>,
}

impl MonitorConnectionOption {
    pub(super) fn from_connection_info(connection: oxideterm_ssh::ConnectionInfo) -> Self {
        Self {
            connection_id: connection.connection_id,
            host: connection.host,
            port: connection.port,
            username: connection.username,
            label: None,
        }
    }

    pub(super) fn from_pool_summary(summary: &ConnectionPoolEntrySummary) -> Self {
        Self {
            connection_id: summary.id.clone(),
            host: summary.host.clone(),
            port: summary.port,
            username: summary.username.clone(),
            label: None,
        }
    }

    pub(super) fn local(label: String) -> Self {
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
pub(crate) struct HostProcessActionRequest {
    pub(crate) connection_id: String,
    pub(crate) pid: String,
    // Process names can contain credential-like command fragments. All UI
    // clones share one buffer that is cleared when the confirmation closes.
    pub(crate) display_command: Arc<zeroize::Zeroizing<String>>,
    pub(crate) action: ProcessActionKind,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostProcessActionRun {
    pub(crate) connection_id: String,
    pub(crate) pid: String,
    pub(crate) action: ProcessActionKind,
}

pub(crate) struct HostProcessActionDelivery {
    pub(crate) request: HostProcessActionRun,
    pub(crate) result: Result<bool, ()>,
}

pub(crate) struct HostProcessActionsState {
    pub(crate) pending_confirm: Option<HostToolConfirmState<HostProcessActionRequest>>,
    pub(crate) running: Option<HostProcessActionRun>,
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
pub(crate) struct HostDockerActionRequest {
    pub(crate) connection_id: String,
    pub(crate) container_id: String,
    pub(crate) container_name: String,
    pub(crate) action: DockerActionKind,
}

pub(crate) struct HostDockerActionDelivery {
    pub(crate) request: HostDockerActionRequest,
    pub(crate) result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostDockerLogsRequest {
    pub(crate) connection_id: String,
    pub(crate) container_id: String,
    pub(crate) container_name: String,
    pub(crate) failure_fallback: String,
    pub(crate) empty_fallback: String,
}

pub(crate) struct HostDockerLogsDelivery {
    pub(crate) request: HostDockerLogsRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostDockerLogsDialog {
    pub(crate) request: HostDockerLogsRequest,
    // Docker output stays in one shared zeroizing buffer while rendered.
    pub(crate) output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub(crate) error: Option<String>,
    pub(crate) loading: bool,
}

pub(crate) struct HostDockerOperationsState {
    pub(crate) pending_confirm: Option<HostToolConfirmState<HostDockerActionRequest>>,
    pub(crate) action_running: Option<HostDockerActionRequest>,
    pub(crate) logs_dialog: Option<HostDockerLogsDialog>,
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
pub(crate) struct HostServiceSnapshotRequest {
    pub(crate) connection_id: String,
    pub(crate) connection_fallback: String,
    pub(crate) failure_fallback: String,
}

pub(crate) struct HostServiceSnapshotDelivery {
    pub(crate) request: HostServiceSnapshotRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

pub(crate) struct HostServiceSnapshotPending {
    pub(crate) request: HostServiceSnapshotRequest,
    pub(crate) runtime: tokio::runtime::Handle,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostServiceActionRequest {
    pub(crate) connection_id: String,
    pub(crate) service_id: String,
    pub(crate) description: String,
    pub(crate) action: ServiceActionKind,
}

pub(crate) struct HostServiceActionDelivery {
    pub(crate) request: HostServiceActionRequest,
    pub(crate) result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostServiceLogsRequest {
    pub(crate) connection_id: String,
    pub(crate) service_id: String,
    pub(crate) description: String,
    pub(crate) failure_fallback: String,
    pub(crate) empty_fallback: String,
}

pub(crate) struct HostServiceLogsDelivery {
    pub(crate) request: HostServiceLogsRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostServiceLogsDialog {
    pub(crate) request: HostServiceLogsRequest,
    // Service output stays in one shared zeroizing buffer while rendered.
    pub(crate) output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub(crate) error: Option<String>,
    pub(crate) loading: bool,
}

pub(crate) struct HostServicesState {
    pub(crate) snapshot_connection_id: Option<String>,
    pub(crate) snapshot: Option<oxideterm_connection_monitor::ResourceServiceSnapshot>,
    pub(crate) snapshot_running: Option<HostServiceSnapshotRequest>,
    pub(crate) snapshot_pending: Option<HostServiceSnapshotPending>,
    pub(crate) snapshot_in_flight: bool,
    pub(crate) pending_confirm: Option<HostToolConfirmState<HostServiceActionRequest>>,
    pub(crate) action_running: Option<HostServiceActionRequest>,
    pub(crate) logs_dialog: Option<HostServiceLogsDialog>,
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
    pub(super) fn should_toast(self) -> bool {
        matches!(self, Self::Toast)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostLogSnapshotRequest {
    pub(crate) connection_id: String,
    pub(crate) preset: LogPreset,
    pub(crate) limit: usize,
    pub(crate) feedback: HostSnapshotFeedback,
    pub(crate) failure_fallback: String,
}

pub(crate) struct HostLogSnapshotDelivery {
    pub(crate) request: HostLogSnapshotRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

pub(crate) fn zeroize_host_snapshot_output(output: &mut SshCommandOutput) {
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
        let mut output = SshCommandOutput {
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

pub(crate) struct HostLogsState {
    pub(crate) expanded_index: Option<usize>,
    pub(crate) preset: LogPreset,
    pub(crate) snapshot_connection_id: Option<String>,
    pub(crate) snapshot: Option<ResourceLogSnapshot>,
    pub(crate) running: Option<HostLogSnapshotRequest>,
    pub(crate) snapshot_in_flight: bool,
    pub(crate) list_state: ListState,
    pub(crate) list_cache: RefCell<VirtualListSignatureCache>,
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
pub(crate) struct HostTmuxSnapshotRequest {
    pub(crate) connection_id: String,
    pub(crate) feedback: HostSnapshotFeedback,
    pub(crate) search_query: String,
    pub(crate) failure_fallback: String,
    pub(crate) unavailable_fallback: String,
}

pub(crate) struct HostTmuxSnapshotDelivery {
    pub(crate) request: HostTmuxSnapshotRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum HostTmuxDestructiveAction {
    KillSession { target: String },
    KillWindow { target: String },
    KillPane { target: String },
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostTmuxActionRequest {
    pub(crate) connection_id: String,
    pub(crate) session_id: String,
    pub session_name: String,
    pub target_label: String,
    // Confirm state accepts only destructive actions, so secret-bearing rename
    // and send-command values can never enter its cloneable request type.
    pub(crate) action: HostTmuxDestructiveAction,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostTmuxActionRun {
    pub(crate) connection_id: String,
    pub(crate) session_id: String,
    pub session_name: String,
    pub target_label: String,
}

pub(crate) struct HostTmuxActionDelivery {
    pub(crate) request: HostTmuxActionRun,
    pub(crate) result: Result<bool, ()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostPortSnapshotRequest {
    pub(crate) connection_id: String,
    pub(crate) feedback: HostSnapshotFeedback,
    pub(crate) failure_fallback: String,
}

pub(crate) struct HostPortSnapshotDelivery {
    pub(crate) request: HostPortSnapshotRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

pub(crate) struct HostPortsState {
    pub(crate) filter: PortFilter,
    pub(crate) expanded_index: Option<usize>,
    pub(crate) snapshot_connection_id: Option<String>,
    pub(crate) snapshot: Option<ResourcePortSnapshot>,
    pub(crate) running: Option<HostPortSnapshotRequest>,
    pub(crate) snapshot_in_flight: bool,
    pub(crate) list_state: ListState,
    pub(crate) list_cache: RefCell<VirtualListSignatureCache>,
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
pub(crate) struct HostScheduleSnapshotRequest {
    pub(crate) connection_id: String,
    pub(crate) feedback: HostSnapshotFeedback,
    pub(crate) failure_fallback: String,
}

pub(crate) struct HostScheduleSnapshotDelivery {
    pub(crate) request: HostScheduleSnapshotRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

pub(crate) struct HostSchedulesState {
    pub(crate) filter: ScheduledTaskFilter,
    pub(crate) expanded_index: Option<usize>,
    pub(crate) snapshot_connection_id: Option<String>,
    pub(crate) snapshot: Option<ResourceScheduledTaskSnapshot>,
    pub(crate) running: Option<HostScheduleSnapshotRequest>,
    pub(crate) snapshot_in_flight: bool,
    pub(crate) list_state: ListState,
    pub(crate) list_cache: RefCell<VirtualListSignatureCache>,
    pub(crate) pending_confirm: Option<HostToolConfirmState<HostScheduleActionRequest>>,
    pub(crate) action_running: Option<HostScheduleActionRequest>,
    pub(crate) logs_dialog: Option<HostScheduleLogsDialog>,
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

pub(crate) struct HostFilesystemSnapshotDelivery {
    pub(crate) request: FilesystemSnapshotRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostPackageSnapshotRequest {
    pub(crate) connection_id: String,
    pub(crate) feedback: HostSnapshotFeedback,
    pub(crate) failure_fallback: String,
}

pub(crate) struct HostPackageSnapshotDelivery {
    pub(crate) request: HostPackageSnapshotRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

/// Packages uninstall through a confirmation because the terminal command
/// removes installed software as soon as the shell receives it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostPackageUninstallRequest {
    pub(crate) connection_id: String,
    pub(crate) manager: String,
    pub(crate) package_id: String,
    pub(crate) package_name: String,
}

pub(crate) struct HostPackagesState {
    pub(crate) filter: PackageFilter,
    pub(crate) expanded_index: Option<usize>,
    pub(crate) pending_confirm: Option<HostToolConfirmState<HostPackageUninstallRequest>>,
    pub(crate) snapshot_connection_id: Option<String>,
    pub(crate) snapshot: Option<ResourcePackageSnapshot>,
    pub(crate) running: Option<HostPackageSnapshotRequest>,
    pub(crate) snapshot_in_flight: bool,
    pub(crate) list_state: ListState,
    pub(crate) list_cache: RefCell<VirtualListSignatureCache>,
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
pub(crate) struct HostScheduleActionRequest {
    pub(crate) connection_id: String,
    pub(crate) task_id: String,
    pub(crate) task_name: String,
    pub(crate) unit: String,
    pub(crate) action: ScheduledTaskActionKind,
}

pub(crate) struct HostScheduleActionDelivery {
    pub(crate) request: HostScheduleActionRequest,
    pub(crate) result: Result<bool, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostScheduleLogsRequest {
    pub(crate) connection_id: String,
    // Keep sampled commands out of asynchronous deliveries; logs need only
    // the public task identity used by the dialog and follow action.
    pub(crate) task_id: String,
    pub(crate) task_name: String,
    pub(crate) task_source: String,
    pub(crate) task_unit: String,
    pub(crate) failure_fallback: String,
    pub(crate) empty_fallback: String,
}

pub(crate) struct HostScheduleLogsDelivery {
    pub(crate) request: HostScheduleLogsRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HostScheduleLogsDialog {
    pub(crate) request: HostScheduleLogsRequest,
    // The Entity and render tree share one zeroizing capture buffer.
    pub(crate) output: Option<Arc<zeroize::Zeroizing<String>>>,
    pub(crate) error: Option<String>,
    pub(crate) loading: bool,
}

#[derive(Clone, Eq, PartialEq)]
pub enum HostTmuxInputDialogKind {
    RenameSession { target: String },
    RenameWindow { target: String },
    SendPaneCommand { target: String },
}

pub struct HostTmuxInputDialog {
    pub(crate) connection_id: String,
    pub(crate) session_id: String,
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
}

pub(crate) struct HostTmuxState {
    pub(crate) mux: super::multiplexer::MuxPanel,
    pub(crate) snapshot_connection_id: Option<String>,
    pub(crate) snapshot: Option<ResourceTmuxSnapshot>,
    pub(crate) snapshot_running: Option<HostTmuxSnapshotRequest>,
    pub(crate) snapshot_in_flight: bool,
    pub(crate) last_error: Option<String>,
    pub(crate) pending_confirm: Option<HostToolConfirmState<HostTmuxActionRequest>>,
    pub(crate) action_running: Option<HostTmuxActionRun>,
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
pub(crate) struct MuxSnapshotRequest {
    pub(crate) provider: Multiplexer,
    pub(crate) connection_id: String,
    pub(crate) feedback: HostSnapshotFeedback,
    pub(crate) generation: u64,
}

impl MuxSnapshotRequest {
    /// The identity repeat clicks and automatic refreshes coalesce on,
    /// deliberately ignoring the generation that makes each capture unique.
    pub(super) fn same_target(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.connection_id == other.connection_id
            && self.feedback == other.feedback
    }
}

pub(crate) struct MuxSnapshotDelivery {
    pub(crate) request: MuxSnapshotRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

/// One capture of the subtree beneath one session; same generation rule as
/// [`MuxSnapshotRequest`].
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct MuxDetailRequest {
    pub(crate) provider: Multiplexer,
    pub(crate) connection_id: String,
    pub(crate) session: String,
    pub(crate) generation: u64,
}

impl MuxDetailRequest {
    pub(super) fn same_target(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.connection_id == other.connection_id
            && self.session == other.session
    }
}

pub(crate) struct MuxDetailDelivery {
    pub(crate) request: MuxDetailRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

/// The verb the user confirmed against one row. `node` is `None` for a
/// session-level verb; the frozen model builds every object command from the
/// node itself, so the request carries it rather than an id it could not
/// turn back into a row.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct MuxActionRequest {
    pub(crate) provider: Multiplexer,
    pub(crate) connection_id: String,
    /// Session that owns the workspace, tab or pane being mutated.
    pub(crate) session: String,
    pub target_label: String,
    pub(crate) node: Option<MuxNode>,
    pub(crate) verb: MuxVerb,
}

/// The one mutating slot a confirmed action reserves. It carries the whole
/// request identity because a target label alone is not unique: after a
/// provider switch an older action can finish with the same label as a newer
/// one and would otherwise clear that action's slot and report its result.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct MuxActionRun {
    pub(crate) provider: Multiplexer,
    pub(crate) connection_id: String,
    pub(crate) session: String,
    pub target_label: String,
    pub(crate) generation: u64,
}

pub(crate) struct MuxActionDelivery {
    pub(crate) request: MuxActionRun,
    // Action output is reduced to a success bit before it crosses delivery.
    pub(crate) result: Result<bool, ()>,
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
pub(crate) struct MuxReadRequest {
    pub(crate) provider: Multiplexer,
    pub(crate) connection_id: String,
    pub(crate) session: String,
    pub(crate) target_id: String,
    pub target_label: String,
    /// Reopening the same pane starts a new capture, so an older answer cannot
    /// overwrite the buffer the user is now reading.
    pub(crate) generation: u64,
}

pub(crate) struct MuxReadDelivery {
    pub(crate) request: MuxReadRequest,
    pub(crate) result: Result<SshCommandOutput, ()>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct MuxReadDialog {
    pub(crate) request: MuxReadRequest,
    // The Entity and render tree share one zeroizing capture buffer.
    pub(crate) output: Option<Arc<zeroize::Zeroizing<String>>>,
    /// A failed capture renders the localized fallback; remote output never
    /// becomes the error text.
    pub(crate) failed: bool,
    pub(crate) loading: bool,
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
    pub(crate) service_connection_missing: String,
    pub(crate) service_action_failed: String,
    pub(crate) log_unknown_error: String,
    pub(crate) port_unknown_error: String,
    pub(crate) filesystem_unknown_error: String,
    pub(crate) package_unknown_error: String,
    pub(crate) schedule_unknown_error: String,
    pub(crate) tmux_unknown_error: String,
    pub(crate) tmux_unavailable: String,
    pub(crate) local_connection_label: String,
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
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompactMonitorLayout {
    Inline,
    Stacked,
}

/// Keeps a standard Host Tools confirmation payload alive while its exit motion runs.
pub(crate) struct HostToolConfirmState<T> {
    pub(crate) request: T,
    pub(crate) presence: ExitPresence,
}

pub(crate) struct HostGpuViewState {
    pub(crate) update_tx: tokio::sync::mpsc::UnboundedSender<GpuUpdate>,
    pub(crate) sampling_task: Option<GpuSamplingTask>,
    pub(crate) snapshot_connection_id: Option<String>,
    pub(crate) snapshot: Option<GpuSnapshot>,
    pub(crate) expanded_uuid: Option<String>,
    pub(crate) list_state: ListState,
    pub(crate) list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostGpuViewState {
    pub fn new(update_tx: tokio::sync::mpsc::UnboundedSender<GpuUpdate>) -> Self {
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
    pub fn new(request: T) -> Self {
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

/// Owns Host Tools input, selection, expansion, and virtual-list presentation state.
pub struct HostToolsUiState {
    pub focused_input: Option<HostToolsTextInput>,
    pub host_process_search_query: String,
    pub(crate) host_process_filter: ProcessFilter,
    pub(crate) host_process_sort: ProcessSort,
    pub(crate) host_process_sort_descending: bool,
    pub host_process_expanded_pid: Option<String>,
    pub(crate) host_process_list_state: ListState,
    pub(crate) host_process_list_cache: RefCell<VirtualListSignatureCache>,
    pub host_process_renice_value: String,
    pub host_docker_search_query: String,
    pub host_docker_expanded_id: Option<String>,
    pub(crate) host_docker_list_state: ListState,
    pub(crate) host_docker_list_cache: RefCell<VirtualListSignatureCache>,
    pub host_service_search_query: String,
    pub host_service_expanded_id: Option<String>,
    pub(crate) host_service_list_state: ListState,
    pub(crate) host_service_list_cache: RefCell<VirtualListSignatureCache>,
    pub host_log_search_query: String,
    pub host_tmux_search_query: String,
    pub host_tmux_expanded_session_id: Option<String>,
    pub host_tmux_expanded_window_id: Option<String>,
    pub host_tmux_input_dialog: Option<HostTmuxInputDialog>,
    pub host_mux_input_dialog: Option<MuxInputDialog>,
    pub(crate) host_tmux_list_state: ListState,
    pub(crate) host_tmux_list_cache: RefCell<VirtualListSignatureCache>,
    pub host_port_search_query: String,
    pub host_schedule_search_query: String,
    pub host_package_search_query: String,
}

impl HostToolsUiState {
    pub fn new() -> Self {
        Self {
            focused_input: None,
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
            host_tmux_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8),
            ),
            host_tmux_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            host_port_search_query: String::new(),
            host_schedule_search_query: String::new(),
            host_package_search_query: String::new(),
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
        }
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
        }
    }
}
