// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use gpui::Context;
use serde_json::{Value, json};

use super::{TabKind, TerminalSessionId, WorkspaceApp};
use oxideterm_plugin_host_api::terminal::NativePluginTerminalNodeSnapshot;
use oxideterm_terminal::SerialSessionConfig;

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
    if let Some(config) = workspace.serial_terminal_configs.get(&session_id) {
        return native_plugin_serial_terminal_target(session_id, config);
    }
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

fn native_plugin_serial_terminal_target(
    session_id: TerminalSessionId,
    config: &SerialSessionConfig,
) -> Value {
    // Serial panes are local device transports, not shell-backed terminals.
    // Plugins need explicit metadata so SSH/local-shell tools stay gated.
    json!({
        "sessionId": session_id.0.to_string(),
        "terminalType": "serial",
        "terminalTransport": "serial",
        "nodeId": null,
        "connectionId": null,
        "connectionState": "active",
        "label": format!("Serial {}", config.port_path),
        "transport": {
            "type": "serial",
            "portPath": config.port_path,
            "baudRate": config.baud_rate,
            "dataBits": config.data_bits,
            "stopBits": config.stop_bits,
            "parity": format!("{:?}", config.parity).to_lowercase(),
            "flowControl": format!("{:?}", config.flow_control).to_lowercase(),
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_terminal::{SerialFlowControl, SerialParity};

    #[test]
    fn serial_active_target_exposes_transport_metadata() {
        let target = native_plugin_serial_terminal_target(
            TerminalSessionId(44),
            &SerialSessionConfig {
                port_path: "/dev/cu.usbserial-test".to_string(),
                baud_rate: 115_200,
                data_bits: 8,
                stop_bits: 1,
                parity: SerialParity::None,
                flow_control: SerialFlowControl::Hardware,
            },
        );

        assert_eq!(target["terminalType"], "serial");
        assert_eq!(target["terminalTransport"], "serial");
        assert_eq!(target["label"], "Serial /dev/cu.usbserial-test");
        assert_eq!(target["transport"]["type"], "serial");
        assert_eq!(target["transport"]["portPath"], "/dev/cu.usbserial-test");
        assert_eq!(target["transport"]["baudRate"], 115_200);
        assert_eq!(target["transport"]["dataBits"], 8);
        assert_eq!(target["transport"]["stopBits"], 1);
        assert_eq!(target["transport"]["parity"], "none");
        assert_eq!(target["transport"]["flowControl"], "hardware");
    }
}
