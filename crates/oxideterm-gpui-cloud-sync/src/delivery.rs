// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync operation delivery adapters.
//!
//! The GPUI app owns scheduling and rendering, while this module owns the
//! request/response glue between the Cloud Sync service, local stores, rollback
//! backup encoding, keychain hints, and UI delivery messages.

use std::{collections::BTreeMap, sync::mpsc::Sender};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use oxideterm_cloud_sync::{
    CloudSyncSettings, MAX_ROLLBACK_BACKUP_BYTES, StructuredSectionRevisions,
    operation::{
        ApplyLegacyPreviewOutcome, ApplyStructuredPreviewOutcome, CloudSyncOperationService,
        LegacyPreview, StructuredPreview, UploadOptions, UploadOutcome,
    },
    progress::{CloudSyncProgress, CloudSyncProgressSink, CloudSyncProgressStage},
    secrets::{
        CloudSyncKeychainSecretProvider, CloudSyncSecretValue, SecretReadMode, get_action_secrets,
    },
    state::{CloudSyncRollbackBackup, CloudSyncRollbackBackupMetadata},
};
use oxideterm_connections::{
    ConnectionStore,
    oxide_file::{
        ImportConflictStrategy, OxideExportOptions, OxideFile, OxideForwardRecord,
        preview_oxide_import_with_progress,
    },
};
use oxideterm_forwarding::{ForwardType, ForwardingRegistry};
use oxideterm_settings::SettingsStore;

use crate::{
    CloudSyncPendingPreview, CloudSyncPreviewSelection, CloudSyncPreviewSource,
    cloud_sync_apply_total_units, cloud_sync_preview_summary, non_empty_secret,
};

#[derive(Debug)]
pub enum CloudSyncDelivery {
    Progress(CloudSyncProgress),
    RollbackBackupCreated(CloudSyncRollbackBackup),
    CheckFinished(CloudSyncActionResult<Option<oxideterm_cloud_sync::backend::RemoteMetadata>>),
    UploadFinished {
        action: CloudSyncUploadActionResult,
        automatic: bool,
    },
    UploadPreviewFinished(CloudSyncActionResult<CloudSyncPendingPreview>),
    PullPreviewFinished(CloudSyncActionResult<CloudSyncPendingPreview>),
    RestoreBackupPreviewFinished(CloudSyncActionResult<CloudSyncPendingPreview>),
    ApplyPreviewFinished(CloudSyncActionResult<CloudSyncApplyUiOutcome>),
}

#[derive(Debug)]
pub struct CloudSyncActionResult<T> {
    pub result: Result<T, String>,
    pub secret_hints: BTreeMap<String, bool>,
}

#[derive(Debug)]
pub struct CloudSyncUploadActionResult {
    pub result: Result<UploadOutcome, String>,
    pub remote_metadata: Option<oxideterm_cloud_sync::backend::RemoteMetadata>,
    pub revision_sequence_consumed: Option<u64>,
    pub secret_hints: BTreeMap<String, bool>,
}

#[derive(Debug)]
pub struct CloudSyncApplyUiOutcome {
    pub connection_store: ConnectionStore,
    pub settings_store: SettingsStore,
    pub outcome: CloudSyncApplyOutcome,
}

#[derive(Debug)]
pub enum CloudSyncApplyOutcome {
    Structured(ApplyStructuredPreviewOutcome),
    Legacy {
        preview: LegacyPreview,
        source: CloudSyncPreviewSource,
        selection: CloudSyncPreviewSelection,
        outcome: ApplyLegacyPreviewOutcome,
    },
}

pub async fn deliver_cloud_sync_check(
    tx: Sender<CloudSyncDelivery>,
    service: CloudSyncOperationService,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    skip_if_busy: bool,
) {
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let progress_tx = tx.clone();
    let mut progress = move |progress| {
        let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
    };
    let result = service
        .check_remote(
            &settings,
            &mut provider,
            skip_if_busy,
            false,
            Some(&mut progress),
        )
        .await
        .map_err(|error| error.to_string());
    send_action_result(tx, CloudSyncDelivery::CheckFinished, result, &provider);
}

#[allow(clippy::too_many_arguments)]
pub async fn deliver_cloud_sync_upload(
    tx: Sender<CloudSyncDelivery>,
    service: CloudSyncOperationService,
    connection_store: ConnectionStore,
    forwarding_registry: ForwardingRegistry,
    settings_store: SettingsStore,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    options: UploadOptions,
    automatic: bool,
) {
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let progress_tx = tx.clone();
    let mut progress = move |progress| {
        let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
    };
    let (result, remote_metadata, revision_sequence_consumed) = match service
        .upload_now(
            &connection_store,
            &forwarding_registry,
            &settings_store,
            &settings,
            &mut provider,
            options,
            Some(&mut progress),
        )
        .await
    {
        Ok(Some(outcome)) => (Ok(outcome), None, None),
        Ok(None) => return,
        Err(error) => (
            Err(error.to_string()),
            error.remote_metadata,
            error.revision_sequence_consumed,
        ),
    };
    let _ = tx.send(CloudSyncDelivery::UploadFinished {
        action: CloudSyncUploadActionResult {
            result,
            remote_metadata,
            revision_sequence_consumed,
            secret_hints: provider.hints().clone(),
        },
        automatic,
    });
}

pub async fn deliver_cloud_sync_upload_preview(
    tx: Sender<CloudSyncDelivery>,
    service: CloudSyncOperationService,
    connection_store: ConnectionStore,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    previous_remote_sections: Option<StructuredSectionRevisions>,
) {
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let progress_tx = tx.clone();
    let mut progress = move |progress| {
        let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
    };
    let result = match service
        .pull_structured_preview(
            &connection_store,
            &settings,
            &mut provider,
            previous_remote_sections.as_ref(),
            Some(&mut progress),
        )
        .await
    {
        Ok(Some(preview)) => Ok(CloudSyncPendingPreview::Structured(preview)),
        Ok(None) => service
            .pull_legacy_preview(
                &connection_store,
                &settings,
                &mut provider,
                settings.default_conflict_strategy.clone(),
                Some(&mut progress),
            )
            .await
            .map(|preview| CloudSyncPendingPreview::Legacy {
                preview,
                source: CloudSyncPreviewSource::Remote,
            }),
        Err(error) => Err(error),
    }
    .map_err(|error| error.to_string());
    send_action_result(
        tx,
        CloudSyncDelivery::UploadPreviewFinished,
        result,
        &provider,
    );
}

pub async fn deliver_cloud_sync_pull_preview(
    tx: Sender<CloudSyncDelivery>,
    service: CloudSyncOperationService,
    connection_store: ConnectionStore,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    previous_remote_sections: Option<StructuredSectionRevisions>,
) {
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let progress_tx = tx.clone();
    let mut progress = move |progress| {
        let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
    };
    let result = match service
        .pull_structured_preview(
            &connection_store,
            &settings,
            &mut provider,
            previous_remote_sections.as_ref(),
            Some(&mut progress),
        )
        .await
    {
        Ok(Some(preview)) => Ok(CloudSyncPendingPreview::Structured(preview)),
        Ok(None) => service
            .pull_legacy_preview(
                &connection_store,
                &settings,
                &mut provider,
                settings.default_conflict_strategy.clone(),
                Some(&mut progress),
            )
            .await
            .map(|preview| CloudSyncPendingPreview::Legacy {
                preview,
                source: CloudSyncPreviewSource::Remote,
            }),
        Err(error) => Err(error),
    }
    .map_err(|error| error.to_string());
    send_action_result(
        tx,
        CloudSyncDelivery::PullPreviewFinished,
        result,
        &provider,
    );
}

pub async fn deliver_cloud_sync_restore_backup_preview(
    tx: Sender<CloudSyncDelivery>,
    connection_store: ConnectionStore,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    backup: CloudSyncRollbackBackup,
) {
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let progress_tx = tx.clone();
    let mut progress = move |progress| {
        let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
    };
    progress(CloudSyncProgress {
        stage: CloudSyncProgressStage::Validating,
        current: 1.0,
        total: 2.0,
        message: None,
    });
    let result = match get_action_secrets(&settings, &mut provider, true, SecretReadMode::Prompt) {
        Ok(secrets) => {
            let password = secrets.sync_password.unwrap_or_default();
            if non_empty_secret(&password).is_none() {
                Err("missing_sync_password: cloud sync password is required".to_string())
            } else {
                preview_cloud_sync_rollback_backup(
                    &connection_store,
                    backup.clone(),
                    &password,
                    Some(&mut progress),
                )
                .map(|preview| CloudSyncPendingPreview::Legacy {
                    preview,
                    source: CloudSyncPreviewSource::Backup {
                        id: backup.id,
                        created_at: backup.created_at,
                    },
                })
                .map_err(|error| error.to_string())
            }
        }
        Err(error) => Err(error.to_string()),
    };
    progress(CloudSyncProgress {
        stage: CloudSyncProgressStage::Done,
        current: 2.0,
        total: 2.0,
        message: None,
    });
    send_action_result(
        tx,
        CloudSyncDelivery::RestoreBackupPreviewFinished,
        result,
        &provider,
    );
}

#[allow(clippy::too_many_arguments)]
pub async fn deliver_cloud_sync_apply_preview(
    tx: Sender<CloudSyncDelivery>,
    service: CloudSyncOperationService,
    mut connection_store: ConnectionStore,
    forwarding_registry: ForwardingRegistry,
    mut settings_store: SettingsStore,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    source_revision: Option<String>,
    preview: CloudSyncPendingPreview,
    selection: CloudSyncPreviewSelection,
    create_rollback_backup: bool,
) {
    let apply_total_units =
        cloud_sync_apply_total_units(&preview, &selection, create_rollback_backup);
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let progress_tx = tx.clone();
    let progress = move |progress| {
        let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
    };
    let Some(sync_password) = read_apply_sync_password(
        &tx,
        &settings,
        &mut provider,
        &preview,
        create_rollback_backup,
    ) else {
        return;
    };
    if create_rollback_backup {
        progress(CloudSyncProgress {
            stage: CloudSyncProgressStage::CreatingBackup,
            current: 0.1,
            total: apply_total_units,
            message: None,
        });
        match create_cloud_sync_rollback_backup(
            &connection_store,
            &forwarding_registry,
            &settings_store,
            source_revision,
            sync_password.as_ref().map(|password| password.as_str()),
        ) {
            Ok(Some(backup)) => {
                let _ = tx.send(CloudSyncDelivery::RollbackBackupCreated(backup));
            }
            Ok(None) => {}
            Err(error) => {
                send_action_result(
                    tx,
                    CloudSyncDelivery::ApplyPreviewFinished,
                    Err(error.to_string()),
                    &provider,
                );
                return;
            }
        }
        progress(CloudSyncProgress {
            stage: CloudSyncProgressStage::CreatingBackup,
            current: 1.0,
            total: apply_total_units,
            message: None,
        });
    }
    let mut apply_progress = |update: CloudSyncProgress| {
        let offset = if create_rollback_backup { 1.0 } else { 0.0 };
        progress(CloudSyncProgress {
            stage: update.stage,
            current: (offset + update.current).min(apply_total_units),
            total: apply_total_units,
            message: update.message,
        });
    };
    let result = match preview {
        CloudSyncPendingPreview::Structured(mut preview) => {
            filter_structured_preview_for_selection(&mut preview, &selection);
            service
                .apply_structured_preview(
                    &mut connection_store,
                    &forwarding_registry,
                    &mut settings_store,
                    &settings,
                    preview,
                    selection.structured_selection(),
                    selection.conflict_strategy.clone(),
                    sync_password.as_ref().map(|password| password.as_str()),
                    Some(&mut apply_progress),
                )
                .map(|outcome| {
                    CloudSyncApplyOutcome::Structured(
                        outcome.expect("cloud sync structured apply unexpectedly skipped"),
                    )
                })
        }
        CloudSyncPendingPreview::Legacy { preview, source } => {
            let summary = cloud_sync_preview_summary(&CloudSyncPendingPreview::Legacy {
                preview: preview.clone(),
                source: source.clone(),
            });
            service
                .apply_legacy_preview(
                    &mut connection_store,
                    &settings,
                    &preview,
                    sync_password.as_ref().map(|password| password.as_str()),
                    selection.effective_import_connections(&summary),
                    selection.selected_connection_names_for_import(&summary),
                    selection.import_forwards,
                    selection.conflict_strategy.clone(),
                    Some(&mut apply_progress),
                )
                .map(|outcome| CloudSyncApplyOutcome::Legacy {
                    preview,
                    source,
                    selection: selection.clone(),
                    outcome: outcome.expect("cloud sync legacy apply unexpectedly skipped"),
                })
        }
    }
    .map(|outcome| CloudSyncApplyUiOutcome {
        connection_store,
        settings_store,
        outcome,
    })
    .map_err(|error| error.to_string());
    send_action_result(
        tx,
        CloudSyncDelivery::ApplyPreviewFinished,
        result,
        &provider,
    );
}

fn filter_structured_preview_for_selection(
    preview: &mut StructuredPreview,
    selection: &CloudSyncPreviewSelection,
) {
    // Apply only selected structured records while preserving the downloaded preview metadata.
    if let Some(snapshot) = preview.connections_snapshot.as_mut() {
        snapshot
            .records
            .retain(|record| selection.selected_connection_ids.contains(&record.id));
    }
    if let Some(snapshot) = preview.forwards_snapshot.as_mut() {
        snapshot
            .records
            .retain(|record| selection.selected_forward_ids.contains(&record.id));
    }
    if let Some(json) = preview.quick_commands_snapshot_json.as_mut()
        && let Ok(mut snapshot) =
            serde_json::from_str::<oxideterm_quick_commands::QuickCommandsSnapshot>(json)
    {
        snapshot
            .commands
            .retain(|command| selection.selected_quick_command_ids.contains(&command.id));
        if let Ok(next_json) = serde_json::to_string(&snapshot) {
            *json = next_json;
        }
    }
    if let Some(snapshot) = preview.serial_profiles_snapshot.as_mut() {
        snapshot
            .records
            .retain(|profile| selection.selected_serial_profile_ids.contains(&profile.id));
    }
}

fn read_apply_sync_password(
    tx: &Sender<CloudSyncDelivery>,
    settings: &CloudSyncSettings,
    provider: &mut CloudSyncKeychainSecretProvider,
    preview: &CloudSyncPendingPreview,
    create_rollback_backup: bool,
) -> Option<Option<CloudSyncSecretValue>> {
    let apply_requires_password = match preview {
        CloudSyncPendingPreview::Structured(preview) => {
            !preview.app_settings_entries.is_empty() || !preview.plugin_settings_entries.is_empty()
        }
        CloudSyncPendingPreview::Legacy { .. } => true,
    };
    let needs_sync_password = apply_requires_password || create_rollback_backup;
    let secret_result = get_action_secrets(
        settings,
        provider,
        needs_sync_password,
        SecretReadMode::Prompt,
    );
    match (secret_result, needs_sync_password) {
        (Ok(secrets), true) => {
            let password = secrets.sync_password.unwrap_or_default();
            if non_empty_secret(&password).is_some() {
                Some(Some(password))
            } else {
                let _ = tx.send(CloudSyncDelivery::ApplyPreviewFinished(
                    CloudSyncActionResult {
                        result: Err(
                            "missing_sync_password: cloud sync password is required".to_string()
                        ),
                        secret_hints: provider.hints().clone(),
                    },
                ));
                None
            }
        }
        (Ok(_), false) => Some(None),
        (Err(error), _) => {
            let _ = tx.send(CloudSyncDelivery::ApplyPreviewFinished(
                CloudSyncActionResult {
                    result: Err(error.to_string()),
                    secret_hints: provider.hints().clone(),
                },
            ));
            None
        }
    }
}

fn send_action_result<T>(
    tx: Sender<CloudSyncDelivery>,
    wrap: impl FnOnce(CloudSyncActionResult<T>) -> CloudSyncDelivery,
    result: Result<T, String>,
    provider: &CloudSyncKeychainSecretProvider,
) {
    let _ = tx.send(wrap(CloudSyncActionResult {
        result,
        secret_hints: provider.hints().clone(),
    }));
}

fn preview_cloud_sync_rollback_backup(
    connection_store: &ConnectionStore,
    backup: CloudSyncRollbackBackup,
    password: &str,
    progress: Option<&mut dyn CloudSyncProgressSink>,
) -> anyhow::Result<LegacyPreview> {
    let bytes = BASE64.decode(backup.bytes_base64.as_bytes())?;
    let metadata = OxideFile::from_bytes(&bytes)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .metadata;
    let mut noop = |_| {};
    let progress = progress.unwrap_or(&mut noop);
    let preview = preview_oxide_import_with_progress(
        connection_store,
        &bytes,
        password,
        ImportConflictStrategy::Replace,
        |_stage, current, total| {
            let fraction = if total == 0 {
                0.0
            } else {
                (current as f64 / total as f64).clamp(0.0, 1.0)
            };
            progress.report(CloudSyncProgress {
                stage: CloudSyncProgressStage::PreviewingImport,
                current: (1.0 + fraction).min(2.0),
                total: 2.0,
                message: None,
            });
        },
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(LegacyPreview {
        remote_metadata: oxideterm_cloud_sync::backend::RemoteMetadata::default(),
        bytes,
        metadata,
        preview,
    })
}

fn create_cloud_sync_rollback_backup(
    connection_store: &ConnectionStore,
    forwarding_registry: &ForwardingRegistry,
    settings_store: &SettingsStore,
    source_revision: Option<String>,
    sync_password: Option<&str>,
) -> anyhow::Result<Option<CloudSyncRollbackBackup>> {
    let has_local_data = !connection_store.connections().is_empty()
        || !forwarding_registry.list_all_saved_forwards().is_empty();
    if !has_local_data {
        return Ok(None);
    }
    let Some(password) = sync_password.and_then(non_empty_secret) else {
        anyhow::bail!("missing_sync_password: cloud sync password is required");
    };
    let connection_ids = connection_store
        .connections()
        .iter()
        .map(|connection| connection.id.clone())
        .collect::<Vec<_>>();
    let selected_ids = connection_ids
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let app_settings_json = oxideterm_settings::export_oxide_settings_snapshot_json(
        settings_store.settings(),
        None,
        true,
    )?;
    let plugin_settings =
        oxideterm_cloud_sync::plugin_settings::load_plugin_settings(settings_store.path())
            .map_err(anyhow::Error::msg)?;
    let forwards = forwarding_registry
        .list_all_saved_forwards()
        .into_iter()
        .filter_map(|forward| {
            let owner_id = forward.owner_connection_id?;
            selected_ids
                .contains(&owner_id)
                .then(|| OxideForwardRecord {
                    id: Some(forward.id),
                    connection_id: owner_id,
                    forward_type: match forward.forward_type {
                        ForwardType::Local => "local".to_string(),
                        ForwardType::Remote => "remote".to_string(),
                        ForwardType::Dynamic => "dynamic".to_string(),
                    },
                    bind_address: forward.rule.bind_address,
                    bind_port: forward.rule.bind_port,
                    target_host: forward.rule.target_host,
                    target_port: forward.rule.target_port,
                    description: Some(forward.rule.description),
                    auto_start: forward.auto_start,
                })
        })
        .collect::<Vec<_>>();
    let bytes = oxideterm_connections::oxide_file::export_connections_to_oxide_with_progress(
        connection_store,
        &connection_ids,
        password,
        OxideExportOptions {
            description: Some("Oxide Cloud Sync rollback backup".to_string()),
            embed_keys: false,
            app_settings_json: Some(app_settings_json),
            plugin_settings,
            forwards,
            ..OxideExportOptions::default()
        },
        |_stage, _current, _total| {},
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if bytes.len() > MAX_ROLLBACK_BACKUP_BYTES {
        anyhow::bail!(
            "rollback_backup_too_large: local rollback backup is too large ({} > {})",
            bytes.len(),
            MAX_ROLLBACK_BACKUP_BYTES
        );
    }
    let metadata = OxideFile::from_bytes(&bytes)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .metadata;
    let preview = preview_oxide_import_with_progress(
        connection_store,
        &bytes,
        password,
        ImportConflictStrategy::Replace,
        |_stage, _current, _total| {},
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(Some(CloudSyncRollbackBackup {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: Utc::now().to_rfc3339(),
        source_revision,
        size_bytes: bytes.len(),
        bytes_base64: BASE64.encode(bytes),
        metadata: Some(CloudSyncRollbackBackupMetadata {
            num_connections: metadata.num_connections,
            connection_names: metadata.connection_names,
            has_app_settings: metadata
                .has_app_settings
                .unwrap_or(preview.has_app_settings),
            plugin_settings_count: preview.plugin_settings_count,
            forwards: preview.total_forwards,
            quick_commands: metadata.quick_commands_count.unwrap_or(0),
            serial_profiles: 0,
            sensitive_credentials: metadata.portable_secret_count.unwrap_or(0),
        }),
    }))
}
