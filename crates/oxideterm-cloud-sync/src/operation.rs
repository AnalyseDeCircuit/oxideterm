// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use oxideterm_connections::{
    ConnectionStore, SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    oxide_file::{
        AppSettingsSectionPreview, ImportConflictStrategy, ImportPreview, ImportResultEnvelope,
        OxideExportOptions, OxideFile, OxideImportOptions, OxideMetadata,
        apply_oxide_import_with_options_with_progress, export_connections_to_oxide_with_progress,
        preflight_export, preview_oxide_import_with_progress,
    },
};
use oxideterm_forwarding::{ForwardingRegistry, SavedForwardsSyncSnapshot};
use oxideterm_settings::{SettingsStore, export_oxide_settings_snapshot_json};

use crate::{
    CloudSyncSettings, ConflictStrategy, RawSyncScope, STRUCTURED_MANIFEST_CONTENT_TYPE,
    STRUCTURED_MANIFEST_FORMAT, StructuredApplySelection, StructuredLocalState, StructuredManifest,
    StructuredManifestSections, StructuredObjectEntry, StructuredSectionRevisions,
    backend::{CloudSyncBackend, RemoteMetadata},
    connections_object_path, forwards_object_path,
    progress::{
        CloudSyncProgressSink, CloudSyncProgressStage, report_fractional_progress, report_progress,
    },
    revision_id,
    secrets::{CloudSyncSecretProvider, SecretReadMode, get_action_secrets},
    service::{
        CloudSyncApplyOutcome, CloudSyncLocalSnapshot, apply_structured_snapshots,
        build_local_snapshot,
    },
    state::CloudSyncHistorySummary,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncOperationKind {
    Check,
    Upload,
    Pull,
    ApplyPreview,
}

#[derive(Clone, Debug, Default)]
pub struct CloudSyncOperationGuard {
    active: Arc<Mutex<Option<CloudSyncOperationKind>>>,
}

impl CloudSyncOperationGuard {
    pub fn begin(
        &self,
        kind: CloudSyncOperationKind,
        skip_if_busy: bool,
    ) -> Result<Option<CloudSyncOperationPermit>> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.is_some() {
            if skip_if_busy {
                return Ok(None);
            }
            bail!("operation_in_progress: another cloud sync operation is already running");
        }
        *active = Some(kind);
        Ok(Some(CloudSyncOperationPermit {
            guard: self.clone(),
            kind,
        }))
    }

    fn finish(&self, kind: CloudSyncOperationKind) {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *active == Some(kind) {
            *active = None;
        }
    }
}

#[derive(Debug)]
pub struct CloudSyncOperationPermit {
    guard: CloudSyncOperationGuard,
    kind: CloudSyncOperationKind,
}

impl Drop for CloudSyncOperationPermit {
    fn drop(&mut self) {
        self.guard.finish(self.kind);
    }
}

#[derive(Clone, Debug)]
pub struct CloudSyncOperationService {
    backend: CloudSyncBackend,
    guard: CloudSyncOperationGuard,
}

impl Default for CloudSyncOperationService {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudSyncOperationService {
    pub fn new() -> Self {
        Self {
            backend: CloudSyncBackend::new(),
            guard: CloudSyncOperationGuard::default(),
        }
    }

    pub async fn check_remote(
        &self,
        settings: &CloudSyncSettings,
        secret_provider: &mut impl CloudSyncSecretProvider,
        skip_if_busy: bool,
        silent_secrets: bool,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<Option<RemoteMetadata>> {
        let Some(_permit) = self
            .guard
            .begin(CloudSyncOperationKind::Check, skip_if_busy)?
        else {
            return Ok(None);
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let secrets = get_action_secrets(
            settings,
            secret_provider,
            false,
            if silent_secrets {
                SecretReadMode::Silent
            } else {
                SecretReadMode::Prompt
            },
        )?;
        report_progress(progress, CloudSyncProgressStage::FetchMetadata, 1, 2);
        let metadata = self
            .backend
            .fetch_remote_metadata(settings, &secrets)
            .await?;
        report_progress(progress, CloudSyncProgressStage::Done, 2, 2);
        Ok(Some(metadata))
    }

    pub async fn upload_now(
        &self,
        connection_store: &ConnectionStore,
        forwarding_registry: &ForwardingRegistry,
        settings_store: &SettingsStore,
        settings: &CloudSyncSettings,
        secret_provider: &mut impl CloudSyncSecretProvider,
        options: UploadOptions,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> std::result::Result<Option<UploadOutcome>, CloudSyncUploadError> {
        let Some(_permit) = self
            .guard
            .begin(CloudSyncOperationKind::Upload, options.skip_if_busy)?
        else {
            return Ok(None);
        };

        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let local_snapshot = build_local_snapshot(
            connection_store,
            forwarding_registry,
            settings_store,
            options.last_synced_structured_state.as_ref(),
            options.raw_sync_scope.as_ref(),
        )?;
        let requires_password =
            local_snapshot.scope.sync_app_settings || local_snapshot.scope.sync_plugin_settings;
        let secrets = get_action_secrets(
            settings,
            secret_provider,
            requires_password,
            if options.automatic {
                SecretReadMode::Silent
            } else {
                SecretReadMode::Prompt
            },
        )?;
        if requires_password
            && secrets
                .sync_password
                .as_deref()
                .unwrap_or_default()
                .is_empty()
        {
            return Err(
                anyhow::anyhow!("missing_sync_password: cloud sync password is required").into(),
            );
        }

        let export_units = local_snapshot.upload_units;
        let upload_units = export_units + 1;
        let total = 4 + export_units + upload_units;
        report_progress(progress, CloudSyncProgressStage::FetchMetadata, 1, total);
        let remote_metadata = self
            .backend
            .fetch_remote_metadata(settings, &secrets)
            .await?;
        if !options.force && remote_metadata.exists {
            if let Err(error) = ensure_no_remote_conflict(
                &local_snapshot,
                &remote_metadata,
                options.previous_remote_revision.as_deref(),
                options.previous_remote_sections.as_ref(),
            ) {
                return Err(CloudSyncUploadError {
                    message: error.to_string(),
                    remote_metadata: Some(remote_metadata),
                    revision_sequence_consumed: None,
                });
            }
        }

        report_progress(progress, CloudSyncProgressStage::Preflight, 2, total);
        if local_snapshot.scope.sync_connections {
            let connection_ids = connection_store
                .connections()
                .iter()
                .map(|connection| connection.id.clone())
                .collect::<Vec<_>>();
            let preflight = preflight_export(connection_store, &connection_ids, false, 0);
            if !preflight.can_export {
                return Err(anyhow::anyhow!("preflight_failed: export preflight failed").into());
            }
        }
        let revision = revision_id(Utc::now(), &options.device_id, options.revision_sequence);
        let uploaded_at = Utc::now().to_rfc3339();
        report_progress(progress, CloudSyncProgressStage::Exporting, 2, total);
        let plan = self
            .build_structured_upload_plan(
                connection_store,
                forwarding_registry,
                settings_store,
                &local_snapshot,
                &revision,
                &uploaded_at,
                &options.device_id,
                secrets.sync_password.as_deref(),
                progress,
                total,
            )
            .await
            .map_err(|error| upload_error_after_revision(error, options.revision_sequence))?;

        let mut completed_uploads = 0usize;
        report_progress(
            progress,
            CloudSyncProgressStage::UploadingBlob,
            2 + export_units,
            total,
        );
        for object in &plan.objects {
            self.backend
                .write_remote_object(
                    settings,
                    &secrets,
                    &object.path,
                    object.bytes.clone(),
                    Some(&object.content_type),
                )
                .await
                .map_err(|error| upload_error_after_revision(error, options.revision_sequence))?;
            completed_uploads += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::UploadingBlob,
                2 + export_units + completed_uploads,
                total,
            );
        }

        let metadata_write = self
            .backend
            .write_remote_metadata(
                settings,
                &secrets,
                &serde_json::to_value(&plan.manifest).map_err(|error| {
                    upload_error_after_revision(error, options.revision_sequence)
                })?,
            )
            .await
            .map_err(|error| upload_error_after_revision(error, options.revision_sequence))?;
        completed_uploads += 1;
        report_progress(
            progress,
            CloudSyncProgressStage::UploadingBlob,
            2 + export_units + completed_uploads,
            total,
        );
        report_progress(progress, CloudSyncProgressStage::Done, total, total);

        Ok(Some(UploadOutcome {
            revision,
            revision_sequence: options.revision_sequence,
            etag: metadata_write.etag,
            local_snapshot,
            manifest: plan.manifest,
        }))
    }

    pub async fn download_remote_snapshot(
        &self,
        settings: &CloudSyncSettings,
        secret_provider: &mut impl CloudSyncSecretProvider,
        include_sync_password: bool,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<crate::backend::RemoteSnapshotDownload> {
        let Some(_permit) = self.guard.begin(CloudSyncOperationKind::Pull, false)? else {
            unreachable!();
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let secrets = get_action_secrets(
            settings,
            secret_provider,
            include_sync_password,
            SecretReadMode::Prompt,
        )?;
        report_progress(progress, CloudSyncProgressStage::Downloading, 1, 2);
        let remote = self
            .backend
            .download_remote_snapshot(settings, &secrets)
            .await?;
        report_progress(progress, CloudSyncProgressStage::Done, 2, 2);
        Ok(remote)
    }

    pub async fn pull_structured_preview(
        &self,
        connection_store: &ConnectionStore,
        settings: &CloudSyncSettings,
        secret_provider: &mut impl CloudSyncSecretProvider,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<Option<StructuredPreview>> {
        let Some(_permit) = self.guard.begin(CloudSyncOperationKind::Pull, false)? else {
            unreachable!();
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let metadata_secrets =
            get_action_secrets(settings, secret_provider, false, SecretReadMode::Prompt)?;
        report_progress(progress, CloudSyncProgressStage::FetchMetadata, 1, 4);
        let metadata = self
            .backend
            .fetch_remote_metadata(settings, &metadata_secrets)
            .await?;
        if !metadata.exists {
            bail!("remote_not_found: no remote snapshot found");
        }
        if metadata.format.as_deref() != Some(STRUCTURED_MANIFEST_FORMAT) {
            return Ok(None);
        }
        let needs_password = metadata
            .sections
            .as_ref()
            .and_then(|sections| sections.get("appSettings"))
            .and_then(|value| value.as_object())
            .is_some_and(|entries| !entries.is_empty())
            || metadata
                .sections
                .as_ref()
                .and_then(|sections| sections.get("pluginSettings"))
                .and_then(|value| value.as_object())
                .is_some_and(|entries| !entries.is_empty());
        let sync_password = if needs_password {
            let secrets =
                get_action_secrets(settings, secret_provider, true, SecretReadMode::Prompt)?;
            Some(required_sync_password(secrets.sync_password.as_deref())?.to_string())
        } else {
            None
        };

        let manifest = manifest_from_metadata(&metadata)
            .context("failed to decode structured cloud sync manifest")?;
        let encrypted_entry_count =
            manifest.sections.app_settings.len() + manifest.sections.plugin_settings.len();
        let total_units = 4.0;
        report_progress(
            progress,
            CloudSyncProgressStage::PreviewingImport,
            2,
            total_units as usize,
        );
        let mut preview = StructuredPreview {
            remote_metadata: metadata,
            manifest,
            connections_snapshot: None,
            forwards_snapshot: None,
            app_settings_entries: std::collections::BTreeMap::new(),
            app_settings_sections: std::collections::BTreeMap::new(),
            plugin_settings_entries: std::collections::BTreeMap::new(),
            plugin_settings_counts: std::collections::BTreeMap::new(),
        };

        let mut completed_encrypted_entries = 0usize;
        if let Some(entry) = preview.manifest.sections.connections.as_ref() {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            preview.connections_snapshot = Some(serde_json::from_slice(&object.bytes)?);
        }
        if let Some(entry) = preview.manifest.sections.forwards.as_ref() {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            preview.forwards_snapshot = Some(serde_json::from_slice(&object.bytes)?);
        }
        for (section_id, entry) in &preview.manifest.sections.app_settings {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            if let Some(password) = sync_password.as_deref() {
                let import_preview = preview_oxide_import_with_progress(
                    connection_store,
                    &object.bytes,
                    password,
                    ImportConflictStrategy::Replace,
                    |stage, current, total| {
                        let fraction = fractional_import_progress(current, total);
                        report_fractional_progress(
                            progress,
                            host_import_progress_stage(stage, true),
                            structured_preview_progress_current(
                                completed_encrypted_entries,
                                encrypted_entry_count,
                                fraction,
                            ),
                            total_units,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let section_preview = import_preview
                    .app_settings_sections
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| AppSettingsSectionPreview {
                        id: section_id.clone(),
                        field_keys: Vec::new(),
                        field_values: Default::default(),
                        contains_env_vars: false,
                    });
                preview
                    .app_settings_sections
                    .insert(section_id.clone(), section_preview);
            }
            preview
                .app_settings_entries
                .insert(section_id.clone(), object.bytes);
            completed_encrypted_entries += 1;
            report_fractional_progress(
                progress,
                CloudSyncProgressStage::PreviewingImport,
                structured_preview_progress_current(
                    completed_encrypted_entries,
                    encrypted_entry_count,
                    0.0,
                ),
                total_units,
            );
        }
        for (plugin_id, entry) in &preview.manifest.sections.plugin_settings {
            if plugin_id == crate::CLOUD_SYNC_PLUGIN_ID {
                continue;
            }
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            if let Some(password) = sync_password.as_deref() {
                let import_preview = preview_oxide_import_with_progress(
                    connection_store,
                    &object.bytes,
                    password,
                    ImportConflictStrategy::Replace,
                    |stage, current, total| {
                        let fraction = fractional_import_progress(current, total);
                        report_fractional_progress(
                            progress,
                            host_import_progress_stage(stage, true),
                            structured_preview_progress_current(
                                completed_encrypted_entries,
                                encrypted_entry_count,
                                fraction,
                            ),
                            total_units,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                preview
                    .plugin_settings_counts
                    .insert(plugin_id.clone(), import_preview.plugin_settings_count);
            }
            preview
                .plugin_settings_entries
                .insert(plugin_id.clone(), object.bytes);
            completed_encrypted_entries += 1;
            report_fractional_progress(
                progress,
                CloudSyncProgressStage::PreviewingImport,
                structured_preview_progress_current(
                    completed_encrypted_entries,
                    encrypted_entry_count,
                    0.0,
                ),
                total_units,
            );
        }
        if encrypted_entry_count == 0 {
            report_progress(progress, CloudSyncProgressStage::PreviewingImport, 3, 4);
        }
        report_progress(progress, CloudSyncProgressStage::Done, 4, 4);
        Ok(Some(preview))
    }

    pub async fn pull_legacy_preview(
        &self,
        connection_store: &ConnectionStore,
        settings: &CloudSyncSettings,
        secret_provider: &mut impl CloudSyncSecretProvider,
        conflict_strategy: ConflictStrategy,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<LegacyPreview> {
        let Some(_permit) = self.guard.begin(CloudSyncOperationKind::Pull, false)? else {
            unreachable!();
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        report_progress(progress, CloudSyncProgressStage::FetchMetadata, 1, 4);
        let secrets = get_action_secrets(settings, secret_provider, true, SecretReadMode::Prompt)?;
        let password = secrets
            .sync_password
            .as_deref()
            .filter(|password| !password.is_empty())
            .context("missing_sync_password: cloud sync password is required")?;
        let remote = self
            .backend
            .download_remote_snapshot(settings, &secrets)
            .await?;
        report_progress(progress, CloudSyncProgressStage::Validating, 2, 4);
        let metadata = OxideFile::from_bytes(&remote.bytes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?
            .metadata;
        let mut preview_progress = |stage: &str, current: usize, total: usize| {
            let fraction = fractional_import_progress(current, total);
            report_fractional_progress(
                progress,
                host_import_progress_stage(stage, true),
                (2.0 + fraction).min(3.0),
                4.0,
            );
        };
        let preview = preview_oxide_import_with_progress(
            connection_store,
            &remote.bytes,
            password,
            import_strategy_from_cloud(conflict_strategy),
            &mut preview_progress,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        report_progress(progress, CloudSyncProgressStage::Done, 4, 4);
        Ok(LegacyPreview {
            remote_metadata: remote.metadata,
            bytes: remote.bytes,
            metadata,
            preview,
        })
    }

    pub fn apply_structured_preview(
        &self,
        connection_store: &mut ConnectionStore,
        forwarding_registry: &ForwardingRegistry,
        settings_store: &mut SettingsStore,
        _settings: &CloudSyncSettings,
        preview: StructuredPreview,
        selection: StructuredApplySelection,
        conflict_strategy: ConflictStrategy,
        sync_password: Option<&str>,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<Option<ApplyStructuredPreviewOutcome>> {
        let Some(_permit) = self
            .guard
            .begin(CloudSyncOperationKind::ApplyPreview, false)?
        else {
            return Ok(None);
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let app_settings_entry_ids = selection
            .app_settings_sections
            .iter()
            .filter(|section_id| preview.app_settings_entries.contains_key(*section_id))
            .cloned()
            .collect::<Vec<_>>();
        let plugin_entry_ids = selection
            .plugin_ids
            .iter()
            .filter(|plugin_id| preview.plugin_settings_entries.contains_key(*plugin_id))
            .cloned()
            .collect::<Vec<_>>();
        let needs_password = !app_settings_entry_ids.is_empty() || !plugin_entry_ids.is_empty();
        let sync_password = if needs_password {
            Some(required_sync_password(sync_password)?)
        } else {
            None
        };

        let total = (app_settings_entry_ids.len()
            + plugin_entry_ids.len()
            + usize::from(selection.connections && preview.connections_snapshot.is_some())
            + usize::from(selection.forwards && preview.forwards_snapshot.is_some()))
        .max(1);
        report_progress(progress, CloudSyncProgressStage::Importing, 0, total);

        let mut completed = 0usize;
        let content_summary = CloudSyncHistorySummary {
            connections: preview
                .connections_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0),
            forwards: preview
                .forwards_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0),
            has_app_settings: !preview.app_settings_entries.is_empty(),
            plugin_settings_count: preview
                .plugin_settings_counts
                .values()
                .copied()
                .sum::<usize>()
                .max(preview.plugin_settings_entries.len()),
        };
        let mut app_settings_snapshots = std::collections::BTreeMap::new();
        let mut plugin_settings_snapshot = Vec::new();
        if let Some(password) = sync_password {
            for section_id in &app_settings_entry_ids {
                let Some(bytes) = preview.app_settings_entries.get(section_id) else {
                    continue;
                };
                let envelope = apply_oxide_import_with_options_with_progress(
                    connection_store,
                    bytes,
                    password,
                    OxideImportOptions {
                        selected_names: Some(Vec::new()),
                        conflict_strategy: ImportConflictStrategy::Replace,
                        import_forwards: false,
                        import_portable_secrets: false,
                    },
                    |stage, current, import_total| {
                        let fraction = fractional_import_progress(current, import_total);
                        report_fractional_progress(
                            progress,
                            host_import_progress_stage(stage, false),
                            (completed as f64 + fraction).min(total as f64),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                if let Some(app_settings_json) = envelope.app_settings_json {
                    app_settings_snapshots.insert(section_id.clone(), app_settings_json);
                }
                completed += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Importing,
                    completed,
                    total,
                );
            }

            for plugin_id in &plugin_entry_ids {
                let Some(bytes) = preview.plugin_settings_entries.get(plugin_id) else {
                    continue;
                };
                let envelope = apply_oxide_import_with_options_with_progress(
                    connection_store,
                    bytes,
                    password,
                    OxideImportOptions {
                        selected_names: Some(Vec::new()),
                        conflict_strategy: ImportConflictStrategy::Replace,
                        import_forwards: false,
                        import_portable_secrets: false,
                    },
                    |stage, current, import_total| {
                        let fraction = fractional_import_progress(current, import_total);
                        report_fractional_progress(
                            progress,
                            host_import_progress_stage(stage, false),
                            (completed as f64 + fraction).min(total as f64),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                plugin_settings_snapshot.extend(envelope.plugin_settings);
                completed += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Importing,
                    completed,
                    total,
                );
            }
        }

        let connections_snapshot = if selection.connections {
            preview.connections_snapshot
        } else {
            None
        };
        let forwards_snapshot = if selection.forwards {
            preview.forwards_snapshot
        } else {
            None
        };
        let connection_conflict_strategy = match conflict_strategy {
            ConflictStrategy::Skip => SavedConnectionsConflictStrategy::Skip,
            ConflictStrategy::Replace => SavedConnectionsConflictStrategy::Replace,
            ConflictStrategy::Merge | ConflictStrategy::Rename => {
                SavedConnectionsConflictStrategy::Merge
            }
        };
        let applied = apply_structured_snapshots(
            connection_store,
            forwarding_registry,
            settings_store,
            connections_snapshot,
            forwards_snapshot,
            app_settings_snapshots,
            plugin_settings_snapshot,
            connection_conflict_strategy,
        )?;
        completed +=
            usize::from(applied.connections.is_some()) + usize::from(applied.forwards.is_some());
        report_progress(
            progress,
            CloudSyncProgressStage::Importing,
            completed.min(total),
            total,
        );

        let local_snapshot = build_local_snapshot(
            connection_store,
            forwarding_registry,
            settings_store,
            None,
            None,
        )?;
        report_progress(progress, CloudSyncProgressStage::Done, total, total);

        let applied_selection = StructuredApplySelection {
            connections: applied.connections.as_ref().is_some_and(|outcome| {
                outcome.result.skipped == 0 && outcome.result.conflicts == 0
            }),
            forwards: applied
                .forwards
                .as_ref()
                .is_some_and(|outcome| outcome.skipped == 0),
            app_settings_sections: app_settings_entry_ids,
            plugin_ids: plugin_entry_ids,
        };

        Ok(Some(ApplyStructuredPreviewOutcome {
            local_snapshot,
            applied,
            content_summary,
            manifest: preview.manifest,
            remote_metadata: preview.remote_metadata,
            selection: applied_selection,
        }))
    }

    pub fn apply_legacy_preview(
        &self,
        connection_store: &mut ConnectionStore,
        _settings: &CloudSyncSettings,
        preview: &LegacyPreview,
        sync_password: Option<&str>,
        import_connections: bool,
        import_forwards: bool,
        conflict_strategy: ConflictStrategy,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<Option<ApplyLegacyPreviewOutcome>> {
        let Some(_permit) = self
            .guard
            .begin(CloudSyncOperationKind::ApplyPreview, false)?
        else {
            return Ok(None);
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let total = 1.0;
        report_fractional_progress(progress, CloudSyncProgressStage::Importing, 0.0, total);
        let password = required_sync_password(sync_password)?;
        let mut import_progress = |stage: &str, current: usize, total: usize| {
            report_fractional_progress(
                progress,
                host_import_progress_stage(stage, false),
                fractional_import_progress(current, total),
                1.0,
            );
        };
        let envelope = apply_oxide_import_with_options_with_progress(
            connection_store,
            &preview.bytes,
            password,
            OxideImportOptions {
                selected_names: if import_connections {
                    None
                } else {
                    Some(Vec::new())
                },
                conflict_strategy: import_strategy_from_cloud(conflict_strategy),
                import_forwards,
                import_portable_secrets: import_connections,
                ..OxideImportOptions::default()
            },
            &mut import_progress,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        report_fractional_progress(progress, CloudSyncProgressStage::Done, total, total);
        Ok(Some(ApplyLegacyPreviewOutcome { envelope }))
    }

    async fn read_required_object(
        &self,
        settings: &CloudSyncSettings,
        secrets: &crate::secrets::CloudSyncSecrets,
        entry: &StructuredObjectEntry,
    ) -> Result<crate::backend::RemoteObject> {
        self.backend
            .read_remote_object(settings, secrets, &entry.path)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("remote_not_found: missing remote object {}", entry.path)
            })
    }

    async fn build_structured_upload_plan(
        &self,
        connection_store: &ConnectionStore,
        forwarding_registry: &ForwardingRegistry,
        settings_store: &SettingsStore,
        local_snapshot: &CloudSyncLocalSnapshot,
        revision: &str,
        uploaded_at: &str,
        device_id: &str,
        sync_password: Option<&str>,
        progress: &mut dyn CloudSyncProgressSink,
        total: usize,
    ) -> Result<StructuredUploadPlan> {
        let mut manifest = crate::create_manifest_base(
            revision.to_string(),
            uploaded_at.to_string(),
            device_id.to_string(),
            local_snapshot.scope.clone(),
        );
        let mut objects = Vec::new();
        let mut completed_exports = 0usize;

        if local_snapshot.scope.sync_connections {
            let snapshot = connection_store.export_saved_connections_snapshot()?;
            let bytes = serde_json::to_vec(&snapshot)?;
            let path = connections_object_path(&snapshot.revision);
            manifest.sections.connections = Some(crate::StructuredObjectEntry {
                revision: snapshot.revision.clone(),
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_forwards {
            let snapshot = forwarding_registry.export_saved_forwards_snapshot()?;
            let bytes = serde_json::to_vec(&snapshot)?;
            let path = forwards_object_path(&snapshot.revision);
            manifest.sections.forwards = Some(crate::StructuredObjectEntry {
                revision: snapshot.revision.clone(),
                path: path.clone(),
                record_count: Some(snapshot.records.len()),
                content_type: "application/json".to_string(),
            });
            objects.push(StructuredUploadObject {
                path,
                bytes,
                content_type: "application/json".to_string(),
            });
            completed_exports += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::Exporting,
                2 + completed_exports,
                total,
            );
        }

        if local_snapshot.scope.sync_app_settings {
            let password =
                sync_password.context("missing_sync_password: cloud sync password is required")?;
            for section_id in &local_snapshot.scope.app_settings_sections {
                let Some(section_revision) = local_snapshot
                    .metadata
                    .app_settings_section_revisions
                    .get(section_id)
                else {
                    continue;
                };
                let selected = std::collections::HashSet::from([section_id.clone()]);
                let app_settings_json = export_oxide_settings_snapshot_json(
                    settings_store.settings(),
                    Some(&selected),
                    local_snapshot.scope.include_local_terminal_env_vars,
                )?;
                let bytes = export_connections_to_oxide_with_progress(
                    connection_store,
                    &[],
                    password,
                    OxideExportOptions {
                        description: Some(format!("Cloud Sync app settings {section_id}")),
                        embed_keys: false,
                        app_settings_json: Some(app_settings_json),
                        ..OxideExportOptions::default()
                    },
                    |_stage, current, export_total| {
                        report_fractional_progress(
                            progress,
                            CloudSyncProgressStage::Exporting,
                            export_progress_current(completed_exports, current, export_total),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let path = crate::app_settings_object_path(section_id, section_revision);
                manifest.sections.app_settings.insert(
                    section_id.clone(),
                    crate::StructuredObjectEntry {
                        revision: section_revision.clone(),
                        path: path.clone(),
                        record_count: None,
                        content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                    },
                );
                objects.push(StructuredUploadObject {
                    path,
                    bytes,
                    content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                });
                completed_exports += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Exporting,
                    2 + completed_exports,
                    total,
                );
            }
        }

        if local_snapshot.scope.sync_plugin_settings {
            let password =
                sync_password.context("missing_sync_password: cloud sync password is required")?;
            let entries = crate::plugin_settings::load_plugin_settings(settings_store.path())
                .map_err(anyhow::Error::msg)?;
            for plugin_id in scoped_plugin_ids(local_snapshot) {
                let Some(plugin_revision) = local_snapshot
                    .metadata
                    .plugin_settings_revisions
                    .get(&plugin_id)
                else {
                    continue;
                };
                let plugin_settings = entries
                    .iter()
                    .filter(|entry| {
                        plugin_id_from_setting_storage_key(&entry.storage_key).as_deref()
                            == Some(plugin_id.as_str())
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let bytes = export_connections_to_oxide_with_progress(
                    connection_store,
                    &[],
                    password,
                    OxideExportOptions {
                        description: Some(format!("Cloud Sync plugin settings {plugin_id}")),
                        embed_keys: false,
                        plugin_settings,
                        ..OxideExportOptions::default()
                    },
                    |_stage, current, export_total| {
                        report_fractional_progress(
                            progress,
                            CloudSyncProgressStage::Exporting,
                            export_progress_current(completed_exports, current, export_total),
                            total as f64,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let path = crate::plugin_settings_object_path(&plugin_id, plugin_revision);
                manifest.sections.plugin_settings.insert(
                    plugin_id.clone(),
                    crate::StructuredObjectEntry {
                        revision: plugin_revision.clone(),
                        path: path.clone(),
                        record_count: None,
                        content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                    },
                );
                objects.push(StructuredUploadObject {
                    path,
                    bytes,
                    content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                });
                completed_exports += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::Exporting,
                    2 + completed_exports,
                    total,
                );
            }
        }

        manifest.section_revisions = crate::build_manifest_section_revisions(&manifest);
        Ok(StructuredUploadPlan { manifest, objects })
    }
}

#[derive(Clone, Debug, Default)]
pub struct UploadOptions {
    pub automatic: bool,
    pub skip_if_busy: bool,
    pub force: bool,
    pub device_id: String,
    pub revision_sequence: u64,
    pub previous_remote_revision: Option<String>,
    pub previous_remote_sections: Option<StructuredSectionRevisions>,
    pub last_synced_structured_state: Option<StructuredLocalState>,
    pub raw_sync_scope: Option<RawSyncScope>,
}

#[derive(Clone, Debug)]
pub struct UploadOutcome {
    pub revision: String,
    pub revision_sequence: u64,
    pub etag: Option<String>,
    pub local_snapshot: CloudSyncLocalSnapshot,
    pub manifest: crate::StructuredManifest,
}

#[derive(Clone, Debug)]
pub struct CloudSyncUploadError {
    pub message: String,
    pub remote_metadata: Option<RemoteMetadata>,
    pub revision_sequence_consumed: Option<u64>,
}

impl std::fmt::Display for CloudSyncUploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CloudSyncUploadError {}

impl From<anyhow::Error> for CloudSyncUploadError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            message: error.to_string(),
            remote_metadata: None,
            revision_sequence_consumed: None,
        }
    }
}

impl From<crate::secrets::CloudSyncSecretError> for CloudSyncUploadError {
    fn from(error: crate::secrets::CloudSyncSecretError) -> Self {
        Self {
            message: error.to_string(),
            remote_metadata: None,
            revision_sequence_consumed: None,
        }
    }
}

impl From<serde_json::Error> for CloudSyncUploadError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            remote_metadata: None,
            revision_sequence_consumed: None,
        }
    }
}

fn upload_error_after_revision(
    error: impl std::fmt::Display,
    revision_sequence: u64,
) -> CloudSyncUploadError {
    CloudSyncUploadError {
        message: error.to_string(),
        remote_metadata: None,
        revision_sequence_consumed: Some(revision_sequence),
    }
}

fn fractional_import_progress(current: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (current as f64 / total as f64).clamp(0.0, 1.0)
    }
}

fn export_progress_current(completed_exports: usize, current: usize, total: usize) -> f64 {
    let fraction = if total == 0 {
        0.0
    } else {
        (current as f64 / total as f64).clamp(0.0, 0.95)
    };
    2.0 + completed_exports as f64 + fraction
}

fn structured_preview_progress_current(
    completed_entries: usize,
    total_entries: usize,
    active_fraction: f64,
) -> f64 {
    if total_entries == 0 {
        3.0
    } else {
        let fraction = ((completed_entries as f64 + active_fraction.clamp(0.0, 1.0))
            / total_entries as f64)
            .clamp(0.0, 1.0);
        (2.0 + fraction).min(3.0)
    }
}

fn host_import_progress_stage(stage: &str, preview: bool) -> CloudSyncProgressStage {
    match stage {
        "parsing_file"
        | "deriving_key"
        | "decrypting_payload"
        | "deserializing_payload"
        | "verifying_checksum" => {
            if preview {
                CloudSyncProgressStage::PreviewingImport
            } else {
                CloudSyncProgressStage::Importing
            }
        }
        "collecting_existing" | "building_preview" | "analyzing_preview" => {
            CloudSyncProgressStage::PreviewingImport
        }
        "filtering_selection"
        | "preparing_connections"
        | "applying_connections"
        | "saving_config" => CloudSyncProgressStage::Importing,
        _ => {
            if preview {
                CloudSyncProgressStage::PreviewingImport
            } else {
                CloudSyncProgressStage::Importing
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct StructuredPreview {
    pub remote_metadata: RemoteMetadata,
    pub manifest: StructuredManifest,
    pub connections_snapshot: Option<SavedConnectionsSyncSnapshot>,
    pub forwards_snapshot: Option<SavedForwardsSyncSnapshot>,
    pub app_settings_entries: std::collections::BTreeMap<String, Vec<u8>>,
    pub app_settings_sections: std::collections::BTreeMap<String, AppSettingsSectionPreview>,
    pub plugin_settings_entries: std::collections::BTreeMap<String, Vec<u8>>,
    pub plugin_settings_counts: std::collections::BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct LegacyPreview {
    pub remote_metadata: RemoteMetadata,
    pub bytes: Vec<u8>,
    pub metadata: OxideMetadata,
    pub preview: ImportPreview,
}

#[derive(Clone, Debug)]
pub struct ApplyStructuredPreviewOutcome {
    pub local_snapshot: CloudSyncLocalSnapshot,
    pub applied: CloudSyncApplyOutcome,
    pub content_summary: CloudSyncHistorySummary,
    pub manifest: StructuredManifest,
    pub remote_metadata: RemoteMetadata,
    pub selection: StructuredApplySelection,
}

#[derive(Clone, Debug)]
pub struct ApplyLegacyPreviewOutcome {
    pub envelope: ImportResultEnvelope,
}

impl StructuredPreview {
    pub fn full_selection(&self) -> StructuredApplySelection {
        StructuredApplySelection {
            connections: self.connections_snapshot.is_some(),
            forwards: self.forwards_snapshot.is_some(),
            app_settings_sections: self.app_settings_entries.keys().cloned().collect(),
            plugin_ids: self.plugin_settings_entries.keys().cloned().collect(),
        }
    }
}

fn required_sync_password(password: Option<&str>) -> Result<&str> {
    password
        .filter(|password| !password.is_empty())
        .context("missing_sync_password: cloud sync password is required")
}

fn import_strategy_from_cloud(strategy: ConflictStrategy) -> ImportConflictStrategy {
    match strategy {
        ConflictStrategy::Merge => ImportConflictStrategy::Merge,
        ConflictStrategy::Replace => ImportConflictStrategy::Replace,
        ConflictStrategy::Skip => ImportConflictStrategy::Skip,
        ConflictStrategy::Rename => ImportConflictStrategy::Rename,
    }
}

#[derive(Clone, Debug)]
struct StructuredUploadPlan {
    manifest: crate::StructuredManifest,
    objects: Vec<StructuredUploadObject>,
}

#[derive(Clone, Debug)]
struct StructuredUploadObject {
    path: String,
    bytes: Vec<u8>,
    content_type: String,
}

fn ensure_no_remote_conflict(
    local_snapshot: &CloudSyncLocalSnapshot,
    remote_metadata: &RemoteMetadata,
    previous_remote_revision: Option<&str>,
    previous_remote_sections: Option<&StructuredSectionRevisions>,
) -> Result<()> {
    if remote_metadata.format.as_deref() != Some(STRUCTURED_MANIFEST_FORMAT) {
        if local_snapshot.dirty.has_dirty
            && remote_metadata.revision.as_deref().is_some_and(|revision| {
                previous_remote_revision.map_or(true, |previous| previous != revision)
            })
        {
            bail!(
                "remote_changed_before_upload: remote snapshot exists while local state is dirty"
            );
        }
        return Ok(());
    }
    if local_snapshot.dirty.has_dirty
        && has_structured_conflict(
            &local_snapshot.dirty.dirty_sections,
            remote_metadata.section_revisions.as_ref(),
            previous_remote_sections,
        )
    {
        bail!(
            "remote_changed_before_upload: remote structured snapshot exists while local state is dirty"
        );
    }
    Ok(())
}

fn has_structured_conflict(
    dirty_sections: &crate::StructuredDirtySections,
    remote_sections: Option<&StructuredSectionRevisions>,
    previous_remote_sections: Option<&StructuredSectionRevisions>,
) -> bool {
    let Some(previous) = previous_remote_sections else {
        return dirty_sections.connections
            || dirty_sections.forwards
            || dirty_sections.app_settings.values().any(|dirty| *dirty)
            || dirty_sections.plugin_settings.values().any(|dirty| *dirty);
    };
    let remote = remote_sections.cloned().unwrap_or_default();
    if dirty_sections.connections && remote.connections != previous.connections {
        return true;
    }
    if dirty_sections.forwards && remote.forwards != previous.forwards {
        return true;
    }
    for (section_id, dirty) in &dirty_sections.app_settings {
        if *dirty && remote.app_settings.get(section_id) != previous.app_settings.get(section_id) {
            return true;
        }
    }
    for (plugin_id, dirty) in &dirty_sections.plugin_settings {
        if *dirty
            && remote.plugin_settings.get(plugin_id) != previous.plugin_settings.get(plugin_id)
        {
            return true;
        }
    }
    false
}

fn manifest_from_metadata(metadata: &RemoteMetadata) -> Result<StructuredManifest> {
    let sections = metadata
        .sections
        .clone()
        .context("missing structured manifest sections")?;
    Ok(StructuredManifest {
        format: metadata
            .format
            .clone()
            .unwrap_or_else(|| STRUCTURED_MANIFEST_FORMAT.to_string()),
        revision: metadata.revision.clone().unwrap_or_default(),
        uploaded_at: metadata.uploaded_at.clone().unwrap_or_default(),
        device_id: metadata.device_id.clone().unwrap_or_default(),
        content_type: metadata
            .content_type
            .clone()
            .unwrap_or_else(|| STRUCTURED_MANIFEST_CONTENT_TYPE.to_string()),
        scope: metadata.scope.clone().unwrap_or_default(),
        sections: serde_json::from_value::<StructuredManifestSections>(sections)?,
        section_revisions: metadata.section_revisions.clone().unwrap_or_default(),
    })
}

fn scoped_plugin_ids(local_snapshot: &CloudSyncLocalSnapshot) -> Vec<String> {
    match local_snapshot.scope.plugin_ids.as_ref() {
        Some(plugin_ids) => crate::get_syncable_plugin_ids(plugin_ids),
        None => crate::get_syncable_plugin_ids(
            &local_snapshot
                .metadata
                .plugin_settings_revisions
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
        ),
    }
}

fn plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";
    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];
    (!plugin_id.is_empty() && !setting_id.is_empty()).then(|| plugin_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
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
    fn upload_conflict_check_rejects_changed_legacy_snapshot_like_tauri() {
        let metadata = RemoteMetadata {
            exists: true,
            revision: Some("remote-new".to_string()),
            format: None,
            ..RemoteMetadata::default()
        };

        let error =
            ensure_no_remote_conflict(&dirty_snapshot(), &metadata, Some("remote-old"), None)
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

        ensure_no_remote_conflict(&dirty_snapshot(), &metadata, Some("remote-current"), None)
            .unwrap();
    }
}
