// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Profiler host API responses and event-diff helpers.

use std::collections::HashMap;

use oxideterm_connection_monitor::{ProfilerRegistry, ProfilerState, ResourceMetrics};
use oxideterm_plugin_protocol as plugin_runtime;
use serde_json::{Value, json};

pub fn native_plugin_profiler_response(
    call: plugin_runtime::PluginHostCall,
    registry: &ProfilerRegistry,
    node_connection_ids: &HashMap<String, String>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        "getMetrics" => match native_plugin_profiler_connection_id(&call.args, node_connection_ids)
        {
            Ok(Some(connection_id)) => plugin_runtime::PluginResponse::ok(
                request_id,
                registry
                    .latest(&connection_id)
                    .map(|metrics| native_plugin_profiler_metrics_snapshot(&metrics))
                    .unwrap_or(Value::Null),
            ),
            Ok(None) => plugin_runtime::PluginResponse::ok(request_id, Value::Null),
            Err(error) => native_plugin_profiler_arg_error(request_id, error),
        },
        "getHistory" => match native_plugin_profiler_connection_id(&call.args, node_connection_ids)
        {
            Ok(Some(connection_id)) => {
                let history =
                    native_plugin_profiler_limited_history(registry, &connection_id, &call.args);
                plugin_runtime::PluginResponse::ok(request_id, Value::Array(history))
            }
            Ok(None) => plugin_runtime::PluginResponse::ok(request_id, json!([])),
            Err(error) => native_plugin_profiler_arg_error(request_id, error),
        },
        "isRunning" => {
            match native_plugin_profiler_connection_id(&call.args, node_connection_ids) {
                Ok(Some(connection_id)) => plugin_runtime::PluginResponse::ok(
                    request_id,
                    json!(registry.state(&connection_id) == Some(ProfilerState::Running)),
                ),
                Ok(None) => plugin_runtime::PluginResponse::ok(request_id, json!(false)),
                Err(error) => native_plugin_profiler_arg_error(request_id, error),
            }
        }
        "onMetrics" => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_profiler_subscription_bridge",
                "profiler subscriptions are registered through the runtime event bridge",
            ),
        ),
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "unknown_profiler_method",
                format!("Unknown profiler.{method} host API"),
            ),
        ),
    }
}

pub fn native_plugin_profiler_snapshot_array(
    registry: &ProfilerRegistry,
    node_connection_ids: &HashMap<String, String>,
) -> Value {
    let mut node_entries = node_connection_ids.iter().collect::<Vec<_>>();
    node_entries.sort_by(|left, right| left.0.cmp(right.0));
    Value::Array(
        node_entries
            .into_iter()
            .filter_map(|(node_id, connection_id)| {
                registry.latest(connection_id).map(|metrics| {
                    json!({
                        "nodeId": node_id,
                        "metrics": native_plugin_profiler_metrics_snapshot(&metrics),
                    })
                })
            })
            .collect(),
    )
}

pub fn native_plugin_profiler_timestamp_map(metrics: &Value) -> HashMap<String, u64> {
    metrics
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let node_id = entry.get("nodeId").and_then(Value::as_str)?;
            let timestamp = entry
                .get("metrics")
                .and_then(|metrics| metrics.get("timestampMs"))
                .and_then(Value::as_u64)?;
            Some((node_id.to_string(), timestamp))
        })
        .collect()
}

pub fn native_plugin_profiler_changed_metric_entries(
    metrics: &Value,
    previous_timestamps: &HashMap<String, u64>,
    next_timestamps: &HashMap<String, u64>,
) -> Vec<Value> {
    metrics
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            let Some(node_id) = entry.get("nodeId").and_then(Value::as_str) else {
                return false;
            };
            next_timestamps.get(node_id) != previous_timestamps.get(node_id)
        })
        .cloned()
        .collect()
}

pub fn native_plugin_subscription_allows_node(filter: Option<&Value>, node_id: &str) -> bool {
    filter
        .and_then(|filter| filter.get("nodeId"))
        .and_then(Value::as_str)
        .is_none_or(|filter_node_id| filter_node_id == node_id)
}

fn native_plugin_profiler_arg_error(
    request_id: String,
    error: String,
) -> plugin_runtime::PluginResponse {
    plugin_runtime::PluginResponse::error(
        request_id,
        plugin_runtime::PluginError::protocol("invalid_profiler_node", error),
    )
}

fn native_plugin_profiler_connection_id(
    args: &Value,
    node_connection_ids: &HashMap<String, String>,
) -> Result<Option<String>, String> {
    let node_id = native_plugin_profiler_node_id_arg(args)?;
    Ok(node_connection_ids.get(&node_id).cloned())
}

fn native_plugin_profiler_node_id_arg(args: &Value) -> Result<String, String> {
    let node_id = args
        .get("nodeId")
        .and_then(Value::as_str)
        .or_else(|| args.as_str())
        .ok_or_else(|| "profiler host calls require args.nodeId".to_string())?;
    if node_id.trim().is_empty() {
        return Err("profiler host calls require a non-empty node id".to_string());
    }
    Ok(node_id.to_string())
}

fn native_plugin_profiler_limited_history(
    registry: &ProfilerRegistry,
    connection_id: &str,
    args: &Value,
) -> Vec<Value> {
    let mut history = registry
        .history(connection_id)
        .iter()
        .map(native_plugin_profiler_metrics_snapshot)
        .collect::<Vec<_>>();
    if let Some(max_points) = args.get("maxPoints").and_then(Value::as_u64) {
        let max_points = max_points as usize;
        if max_points < history.len() {
            history.drain(0..history.len() - max_points);
        }
    }
    history
}

fn native_plugin_profiler_metrics_snapshot(metrics: &ResourceMetrics) -> Value {
    // Tauri's ProfilerMetricsSnapshot intentionally excludes backend-only disk
    // and sampler-source fields; keep the plugin contract stable here.
    json!({
        "timestampMs": metrics.timestamp_ms,
        "cpuPercent": metrics.cpu_percent,
        "memoryUsed": metrics.memory_used,
        "memoryTotal": metrics.memory_total,
        "memoryPercent": metrics.memory_percent,
        "loadAvg1": metrics.load_avg_1,
        "loadAvg5": metrics.load_avg_5,
        "loadAvg15": metrics.load_avg_15,
        "cpuCores": metrics.cpu_cores,
        "netRxBytesPerSec": metrics.net_rx_bytes_per_sec,
        "netTxBytesPerSec": metrics.net_tx_bytes_per_sec,
        "sshRttMs": metrics.ssh_rtt_ms,
    })
}
