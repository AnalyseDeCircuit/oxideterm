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
        plugin_profiles: vec![serde_json::from_value(serde_json::json!({
            "id":"custom", "plugin_id":"missing.provider", "type_id":"custom", "name":"Device", "group":null,
            "config":{}, "credential_refs":{}, "created_at":"2026-10-19T00:00:00Z", "updated_at":"2026-10-19T00:00:00Z", "last_used_at":null
        })).unwrap()],
        plugin_tombstones: vec![oxideterm_connections::DeletedConnectionTombstone { id: "removed".into(), deleted_at: chrono::Utc::now() }],
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

    assert_eq!(
        changes
            .iter()
            .map(|change| (
                change.action,
                change.target.as_str(),
                change.after.as_deref()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("snapshotUpsert", "id-1", Some("Prod")),
            ("snapshotUpsert", "custom", Some("Device")),
            ("snapshotDelete", "removed", None),
        ]
    );
}

#[test]
fn batch_create_args_parse_with_files_and_policy() {
    use clap::Parser;
    let cli = crate::args::Cli::parse_from([
        "oxideterm",
        "connections",
        "batch-create",
        "--template-file",
        "template.conf",
        "--data-file",
        "hosts.txt",
        "--duplicate",
        "rename",
    ]);
    let crate::args::Command::Connections(ConnectionsCommand {
        action: ConnectionsAction::BatchCreate(args),
    }) = cli.command
    else {
        panic!("expected batch-create");
    };
    assert_eq!(args.template_path.as_deref(), Some("template.conf"));
    assert_eq!(args.data_path.as_deref(), Some("hosts.txt"));
    assert!(matches!(
        args.duplicate,
        crate::args::ConnectionBatchDuplicateArg::Rename
    ));
    assert!(!args.write.yes, "writes require an explicit confirmation");
}

#[test]
fn batch_default_auth_follows_the_template_identity_source() {
    use oxideterm_connections::{BatchTemplate, SecretString, parse_batch_template};

    let with_identity: BatchTemplate =
        parse_batch_template("Host --\n  User u\n  IdentityFile ~/.ssh/kn\n").unwrap();
    let conflict = batch::BatchAuthInput {
        password: Some(SecretString::from("unused-secret")),
        key_path: None,
    };
    let error = batch::resolve_batch_default_auth(&with_identity, &conflict, true).unwrap_err();
    assert_eq!(error.code, "connection_batch_auth_conflict");
    assert!(
        batch::resolve_batch_default_auth(
            &with_identity,
            &batch::BatchAuthInput {
                password: None,
                key_path: None,
            },
            true,
        )
        .unwrap()
        .is_none(),
        "an identity template needs no dialog authentication"
    );

    let without_identity: BatchTemplate = parse_batch_template("Host --\n  User u\n").unwrap();
    let auth = batch::resolve_batch_default_auth(
        &without_identity,
        &batch::BatchAuthInput {
            password: Some(SecretString::from("pw")),
            key_path: None,
        },
        true,
    )
    .unwrap()
    .expect("password becomes the shared connection authentication");
    match auth {
        oxideterm_connections::SavedAuth::Password {
            plaintext_password, ..
        } => assert_eq!(
            plaintext_password
                .expect("the set account keeps its secret")
                .expose_secret(),
            "pw",
            "the supplied secret must reach the saved authentication unchanged"
        ),
        other => panic!("expected the shared password, got {other:?}"),
    }
    let auth = batch::resolve_batch_default_auth(
        &without_identity,
        &batch::BatchAuthInput {
            password: None,
            key_path: Some("~/.ssh/kn".into()),
        },
        true,
    )
    .unwrap()
    .expect("key path becomes the shared connection authentication");
    assert!(matches!(auth, oxideterm_connections::SavedAuth::Key { .. }));
    let error = batch::resolve_batch_default_auth(
        &without_identity,
        &batch::BatchAuthInput {
            password: Some(SecretString::from("pw")),
            key_path: Some("both".into()),
        },
        true,
    )
    .unwrap_err();
    assert_eq!(error.code, "connection_batch_auth_conflict");
}

#[test]
fn batch_changes_describe_rows_and_invalid_lines() {
    use oxideterm_connections::{
        BatchDuplicateNamePolicy, BatchExistingConnection, parse_batch_data, parse_batch_template,
        resolve_batch_rows,
    };

    let template = parse_batch_template("Host --\n  User u\n  IdentityFile ~/.ssh/kn\n").unwrap();
    let data = parse_batch_data("hktkosl1100\nhktkosl1100\nbad host\n").unwrap();
    let preview = resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
    let changes = batch::batch_changes(&preview);
    assert_eq!(
        changes
            .iter()
            .map(|change| change.action)
            .collect::<Vec<&str>>(),
        vec!["batchCreate", "batchSkipDuplicateTarget", "batchInvalid"]
    );
    assert_eq!(changes[0].target, "hktkosl1100");
    assert_eq!(changes[0].after.as_deref(), Some("u@hktkosl1100:22"));
    assert_eq!(changes[2].target, "line 3");

    // A stored connection with the same target is rewritten, not duplicated.
    // The stored connection carries the same expanded credential the batch
    // resolves, which is what `BatchExistingConnection::from_saved` reports.
    let key_path = resolve_batch_rows(
        &template,
        &parse_batch_data("hktkosl1100\n").unwrap(),
        None,
        &[],
        BatchDuplicateNamePolicy::Skip,
    )
    .addable_rows()[0]
        .auth
        .key_path()
        .map(str::to_owned);
    let existing = [BatchExistingConnection {
        id: "saved-id".to_owned(),
        name: "hktkosl1100".to_owned(),
        host: "hktkosl1100".to_owned(),
        port: 22,
        username: "u".to_owned(),
        group: None,
        tags: Vec::new(),
        auth_type: oxideterm_connections::AuthType::Key,
        key_path,
        cert_path: None,
        managed_key_id: None,
        has_proxy_command: false,
        connect_timeout_seconds: None,
        agent_forwarding: false,
    }];
    let data = parse_batch_data("hktkosl1100?group=helphub\n").unwrap();
    let preview = resolve_batch_rows(
        &template,
        &data,
        None,
        &existing,
        BatchDuplicateNamePolicy::Rename,
    );
    let changes = batch::batch_changes(&preview);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].action, "batchUpdate");
    assert_eq!(changes[0].target, "hktkosl1100");

    // Repeating stored data reports the row as already there.
    let data = parse_batch_data("hktkosl1100\n").unwrap();
    let preview = resolve_batch_rows(
        &template,
        &data,
        None,
        &existing,
        BatchDuplicateNamePolicy::Skip,
    );
    let changes = batch::batch_changes(&preview);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].action, "batchUnchanged");

    // Rendered reports must not carry credential values.
    let cred_template = parse_batch_template("Host --\n  HostName {host}\n  User u\n").unwrap();
    let cred_preview = resolve_batch_rows(
        &cred_template,
        &parse_batch_data("ops:example-pw@db1?name=db1-row\n").unwrap(),
        None,
        &[],
        BatchDuplicateNamePolicy::Skip,
    );
    let rendered = format!(
        "{cred_preview:?} {}",
        serde_json::to_string(&batch::batch_changes(&cred_preview)).unwrap()
    );
    assert!(
        !rendered.contains("example-pw"),
        "batch reports must not render secrets: {rendered}"
    );
}

#[test]
fn batch_write_is_refused_while_any_row_is_invalid() {
    use oxideterm_connections::{
        BatchDuplicateNamePolicy, parse_batch_data, parse_batch_template, resolve_batch_rows,
    };

    let template = parse_batch_template("Host --\n  User u\n  IdentityFile ~/.ssh/kn\n").unwrap();
    let data = parse_batch_data("hktkosl1100\nbad host\n").unwrap();
    let preview = resolve_batch_rows(&template, &data, None, &[], BatchDuplicateNamePolicy::Skip);
    assert_eq!(preview.addable_count(), 1, "one row alone would be written");

    let error = batch::ensure_rows_valid(&preview, false, true).unwrap_err();
    assert_eq!(error.code, "connection_batch_rows_invalid");
    assert!(
        error.message.contains("2 ("),
        "the message names the offending line: {}",
        error.message
    );

    batch::ensure_rows_valid(&preview, true, true)
        .expect("a dry run reports invalid rows instead of failing");
}
