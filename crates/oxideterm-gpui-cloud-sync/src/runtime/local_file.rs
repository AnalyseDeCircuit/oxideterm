use super::*;
use oxideterm_cloud_sync::secrets::{CloudSyncSecretProvider, SecretReadMode};

impl CloudSyncRuntime<'_> {
    pub fn start_cloud_sync_local_file(
        &mut self,
        exporting: bool,
        preview_only: bool,
        cx: &mut App,
    ) {
        if self.cloud_sync.read(cx).operation_in_flight() {
            return;
        }
        if !self.persist_cloud_sync_configuration(false, cx) {
            return;
        }
        let state = self.cloud_sync.read(cx).controller.store.state();
        let hints = state.secret_hints.clone();
        let strategy = state.settings.default_conflict_strategy.clone();
        let raw_scope = self
            .cloud_sync
            .read(cx)
            .view
            .upload_selection
            .as_ref()
            .filter(|_| !preview_only)
            .map(|selection| selection.raw_scope(&state.sync_scope))
            .unwrap_or_else(|| state.sync_scope.clone());
        let scope = normalize_sync_scope(Some(&raw_scope), &[]);
        let filter = self
            .cloud_sync
            .read(cx)
            .view
            .upload_selection
            .as_ref()
            .filter(|_| !preview_only)
            .map(CloudSyncUploadSelection::item_filter)
            .unwrap_or_default();
        let secrets = if exporting {
            match self.collect_cloud_sync_sensitive_portable_secrets(&raw_scope) {
                Ok(secrets) => secrets,
                Err(error) => {
                    self.finish_cloud_sync_error("export", error, cx);
                    return;
                }
            }
        } else {
            Vec::new()
        };
        let connections = self.connection_store.clone();
        let settings = self.settings_store.clone();
        let forwarding = self.forwarding_registry.clone();
        let service = self.cloud_sync.read(cx).controller.service.clone();
        let picker = (!preview_only).then(|| (self.file_dialog)(cx, exporting, self.i18n));
        let local_audit =
            (!preview_only).then(|| {
                let mut audit = oxideterm_audit::AuditOperation::begin(
                    oxideterm_audit::AuditCategory::Configuration,
                    if exporting {
                        "cloud_sync_local_export"
                    } else {
                        "cloud_sync_local_import_preview"
                    },
                    None,
                    Some("oxide_file"),
                );
                let selected_kinds = [
                    scope.sync_connections,
                    scope.sync_forwards,
                    scope.sync_quick_commands,
                    scope.sync_serial_profiles,
                    scope.sync_container_profiles,
                    scope.sync_ble_profiles,
                    scope.sync_telnet_profiles,
                    scope.sync_mosh_profiles,
                    scope.sync_remote_desktop_profiles,
                    scope.sync_app_settings,
                    scope.sync_plugin_settings,
                ]
                .into_iter()
                .filter(|selected| *selected)
                .count();
                audit.summary(&format!(
                "direction={},format=oxide,scope_kinds={selected_kinds},sensitive_credentials={}",
                if exporting { "export" } else { "import_preview" },
                scope.sync_sensitive_credentials,
            ));
                audit
            });
        let task = self.cloud_sync.update(cx, |_, cx| {
            cx.spawn(async move |entity, cx| {
                let path = match picker {
                    Some(picker) => picker.await,
                    None => Ok(None),
                };
                let cancelled = matches!(&path, Ok(None)) && !preview_only;
                let mut local_audit = local_audit;
                let result = match path {
                    Ok(None) if !preview_only => {
                        if let Some(audit) = local_audit.take() {
                            audit.finish(
                                oxideterm_audit::AuditOutcome::Cancelled,
                                oxideterm_audit::AuditEvidence::Request,
                                None,
                                None,
                            );
                        }
                        Ok(None)
                    }
                    Err(error) => {
                        if let Some(audit) = local_audit.take() {
                            audit.finish(
                                oxideterm_audit::AuditOutcome::Failed,
                                oxideterm_audit::AuditEvidence::Protocol,
                                None,
                                None,
                            );
                        }
                        Err(error)
                    }
                    Ok(path) => {
                        cx.background_executor()
                            .spawn(async move {
                                let run = || -> anyhow::Result<Option<CloudSyncPendingPreview>> {
                                    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
                                    let password = provider
                                    .get_secret(secret_keys::SYNC_PASSWORD, SecretReadMode::Prompt)?
                                    .filter(|password| !password.is_empty())
                                    .ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "missing_sync_password: cloud sync password is required"
                                        )
                                    })?;
                                    let bytes = if exporting {
                                        service.export_local_file(
                                            &connections,
                                            &forwarding,
                                            &settings,
                                            password.as_str(),
                                            &scope,
                                            &filter,
                                            secrets,
                                        )?
                                    } else {
                                        std::fs::read(path.as_ref().ok_or_else(|| {
                                            anyhow::anyhow!("missing local file")
                                        })?)?
                                    };
                                    if exporting && !preview_only {
                                        oxideterm_atomic_file::durable_write(
                                            path.as_ref().ok_or_else(|| {
                                                anyhow::anyhow!("missing local file")
                                            })?,
                                            &bytes,
                                        )?;
                                        Ok(None)
                                    } else {
                                        Ok(Some(CloudSyncPendingPreview::Legacy {
                                            preview: service.preview_local_file(
                                                &connections,
                                                bytes,
                                                password.as_str(),
                                                strategy,
                                            )?,
                                            source: CloudSyncPreviewSource::LocalFile,
                                        }))
                                    }
                                };
                                let result = run().map_err(|error| error.to_string());
                                if let Some(audit) = local_audit.take() {
                                    audit.finish(
                                        if result.is_ok() {
                                            oxideterm_audit::AuditOutcome::Succeeded
                                        } else {
                                            oxideterm_audit::AuditOutcome::Failed
                                        },
                                        oxideterm_audit::AuditEvidence::Protocol,
                                        None,
                                        None,
                                    );
                                }
                                result
                            })
                            .await
                    }
                };
                let _ = entity.update(cx, |sync, cx| {
                    sync.controller.local_file_result = Some(
                        result
                            .map(|preview| (exporting, cancelled, preview))
                            .map_err(|error| (exporting, error)),
                    );
                    cx.emit(CloudSyncWorkspaceEvent::LocalFileFinished);
                });
            })
        });
        self.cloud_sync.update(cx, |cloud_sync, _| {
            cloud_sync.controller.local_file_task = Some(task);
        });
        self.notify(cx);
    }
    pub(super) fn finish_local_file(&mut self, cx: &mut App) {
        let Some(result) = self.cloud_sync.update(cx, |sync, _| {
            let result = sync.controller.local_file_result.take()?;
            sync.controller.local_file_task = None;
            Some(result)
        }) else {
            return;
        };
        match result {
            Ok((true, _, Some(preview))) => self.finish_cloud_sync_upload_preview(preview, cx),
            Ok((false, _, Some(preview))) => self.finish_cloud_sync_pull_preview(preview, cx),
            Ok((true, false, None)) => {
                self.cloud_sync.update(cx, |sync, _| {
                    sync.view.upload_preview = None;
                    sync.view.upload_selection = None;
                    sync.controller.store.state_mut().last_error = None;
                });
                self.push_cloud_sync_toast(
                    self.i18n.t("plugin.cloud_sync.actions.export_local"),
                    None,
                    CloudSyncNoticeKind::Success,
                    cx,
                );
            }
            Ok(_) => {}
            Err((exporting, error)) => {
                self.finish_cloud_sync_error(if exporting { "export" } else { "import" }, error, cx)
            }
        }
        self.notify(cx);
    }
}

pub(crate) type LocalFileResult =
    Result<(bool, bool, Option<CloudSyncPendingPreview>), (bool, String)>;
