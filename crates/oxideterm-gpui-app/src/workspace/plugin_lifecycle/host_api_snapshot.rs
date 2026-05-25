// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Workspace sampling for read-only native plugin host API snapshots.
//!
//! `oxideterm-plugin-host-api` owns the DTO and dispatcher. The GPUI app only
//! reads live workspace state that requires `WorkspaceApp` or `Context`.

use std::collections::HashMap;

use gpui::Context;
use serde_json::json;

use super::*;

pub(super) use oxideterm_plugin_host_api::readonly::{
    NativePluginHostApiSnapshot, native_plugin_returnable_host_api_response,
    native_plugin_ui_registration_from_args, native_plugin_ui_tab_id_arg,
};

pub(super) fn native_plugin_host_api_snapshot_from_workspace(
    workspace: &WorkspaceApp,
    cx: &mut Context<WorkspaceApp>,
) -> NativePluginHostApiSnapshot {
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

    NativePluginHostApiSnapshot {
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
