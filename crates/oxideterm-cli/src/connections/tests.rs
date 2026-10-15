// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{AuthType, SavedUpstreamProxyPolicy};

use super::*;

#[test]
fn connection_specs_resolve_and_persist_totp_bindings() {
    use oxideterm_connections::totp::{DEFAULT_TOTP_PROMPT, TotpParameters};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("connections.json");
    let mut store = ConnectionStore::load(&path).unwrap();
    // CLI selection needs metadata only; seed storage is covered by the store's own tests.
    let credential_id = "totp-shared".to_string();
    let mut snapshot = store.export_saved_connections_snapshot().unwrap();
    snapshot.totp_credentials = serde_json::from_value(serde_json::json!([{
        "id": credential_id, "name": "Shared MFA", "parameters": TotpParameters::default(),
        "prompt_pattern": DEFAULT_TOTP_PROMPT, "enabled": true,
        "updated_at": "2026-10-15T00:00:00Z", "secret_revision": "synthetic-revision"
    }]))
    .unwrap();
    store
        .apply_saved_connections_snapshot(snapshot, SavedConnectionsConflictStrategy::Replace)
        .unwrap();
    let spec = serde_json::from_value(serde_json::json!({
        "name": "Target", "host": "server.test", "username": "user", "totpCredential": "shared mfa",
        "proxyChain": [{"host": "jump.test", "username": "jump", "auth": {"type": "agent"}, "totpCredential": credential_id}]
    })).unwrap();
    let request = connection_request_from_spec(spec, None, &store, true).unwrap();
    let owner = store.upsert(request).unwrap().id;
    let saved = ConnectionStore::load(&path).unwrap();
    let existing = saved.get(&owner).unwrap();
    assert_eq!(
        existing.options.totp_credential_id.as_deref(),
        Some(credential_id.as_str())
    );
    assert_eq!(
        existing.proxy_chain[0].totp_credential_id.as_deref(),
        Some(credential_id.as_str())
    );
    for (spec, expected) in [
        (
            serde_json::json!({"totpCredential": credential_id}),
            Some(credential_id.as_str()),
        ),
        (
            serde_json::json!({"name": "Renamed"}),
            Some(credential_id.as_str()),
        ),
        (serde_json::json!({"clearTotpCredential": true}), None),
        (serde_json::json!({"host": "different.test"}), None),
    ] {
        let request = connection_request_from_spec(
            serde_json::from_value(spec).unwrap(),
            Some(existing),
            &saved,
            true,
        )
        .unwrap();
        assert_eq!(request.totp_credential_id.as_deref(), expected);
        assert_eq!(
            request.proxy_chain[0].totp_credential_id.as_deref(),
            Some(credential_id.as_str())
        );
    }
    use clap::Parser;
    let cli = crate::args::Cli::parse_from([
        "oxideterm",
        "connections",
        "edit",
        &owner,
        "--clear-totp-credential",
    ]);
    let crate::args::Command::Connections(ConnectionsCommand {
        action: ConnectionsAction::Edit(args),
    }) = cli.command
    else {
        panic!("expected edit");
    };
    let spec = read_connection_input(args.spec_path, args.direct, true).unwrap();
    let cleared = connection_request_from_spec(spec, Some(existing), &saved, true).unwrap();
    store.upsert(cleared).unwrap();
    assert_eq!(
        ConnectionStore::load(&path)
            .unwrap()
            .get(&owner)
            .unwrap()
            .options
            .totp_credential_id,
        None
    );
    let invalid = serde_json::from_value(serde_json::json!({"totpCredential": "missing"})).unwrap();
    assert_eq!(
        connection_request_from_spec(invalid, Some(existing), &saved, true)
            .unwrap_err()
            .code,
        "totp_credential_not_found"
    );
    let mut snapshot = store.export_saved_connections_snapshot().unwrap();
    let mut duplicate = snapshot.totp_credentials[0].clone();
    duplicate.id = "totp-duplicate".into();
    snapshot.totp_credentials.push(duplicate);
    store
        .apply_saved_connections_snapshot(snapshot, SavedConnectionsConflictStrategy::Replace)
        .unwrap();
    let ambiguous =
        serde_json::from_value(serde_json::json!({"totpCredential": "Shared MFA"})).unwrap();
    assert_eq!(
        connection_request_from_spec(ambiguous, Some(existing), &store, true)
            .unwrap_err()
            .code,
        "totp_credential_ambiguous"
    );
}

pub(crate) fn sample_connection(id: &str, name: &str) -> ConnectionInfo {
    ConnectionInfo {
        totp_credential_id: None,
        empty_password: false,
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
            empty_password: false,
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
        totp_credentials: Vec::new(),
        local_terminal_profiles: Vec::new(),
        local_terminal_tombstones: Vec::new(),
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
