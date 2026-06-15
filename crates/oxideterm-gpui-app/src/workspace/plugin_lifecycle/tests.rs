// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Regression coverage for native plugin lifecycle, host calls, and adapters.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use oxideterm_connection_monitor::{ProfilerRegistry, ResourceMetrics};
use oxideterm_connections::{
    LocalSyncMetadata as SavedConnectionsLocalSyncMetadata, SavedConnectionsConflictStrategy,
    SavedConnectionsSyncSnapshot,
    oxide_file::{
        ImportResultEnvelope, OxideExportOptions, OxideFile, export_connections_to_oxide,
    },
};
use oxideterm_forwarding::{ForwardRule, ForwardStatus, ForwardType, ForwardingRegistry};
use oxideterm_gpui_ide::{IdePluginFileSnapshot, IdePluginSnapshot};
use oxideterm_sftp::{
    BackgroundTransferDirection, BackgroundTransferSnapshot, BackgroundTransferState,
    SftpTransferManager,
};
use oxideterm_ssh::{NodeReadiness, NodeRouter};
use serde_json::Map;

use super::test_support::*;
use super::*;
use crate::workspace::TerminalInputInterceptorResult;

#[test]
fn native_plugin_permissions_cover_implemented_host_api_namespaces() {
    let permissions = native_process_plugin_permissions();
    assert!(
        permissions
            .capabilities
            .contains(&NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ.to_string())
    );
    assert!(
        permissions
            .capabilities
            .contains(&NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE.to_string())
    );
    assert!(
        permissions
            .capabilities
            .contains(&NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD.to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"ui.showToast".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"ui.showConfirm".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"ui.showProgress".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"ui.showNotification".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"storage.set".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"storage.remove".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"storage.get".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"app.getVersion".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"app.getSettings".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"app.refreshAfterExternalSync".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"api.invoke".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"connections.getAll".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"connections.getByNode".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"sessions.getTree".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"sessions.getNodeState".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"eventLog.getEntries".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"terminal.getActiveTarget".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"terminal.getBufferSize".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"terminal.writeToActive".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"terminal.clearBuffer".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"terminal.openTelnet".to_string())
    );
    for api in [
        "sftp.listDir",
        "sftp.stat",
        "sftp.readFile",
        "sftp.writeFile",
        "sftp.mkdir",
        "sftp.delete",
        "sftp.rename",
        "forward.list",
        "forward.listSavedForwards",
        "forward.onSavedForwardsChange",
        "forward.exportSavedForwardsSnapshot",
        "forward.applySavedForwardsSnapshot",
        "forward.create",
        "forward.stop",
        "forward.stopAll",
        "forward.getStats",
        "sync.listSavedConnections",
        "sync.refreshSavedConnections",
        "sync.exportSavedConnectionsSnapshot",
        "sync.applySavedConnectionsSnapshot",
        "sync.getLocalSyncMetadata",
        "sync.preflightExport",
        "sync.exportOxide",
        "sync.validateOxide",
        "sync.previewImport",
        "sync.importOxide",
        "transfers.getAll",
        "transfers.getByNode",
        "transfers.onProgress",
        "transfers.onComplete",
        "transfers.onError",
        "profiler.getMetrics",
        "profiler.getHistory",
        "profiler.isRunning",
        "profiler.onMetrics",
        "ide.isOpen",
        "ide.getProject",
        "ide.getOpenFiles",
        "ide.getActiveFile",
        "ide.onFileOpen",
        "ide.onFileClose",
        "ide.onActiveFileChange",
        "ai.getConversations",
        "ai.getMessages",
        "ai.getActiveProvider",
        "ai.getAvailableModels",
        "ai.onMessage",
    ] {
        assert!(permissions.allowed_host_apis.contains(&api.to_string()));
    }
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"events.emit".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"i18n.t".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"settings.get".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"settings.set".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"settings.exportSyncableSettings".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"settings.applySyncableSettings".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"ui.getLayout".to_string())
    );
    for api in [
        "ui.registerTabView",
        "ui.registerSidebarPanel",
        "ui.openTab",
    ] {
        assert!(permissions.allowed_host_apis.contains(&api.to_string()));
    }
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"secrets.get".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"secrets.getMany".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"secrets.set".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"secrets.has".to_string())
    );
    assert!(
        permissions
            .allowed_host_apis
            .contains(&"secrets.delete".to_string())
    );
}

#[test]
fn sync_host_call_returns_saved_connection_snapshots_and_metadata() {
    let connection_store = test_connection_store("sync-readonly");
    let saved_connections = serde_json::json!([
        {
            "id": "conn-1",
            "name": "Production",
            "host": "example.test"
        }
    ]);
    let saved_connections_snapshot = SavedConnectionsSyncSnapshot {
        revision: "rev-connections".to_string(),
        exported_at: "2026-05-25T00:00:00Z".to_string(),
        records: Vec::new(),
    };
    let local_metadata = SavedConnectionsLocalSyncMetadata {
        saved_connections_revision: "rev-connections".to_string(),
        saved_connections_updated_at: "2026-05-25T00:00:00Z".to_string(),
    };
    let plugin_settings = Vec::new();
    let plugin_settings_revisions = Map::new();

    let list_response = native_plugin_sync_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sync-list-1".to_string(),
            namespace: "sync".to_string(),
            method: "listSavedConnections".to_string(),
            args: serde_json::json!({}),
        },
        &connection_store,
        &saved_connections,
        Ok(&saved_connections_snapshot),
        Ok(&local_metadata),
        Some("rev-forwards"),
        &plugin_settings,
        &plugin_settings_revisions,
        None,
    );
    assert_eq!(
        list_response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: saved_connections.clone()
        }
    );

    let metadata_response = native_plugin_sync_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sync-meta-1".to_string(),
            namespace: "sync".to_string(),
            method: "getLocalSyncMetadata".to_string(),
            args: serde_json::json!({}),
        },
        &connection_store,
        &saved_connections,
        Ok(&saved_connections_snapshot),
        Ok(&local_metadata),
        Some("rev-forwards"),
        &plugin_settings,
        &plugin_settings_revisions,
        None,
    );
    assert_eq!(
        metadata_response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "savedConnectionsRevision": "rev-connections",
                "savedConnectionsUpdatedAt": "2026-05-25T00:00:00Z",
                "savedForwardsRevision": "rev-forwards",
                "pluginSettingsRevisions": {}
            })
        }
    );
}

#[test]
fn sync_apply_saved_connections_requires_workspace_bridge() {
    let connection_store = test_connection_store("sync-pending");
    let saved_connections = serde_json::json!([]);
    let saved_connections_snapshot = SavedConnectionsSyncSnapshot {
        revision: "rev-connections".to_string(),
        exported_at: "2026-05-25T00:00:00Z".to_string(),
        records: Vec::new(),
    };
    let local_metadata = SavedConnectionsLocalSyncMetadata {
        saved_connections_revision: "rev-connections".to_string(),
        saved_connections_updated_at: "2026-05-25T00:00:00Z".to_string(),
    };
    let plugin_settings = Vec::new();
    let plugin_settings_revisions = Map::new();

    let response = native_plugin_sync_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sync-apply-1".to_string(),
            namespace: "sync".to_string(),
            method: "applySavedConnectionsSnapshot".to_string(),
            args: serde_json::json!({}),
        },
        &connection_store,
        &saved_connections,
        Ok(&saved_connections_snapshot),
        Ok(&local_metadata),
        None,
        &plugin_settings,
        &plugin_settings_revisions,
        None,
    );

    assert!(matches!(
        response.result,
        plugin_runtime::PluginResponseResult::Error {
            error: plugin_runtime::PluginError {
                ref code,
                recoverable: true,
                ..
            }
        } if code == "plugin_sync_apply_unavailable"
    ));
}

#[test]
fn sync_apply_saved_connections_args_parse_snapshot_and_strategy() {
    let snapshot = SavedConnectionsSyncSnapshot {
        revision: "rev-connections".to_string(),
        exported_at: "2026-05-25T00:00:00Z".to_string(),
        records: Vec::new(),
    };

    let (parsed_snapshot, strategy) =
        native_plugin_sync_apply_saved_connections_args(&serde_json::json!({
            "snapshot": snapshot,
            "conflictStrategy": "merge"
        }))
        .unwrap();

    assert_eq!(parsed_snapshot.revision, "rev-connections");
    assert_eq!(strategy, SavedConnectionsConflictStrategy::Merge);
}

#[test]
fn sync_oxide_host_calls_export_validate_and_preview_without_workspace_mutation() {
    let connection_store = test_connection_store_with_agent_connection("sync-oxide");
    let saved_connections = serde_json::json!([]);
    let saved_connections_snapshot = SavedConnectionsSyncSnapshot {
        revision: "rev-connections".to_string(),
        exported_at: "2026-05-25T00:00:00Z".to_string(),
        records: Vec::new(),
    };
    let local_metadata = SavedConnectionsLocalSyncMetadata {
        saved_connections_revision: "rev-connections".to_string(),
        saved_connections_updated_at: "2026-05-25T00:00:00Z".to_string(),
    };
    let plugin_settings = Vec::new();
    let plugin_settings_revisions = Map::new();

    let preflight = native_plugin_sync_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sync-preflight-1".to_string(),
            namespace: "sync".to_string(),
            method: "preflightExport".to_string(),
            args: serde_json::json!({ "connectionIds": null, "embedKeys": false }),
        },
        &connection_store,
        &saved_connections,
        Ok(&saved_connections_snapshot),
        Ok(&local_metadata),
        None,
        &plugin_settings,
        &plugin_settings_revisions,
        None,
    );
    assert_eq!(
        preflight.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "totalConnections": 1,
                "missingKeys": [],
                "connectionsWithKeys": 0,
                "connectionsWithPasswords": 0,
                "connectionsWithAgent": 1,
                "keyPassphraseCount": 0,
                "managedKeyCount": 0,
                "managedKeyPassphraseCount": 0,
                "blockedManagedKeyConnections": [],
                "totalKeyBytes": 0,
                "canExport": true,
                "portableSecretCount": 0,
            })
        }
    );

    let (progress_tx, progress_rx) = mpsc::channel();
    let export_response = native_plugin_sync_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sync-export-1".to_string(),
            namespace: "sync".to_string(),
            method: "exportOxide".to_string(),
            args: serde_json::json!({
                "connectionIds": ["conn-1"],
                "password": "StrongPass!123",
                "description": "Plugin export",
                "embedKeys": false,
                "progressRegistrationId": "sync-progress-1"
            }),
        },
        &connection_store,
        &saved_connections,
        Ok(&saved_connections_snapshot),
        Ok(&local_metadata),
        None,
        &plugin_settings,
        &plugin_settings_revisions,
        Some(&progress_tx),
    );
    let plugin_runtime::PluginResponseResult::Ok { value: exported } = export_response.result
    else {
        panic!("expected sync.exportOxide to return .oxide bytes");
    };
    let progress_messages = progress_rx.try_iter().collect::<Vec<_>>();
    assert!(progress_messages.iter().any(|request| matches!(
        &request.action,
        NativePluginSyncAction::ReportProgress {
            plugin_id,
            registration_id,
            ..
        } if plugin_id == "com.example.demo" && registration_id == "sync-progress-1"
    )));
    let exported_bytes = native_plugin_u8_array(exported.as_array().unwrap()).unwrap();

    let validate_response = native_plugin_sync_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sync-validate-1".to_string(),
            namespace: "sync".to_string(),
            method: "validateOxide".to_string(),
            args: serde_json::json!({ "fileData": exported_bytes.clone() }),
        },
        &connection_store,
        &saved_connections,
        Ok(&saved_connections_snapshot),
        Ok(&local_metadata),
        None,
        &plugin_settings,
        &plugin_settings_revisions,
        None,
    );
    let plugin_runtime::PluginResponseResult::Ok { value: metadata } = validate_response.result
    else {
        panic!("expected sync.validateOxide to return metadata");
    };
    assert_eq!(metadata["description"], "Plugin export");
    assert_eq!(metadata["connection_names"], serde_json::json!(["Home"]));

    let preview_response = native_plugin_sync_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sync-preview-1".to_string(),
            namespace: "sync".to_string(),
            method: "previewImport".to_string(),
            args: serde_json::json!({
                "fileData": exported_bytes,
                "password": "StrongPass!123",
                "conflictStrategy": "skip"
            }),
        },
        &connection_store,
        &saved_connections,
        Ok(&saved_connections_snapshot),
        Ok(&local_metadata),
        None,
        &plugin_settings,
        &plugin_settings_revisions,
        None,
    );
    let plugin_runtime::PluginResponseResult::Ok { value: preview } = preview_response.result
    else {
        panic!("expected sync.previewImport to return an import preview");
    };
    assert_eq!(preview["totalConnections"], 1);
    assert_eq!(preview["willSkip"], serde_json::json!(["Home"]));
}

#[test]
fn sync_import_oxide_args_and_core_import_match_tauri_defaults() {
    let source_store = test_connection_store_with_agent_connection("sync-import-source");
    let bytes = export_connections_to_oxide(
        &source_store,
        &["conn-1".to_string()],
        "StrongPass!123",
        OxideExportOptions::default(),
    )
    .unwrap();
    let (parsed_bytes, password, options) =
        native_plugin_sync_import_oxide_args(&serde_json::json!({
            "fileData": bytes,
            "password": "StrongPass!123",
            "conflictStrategy": "rename",
            "selectedPluginIds": []
        }))
        .unwrap();
    assert!(options.oxide_options.import_forwards);
    assert!(!options.oxide_options.import_portable_secrets);
    assert!(options.import_app_settings);
    assert!(options.import_plugin_settings);
    assert_eq!(options.selected_plugin_ids, Some(HashSet::new()));

    let mut target_store = test_connection_store("sync-import-target");
    let envelope = native_plugin_apply_oxide_import_core(
        &mut target_store,
        &parsed_bytes,
        &password,
        options.oxide_options,
    )
    .unwrap();
    assert_eq!(envelope.imported, 1);
    assert!(target_store.get("conn-1").is_none());
    assert!(target_store.connections().iter().any(|connection| {
        connection.name == "Home"
            && matches!(connection.auth, oxideterm_connections::SavedAuth::Agent)
    }));
}

#[test]
fn sync_import_result_omits_consumed_sidecar_payloads() {
    let envelope = ImportResultEnvelope {
        imported: 1,
        app_settings_json: Some("{}".to_string()),
        quick_commands_json: Some("[]".to_string()),
        plugin_settings: vec![oxideterm_connections::oxide_file::EncryptedPluginSetting {
            storage_key: "oxide-plugin-com.example.demo-setting-mode".to_string(),
            serialized_value: "\"auto\"".to_string(),
        }],
        ..ImportResultEnvelope::default()
    };

    let value = native_plugin_sync_import_result_value(
        &envelope,
        true,
        false,
        2,
        false,
        Vec::new(),
        1,
        false,
    );

    assert_eq!(value["imported"], 1);
    assert_eq!(value["importedAppSettings"], true);
    assert_eq!(value["importedQuickCommands"], 2);
    assert_eq!(value["importedPluginSettings"], 1);
    assert!(value.get("appSettingsJson").is_none());
    assert!(value.get("quickCommandsJson").is_none());
    assert!(value.get("pluginSettings").is_none());
}

#[test]
fn sync_plugin_settings_export_filters_selected_plugins_and_revisions() {
    let connection_store = test_connection_store("sync-plugin-settings");
    let plugin_settings = vec![
        oxideterm_connections::oxide_file::EncryptedPluginSetting {
            storage_key: "oxide-plugin-com.example.demo-setting-mode".to_string(),
            serialized_value: "\"auto\"".to_string(),
        },
        oxideterm_connections::oxide_file::EncryptedPluginSetting {
            storage_key: "oxide-plugin-com.example.other-setting-mode".to_string(),
            serialized_value: "\"manual\"".to_string(),
        },
    ];

    let response = native_plugin_sync_export_oxide_response(
        "com.example.demo",
        "sync-plugin-export-1".to_string(),
        &connection_store,
        &plugin_settings,
        &serde_json::json!({
            "connectionIds": [],
            "password": "StrongPass!123",
            "includePluginSettings": true,
            "selectedPluginIds": ["com.example.demo"]
        }),
        None,
    );
    let plugin_runtime::PluginResponseResult::Ok { value } = response.result else {
        panic!("expected sync.exportOxide to include selected plugin settings");
    };
    let bytes = native_plugin_u8_array(value.as_array().unwrap()).unwrap();
    let file = OxideFile::from_bytes(&bytes).unwrap();
    assert_eq!(file.metadata.plugin_settings_count, Some(1));

    let revisions = native_plugin_settings_revision_map(&plugin_settings);
    assert!(
        revisions
            .get("com.example.demo")
            .and_then(Value::as_str)
            .is_some_and(|revision| revision.starts_with("fnv1a-"))
    );
    assert!(revisions.contains_key("com.example.other"));
}

#[test]
fn transfers_host_calls_return_tauri_snapshot_shape_and_filter_by_node() {
    let manager = Arc::new(SftpTransferManager::new());
    let first_transfer = BackgroundTransferSnapshot::new(
        "tx-1".to_string(),
        "node-a".to_string(),
        "Upload logs".to_string(),
        "/local/logs".to_string(),
        "/remote/logs".to_string(),
        BackgroundTransferDirection::Upload,
        oxideterm_sftp::BackgroundTransferKind::Directory,
        oxideterm_sftp::TransferStrategy::DirectoryRecursive,
        2048,
        512,
    );
    let second_transfer = BackgroundTransferSnapshot::new(
        "tx-2".to_string(),
        "node-b".to_string(),
        "Download report".to_string(),
        "/local/report.txt".to_string(),
        "/remote/report.txt".to_string(),
        BackgroundTransferDirection::Download,
        oxideterm_sftp::BackgroundTransferKind::File,
        oxideterm_sftp::TransferStrategy::File,
        64,
        64,
    );
    manager.register_background_transfer(first_transfer);
    manager.register_background_transfer(second_transfer);
    manager.mark_background_transfer_active("tx-1");
    manager.finish_background_transfer("tx-2", BackgroundTransferState::Completed, None, None);

    let all_response = native_plugin_transfers_response(
        plugin_runtime::PluginHostCall {
            request_id: "transfers-all-1".to_string(),
            namespace: "transfers".to_string(),
            method: "getAll".to_string(),
            args: serde_json::json!({}),
        },
        &manager,
    );
    let plugin_runtime::PluginResponseResult::Ok { value: all_value } = all_response.result else {
        panic!("expected transfers.getAll to return snapshots");
    };
    let all = all_value.as_array().unwrap();
    assert_eq!(all.len(), 2);
    let first = all
        .iter()
        .find(|transfer| transfer["id"] == "tx-1")
        .unwrap();
    assert_eq!(first["nodeId"], "node-a");
    assert_eq!(first["direction"], "upload");
    assert_eq!(first["state"], "active");
    assert!(first.get("strategy").is_none());

    let by_node_response = native_plugin_transfers_response(
        plugin_runtime::PluginHostCall {
            request_id: "transfers-node-1".to_string(),
            namespace: "transfers".to_string(),
            method: "getByNode".to_string(),
            args: serde_json::json!({ "nodeId": "node-b" }),
        },
        &manager,
    );
    assert_eq!(
        by_node_response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!([
                {
                    "id": "tx-2",
                    "nodeId": "node-b",
                    "name": "Download report",
                    "localPath": "/local/report.txt",
                    "remotePath": "/remote/report.txt",
                    "direction": "download",
                    "size": 64,
                    "transferred": 64,
                    "state": "completed",
                    "error": null,
                    "startTime": all
                        .iter()
                        .find(|transfer| transfer["id"] == "tx-2")
                        .unwrap()["startTime"],
                    "endTime": all
                        .iter()
                        .find(|transfer| transfer["id"] == "tx-2")
                        .unwrap()["endTime"],
                }
            ])
        }
    );
}

#[test]
fn transfer_state_helpers_detect_complete_and_error_transitions() {
    let previous = serde_json::json!([
        { "id": "tx-1", "state": "active" },
        { "id": "tx-2", "state": "pending" }
    ]);
    let next = serde_json::json!([
        { "id": "tx-1", "state": "completed" },
        { "id": "tx-2", "state": "error" }
    ]);
    let previous_states = native_plugin_transfer_state_map(&previous);
    let next_states = native_plugin_transfer_state_map(&next);

    let completed = native_plugin_transfer_transition_values(
        &next,
        &previous_states,
        &next_states,
        BackgroundTransferState::Completed,
    );
    let errored = native_plugin_transfer_transition_values(
        &next,
        &previous_states,
        &next_states,
        BackgroundTransferState::Error,
    );

    assert_eq!(completed[0]["id"], "tx-1");
    assert_eq!(errored[0]["id"], "tx-2");
}

#[test]
fn profiler_host_calls_map_node_ids_to_tauri_metrics_shape() {
    let registry = ProfilerRegistry::new();
    registry.start("conn-1");
    registry.record_metrics(oxideterm_connection_monitor::ProfilerUpdate {
        connection_id: "conn-1".to_string(),
        metrics: ResourceMetrics {
            timestamp_ms: 42,
            cpu_percent: Some(12.5),
            memory_used: Some(1024),
            memory_total: Some(2048),
            memory_percent: Some(50.0),
            memory_buffers: None,
            memory_cached: None,
            swap_used: None,
            swap_total: None,
            swap_percent: None,
            disk_used: Some(10),
            disk_total: Some(20),
            disk_percent: Some(50.0),
            load_avg_1: Some(0.1),
            load_avg_5: Some(0.2),
            load_avg_15: Some(0.3),
            cpu_cores: Some(8),
            cpu_per_core: Vec::new(),
            disks: Vec::new(),
            net_rx_bytes_per_sec: Some(100),
            net_tx_bytes_per_sec: Some(200),
            net_interfaces: Vec::new(),
            gpus: Vec::new(),
            top_processes: Vec::new(),
            docker: Default::default(),
            ssh_rtt_ms: Some(9),
            source: oxideterm_connection_monitor::MetricsSource::Full,
        },
    });
    let node_connection_ids = HashMap::from([("node-1".to_string(), "conn-1".to_string())]);

    let response = native_plugin_profiler_response(
        plugin_runtime::PluginHostCall {
            request_id: "profiler-metrics-1".to_string(),
            namespace: "profiler".to_string(),
            method: "getMetrics".to_string(),
            args: serde_json::json!({ "nodeId": "node-1" }),
        },
        &registry,
        &node_connection_ids,
    );
    assert_eq!(
        response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "timestampMs": 42,
                "cpuPercent": 12.5,
                "memoryUsed": 1024,
                "memoryTotal": 2048,
                "memoryPercent": 50.0,
                "loadAvg1": 0.1,
                "loadAvg5": 0.2,
                "loadAvg15": 0.3,
                "cpuCores": 8,
                "netRxBytesPerSec": 100,
                "netTxBytesPerSec": 200,
                "sshRttMs": 9,
            })
        }
    );

    let running_response = native_plugin_profiler_response(
        plugin_runtime::PluginHostCall {
            request_id: "profiler-running-1".to_string(),
            namespace: "profiler".to_string(),
            method: "isRunning".to_string(),
            args: serde_json::json!({ "nodeId": "node-1" }),
        },
        &registry,
        &node_connection_ids,
    );
    assert_eq!(
        running_response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!(true)
        }
    );
}

#[test]
fn profiler_history_limits_and_subscription_filters_are_node_scoped() {
    let registry = ProfilerRegistry::new();
    registry.start("conn-1");
    for timestamp_ms in [1, 2, 3] {
        registry.record_metrics(oxideterm_connection_monitor::ProfilerUpdate {
            connection_id: "conn-1".to_string(),
            metrics: ResourceMetrics::empty(
                timestamp_ms,
                oxideterm_connection_monitor::MetricsSource::Full,
            ),
        });
    }
    let node_connection_ids = HashMap::from([("node-1".to_string(), "conn-1".to_string())]);

    let history_response = native_plugin_profiler_response(
        plugin_runtime::PluginHostCall {
            request_id: "profiler-history-1".to_string(),
            namespace: "profiler".to_string(),
            method: "getHistory".to_string(),
            args: serde_json::json!({ "nodeId": "node-1", "maxPoints": 2 }),
        },
        &registry,
        &node_connection_ids,
    );
    let plugin_runtime::PluginResponseResult::Ok { value } = history_response.result else {
        panic!("expected profiler.getHistory to return a history array");
    };
    let history = value.as_array().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["timestampMs"], 2);
    assert!(native_plugin_subscription_allows_node(
        Some(&serde_json::json!({ "nodeId": "node-1" })),
        "node-1"
    ));
    assert!(!native_plugin_subscription_allows_node(
        Some(&serde_json::json!({ "nodeId": "node-2" })),
        "node-1"
    ));
}

#[test]
fn ide_host_calls_return_project_open_files_and_active_file() {
    let ide_snapshot = native_plugin_ide_snapshot_value(&IdePluginSnapshot {
        project: oxideterm_gpui_ide::IdePluginProjectSnapshot {
            node_id: "node-1".to_string(),
            root_path: "/srv/app".to_string(),
            name: "app".to_string(),
            is_git_repo: true,
            git_branch: Some("main".to_string()),
        },
        open_files: vec![
            IdePluginFileSnapshot {
                path: "/srv/app/src/main.rs".to_string(),
                name: "main.rs".to_string(),
                language: "Rust".to_string(),
                is_dirty: false,
                is_active: true,
                is_pinned: false,
            },
            IdePluginFileSnapshot {
                path: "/srv/app/README.md".to_string(),
                name: "README.md".to_string(),
                language: "Markdown".to_string(),
                is_dirty: true,
                is_active: false,
                is_pinned: true,
            },
        ],
        active_file: Some(IdePluginFileSnapshot {
            path: "/srv/app/src/main.rs".to_string(),
            name: "main.rs".to_string(),
            language: "Rust".to_string(),
            is_dirty: false,
            is_active: true,
            is_pinned: false,
        }),
    });

    let project_response = native_plugin_ide_response(
        plugin_runtime::PluginHostCall {
            request_id: "ide-project-1".to_string(),
            namespace: "ide".to_string(),
            method: "getProject".to_string(),
            args: serde_json::json!({}),
        },
        &ide_snapshot,
    );
    assert_eq!(
        project_response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "nodeId": "node-1",
                "rootPath": "/srv/app",
                "name": "app",
                "isGitRepo": true,
                "gitBranch": "main",
            })
        }
    );

    let active_response = native_plugin_ide_response(
        plugin_runtime::PluginHostCall {
            request_id: "ide-active-1".to_string(),
            namespace: "ide".to_string(),
            method: "getActiveFile".to_string(),
            args: serde_json::json!({}),
        },
        &ide_snapshot,
    );
    assert_eq!(
        active_response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "path": "/srv/app/src/main.rs",
                "name": "main.rs",
                "language": "Rust",
                "isDirty": false,
                "isActive": true,
                "isPinned": false,
            })
        }
    );
}

#[test]
fn ide_file_maps_detect_open_close_and_active_changes() {
    let previous = serde_json::json!({
        "openFiles": [
            { "path": "/a.rs", "name": "a.rs", "isActive": true }
        ],
        "activeFile": { "path": "/a.rs" }
    });
    let next = serde_json::json!({
        "openFiles": [
            { "path": "/b.rs", "name": "b.rs", "isActive": true }
        ],
        "activeFile": { "path": "/b.rs" }
    });

    let previous_files = native_plugin_ide_file_map(&previous);
    let next_files = native_plugin_ide_file_map(&next);
    assert!(previous_files.contains_key("/a.rs"));
    assert!(!next_files.contains_key("/a.rs"));
    assert!(next_files.contains_key("/b.rs"));
    assert_ne!(
        native_plugin_ide_active_file_path(&previous),
        native_plugin_ide_active_file_path(&next)
    );
}

#[test]
fn ai_host_calls_return_sanitized_messages_and_provider_info() {
    let chat = oxideterm_ai::AiChatState {
        conversations: vec![oxideterm_ai::AiConversation {
            id: "conversation-1".to_string(),
            title: "Deploy help".to_string(),
            messages: vec![
                oxideterm_ai::AiChatMessage {
                    id: "message-user-1".to_string(),
                    role: oxideterm_ai::AiChatRole::User,
                    content: "Authorization: Bearer secret-token-value".to_string(),
                    timestamp_ms: 10,
                    model: None,
                    context: None,
                    thinking_content: None,
                    is_streaming: false,
                    metadata: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                    suggestions: Vec::new(),
                },
                oxideterm_ai::AiChatMessage {
                    id: "message-tool-1".to_string(),
                    role: oxideterm_ai::AiChatRole::Tool,
                    content: "{\"token\":\"tool-secret-value\"}".to_string(),
                    timestamp_ms: 11,
                    model: None,
                    context: None,
                    thinking_content: None,
                    is_streaming: false,
                    metadata: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                    suggestions: Vec::new(),
                },
            ],
            created_at_ms: 1,
            updated_at_ms: 12,
            origin: "sidebar".to_string(),
            profile_id: None,
            message_count: 2,
            session_id: None,
            session_metadata: None,
            messages_loaded: true,
        }],
        active_conversation_id: Some("conversation-1".to_string()),
    };
    let mut model_context_windows = Map::new();
    model_context_windows.insert(
        "provider-1".to_string(),
        serde_json::json!({
            "gpt-4o-mini": { "contextWindow": 128000 },
            "gpt-4.1": { "contextWindow": 1048576 }
        }),
    );
    let providers = vec![serde_json::json!({
        "id": "provider-1",
        "type": "openai",
        "name": "OpenAI",
        "models": ["gpt-4o-mini"],
        "defaultModel": "gpt-4o-mini"
    })];
    let snapshot = native_plugin_ai_snapshot_value(
        &chat,
        &providers,
        Some("provider-1"),
        &model_context_windows,
    );

    let conversations_response = native_plugin_ai_response(
        plugin_runtime::PluginHostCall {
            request_id: "ai-conversations-1".to_string(),
            namespace: "ai".to_string(),
            method: "getConversations".to_string(),
            args: serde_json::json!({}),
        },
        &snapshot,
    );
    assert_eq!(
        conversations_response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!([{
                "id": "conversation-1",
                "title": "Deploy help",
                "messageCount": 1,
                "createdAt": 1,
                "updatedAt": 12,
            }])
        }
    );

    let messages_response = native_plugin_ai_response(
        plugin_runtime::PluginHostCall {
            request_id: "ai-messages-1".to_string(),
            namespace: "ai".to_string(),
            method: "getMessages".to_string(),
            args: serde_json::json!({ "conversationId": "conversation-1" }),
        },
        &snapshot,
    );
    let plugin_runtime::PluginResponseResult::Ok { value: messages } = messages_response.result
    else {
        panic!("expected ai.getMessages to return sanitized message snapshots");
    };
    assert_eq!(messages.as_array().unwrap().len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "Authorization: Bearer [REDACTED]");

    let active_provider_response = native_plugin_ai_response(
        plugin_runtime::PluginHostCall {
            request_id: "ai-provider-1".to_string(),
            namespace: "ai".to_string(),
            method: "getActiveProvider".to_string(),
            args: serde_json::json!({}),
        },
        &snapshot,
    );
    assert_eq!(
        active_provider_response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "type": "openai",
                "displayName": "OpenAI"
            })
        }
    );

    let models_response = native_plugin_ai_response(
        plugin_runtime::PluginHostCall {
            request_id: "ai-models-1".to_string(),
            namespace: "ai".to_string(),
            method: "getAvailableModels".to_string(),
            args: serde_json::json!({}),
        },
        &snapshot,
    );
    let plugin_runtime::PluginResponseResult::Ok { value: models } = models_response.result else {
        panic!("expected ai.getAvailableModels to return configured model keys");
    };
    assert!(models.as_array().unwrap().contains(&json!("gpt-4o-mini")));
    assert!(models.as_array().unwrap().contains(&json!("gpt-4.1")));
}

#[test]
fn ai_new_message_events_omit_message_content() {
    let snapshot = serde_json::json!({
        "conversations": [
            {
                "id": "conversation-1",
                "title": "Deploy help",
                "messageCount": 2,
                "createdAt": 1,
                "updatedAt": 20
            }
        ],
        "messagesByConversation": {
            "conversation-1": [
                {
                    "id": "message-user-1",
                    "role": "user",
                    "content": "safe prompt",
                    "timestamp": 10
                },
                {
                    "id": "message-assistant-1",
                    "role": "assistant",
                    "content": "answer with sanitized details",
                    "timestamp": 20
                }
            ]
        }
    });
    let previous_counts = HashMap::from([("conversation-1".to_string(), 1)]);

    let events = native_plugin_ai_new_message_events(&snapshot, &previous_counts);

    assert_eq!(
        events,
        vec![serde_json::json!({
            "conversationId": "conversation-1",
            "messageId": "message-assistant-1",
            "role": "assistant"
        })]
    );
    // Tauri's onMessage payload is metadata-only; native keeps content out
    // of the event and requires plugins to call getMessages for sanitized text.
    assert!(events[0].get("content").is_none());
}

#[test]
fn sftp_host_call_args_reject_missing_or_invalid_paths() {
    let missing_node = serde_json::json!({ "path": "/tmp/file" });
    assert!(native_plugin_sftp_node_id_arg(&missing_node).is_err());

    let empty_path = serde_json::json!({ "nodeId": "node-1", "path": "" });
    assert!(native_plugin_sftp_path_arg(&empty_path, "path").is_err());

    let nul_path = serde_json::json!({ "nodeId": "node-1", "path": "/tmp/a\0b" });
    assert!(native_plugin_sftp_path_arg(&nul_path, "path").is_err());
}

#[test]
fn sftp_host_calls_require_matching_filesystem_capability() {
    let read_only = plugin_runtime::PluginPermissionSet {
        capabilities: vec![NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ.to_string()],
        allowed_host_apis: Vec::new(),
    };
    assert!(native_plugin_sftp_check_capability("listDir", &read_only).is_ok());
    assert!(native_plugin_sftp_check_capability("readFile", &read_only).is_ok());
    assert!(native_plugin_sftp_check_capability("writeFile", &read_only).is_err());
    assert!(native_plugin_sftp_check_capability("delete", &read_only).is_err());

    let write_enabled = plugin_runtime::PluginPermissionSet {
        capabilities: vec![NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE.to_string()],
        allowed_host_apis: Vec::new(),
    };
    assert!(native_plugin_sftp_check_capability("rename", &write_enabled).is_ok());
}

#[test]
fn forward_host_calls_require_network_forward_capability() {
    let denied = plugin_runtime::PluginPermissionSet::default();
    assert!(native_plugin_forward_check_capability("create", &denied).is_err());
    assert!(native_plugin_forward_check_capability("list", &denied).is_err());

    let allowed = plugin_runtime::PluginPermissionSet {
        capabilities: vec![NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD.to_string()],
        allowed_host_apis: Vec::new(),
    };
    assert!(native_plugin_forward_check_capability("create", &allowed).is_ok());
    assert!(
        native_plugin_forward_check_capability("exportSavedForwardsSnapshot", &allowed).is_ok()
    );
}

#[test]
fn forward_create_request_accepts_tauri_camel_case_shape() {
    let request = native_plugin_forward_create_request(&serde_json::json!({
        "sessionId": "node:abc",
        "forwardType": "local",
        "bindAddress": "127.0.0.1",
        "bindPort": 8080,
        "targetHost": "localhost",
        "targetPort": 80,
        "description": "plugin forward",
    }))
    .unwrap();

    assert_eq!(request.session_id, "node:abc");
    assert_eq!(request.forward_type, ForwardType::Local);
    assert_eq!(request.bind_port, 8080);
    assert_eq!(request.target_port, 80);
}

#[test]
fn forward_rule_snapshot_matches_plugin_forward_rule_shape() {
    let mut rule = ForwardRule::local("127.0.0.1", 8080, "localhost", 80);
    rule.id = "forward-1".to_string();
    rule.status = ForwardStatus::Active;
    rule.description = "plugin forward".to_string();

    let snapshot = native_plugin_forward_rule_snapshot(rule);
    assert_eq!(snapshot["id"], "forward-1");
    assert_eq!(snapshot["forward_type"], "local");
    assert_eq!(snapshot["bind_address"], "127.0.0.1");
    assert_eq!(snapshot["status"], "active");
    assert_eq!(snapshot["description"], "plugin forward");
}

#[test]
fn notification_severity_maps_to_workspace_toast_variant() {
    assert_eq!(
        native_plugin_notification_variant("error"),
        TerminalNoticeVariant::Error
    );
    assert_eq!(
        native_plugin_notification_variant("warning"),
        TerminalNoticeVariant::Warning
    );
    assert_eq!(
        native_plugin_notification_variant("info"),
        TerminalNoticeVariant::Default
    );
}

#[test]
fn progress_effect_updates_host_owned_toast_payload() {
    let notice = native_plugin_progress_notice(
        "com.example.demo",
        "progress-1",
        serde_json::json!({
            "title": "Indexing",
            "value": 2,
            "total": 4,
            "message": "Half done",
        }),
    );

    assert_eq!(notice.title, "Indexing (com.example.demo)");
    assert_eq!(notice.description.as_deref(), Some("Half done"));
    assert_eq!(notice.status_text.as_deref(), Some("50%"));
    assert_eq!(notice.progress, Some(50.0));
    assert!(native_plugin_progress_is_done(
        &serde_json::json!({"done": true})
    ));
}

#[test]
fn show_progress_returnable_host_api_creates_host_owned_reporter() {
    let (progress_tx, progress_rx) = mpsc::channel();
    let response = native_plugin_show_progress_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "progress-1".to_string(),
            namespace: "ui".to_string(),
            method: "showProgress".to_string(),
            args: serde_json::json!({
                "title": "Syncing",
                "registrationId": "progress-sync-1",
            }),
        },
        Some(&progress_tx),
    );

    assert_eq!(
        response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "id": "progress-sync-1",
                "registrationId": "progress-sync-1",
            })
        }
    );
    let request = progress_rx.recv().unwrap();
    assert!(matches!(
        request.action,
        NativePluginSyncAction::ReportProgress {
            plugin_id,
            registration_id,
            ..
        } if plugin_id == "com.example.demo" && registration_id == "progress-sync-1"
    ));
}

#[test]
fn show_confirm_returnable_host_api_resolves_user_choice() {
    let (confirm_tx, confirm_rx) = mpsc::channel::<NativePluginConfirmRequest>();
    let handle = std::thread::spawn(move || {
        let request = confirm_rx.recv().unwrap();
        assert_eq!(request.plugin_id, "com.example.demo");
        assert_eq!(request.request_id, "confirm-1");
        assert_eq!(request.title, "Delete cache?");
        assert_eq!(request.description, "This cannot be undone.");
        request.response_tx.send(true).unwrap();
    });

    let response = native_plugin_show_confirm_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "confirm-1".to_string(),
            namespace: "ui".to_string(),
            method: "showConfirm".to_string(),
            args: serde_json::json!({
                "title": "Delete cache?",
                "description": "This cannot be undone.",
            }),
        },
        &confirm_tx,
    );

    handle.join().unwrap();
    assert_eq!(
        response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!(true)
        }
    );
}

#[test]
fn show_confirm_returnable_host_api_rejects_missing_description() {
    let (confirm_tx, _confirm_rx) = mpsc::channel();
    let response = native_plugin_show_confirm_response(
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "confirm-2".to_string(),
            namespace: "ui".to_string(),
            method: "showConfirm".to_string(),
            args: serde_json::json!({
                "title": "Missing body",
            }),
        },
        &confirm_tx,
    );

    assert!(matches!(
        response.result,
        plugin_runtime::PluginResponseResult::Error { .. }
    ));
}

#[test]
fn storage_get_returnable_host_api_returns_json_or_null() {
    let snapshot = test_host_api_snapshot();
    let response = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "storage-get-1".to_string(),
            namespace: "storage".to_string(),
            method: "get".to_string(),
            args: serde_json::json!({ "key": "missing" }),
        },
    )
    .unwrap();
    assert_eq!(
        response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::Value::Null
        }
    );

    let error = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "storage-get-2".to_string(),
            namespace: "storage".to_string(),
            method: "get".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert!(matches!(
        error.result,
        plugin_runtime::PluginResponseResult::Error { .. }
    ));
}

#[test]
fn app_returnable_host_apis_match_tauri_snapshot_shape() {
    let snapshot = test_host_api_snapshot();

    let theme = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "app-theme".to_string(),
            namespace: "app".to_string(),
            method: "getTheme".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert_eq!(
        theme.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "name": "default",
                "isDark": true,
            })
        }
    );

    let settings = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "app-settings".to_string(),
            namespace: "app".to_string(),
            method: "getSettings".to_string(),
            args: serde_json::json!({ "category": "general" }),
        },
    )
    .unwrap();
    assert!(matches!(
        settings.result,
        plugin_runtime::PluginResponseResult::Ok { .. }
    ));
    if let plugin_runtime::PluginResponseResult::Ok { value } = settings.result {
        assert_eq!(value["language"], "zh-CN");
    }

    let pool_stats = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "app-pool".to_string(),
            namespace: "app".to_string(),
            method: "getPoolStats".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert_eq!(
        pool_stats.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "activeConnections": 0,
                "totalSessions": 0,
            })
        }
    );

    for (method, expected) in [
        ("getVersion", serde_json::json!(env!("CARGO_PKG_VERSION"))),
        (
            "getPlatform",
            serde_json::json!(native_plugin_platform_label()),
        ),
        ("getLocale", serde_json::json!("zh-CN")),
    ] {
        let response = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: format!("app-{method}"),
                namespace: "app".to_string(),
                method: method.to_string(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok { value: expected }
        );
    }
}

#[test]
fn api_invoke_rejects_undeclared_commands_and_runs_supported_whitelisted_commands() {
    let snapshot = test_host_api_snapshot_with_declared_api_commands();
    let permissions = plugin_runtime::PluginPermissionSet {
        capabilities: Vec::new(),
        allowed_host_apis: Vec::new(),
    };
    let sftp_router = NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::new(
        oxideterm_ssh::ConnectionPoolConfig::default(),
    ));
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let forwarding_registry = ForwardingRegistry::new();
    let transfer_manager = Arc::new(SftpTransferManager::new());
    let allowed = native_plugin_api_invoke_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "api-pool-stats".to_string(),
            namespace: "api".to_string(),
            method: "invoke".to_string(),
            args: serde_json::json!({
                "command": NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS,
                "args": {}
            }),
        },
        NativePluginBackendAdapters {
            permissions: &permissions,
            sftp_router: &sftp_router,
            sftp_runtime: &runtime,
            forwarding_registry: &forwarding_registry,
            forwarding_runtime: &runtime,
            transfer_manager: &transfer_manager,
        },
    );
    assert_eq!(
        allowed.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "activeConnections": 0,
                "totalSessions": 0,
            })
        }
    );

    let denied = native_plugin_api_invoke_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "api-denied".to_string(),
            namespace: "api".to_string(),
            method: "invoke".to_string(),
            args: serde_json::json!({ "command": "read_plugin_file" }),
        },
        NativePluginBackendAdapters {
            permissions: &permissions,
            sftp_router: &sftp_router,
            sftp_runtime: &runtime,
            forwarding_registry: &forwarding_registry,
            forwarding_runtime: &runtime,
            transfer_manager: &transfer_manager,
        },
    );
    assert!(matches!(
        denied.result,
        plugin_runtime::PluginResponseResult::Error {
            error: plugin_runtime::PluginError { ref code, .. }
        } if code == "backend_command_not_whitelisted"
    ));

    let unsupported = native_plugin_api_invoke_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "api-unsupported".to_string(),
            namespace: "api".to_string(),
            method: "invoke".to_string(),
            args: serde_json::json!({ "command": "custom_declared_command" }),
        },
        NativePluginBackendAdapters {
            permissions: &permissions,
            sftp_router: &sftp_router,
            sftp_runtime: &runtime,
            forwarding_registry: &forwarding_registry,
            forwarding_runtime: &runtime,
            transfer_manager: &transfer_manager,
        },
    );
    assert!(matches!(
        unsupported.result,
        plugin_runtime::PluginResponseResult::Error {
            error: plugin_runtime::PluginError { ref code, .. }
        } if code == "backend_command_not_supported"
    ));
}

#[test]
fn api_invoke_native_adapters_cover_system_transfer_and_capability_paths() {
    let snapshot = test_host_api_snapshot_with_declared_api_commands();
    let supported_commands = native_plugin_supported_backend_commands()
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    assert_eq!(
        supported_commands.len(),
        native_plugin_supported_backend_commands().len()
    );

    let permissions = plugin_runtime::PluginPermissionSet {
        capabilities: Vec::new(),
        allowed_host_apis: Vec::new(),
    };
    let sftp_router = NodeRouter::new(oxideterm_ssh::SshConnectionRegistry::new(
        oxideterm_ssh::ConnectionPoolConfig::default(),
    ));
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let forwarding_registry = ForwardingRegistry::new();
    let transfer_manager = Arc::new(SftpTransferManager::new());

    let version = native_plugin_api_invoke_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "api-version".to_string(),
            namespace: "api".to_string(),
            method: "invoke".to_string(),
            args: serde_json::json!({ "command": NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION }),
        },
        NativePluginBackendAdapters {
            permissions: &permissions,
            sftp_router: &sftp_router,
            sftp_runtime: &runtime,
            forwarding_registry: &forwarding_registry,
            forwarding_runtime: &runtime,
            transfer_manager: &transfer_manager,
        },
    );
    assert_eq!(
        version.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!(env!("CARGO_PKG_VERSION"))
        }
    );

    let transfer_stats = native_plugin_api_invoke_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "api-transfer-stats".to_string(),
            namespace: "api".to_string(),
            method: "invoke".to_string(),
            args: serde_json::json!({
                "command": NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS
            }),
        },
        NativePluginBackendAdapters {
            permissions: &permissions,
            sftp_router: &sftp_router,
            sftp_runtime: &runtime,
            forwarding_registry: &forwarding_registry,
            forwarding_runtime: &runtime,
            transfer_manager: &transfer_manager,
        },
    );
    assert_eq!(
        transfer_stats.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "active": 0,
                "queued": 0,
                "completed": 0,
            })
        }
    );

    let invalid_http = native_plugin_api_invoke_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "api-http-invalid-url".to_string(),
            namespace: "api".to_string(),
            method: "invoke".to_string(),
            args: serde_json::json!({
                "command": NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST,
                "args": {
                    "url": "file:///tmp/not-allowed",
                    "method": "GET",
                    "headers": {}
                }
            }),
        },
        NativePluginBackendAdapters {
            permissions: &permissions,
            sftp_router: &sftp_router,
            sftp_runtime: &runtime,
            forwarding_registry: &forwarding_registry,
            forwarding_runtime: &runtime,
            transfer_manager: &transfer_manager,
        },
    );
    assert!(matches!(
        invalid_http.result,
        plugin_runtime::PluginResponseResult::Error {
            error: plugin_runtime::PluginError { ref code, .. }
        } if code == "plugin_http_request_error"
    ));

    let denied_sftp = native_plugin_api_invoke_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "api-sftp-denied".to_string(),
            namespace: "api".to_string(),
            method: "invoke".to_string(),
            args: serde_json::json!({
                "command": NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR,
                "args": { "nodeId": "node-a", "path": "/" }
            }),
        },
        NativePluginBackendAdapters {
            permissions: &permissions,
            sftp_router: &sftp_router,
            sftp_runtime: &runtime,
            forwarding_registry: &forwarding_registry,
            forwarding_runtime: &runtime,
            transfer_manager: &transfer_manager,
        },
    );
    assert!(matches!(
        denied_sftp.result,
        plugin_runtime::PluginResponseResult::Error {
            error: plugin_runtime::PluginError { ref code, .. }
        } if code == "plugin_sftp_capability_denied"
    ));
}

#[test]
fn ui_get_layout_returnable_host_api_matches_tauri_snapshot_shape() {
    let snapshot = test_host_api_snapshot();
    let response = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "ui-layout".to_string(),
            namespace: "ui".to_string(),
            method: "getLayout".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();

    assert_eq!(
        response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "sidebarCollapsed": false,
                "activeTabId": null,
                "tabCount": 0,
            })
        }
    );
    assert_eq!(
        native_plugin_layout_snapshot(true, Some("7".to_string()), 3),
        serde_json::json!({
            "sidebarCollapsed": true,
            "activeTabId": "7",
            "tabCount": 3,
        })
    );
}

#[test]
fn connections_returnable_host_apis_match_tauri_snapshot_shape() {
    let snapshot = test_host_api_snapshot_with_connections();
    let all = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "connections-all".to_string(),
            namespace: "connections".to_string(),
            method: "getAll".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert_eq!(
        all.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!([{
                "id": "conn-1",
                "host": "example.test",
                "port": 22,
                "username": "deploy",
                "state": "active",
                "refCount": 2,
                "keepAlive": true,
                "createdAt": "1970-01-01T00:00:01.000Z",
                "lastActive": "1970-01-01T00:00:02.000Z",
                "terminalIds": ["term-1"],
                "parentConnectionId": null,
            }])
        }
    );

    let by_id = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "connections-get".to_string(),
            namespace: "connections".to_string(),
            method: "get".to_string(),
            args: serde_json::json!({ "connectionId": "conn-1" }),
        },
    )
    .unwrap();
    if let plugin_runtime::PluginResponseResult::Ok { value } = by_id.result {
        assert_eq!(value["host"], "example.test");
        assert_eq!(value["terminalIds"], serde_json::json!(["term-1"]));
    } else {
        panic!("connections.get returned an error");
    }

    let state = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "connections-state".to_string(),
            namespace: "connections".to_string(),
            method: "getState".to_string(),
            args: serde_json::json!({ "connectionId": "conn-1" }),
        },
    )
    .unwrap();
    assert_eq!(
        state.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!("active")
        }
    );

    let by_node = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "connections-node".to_string(),
            namespace: "connections".to_string(),
            method: "getByNode".to_string(),
            args: serde_json::json!({ "nodeId": "node-1" }),
        },
    )
    .unwrap();
    if let plugin_runtime::PluginResponseResult::Ok { value } = by_node.result {
        assert_eq!(value["id"], "conn-1");
    } else {
        panic!("connections.getByNode returned an error");
    }
}

#[test]
fn connections_returnable_host_apis_return_null_for_missing_ids() {
    let snapshot = test_host_api_snapshot_with_connections();
    for (method, args) in [
        ("get", serde_json::json!({ "connectionId": "missing" })),
        ("getState", serde_json::json!({ "connectionId": "missing" })),
        ("getByNode", serde_json::json!({ "nodeId": "missing" })),
    ] {
        let response = native_plugin_returnable_host_api_response(
            &snapshot,
            "com.example.demo",
            plugin_runtime::PluginHostCall {
                request_id: format!("connections-{method}-missing"),
                namespace: "connections".to_string(),
                method: method.to_string(),
                args,
            },
        )
        .unwrap();
        assert_eq!(
            response.result,
            plugin_runtime::PluginResponseResult::Ok {
                value: serde_json::Value::Null
            }
        );
    }
}

#[test]
fn sessions_returnable_host_apis_match_tauri_snapshot_shape() {
    let snapshot = test_host_api_snapshot_with_sessions();
    let tree = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sessions-tree".to_string(),
            namespace: "sessions".to_string(),
            method: "getTree".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert_eq!(
        tree.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!([{
                "id": "node-1",
                "label": "Production",
                "host": "example.test",
                "port": 22,
                "username": "deploy",
                "parentId": null,
                "childIds": ["node-2"],
                "connectionState": "active",
                "connectionId": "conn-1",
                "terminalIds": ["term-1"],
                "sftpSessionId": null,
            }, {
                "id": "node-2",
                "label": "root@child.test",
                "host": "child.test",
                "port": 2222,
                "username": "root",
                "parentId": "node-1",
                "childIds": [],
                "connectionState": "connecting",
                "connectionId": null,
                "terminalIds": [],
                "sftpSessionId": "sftp-2",
            }])
        }
    );

    let active = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sessions-active".to_string(),
            namespace: "sessions".to_string(),
            method: "getActiveNodes".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert_eq!(
        active.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!([{
                "nodeId": "node-1",
                "sessionId": "term-1",
                "connectionState": "active",
            }])
        }
    );

    let state = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sessions-state".to_string(),
            namespace: "sessions".to_string(),
            method: "getNodeState".to_string(),
            args: serde_json::json!({ "nodeId": "node-2" }),
        },
    )
    .unwrap();
    assert_eq!(
        state.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!("connecting")
        }
    );
}

#[test]
fn sessions_returnable_host_apis_return_null_for_missing_node() {
    let snapshot = test_host_api_snapshot_with_sessions();
    let state = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "sessions-state-missing".to_string(),
            namespace: "sessions".to_string(),
            method: "getNodeState".to_string(),
            args: serde_json::json!({ "nodeId": "missing" }),
        },
    )
    .unwrap();
    assert_eq!(
        state.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::Value::Null
        }
    );
}

#[test]
fn session_connection_state_maps_link_down_to_tauri_status() {
    let state = oxideterm_ssh::NodeState {
        readiness: NodeReadiness::Error,
        error: Some("Link down".to_string()),
        sftp_ready: false,
        sftp_cwd: None,
        ws_endpoint: None,
    };
    assert_eq!(
        native_plugin_session_connection_state(&state, 0),
        "link-down"
    );
}

#[test]
fn event_log_get_entries_filters_tauri_snapshot_shape() {
    let snapshot = test_host_api_snapshot_with_event_log_entries();
    let all = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "event-log-all".to_string(),
            namespace: "eventLog".to_string(),
            method: "getEntries".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert_eq!(
        all.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!([{
                "id": 1,
                "timestamp": 1000,
                "severity": "info",
                "category": "connection",
                "nodeId": "node-1",
                "connectionId": "conn-1",
                "title": "Connected",
                "detail": "ready",
                "source": "connection_status_changed",
            }, {
                "id": 2,
                "timestamp": 2000,
                "severity": "error",
                "category": "node",
                "title": "Failed",
                "source": "node_state_changed",
            }])
        }
    );

    let filtered = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "event-log-filtered".to_string(),
            namespace: "eventLog".to_string(),
            method: "getEntries".to_string(),
            args: serde_json::json!({
                "filter": {
                    "severity": "error",
                    "category": "node",
                }
            }),
        },
    )
    .unwrap();
    assert_eq!(
        filtered.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!([{
                "id": 2,
                "timestamp": 2000,
                "severity": "error",
                "category": "node",
                "title": "Failed",
                "source": "node_state_changed",
            }])
        }
    );
}

#[test]
fn terminal_readonly_returnable_host_apis_use_node_snapshots() {
    let snapshot = test_host_api_snapshot_with_terminal();
    let active = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-active".to_string(),
            namespace: "terminal".to_string(),
            method: "getActiveTarget".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert_eq!(
        active.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "sessionId": "term-1",
                "terminalType": "terminal",
                "nodeId": "node-1",
                "connectionId": "conn-1",
                "connectionState": "active",
                "label": "Production",
            })
        }
    );

    let buffer = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-buffer".to_string(),
            namespace: "terminal".to_string(),
            method: "getNodeBuffer".to_string(),
            args: serde_json::json!({ "nodeId": "node-1" }),
        },
    )
    .unwrap();
    assert_eq!(
        buffer.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!("alpha\nbeta\nAlpha")
        }
    );

    let selection = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-selection".to_string(),
            namespace: "terminal".to_string(),
            method: "getNodeSelection".to_string(),
            args: serde_json::json!({ "nodeId": "node-1" }),
        },
    )
    .unwrap();
    assert_eq!(
        selection.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!("beta")
        }
    );
}

#[test]
fn terminal_search_scroll_and_size_are_bounded() {
    let snapshot = test_host_api_snapshot_with_terminal();
    let search = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-search".to_string(),
            namespace: "terminal".to_string(),
            method: "search".to_string(),
            args: serde_json::json!({
                "nodeId": "node-1",
                "query": "alpha",
                "options": { "caseSensitive": false },
            }),
        },
    )
    .unwrap();
    assert_eq!(
        search.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "matches": [
                    {
                        "line_number": 0,
                        "column_start": 0,
                        "column_end": 5,
                        "matched_text": "alpha",
                        "line_content": "alpha",
                    },
                    {
                        "line_number": 2,
                        "column_start": 0,
                        "column_end": 5,
                        "matched_text": "Alpha",
                        "line_content": "Alpha",
                    },
                ],
                "total_matches": 2,
                "truncated": false,
                "error": null,
            })
        }
    );

    let scroll = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-scroll".to_string(),
            namespace: "terminal".to_string(),
            method: "getScrollBuffer".to_string(),
            args: serde_json::json!({
                "nodeId": "node-1",
                "startLine": 1,
                "count": 1,
            }),
        },
    )
    .unwrap();
    assert_eq!(
        scroll.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!([{ "text": "beta", "lineNumber": 1 }])
        }
    );

    let size = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-size".to_string(),
            namespace: "terminal".to_string(),
            method: "getBufferSize".to_string(),
            args: serde_json::json!({ "nodeId": "node-1" }),
        },
    )
    .unwrap();
    assert_eq!(
        size.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "currentLines": 3,
                "totalLines": 3,
                "maxLines": 3,
            })
        }
    );
}

#[test]
fn terminal_search_supports_regex_whole_word_and_invalid_regex() {
    let snapshot = test_host_api_snapshot_with_terminal();
    let whole_word = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-search-whole-word".to_string(),
            namespace: "terminal".to_string(),
            method: "search".to_string(),
            args: serde_json::json!({
                "nodeId": "node-1",
                "query": "alpha",
                "options": { "wholeWord": true, "caseSensitive": false },
            }),
        },
    )
    .unwrap();
    assert_eq!(
        whole_word.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "matches": [
                    {
                        "line_number": 0,
                        "column_start": 0,
                        "column_end": 5,
                        "matched_text": "alpha",
                        "line_content": "alpha",
                    },
                    {
                        "line_number": 2,
                        "column_start": 0,
                        "column_end": 5,
                        "matched_text": "Alpha",
                        "line_content": "Alpha",
                    },
                ],
                "total_matches": 2,
                "truncated": false,
                "error": null,
            })
        }
    );

    let regex = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-search-regex".to_string(),
            namespace: "terminal".to_string(),
            method: "search".to_string(),
            args: serde_json::json!({
                "nodeId": "node-1",
                "query": "^b.*a$",
                "options": { "regex": true, "caseSensitive": true },
            }),
        },
    )
    .unwrap();
    if let plugin_runtime::PluginResponseResult::Ok { value } = regex.result {
        assert_eq!(value["total_matches"], 1);
        assert_eq!(value["matches"][0]["matched_text"], "beta");
    } else {
        panic!("terminal regex search returned an error response");
    }

    let invalid = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "terminal-search-invalid".to_string(),
            namespace: "terminal".to_string(),
            method: "search".to_string(),
            args: serde_json::json!({
                "nodeId": "node-1",
                "query": "[invalid(",
                "options": { "regex": true },
            }),
        },
    )
    .unwrap();
    if let plugin_runtime::PluginResponseResult::Ok { value } = invalid.result {
        assert_eq!(value["total_matches"], 0);
        assert_eq!(value["matches"], serde_json::json!([]));
        assert!(value["error"].as_str().unwrap().contains("Invalid regex"));
    } else {
        panic!("invalid terminal regex search returned a protocol error");
    }
}

#[test]
fn terminal_write_host_calls_parse_text_and_node_id() {
    let active = native_plugin_terminal_action_from_call(&plugin_runtime::PluginHostCall {
        request_id: "terminal-write-active".to_string(),
        namespace: "terminal".to_string(),
        method: "writeToActive".to_string(),
        args: serde_json::json!({ "text": "ls\n" }),
    })
    .unwrap();
    assert!(matches!(
        active,
        NativePluginTerminalAction::WriteActive { ref text } if text == "ls\n"
    ));

    let node = native_plugin_terminal_action_from_call(&plugin_runtime::PluginHostCall {
        request_id: "terminal-write-node".to_string(),
        namespace: "terminal".to_string(),
        method: "writeToNode".to_string(),
        args: serde_json::json!({ "nodeId": "node-1", "text": "pwd\n" }),
    })
    .unwrap();
    assert!(matches!(
        node,
        NativePluginTerminalAction::WriteNode { ref node_id, ref text }
            if node_id == "node-1" && text == "pwd\n"
    ));

    let clear = native_plugin_terminal_action_from_call(&plugin_runtime::PluginHostCall {
        request_id: "terminal-clear-buffer".to_string(),
        namespace: "terminal".to_string(),
        method: "clearBuffer".to_string(),
        args: serde_json::json!({ "nodeId": "node-1" }),
    })
    .unwrap();
    assert!(matches!(
        clear,
        NativePluginTerminalAction::ClearBuffer { ref node_id } if node_id == "node-1"
    ));

    let telnet = native_plugin_terminal_action_from_call(&plugin_runtime::PluginHostCall {
        request_id: "terminal-open-telnet".to_string(),
        namespace: "terminal".to_string(),
        method: "openTelnet".to_string(),
        args: serde_json::json!({ "host": " example.com ", "port": 2323 }),
    })
    .unwrap();
    assert!(matches!(
        telnet,
        NativePluginTerminalAction::OpenTelnet { ref host, port }
            if host == "example.com" && port == 2323
    ));
}

#[test]
fn terminal_hook_response_values_parse_text_and_bytes() {
    assert_eq!(
        native_plugin_terminal_hook_text_value(&serde_json::json!({ "data": "cd /tmp\n" })),
        Some("cd /tmp\n".to_string())
    );
    assert_eq!(
        native_plugin_terminal_hook_bytes_value(&serde_json::json!([65, 66, 10])),
        Some(b"AB\n".to_vec())
    );
    assert_eq!(
        native_plugin_terminal_hook_bytes_value(&serde_json::json!({ "bytes": [120, 121] })),
        Some(b"xy".to_vec())
    );
    assert_eq!(
        native_plugin_terminal_hook_bytes_value(&serde_json::json!({ "bytes": [256] })),
        None
    );
    assert_eq!(native_plugin_terminal_hook_bytes_value(&Value::Null), None);
}

#[test]
fn terminal_input_interceptors_run_in_order_and_fail_open() {
    let hooks = vec![
        test_terminal_hook("first", "demo.first"),
        test_terminal_hook("timeout", "demo.timeout"),
        test_terminal_hook("second", "demo.second"),
    ];
    let result = native_plugin_reduce_input_interceptors(b"ls", &hooks, |hook, args| {
        match hook.registration_id.as_str() {
            "first" => {
                assert_eq!(args["data"], "ls");
                Some(json!({ "data": "sudo ls" }))
            }
            "timeout" => None,
            "second" => {
                assert_eq!(args["data"], "sudo ls");
                Some(json!("sudo ls -la"))
            }
            _ => unreachable!(),
        }
    });

    match result {
        TerminalInputInterceptorResult::Continue(bytes) => {
            assert_eq!(bytes, b"sudo ls -la");
        }
        TerminalInputInterceptorResult::Suppress => panic!("input should not be suppressed"),
    }
}

#[test]
fn terminal_input_interceptor_null_suppresses_input() {
    let hooks = vec![test_terminal_hook("suppress", "demo.suppress")];
    let result = native_plugin_reduce_input_interceptors(b"rm -rf /tmp/demo", &hooks, |_, _| {
        Some(Value::Null)
    });
    assert!(matches!(result, TerminalInputInterceptorResult::Suppress));
}

#[test]
fn terminal_output_processors_preserve_bytes_on_failure() {
    let hooks = vec![
        test_terminal_hook("first", "demo.first"),
        test_terminal_hook("error", "demo.error"),
        test_terminal_hook("second", "demo.second"),
    ];
    let output = native_plugin_reduce_output_processors(b"abc", &hooks, |hook, args| {
        match hook.registration_id.as_str() {
            "first" => {
                assert_eq!(args["bytes"], json!([97, 98, 99]));
                Some(json!({ "bytes": [65, 66, 67] }))
            }
            "error" => None,
            "second" => {
                assert_eq!(args["bytes"], json!([65, 66, 67]));
                Some(json!("done"))
            }
            _ => unreachable!(),
        }
    });
    assert_eq!(output, b"done");
}

#[test]
fn plugin_secret_account_ids_are_plugin_scoped_and_validated() {
    assert_eq!(
        native_plugin_secret_account_id("com.example.alpha", "token").unwrap(),
        "plugin-secret:17:com.example.alpha:5:token"
    );
    assert_ne!(
        native_plugin_secret_account_id("com.example.alpha", "token").unwrap(),
        native_plugin_secret_account_id("com.example.beta", "token").unwrap()
    );
    assert!(native_plugin_secret_account_id("com.example.alpha", "").is_err());
    assert!(native_plugin_secret_account_id("com.example.alpha", "bad\nkey").is_err());
    assert!(native_plugin_secret_account_id("../escape", "token").is_err());
}

#[test]
fn i18n_returnable_host_apis_use_plugin_scoped_fallback() {
    let snapshot = test_host_api_snapshot();
    let language = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "i18n-language".to_string(),
            namespace: "i18n".to_string(),
            method: "getLanguage".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();
    assert_eq!(
        language.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!("zh-CN")
        }
    );

    let translated = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "i18n-t".to_string(),
            namespace: "i18n".to_string(),
            method: "t".to_string(),
            args: serde_json::json!({ "key": "missing.title" }),
        },
    )
    .unwrap();
    assert_eq!(
        translated.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!("missing.title")
        }
    );
}

#[test]
fn settings_get_returnable_host_api_uses_declared_defaults() {
    let snapshot = test_host_api_snapshot_with_declared_setting();
    let value = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "settings-get".to_string(),
            namespace: "settings".to_string(),
            method: "get".to_string(),
            args: serde_json::json!({ "key": "mode" }),
        },
    )
    .unwrap();
    assert_eq!(
        value.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!("auto")
        }
    );

    let undeclared = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "settings-get-undeclared".to_string(),
            namespace: "settings".to_string(),
            method: "get".to_string(),
            args: serde_json::json!({ "key": "unknown" }),
        },
    )
    .unwrap();
    assert_eq!(
        undeclared.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::Value::Null
        }
    );
}

#[test]
fn syncable_settings_export_returns_tauri_shaped_payload() {
    let snapshot = test_host_api_snapshot();
    let response = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "settings-export".to_string(),
            namespace: "settings".to_string(),
            method: "exportSyncableSettings".to_string(),
            args: serde_json::json!({}),
        },
    )
    .unwrap();

    let plugin_runtime::PluginResponseResult::Ok { value } = response.result else {
        panic!("expected exportSyncableSettings to succeed");
    };
    assert_eq!(value["payload"]["appearance"]["language"], "zh-CN");
    assert_eq!(value["payload"]["appearance"]["uiDensity"], "comfortable");
    assert_eq!(value["payload"]["terminal"]["fontSize"], 14);
    assert_eq!(value["payload"]["terminal"]["theme"], "default");
    assert_eq!(value["payload"]["reconnect"]["autoReconnect"], true);
    assert_eq!(value["warnings"], serde_json::json!([]));
    assert!(
        value["revision"]
            .as_str()
            .is_some_and(|revision| { revision.starts_with("fnv1a-") })
    );
    assert!(
        value["exportedAt"]
            .as_str()
            .is_some_and(|exported_at| { exported_at.ends_with('Z') })
    );
}

#[test]
fn syncable_settings_apply_normalizes_payload_and_warnings() {
    let normalized = native_normalize_syncable_settings_payload(&serde_json::json!({
        "appearance": {
            "language": "xx-XX",
            "uiDensity": "wide",
        },
        "terminal": {
            "fontSize": 100.4,
            "theme": "   ",
        },
        "reconnect": {
            "autoReconnect": "yes",
        },
    }));

    assert_eq!(
        normalized.payload,
        serde_json::json!({
            "terminal": { "fontSize": 32 }
        })
    );
    assert_eq!(
        normalized.warnings,
        vec![
            serde_json::json!({
                "path": "appearance.language",
                "code": "unsupported-language",
                "applied": false,
                "message": "Unsupported language: xx-XX",
            }),
            serde_json::json!({
                "path": "appearance.uiDensity",
                "code": "invalid-ui-density",
                "applied": false,
                "message": "Unsupported ui density: wide",
            }),
            serde_json::json!({
                "path": "terminal.fontSize",
                "code": "font-size-clamped",
                "applied": true,
                "message": "Font size was clamped to 32",
                "normalizedValue": 32,
            }),
            serde_json::json!({
                "path": "terminal.theme",
                "code": "missing-theme",
                "applied": false,
                "message": "Theme id cannot be empty",
            }),
            serde_json::json!({
                "path": "reconnect.autoReconnect",
                "code": "invalid-auto-reconnect",
                "applied": false,
                "message": "autoReconnect must be a boolean",
            }),
        ]
    );
}

#[test]
fn syncable_settings_apply_returnable_host_api_reports_applied_payload() {
    let snapshot = test_host_api_snapshot();
    let response = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "settings-apply".to_string(),
            namespace: "settings".to_string(),
            method: "applySyncableSettings".to_string(),
            args: serde_json::json!({
                "payload": {
                    "appearance": { "language": "ja", "uiDensity": "compact" },
                    "terminal": { "fontSize": 16, "theme": "solarized-dark" },
                    "reconnect": { "autoReconnect": false },
                }
            }),
        },
    )
    .unwrap();

    let expected_payload = serde_json::json!({
        "appearance": { "language": "ja", "uiDensity": "compact" },
        "terminal": { "fontSize": 16, "theme": "solarized-dark" },
        "reconnect": { "autoReconnect": false },
    });
    assert_eq!(
        response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "revision": native_syncable_settings_revision(&expected_payload),
                "appliedPayload": expected_payload,
                "warnings": [],
            })
        }
    );
}

#[test]
fn custom_event_emit_returnable_host_api_is_plugin_scoped() {
    let snapshot = test_host_api_snapshot();
    let response = native_plugin_returnable_host_api_response(
        &snapshot,
        "com.example.demo",
        plugin_runtime::PluginHostCall {
            request_id: "events-emit".to_string(),
            namespace: "events".to_string(),
            method: "emit".to_string(),
            args: serde_json::json!({
                "name": "build.done",
                "payload": { "ok": true },
            }),
        },
    )
    .unwrap();

    assert_eq!(
        response.result,
        plugin_runtime::PluginResponseResult::Ok {
            value: serde_json::json!({
                "emitted": true,
                "event": "plugin.com.example.demo:build.done",
            })
        }
    );
    let (event_key, payload) = native_plugin_custom_event_from_args(
        "com.example.demo",
        serde_json::json!({
            "name": "build.done",
            "payload": { "ok": true },
        }),
    )
    .unwrap();
    assert_eq!(event_key, "plugin.com.example.demo:build.done");
    assert_eq!(payload["pluginId"], "com.example.demo");
    assert_eq!(payload["name"], "build.done");
    assert_eq!(payload["payload"], serde_json::json!({ "ok": true }));
}
