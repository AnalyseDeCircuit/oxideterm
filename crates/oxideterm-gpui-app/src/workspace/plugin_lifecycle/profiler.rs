// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use super::{WorkspaceApp, constants::NATIVE_PLUGIN_PROFILER_METRICS_INTERVAL};
pub(super) use oxideterm_plugin_host_api::profiler::{
    native_plugin_profiler_changed_metric_entries, native_plugin_profiler_response,
    native_plugin_profiler_snapshot_array, native_plugin_profiler_timestamp_map,
    native_plugin_subscription_allows_node,
};

pub(super) fn native_plugin_profiler_node_connection_ids(
    workspace: &WorkspaceApp,
) -> HashMap<String, String> {
    workspace
        .node_runtime_store
        .export_snapshot()
        .nodes
        .into_iter()
        .filter_map(|node| {
            node.connection_id
                .map(|connection_id| (node.id.0, connection_id))
        })
        .collect()
}

pub(super) fn native_plugin_profiler_metrics_due(workspace: &WorkspaceApp) -> bool {
    workspace
        .native_plugin_profiler_last_emitted
        .map(|last_emitted| last_emitted.elapsed() >= NATIVE_PLUGIN_PROFILER_METRICS_INTERVAL)
        .unwrap_or(true)
}
