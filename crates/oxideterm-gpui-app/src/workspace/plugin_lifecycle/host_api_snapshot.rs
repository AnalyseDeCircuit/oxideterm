// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Read-only plugin host API snapshots and returnable call routing.

use std::collections::HashMap;

use chrono::{SecondsFormat, Utc};
use gpui::Context;
use oxideterm_i18n::I18n;
use serde_json::{Value, json};

use super::*;

#[derive(Clone)]
pub(super) struct NativePluginHostApiSnapshot {
    pub(super) registry: super::super::plugin_host::NativePluginRegistry,
    pub(super) i18n: I18n,
    pub(super) settings: Value,
    pub(super) locale: String,
    pub(super) theme_name: String,
    pub(super) pool_stats: Value,
    pub(super) layout: Value,
    pub(super) connections: Vec<Value>,
    pub(super) connection_states: HashMap<String, Value>,
    pub(super) node_connection_ids: HashMap<String, String>,
    pub(super) session_tree: Vec<Value>,
    pub(super) session_node_states: HashMap<String, String>,
    pub(super) event_log_entries: Vec<Value>,
    pub(super) active_terminal_target: Value,
    pub(super) terminal_nodes: HashMap<String, NativePluginTerminalNodeSnapshot>,
}

impl NativePluginHostApiSnapshot {
    pub(super) fn from_workspace(workspace: &WorkspaceApp, cx: &mut Context<WorkspaceApp>) -> Self {
        let settings = workspace.settings_store.settings();
        let monitor_stats = workspace.ssh_registry.monitor_stats();
        let mut connection_infos = workspace.ssh_registry.list();
        connection_infos.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));
        let connections = connection_infos
            .iter()
            .map(native_plugin_connection_snapshot)
            .collect::<Vec<_>>();
        let connection_states = connection_infos
            .iter()
            .map(|info| {
                (
                    info.connection_id.clone(),
                    native_plugin_connection_state(&info.state),
                )
            })
            .collect::<HashMap<_, _>>();
        let node_connection_ids = workspace
            .node_runtime_store
            .export_snapshot()
            .nodes
            .into_iter()
            .filter_map(|node| {
                node.connection_id
                    .map(|connection_id| (node.id.0, connection_id))
            })
            .collect::<HashMap<_, _>>();
        let session_tree = workspace.native_plugin_session_tree_snapshot_values();
        let session_node_states = native_plugin_session_state_map_from_nodes(&session_tree);
        let event_log_entries =
            native_plugin_event_log_entries(workspace.notification_center.event_log.entries.iter());
        let (active_terminal_target, terminal_nodes) =
            native_plugin_terminal_snapshots(workspace, &connection_states, cx);
        Self {
            registry: workspace.plugin_registry.clone(),
            i18n: workspace.i18n.clone(),
            settings: serde_json::to_value(settings).unwrap_or_else(|_| json!({})),
            locale: settings.general.language.as_str().to_string(),
            theme_name: settings.terminal.theme.clone(),
            // Tauri's PluginAppAPI exposes the compact ssh_get_pool_stats shape,
            // not the full native monitor payload. Keep this RPC-compatible.
            pool_stats: json!({
                "activeConnections": monitor_stats.active_connections,
                "totalSessions": monitor_stats.total_terminals,
            }),
            layout: workspace.native_plugin_layout_snapshot(),
            connections,
            connection_states,
            node_connection_ids,
            session_tree,
            session_node_states,
            event_log_entries,
            active_terminal_target,
            terminal_nodes,
        }
    }
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

pub(super) fn native_plugin_ui_registration_from_args(
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

pub(super) fn native_plugin_ui_tab_id_arg(args: &Value) -> Option<String> {
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

pub(super) fn native_plugin_returnable_host_api_response(
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
