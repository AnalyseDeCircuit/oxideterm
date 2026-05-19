// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use oxideterm_connections::{
    ConnectionStore, SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    oxide_file::{
        ImportConflictStrategy, ImportPreview, ImportResultEnvelope, OxideExportOptions, OxideFile,
        OxideImportOptions, OxideMetadata, apply_oxide_import_with_options,
        apply_oxide_import_with_options_with_progress, export_connections_to_oxide_with_progress,
        preview_oxide_import_with_progress,
    },
};
use oxideterm_forwarding::{ForwardingRegistry, SavedForwardsSyncSnapshot};
use oxideterm_settings::{SettingsStore, export_oxide_settings_snapshot_json};

use crate::{
    CloudSyncSettings, ConflictStrategy, STRUCTURED_MANIFEST_CONTENT_TYPE,
    STRUCTURED_MANIFEST_FORMAT, StructuredApplySelection, StructuredManifest,
    StructuredManifestSections, StructuredObjectEntry, StructuredSectionRevisions,
    backend::{CloudSyncBackend, RemoteMetadata},
    connections_object_path, forwards_object_path,
    progress::{CloudSyncProgressSink, CloudSyncProgressStage, report_progress},
    revision_id,
    secrets::{CloudSyncSecretProvider, SecretReadMode, get_action_secrets},
    service::{
        CloudSyncApplyOutcome, CloudSyncLocalSnapshot, apply_structured_snapshots,
        build_local_snapshot,
    },
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
        report_progress(progress, CloudSyncProgressStage::FetchMetadata, 1, 2);
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
    ) -> Result<Option<UploadOutcome>> {
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
            None,
            None,
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
            bail!("missing_sync_password: cloud sync password is required");
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
            ensure_no_remote_conflict(
                &local_snapshot,
                &remote_metadata,
                options.previous_remote_sections.as_ref(),
            )?;
        }

        report_progress(progress, CloudSyncProgressStage::Preflight, 2, total);
        let revision = revision_id(Utc::now(), &options.device_id, options.revision_sequence);
        let uploaded_at = Utc::now().to_rfc3339();
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
            .await?;

        let mut completed_uploads = 0usize;
        for object in &plan.objects {
            self.backend
                .write_remote_object(
                    settings,
                    &secrets,
                    &object.path,
                    object.bytes.clone(),
                    Some(&object.content_type),
                )
                .await?;
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
            .write_remote_metadata(settings, &secrets, &serde_json::to_value(&plan.manifest)?)
            .await?;
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
        settings: &CloudSyncSettings,
        secret_provider: &mut impl CloudSyncSecretProvider,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<Option<StructuredPreview>> {
        let Some(_permit) = self.guard.begin(CloudSyncOperationKind::Pull, false)? else {
            unreachable!();
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        report_progress(progress, CloudSyncProgressStage::FetchMetadata, 1, 4);
        let metadata_secrets =
            get_action_secrets(settings, secret_provider, false, SecretReadMode::Prompt)?;
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
        let _preview_secrets = if needs_password {
            let secrets =
                get_action_secrets(settings, secret_provider, true, SecretReadMode::Prompt)?;
            if secrets
                .sync_password
                .as_deref()
                .unwrap_or_default()
                .is_empty()
            {
                bail!("missing_sync_password: cloud sync password is required");
            }
            secrets
        } else {
            metadata_secrets.clone()
        };

        let manifest = manifest_from_metadata(&metadata)
            .context("failed to decode structured cloud sync manifest")?;
        let total_units = 4 + count_manifest_objects(&manifest);
        report_progress(
            progress,
            CloudSyncProgressStage::Downloading,
            2,
            total_units,
        );
        let mut preview = StructuredPreview {
            remote_metadata: metadata,
            manifest,
            connections_snapshot: None,
            forwards_snapshot: None,
            app_settings_entries: std::collections::BTreeMap::new(),
            plugin_settings_entries: std::collections::BTreeMap::new(),
        };

        let mut completed = 2usize;
        if let Some(entry) = preview.manifest.sections.connections.as_ref() {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            preview.connections_snapshot = Some(serde_json::from_slice(&object.bytes)?);
            completed += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::PreviewingImport,
                completed,
                total_units,
            );
        }
        if let Some(entry) = preview.manifest.sections.forwards.as_ref() {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            preview.forwards_snapshot = Some(serde_json::from_slice(&object.bytes)?);
            completed += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::PreviewingImport,
                completed,
                total_units,
            );
        }
        for (section_id, entry) in &preview.manifest.sections.app_settings {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            preview
                .app_settings_entries
                .insert(section_id.clone(), object.bytes);
            completed += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::PreviewingImport,
                completed,
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
            preview
                .plugin_settings_entries
                .insert(plugin_id.clone(), object.bytes);
            completed += 1;
            report_progress(
                progress,
                CloudSyncProgressStage::PreviewingImport,
                completed,
                total_units,
            );
        }
        report_progress(
            progress,
            CloudSyncProgressStage::Done,
            total_units,
            total_units,
        );
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
        report_progress(progress, CloudSyncProgressStage::Downloading, 1, 4);
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
        report_progress(progress, CloudSyncProgressStage::Preflight, 2, 4);
        let metadata = OxideFile::from_bytes(&remote.bytes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?
            .metadata;
        let mut preview_progress = |stage: &str, current: usize, total: usize| {
            let mapped_stage = match stage {
                "parsing_file" | "collecting_existing" | "building_preview" => {
                    CloudSyncProgressStage::PreviewingImport
                }
                _ => CloudSyncProgressStage::PreviewingImport,
            };
            let current = 2 + usize::from(total > 0 && current >= total);
            report_progress(progress, mapped_stage, current.min(3), 4);
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
        secret_provider: &mut impl CloudSyncSecretProvider,
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
        let selection = preview.full_selection();
        let needs_password =
            !preview.app_settings_entries.is_empty() || !preview.plugin_settings_entries.is_empty();
        let sync_password = if needs_password {
            let password = secret_provider
                .get_secret(crate::secret_keys::SYNC_PASSWORD, SecretReadMode::Prompt)?;
            let password = password.unwrap_or_default();
            if password.is_empty() {
                bail!("missing_sync_password: cloud sync password is required");
            }
            Some(password)
        } else {
            None
        };

        let total = 2
            + preview.app_settings_entries.len()
            + preview.plugin_settings_entries.len()
            + usize::from(preview.connections_snapshot.is_some())
            + usize::from(preview.forwards_snapshot.is_some());
        report_progress(progress, CloudSyncProgressStage::CreatingBackup, 1, total);

        let mut completed = 1usize;
        let mut app_settings_snapshots = std::collections::BTreeMap::new();
        let mut plugin_settings_snapshot = Vec::new();
        if let Some(password) = sync_password.as_deref() {
            for (section_id, bytes) in &preview.app_settings_entries {
                let envelope = apply_oxide_import_with_options(
                    connection_store,
                    bytes,
                    password,
                    OxideImportOptions {
                        selected_names: Some(Vec::new()),
                        conflict_strategy: ImportConflictStrategy::Merge,
                        import_forwards: false,
                        import_portable_secrets: false,
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

            for bytes in preview.plugin_settings_entries.values() {
                let envelope = apply_oxide_import_with_options(
                    connection_store,
                    bytes,
                    password,
                    OxideImportOptions {
                        selected_names: Some(Vec::new()),
                        conflict_strategy: ImportConflictStrategy::Merge,
                        import_forwards: false,
                        import_portable_secrets: false,
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

        let applied = apply_structured_snapshots(
            connection_store,
            forwarding_registry,
            settings_store,
            preview.connections_snapshot,
            preview.forwards_snapshot,
            app_settings_snapshots,
            plugin_settings_snapshot,
            SavedConnectionsConflictStrategy::Merge,
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

        Ok(Some(ApplyStructuredPreviewOutcome {
            local_snapshot,
            applied,
            manifest: preview.manifest,
            remote_metadata: preview.remote_metadata,
            selection,
        }))
    }

    pub fn apply_legacy_preview(
        &self,
        connection_store: &mut ConnectionStore,
        settings: &CloudSyncSettings,
        preview: &LegacyPreview,
        secret_provider: &mut impl CloudSyncSecretProvider,
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
        report_progress(progress, CloudSyncProgressStage::Importing, 1, 2);
        let secrets = get_action_secrets(settings, secret_provider, true, SecretReadMode::Prompt)?;
        let password = secrets
            .sync_password
            .as_deref()
            .filter(|password| !password.is_empty())
            .context("missing_sync_password: cloud sync password is required")?;
        let mut import_progress = |stage: &str, current: usize, total: usize| {
            let mapped = match stage {
                "parsing_file" | "filtering_selection" | "collecting_existing" => {
                    CloudSyncProgressStage::PreviewingImport
                }
                _ => CloudSyncProgressStage::Importing,
            };
            report_progress(progress, mapped, current.min(total.max(1)), total.max(1));
        };
        let envelope = apply_oxide_import_with_options_with_progress(
            connection_store,
            &preview.bytes,
            password,
            OxideImportOptions {
                conflict_strategy: import_strategy_from_cloud(conflict_strategy),
                import_forwards: true,
                import_portable_secrets: true,
                ..OxideImportOptions::default()
            },
            &mut import_progress,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        report_progress(progress, CloudSyncProgressStage::Done, 2, 2);
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
                    |_stage, _current, _total| {},
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
                    |_stage, _current, _total| {},
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
    pub previous_remote_sections: Option<StructuredSectionRevisions>,
}

#[derive(Clone, Debug)]
pub struct UploadOutcome {
    pub revision: String,
    pub etag: Option<String>,
    pub local_snapshot: CloudSyncLocalSnapshot,
    pub manifest: crate::StructuredManifest,
}

#[derive(Clone, Debug)]
pub struct StructuredPreview {
    pub remote_metadata: RemoteMetadata,
    pub manifest: StructuredManifest,
    pub connections_snapshot: Option<SavedConnectionsSyncSnapshot>,
    pub forwards_snapshot: Option<SavedForwardsSyncSnapshot>,
    pub app_settings_entries: std::collections::BTreeMap<String, Vec<u8>>,
    pub plugin_settings_entries: std::collections::BTreeMap<String, Vec<u8>>,
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
    previous_remote_sections: Option<&StructuredSectionRevisions>,
) -> Result<()> {
    if remote_metadata.format.as_deref() != Some(STRUCTURED_MANIFEST_FORMAT) {
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

fn count_manifest_objects(manifest: &StructuredManifest) -> usize {
    usize::from(manifest.sections.connections.is_some())
        + usize::from(manifest.sections.forwards.is_some())
        + manifest.sections.app_settings.len()
        + manifest
            .sections
            .plugin_settings
            .keys()
            .filter(|plugin_id| plugin_id.as_str() != crate::CLOUD_SYNC_PLUGIN_ID)
            .count()
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
}
