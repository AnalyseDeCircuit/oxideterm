// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Backend model for the Tauri-compatible connection monitor.
//!
//! This crate owns the UI-consumed monitor shapes and profiler state contract.
//! SSH registries feed it snapshots; GPUI surfaces render it.

mod metrics;
mod profiler;
mod stats;

pub use metrics::{
    CpuSnapshot, MetricsSource, NetSnapshot, PreviousResourceSample, RESOURCE_HISTORY_CAPACITY,
    ResourceMetrics, parse_cpu_snapshot, parse_loadavg, parse_meminfo, parse_net_snapshot,
    parse_nproc, parse_resource_metrics, previous_sample_from_metrics, push_history,
};
pub use profiler::{
    ConnectionProfilerSnapshot, ProfilerRegistry, ProfilerState, ProfilerUpdate,
    RESOURCE_CHANNEL_OPEN_TIMEOUT, RESOURCE_END_MARKER, RESOURCE_MAX_CONSECUTIVE_FAILURES,
    RESOURCE_MAX_OUTPUT_SIZE, RESOURCE_SAMPLE_INTERVAL, RESOURCE_SAMPLE_TIMEOUT,
    ResourceSampleShell, ResourceSampler, ResourceSamplerFuture, build_sample_command,
    shell_init_command,
};
pub use stats::{
    ConnectionMonitorConsumerKind, ConnectionPoolEntryState, ConnectionPoolEntrySummary,
    ConnectionPoolMonitorStats, PoolConnectionMonitorSnapshot, PoolConnectionSummarySnapshot,
};
