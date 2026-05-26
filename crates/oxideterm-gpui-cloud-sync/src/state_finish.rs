// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync persisted-state transitions after backend operations finish.

use oxideterm_cloud_sync::{
    CloudSyncStatus, STRUCTURED_MANIFEST_FORMAT, StructuredDirtyInfo,
    operation::{ApplyStructuredPreviewOutcome, LegacyPreview, UploadOutcome},
    service::CloudSyncLocalSnapshot,
    state::{
        CloudSyncConflictDetails, CloudSyncHistoryEntry, CloudSyncHistorySummary,
        CloudSyncPersistedState,
    },
    state_transitions::{
        LegacyApplyStateInput, finish_legacy_apply_state, finish_structured_apply_state,
        finish_upload_state,
    },
};

use crate::{
    CloudSyncPendingPreview, CloudSyncPreviewSelection, CloudSyncPreviewSource,
    cloud_sync_preview_summary, has_cloud_sync_structured_conflict,
    history_summary_from_legacy_preview, legacy_apply_covers_full_remote,
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
    finish_upload_state(state, outcome)
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
    finish_structured_apply_state(state, outcome, local_snapshot, now)
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
    let applied_full_remote = matches!(source, CloudSyncPreviewSource::Remote)
        && legacy_apply_covers_full_remote(&summary, selection);
    finish_legacy_apply_state(
        state,
        LegacyApplyStateInput {
            remote_metadata: &preview.remote_metadata,
            history_summary: history_summary_from_legacy_preview(preview),
            local_snapshot,
            now,
            applied_full_remote,
            is_remote_source: matches!(source, CloudSyncPreviewSource::Remote),
            is_backup_source: source.is_backup(),
        },
    )
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
