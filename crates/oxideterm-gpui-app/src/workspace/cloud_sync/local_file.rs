// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_cloud_sync::secrets::{CloudSyncSecretProvider, SecretReadMode};

impl WorkspaceApp {
    pub(super) fn start_cloud_sync_local_file(
        &mut self,
        exporting: bool,
        preview_only: bool,
        cx: &mut Context<Self>,
    ) {
        if self.cloud_sync.read(cx).operation_in_flight()
            || self.cloud_sync.read(cx).view.local_file_task.is_some()
        {
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
            match self.collect_cloud_sync_sensitive_portable_secrets(&raw_scope, cx) {
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
        let forwarding = self.forwarding_service.registry().clone();
        let service = self.cloud_sync.read(cx).controller.service.clone();
        let import_picker = (!exporting).then(|| {
            cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(self.i18n.t("plugin.cloud_sync.actions.import_local").into()),
            })
        });
        let export_picker = (exporting && !preview_only).then(|| {
            let directory = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            cx.prompt_for_new_path(
                &directory,
                Some(&format!(
                    "oxideterm-{}.oxide",
                    Utc::now().format("%Y%m%d-%H%M%S")
                )),
            )
        });
        let task = cx.spawn(async move |workspace, cx| {
            let path = async {
                if let Some(picker) = import_picker {
                    Ok(picker
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?
                        .and_then(|paths| paths.into_iter().next()))
                } else if let Some(picker) = export_picker {
                    picker
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())
                } else {
                    Ok(None)
                }
            }
            .await;
            let cancelled = matches!(&path, Ok(None)) && !preview_only;
            let result = match path {
                Ok(None) if !preview_only => Ok(None),
                Err(error) => Err(error),
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
                                    std::fs::read(
                                        path.as_ref()
                                            .ok_or_else(|| anyhow::anyhow!("missing local file"))?,
                                    )?
                                };
                                if exporting && !preview_only {
                                    oxideterm_atomic_file::durable_write(
                                        path.as_ref()
                                            .ok_or_else(|| anyhow::anyhow!("missing local file"))?,
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
                            run().map_err(|error| error.to_string())
                        })
                        .await
                }
            };
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.cloud_sync.update(cx, |cloud_sync, _| {
                    cloud_sync.view.local_file_task = None;
                });
                match result {
                    Ok(Some(preview)) if exporting => {
                        workspace.finish_cloud_sync_upload_preview(preview, cx)
                    }
                    Ok(Some(preview)) => workspace.finish_cloud_sync_pull_preview(preview, cx),
                    Ok(None) if exporting && !cancelled => {
                        workspace.cloud_sync.update(cx, |cloud_sync, _| {
                            cloud_sync.view.upload_preview = None;
                            cloud_sync.view.upload_selection = None;
                            cloud_sync.controller.store.state_mut().last_error = None;
                        });
                        workspace.push_cloud_sync_toast(
                            workspace.i18n.t("plugin.cloud_sync.actions.export_local"),
                            None,
                            TerminalNoticeVariant::Success,
                            cx,
                        );
                    }
                    Ok(None) => {}
                    Err(error) => workspace.finish_cloud_sync_error(
                        if exporting { "export" } else { "import" },
                        error,
                        cx,
                    ),
                }
                cx.notify();
            });
        });
        self.cloud_sync.update(cx, |cloud_sync, _| {
            cloud_sync.view.local_file_task = Some(task);
        });
        cx.notify();
    }
}
