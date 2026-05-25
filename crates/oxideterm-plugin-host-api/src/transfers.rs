// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use oxideterm_plugin_protocol as plugin_runtime;
use oxideterm_sftp::{
    BackgroundTransferDirection, BackgroundTransferSnapshot, BackgroundTransferState,
    SftpTransferManager,
};
use serde_json::{Value, json};

// Transfer polling and snapshots stay together because event emission compares
// previous and current plugin-facing transfer states, not raw manager records.
pub fn native_plugin_transfers_response(
    call: plugin_runtime::PluginHostCall,
    manager: &Arc<SftpTransferManager>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        "getAll" => plugin_runtime::PluginResponse::ok(
            request_id,
            native_plugin_transfer_snapshot_array(manager, None),
        ),
        "getByNode" => match native_plugin_transfer_node_id_arg(&call.args) {
            Ok(node_id) => plugin_runtime::PluginResponse::ok(
                request_id,
                native_plugin_transfer_snapshot_array(manager, Some(node_id.as_str())),
            ),
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_transfer_node", error),
            ),
        },
        "onProgress" | "onComplete" | "onError" => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_transfer_subscription_bridge",
                "transfer subscriptions are registered through the runtime event bridge",
            ),
        ),
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "unknown_transfer_method",
                format!("Unknown transfers.{method} host API"),
            ),
        ),
    }
}

pub fn native_plugin_transfer_snapshot_array(
    manager: &Arc<SftpTransferManager>,
    node_id: Option<&str>,
) -> Value {
    Value::Array(
        manager
            .list_background_transfers(node_id)
            .iter()
            .map(native_plugin_transfer_snapshot)
            .collect(),
    )
}

fn native_plugin_transfer_snapshot(snapshot: &BackgroundTransferSnapshot) -> Value {
    // Match Tauri's TransferSnapshot projection and intentionally omit native
    // implementation details such as transfer strategy, backend speed, and
    // retained item counts.
    json!({
        "id": &snapshot.id,
        "nodeId": &snapshot.node_id,
        "name": &snapshot.name,
        "localPath": &snapshot.local_path,
        "remotePath": &snapshot.remote_path,
        "direction": native_plugin_transfer_direction_label(snapshot.direction),
        "size": snapshot.size,
        "transferred": snapshot.transferred,
        "state": native_plugin_transfer_state_label(snapshot.state),
        "error": &snapshot.error,
        "startTime": snapshot.start_time,
        "endTime": snapshot.end_time,
    })
}

fn native_plugin_transfer_node_id_arg(args: &Value) -> Result<String, String> {
    let node_id = args
        .get("nodeId")
        .and_then(Value::as_str)
        .or_else(|| args.as_str())
        .ok_or_else(|| "transfers.getByNode requires args.nodeId".to_string())?;
    if node_id.trim().is_empty() {
        return Err("transfers.getByNode requires a non-empty node id".to_string());
    }
    Ok(node_id.to_string())
}

fn native_plugin_transfer_direction_label(direction: BackgroundTransferDirection) -> &'static str {
    match direction {
        BackgroundTransferDirection::Upload => "upload",
        BackgroundTransferDirection::Download => "download",
    }
}

fn native_plugin_transfer_state_label(state: BackgroundTransferState) -> &'static str {
    match state {
        BackgroundTransferState::Pending => "pending",
        BackgroundTransferState::Active => "active",
        BackgroundTransferState::Paused => "paused",
        BackgroundTransferState::Completed => "completed",
        BackgroundTransferState::Cancelled => "cancelled",
        BackgroundTransferState::Error => "error",
    }
}

pub fn native_plugin_transfer_state_map(
    transfers: &Value,
) -> HashMap<String, BackgroundTransferState> {
    transfers
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|transfer| {
            let id = transfer.get("id").and_then(Value::as_str)?;
            let state = transfer
                .get("state")
                .and_then(Value::as_str)
                .and_then(native_plugin_transfer_state_from_label)?;
            Some((id.to_string(), state))
        })
        .collect()
}

fn native_plugin_transfer_state_from_label(state: &str) -> Option<BackgroundTransferState> {
    match state {
        "pending" => Some(BackgroundTransferState::Pending),
        "active" => Some(BackgroundTransferState::Active),
        "paused" => Some(BackgroundTransferState::Paused),
        "completed" => Some(BackgroundTransferState::Completed),
        "cancelled" => Some(BackgroundTransferState::Cancelled),
        "error" => Some(BackgroundTransferState::Error),
        _ => None,
    }
}

pub fn native_plugin_transfer_values_by_state(
    transfers: &Value,
    state: BackgroundTransferState,
) -> Vec<Value> {
    transfers
        .as_array()
        .into_iter()
        .flatten()
        .filter(|transfer| {
            transfer
                .get("state")
                .and_then(Value::as_str)
                .and_then(native_plugin_transfer_state_from_label)
                == Some(state)
        })
        .cloned()
        .collect()
}

pub fn native_plugin_transfer_transition_values(
    transfers: &Value,
    previous_states: &HashMap<String, BackgroundTransferState>,
    next_states: &HashMap<String, BackgroundTransferState>,
    target_state: BackgroundTransferState,
) -> Vec<Value> {
    transfers
        .as_array()
        .into_iter()
        .flatten()
        .filter(|transfer| {
            let Some(id) = transfer.get("id").and_then(Value::as_str) else {
                return false;
            };
            next_states.get(id) == Some(&target_state)
                && previous_states.get(id) != Some(&target_state)
        })
        .cloned()
        .collect()
}

pub fn native_plugin_transfer_progress_due(
    last_emitted: Option<Instant>,
    interval: Duration,
) -> bool {
    last_emitted
        .map(|last_emitted| last_emitted.elapsed() >= interval)
        .unwrap_or(true)
}
