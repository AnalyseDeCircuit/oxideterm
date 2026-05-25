// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use gpui::Context;
use serde_json::{Value, json};

use super::{TabKind, TerminalSessionId, WorkspaceApp};
use oxideterm_plugin_host_api::terminal::NativePluginTerminalNodeSnapshot;

// Terminal read APIs project pane state into the plugin contract. Keeping search
// and scroll-buffer code here prevents lifecycle from owning terminal query rules.
pub(super) fn native_plugin_terminal_snapshots(
    workspace: &WorkspaceApp,
    connection_states: &HashMap<String, Value>,
    cx: &mut Context<WorkspaceApp>,
) -> (Value, HashMap<String, NativePluginTerminalNodeSnapshot>) {
    let mut terminal_nodes = HashMap::new();
    for (node_id, node) in &workspace.ssh_nodes {
        let Some(session_id) = node.terminal_ids.first().copied() else {
            continue;
        };
        let Some(pane) = native_plugin_pane_for_session(workspace, session_id) else {
            continue;
        };
        let pane = pane.read(cx);
        terminal_nodes.insert(
            node_id.0.clone(),
            NativePluginTerminalNodeSnapshot {
                buffer: pane.visible_text_snapshot(),
                selection: pane.selected_text_snapshot(),
                current_lines: pane.buffer_line_count(),
            },
        );
    }

    (
        native_plugin_active_terminal_target(workspace, connection_states),
        terminal_nodes,
    )
}

pub(super) fn native_plugin_pane_for_session(
    workspace: &WorkspaceApp,
    session_id: TerminalSessionId,
) -> Option<gpui::Entity<oxideterm_gpui_terminal::TerminalPane>> {
    for tab in &workspace.tabs {
        let Some(root) = tab.root_pane.as_ref() else {
            continue;
        };
        let mut pane_ids = Vec::new();
        root.collect_pane_ids(&mut pane_ids);
        for pane_id in pane_ids {
            if root.session_id_for_pane(pane_id) == Some(session_id) {
                return workspace.panes.get(&pane_id).cloned();
            }
        }
    }
    None
}

pub(super) fn native_plugin_active_terminal_target(
    workspace: &WorkspaceApp,
    connection_states: &HashMap<String, Value>,
) -> Value {
    let Some(session_id) = workspace.active_terminal_session_id() else {
        return Value::Null;
    };
    let terminal_type = workspace
        .active_tab()
        .map(|tab| {
            if tab.kind == TabKind::LocalTerminal {
                "local_terminal"
            } else {
                "terminal"
            }
        })
        .unwrap_or("terminal");

    if terminal_type == "local_terminal" {
        return json!({
            "sessionId": session_id.0.to_string(),
            "terminalType": "local_terminal",
            "nodeId": null,
            "connectionId": null,
            "connectionState": "active",
            "label": session_id.0.to_string(),
        });
    }

    let node_id = workspace.terminal_ssh_nodes.get(&session_id).cloned();
    let connection_id = node_id
        .as_ref()
        .and_then(|node_id| workspace.node_runtime_store.connection_id_for_node(node_id));
    let connection_state = connection_id
        .as_ref()
        .and_then(|connection_id| connection_states.get(connection_id))
        .map(native_plugin_terminal_state_label)
        .unwrap_or(Value::Null);
    let label = node_id
        .as_ref()
        .and_then(|node_id| workspace.ssh_nodes.get(node_id))
        .map(|node| node.title.clone())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| session_id.0.to_string());

    // Tauri derives active terminal target from the pane registry and session
    // tree. Native uses the same visible ids but projects Rust error objects to
    // the plugin-facing `"error"` state string used by pluginContextFactory.
    json!({
        "sessionId": session_id.0.to_string(),
        "terminalType": "terminal",
        "nodeId": node_id.map(|node_id| node_id.0),
        "connectionId": connection_id,
        "connectionState": connection_state,
        "label": label,
    })
}

fn native_plugin_terminal_state_label(state: &Value) -> Value {
    if let Some(state) = state.as_str() {
        return json!(state);
    }
    if state.get("error").is_some() {
        return json!("error");
    }
    Value::Null
}
