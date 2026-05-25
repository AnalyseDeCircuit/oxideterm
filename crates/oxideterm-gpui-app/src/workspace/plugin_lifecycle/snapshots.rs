// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};
use oxideterm_notification_center::{EventCategory, EventLogEntry, EventSeverity};
use oxideterm_ssh::{
    ConnectionConsumer, ConnectionInfo, ConnectionState, NodeReadiness, NodeTreeSnapshotNode,
};
use serde_json::{Map, Value, json};

pub(super) fn native_plugin_connection_snapshot(connection: &ConnectionInfo) -> Value {
    // Tauri pluginUtils.toSnapshot exposes this exact read-only projection from
    // SshConnectionInfo. Native derives terminal ids from registry consumers so
    // the plugin never receives transport handles, auth material, or pool keys.
    json!({
        "id": connection.connection_id,
        "host": connection.host,
        "port": connection.port,
        "username": connection.username,
        "state": native_plugin_connection_state(&connection.state),
        "refCount": connection.ref_count,
        "keepAlive": connection.keep_alive,
        "createdAt": native_plugin_connection_time(connection.created_at),
        "lastActive": native_plugin_connection_time(connection.last_active_at),
        "terminalIds": native_plugin_connection_terminal_ids(&connection.consumers),
        "parentConnectionId": connection.parent_connection_id,
    })
}

pub(super) fn native_plugin_connection_state(state: &ConnectionState) -> Value {
    match state {
        ConnectionState::Connecting => json!("connecting"),
        ConnectionState::Active => json!("active"),
        ConnectionState::Idle => json!("idle"),
        ConnectionState::LinkDown => json!("link_down"),
        ConnectionState::Reconnecting => json!("reconnecting"),
        ConnectionState::Disconnecting => json!("disconnecting"),
        ConnectionState::Disconnected => json!("disconnected"),
        ConnectionState::Error(error) => json!({ "error": error }),
    }
}

fn native_plugin_connection_time(time: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn native_plugin_connection_terminal_ids(consumers: &[ConnectionConsumer]) -> Vec<String> {
    let mut terminal_ids = consumers
        .iter()
        .filter_map(|consumer| match consumer {
            ConnectionConsumer::Terminal(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    terminal_ids.sort();
    terminal_ids
}

pub(super) fn native_plugin_session_tree_from_nodes(
    mut nodes: Vec<NodeTreeSnapshotNode>,
    titles: &HashMap<String, String>,
    terminal_ids_by_node: &HashMap<String, Vec<String>>,
) -> Vec<Value> {
    nodes.sort_by_key(|node| (node.depth, node.created_at_ms, node.id.0.clone()));
    nodes
        .into_iter()
        .map(|node| native_plugin_session_node_snapshot(node, titles, terminal_ids_by_node))
        .collect()
}

fn native_plugin_session_node_snapshot(
    node: NodeTreeSnapshotNode,
    titles: &HashMap<String, String>,
    terminal_ids_by_node: &HashMap<String, Vec<String>>,
) -> Value {
    let node_id = node.id.0.clone();
    let mut terminal_ids = terminal_ids_by_node
        .get(&node_id)
        .cloned()
        .or_else(|| {
            node.terminal_session_id
                .clone()
                .map(|session_id| vec![session_id])
        })
        .unwrap_or_default();
    terminal_ids.sort();
    terminal_ids.dedup();
    let connection_state = native_plugin_session_connection_state(&node.state, terminal_ids.len());
    let label = titles
        .get(&node_id)
        .filter(|title| !title.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| format!("{}@{}", node.config.username, node.config.host));
    let mut value = json!({
        "id": node_id,
        "label": label,
        "host": node.config.host,
        "port": node.config.port,
        "username": node.config.username,
        "parentId": node.parent_id.map(|id| id.0),
        "childIds": node.children_ids.into_iter().map(|id| id.0).collect::<Vec<_>>(),
        "connectionState": connection_state,
        "connectionId": node.connection_id,
        "terminalIds": terminal_ids,
        "sftpSessionId": node.sftp_session_id,
    });
    if let (Some(error), Value::Object(fields)) = (node.state.error, &mut value) {
        fields.insert("errorMessage".to_string(), json!(error));
    }
    value
}

pub(super) fn native_plugin_session_connection_state(
    state: &oxideterm_ssh::NodeState,
    terminal_count: usize,
) -> &'static str {
    if state.error.as_deref() == Some("Link down") {
        return "link-down";
    }
    match state.readiness {
        NodeReadiness::Ready => {
            if terminal_count > 0 {
                "active"
            } else {
                "connected"
            }
        }
        NodeReadiness::Connecting => "connecting",
        NodeReadiness::Error => "error",
        NodeReadiness::Disconnected => "idle",
    }
}

pub(super) fn native_plugin_active_session_nodes(session_tree: &[Value]) -> Value {
    let active_nodes = session_tree
        .iter()
        .filter(|node| {
            matches!(
                node.get("connectionState").and_then(Value::as_str),
                Some("active" | "connected")
            )
        })
        .map(|node| {
            json!({
                "nodeId": node.get("id").and_then(Value::as_str).unwrap_or_default(),
                "sessionId": node
                    .get("terminalIds")
                    .and_then(Value::as_array)
                    .and_then(|terminal_ids| terminal_ids.first())
                    .cloned()
                    .unwrap_or(Value::Null),
                "connectionState": node
                    .get("connectionState")
                    .and_then(Value::as_str)
                    .unwrap_or("idle"),
            })
        })
        .collect::<Vec<_>>();
    json!(active_nodes)
}

pub(super) fn native_plugin_session_state_map(tree: &Value) -> HashMap<String, String> {
    tree.as_array()
        .map(|nodes| native_plugin_session_state_map_from_nodes(nodes))
        .unwrap_or_default()
}

pub(super) fn native_plugin_session_state_map_from_nodes(
    nodes: &[Value],
) -> HashMap<String, String> {
    nodes
        .iter()
        .filter_map(|node| {
            let node_id = node.get("id").and_then(Value::as_str)?;
            let state = node.get("connectionState").and_then(Value::as_str)?;
            Some((node_id.to_string(), state.to_string()))
        })
        .collect()
}

pub(super) fn native_plugin_event_log_entries<'a>(
    entries: impl Iterator<Item = &'a EventLogEntry>,
) -> Vec<Value> {
    entries
        .map(native_plugin_event_log_entry_snapshot)
        .collect()
}

pub(super) fn native_plugin_event_log_entry_snapshot(entry: &EventLogEntry) -> Value {
    let mut snapshot = Map::new();
    snapshot.insert("id".to_string(), json!(entry.id));
    snapshot.insert(
        "timestamp".to_string(),
        json!(native_plugin_unix_ms(entry.timestamp)),
    );
    snapshot.insert(
        "severity".to_string(),
        json!(native_plugin_event_severity(entry.severity)),
    );
    snapshot.insert(
        "category".to_string(),
        json!(native_plugin_event_category(entry.category)),
    );
    if let Some(node_id) = &entry.node_id {
        snapshot.insert("nodeId".to_string(), json!(node_id));
    }
    if let Some(connection_id) = &entry.connection_id {
        snapshot.insert("connectionId".to_string(), json!(connection_id));
    }
    snapshot.insert("title".to_string(), json!(entry.title));
    if let Some(detail) = &entry.detail {
        snapshot.insert("detail".to_string(), json!(detail));
    }
    snapshot.insert("source".to_string(), json!(entry.source));
    Value::Object(snapshot)
}

pub(super) fn native_plugin_filtered_event_log_entries(entries: &[Value], args: &Value) -> Value {
    let filter = args.get("filter").unwrap_or(args);
    let severity = filter.get("severity").and_then(Value::as_str);
    let category = filter.get("category").and_then(Value::as_str);
    let filtered = entries
        .iter()
        .filter(|entry| {
            severity.is_none_or(|severity| {
                entry.get("severity").and_then(Value::as_str) == Some(severity)
            }) && category.is_none_or(|category| {
                entry.get("category").and_then(Value::as_str) == Some(category)
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    json!(filtered)
}

fn native_plugin_event_severity(severity: EventSeverity) -> &'static str {
    match severity {
        EventSeverity::Info => "info",
        EventSeverity::Warn => "warn",
        EventSeverity::Error => "error",
    }
}

fn native_plugin_event_category(category: EventCategory) -> &'static str {
    match category {
        EventCategory::Connection => "connection",
        EventCategory::Reconnect => "reconnect",
        EventCategory::Node => "node",
    }
}

fn native_plugin_unix_ms(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
