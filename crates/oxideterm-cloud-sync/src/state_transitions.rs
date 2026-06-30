// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared Cloud Sync persisted-state transitions.
//!
//! These functions keep backend operation results independent from GPUI or CLI
//! delivery code, so every caller records the same baselines, history, and
//! conflict-clear semantics after a successful operation.

use crate::{
    CLOUD_SYNC_PLUGIN_ID, CloudSyncStatus, StructuredApplySelection, StructuredManifest,
    StructuredSectionRevisions,
    backend::RemoteMetadata,
    build_manifest_section_revisions, compute_structured_dirty_sections, merge_structured_baseline,
    operation::{ApplyStructuredPreviewOutcome, UploadOutcome},
    service::CloudSyncLocalSnapshot,
    state::{CloudSyncHistoryEntry, CloudSyncHistorySummary, CloudSyncPersistedState},
};

pub fn history_summary_from_manifest(manifest: &StructuredManifest) -> CloudSyncHistorySummary {
    CloudSyncHistorySummary {
        connections: manifest
            .sections
            .connections
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        forwards: manifest
            .sections
            .forwards
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        quick_commands: manifest
            .sections
            .quick_commands
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        serial_profiles: manifest
            .sections
            .serial_profiles
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        raw_tcp_profiles: manifest
            .sections
            .raw_tcp_profiles
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        raw_udp_profiles: manifest
            .sections
            .raw_udp_profiles
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        sensitive_credentials: manifest
            .sections
            .sensitive_credentials
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        has_app_settings: !manifest.sections.app_settings.is_empty(),
        plugin_settings_count: manifest.sections.plugin_settings.len(),
    }
}

pub fn finish_upload_state(state: &mut CloudSyncPersistedState, outcome: &UploadOutcome) -> String {
    let remote_sections = build_manifest_section_revisions(&outcome.manifest);
    let revision = outcome.manifest.revision.clone();
    let uploaded_at = outcome.manifest.uploaded_at.clone();

    state.status = CloudSyncStatus::Idle;
    state.last_error = None;
    state.revision_seq = state.revision_seq.max(outcome.revision_sequence);
    state.last_sync_at = Some(uploaded_at.clone());
    state.last_upload_at = Some(uploaded_at);
    state.last_known_remote_revision = Some(revision.clone());
    state.last_known_remote_etag = outcome.etag.clone();
    state.remote_format = Some(outcome.manifest.format.clone());
    state.remote_section_revisions = Some(remote_sections.clone());
    state.remote_updated_at = Some(outcome.manifest.uploaded_at.clone());
    state.remote_device_id = Some(outcome.manifest.device_id.clone());
    state.remote_exists = true;
    state.last_synced_local_metadata = Some(outcome.local_snapshot.metadata.clone());
    state.last_synced_structured_state = Some(outcome.local_snapshot.dirty.current_state.clone());
    state.last_synced_remote_sections = Some(remote_sections);
    state.local_dirty = false;
    state.local_dirty_sections = Some(outcome.local_snapshot.dirty.dirty_sections.clone());
    state.auto_upload_blocked_by_conflict = false;
    state.conflict_details = None;
    state.append_history(CloudSyncHistoryEntry::new(
        "upload",
        history_summary_from_manifest(&outcome.manifest),
        true,
        None,
        Some(revision.clone()),
    ));

    revision
}

pub fn structured_apply_covers_full_remote(
    manifest: &StructuredManifest,
    selection: &StructuredApplySelection,
) -> bool {
    (manifest.sections.connections.is_none() || selection.connections)
        && (manifest.sections.forwards.is_none() || selection.forwards)
        && manifest
            .sections
            .app_settings
            .keys()
            .all(|section_id| selection.app_settings_sections.contains(section_id))
        && manifest
            .sections
            .plugin_settings
            .keys()
            .filter(|plugin_id| plugin_id.as_str() != CLOUD_SYNC_PLUGIN_ID)
            .all(|plugin_id| selection.plugin_ids.contains(plugin_id))
}

pub fn merge_structured_remote_baseline(
    previous: Option<&StructuredSectionRevisions>,
    next: &StructuredSectionRevisions,
    selection: &StructuredApplySelection,
) -> StructuredSectionRevisions {
    let mut merged = previous.cloned().unwrap_or_default();
    if selection.connections {
        merged.connections = next.connections.clone();
    }
    if selection.forwards {
        merged.forwards = next.forwards.clone();
    }
    for section_id in &selection.app_settings_sections {
        if let Some(revision) = next.app_settings.get(section_id) {
            merged
                .app_settings
                .insert(section_id.clone(), revision.clone());
        }
    }
    for plugin_id in &selection.plugin_ids {
        if let Some(revision) = next.plugin_settings.get(plugin_id) {
            merged
                .plugin_settings
                .insert(plugin_id.clone(), revision.clone());
        }
    }
    merged
}

pub fn finish_structured_apply_state(
    state: &mut CloudSyncPersistedState,
    outcome: &ApplyStructuredPreviewOutcome,
    local_snapshot: &CloudSyncLocalSnapshot,
    now: String,
) -> bool {
    let remote_sections = build_manifest_section_revisions(&outcome.manifest);
    let previous_local_baseline = state.last_synced_structured_state.clone();
    let previous_remote_baseline = state.last_synced_remote_sections.clone();
    let was_conflict_blocked = state.auto_upload_blocked_by_conflict;
    let applied_full_remote =
        structured_apply_covers_full_remote(&outcome.manifest, &outcome.selection)
            && !outcome.requires_upload_after_merge;
    let next_local_baseline = merge_structured_baseline(
        previous_local_baseline.as_ref(),
        &local_snapshot.dirty.current_state,
        &outcome.selection,
    );
    let next_remote_baseline = merge_structured_remote_baseline(
        previous_remote_baseline.as_ref(),
        &remote_sections,
        &outcome.selection,
    );
    let dirty_after = compute_structured_dirty_sections(
        &local_snapshot.metadata,
        Some(&next_local_baseline),
        &local_snapshot.scope,
    );

    state.status = CloudSyncStatus::Idle;
    state.last_error = None;
    state.last_sync_at = Some(now);
    state.last_known_remote_revision = if applied_full_remote {
        Some(outcome.manifest.revision.clone())
    } else {
        state.last_known_remote_revision.clone()
    };
    state.last_known_remote_etag = if applied_full_remote {
        outcome.remote_metadata.etag.clone()
    } else {
        state.last_known_remote_etag.clone()
    };
    state.remote_format = Some(outcome.manifest.format.clone());
    state.remote_section_revisions = Some(remote_sections.clone());
    state.remote_updated_at = Some(outcome.manifest.uploaded_at.clone());
    state.remote_device_id = Some(outcome.manifest.device_id.clone());
    state.remote_exists = true;
    state.last_synced_local_metadata = Some(local_snapshot.metadata.clone());
    state.last_synced_structured_state = Some(next_local_baseline);
    state.last_synced_remote_sections = Some(next_remote_baseline);
    state.local_dirty = dirty_after.has_dirty;
    state.local_dirty_sections = Some(dirty_after.dirty_sections.clone());
    state.auto_upload_blocked_by_conflict =
        (dirty_after.has_dirty && !applied_full_remote && was_conflict_blocked)
            || outcome.requires_upload_after_merge;
    if !state.auto_upload_blocked_by_conflict {
        state.conflict_details = None;
    }
    state.append_history(CloudSyncHistoryEntry::new(
        "pull",
        outcome.content_summary.clone(),
        true,
        None,
        Some(outcome.manifest.revision.clone()),
    ));

    (was_conflict_blocked && !applied_full_remote) || outcome.requires_upload_after_merge
}

pub struct LegacyApplyStateInput<'a> {
    pub remote_metadata: &'a RemoteMetadata,
    pub history_summary: CloudSyncHistorySummary,
    pub local_snapshot: Option<&'a CloudSyncLocalSnapshot>,
    pub now: String,
    pub applied_full_remote: bool,
    pub is_remote_source: bool,
    pub is_backup_source: bool,
}

pub fn finish_legacy_apply_state(
    state: &mut CloudSyncPersistedState,
    input: LegacyApplyStateInput<'_>,
) -> bool {
    let was_conflict_blocked = state.auto_upload_blocked_by_conflict;
    let should_trigger_upload_after =
        input.is_remote_source && was_conflict_blocked && !input.applied_full_remote;

    if input.applied_full_remote {
        state.last_sync_at = Some(input.now);
        state.last_known_remote_revision = input.remote_metadata.revision.clone();
        state.last_known_remote_etag = input.remote_metadata.etag.clone();
    }
    state.status = CloudSyncStatus::Idle;
    state.last_error = None;
    if input.is_remote_source {
        state.remote_format = input.remote_metadata.format.clone();
        state.remote_section_revisions = input.remote_metadata.section_revisions.clone();
        state.remote_updated_at = input.remote_metadata.uploaded_at.clone();
        state.remote_device_id = input.remote_metadata.device_id.clone();
        state.remote_exists = input.remote_metadata.exists;
    }
    if let Some(snapshot) = input.local_snapshot {
        if input.applied_full_remote {
            state.last_synced_local_metadata = Some(snapshot.metadata.clone());
            state.last_synced_structured_state = Some(snapshot.dirty.current_state.clone());
        }
        state.local_dirty = !input.applied_full_remote && snapshot.dirty.has_dirty;
        state.local_dirty_sections = Some(snapshot.dirty.dirty_sections.clone());
    }
    state.auto_upload_blocked_by_conflict = should_trigger_upload_after;
    if !state.auto_upload_blocked_by_conflict {
        state.conflict_details = None;
    }
    let action = if input.is_backup_source {
        "restore"
    } else {
        "pull"
    };
    state.append_history(CloudSyncHistoryEntry::new(
        action,
        input.history_summary,
        true,
        None,
        input.remote_metadata.revision.clone(),
    ));

    should_trigger_upload_after
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        CloudSyncStatus, StructuredDirtyInfo, StructuredDirtySections, StructuredLocalState,
        StructuredObjectEntry,
        backend::RemoteMetadata,
        create_manifest_base,
        operation::{ApplyStructuredPreviewOutcome, UploadOutcome},
        service::{CloudSyncApplyOutcome, CloudSyncLocalSnapshot},
        state::{CloudSyncHistorySummary, CloudSyncPersistedState},
    };

    use super::{
        LegacyApplyStateInput, finish_legacy_apply_state, finish_structured_apply_state,
        finish_upload_state,
    };

    #[test]
    fn finish_upload_state_records_remote_baseline_and_clears_conflict() {
        let mut manifest = create_manifest_base(
            "rev-2",
            "2026-05-27T00:00:00Z",
            "cli-device",
            Default::default(),
        );
        manifest.sections.connections = Some(StructuredObjectEntry {
            revision: "conn-rev-2".to_string(),
            path: "objects/connections.json".to_string(),
            record_count: Some(3),
            content_type: "application/json".to_string(),
        });
        manifest.sections.app_settings.insert(
            "appearance".to_string(),
            StructuredObjectEntry {
                revision: "appearance-rev-2".to_string(),
                path: "objects/app-settings/appearance.json".to_string(),
                record_count: None,
                content_type: "application/json".to_string(),
            },
        );

        let local_state = StructuredLocalState {
            connections: Some("conn-rev-2".to_string()),
            forwards: None,
            quick_commands: None,
            serial_profiles: None,
            raw_tcp_profiles: None,
            raw_udp_profiles: None,
            sensitive_credentials: None,
            app_settings: BTreeMap::from([(
                "appearance".to_string(),
                Some("appearance-rev-2".to_string()),
            )]),
            plugin_settings: BTreeMap::new(),
        };
        let snapshot = CloudSyncLocalSnapshot {
            metadata: Default::default(),
            scope: Default::default(),
            dirty: StructuredDirtyInfo {
                current_state: local_state.clone(),
                dirty_sections: StructuredDirtySections {
                    connections: true,
                    ..Default::default()
                },
                has_dirty: true,
            },
            upload_units: 2,
            connections_record_count: 3,
            forwards_record_count: 0,
            quick_commands_record_count: 0,
            serial_profiles_record_count: 0,
            raw_tcp_profiles_record_count: 0,
            raw_udp_profiles_record_count: 0,
            sensitive_credentials_record_count: 0,
        };
        let outcome = UploadOutcome {
            revision: "rev-2".to_string(),
            revision_sequence: 7,
            etag: Some("etag-2".to_string()),
            local_snapshot: snapshot,
            manifest,
            created_remote_id: None,
        };
        let mut state = CloudSyncPersistedState {
            status: CloudSyncStatus::Conflict,
            auto_upload_blocked_by_conflict: true,
            conflict_details: Some(Default::default()),
            ..Default::default()
        };

        let revision = finish_upload_state(&mut state, &outcome);

        assert_eq!(revision, "rev-2");
        assert_eq!(state.status, CloudSyncStatus::Idle);
        assert!(!state.auto_upload_blocked_by_conflict);
        assert!(state.conflict_details.is_none());
        assert_eq!(state.last_synced_structured_state, Some(local_state));
        assert_eq!(
            state
                .last_synced_remote_sections
                .as_ref()
                .and_then(|sections| sections.connections.as_deref()),
            Some("conn-rev-2")
        );
        assert_eq!(
            state
                .remote_section_revisions
                .as_ref()
                .and_then(|sections| sections.app_settings.get("appearance"))
                .map(String::as_str),
            Some("appearance-rev-2")
        );
        assert_eq!(state.sync_history.len(), 1);
        assert_eq!(state.sync_history[0].summary.connections, 3);
    }

    #[test]
    fn finish_structured_apply_merges_only_selected_remote_sections() {
        let mut manifest = create_manifest_base(
            "rev-remote",
            "2026-05-27T01:00:00Z",
            "remote-device",
            Default::default(),
        );
        manifest.sections.connections = Some(StructuredObjectEntry {
            revision: "conn-remote".to_string(),
            path: "objects/connections.json".to_string(),
            record_count: Some(2),
            content_type: "application/json".to_string(),
        });
        manifest.sections.app_settings.insert(
            "appearance".to_string(),
            StructuredObjectEntry {
                revision: "appearance-remote".to_string(),
                path: "objects/app-settings/appearance.json".to_string(),
                record_count: None,
                content_type: "application/json".to_string(),
            },
        );

        let local_state = StructuredLocalState {
            connections: Some("conn-remote".to_string()),
            forwards: None,
            quick_commands: None,
            serial_profiles: None,
            raw_tcp_profiles: None,
            raw_udp_profiles: None,
            sensitive_credentials: None,
            app_settings: BTreeMap::from([(
                "appearance".to_string(),
                Some("appearance-remote".to_string()),
            )]),
            plugin_settings: BTreeMap::new(),
        };
        let snapshot = CloudSyncLocalSnapshot {
            metadata: Default::default(),
            scope: Default::default(),
            dirty: StructuredDirtyInfo {
                current_state: local_state,
                dirty_sections: Default::default(),
                has_dirty: false,
            },
            upload_units: 0,
            connections_record_count: 2,
            forwards_record_count: 0,
            quick_commands_record_count: 0,
            serial_profiles_record_count: 0,
            raw_tcp_profiles_record_count: 0,
            raw_udp_profiles_record_count: 0,
            sensitive_credentials_record_count: 0,
        };
        let outcome = ApplyStructuredPreviewOutcome {
            local_snapshot: snapshot.clone(),
            applied: CloudSyncApplyOutcome {
                connections: None,
                forwards: None,
                quick_commands_applied: 0,
                serial_profiles_applied: 0,
                raw_tcp_profiles_applied: 0,
                raw_udp_profiles_applied: 0,
                app_settings_applied: 0,
                plugin_settings_applied: 0,
            },
            sensitive_credentials_envelope: None,
            content_summary: Default::default(),
            manifest,
            remote_metadata: Default::default(),
            selection: crate::StructuredApplySelection {
                connections: true,
                forwards: false,
                quick_commands: false,
                serial_profiles: false,
                raw_tcp_profiles: false,
                raw_udp_profiles: false,
                sensitive_credentials: false,
                app_settings_sections: Vec::new(),
                plugin_ids: Vec::new(),
            },
            requires_upload_after_merge: false,
        };
        let mut state = CloudSyncPersistedState::default();

        let should_upload_after = finish_structured_apply_state(
            &mut state,
            &outcome,
            &snapshot,
            "2026-05-27T02:00:00Z".to_string(),
        );

        assert!(!should_upload_after);
        assert_eq!(
            state
                .last_synced_remote_sections
                .as_ref()
                .and_then(|sections| sections.connections.as_deref()),
            Some("conn-remote")
        );
        assert!(
            state
                .last_synced_remote_sections
                .as_ref()
                .and_then(|sections| sections.app_settings.get("appearance"))
                .is_none()
        );
        assert!(state.last_known_remote_revision.is_none());
        assert_eq!(state.sync_history[0].action, "pull");
    }

    #[test]
    fn finish_legacy_apply_preserves_conflict_when_remote_is_partial() {
        let snapshot = CloudSyncLocalSnapshot {
            metadata: Default::default(),
            scope: Default::default(),
            dirty: StructuredDirtyInfo {
                current_state: Default::default(),
                dirty_sections: StructuredDirtySections {
                    connections: true,
                    ..Default::default()
                },
                has_dirty: true,
            },
            upload_units: 0,
            connections_record_count: 1,
            forwards_record_count: 0,
            quick_commands_record_count: 0,
            serial_profiles_record_count: 0,
            raw_tcp_profiles_record_count: 0,
            raw_udp_profiles_record_count: 0,
            sensitive_credentials_record_count: 0,
        };
        let metadata = RemoteMetadata {
            exists: true,
            revision: Some("legacy-rev".to_string()),
            etag: Some("legacy-etag".to_string()),
            uploaded_at: Some("2026-05-27T03:00:00Z".to_string()),
            ..Default::default()
        };
        let mut state = CloudSyncPersistedState {
            auto_upload_blocked_by_conflict: true,
            ..Default::default()
        };

        let should_upload_after = finish_legacy_apply_state(
            &mut state,
            LegacyApplyStateInput {
                remote_metadata: &metadata,
                history_summary: CloudSyncHistorySummary {
                    connections: 1,
                    ..Default::default()
                },
                local_snapshot: Some(&snapshot),
                now: "2026-05-27T04:00:00Z".to_string(),
                applied_full_remote: false,
                is_remote_source: true,
                is_backup_source: false,
            },
        );

        assert!(should_upload_after);
        assert!(state.auto_upload_blocked_by_conflict);
        assert!(state.last_known_remote_revision.is_none());
        assert_eq!(
            state.remote_updated_at.as_deref(),
            Some("2026-05-27T03:00:00Z")
        );
        assert_eq!(state.sync_history[0].action, "pull");
    }
}
