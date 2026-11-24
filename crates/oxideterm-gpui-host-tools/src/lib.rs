//! Host Tools panels and page-owned operations. The app owns window input and the shared SSH nodes.

mod bridge;
pub use bridge::*;
mod delivery;
mod dialogs;
pub use dialogs::{HostToolDialogWindow, HostToolsDialogs};
mod entity;
mod events;
#[cfg(not(target_family = "wasm"))]
mod exec;
mod filesystem_panel;
mod filesystem_state;
mod health;
mod helpers;
mod lifecycle;
mod multiplexer;
mod topology;
mod types;
mod window_text;

use gpui::{
    AnyElement, App, ClipboardItem, Context, CursorStyle, IntoElement, ListAlignment, ListState,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Rgba, SharedString, Styled,
    Window, canvas, div, prelude::*, px, relative, rgb, rgba, svg,
};
use helpers::*;
use oxideterm_connection_monitor::container::{
    ContainerAction, ContainerCapture, ContainerDiscovery, ContainerEngine, ContainerInstance,
    ContainerRegistry, ContainerRegistrySource, ContainerRunOptions, ContainerSnapshot,
    ContainerStat, ContainerStatus, capture_command, container_action_availability,
    container_action_command, container_prune_command, create_network_command, create_pod_command,
    create_volume_command, discovery_command, exec_shell_command, follow_logs_command,
    healthcheck_command, image_action_command, logout_command, network_action_command,
    parse_discovery, parse_registries, parse_run_options, parse_snapshot, parse_stats,
    pod_action_availability, pod_action_command, prune_images_command, registries_command,
    rename_container_command, snapshot_command, stats_command, tag_image_command, terminal_command,
    volume_action_command,
};
use oxideterm_connection_monitor::{
    ConnectionPoolEntrySummary, ConnectionPoolMonitorStats, DockerActionKind,
    FilesystemCommandCapability, GpuDevice, GpuProvider, GpuSnapshot, GpuSnapshotStatus, GpuUpdate,
    LogCommandCapability, LogPreset, MetricsSource, MonitorValueLevel, PackageCommandCapability,
    PackageFilter, PortCommandCapability, PortFilter, ProcessActionKind, ProcessCommandCapability,
    ProcessFilter, ProcessSort, REDIS_KEY_EXISTS_SENTINEL, RedisCreateKind, RedisKeyMutation,
    RedisValueWriteKind, ResourceDockerContainer, ResourceDockerStatus, ResourceFilesystemSnapshot,
    ResourceFilesystemStatus, ResourceLogEntry, ResourceLogSnapshot, ResourceLogStatus,
    ResourceMetrics, ResourcePackageEntry, ResourcePackageSnapshot, ResourcePackageStatus,
    ResourcePortEntry, ResourcePortSnapshot, ResourcePortStatus, ResourceRedisDatabase,
    ResourceRedisEndpoint, ResourceRedisEndpointKind, ResourceRedisKeyNode, ResourceRedisKeys,
    ResourceRedisSnapshot, ResourceRedisStatus, ResourceRedisTtl, ResourceRedisValue,
    ResourceRedisValueKind, ResourceScheduledTask, ResourceScheduledTaskSnapshot,
    ResourceScheduledTaskStatus, ResourceService, ResourceServiceStatus, ResourceTmuxPane,
    ResourceTmuxSession, ResourceTmuxSnapshot, ResourceTmuxStatus, ResourceTmuxWindow,
    ResourceTopProcess, ScheduledTaskActionKind, ScheduledTaskCapability, ScheduledTaskFilter,
    ServiceActionKind, ServiceCommandCapability, TmuxActionKind, TmuxCommandCapability,
    build_docker_action_command, build_docker_exec_shell_command, build_docker_follow_logs_command,
    build_docker_logs_command, build_filesystem_diagnostic_command,
    build_filesystem_snapshot_command, build_log_follow_command, build_log_snapshot_command,
    build_package_inspect_command, build_package_snapshot_command, build_package_uninstall_command,
    build_package_upgrade_command, build_port_diagnostic_command, build_port_snapshot_command,
    build_process_action_command, build_scheduled_task_action_command,
    build_scheduled_task_diagnostic_command, build_scheduled_task_logs_command,
    build_scheduled_task_snapshot_command, build_service_action_command,
    build_service_follow_logs_command, build_service_logs_command, build_tmux_action_command,
    build_tmux_attach_command, build_tmux_new_session_command, build_tmux_rename_session_command,
    build_tmux_rename_window_command, build_tmux_send_pane_command, build_tmux_snapshot_command,
    docker_action_succeeded, docker_row_signature, docker_state_label_key, format_bytes,
    gpu_device_row_signature, log_level_label_key, log_preset_label_key, log_row_signature,
    package_filter_label_key, package_filter_manager, package_filter_options,
    package_row_signature, package_status_label_key, parse_log_snapshot, parse_package_snapshot,
    parse_port_snapshot, parse_redis_key_mutation, parse_redis_keys, parse_redis_snapshot,
    parse_redis_value, percent_level, port_endpoint, port_filter_label_key, port_is_risky_exposure,
    port_row_signature, port_state_label_key, process_display_command, process_display_name,
    process_row_signature, process_state_label_key, redis_create_key_command,
    redis_discovery_command, redis_key_mutation_command, redis_key_tree, redis_keys_command,
    redis_keys_delete_command, redis_replace_value_command, redis_stdin_payload,
    redis_stdin_payload_with_body, redis_value_command, redis_value_json, redis_value_rows,
    redis_value_text, scheduled_task_active_label_key, scheduled_task_enabled_label_key,
    scheduled_task_filter_label_key, scheduled_task_row_signature, scheduled_task_source_label_key,
    service_action_succeeded, service_enabled_label_key, service_row_signature,
    service_state_label_key, tmux_session_row_signature, visible_docker_rows, visible_log_rows,
    visible_package_rows, visible_port_rows, visible_process_rows, visible_scheduled_task_rows,
    visible_service_rows, visible_tmux_session_rows,
};
#[cfg(not(target_family = "wasm"))]
use oxideterm_connection_monitor::{GpuSamplingTask, ProfilerRegistry, start_gpu_sampling_on};
use oxideterm_gpui_ui::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView,
    scroll::ScrollableElement,
    select::{BrowserFocusOrigin, browser_focus_visible},
    virtual_list::{
        TauriVirtualListSpec, VirtualListSignatureCache, expandable_row_signature,
        sync_tauri_variable_list_state_by_signatures, tauri_virtual_list, tauri_virtual_list_state,
    },
};
use oxideterm_i18n::I18n;
#[cfg(not(target_family = "wasm"))]
use oxideterm_ssh::{ManagedKeyResolver, NodeRouter, SshConnectionRegistry, SshPromptHandler};
use oxideterm_theme::{AppUiColors, ThemeTokens};
use oxideterm_topology::NodeId;
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
use std::{cell::RefCell, collections::HashSet, sync::Arc, time::Duration};
#[cfg(target_family = "wasm")]
use web_time::Instant;
use window_text::render_lucide_icon;

pub use entity::HostToolsEntity;
pub use events::{
    HostToolsEvent, HostToolsNotice, HostToolsWindowIntent, HostToolsWindowRequest,
    ScheduleActionNoticeKind,
};
#[cfg(not(target_family = "wasm"))]
pub use exec::{
    HostCommandTarget, LOCAL_HOST_CONNECTION_ID, LocalResourceSampler, local_host_os_type,
};
pub use filesystem_panel::FilesystemSearchFrame;
use filesystem_panel::{FilesystemAction, FilesystemPanelView, FilesystemStatusIcon};
use filesystem_state::{FilesystemPanel, FilesystemSnapshotRequest};
pub use health::container::*;
pub use health::monitor::MonitorRenderContext;
pub use health::redis::HostRedisValueBody;
pub use health::redis::*;
pub use health::tmux::{host_tmux_input_placeholder_key, host_tmux_input_submit_key};
pub use helpers::{
    host_tools_center_state, host_tools_tooltip_icon_button, topology_transform_x,
    topology_transform_y, topology_view_status_color,
};
pub use multiplexer::{mux_input_placeholder_key, mux_input_title_key};
pub use types::*;
pub use window_text::{HostToolsPlainTextImeFrame, HostToolsText, HostToolsTextRole};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextSidebarTool {
    Monitor,
    Gpu,
    Processes,
    Services,
    Logs,
    Tmux,
    Docker,
    Container,
    Ports,
    Schedules,
    Filesystems,
    Packages,
    Redis,
}
