// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncOperationService {
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
}
