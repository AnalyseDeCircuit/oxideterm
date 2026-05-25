// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::{Arc, mpsc};

use oxideterm_forwarding::{
    ForwardRule, ForwardStats, ForwardStatus, ForwardType, ForwardUpdate, ForwardingRegistry,
    SavedForwardsSyncSnapshot,
};
use serde_json::{Value, json};

use oxideterm_plugin_protocol as plugin_runtime;

use crate::capabilities::NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD;

// Keeps the forward namespace together: permission checks, saved-forward sync,
// live manager calls, and plugin-facing JSON snapshots share one contract.
pub fn native_plugin_forward_response(
    call: plugin_runtime::PluginHostCall,
    permissions: &plugin_runtime::PluginPermissionSet,
    registry: &ForwardingRegistry,
    runtime: &Arc<tokio::runtime::Runtime>,
    valid_owner_connection_ids: &std::collections::HashSet<String>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    if let Err(error) = native_plugin_forward_check_capability(&call.method, permissions) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("plugin_forward_capability_denied", error),
        );
    }

    match call.method.as_str() {
        "listSavedForwards" => {
            let value = match native_plugin_forward_saved_forwards(registry) {
                Ok(value) => value,
                Err(error) => {
                    return plugin_runtime::PluginResponse::error(
                        request_id,
                        plugin_runtime::PluginError::runtime("plugin_forward_error", error),
                    );
                }
            };
            return plugin_runtime::PluginResponse::ok(request_id, value);
        }
        "exportSavedForwardsSnapshot" => {
            let value = match registry.export_saved_forwards_snapshot() {
                Ok(snapshot) => json!(snapshot),
                Err(error) => {
                    return plugin_runtime::PluginResponse::error(
                        request_id,
                        plugin_runtime::PluginError::runtime(
                            "plugin_forward_error",
                            error.to_string(),
                        ),
                    );
                }
            };
            return plugin_runtime::PluginResponse::ok(request_id, value);
        }
        "applySavedForwardsSnapshot" => {
            let snapshot =
                match native_plugin_forward_snapshot_arg::<SavedForwardsSyncSnapshot>(&call.args) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        return plugin_runtime::PluginResponse::error(
                            request_id,
                            plugin_runtime::PluginError::protocol(
                                "invalid_forward_snapshot",
                                error,
                            ),
                        );
                    }
                };
            let value = match registry
                .apply_saved_forwards_snapshot(snapshot, valid_owner_connection_ids)
            {
                Ok(result) => json!(result),
                Err(error) => {
                    return plugin_runtime::PluginResponse::error(
                        request_id,
                        plugin_runtime::PluginError::runtime(
                            "plugin_forward_error",
                            error.to_string(),
                        ),
                    );
                }
            };
            return plugin_runtime::PluginResponse::ok(request_id, value);
        }
        _ => {}
    }

    let method = call.method.clone();
    let args = call.args.clone();
    let registry = registry.clone();
    let (response_tx, response_rx) = mpsc::channel();
    // Forward listener creation and teardown can await SSH channel operations.
    // Keep those operations on the long-lived forwarding runtime that owns the
    // registry managers instead of the plugin stdio reader.
    runtime.spawn(async move {
        let result = native_plugin_forward_async_result(&registry, &method, &args).await;
        let _ = response_tx.send(result);
    });

    match response_rx.recv() {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_forward_error", error),
        ),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_forward_unavailable",
                "Native plugin forwarding worker closed before returning a response",
            ),
        ),
    }
}

async fn native_plugin_forward_async_result(
    registry: &ForwardingRegistry,
    method: &str,
    args: &Value,
) -> Result<Value, String> {
    match method {
        "list" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(json!(
                manager
                    .list_forwards()
                    .into_iter()
                    .map(native_plugin_forward_rule_snapshot)
                    .collect::<Vec<_>>()
            ))
        }
        "create" => {
            let request = native_plugin_forward_create_request(args)?;
            let manager = native_plugin_forward_manager(registry, &request.session_id)?;
            let rule = native_plugin_forward_rule_from_request(&request);
            let response = match manager.create_forward(rule).await {
                Ok(rule) => json!({
                    "success": true,
                    "forward": native_plugin_forward_rule_snapshot(rule),
                    "error": Value::Null,
                }),
                Err(error) => json!({
                    "success": false,
                    "forward": Value::Null,
                    "error": error.to_string(),
                }),
            };
            Ok(response)
        }
        "stop" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(match manager.stop_forward(&forward_id).await {
                Ok(_) => native_plugin_forward_response_value(true, Value::Null, None),
                Err(error) => native_plugin_forward_response_value(
                    false,
                    Value::Null,
                    Some(error.to_string()),
                ),
            })
        }
        "delete" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(match manager.delete_forward(&forward_id).await {
                Ok(_) => native_plugin_forward_response_value(true, Value::Null, None),
                Err(error) => native_plugin_forward_response_value(
                    false,
                    Value::Null,
                    Some(error.to_string()),
                ),
            })
        }
        "restart" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(match manager.restart_forward(&forward_id).await {
                Ok(rule) => native_plugin_forward_response_value(
                    true,
                    native_plugin_forward_rule_snapshot(rule),
                    None,
                ),
                Err(error) => native_plugin_forward_response_value(
                    false,
                    Value::Null,
                    Some(error.to_string()),
                ),
            })
        }
        "update" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let update = native_plugin_forward_update_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            Ok(match manager.update_forward(&forward_id, update) {
                Ok(rule) => native_plugin_forward_response_value(
                    true,
                    native_plugin_forward_rule_snapshot(rule),
                    None,
                ),
                Err(error) => native_plugin_forward_response_value(
                    false,
                    Value::Null,
                    Some(error.to_string()),
                ),
            })
        }
        "stopAll" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            if let Some(manager) = registry.get(&session_id) {
                manager.stop_all().await;
            }
            Ok(Value::Null)
        }
        "getStats" => {
            let session_id = native_plugin_forward_session_id_arg(args)?;
            let forward_id = native_plugin_forward_id_arg(args)?;
            let manager = native_plugin_forward_manager(registry, &session_id)?;
            match manager.get_stats(&forward_id) {
                Ok(stats) => Ok(native_plugin_forward_stats_snapshot(stats)),
                Err(oxideterm_forwarding::ForwardingError::NotFound(_)) => Ok(Value::Null),
                Err(error) => Err(error.to_string()),
            }
        }
        "onSavedForwardsChange" => Err(
            "forward.onSavedForwardsChange is registered through the native event subscription bridge, not as a direct host call"
                .to_string(),
        ),
        method => Err(format!("Unsupported forward host call: {method}")),
    }
}

pub fn native_plugin_forward_check_capability(
    method: &str,
    permissions: &plugin_runtime::PluginPermissionSet,
) -> Result<(), String> {
    let requires_forward = matches!(
        method,
        "create"
            | "stop"
            | "delete"
            | "restart"
            | "update"
            | "stopAll"
            | "list"
            | "getStats"
            | "listSavedForwards"
            | "onSavedForwardsChange"
            | "exportSavedForwardsSnapshot"
            | "applySavedForwardsSnapshot"
    );
    if !requires_forward {
        return Ok(());
    }
    if permissions
        .capabilities
        .iter()
        .any(|capability| capability == NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD)
    {
        return Ok(());
    }
    Err(format!(
        "Native plugin forward host call \"{method}\" requires capability \"{NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD}\""
    ))
}

fn native_plugin_forward_manager(
    registry: &ForwardingRegistry,
    session_id: &str,
) -> Result<Arc<oxideterm_forwarding::ForwardingManager>, String> {
    registry
        .get(session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))
}

pub fn native_plugin_forward_saved_forwards(
    registry: &ForwardingRegistry,
) -> Result<Value, String> {
    let snapshot = registry
        .export_saved_forwards_snapshot()
        .map_err(|error| error.to_string())?;
    let forwards = snapshot
        .records
        .into_iter()
        .filter(|record| !record.deleted)
        .filter_map(|record| record.payload)
        .map(|payload| json!(payload))
        .collect::<Vec<_>>();
    Ok(json!(forwards))
}

fn native_plugin_forward_snapshot_arg<T>(args: &Value) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let value = args
        .get("snapshot")
        .cloned()
        .unwrap_or_else(|| args.clone());
    serde_json::from_value(value).map_err(|error| error.to_string())
}

#[derive(Clone)]
pub struct NativePluginForwardCreateRequest {
    pub session_id: String,
    pub forward_type: ForwardType,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub description: String,
}

pub fn native_plugin_forward_create_request(
    args: &Value,
) -> Result<NativePluginForwardCreateRequest, String> {
    let request = args.get("request").unwrap_or(args);
    let session_id = native_plugin_required_string(request, "sessionId")
        .or_else(|_| native_plugin_required_string(request, "session_id"))?;
    let forward_type = native_plugin_forward_type_arg(request)?;
    let bind_address = native_plugin_required_string(request, "bindAddress")
        .or_else(|_| native_plugin_required_string(request, "bind_address"))?;
    let bind_port = native_plugin_port_arg(request, "bindPort")
        .or_else(|_| native_plugin_port_arg(request, "bind_port"))?;
    let target_host = native_plugin_required_string(request, "targetHost")
        .or_else(|_| native_plugin_required_string(request, "target_host"))
        .unwrap_or_default();
    let target_port = native_plugin_port_arg(request, "targetPort")
        .or_else(|_| native_plugin_port_arg(request, "target_port"))
        .unwrap_or_default();
    let description = request
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(NativePluginForwardCreateRequest {
        session_id,
        forward_type,
        bind_address,
        bind_port,
        target_host,
        target_port,
        description,
    })
}

fn native_plugin_forward_rule_from_request(
    request: &NativePluginForwardCreateRequest,
) -> ForwardRule {
    let mut rule = match request.forward_type {
        ForwardType::Local => ForwardRule::local(
            request.bind_address.clone(),
            request.bind_port,
            request.target_host.clone(),
            request.target_port,
        ),
        ForwardType::Remote => ForwardRule::remote(
            request.bind_address.clone(),
            request.bind_port,
            request.target_host.clone(),
            request.target_port,
        ),
        ForwardType::Dynamic => {
            ForwardRule::dynamic(request.bind_address.clone(), request.bind_port)
        }
    };
    rule.description = request.description.clone();
    rule
}

fn native_plugin_forward_update_arg(args: &Value) -> Result<ForwardUpdate, String> {
    let request = args.get("request").unwrap_or(args);
    Ok(ForwardUpdate {
        forward_type: request
            .get("forwardType")
            .or_else(|| request.get("forward_type"))
            .and_then(Value::as_str)
            .map(native_plugin_forward_type_from_label)
            .transpose()?,
        bind_address: native_plugin_optional_string_arg(request, "bindAddress")
            .or_else(|| native_plugin_optional_string_arg(request, "bind_address")),
        bind_port: native_plugin_optional_port_arg(request, "bindPort")
            .or_else(|| native_plugin_optional_port_arg(request, "bind_port")),
        target_host: native_plugin_optional_string_arg(request, "targetHost")
            .or_else(|| native_plugin_optional_string_arg(request, "target_host")),
        target_port: native_plugin_optional_port_arg(request, "targetPort")
            .or_else(|| native_plugin_optional_port_arg(request, "target_port")),
        description: native_plugin_optional_string_arg(request, "description"),
    })
}

fn native_plugin_forward_response_value(
    success: bool,
    forward: Value,
    error: Option<String>,
) -> Value {
    json!({
        "success": success,
        "forward": forward,
        "error": error,
    })
}

fn native_plugin_forward_session_id_arg(args: &Value) -> Result<String, String> {
    native_plugin_required_string(args, "sessionId")
        .or_else(|_| native_plugin_required_string(args, "session_id"))
}

fn native_plugin_forward_id_arg(args: &Value) -> Result<String, String> {
    native_plugin_required_string(args, "forwardId")
        .or_else(|_| native_plugin_required_string(args, "forward_id"))
}

fn native_plugin_forward_type_arg(args: &Value) -> Result<ForwardType, String> {
    let value = native_plugin_required_string(args, "forwardType")
        .or_else(|_| native_plugin_required_string(args, "forward_type"))?;
    native_plugin_forward_type_from_label(&value)
}

fn native_plugin_forward_type_from_label(value: &str) -> Result<ForwardType, String> {
    match value {
        "local" => Ok(ForwardType::Local),
        "remote" => Ok(ForwardType::Remote),
        "dynamic" => Ok(ForwardType::Dynamic),
        _ => Err(format!("Invalid forward type: {value}")),
    }
}

fn native_plugin_port_arg(args: &Value, field: &str) -> Result<u16, String> {
    let port = args
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("forward host call requires args.{field}"))?;
    u16::try_from(port).map_err(|_| format!("forward args.{field} is outside u16 range"))
}

fn native_plugin_optional_port_arg(args: &Value, field: &str) -> Option<u16> {
    args.get(field)
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
}

fn native_plugin_optional_string_arg(args: &Value, field: &str) -> Option<String> {
    args.get(field).and_then(Value::as_str).map(str::to_string)
}

fn native_plugin_required_string(args: &Value, field: &str) -> Result<String, String> {
    args.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("forward host call requires args.{field}"))
}

pub fn native_plugin_forward_rule_snapshot(rule: ForwardRule) -> Value {
    json!({
        "id": rule.id,
        "forward_type": native_plugin_forward_type_label(rule.forward_type),
        "bind_address": rule.bind_address,
        "bind_port": rule.bind_port,
        "target_host": rule.target_host,
        "target_port": rule.target_port,
        "status": native_plugin_forward_status_label(&rule.status),
        "description": if rule.description.is_empty() { Value::Null } else { json!(rule.description) },
    })
}

fn native_plugin_forward_stats_snapshot(stats: ForwardStats) -> Value {
    json!({
        "connectionCount": stats.connection_count,
        "activeConnections": stats.active_connections,
        "bytesSent": stats.bytes_sent,
        "bytesReceived": stats.bytes_received,
    })
}

fn native_plugin_forward_type_label(forward_type: ForwardType) -> &'static str {
    match forward_type {
        ForwardType::Local => "local",
        ForwardType::Remote => "remote",
        ForwardType::Dynamic => "dynamic",
    }
}

fn native_plugin_forward_status_label(status: &ForwardStatus) -> &'static str {
    match status {
        ForwardStatus::Starting => "starting",
        ForwardStatus::Active => "active",
        ForwardStatus::Stopped => "stopped",
        ForwardStatus::Error => "error",
        ForwardStatus::Suspended => "suspended",
    }
}
