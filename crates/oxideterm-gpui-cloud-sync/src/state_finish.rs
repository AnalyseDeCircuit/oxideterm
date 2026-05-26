// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync persisted-state transitions after backend operations finish.

use oxideterm_cloud_sync::{
    CloudSyncStatus, STRUCTURED_MANIFEST_FORMAT, StructuredDirtyInfo,
    build_manifest_section_revisions, compute_structured_dirty_sections, merge_structured_baseline,
    operation::{ApplyStructuredPreviewOutcome, LegacyPreview, UploadOutcome},
    service::CloudSyncLocalSnapshot,
    state::{
        CloudSyncConflictDetails, CloudSyncHistoryEntry, CloudSyncHistorySummary,
        CloudSyncPersistedState,
    },
};

use crate::{
    CloudSyncPendingPreview, CloudSyncPreviewSelection, CloudSyncPreviewSource,
    cloud_sync_preview_summary, has_cloud_sync_structured_conflict,
    history_summary_from_legacy_preview, history_summary_from_manifest,
    legacy_apply_covers_full_remote, merge_structured_remote_baseline,
    structured_apply_covers_full_remote,
};

pub fn persist_remote_metadata(
    state: &mut CloudSyncPersistedState,
    metadata: &oxideterm_cloud_sync::backend::RemoteMetadata,
) {
    state.remote_exists = metadata.exists;
    state.remote_format = metadata.format.clone();
    state.remote_section_revisions = metadata.section_revisions.clone();
    state.last_known_remote_revision = metadata.revision.clone();
    state.last_known_remote_etag = metadata.etag.clone();
    state.remote_updated_at = metadata.uploaded_at.clone();
    state.remote_device_id = metadata.device_id.clone();
}

pub fn finish_cloud_sync_upload_state(
    state: &mut CloudSyncPersistedState,
    outcome: &UploadOutcome,
) -> String {
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

pub fn finish_cloud_sync_automatic_upload_error_state(
    state: &mut CloudSyncPersistedState,
    raw_error: &str,
    display_error: String,
    history_summary: CloudSyncHistorySummary,
) {
    let remote_revision = state.last_known_remote_revision.clone();
    state.status = CloudSyncStatus::Error;
    state.last_error = Some(display_error.clone());
    if raw_error
        .trim_start()
        .starts_with("remote_changed_before_upload")
    {
        state.auto_upload_blocked_by_conflict = true;
        state.conflict_details = Some(CloudSyncConflictDetails {
            revision: state.last_known_remote_revision.clone(),
            device_id: state.remote_device_id.clone(),
            updated_at: state.remote_updated_at.clone(),
        });
    }
    state.append_history(CloudSyncHistoryEntry::new(
        "upload",
        history_summary,
        false,
        Some(display_error),
        remote_revision,
    ));
}

pub fn finish_cloud_sync_pull_preview_state(
    state: &mut CloudSyncPersistedState,
    preview: &CloudSyncPendingPreview,
) {
    match preview {
        CloudSyncPendingPreview::Structured(preview) => {
            persist_remote_metadata(state, &preview.remote_metadata);
        }
        CloudSyncPendingPreview::Legacy {
            preview,
            source: CloudSyncPreviewSource::Remote,
        } => {
            persist_remote_metadata(state, &preview.remote_metadata);
        }
        CloudSyncPendingPreview::Legacy {
            source: CloudSyncPreviewSource::Backup { .. },
            ..
        } => {}
    }
    state.status = CloudSyncStatus::Idle;
    state.last_error = None;
}

pub fn finish_structured_cloud_sync_apply_state(
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
        structured_apply_covers_full_remote(&outcome.manifest, &outcome.selection);
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
        dirty_after.has_dirty && !applied_full_remote && was_conflict_blocked;
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
    was_conflict_blocked && !applied_full_remote
}

pub fn finish_legacy_cloud_sync_apply_state(
    state: &mut CloudSyncPersistedState,
    preview: &LegacyPreview,
    source: &CloudSyncPreviewSource,
    selection: &CloudSyncPreviewSelection,
    local_snapshot: Option<&CloudSyncLocalSnapshot>,
    now: String,
) -> bool {
    let summary = cloud_sync_preview_summary(&CloudSyncPendingPreview::Legacy {
        preview: preview.clone(),
        source: source.clone(),
    });
    let was_conflict_blocked = state.auto_upload_blocked_by_conflict;
    let applied_full_remote = matches!(source, CloudSyncPreviewSource::Remote)
        && legacy_apply_covers_full_remote(&summary, selection);
    let should_trigger_upload_after = matches!(source, CloudSyncPreviewSource::Remote)
        && was_conflict_blocked
        && !applied_full_remote;
    if applied_full_remote {
        state.last_sync_at = Some(now);
        state.last_known_remote_revision = preview.remote_metadata.revision.clone();
        state.last_known_remote_etag = preview.remote_metadata.etag.clone();
    }
    state.status = CloudSyncStatus::Idle;
    state.last_error = None;
    if matches!(source, CloudSyncPreviewSource::Remote) {
        state.remote_format = preview.remote_metadata.format.clone();
        state.remote_section_revisions = preview.remote_metadata.section_revisions.clone();
        state.remote_updated_at = preview.remote_metadata.uploaded_at.clone();
        state.remote_device_id = preview.remote_metadata.device_id.clone();
        state.remote_exists = preview.remote_metadata.exists;
    }
    if let Some(snapshot) = local_snapshot {
        if applied_full_remote {
            state.last_synced_local_metadata = Some(snapshot.metadata.clone());
            state.last_synced_structured_state = Some(snapshot.dirty.current_state.clone());
        }
        state.local_dirty = !applied_full_remote && snapshot.dirty.has_dirty;
        state.local_dirty_sections = Some(snapshot.dirty.dirty_sections.clone());
    }
    state.auto_upload_blocked_by_conflict = should_trigger_upload_after;
    if !state.auto_upload_blocked_by_conflict {
        state.conflict_details = None;
    }
    let action = if source.is_backup() {
        "restore"
    } else {
        "pull"
    };
    state.append_history(CloudSyncHistoryEntry::new(
        action,
        history_summary_from_legacy_preview(preview),
        true,
        None,
        preview.remote_metadata.revision.clone(),
    ));
    should_trigger_upload_after
}

pub fn finish_cloud_sync_error_state(
    state: &mut CloudSyncPersistedState,
    action: &str,
    raw_error: &str,
    display_error: String,
    upload_history_summary: Option<CloudSyncHistorySummary>,
) {
    let remote_revision = state.last_known_remote_revision.clone();
    state.status = CloudSyncStatus::Error;
    state.last_error = Some(display_error.clone());
    if action == "upload"
        && raw_error
            .trim_start()
            .starts_with("remote_changed_before_upload")
    {
        state.auto_upload_blocked_by_conflict = true;
        state.conflict_details = Some(CloudSyncConflictDetails {
            revision: state.last_known_remote_revision.clone(),
            device_id: state.remote_device_id.clone(),
            updated_at: state.remote_updated_at.clone(),
        });
    }
    if let Some(history_summary) = upload_history_summary {
        state.append_history(CloudSyncHistoryEntry::new(
            action,
            history_summary,
            false,
            Some(display_error),
            remote_revision,
        ));
    }
}

pub fn finish_cloud_sync_check_state(
    state: &mut CloudSyncPersistedState,
    metadata: Option<&oxideterm_cloud_sync::backend::RemoteMetadata>,
    dirty: Option<&StructuredDirtyInfo>,
    conflict_error: Option<String>,
    now: String,
) {
    let previous_remote_revision = state.last_known_remote_revision.clone();
    let previous_remote_sections = state.last_synced_remote_sections.clone();
    if let Some(metadata) = metadata {
        let remote_updated = metadata.revision.as_ref().is_some_and(|revision| {
            previous_remote_revision
                .as_ref()
                .map_or(true, |previous| previous != revision)
        });
        persist_remote_metadata(state, metadata);
        if let Some(dirty) = dirty {
            state.local_dirty = dirty.has_dirty;
            state.local_dirty_sections = Some(dirty.dirty_sections.clone());
        }
        let conflict = dirty.is_some_and(|dirty| {
            if !dirty.has_dirty || !metadata.exists {
                return false;
            }
            if metadata.format.as_deref() != Some(STRUCTURED_MANIFEST_FORMAT) {
                return remote_updated;
            }
            has_cloud_sync_structured_conflict(
                &dirty.dirty_sections,
                state.remote_section_revisions.as_ref(),
                previous_remote_sections.as_ref(),
            )
        });
        state.status = if conflict {
            CloudSyncStatus::Conflict
        } else if remote_updated {
            CloudSyncStatus::RemoteUpdate
        } else {
            CloudSyncStatus::Idle
        };
        state.conflict_details = conflict.then(|| CloudSyncConflictDetails {
            revision: state.last_known_remote_revision.clone(),
            device_id: state.remote_device_id.clone(),
            updated_at: state.remote_updated_at.clone(),
        });
        state.auto_upload_blocked_by_conflict = conflict;
        state.last_error = conflict.then(|| conflict_error.unwrap_or_default());
    } else {
        state.status = CloudSyncStatus::Idle;
        state.last_error = None;
    }
    state.last_check_at = Some(now);
}
