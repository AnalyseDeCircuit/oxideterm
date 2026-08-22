// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{AuthType, SavedUpstreamProxyPolicy};

use super::*;

fn sample_connection(id: &str, name: &str) -> ConnectionInfo {
    ConnectionInfo {
        id: id.to_string(),
        name: name.to_string(),
        group: Some("prod".to_string()),
        notes: None,
        host: "example.com".to_string(),
        port: 22,
        username: "root".to_string(),
        auth_type: AuthType::Password,
        key_path: None,
        cert_path: None,
        managed_key_id: None,
        managed_key_name: None,
        gssapi_authentication: false,
        gssapi_server_identity: None,
        gssapi_delegate_credentials: false,
        proxy_chain: Vec::new(),
        upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
        created_at: "2026-05-26T00:00:00Z".to_string(),
        last_used_at: None,
        color: None,
        icon_background_color: None,
        icon: None,
        tags: vec!["primary".to_string()],
        agent_forwarding: false,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: false,
        ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
        post_connect_command: None,
    }
}

#[test]
fn finds_connection_by_id_or_case_insensitive_name() {
    let connections = vec![sample_connection("id-1", "Prod")];

    assert_eq!(find_connection(&connections, "id-1").unwrap().name, "Prod");
    assert_eq!(find_connection(&connections, "prod").unwrap().id, "id-1");
    assert!(find_connection(&connections, "missing").is_none());
}

#[test]
fn filters_connections_by_common_fields() {
    let connections = vec![
        sample_connection("id-1", "Prod"),
        ConnectionInfo {
            host: "staging.example.com".to_string(),
            group: Some("stage".to_string()),
            tags: vec!["preview".to_string()],
            ..sample_connection("id-2", "Staging")
        },
    ];

    assert_eq!(filter_connections(&connections, "primary").len(), 1);
    assert_eq!(filter_connections(&connections, "stage")[0].name, "Staging");
    assert_eq!(filter_connections(&connections, "example.com").len(), 2);
    assert!(filter_connections(&connections, "missing").is_empty());
}

#[test]
fn snapshot_changes_describe_incoming_records() {
    let snapshot = SavedConnectionsSyncSnapshot {
        revision: "rev".to_string(),
        exported_at: "2026-05-27T00:00:00Z".to_string(),
        records: vec![oxideterm_connections::SavedConnectionSyncRecord {
            id: "id-1".to_string(),
            revision: "record-rev".to_string(),
            updated_at: "2026-05-27T00:00:00Z".to_string(),
            deleted: false,
            payload: Some(sample_connection("id-1", "Prod")),
            options: None,
        }],
    };

    let changes = connections_snapshot_changes(&snapshot);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].action, "snapshotUpsert");
    assert_eq!(changes[0].after.as_deref(), Some("Prod"));
}
