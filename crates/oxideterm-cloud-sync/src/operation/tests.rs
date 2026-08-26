use super::*;

fn connection_sync_record(
    options: oxideterm_connections::ConnectionOptions,
) -> oxideterm_connections::SavedConnectionSyncRecord {
    oxideterm_connections::SavedConnectionSyncRecord {
        id: "conn-1".to_string(),
        revision: "base-revision".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        deleted: false,
        payload: Some(oxideterm_connections::ConnectionInfo {
            id: "conn-1".to_string(),
            name: "Production".to_string(),
            group: None,
            notes: None,
            host: "example.test".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth_type: oxideterm_connections::AuthType::Agent,
            key_path: None,
            cert_path: None,
            managed_key_id: None,
            managed_key_name: None,
            gssapi_authentication: false,
            gssapi_server_identity: None,
            gssapi_delegate_credentials: false,
            proxy_chain: Vec::new(),
            upstream_proxy: oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_used_at: None,
            color: None,
            icon_background_color: None,
            icon: None,
            tags: Vec::new(),
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
            post_connect_command: None,
        }),
        options: Some(options),
    }
}

fn remote_desktop_snapshot(
    credential_ref: Option<&str>,
    host: &str,
) -> oxideterm_connections::RemoteDesktopProfilesSyncSnapshot {
    serde_json::from_value(serde_json::json!({
        "revision": "remote-desktop-revision",
        "exportedAt": "2026-07-26T00:00:00Z",
        "records": [{
            "id": "desktop-1",
            "name": "Production desktop",
            "protocol": "vnc",
            "host": host,
            "port": 5900,
            "credential_ref": credential_ref,
            "created_at": "2026-07-26T00:00:00Z",
            "updated_at": "2026-07-26T00:00:00Z"
        }]
    }))
    .expect("remote desktop snapshot should deserialize")
}

#[test]
fn remote_desktop_cloud_metadata_omits_device_local_credential_refs() {
    let mut snapshot = remote_desktop_snapshot(Some("device-keychain-entry"), "desktop.test");

    strip_remote_desktop_credential_refs(&mut snapshot);

    let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");
    assert!(!json.contains("device-keychain-entry"));
    assert!(snapshot.records[0].credential_ref.is_none());
}

#[test]
fn remote_desktop_apply_preserves_local_credentials_and_valid_gateway_refs() {
    let path = std::env::temp_dir().join(format!(
        "oxideterm-cloud-sync-remote-desktop-{}.json",
        uuid::Uuid::new_v4()
    ));
    let mut store_data = oxideterm_connections::ConnectionStoreData::default();
    store_data.remote_desktop_profiles =
        remote_desktop_snapshot(Some("current-device-keychain-entry"), "desktop.test").records;
    std::fs::write(&path, serde_json::to_vec(&store_data).unwrap()).unwrap();
    let mut store = ConnectionStore::load(&path).expect("connection store should load");
    let mut incoming =
        remote_desktop_snapshot(Some("untrusted-remote-keychain-entry"), "remote.test");
    incoming.records[0].ssh_gateway_connection_id = Some("conn-1".to_string());
    let incoming_connections = oxideterm_connections::SavedConnectionsSyncSnapshot {
        revision: "incoming-connections".to_string(),
        exported_at: "2026-07-26T00:00:00Z".to_string(),
        records: vec![connection_sync_record(
            oxideterm_connections::ConnectionOptions::default(),
        )],
    };
    let _prepared_connections = store
        .prepare_saved_connections_snapshot(
            incoming_connections,
            oxideterm_connections::SavedConnectionsConflictStrategy::Merge,
        )
        .expect("incoming gateway connection should prepare");

    preserve_local_remote_desktop_credential_refs(&mut incoming, &store);
    retain_available_remote_desktop_gateway_refs(&mut incoming, &store);

    assert_eq!(
        incoming.records[0].credential_ref.as_deref(),
        Some("current-device-keychain-entry")
    );
    assert_eq!(
        incoming.records[0].ssh_gateway_connection_id.as_deref(),
        Some("conn-1")
    );
    incoming.records[0].ssh_gateway_connection_id = Some("missing-gateway".to_string());
    retain_available_remote_desktop_gateway_refs(&mut incoming, &store);
    assert!(incoming.records[0].ssh_gateway_connection_id.is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn remote_desktop_three_way_merge_preserves_independent_changes() {
    let base = remote_desktop_snapshot(None, "old.test");
    let mut local = base.clone();
    local.records[0].name = "Renamed desktop".to_string();
    let mut remote = base.clone();
    remote.records[0].host = "new.test".to_string();

    let changed = merge_remote_desktop_profile_records(
        &mut remote,
        &base,
        &local,
        &ConflictStrategy::Merge,
        Utc::now(),
    )
    .expect("remote desktop profile merge should succeed");

    assert!(changed);
    assert_eq!(remote.records[0].name, "Renamed desktop");
    assert_eq!(remote.records[0].host, "new.test");
}

#[test]
fn field_merge_preserves_independent_local_and_remote_changes() {
    let base = serde_json::json!({
        "name": "Prod",
        "host": "old.example.test",
        "username": "ops"
    });
    let local = serde_json::json!({
        "name": "Production",
        "host": "old.example.test",
        "username": "ops"
    });
    let remote = serde_json::json!({
        "name": "Prod",
        "host": "new.example.test",
        "username": "ops"
    });

    let merged = merge_structured_model_fields(&base, &local, &remote, &ConflictStrategy::Merge)
        .expect("field merge should succeed")
        .expect("independent local field should be preserved");

    assert_eq!(merged["name"], "Production");
    assert_eq!(merged["host"], "new.example.test");
    assert_eq!(merged["username"], "ops");
}

#[test]
fn field_merge_uses_strategy_for_same_field_conflicts() {
    let base = serde_json::json!({ "host": "old.example.test" });
    let local = serde_json::json!({ "host": "local.example.test" });
    let remote = serde_json::json!({ "host": "remote.example.test" });

    let merge_result =
        merge_structured_model_fields(&base, &local, &remote, &ConflictStrategy::Merge)
            .expect("merge strategy should succeed")
            .expect("merge strategy should preserve local conflict");
    let replace_result =
        merge_structured_model_fields(&base, &local, &remote, &ConflictStrategy::Replace)
            .expect("replace strategy should succeed");

    assert_eq!(merge_result["host"], "local.example.test");
    assert!(replace_result.is_none());
}

#[test]
fn connection_merge_preserves_independent_full_option_changes() {
    let base_record = connection_sync_record(oxideterm_connections::ConnectionOptions::default());
    let mut local_record = base_record.clone();
    local_record.options.as_mut().unwrap().compression = true;
    local_record.options.as_mut().unwrap().ssh_algorithms.cipher =
        vec!["aes256-gcm@openssh.com".to_string()];
    let mut remote_record = base_record.clone();
    remote_record.options.as_mut().unwrap().keep_alive_interval = 45;
    remote_record.options.as_mut().unwrap().ssh_algorithms.mac =
        vec!["hmac-sha2-512-etm@openssh.com".to_string()];
    let base = SavedConnectionsSyncSnapshot {
        revision: "base".to_string(),
        exported_at: "2026-01-01T00:00:00Z".to_string(),
        records: vec![base_record],
    };
    let local = SavedConnectionsSyncSnapshot {
        revision: "local".to_string(),
        exported_at: "2026-01-01T00:00:00Z".to_string(),
        records: vec![local_record],
    };
    let mut remote = SavedConnectionsSyncSnapshot {
        revision: "remote".to_string(),
        exported_at: "2026-01-01T00:00:00Z".to_string(),
        records: vec![remote_record],
    };

    assert!(
        merge_connection_records(
            &mut remote,
            &base,
            &local,
            &ConflictStrategy::Merge,
            "2026-01-02T00:00:00Z",
        )
        .unwrap()
    );

    let merged_record = &remote.records[0];
    let merged_options = merged_record.options.as_ref().unwrap();
    assert!(merged_options.compression);
    assert_eq!(merged_options.keep_alive_interval, 45);
    assert_eq!(
        merged_options.ssh_algorithms.cipher,
        ["aes256-gcm@openssh.com"]
    );
    assert_eq!(
        merged_options.ssh_algorithms.mac,
        ["hmac-sha2-512-etm@openssh.com"]
    );
    assert_eq!(merged_record.updated_at, "2026-01-02T00:00:00Z");
    assert_eq!(
        merged_record.revision,
        saved_connection_record_revision(merged_record).unwrap()
    );
}

#[test]
fn operation_guard_clears_when_permit_drops() {
    let guard = CloudSyncOperationGuard::default();
    {
        let _permit = guard
            .begin(CloudSyncOperationKind::Upload, false)
            .unwrap()
            .unwrap();
    }

    assert!(
        guard
            .begin(CloudSyncOperationKind::Check, false)
            .unwrap()
            .is_some()
    );
}

#[test]
fn connection_preflight_allows_managed_keys_only_with_sensitive_credentials_scope() {
    let without_sensitive_credentials = crate::SyncScope {
        sync_sensitive_credentials: false,
        ..crate::SyncScope::default()
    };
    let with_sensitive_credentials = crate::SyncScope {
        sync_sensitive_credentials: true,
        ..crate::SyncScope::default()
    };

    assert!(!include_managed_keys_in_connection_preflight(
        &without_sensitive_credentials
    ));
    assert!(include_managed_keys_in_connection_preflight(
        &with_sensitive_credentials
    ));
}

#[test]
fn upload_conflict_check_rejects_changed_sensitive_credentials_section() {
    let local_snapshot = CloudSyncLocalSnapshot {
        scope: crate::SyncScope {
            sync_sensitive_credentials: true,
            ..crate::SyncScope::default()
        },
        dirty: crate::StructuredDirtyInfo {
            current_state: crate::StructuredLocalState {
                sensitive_credentials: Some("local-sensitive".to_string()),
                ..crate::StructuredLocalState::default()
            },
            dirty_sections: crate::StructuredDirtySections {
                sensitive_credentials: true,
                ..crate::StructuredDirtySections::default()
            },
            has_dirty: true,
        },
        upload_units: 1,
        sensitive_credentials_record_count: 1,
        ..CloudSyncLocalSnapshot::default()
    };
    let metadata = RemoteMetadata {
        exists: true,
        format: Some(STRUCTURED_MANIFEST_FORMAT.to_string()),
        section_revisions: Some(StructuredSectionRevisions {
            sensitive_credentials: Some("remote-new".to_string()),
            ..StructuredSectionRevisions::default()
        }),
        ..RemoteMetadata::default()
    };
    let previous_sections = StructuredSectionRevisions {
        sensitive_credentials: Some("remote-old".to_string()),
        ..StructuredSectionRevisions::default()
    };

    let error =
        ensure_no_remote_conflict(&local_snapshot, &metadata, None, Some(&previous_sections))
            .unwrap_err()
            .to_string();

    assert!(error.contains("remote_changed_before_upload"));
}

#[test]
fn legacy_preview_uses_selected_connection_names_when_importing() {
    let selected_names =
        legacy_preview_selected_names(true, Some(vec!["Prod".to_string(), "Staging".to_string()]))
            .unwrap();

    assert_eq!(
        selected_names,
        vec!["Prod".to_string(), "Staging".to_string()]
    );
}

#[test]
fn legacy_preview_clears_connection_names_when_connections_are_disabled() {
    let selected_names = legacy_preview_selected_names(true, None);
    assert!(selected_names.is_none());

    let selected_names =
        legacy_preview_selected_names(false, Some(vec!["Prod".to_string()])).unwrap();
    assert!(selected_names.is_empty());
}
