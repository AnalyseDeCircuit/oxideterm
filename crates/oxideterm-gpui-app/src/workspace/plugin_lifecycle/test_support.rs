// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared native plugin lifecycle test fixtures.

use std::{collections::HashMap, time::Duration};

use oxideterm_i18n::I18n;
use oxideterm_notification_center::{EventCategory, EventLogEntry, EventSeverity};
use oxideterm_ssh::{
    ConnectionConsumer, ConnectionInfo, ConnectionState, NodeReadiness, NodeTreeSnapshotNode,
};
use serde_json::Value;

use super::*;

pub(super) fn test_terminal_hook(
    registration_id: &str,
    command: &str,
) -> super::super::plugin_host::NativePluginRuntimeTerminalHookContribution {
    super::super::plugin_host::NativePluginRuntimeTerminalHookContribution {
        plugin_id: "com.example.demo".to_string(),
        plugin_name: "Demo".to_string(),
        registration_id: registration_id.to_string(),
        command: command.to_string(),
    }
}

pub(super) fn test_host_api_snapshot() -> NativePluginHostApiSnapshot {
    NativePluginHostApiSnapshot {
        registry: super::super::plugin_host::NativePluginRegistry::default(),
        i18n: I18n::new(oxideterm_i18n::Locale::ZhCn),
        settings: serde_json::to_value(oxideterm_settings::PersistedSettings::default()).unwrap(),
        locale: "zh-CN".to_string(),
        theme_name: "default".to_string(),
        pool_stats: serde_json::json!({
            "activeConnections": 0,
            "totalSessions": 0,
        }),
        layout: native_plugin_layout_snapshot(false, None, 0),
        connections: Vec::new(),
        connection_states: HashMap::new(),
        node_connection_ids: HashMap::new(),
        session_tree: Vec::new(),
        session_node_states: HashMap::new(),
        event_log_entries: Vec::new(),
        active_terminal_target: Value::Null,
        terminal_nodes: HashMap::new(),
    }
}

pub(super) fn test_host_api_snapshot_with_connections() -> NativePluginHostApiSnapshot {
    let connection = ConnectionInfo {
        connection_id: "conn-1".to_string(),
        key: "redacted-key".to_string(),
        host: "example.test".to_string(),
        port: 22,
        username: "deploy".to_string(),
        parent_connection_id: None,
        state: ConnectionState::Active,
        ref_count: 2,
        keep_alive: true,
        consumers: vec![
            ConnectionConsumer::Sftp("sftp-1".to_string()),
            ConnectionConsumer::Terminal("term-1".to_string()),
        ],
        created_at: std::time::UNIX_EPOCH + Duration::from_secs(1),
        last_active_at: std::time::UNIX_EPOCH + Duration::from_secs(2),
        idle_timeout_secs: Some(1800),
    };
    let connections = vec![native_plugin_connection_snapshot(&connection)];
    let connection_states = HashMap::from([(
        connection.connection_id.clone(),
        native_plugin_connection_state(&connection.state),
    )]);
    let node_connection_ids =
        HashMap::from([("node-1".to_string(), connection.connection_id.clone())]);
    NativePluginHostApiSnapshot {
        connections,
        connection_states,
        node_connection_ids,
        ..test_host_api_snapshot()
    }
}

pub(super) fn test_host_api_snapshot_with_event_log_entries() -> NativePluginHostApiSnapshot {
    let entries = vec![
        EventLogEntry {
            id: 1,
            timestamp: std::time::UNIX_EPOCH + Duration::from_secs(1),
            severity: EventSeverity::Info,
            category: EventCategory::Connection,
            node_id: Some("node-1".to_string()),
            connection_id: Some("conn-1".to_string()),
            title: "Connected".to_string(),
            detail: Some("ready".to_string()),
            source: "connection_status_changed",
        },
        EventLogEntry {
            id: 2,
            timestamp: std::time::UNIX_EPOCH + Duration::from_secs(2),
            severity: EventSeverity::Error,
            category: EventCategory::Node,
            node_id: None,
            connection_id: None,
            title: "Failed".to_string(),
            detail: None,
            source: "node_state_changed",
        },
    ];
    let event_log_entries = native_plugin_event_log_entries(entries.iter());
    NativePluginHostApiSnapshot {
        event_log_entries,
        ..test_host_api_snapshot()
    }
}

pub(super) fn test_host_api_snapshot_with_terminal() -> NativePluginHostApiSnapshot {
    NativePluginHostApiSnapshot {
        active_terminal_target: serde_json::json!({
            "sessionId": "term-1",
            "terminalType": "terminal",
            "nodeId": "node-1",
            "connectionId": "conn-1",
            "connectionState": "active",
            "label": "Production",
        }),
        terminal_nodes: HashMap::from([(
            "node-1".to_string(),
            NativePluginTerminalNodeSnapshot {
                buffer: "alpha\nbeta\nAlpha".to_string(),
                selection: Some("beta".to_string()),
                current_lines: 3,
            },
        )]),
        ..test_host_api_snapshot()
    }
}

pub(super) fn test_host_api_snapshot_with_sessions() -> NativePluginHostApiSnapshot {
    let root_id = oxideterm_ssh::NodeId::new("node-1");
    let child_id = oxideterm_ssh::NodeId::new("node-2");
    let nodes = vec![
        NodeTreeSnapshotNode {
            id: root_id.clone(),
            parent_id: None,
            children_ids: vec![child_id.clone()],
            depth: 0,
            config: oxideterm_ssh::SshConfig {
                host: "example.test".to_string(),
                port: 22,
                username: "deploy".to_string(),
                ..oxideterm_ssh::SshConfig::default()
            },
            origin: oxideterm_ssh::NodeOrigin::Direct,
            state: oxideterm_ssh::NodeState {
                readiness: NodeReadiness::Ready,
                error: None,
                sftp_ready: false,
                sftp_cwd: None,
                ws_endpoint: None,
            },
            connection_id: Some("conn-1".to_string()),
            terminal_session_id: Some("term-legacy".to_string()),
            sftp_session_id: None,
            created_at_ms: 1,
            generation: 1,
        },
        NodeTreeSnapshotNode {
            id: child_id,
            parent_id: Some(root_id),
            children_ids: Vec::new(),
            depth: 1,
            config: oxideterm_ssh::SshConfig {
                host: "child.test".to_string(),
                port: 2222,
                username: "root".to_string(),
                ..oxideterm_ssh::SshConfig::default()
            },
            origin: oxideterm_ssh::NodeOrigin::DrillDown { timestamp: 1 },
            state: oxideterm_ssh::NodeState {
                readiness: NodeReadiness::Connecting,
                error: None,
                sftp_ready: false,
                sftp_cwd: None,
                ws_endpoint: None,
            },
            connection_id: None,
            terminal_session_id: None,
            sftp_session_id: Some("sftp-2".to_string()),
            created_at_ms: 2,
            generation: 1,
        },
    ];
    let titles = HashMap::from([("node-1".to_string(), "Production".to_string())]);
    let terminal_ids = HashMap::from([("node-1".to_string(), vec!["term-1".to_string()])]);
    let session_tree = native_plugin_session_tree_from_nodes(nodes, &titles, &terminal_ids);
    let session_node_states = native_plugin_session_state_map_from_nodes(&session_tree);
    NativePluginHostApiSnapshot {
        session_tree,
        session_node_states,
        ..test_host_api_snapshot()
    }
}

pub(super) fn test_host_api_snapshot_with_declared_setting() -> NativePluginHostApiSnapshot {
    let temp_dir = std::env::temp_dir().join(format!(
        "oxideterm-plugin-lifecycle-settings-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let settings_path = temp_dir.join("settings.json");
    let plugin_dir = super::super::plugin_host::native_plugins_dir(&settings_path).join("demo");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let manifest_path = plugin_dir.join("plugin.json");
    let manifest = serde_json::json!({
        "id": "com.example.demo",
        "name": "Demo",
        "version": "1.0.0",
        "runtime": { "kind": "manifest-only", "entry": "plugin.json" },
        "contributes": {
            "settings": [{
                "id": "mode",
                "type": "select",
                "default": "auto",
                "title": "Mode",
                "options": [{ "label": "Auto", "value": "auto" }]
            }]
        }
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let registry = super::super::plugin_host::NativePluginRegistry::discover(&settings_path);
    let _ = std::fs::remove_dir_all(&temp_dir);
    NativePluginHostApiSnapshot {
        registry,
        ..test_host_api_snapshot()
    }
}

pub(super) fn test_host_api_snapshot_with_declared_api_commands() -> NativePluginHostApiSnapshot {
    let temp_dir = std::env::temp_dir().join(format!(
        "oxideterm-plugin-lifecycle-api-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let settings_path = temp_dir.join("settings.json");
    let plugin_dir = super::super::plugin_host::native_plugins_dir(&settings_path).join("demo");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let manifest_path = plugin_dir.join("plugin.json");
    let mut api_commands = native_plugin_supported_backend_commands().to_vec();
    api_commands.push("custom_declared_command");
    let manifest = serde_json::json!({
        "id": "com.example.demo",
        "name": "Demo",
        "version": "1.0.0",
        "runtime": { "kind": "manifest-only", "entry": "plugin.json" },
        "contributes": {
            "apiCommands": api_commands
        }
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let registry = super::super::plugin_host::NativePluginRegistry::discover(&settings_path);
    let _ = std::fs::remove_dir_all(&temp_dir);
    NativePluginHostApiSnapshot {
        registry,
        ..test_host_api_snapshot()
    }
}

pub(super) fn test_connection_store(name: &str) -> oxideterm_connections::ConnectionStore {
    let path = std::env::temp_dir().join(format!(
        "oxideterm-plugin-lifecycle-{name}-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    oxideterm_connections::ConnectionStore::load(path).unwrap()
}

pub(super) fn test_connection_store_with_agent_connection(
    name: &str,
) -> oxideterm_connections::ConnectionStore {
    let mut store = test_connection_store(name);
    store
        .upsert(oxideterm_connections::SaveConnectionRequest {
            id: Some("conn-1".to_string()),
            name: "Home".to_string(),
            group: None,
            host: "192.168.1.2".to_string(),
            port: 22,
            username: "me".to_string(),
            auth: oxideterm_connections::SavedAuth::Agent,
            proxy_chain: Vec::new(),
            color: None,
            tags: Vec::new(),
            agent_forwarding: false,
            post_connect_command: None,
        })
        .unwrap();
    store
}
