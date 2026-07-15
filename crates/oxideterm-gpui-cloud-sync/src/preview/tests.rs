use oxideterm_cloud_sync::{
    ConflictStrategy, RawSyncScope, StructuredDirtyInfo, StructuredDirtySections,
    StructuredLocalState, StructuredSectionRevisions, SyncScope, service::CloudSyncLocalSnapshot,
    state::CloudSyncPersistedState,
};

use super::*;

#[test]
fn preview_fact_rows_preserve_display_order() {
    let summary = CloudSyncPreviewSummary {
        connections: 2,
        forwards: 3,
        plugin_settings_count: 4,
        has_embedded_keys: true,
        ..CloudSyncPreviewSummary::default()
    };

    assert_eq!(
        cloud_sync_preview_fact_rows(&summary),
        vec![
            vec![
                CloudSyncPreviewFactSpec {
                    label_key: "plugin.cloud_sync.preview.connection_count",
                    value: CloudSyncPreviewFactValue::Count(2),
                },
                CloudSyncPreviewFactSpec {
                    label_key: "plugin.cloud_sync.preview.total_forwards",
                    value: CloudSyncPreviewFactValue::Count(3),
                },
            ],
            vec![
                CloudSyncPreviewFactSpec {
                    label_key: "plugin.cloud_sync.preview.plugin_settings_label",
                    value: CloudSyncPreviewFactValue::Count(4),
                },
                CloudSyncPreviewFactSpec {
                    label_key: "plugin.cloud_sync.preview.embedded_keys_label",
                    value: CloudSyncPreviewFactValue::YesNo(true),
                },
            ],
        ]
    );
}

#[test]
fn preview_body_sections_keep_selection_first() {
    let summary = CloudSyncPreviewSummary {
        forward_details: vec![CloudSyncForwardDetail {
            owner_connection_name: "prod".to_string(),
            direction: "local".to_string(),
            description: "Local tunnel".to_string(),
        }],
        records: vec![CloudSyncPreviewRecord {
            resource: "connection".to_string(),
            name: "prod".to_string(),
            action: "import".to_string(),
            reason_code: "new".to_string(),
            target_name: None,
        }],
        ..CloudSyncPreviewSummary::default()
    };

    let sections = cloud_sync_preview_body_sections(&summary);

    assert!(matches!(
        sections[0],
        CloudSyncPreviewBodySection::Selection
    ));
    assert!(matches!(
        sections[1],
        CloudSyncPreviewBodySection::ForwardDetails(_)
    ));
    assert!(matches!(
        sections[2],
        CloudSyncPreviewBodySection::RecordGroup {
            action: "import",
            ..
        }
    ));
}

#[test]
fn coverage_model_marks_partial_sections_and_sensitive_exclusion() {
    let raw_scope = RawSyncScope {
        app_settings_sections: Some(vec!["general".to_string(), "network".to_string()]),
        sync_sensitive_credentials: Some(false),
        ..RawSyncScope::default()
    };

    let items = cloud_sync_coverage_model(&raw_scope);

    let app_settings = items
        .iter()
        .find(|item| item.label_key == "plugin.cloud_sync.settings.sync_app_settings")
        .expect("app settings coverage item");
    assert_eq!(app_settings.status, CloudSyncCoverageStatus::Partial);
    assert_eq!(
        app_settings.detail,
        CloudSyncCoverageDetail::AppSettingsSections(vec![
            "general".to_string(),
            "network".to_string()
        ])
    );

    let sensitive = items
        .iter()
        .find(|item| item.label_key == "plugin.cloud_sync.settings.sync_sensitive_credentials")
        .expect("sensitive credentials coverage item");
    assert_eq!(sensitive.status, CloudSyncCoverageStatus::Excluded);
}

#[test]
fn preview_impact_items_explain_excluded_and_partial_selection() {
    let summary = CloudSyncPreviewSummary {
        connections: 2,
        forwards: 1,
        quick_commands: 3,
        has_app_settings: true,
        app_settings_sections: vec![
            CloudSyncAppSettingsSection {
                id: "general".to_string(),
                field_count: 2,
            },
            CloudSyncAppSettingsSection {
                id: "network".to_string(),
                field_count: 1,
            },
        ],
        ..CloudSyncPreviewSummary::default()
    };
    let mut selection = CloudSyncPreviewSelection {
        import_connections: true,
        selected_connection_names: summary.connection_record_names(),
        selected_connection_ids: Default::default(),
        import_quick_commands: false,
        selected_quick_command_ids: Default::default(),
        import_serial_profiles: false,
        selected_serial_profile_ids: Default::default(),
        import_sensitive_credentials: false,
        import_app_settings: true,
        selected_app_settings_sections: ["general".to_string()].into_iter().collect(),
        import_plugin_settings: false,
        selected_plugin_ids: Default::default(),
        import_forwards: true,
        selected_forward_ids: Default::default(),
        conflict_strategy: ConflictStrategy::Merge,
    };

    let items = cloud_sync_preview_impact_items(&summary, &selection);

    assert!(items.iter().any(|item| {
        item.label_key == "plugin.cloud_sync.preview.quick_commands_label"
            && item.status == CloudSyncCoverageStatus::Excluded
    }));
    assert!(items.iter().any(|item| {
        item.label_key == "plugin.cloud_sync.settings.sync_app_settings"
            && item.status == CloudSyncCoverageStatus::Partial
    }));

    selection.selected_app_settings_sections.clear();
    let items = cloud_sync_preview_impact_items(&summary, &selection);
    assert!(items.iter().any(|item| {
        item.label_key == "plugin.cloud_sync.settings.sync_app_settings"
            && item.status == CloudSyncCoverageStatus::Excluded
    }));
}

#[test]
fn upload_diff_items_mark_local_changes_and_remote_overwrites() {
    let snapshot = test_snapshot(
        SyncScope::default(),
        StructuredLocalState {
            connections: Some("local-connections-2".to_string()),
            forwards: Some("forwards-1".to_string()),
            ..StructuredLocalState::default()
        },
    );
    let state = CloudSyncPersistedState {
        last_check_at: Some("2026-06-12T00:00:00Z".to_string()),
        last_synced_structured_state: Some(StructuredLocalState {
            connections: Some("local-connections-1".to_string()),
            forwards: Some("forwards-1".to_string()),
            ..StructuredLocalState::default()
        }),
        remote_section_revisions: Some(StructuredSectionRevisions {
            connections: Some("remote-connections".to_string()),
            forwards: Some("forwards-1".to_string()),
            ..StructuredSectionRevisions::default()
        }),
        ..CloudSyncPersistedState::default()
    };

    let items = cloud_sync_upload_diff_items(&snapshot, &state);

    let connections = items
        .iter()
        .find(|item| {
            item.label == CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_connections")
        })
        .expect("connections diff item");
    assert_eq!(connections.local_status, CloudSyncLocalDiffStatus::Modified);
    assert_eq!(
        connections.remote_status,
        CloudSyncRemoteDiffStatus::Overwrites
    );
    let forwards = items
        .iter()
        .find(|item| {
            item.label == CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_forwards")
        })
        .expect("forwards diff item");
    assert_eq!(forwards.local_status, CloudSyncLocalDiffStatus::Unchanged);
    assert_eq!(forwards.remote_status, CloudSyncRemoteDiffStatus::Unchanged);
}

#[test]
fn upload_diff_items_show_scope_exclusions_that_remove_remote_sections() {
    let mut scope = SyncScope::default();
    scope.sync_sensitive_credentials = false;
    let snapshot = test_snapshot(scope, StructuredLocalState::default());
    let state = CloudSyncPersistedState {
        last_check_at: Some("2026-06-12T00:00:00Z".to_string()),
        remote_section_revisions: Some(StructuredSectionRevisions {
            sensitive_credentials: Some("remote-secrets".to_string()),
            ..StructuredSectionRevisions::default()
        }),
        ..CloudSyncPersistedState::default()
    };

    let items = cloud_sync_upload_diff_items(&snapshot, &state);

    let sensitive = items
        .iter()
        .find(|item| {
            item.label
                == CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_sensitive_credentials")
        })
        .expect("sensitive credentials diff item");
    assert_eq!(sensitive.local_status, CloudSyncLocalDiffStatus::Excluded);
    assert_eq!(
        sensitive.remote_status,
        CloudSyncRemoteDiffStatus::RemovedByScope
    );
}

#[test]
fn apply_field_diff_items_show_changed_quick_command_fields() {
    let preview = CloudSyncPendingPreview::Structured(StructuredPreview {
        remote_metadata: Default::default(),
        manifest: oxideterm_cloud_sync::create_manifest_base(
            "rev-1",
            "2026-06-12T00:00:00Z",
            "device",
            SyncScope::default(),
        ),
        connections_snapshot: None,
        forwards_snapshot: None,
        quick_commands_snapshot_json: Some(
            serde_json::to_string(&QuickCommandsSnapshot {
                version: 1,
                categories: Vec::new(),
                commands: vec![quick_command("cmd-1", "Deploy", "deploy --prod")],
                updated_at: 2,
            })
            .expect("remote quick commands"),
        ),
        serial_profiles_snapshot: None,
        base_connections_snapshot: None,
        base_forwards_snapshot: None,
        base_quick_commands_snapshot_json: None,
        base_serial_profiles_snapshot: None,
        sensitive_credentials_entry: None,
        sensitive_credentials_preview: None,
        app_settings_entries: Default::default(),
        app_settings_sections: Default::default(),
        plugin_settings_entries: Default::default(),
        plugin_settings_counts: Default::default(),
    });
    let selection = CloudSyncPreviewSelection {
        import_connections: false,
        selected_connection_names: Default::default(),
        selected_connection_ids: Default::default(),
        import_quick_commands: true,
        selected_quick_command_ids: Default::default(),
        import_serial_profiles: false,
        selected_serial_profile_ids: Default::default(),
        import_sensitive_credentials: false,
        import_app_settings: false,
        selected_app_settings_sections: Default::default(),
        import_plugin_settings: false,
        selected_plugin_ids: Default::default(),
        import_forwards: false,
        selected_forward_ids: Default::default(),
        conflict_strategy: ConflictStrategy::Merge,
    };
    let local = CloudSyncLocalFieldDiffSnapshot {
        quick_commands: Some(QuickCommandsSnapshot {
            version: 1,
            categories: Vec::new(),
            commands: vec![quick_command("cmd-1", "Deploy", "deploy --staging")],
            updated_at: 1,
        }),
        ..CloudSyncLocalFieldDiffSnapshot::default()
    };

    let items = cloud_sync_apply_field_diff_items(&preview, &selection, &local);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, CloudSyncFieldDiffStatus::Modified);
    assert!(items[0].fields.iter().any(|field| {
        field.label_key == "plugin.cloud_sync.diff_fields.command"
            && field.before.as_deref() == Some("deploy --staging")
            && field.after.as_deref() == Some("deploy --prod")
    }));
}

#[test]
fn apply_field_diff_items_show_effective_field_merge_result() {
    let base_command = quick_command("cmd-1", "Deploy", "deploy --old");
    let mut local_command = base_command.clone();
    local_command.description = Some("local note".to_string());
    let mut remote_command = base_command.clone();
    remote_command.command = "deploy --prod".to_string();
    let preview = CloudSyncPendingPreview::Structured(StructuredPreview {
        remote_metadata: Default::default(),
        manifest: oxideterm_cloud_sync::create_manifest_base(
            "rev-1",
            "2026-06-12T00:00:00Z",
            "device",
            SyncScope::default(),
        ),
        connections_snapshot: None,
        forwards_snapshot: None,
        quick_commands_snapshot_json: Some(
            serde_json::to_string(&QuickCommandsSnapshot {
                version: 1,
                categories: Vec::new(),
                commands: vec![remote_command],
                updated_at: 2,
            })
            .expect("remote quick commands"),
        ),
        serial_profiles_snapshot: None,
        base_connections_snapshot: None,
        base_forwards_snapshot: None,
        base_quick_commands_snapshot_json: Some(
            serde_json::to_string(&QuickCommandsSnapshot {
                version: 1,
                categories: Vec::new(),
                commands: vec![base_command],
                updated_at: 1,
            })
            .expect("base quick commands"),
        ),
        base_serial_profiles_snapshot: None,
        sensitive_credentials_entry: None,
        sensitive_credentials_preview: None,
        app_settings_entries: Default::default(),
        app_settings_sections: Default::default(),
        plugin_settings_entries: Default::default(),
        plugin_settings_counts: Default::default(),
    });
    let selection = CloudSyncPreviewSelection {
        import_connections: false,
        selected_connection_names: Default::default(),
        selected_connection_ids: Default::default(),
        import_quick_commands: true,
        selected_quick_command_ids: Default::default(),
        import_serial_profiles: false,
        selected_serial_profile_ids: Default::default(),
        import_sensitive_credentials: false,
        import_app_settings: false,
        selected_app_settings_sections: Default::default(),
        import_plugin_settings: false,
        selected_plugin_ids: Default::default(),
        import_forwards: false,
        selected_forward_ids: Default::default(),
        conflict_strategy: ConflictStrategy::Merge,
    };
    let local = CloudSyncLocalFieldDiffSnapshot {
        quick_commands: Some(QuickCommandsSnapshot {
            version: 1,
            categories: Vec::new(),
            commands: vec![local_command],
            updated_at: 3,
        }),
        ..CloudSyncLocalFieldDiffSnapshot::default()
    };

    let items = cloud_sync_apply_field_diff_items(&preview, &selection, &local);

    assert_eq!(items.len(), 1);
    assert!(items[0].fields.iter().any(|field| {
        field.label_key == "plugin.cloud_sync.diff_fields.command"
            && field.before.as_deref() == Some("deploy --old")
            && field.after.as_deref() == Some("deploy --prod")
            && field.merge_outcome == Some(CloudSyncFieldMergeOutcome::Remote)
    }));
    assert!(items[0].fields.iter().any(|field| {
        field.label_key == "plugin.cloud_sync.diff_fields.description"
            && field.before.as_deref() == Some("local note")
            && field.after.as_deref() == Some("local note")
            && field.merge_outcome == Some(CloudSyncFieldMergeOutcome::Local)
    }));
}

#[test]
fn upload_field_diff_items_show_local_after_remote_before() {
    let preview = CloudSyncPendingPreview::Structured(StructuredPreview {
        remote_metadata: Default::default(),
        manifest: oxideterm_cloud_sync::create_manifest_base(
            "rev-1",
            "2026-06-12T00:00:00Z",
            "device",
            SyncScope::default(),
        ),
        connections_snapshot: None,
        forwards_snapshot: None,
        quick_commands_snapshot_json: Some(
            serde_json::to_string(&QuickCommandsSnapshot {
                version: 1,
                categories: Vec::new(),
                commands: vec![quick_command("cmd-1", "Deploy", "deploy --prod")],
                updated_at: 2,
            })
            .expect("remote quick commands"),
        ),
        serial_profiles_snapshot: None,
        base_connections_snapshot: None,
        base_forwards_snapshot: None,
        base_quick_commands_snapshot_json: None,
        base_serial_profiles_snapshot: None,
        sensitive_credentials_entry: None,
        sensitive_credentials_preview: None,
        app_settings_entries: Default::default(),
        app_settings_sections: Default::default(),
        plugin_settings_entries: Default::default(),
        plugin_settings_counts: Default::default(),
    });
    let local = CloudSyncLocalFieldDiffSnapshot {
        quick_commands: Some(QuickCommandsSnapshot {
            version: 1,
            categories: Vec::new(),
            commands: vec![quick_command("cmd-1", "Deploy", "deploy --staging")],
            updated_at: 3,
        }),
        ..CloudSyncLocalFieldDiffSnapshot::default()
    };

    let items = cloud_sync_upload_field_diff_items(&preview, &local, &SyncScope::default());

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, CloudSyncFieldDiffStatus::Modified);
    assert!(items[0].fields.iter().any(|field| {
        field.label_key == "plugin.cloud_sync.diff_fields.command"
            && field.before.as_deref() == Some("deploy --prod")
            && field.after.as_deref() == Some("deploy --staging")
    }));
}

fn test_snapshot(scope: SyncScope, current_state: StructuredLocalState) -> CloudSyncLocalSnapshot {
    CloudSyncLocalSnapshot {
        metadata: Default::default(),
        scope,
        dirty: StructuredDirtyInfo {
            current_state,
            dirty_sections: StructuredDirtySections::default(),
            has_dirty: true,
        },
        upload_units: 0,
        connections_record_count: 2,
        forwards_record_count: 1,
        quick_commands_record_count: 0,
        serial_profiles_record_count: 0,
        sensitive_credentials_record_count: 0,
    }
}

fn quick_command(id: &str, name: &str, command: &str) -> QuickCommand {
    QuickCommand {
        id: id.to_string(),
        name: name.to_string(),
        command: command.to_string(),
        category: "default".to_string(),
        description: None,
        host_pattern: None,
        created_at: 1,
        updated_at: 1,
    }
}
