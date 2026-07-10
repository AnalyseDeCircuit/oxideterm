// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Backend model for the Tauri-compatible connection monitor.
//!
//! This crate owns the UI-consumed monitor shapes and profiler state contract.
//! SSH registries feed it snapshots; GPUI surfaces render it.

mod action;
mod capture;
mod docker;
mod filesystem;
mod log;
mod metrics;
mod package;
mod port;
mod process;
mod profiler;
mod scheduled_task;
mod service;
mod shell;
mod stats;
mod summary;
mod tmux;

pub use action::{
    HostToolActionOutcome, host_tool_capture_failure_message, interpret_docker_action_output,
    interpret_process_action_output, interpret_scheduled_task_action_output,
    interpret_service_action_output, interpret_tmux_action_output,
};
pub use docker::{
    DockerActionAvailability, DockerActionCommand, DockerActionKind, DockerCaptureCommand,
    ResourceDockerContainer, ResourceDockerSnapshot, ResourceDockerStatus,
    build_docker_action_command, build_docker_exec_shell_command, build_docker_follow_logs_command,
    build_docker_logs_command, docker_action_availability, docker_action_failure_message,
    docker_action_succeeded, docker_action_success_message, docker_row_signature,
    docker_sample_command, docker_state_label_key, parse_docker_snapshot, visible_docker_rows,
};
pub use filesystem::{
    FilesystemCaptureCommand, FilesystemCommandCapability, FilesystemEntrySeverity,
    FilesystemFilter, ResourceFilesystemEntry, ResourceFilesystemSnapshot,
    ResourceFilesystemStatus, build_filesystem_diagnostic_command,
    build_filesystem_snapshot_command, filesystem_attention_label_keys,
    filesystem_capture_snapshot, filesystem_entry_severity, filesystem_filter_label_key,
    filesystem_kind_label_key, filesystem_percent_severity, filesystem_read_only_label_key,
    filesystem_row_signature, parse_filesystem_snapshot, visible_filesystem_rows,
};
pub use log::{
    LogCaptureCommand, LogCommandCapability, LogPreset, ResourceLogEntry, ResourceLogSnapshot,
    ResourceLogStatus, build_log_follow_command, build_log_snapshot_command, log_level_label_key,
    log_preset_label_key, log_row_signature, parse_log_snapshot, visible_log_rows,
};
pub use metrics::{
    CpuSnapshot, MemorySnapshot, MetricsSource, NetInterfaceSnapshot, NetSnapshot,
    PreviousResourceSample, RESOURCE_HISTORY_CAPACITY, ResourceCpuCore, ResourceDisk, ResourceGpu,
    ResourceMetrics, ResourceNetInterface, ResourceTopProcess, parse_cpu_snapshot,
    parse_disk_usage, parse_disks, parse_gpus, parse_loadavg, parse_meminfo, parse_memory_snapshot,
    parse_net_snapshot, parse_nproc, parse_resource_metrics, parse_top_processes,
    previous_sample_from_metrics, push_history,
};
pub use package::{
    PackageCaptureCommand, PackageCommandCapability, PackageFilter, PackageInspectCommand,
    ResourcePackageEntry, ResourcePackageManager, ResourcePackageSnapshot, ResourcePackageStatus,
    build_package_inspect_command, build_package_snapshot_command, package_filter_label_key,
    package_row_signature, package_status_label_key, parse_package_snapshot, visible_package_rows,
};
pub use port::{
    PortCaptureCommand, PortCommandCapability, PortFilter, ResourcePortEntry, ResourcePortSnapshot,
    ResourcePortStatus, build_port_diagnostic_command, build_port_snapshot_command,
    parse_port_snapshot, port_endpoint, port_filter_label_key, port_is_risky_exposure,
    port_row_signature, port_state_label_key, visible_port_rows,
};
pub use process::{
    ProcessActionCommand, ProcessActionKind, ProcessCommandCapability, ProcessFilter, ProcessSort,
    build_process_action_command, process_action_failure_message, process_action_succeeded,
    process_action_success_message, process_display_command, process_display_name,
    process_matches_filter, process_matches_query, process_row_signature, process_state_label_key,
    sort_process_rows, visible_process_rows,
};
pub use profiler::{
    ConnectionProfilerSnapshot, ProfilerRegistry, ProfilerState, ProfilerUpdate,
    RESOURCE_CHANNEL_OPEN_TIMEOUT, RESOURCE_END_MARKER, RESOURCE_MAX_CONSECUTIVE_FAILURES,
    RESOURCE_MAX_OUTPUT_SIZE, RESOURCE_SAMPLE_INTERVAL, RESOURCE_SAMPLE_TIMEOUT,
    ResourceSampleShell, ResourceSampler, ResourceSamplerFuture, build_sample_command,
    shell_init_command,
};
pub use scheduled_task::{
    ResourceScheduledTask, ResourceScheduledTaskSnapshot, ResourceScheduledTaskStatus,
    ScheduledTaskActionAvailability, ScheduledTaskActionCommand, ScheduledTaskActionKind,
    ScheduledTaskCapability, ScheduledTaskCaptureCommand, ScheduledTaskFilter,
    ScheduledTaskToggleAction, build_scheduled_task_action_command,
    build_scheduled_task_diagnostic_command, build_scheduled_task_logs_command,
    build_scheduled_task_snapshot_command, parse_scheduled_task_snapshot,
    scheduled_task_action_availability, scheduled_task_active_label_key,
    scheduled_task_capture_snapshot, scheduled_task_enabled_label_key,
    scheduled_task_filter_label_key, scheduled_task_row_signature, scheduled_task_source_label_key,
    visible_scheduled_task_rows,
};
pub use service::{
    ResourceService, ResourceServiceSnapshot, ResourceServiceStatus, ServiceActionAvailability,
    ServiceActionCommand, ServiceActionKind, ServiceCaptureCommand, ServiceCommandCapability,
    build_service_action_command, build_service_follow_logs_command, build_service_logs_command,
    parse_service_snapshot, service_action_availability, service_action_failure_message,
    service_action_succeeded, service_action_success_message, service_enabled_label_key,
    service_row_signature, service_sample_command, service_state_label_key, visible_service_rows,
};
pub use stats::{
    ConnectionMonitorConsumerKind, ConnectionPoolEntryState, ConnectionPoolEntrySummary,
    ConnectionPoolMonitorStats, PoolConnectionMonitorSnapshot, PoolConnectionSummarySnapshot,
};
pub use summary::{
    CompactMonitorRow, GpuMemorySummary, MonitorListRow, MonitorMetricKind, MonitorSectionKind,
    MonitorValueLevel, compact_monitor_row_signature, compact_monitor_rows, disk_list_rows,
    format_bytes, format_rate, gpu_detail_value, gpu_label, gpu_list_rows, gpu_memory_percent,
    gpu_memory_summary, gpu_utilization_percent, interface_list_rows, metrics_source_label_key,
    percent_level, resource_metrics_is_rtt_only, rtt_level, top_process_list_rows,
};
pub use tmux::{
    ResourceTmuxPane, ResourceTmuxSession, ResourceTmuxSnapshot, ResourceTmuxStatus,
    ResourceTmuxWindow, TmuxActionCommand, TmuxActionKind, TmuxCaptureCommand,
    TmuxCommandCapability, build_tmux_action_command, build_tmux_attach_command,
    build_tmux_new_session_command, build_tmux_snapshot_command, parse_tmux_snapshot,
    tmux_action_failure_message, tmux_action_succeeded, tmux_action_success_message,
    tmux_capture_snapshot, tmux_session_row_signature, visible_tmux_session_rows,
};
