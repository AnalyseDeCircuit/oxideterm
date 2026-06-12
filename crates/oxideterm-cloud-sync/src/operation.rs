// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use oxideterm_connections::{
    ConnectionStore, SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    SerialProfilesSyncSnapshot,
    oxide_file::{
        AppSettingsSectionPreview, EncryptedPortableSecret, ImportConflictStrategy, ImportPreview,
        ImportResultEnvelope, OxideExportOptions, OxideFile, OxideImportOptions, OxideMetadata,
        apply_oxide_import_with_options_with_progress, export_connections_to_oxide_with_progress,
        preflight_export, preview_oxide_import_with_progress,
    },
};
use oxideterm_forwarding::{ForwardingRegistry, SavedForwardsSyncSnapshot};
use oxideterm_quick_commands::{QuickCommand, QuickCommandCategory, QuickCommandsSnapshot};
use oxideterm_settings::{SettingsStore, export_oxide_settings_snapshot_json};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    BackendType, CloudSyncSettings, ConflictStrategy, RawSyncScope,
    STRUCTURED_MANIFEST_CONTENT_TYPE, STRUCTURED_MANIFEST_FORMAT, StructuredApplySelection,
    StructuredLocalState, StructuredManifest, StructuredManifestSections, StructuredObjectEntry,
    StructuredSectionRevisions,
    backend::{CloudSyncBackend, RemoteMetadata, RemoteUploadObject},
    connections_object_path, forwards_object_path,
    progress::{
        CloudSyncProgressSink, CloudSyncProgressStage, report_fractional_progress, report_progress,
    },
    quick_commands_object_path, revision_id, secret_keys,
    secrets::{CloudSyncSecretProvider, SecretReadMode, get_action_secrets},
    sensitive_credentials_object_path, serial_profiles_object_path,
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

    async fn prepare_action_secrets(
        &self,
        settings: &CloudSyncSettings,
        secret_provider: &mut impl CloudSyncSecretProvider,
        secrets: &mut crate::secrets::CloudSyncSecrets,
    ) -> Result<()> {
        if !matches!(settings.backend_type, BackendType::OneDrive) {
            return Ok(());
        }
        let Some(refresh_token) = secrets
            .microsoft_refresh_token
            .as_ref()
            .map(|token| token.as_str())
            .filter(|token| !token.is_empty())
        else {
            anyhow::bail!(
                "missing_microsoft_refresh_token: Microsoft refresh token is not configured"
            );
        };
        let refreshed = self
            .backend
            .refresh_microsoft_access_token(&settings.microsoft_oauth_client_id, refresh_token)
            .await?;
        // Refreshed Microsoft tokens cross directly into the keychain-backed
        // provider and stay in zeroizing owners for the rest of this action.
        secret_provider.store_secret(secret_keys::TOKEN, Some(refreshed.access_token.as_str()))?;
        if let Some(next_refresh_token) = refreshed.refresh_token.as_ref() {
            secret_provider.store_secret(
                secret_keys::MICROSOFT_REFRESH_TOKEN,
                Some(next_refresh_token.as_str()),
            )?;
        }
        secrets.token = Some(refreshed.access_token);
        if let Some(refresh_token) = refreshed.refresh_token {
            secrets.microsoft_refresh_token = Some(refresh_token);
        }
        Ok(())
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
        let mut secrets = get_action_secrets(
            settings,
            secret_provider,
            false,
            if silent_secrets {
                SecretReadMode::Silent
            } else {
                SecretReadMode::Prompt
            },
        )?;
        self.prepare_action_secrets(settings, secret_provider, &mut secrets)
            .await?;
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
        let requires_password = local_snapshot.scope.sync_app_settings
            || local_snapshot.scope.sync_plugin_settings
            || local_snapshot.scope.sync_sensitive_credentials;
        let mut secrets = get_action_secrets(
            settings,
            secret_provider,
            requires_password,
            if options.automatic {
                SecretReadMode::Silent
            } else {
                SecretReadMode::Prompt
            },
        )?;
        self.prepare_action_secrets(settings, secret_provider, &mut secrets)
            .await?;
        if requires_password
            && secrets
                .sync_password
                .as_ref()
                .map(|password| password.as_str())
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
        let mut effective_settings = settings.clone();
        let created_remote_id = if matches!(settings.backend_type, BackendType::GithubGist)
            && settings.git_repository.trim().is_empty()
        {
            let gist_id = self
                .backend
                .create_github_gist(&effective_settings, &secrets)
                .await
                .map_err(|error| upload_error_after_revision(error, options.revision_sequence))?;
            effective_settings.git_repository = gist_id.clone();
            Some(gist_id)
        } else {
            None
        };
        let remote_metadata = self
            .backend
            .fetch_remote_metadata(&effective_settings, &secrets)
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
                .filter(|connection_id| {
                    options
                        .item_filter
                        .connection_ids
                        .as_ref()
                        .is_none_or(|ids| ids.contains(connection_id))
                })
                .collect::<Vec<_>>();
            let preflight = preflight_export(connection_store, &connection_ids, false, false, 0);
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
                secrets
                    .sync_password
                    .as_ref()
                    .map(|password| password.as_str()),
                options.portable_secrets.clone(),
                &options.item_filter,
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
        let manifest_value = serde_json::to_value(&plan.manifest)
            .map_err(|error| upload_error_after_revision(error, options.revision_sequence))?;
        let metadata_write = if matches!(effective_settings.backend_type, BackendType::GithubGist) {
            let objects = plan
                .objects
                .iter()
                .map(|object| RemoteUploadObject {
                    path: object.path.clone(),
                    bytes: object.bytes.clone(),
                })
                .collect::<Vec<_>>();
            let write = self
                .backend
                .write_gist_objects_and_metadata(
                    &effective_settings,
                    &secrets,
                    &objects,
                    &manifest_value,
                    remote_metadata.etag.as_deref(),
                )
                .await
                .map_err(|error| upload_error_after_revision(error, options.revision_sequence))?;
            completed_uploads += plan.objects.len();
            report_progress(
                progress,
                CloudSyncProgressStage::UploadingBlob,
                2 + export_units + completed_uploads,
                total,
            );
            write
        } else {
            for object in &plan.objects {
                self.backend
                    .write_remote_object(
                        &effective_settings,
                        &secrets,
                        &object.path,
                        object.bytes.clone(),
                        Some(&object.content_type),
                    )
                    .await
                    .map_err(|error| {
                        upload_error_after_revision(error, options.revision_sequence)
                    })?;
                completed_uploads += 1;
                report_progress(
                    progress,
                    CloudSyncProgressStage::UploadingBlob,
                    2 + export_units + completed_uploads,
                    total,
                );
            }
            self.backend
                .write_remote_metadata(
                    &effective_settings,
                    &secrets,
                    &manifest_value,
                    remote_metadata.etag.as_deref(),
                )
                .await
                .map_err(|error| upload_error_after_revision(error, options.revision_sequence))?
        };
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
            created_remote_id,
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
        let mut secrets = get_action_secrets(
            settings,
            secret_provider,
            include_sync_password,
            SecretReadMode::Prompt,
        )?;
        self.prepare_action_secrets(settings, secret_provider, &mut secrets)
            .await?;
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
        previous_remote_sections: Option<&StructuredSectionRevisions>,
        progress: Option<&mut dyn CloudSyncProgressSink>,
    ) -> Result<Option<StructuredPreview>> {
        let Some(_permit) = self.guard.begin(CloudSyncOperationKind::Pull, false)? else {
            unreachable!();
        };
        let mut noop = |_| {};
        let progress = progress.unwrap_or(&mut noop);
        let mut metadata_secrets =
            get_action_secrets(settings, secret_provider, false, SecretReadMode::Prompt)?;
        self.prepare_action_secrets(settings, secret_provider, &mut metadata_secrets)
            .await?;
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
        let needs_password = needs_password
            || metadata
                .sections
                .as_ref()
                .and_then(|sections| sections.get("sensitiveCredentials"))
                .is_some();
        let sync_password = if needs_password {
            let mut secrets =
                get_action_secrets(settings, secret_provider, true, SecretReadMode::Prompt)?;
            self.prepare_action_secrets(settings, secret_provider, &mut secrets)
                .await?;
            // Keep the structured preview password in a zeroizing owner while it
            // is reused across per-section decryptions.
            Some(Zeroizing::new(
                required_sync_password(
                    secrets
                        .sync_password
                        .as_ref()
                        .map(|password| password.as_str()),
                )?
                .to_string(),
            ))
        } else {
            None
        };

        let manifest = manifest_from_metadata(&metadata)
            .context("failed to decode structured cloud sync manifest")?;
        let encrypted_entry_count = manifest.sections.app_settings.len()
            + manifest.sections.plugin_settings.len()
            + usize::from(manifest.sections.sensitive_credentials.is_some());
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
            quick_commands_snapshot_json: None,
            serial_profiles_snapshot: None,
            base_connections_snapshot: None,
            base_forwards_snapshot: None,
            base_quick_commands_snapshot_json: None,
            base_serial_profiles_snapshot: None,
            sensitive_credentials_entry: None,
            sensitive_credentials_preview: None,
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
        if let Some(entry) = preview.manifest.sections.quick_commands.as_ref() {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            // Keep the structured payload as JSON text so the quick-command
            // store can reuse its own sanitizer and conflict merge code.
            preview.quick_commands_snapshot_json = Some(String::from_utf8(object.bytes)?);
        }
        if let Some(entry) = preview.manifest.sections.serial_profiles.as_ref() {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            preview.serial_profiles_snapshot = Some(serde_json::from_slice(&object.bytes)?);
        }
        if let Some(previous) = previous_remote_sections {
            preview.base_connections_snapshot = read_optional_snapshot_at_revision(
                self,
                settings,
                &metadata_secrets,
                previous.connections.as_deref(),
                connections_object_path,
            )
            .await?;
            preview.base_forwards_snapshot = read_optional_snapshot_at_revision(
                self,
                settings,
                &metadata_secrets,
                previous.forwards.as_deref(),
                forwards_object_path,
            )
            .await?;
            preview.base_quick_commands_snapshot_json = read_optional_text_at_revision(
                self,
                settings,
                &metadata_secrets,
                previous.quick_commands.as_deref(),
                quick_commands_object_path,
            )
            .await?;
            preview.base_serial_profiles_snapshot = read_optional_snapshot_at_revision(
                self,
                settings,
                &metadata_secrets,
                previous.serial_profiles.as_deref(),
                serial_profiles_object_path,
            )
            .await?;
        }
        if let Some(entry) = preview.manifest.sections.sensitive_credentials.as_ref() {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            if let Some(password) = sync_password.as_ref().map(|password| password.as_str()) {
                let import_preview = preview_oxide_import_with_progress(
                    connection_store,
                    &object.bytes,
                    password,
                    ImportConflictStrategy::Merge,
                    |stage, current, total| {
                        let fraction = fractional_import_progress(current, total);
                        report_fractional_progress(
                            progress,
                            host_import_progress_stage(stage, true),
                            structured_preview_progress_current(
                                completed_encrypted_entries,
                                encrypted_entry_count.max(1),
                                fraction,
                            ),
                            total_units,
                        );
                    },
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                preview.sensitive_credentials_preview = Some(import_preview);
            }
            preview.sensitive_credentials_entry = Some(object.bytes);
        }
        for (section_id, entry) in &preview.manifest.sections.app_settings {
            let object = self
                .read_required_object(settings, &metadata_secrets, entry)
                .await?;
            if let Some(password) = sync_password.as_ref().map(|password| password.as_str()) {
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
            if let Some(password) = sync_password.as_ref().map(|password| password.as_str()) {
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
        let mut secrets =
            get_action_secrets(settings, secret_provider, true, SecretReadMode::Prompt)?;
        self.prepare_action_secrets(settings, secret_provider, &mut secrets)
            .await?;
        let password = secrets
            .sync_password
            .as_ref()
            .map(|password| password.as_str())
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
        mut preview: StructuredPreview,
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
        let apply_quick_commands =
            selection.quick_commands && preview.quick_commands_snapshot_json.is_some();
        let apply_serial_profiles =
            selection.serial_profiles && preview.serial_profiles_snapshot.is_some();
        let apply_sensitive_credentials =
            selection.sensitive_credentials && preview.sensitive_credentials_entry.is_some();
        let needs_password = !app_settings_entry_ids.is_empty() || !plugin_entry_ids.is_empty();
        let needs_password = needs_password || apply_sensitive_credentials;
        let sync_password = if needs_password {
            Some(required_sync_password(sync_password)?)
        } else {
            None
        };

        let total = (app_settings_entry_ids.len()
            + plugin_entry_ids.len()
            + usize::from(selection.connections && preview.connections_snapshot.is_some())
            + usize::from(selection.forwards && preview.forwards_snapshot.is_some())
            + usize::from(apply_quick_commands)
            + usize::from(apply_serial_profiles)
            + usize::from(apply_sensitive_credentials))
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
            quick_commands: preview
                .quick_commands_snapshot_json
                .as_deref()
                .and_then(|json| {
                    serde_json::from_str::<oxideterm_quick_commands::QuickCommandsSnapshot>(json)
                        .ok()
                        .map(|snapshot| snapshot.commands.len())
                })
                .unwrap_or(0),
            serial_profiles: preview
                .serial_profiles_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0),
            sensitive_credentials: preview
                .sensitive_credentials_preview
                .as_ref()
                .map(|preview| preview.total_connections + preview.portable_secret_count)
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
        let mut sensitive_credentials_envelope = None;
        if let Some(password) = sync_password {
            if apply_sensitive_credentials {
                if let Some(bytes) = preview.sensitive_credentials_entry.as_ref() {
                    let envelope = apply_oxide_import_with_options_with_progress(
                        connection_store,
                        bytes,
                        password,
                        OxideImportOptions {
                            selected_names: None,
                            selected_forward_ids: None,
                            conflict_strategy: ImportConflictStrategy::Merge,
                            import_forwards: false,
                            import_portable_secrets: true,
                            restore_managed_keys: true,
                            restore_managed_key_passphrases: true,
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
                    sensitive_credentials_envelope = Some(envelope);
                    completed += 1;
                    report_progress(
                        progress,
                        CloudSyncProgressStage::Importing,
                        completed,
                        total,
                    );
                }
            }
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
                        selected_forward_ids: None,
                        conflict_strategy: ImportConflictStrategy::Replace,
                        import_forwards: false,
                        import_portable_secrets: false,
                        ..OxideImportOptions::default()
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
                        selected_forward_ids: None,
                        conflict_strategy: ImportConflictStrategy::Replace,
                        import_forwards: false,
                        import_portable_secrets: false,
                        ..OxideImportOptions::default()
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

        let requires_upload_after_merge = merge_structured_preview_fields(
            connection_store,
            forwarding_registry,
            settings_store,
            &mut preview,
            &selection,
            &conflict_strategy,
        )?;

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
        let quick_commands_snapshot_json = if selection.quick_commands {
            preview.quick_commands_snapshot_json
        } else {
            None
        };
        let serial_profiles_snapshot = if selection.serial_profiles {
            preview.serial_profiles_snapshot
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
            quick_commands_snapshot_json,
            serial_profiles_snapshot,
            app_settings_snapshots,
            plugin_settings_snapshot,
            connection_conflict_strategy,
        )?;
        completed += usize::from(applied.connections.is_some())
            + usize::from(applied.forwards.is_some())
            + usize::from(apply_quick_commands)
            + usize::from(apply_serial_profiles);
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
            quick_commands: apply_quick_commands,
            serial_profiles: apply_serial_profiles,
            sensitive_credentials: apply_sensitive_credentials,
            app_settings_sections: app_settings_entry_ids,
            plugin_ids: plugin_entry_ids,
        };

        Ok(Some(ApplyStructuredPreviewOutcome {
            local_snapshot,
            applied,
            sensitive_credentials_envelope,
            content_summary,
            manifest: preview.manifest,
            remote_metadata: preview.remote_metadata,
            selection: applied_selection,
            requires_upload_after_merge,
        }))
    }

    pub fn apply_legacy_preview(
        &self,
        connection_store: &mut ConnectionStore,
        _settings: &CloudSyncSettings,
        preview: &LegacyPreview,
        sync_password: Option<&str>,
        import_connections: bool,
        selected_connection_names: Option<Vec<String>>,
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
                selected_names: legacy_preview_selected_names(
                    import_connections,
                    selected_connection_names,
                ),
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

    async fn read_optional_object(
        &self,
        settings: &CloudSyncSettings,
        secrets: &crate::secrets::CloudSyncSecrets,
        path: &str,
    ) -> Result<Option<crate::backend::RemoteObject>> {
        self.backend
            .read_remote_object(settings, secrets, path)
            .await
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
        portable_secrets: Vec<EncryptedPortableSecret>,
        item_filter: &StructuredUploadItemFilter,
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
            let mut snapshot = connection_store.export_saved_connections_snapshot()?;
            filter_saved_connection_snapshot(&mut snapshot, item_filter.connection_ids.as_ref());
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
            let mut snapshot = forwarding_registry.export_saved_forwards_snapshot()?;
            filter_saved_forwards_snapshot(&mut snapshot, item_filter.forward_ids.as_ref());
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

        if local_snapshot.scope.sync_quick_commands {
            if let Some(revision) = local_snapshot.metadata.quick_commands_revision.as_ref() {
                let mut snapshot_json =
                    oxideterm_quick_commands::export_snapshot_json(settings_store.path())
                        .map_err(anyhow::Error::msg)?;
                let record_count = filter_quick_commands_snapshot_json(
                    &mut snapshot_json,
                    item_filter.quick_command_ids.as_ref(),
                );
                let path = quick_commands_object_path(revision);
                manifest.sections.quick_commands = Some(crate::StructuredObjectEntry {
                    revision: revision.clone(),
                    path: path.clone(),
                    record_count: Some(record_count),
                    content_type: "application/json".to_string(),
                });
                objects.push(StructuredUploadObject {
                    path,
                    bytes: snapshot_json.into_bytes(),
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
        }

        if local_snapshot.scope.sync_serial_profiles {
            let mut snapshot = connection_store.export_serial_profiles_snapshot()?;
            filter_serial_profiles_snapshot(&mut snapshot, item_filter.serial_profile_ids.as_ref());
            let bytes = serde_json::to_vec(&snapshot)?;
            let path = serial_profiles_object_path(&snapshot.revision);
            manifest.sections.serial_profiles = Some(crate::StructuredObjectEntry {
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

        if local_snapshot.scope.sync_sensitive_credentials {
            let password =
                sync_password.context("missing_sync_password: cloud sync password is required")?;
            if let Some(revision) = local_snapshot
                .metadata
                .sensitive_credentials_revision
                .as_ref()
            {
                let connection_ids = connection_store
                    .connections()
                    .iter()
                    .map(|connection| connection.id.clone())
                    .filter(|connection_id| {
                        item_filter
                            .connection_ids
                            .as_ref()
                            .is_none_or(|ids| ids.contains(connection_id))
                    })
                    .collect::<Vec<_>>();
                let bytes = export_connections_to_oxide_with_progress(
                    connection_store,
                    &connection_ids,
                    password,
                    OxideExportOptions {
                        description: Some("Cloud Sync sensitive credentials".to_string()),
                        embed_keys: false,
                        include_passwords: true,
                        include_key_passphrases: true,
                        include_managed_keys: true,
                        include_managed_key_passphrases: true,
                        portable_secrets,
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
                let path = sensitive_credentials_object_path(revision);
                manifest.sections.sensitive_credentials = Some(crate::StructuredObjectEntry {
                    revision: revision.clone(),
                    path: path.clone(),
                    record_count: Some(local_snapshot.sensitive_credentials_record_count),
                    content_type: crate::OXIDE_CONTENT_TYPE.to_string(),
                });
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
    pub item_filter: StructuredUploadItemFilter,
    pub portable_secrets: Vec<EncryptedPortableSecret>,
}

#[derive(Clone, Debug, Default)]
pub struct StructuredUploadItemFilter {
    // A missing set means the whole resource group is selected; an empty set means upload no items.
    pub connection_ids: Option<BTreeSet<String>>,
    pub forward_ids: Option<BTreeSet<String>>,
    pub quick_command_ids: Option<BTreeSet<String>>,
    pub serial_profile_ids: Option<BTreeSet<String>>,
}

#[derive(Clone, Debug)]
pub struct UploadOutcome {
    pub revision: String,
    pub revision_sequence: u64,
    pub etag: Option<String>,
    pub local_snapshot: CloudSyncLocalSnapshot,
    pub manifest: crate::StructuredManifest,
    pub created_remote_id: Option<String>,
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

async fn read_optional_snapshot_at_revision<T>(
    service: &CloudSyncOperationService,
    settings: &CloudSyncSettings,
    secrets: &crate::secrets::CloudSyncSecrets,
    revision: Option<&str>,
    path_for_revision: impl FnOnce(&str) -> String,
) -> Result<Option<T>>
where
    T: DeserializeOwned,
{
    let Some(revision) = revision.filter(|revision| !revision.is_empty()) else {
        return Ok(None);
    };
    let path = path_for_revision(revision);
    service
        .read_optional_object(settings, secrets, &path)
        .await?
        .map(|object| serde_json::from_slice(&object.bytes).map_err(anyhow::Error::from))
        .transpose()
}

async fn read_optional_text_at_revision(
    service: &CloudSyncOperationService,
    settings: &CloudSyncSettings,
    secrets: &crate::secrets::CloudSyncSecrets,
    revision: Option<&str>,
    path_for_revision: impl FnOnce(&str) -> String,
) -> Result<Option<String>> {
    let Some(revision) = revision.filter(|revision| !revision.is_empty()) else {
        return Ok(None);
    };
    let path = path_for_revision(revision);
    service
        .read_optional_object(settings, secrets, &path)
        .await?
        .map(|object| String::from_utf8(object.bytes).map_err(anyhow::Error::from))
        .transpose()
}

fn merge_structured_preview_fields(
    connection_store: &ConnectionStore,
    forwarding_registry: &ForwardingRegistry,
    settings_store: &SettingsStore,
    preview: &mut StructuredPreview,
    selection: &StructuredApplySelection,
    conflict_strategy: &ConflictStrategy,
) -> Result<bool> {
    let now_rfc3339 = Utc::now().to_rfc3339();
    let mut changed = false;
    if selection.connections
        && let (Some(remote), Some(base)) = (
            preview.connections_snapshot.as_mut(),
            preview.base_connections_snapshot.as_ref(),
        )
    {
        let local = connection_store.export_saved_connections_snapshot()?;
        changed |= merge_connection_records(remote, base, &local, conflict_strategy, &now_rfc3339)?;
    }
    if selection.forwards
        && let (Some(remote), Some(base)) = (
            preview.forwards_snapshot.as_mut(),
            preview.base_forwards_snapshot.as_ref(),
        )
    {
        let local = forwarding_registry.export_saved_forwards_snapshot()?;
        changed |= merge_forward_records(remote, base, &local, conflict_strategy, &now_rfc3339)?;
    }
    if selection.quick_commands
        && let (Some(remote_json), Some(base_json)) = (
            preview.quick_commands_snapshot_json.as_mut(),
            preview.base_quick_commands_snapshot_json.as_deref(),
        )
    {
        let local_json = oxideterm_quick_commands::export_snapshot_json(settings_store.path())
            .map_err(anyhow::Error::msg)?;
        changed |= merge_quick_command_records(
            remote_json,
            base_json,
            &local_json,
            conflict_strategy,
            Utc::now().timestamp_millis().max(0) as u64,
        )?;
    }
    if selection.serial_profiles
        && let (Some(remote), Some(base)) = (
            preview.serial_profiles_snapshot.as_mut(),
            preview.base_serial_profiles_snapshot.as_ref(),
        )
    {
        let local = connection_store.export_serial_profiles_snapshot()?;
        changed |=
            merge_serial_profile_records(remote, base, &local, conflict_strategy, Utc::now())?;
    }
    Ok(changed)
}

fn merge_connection_records(
    remote: &mut SavedConnectionsSyncSnapshot,
    base: &SavedConnectionsSyncSnapshot,
    local: &SavedConnectionsSyncSnapshot,
    conflict_strategy: &ConflictStrategy,
    merged_at: &str,
) -> Result<bool> {
    let base_records = sync_records_by_id(&base.records);
    let local_records = sync_records_by_id(&local.records);
    let mut changed = false;
    for remote_record in &mut remote.records {
        if remote_record.deleted {
            continue;
        }
        let Some(remote_payload) = remote_record.payload.as_ref() else {
            continue;
        };
        let Some(base_payload) = base_records
            .get(remote_record.id.as_str())
            .filter(|record| !record.deleted)
            .and_then(|record| record.payload.as_ref())
        else {
            continue;
        };
        let Some(local_payload) = local_records
            .get(remote_record.id.as_str())
            .filter(|record| !record.deleted)
            .and_then(|record| record.payload.as_ref())
        else {
            continue;
        };
        if let Some(merged_payload) = merge_structured_model_fields(
            base_payload,
            local_payload,
            remote_payload,
            conflict_strategy,
        )? {
            remote_record.payload = Some(merged_payload);
            remote_record.updated_at = merged_at.to_string();
            changed = true;
        }
    }
    Ok(changed)
}

fn merge_forward_records(
    remote: &mut SavedForwardsSyncSnapshot,
    base: &SavedForwardsSyncSnapshot,
    local: &SavedForwardsSyncSnapshot,
    conflict_strategy: &ConflictStrategy,
    merged_at: &str,
) -> Result<bool> {
    let base_records = forward_records_by_id(&base.records);
    let local_records = forward_records_by_id(&local.records);
    let mut changed = false;
    for remote_record in &mut remote.records {
        if remote_record.deleted {
            continue;
        }
        let Some(remote_payload) = remote_record.payload.as_ref() else {
            continue;
        };
        let Some(base_payload) = base_records
            .get(remote_record.id.as_str())
            .filter(|record| !record.deleted)
            .and_then(|record| record.payload.as_ref())
        else {
            continue;
        };
        let Some(local_payload) = local_records
            .get(remote_record.id.as_str())
            .filter(|record| !record.deleted)
            .and_then(|record| record.payload.as_ref())
        else {
            continue;
        };
        if let Some(merged_payload) = merge_structured_model_fields(
            base_payload,
            local_payload,
            remote_payload,
            conflict_strategy,
        )? {
            remote_record.payload = Some(merged_payload);
            remote_record.updated_at = merged_at.to_string();
            changed = true;
        }
    }
    Ok(changed)
}

fn merge_serial_profile_records(
    remote: &mut SerialProfilesSyncSnapshot,
    base: &SerialProfilesSyncSnapshot,
    local: &SerialProfilesSyncSnapshot,
    conflict_strategy: &ConflictStrategy,
    merged_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    let base_records = base
        .records
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();
    let local_records = local
        .records
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();
    let mut changed = false;
    for remote_profile in &mut remote.records {
        let Some(base_profile) = base_records.get(remote_profile.id.as_str()).copied() else {
            continue;
        };
        let Some(local_profile) = local_records.get(remote_profile.id.as_str()).copied() else {
            continue;
        };
        if let Some(mut merged_profile) = merge_structured_model_fields(
            base_profile,
            local_profile,
            remote_profile,
            conflict_strategy,
        )? {
            merged_profile.updated_at = merged_at;
            *remote_profile = merged_profile;
            changed = true;
        }
    }
    Ok(changed)
}

fn merge_quick_command_records(
    remote_json: &mut String,
    base_json: &str,
    local_json: &str,
    conflict_strategy: &ConflictStrategy,
    merged_at: u64,
) -> Result<bool> {
    let base = serde_json::from_str::<QuickCommandsSnapshot>(base_json)?;
    let local = serde_json::from_str::<QuickCommandsSnapshot>(local_json)?;
    let mut remote = serde_json::from_str::<QuickCommandsSnapshot>(remote_json)?;
    let mut changed = false;
    changed |= merge_quick_command_categories(
        &mut remote.categories,
        &base.categories,
        &local.categories,
        conflict_strategy,
    )?;
    changed |= merge_quick_commands(
        &mut remote.commands,
        &base.commands,
        &local.commands,
        conflict_strategy,
        merged_at,
    )?;
    if changed {
        remote.updated_at = merged_at;
        *remote_json = serde_json::to_string(&remote)?;
    }
    Ok(changed)
}

fn merge_quick_command_categories(
    remote: &mut [QuickCommandCategory],
    base: &[QuickCommandCategory],
    local: &[QuickCommandCategory],
    conflict_strategy: &ConflictStrategy,
) -> Result<bool> {
    let base_records = base
        .iter()
        .map(|category| (category.id.as_str(), category))
        .collect::<BTreeMap<_, _>>();
    let local_records = local
        .iter()
        .map(|category| (category.id.as_str(), category))
        .collect::<BTreeMap<_, _>>();
    let mut changed = false;
    for remote_category in remote {
        let Some(base_category) = base_records.get(remote_category.id.as_str()).copied() else {
            continue;
        };
        let Some(local_category) = local_records.get(remote_category.id.as_str()).copied() else {
            continue;
        };
        if let Some(merged_category) = merge_structured_model_fields(
            base_category,
            local_category,
            remote_category,
            conflict_strategy,
        )? {
            *remote_category = merged_category;
            changed = true;
        }
    }
    Ok(changed)
}

fn merge_quick_commands(
    remote: &mut [QuickCommand],
    base: &[QuickCommand],
    local: &[QuickCommand],
    conflict_strategy: &ConflictStrategy,
    merged_at: u64,
) -> Result<bool> {
    let base_records = base
        .iter()
        .map(|command| (command.id.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    let local_records = local
        .iter()
        .map(|command| (command.id.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    let mut changed = false;
    for remote_command in remote {
        let Some(base_command) = base_records.get(remote_command.id.as_str()).copied() else {
            continue;
        };
        let Some(local_command) = local_records.get(remote_command.id.as_str()).copied() else {
            continue;
        };
        if let Some(mut merged_command) = merge_structured_model_fields(
            base_command,
            local_command,
            remote_command,
            conflict_strategy,
        )? {
            merged_command.updated_at = merged_at;
            *remote_command = merged_command;
            changed = true;
        }
    }
    Ok(changed)
}

fn sync_records_by_id(
    records: &[oxideterm_connections::SavedConnectionSyncRecord],
) -> BTreeMap<&str, &oxideterm_connections::SavedConnectionSyncRecord> {
    records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect()
}

fn forward_records_by_id(
    records: &[oxideterm_forwarding::SavedForwardSyncRecord],
) -> BTreeMap<&str, &oxideterm_forwarding::SavedForwardSyncRecord> {
    records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect()
}

/// Three-way merges a structured sync model using base/local/remote values.
///
/// Returns `None` when the remote model already represents the effective result.
pub fn merge_structured_model_fields<T>(
    base: &T,
    local: &T,
    remote: &T,
    conflict_strategy: &ConflictStrategy,
) -> Result<Option<T>>
where
    T: Serialize + DeserializeOwned,
{
    let base_value = serde_json::to_value(base)?;
    let local_value = serde_json::to_value(local)?;
    let remote_value = serde_json::to_value(remote)?;
    let (Some(merged_value), used_local) = merge_structured_json_value(
        Some(&base_value),
        Some(&local_value),
        Some(&remote_value),
        conflict_strategy,
    ) else {
        return Ok(None);
    };
    if !used_local || merged_value == remote_value {
        return Ok(None);
    }
    serde_json::from_value(merged_value)
        .map(Some)
        .map_err(anyhow::Error::from)
}

fn merge_structured_json_value(
    base: Option<&Value>,
    local: Option<&Value>,
    remote: Option<&Value>,
    conflict_strategy: &ConflictStrategy,
) -> (Option<Value>, bool) {
    if local == remote {
        return (remote.cloned(), false);
    }
    if base == local {
        return (remote.cloned(), false);
    }
    if base == remote {
        return (local.cloned(), true);
    }
    if let (
        Some(Value::Object(base_object)),
        Some(Value::Object(local_object)),
        Some(Value::Object(remote_object)),
    ) = (base, local, remote)
    {
        let mut keys = BTreeSet::new();
        keys.extend(base_object.keys().map(String::as_str));
        keys.extend(local_object.keys().map(String::as_str));
        keys.extend(remote_object.keys().map(String::as_str));
        let mut merged = serde_json::Map::new();
        let mut used_local = false;
        for key in keys {
            let (value, child_used_local) = merge_structured_json_value(
                base_object.get(key),
                local_object.get(key),
                remote_object.get(key),
                conflict_strategy,
            );
            used_local |= child_used_local;
            if let Some(value) = value {
                merged.insert(key.to_string(), value);
            }
        }
        return (Some(Value::Object(merged)), used_local);
    }
    if merge_conflict_prefers_local(conflict_strategy) {
        (local.cloned(), true)
    } else {
        (remote.cloned(), false)
    }
}

fn merge_conflict_prefers_local(conflict_strategy: &ConflictStrategy) -> bool {
    !matches!(conflict_strategy, ConflictStrategy::Replace)
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
    pub quick_commands_snapshot_json: Option<String>,
    pub serial_profiles_snapshot: Option<SerialProfilesSyncSnapshot>,
    pub base_connections_snapshot: Option<SavedConnectionsSyncSnapshot>,
    pub base_forwards_snapshot: Option<SavedForwardsSyncSnapshot>,
    pub base_quick_commands_snapshot_json: Option<String>,
    pub base_serial_profiles_snapshot: Option<SerialProfilesSyncSnapshot>,
    pub sensitive_credentials_entry: Option<Vec<u8>>,
    pub sensitive_credentials_preview: Option<ImportPreview>,
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
    pub sensitive_credentials_envelope: Option<ImportResultEnvelope>,
    pub content_summary: CloudSyncHistorySummary,
    pub manifest: StructuredManifest,
    pub remote_metadata: RemoteMetadata,
    pub selection: StructuredApplySelection,
    pub requires_upload_after_merge: bool,
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
            quick_commands: self.quick_commands_snapshot_json.is_some(),
            serial_profiles: self.serial_profiles_snapshot.is_some(),
            sensitive_credentials: self.sensitive_credentials_entry.is_some(),
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

fn legacy_preview_selected_names(
    import_connections: bool,
    selected_connection_names: Option<Vec<String>>,
) -> Option<Vec<String>> {
    if import_connections {
        selected_connection_names
    } else {
        Some(Vec::new())
    }
}

fn filter_saved_connection_snapshot(
    snapshot: &mut SavedConnectionsSyncSnapshot,
    selected_ids: Option<&BTreeSet<String>>,
) {
    if let Some(selected_ids) = selected_ids {
        snapshot
            .records
            .retain(|record| selected_ids.contains(&record.id));
    }
}

fn filter_saved_forwards_snapshot(
    snapshot: &mut SavedForwardsSyncSnapshot,
    selected_ids: Option<&BTreeSet<String>>,
) {
    if let Some(selected_ids) = selected_ids {
        snapshot
            .records
            .retain(|record| selected_ids.contains(&record.id));
    }
}

fn filter_serial_profiles_snapshot(
    snapshot: &mut SerialProfilesSyncSnapshot,
    selected_ids: Option<&BTreeSet<String>>,
) {
    if let Some(selected_ids) = selected_ids {
        snapshot
            .records
            .retain(|profile| selected_ids.contains(&profile.id));
    }
}

fn filter_quick_commands_snapshot_json(
    snapshot_json: &mut String,
    selected_ids: Option<&BTreeSet<String>>,
) -> usize {
    // Keep filtering at the serialized snapshot boundary so upload writes exactly the chosen object.
    let Ok(mut snapshot) =
        serde_json::from_str::<oxideterm_quick_commands::QuickCommandsSnapshot>(snapshot_json)
    else {
        return 0;
    };
    if let Some(selected_ids) = selected_ids {
        snapshot
            .commands
            .retain(|command| selected_ids.contains(&command.id));
        if let Ok(filtered_json) = serde_json::to_string(&snapshot) {
            *snapshot_json = filtered_json;
        }
    }
    snapshot.commands.len()
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
            || dirty_sections.quick_commands
            || dirty_sections.serial_profiles
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
    if dirty_sections.quick_commands && remote.quick_commands != previous.quick_commands {
        return true;
    }
    if dirty_sections.serial_profiles && remote.serial_profiles != previous.serial_profiles {
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

        let merged =
            merge_structured_model_fields(&base, &local, &remote, &ConflictStrategy::Merge)
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

    #[test]
    fn legacy_preview_uses_selected_connection_names_when_importing() {
        let selected_names = legacy_preview_selected_names(
            true,
            Some(vec!["Prod".to_string(), "Staging".to_string()]),
        )
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
}
