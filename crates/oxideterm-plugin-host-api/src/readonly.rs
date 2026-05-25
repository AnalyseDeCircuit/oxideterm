// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Read-only plugin host API snapshots and returnable call routing.

use std::collections::HashMap;

use chrono::{SecondsFormat, Utc};
use oxideterm_i18n::I18n;
use oxideterm_plugin_protocol as plugin_runtime;
use oxideterm_plugin_registry::NativePluginRegistry;
use serde_json::{Value, json};

use crate::{
    app::{
        native_plugin_custom_event_from_args, native_plugin_i18n_translate,
        native_plugin_platform_label, native_plugin_settings_section, native_plugin_theme_snapshot,
    },
    settings::{
        native_normalize_syncable_settings_payload, native_syncable_settings_payload,
        native_syncable_settings_payload_arg, native_syncable_settings_revision,
    },
    terminal::{
        NativePluginTerminalNodeSnapshot, native_plugin_terminal_buffer_size_response,
        native_plugin_terminal_scroll_buffer_response, native_plugin_terminal_search_response,
    },
};

#[derive(Clone)]
pub struct NativePluginHostApiSnapshot {
    pub registry: NativePluginRegistry,
    pub i18n: I18n,
    pub settings: Value,
    pub locale: String,
    pub theme_name: String,
    pub pool_stats: Value,
    pub layout: Value,
    pub connections: Vec<Value>,
    pub connection_states: HashMap<String, Value>,
    pub node_connection_ids: HashMap<String, String>,
    pub session_tree: Vec<Value>,
    pub session_node_states: HashMap<String, String>,
    pub event_log_entries: Vec<Value>,
    pub active_terminal_target: Value,
    pub terminal_nodes: HashMap<String, NativePluginTerminalNodeSnapshot>,
}

fn native_plugin_ui_registration_preflight_response(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    kind: plugin_runtime::PluginRegistrationKind,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match native_plugin_ui_registration_from_args(plugin_id, kind, &call.args).and_then(
        |registration| {
            let mut registry = snapshot.registry.clone();
            registry.apply_runtime_registration(registration)
        },
    ) {
        Ok(()) => plugin_runtime::PluginResponse::ok(
            request_id,
            json!({
                "queued": true,
            }),
        ),
        Err(error) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("invalid_declarative_ui", error),
        ),
    }
}

fn native_plugin_ui_open_tab_preflight_response(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let Some(tab_id) = native_plugin_ui_tab_id_arg(&call.args) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_plugin_tab",
                "Native plugin ui.openTab requires args.tabId",
            ),
        );
    };
    if snapshot
        .registry
        .contributions()
        .tab_contribution(plugin_id, &tab_id)
        .is_none()
    {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "plugin_tab_not_declared",
                format!("Tab \"{tab_id}\" not declared in manifest contributes.tabs"),
            ),
        );
    }
    plugin_runtime::PluginResponse::ok(request_id, json!({ "queued": true }))
}

pub fn native_plugin_ui_registration_from_args(
    plugin_id: &str,
    kind: plugin_runtime::PluginRegistrationKind,
    args: &Value,
) -> Result<plugin_runtime::PluginRegistration, String> {
    let view_id = match kind {
        plugin_runtime::PluginRegistrationKind::Tab => native_plugin_ui_tab_id_arg(args)
            .ok_or_else(|| "Native plugin ui.registerTabView requires args.tabId".to_string())?,
        plugin_runtime::PluginRegistrationKind::SidebarPanel => native_plugin_ui_panel_id_arg(args)
            .ok_or_else(|| {
                "Native plugin ui.registerSidebarPanel requires args.panelId".to_string()
            })?,
        _ => return Err("Unsupported native plugin declarative UI registration kind".to_string()),
    };
    let registration_id = args
        .get("registrationId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| native_plugin_ui_registration_id(kind, &view_id));
    Ok(plugin_runtime::PluginRegistration {
        registration_id,
        plugin_id: plugin_id.to_string(),
        kind,
        metadata: args.clone(),
    })
}

pub fn native_plugin_ui_tab_id_arg(args: &Value) -> Option<String> {
    args.get("tabId")
        .or_else(|| args.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn native_plugin_ui_panel_id_arg(args: &Value) -> Option<String> {
    args.get("panelId")
        .or_else(|| args.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn native_plugin_ui_registration_id(
    kind: plugin_runtime::PluginRegistrationKind,
    view_id: &str,
) -> String {
    let namespace = match kind {
        plugin_runtime::PluginRegistrationKind::Tab => "tab",
        plugin_runtime::PluginRegistrationKind::SidebarPanel => "sidebar-panel",
        _ => "view",
    };
    format!("ctx.ui.{namespace}:{view_id}")
}

pub fn native_plugin_returnable_host_api_response(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
) -> Option<plugin_runtime::PluginResponse> {
    match (call.namespace.as_str(), call.method.as_str()) {
        ("api", "invoke") => Some(plugin_runtime::PluginResponse::error(
            call.request_id,
            plugin_runtime::PluginError::runtime(
                "backend_adapter_unavailable",
                "api.invoke is resolved by the Workspace backend adapter",
            ),
        )),
        ("ui", "registerTabView") => Some(native_plugin_ui_registration_preflight_response(
            snapshot,
            plugin_id,
            call,
            plugin_runtime::PluginRegistrationKind::Tab,
        )),
        ("ui", "registerSidebarPanel") => Some(native_plugin_ui_registration_preflight_response(
            snapshot,
            plugin_id,
            call,
            plugin_runtime::PluginRegistrationKind::SidebarPanel,
        )),
        ("ui", "openTab") => Some(native_plugin_ui_open_tab_preflight_response(
            snapshot, plugin_id, call,
        )),
        ("app", "getTheme") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            native_plugin_theme_snapshot(&snapshot.theme_name),
        )),
        ("app", "getSettings") => {
            let Some(category) = call.args.get("category").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_settings_category",
                        "Native plugin app.getSettings requires args.category",
                    ),
                ));
            };
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                native_plugin_settings_section(&snapshot.settings, category),
            ))
        }
        ("app", "getVersion") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(env!("CARGO_PKG_VERSION")),
        )),
        ("app", "getPlatform") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(native_plugin_platform_label()),
        )),
        ("app", "getLocale") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(snapshot.locale),
        )),
        ("app", "getPoolStats") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            snapshot.pool_stats.clone(),
        )),
        ("connections", "getAll") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(snapshot.connections),
        )),
        ("connections", "get") => {
            let Some(connection_id) = call.args.get("connectionId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_connection_id",
                        "Native plugin connections.get requires args.connectionId",
                    ),
                ));
            };
            let connection = snapshot
                .connections
                .iter()
                .find(|connection| {
                    connection.get("id").and_then(Value::as_str) == Some(connection_id)
                })
                .cloned()
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                connection,
            ))
        }
        ("connections", "getState") => {
            let Some(connection_id) = call.args.get("connectionId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_connection_id",
                        "Native plugin connections.getState requires args.connectionId",
                    ),
                ));
            };
            let state = snapshot
                .connection_states
                .get(connection_id)
                .cloned()
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, state))
        }
        ("connections", "getByNode") => {
            let Some(node_id) = call.args.get("nodeId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_node_id",
                        "Native plugin connections.getByNode requires args.nodeId",
                    ),
                ));
            };
            let connection = snapshot
                .node_connection_ids
                .get(node_id)
                .and_then(|connection_id| {
                    snapshot.connections.iter().find(|connection| {
                        connection.get("id").and_then(Value::as_str) == Some(connection_id.as_str())
                    })
                })
                .cloned()
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                connection,
            ))
        }
        ("sessions", "getTree") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(snapshot.session_tree),
        )),
        ("sessions", "getActiveNodes") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            native_plugin_active_session_nodes(&snapshot.session_tree),
        )),
        ("sessions", "getNodeState") => {
            let Some(node_id) = call.args.get("nodeId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_node_id",
                        "Native plugin sessions.getNodeState requires args.nodeId",
                    ),
                ));
            };
            let state = snapshot
                .session_node_states
                .get(node_id)
                .map(|state| json!(state))
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, state))
        }
        ("eventLog", "getEntries") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            native_plugin_filtered_event_log_entries(&snapshot.event_log_entries, &call.args),
        )),
        ("terminal", "getActiveTarget") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            snapshot.active_terminal_target.clone(),
        )),
        ("terminal", "getNodeBuffer") => {
            let Some(node_id) = call.args.get("nodeId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_node_id",
                        "Native plugin terminal.getNodeBuffer requires args.nodeId",
                    ),
                ));
            };
            let value = snapshot
                .terminal_nodes
                .get(node_id)
                .map(|terminal| json!(terminal.buffer))
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, value))
        }
        ("terminal", "getNodeSelection") => {
            let Some(node_id) = call.args.get("nodeId").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_node_id",
                        "Native plugin terminal.getNodeSelection requires args.nodeId",
                    ),
                ));
            };
            let value = snapshot
                .terminal_nodes
                .get(node_id)
                .and_then(|terminal| terminal.selection.clone())
                .map(Value::String)
                .unwrap_or(Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, value))
        }
        ("terminal", "search") => Some(native_plugin_terminal_search_response(
            call.request_id,
            &snapshot.terminal_nodes,
            call.args,
        )),
        ("terminal", "getScrollBuffer") => Some(native_plugin_terminal_scroll_buffer_response(
            call.request_id,
            &snapshot.terminal_nodes,
            call.args,
        )),
        ("terminal", "getBufferSize") => Some(native_plugin_terminal_buffer_size_response(
            call.request_id,
            &snapshot.terminal_nodes,
            call.args,
        )),
        ("ui", "getLayout") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            snapshot.layout.clone(),
        )),
        ("events", "emit") => Some(
            match native_plugin_custom_event_from_args(plugin_id, call.args) {
                Ok((event_key, _payload)) => plugin_runtime::PluginResponse::ok(
                    call.request_id,
                    json!({
                        "emitted": true,
                        "event": event_key,
                    }),
                ),
                Err(error) => plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol("invalid_plugin_event", error),
                ),
            },
        ),
        ("i18n", "getLanguage") => Some(plugin_runtime::PluginResponse::ok(
            call.request_id,
            json!(snapshot.locale),
        )),
        ("i18n", "t") => {
            let Some(key) = call.args.get("key").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_i18n_key",
                        "Native plugin i18n.t requires args.key",
                    ),
                ));
            };
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                json!(native_plugin_i18n_translate(&snapshot.i18n, plugin_id, key)),
            ))
        }
        ("settings", "get") => {
            let Some(key) = call.args.get("key").and_then(Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_plugin_setting_key",
                        "Native plugin settings.get requires args.key",
                    ),
                ));
            };
            // Native plugin settings are declaration-backed. This intentionally
            // uses the same registry path as manifest-rendered settings controls
            // so runtime plugins cannot create a parallel config namespace.
            let value = snapshot
                .registry
                .plugin_setting_value(plugin_id, key)
                .unwrap_or(serde_json::Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, value))
        }
        ("settings", "exportSyncableSettings") => {
            let normalized = native_normalize_syncable_settings_payload(
                &native_syncable_settings_payload(&snapshot.settings),
            );
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                json!({
                    "revision": native_syncable_settings_revision(&normalized.payload),
                    "exportedAt": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                    "payload": normalized.payload,
                    "warnings": normalized.warnings,
                }),
            ))
        }
        ("settings", "applySyncableSettings") => {
            let normalized = native_normalize_syncable_settings_payload(
                &native_syncable_settings_payload_arg(call.args),
            );
            Some(plugin_runtime::PluginResponse::ok(
                call.request_id,
                json!({
                    "revision": native_syncable_settings_revision(&normalized.payload),
                    "appliedPayload": normalized.payload,
                    "warnings": normalized.warnings,
                }),
            ))
        }
        ("storage", "get") => {
            let Some(key) = call.args.get("key").and_then(serde_json::Value::as_str) else {
                return Some(plugin_runtime::PluginResponse::error(
                    call.request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_storage_key",
                        "Native plugin storage.get requires args.key",
                    ),
                ));
            };
            // Tauri localStorage-backed plugin storage returns null for missing
            // or unreadable JSON. Native mirrors that through a scoped registry
            // lookup and returns the raw JSON value to the process runtime.
            let value = snapshot
                .registry
                .plugin_storage_value(plugin_id, key)
                .unwrap_or(serde_json::Value::Null);
            Some(plugin_runtime::PluginResponse::ok(call.request_id, value))
        }
        _ => None,
    }
}

pub fn native_plugin_active_session_nodes(session_tree: &[Value]) -> Value {
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

pub fn native_plugin_session_state_map(tree: &Value) -> HashMap<String, String> {
    tree.as_array()
        .map(|nodes| native_plugin_session_state_map_from_nodes(nodes))
        .unwrap_or_default()
}

pub fn native_plugin_session_state_map_from_nodes(nodes: &[Value]) -> HashMap<String, String> {
    nodes
        .iter()
        .filter_map(|node| {
            let node_id = node.get("id").and_then(Value::as_str)?;
            let state = node.get("connectionState").and_then(Value::as_str)?;
            Some((node_id.to_string(), state.to_string()))
        })
        .collect()
}

pub fn native_plugin_filtered_event_log_entries(entries: &[Value], args: &Value) -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_i18n::Locale;

    fn sample_snapshot() -> NativePluginHostApiSnapshot {
        NativePluginHostApiSnapshot {
            registry: NativePluginRegistry::default(),
            i18n: I18n::new(Locale::ZhCn),
            settings: json!({
                "terminal": {
                    "theme": "default-dark",
                    "fontSize": 14,
                },
                "general": {
                    "language": "zh-CN",
                }
            }),
            locale: "zh-CN".to_string(),
            theme_name: "default-dark".to_string(),
            pool_stats: json!({
                "activeConnections": 1,
                "totalSessions": 2,
            }),
            layout: json!({
                "sidebarCollapsed": false,
                "activeTabId": "tab-1",
                "tabCount": 1,
            }),
            connections: Vec::new(),
            connection_states: HashMap::new(),
            node_connection_ids: HashMap::new(),
            session_tree: vec![json!({
                "id": "node-1",
                "connectionState": "active",
                "terminalIds": ["term-1"],
            })],
            session_node_states: HashMap::from([("node-1".to_string(), "active".to_string())]),
            event_log_entries: vec![json!({
                "id": 1,
                "severity": "info",
                "category": "connection",
            })],
            active_terminal_target: Value::Null,
            terminal_nodes: HashMap::from([(
                "node-1".to_string(),
                NativePluginTerminalNodeSnapshot {
                    buffer: "alpha\nbeta".to_string(),
                    selection: Some("beta".to_string()),
                    current_lines: 2,
                },
            )]),
        }
    }

    fn host_call(namespace: &str, method: &str, args: Value) -> plugin_runtime::PluginHostCall {
        plugin_runtime::PluginHostCall {
            request_id: format!("{namespace}.{method}"),
            namespace: namespace.to_string(),
            method: method.to_string(),
            args,
        }
    }

    #[test]
    fn readonly_dispatcher_returns_app_theme_and_settings_sections() {
        let snapshot = sample_snapshot();
        let theme = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            host_call("app", "getTheme", Value::Null),
        )
        .unwrap();
        assert!(matches!(
            theme.result,
            plugin_runtime::PluginResponseResult::Ok { value }
                if value.get("name").and_then(Value::as_str) == Some("default-dark")
        ));

        let settings = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            host_call("app", "getSettings", json!({ "category": "terminal" })),
        )
        .unwrap();
        assert!(matches!(
            settings.result,
            plugin_runtime::PluginResponseResult::Ok { value }
                if value.get("fontSize").and_then(Value::as_i64) == Some(14)
        ));
    }

    #[test]
    fn readonly_dispatcher_filters_sessions_events_and_terminal_search() {
        let snapshot = sample_snapshot();
        let active_nodes = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            host_call("sessions", "getActiveNodes", Value::Null),
        )
        .unwrap();
        assert!(matches!(
            active_nodes.result,
            plugin_runtime::PluginResponseResult::Ok { value }
                if value.as_array().map(Vec::len) == Some(1)
        ));

        let filtered_log = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            host_call(
                "eventLog",
                "getEntries",
                json!({ "filter": { "severity": "info" } }),
            ),
        )
        .unwrap();
        assert!(matches!(
            filtered_log.result,
            plugin_runtime::PluginResponseResult::Ok { value }
                if value.as_array().map(Vec::len) == Some(1)
        ));

        let search = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            host_call(
                "terminal",
                "search",
                json!({ "nodeId": "node-1", "query": "beta" }),
            ),
        )
        .unwrap();
        assert!(matches!(
            search.result,
            plugin_runtime::PluginResponseResult::Ok { value }
                if value.get("total_matches").and_then(Value::as_u64) == Some(1)
        ));
    }

    #[test]
    fn readonly_dispatcher_scopes_custom_events_to_plugin() {
        let snapshot = sample_snapshot();
        let response = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            host_call("events", "emit", json!({ "name": "ready" })),
        )
        .unwrap();
        assert!(matches!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok { value }
                if value.get("event").and_then(Value::as_str) == Some("plugin.com.example.demo:ready")
        ));
    }
}
