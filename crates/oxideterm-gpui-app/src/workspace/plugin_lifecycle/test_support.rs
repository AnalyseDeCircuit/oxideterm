// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared native plugin lifecycle test fixtures.

use std::collections::HashMap;

use oxideterm_i18n::I18n;
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
        registry: super::super::plugin_host::NativePluginRegistry::default().into(),
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
        saved_connections: Vec::new(),
        connection_states: HashMap::new(),
        node_connection_ids: HashMap::new(),
        session_tree: Vec::new(),
        session_node_states: HashMap::new(),
        event_log_entries: Vec::new(),
        active_terminal_target: Value::Null,
        terminal_nodes: HashMap::new(),
        notification_summary: serde_json::json!({
            "total": 0,
            "unread": 0,
            "unreadCritical": 0,
            "dndEnabled": true,
            "byKind": {
                "agent": 0,
                "connection": 0,
                "health": 0,
                "plugin": 0,
                "security": 0,
                "transfer": 0,
                "update": 0,
            },
            "bySeverity": {
                "critical": 0,
                "error": 0,
                "info": 0,
                "warning": 0,
            },
            "byStatus": { "read": 0, "unread": 0 },
        }),
        notifications: serde_json::json!([]),
        quick_command_metadata: serde_json::json!({
            "categories": [],
            "commands": [],
        }),
        quick_commands: serde_json::json!({ "categories": [], "commands": [] }),
        theme_tokens: serde_json::json!({
            "name": "default",
            "terminal": {},
            "ui": {},
            "metrics": {},
            "radii": {},
            "spacing": {},
            "density": "comfortable",
            "motion": {},
        }),
        available_themes: serde_json::json!({
            "active": "default",
            "builtIn": ["default"],
            "custom": [],
        }),
        cloud_sync_summary: serde_json::json!({
            "enabled": false,
            "backend": "webdav",
            "configured": false,
            "status": "idle",
            "activeAction": null,
            "progress": null,
            "dirty": false,
            "conflict": false,
            "historyCount": 0,
            "lastSuccessAt": null,
        }),
        cloud_sync_history: serde_json::json!([]),
        host_tools_snapshots: serde_json::json!([]),
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
        registry: registry.into(),
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
            notes: None,
            host: "192.168.1.2".to_string(),
            port: 22,
            username: "me".to_string(),
            auth: oxideterm_connections::SavedAuth::Agent,
            proxy_chain: Vec::new(),
            upstream_proxy: oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal,
            proxy_command: None,
            color: None,
            icon_background_color: None,
            icon: None,
            tags: Vec::new(),
            connect_timeout_seconds: oxideterm_connections::DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
            dedicated_new_terminal_connection: false,
            ssh_channel_strategy: oxideterm_connections::SshChannelStrategy::default(),
            x11_forwarding: oxideterm_connections::ConnectionX11ForwardingOptions::default(),
            post_connect_command: None,
            terminal: oxideterm_connections::ConnectionTerminalOptions::default(),
        })
        .unwrap();
    store
}
