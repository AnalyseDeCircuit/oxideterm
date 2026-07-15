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
            host: "example.test".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth_type: oxideterm_connections::AuthType::Agent,
            key_path: None,
            cert_path: None,
            managed_key_id: None,
            managed_key_name: None,
            proxy_chain: Vec::new(),
            upstream_proxy: oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_used_at: None,
            color: None,
            icon: None,
            tags: Vec::new(),
            agent_forwarding: false,
            legacy_ssh_compatibility: false,
            post_connect_command: None,
        }),
        options: Some(options),
    }
}

fn dirty_snapshot() -> CloudSyncLocalSnapshot {
    CloudSyncLocalSnapshot {
        metadata: crate::LocalSyncMetadata::default(),
        scope: crate::SyncScope::default(),
        dirty: crate::StructuredDirtyInfo {
            current_state: crate::StructuredLocalState::default(),
            dirty_sections: crate::StructuredDirtySections {
                connections: true,
                ..crate::StructuredDirtySections::default()
            },
            has_dirty: true,
        },
        upload_units: 0,
        connections_record_count: 0,
        forwards_record_count: 0,
        quick_commands_record_count: 0,
        serial_profiles_record_count: 0,
        sensitive_credentials_record_count: 0,
    }
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
    let mut remote_record = base_record.clone();
    remote_record.options.as_mut().unwrap().keep_alive_interval = 45;
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
    assert_eq!(merged_record.updated_at, "2026-01-02T00:00:00Z");
    assert_eq!(
        merged_record.revision,
        saved_connection_record_revision(merged_record).unwrap()
    );
}

#[test]
fn operation_guard_skips_or_rejects_concurrent_operation_like_tauri() {
    let guard = CloudSyncOperationGuard::default();
    let _permit = guard
        .begin(CloudSyncOperationKind::Upload, false)
        .unwrap()
        .unwrap();

    assert!(
        guard
            .begin(CloudSyncOperationKind::Check, true)
            .unwrap()
            .is_none()
    );
    let error = guard
        .begin(CloudSyncOperationKind::Check, false)
        .unwrap_err()
        .to_string();
    assert!(error.contains("operation_in_progress"));
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
fn upload_conflict_check_rejects_changed_legacy_snapshot_like_tauri() {
    let metadata = RemoteMetadata {
        exists: true,
        revision: Some("remote-new".to_string()),
        format: None,
        ..RemoteMetadata::default()
    };

    let error = ensure_no_remote_conflict(&dirty_snapshot(), &metadata, Some("remote-old"), None)
        .unwrap_err()
        .to_string();

    assert!(error.contains("remote_changed_before_upload"));
}

#[test]
fn upload_conflict_check_allows_unchanged_legacy_snapshot_like_tauri() {
    let metadata = RemoteMetadata {
        exists: true,
        revision: Some("remote-current".to_string()),
        format: None,
        ..RemoteMetadata::default()
    };

    ensure_no_remote_conflict(&dirty_snapshot(), &metadata, Some("remote-current"), None).unwrap();
}

#[test]
fn upload_conflict_check_rejects_changed_sensitive_credentials_section() {
    let local_snapshot = CloudSyncLocalSnapshot {
        metadata: crate::LocalSyncMetadata::default(),
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
        connections_record_count: 0,
        forwards_record_count: 0,
        quick_commands_record_count: 0,
        serial_profiles_record_count: 0,
        sensitive_credentials_record_count: 1,
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
