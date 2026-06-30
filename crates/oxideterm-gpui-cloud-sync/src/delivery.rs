// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync operation delivery adapters.
//!
//! The GPUI app owns scheduling and rendering, while this module owns the
//! request/response glue between the Cloud Sync service, local stores, rollback
//! backup encoding, keychain hints, and UI delivery messages.

use std::{
    collections::BTreeMap,
    fmt,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use oxideterm_cloud_sync::{
    CloudSyncSettings, MAX_ROLLBACK_BACKUP_BYTES, StructuredSectionRevisions,
    backend::{CloudSyncBackend, GithubDeviceTokenPoll, MicrosoftDeviceTokenPoll},
    operation::{
        ApplyLegacyPreviewOutcome, ApplyStructuredPreviewOutcome, CloudSyncOperationService,
        LegacyPreview, StructuredPreview, UploadOptions, UploadOutcome,
    },
    progress::{CloudSyncProgress, CloudSyncProgressSink, CloudSyncProgressStage},
    secret_keys,
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
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use zeroize::Zeroizing;

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
    GithubOauthCode(CloudSyncOauthDevicePrompt),
    GithubOauthFinished(CloudSyncActionResult<()>),
    MicrosoftOauthCode(CloudSyncOauthDevicePrompt),
    MicrosoftOauthFinished(CloudSyncActionResult<()>),
    GoogleOauthUrl(CloudSyncOauthBrowserPrompt),
    GoogleOauthFinished(CloudSyncActionResult<()>),
}

#[derive(Debug)]
pub struct CloudSyncActionResult<T> {
    pub result: Result<T, String>,
    pub secret_hints: BTreeMap<String, bool>,
}

#[derive(Debug)]
pub struct CloudSyncOauthDevicePrompt {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[derive(Debug)]
pub struct CloudSyncOauthBrowserPrompt {
    pub authorization_url: String,
    pub expires_in: u64,
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

pub async fn deliver_cloud_sync_github_oauth(
    tx: Sender<CloudSyncDelivery>,
    client_id: String,
    hints: BTreeMap<String, bool>,
) {
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let backend = CloudSyncBackend::new();
    let result = async {
        let device = backend.start_github_device_flow(&client_id).await?;
        let _ = tx.send(CloudSyncDelivery::GithubOauthCode(
            CloudSyncOauthDevicePrompt {
                user_code: device.user_code.clone(),
                verification_uri: device.verification_uri.clone(),
                expires_in: device.expires_in,
            },
        ));
        let deadline = Instant::now() + Duration::from_secs(device.expires_in);
        let mut interval = device.interval.max(1);
        loop {
            if Instant::now() >= deadline {
                anyhow::bail!("github_oauth_expired: GitHub device code expired");
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;
            match backend
                .poll_github_device_flow(&client_id, &device.device_code, interval)
                .await?
            {
                GithubDeviceTokenPoll::Pending { interval: next } => {
                    interval = next.max(1);
                }
                GithubDeviceTokenPoll::SlowDown { interval: next } => {
                    interval = next.max(1);
                }
                GithubDeviceTokenPoll::Token { access_token } => {
                    // Store the zeroizing OAuth token at the keychain boundary;
                    // never echo it back to the UI or progress messages.
                    provider.store_secret(secret_keys::GIT_TOKEN, Some(access_token.as_str()))?;
                    return Ok(());
                }
            }
        }
    }
    .await
    .map_err(|error: anyhow::Error| error.to_string());
    send_action_result(
        tx,
        CloudSyncDelivery::GithubOauthFinished,
        result,
        &provider,
    );
}

pub async fn deliver_cloud_sync_microsoft_oauth(
    tx: Sender<CloudSyncDelivery>,
    client_id: String,
    hints: BTreeMap<String, bool>,
) {
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let backend = CloudSyncBackend::new();
    let result = async {
        let device = backend.start_microsoft_device_flow(&client_id).await?;
        let _ = tx.send(CloudSyncDelivery::MicrosoftOauthCode(
            CloudSyncOauthDevicePrompt {
                user_code: device.user_code.clone(),
                verification_uri: device.verification_uri.clone(),
                expires_in: device.expires_in,
            },
        ));
        let deadline = Instant::now() + Duration::from_secs(device.expires_in);
        let mut interval = device.interval.max(1);
        loop {
            if Instant::now() >= deadline {
                anyhow::bail!("microsoft_oauth_expired: Microsoft device code expired");
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;
            match backend
                .poll_microsoft_device_flow(&client_id, &device.device_code, interval)
                .await?
            {
                MicrosoftDeviceTokenPoll::Pending { interval: next } => {
                    interval = next.max(1);
                }
                MicrosoftDeviceTokenPoll::SlowDown { interval: next } => {
                    interval = next.max(1);
                }
                MicrosoftDeviceTokenPoll::Token {
                    access_token,
                    refresh_token,
                } => {
                    // Store Microsoft OAuth tokens at the keychain boundary;
                    // neither token is displayed or copied into UI state.
                    provider.store_secret(secret_keys::TOKEN, Some(access_token.as_str()))?;
                    provider.store_secret(
                        secret_keys::MICROSOFT_REFRESH_TOKEN,
                        Some(refresh_token.as_str()),
                    )?;
                    return Ok(());
                }
            }
        }
    }
    .await
    .map_err(|error: anyhow::Error| error.to_string());
    send_action_result(
        tx,
        CloudSyncDelivery::MicrosoftOauthFinished,
        result,
        &provider,
    );
}

pub async fn deliver_cloud_sync_google_oauth(
    tx: Sender<CloudSyncDelivery>,
    client_id: String,
    hints: BTreeMap<String, bool>,
) {
    let mut provider = CloudSyncKeychainSecretProvider::new(hints);
    let backend = CloudSyncBackend::new();
    let result = async {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|error| {
            anyhow::anyhow!(
                "google_oauth_redirect_failed: failed to start local OAuth listener: {error}"
            )
        })?;
        let local_addr = listener.local_addr().map_err(|error| {
            anyhow::anyhow!(
                "google_oauth_redirect_failed: failed to read local OAuth listener address: {error}"
            )
        })?;
        let redirect_uri = format!(
            "http://127.0.0.1:{}/oauth/google/callback",
            local_addr.port()
        );
        let flow = backend.start_google_oauth_flow(&client_id, &redirect_uri)?;
        let _ = tx.send(CloudSyncDelivery::GoogleOauthUrl(
            CloudSyncOauthBrowserPrompt {
                authorization_url: flow.authorization_url.clone(),
                expires_in: 300,
            },
        ));
        let callback = tokio::time::timeout(Duration::from_secs(300), async {
            let (mut stream, _) = listener.accept().await.map_err(|error| {
                anyhow::anyhow!(
                    "google_oauth_redirect_failed: failed to accept OAuth redirect: {error}"
                )
            })?;
            // The localhost redirect contains a short-lived OAuth code. Keep
            // the raw request buffer zeroized after parsing so the browser
            // handoff has the same transient secret boundary as keychain reads.
            let mut buffer = Zeroizing::new(vec![0u8; 8192]);
            let bytes_read = stream.read(&mut buffer[..]).await.map_err(|error| {
                anyhow::anyhow!(
                    "google_oauth_redirect_failed: failed to read OAuth redirect: {error}"
                )
            })?;
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            let callback = parse_google_oauth_callback(&request)?;
            let callback_success = callback.error.is_none()
                && callback.state.as_deref() == Some(flow.state.as_str())
                && callback
                    .code
                    .as_ref()
                    .is_some_and(|code| !code.trim().is_empty());
            // The browser only needs an acknowledgement; the authorization
            // code itself stays inside this background task until exchange.
            write_google_oauth_callback_response(&mut stream, callback_success).await?;
            Ok::<_, anyhow::Error>(callback)
        })
        .await
        .map_err(|_| anyhow::anyhow!("google_oauth_timeout: Google OAuth login timed out"))??;

        if callback.state.as_deref() != Some(flow.state.as_str()) {
            anyhow::bail!("google_oauth_invalid_state: Google OAuth state did not match");
        }
        if let Some(error) = callback.error.as_deref() {
            return Err(google_oauth_callback_error(
                error,
                callback.error_description.as_deref(),
            ));
        }
        let code = callback.code.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "google_oauth_empty_response: Google did not return an authorization code"
            )
        })?;
        let tokens = backend
            .exchange_google_authorization_code(
                &client_id,
                code.as_str(),
                flow.code_verifier.as_str(),
                &redirect_uri,
            )
            .await?;
        provider.store_secret(secret_keys::TOKEN, Some(tokens.access_token.as_str()))?;
        if let Some(refresh_token) = tokens.refresh_token.as_ref() {
            provider.store_secret(
                secret_keys::GOOGLE_REFRESH_TOKEN,
                Some(refresh_token.as_str()),
            )?;
        }
        Ok(())
    }
    .await
    .map_err(|error: anyhow::Error| error.to_string());
    send_action_result(
        tx,
        CloudSyncDelivery::GoogleOauthFinished,
        result,
        &provider,
    );
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
    if let Some(snapshot) = preview.raw_tcp_profiles_snapshot.as_mut() {
        snapshot
            .records
            .retain(|profile| selection.selected_raw_tcp_profile_ids.contains(&profile.id));
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

#[derive(Default)]
struct GoogleOauthCallback {
    code: Option<Zeroizing<String>>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

impl fmt::Debug for GoogleOauthCallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The callback can hold an OAuth authorization code. Redact it from
        // Debug so diagnostics and test failures cannot expose bearer material.
        f.debug_struct("GoogleOauthCallback")
            .field("code", &self.code.as_ref().map(|_| "<redacted>"))
            .field("state", &self.state)
            .field("error", &self.error)
            .field("error_description", &self.error_description)
            .finish()
    }
}

fn parse_google_oauth_callback(request: &str) -> anyhow::Result<GoogleOauthCallback> {
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("google_oauth_redirect_failed: empty OAuth redirect"))?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("google_oauth_redirect_failed: malformed OAuth redirect"))?;
    let query = path.split_once('?').map(|(_, query)| query).unwrap_or("");
    let mut callback = GoogleOauthCallback::default();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = percent_decode_component(key);
        let value = percent_decode_component(value);
        match key.as_str() {
            "code" => callback.code = Some(Zeroizing::new(value)),
            "state" => callback.state = Some(value),
            "error" => callback.error = Some(value),
            "error_description" => callback.error_description = Some(value),
            _ => {}
        }
    }
    Ok(callback)
}

fn google_oauth_callback_error(error: &str, description: Option<&str>) -> anyhow::Error {
    let code = match error {
        "access_denied" => "google_oauth_denied",
        "admin_policy_enforced" => "google_oauth_admin_policy",
        "invalid_client" | "unauthorized_client" => "google_oauth_bad_client",
        "invalid_scope" => "google_oauth_missing_scope",
        "consent_required" | "interaction_required" => "google_oauth_consent_required",
        "invalid_request" => "google_oauth_invalid_request",
        _ => "google_oauth_exchange_failed",
    };
    let message = description.unwrap_or("Google OAuth browser authorization failed");
    anyhow::anyhow!("{code}: {message}")
}

async fn write_google_oauth_callback_response(
    stream: &mut tokio::net::TcpStream,
    success: bool,
) -> anyhow::Result<()> {
    let title = if success {
        "OxideTerm Google Drive login finished"
    } else {
        "OxideTerm Google Drive login failed"
    };
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>{title}</title><body><h1>{title}</h1><p>You can return to OxideTerm now.</p></body>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    let _ = stream.shutdown().await;
    Ok(())
}

fn percent_decode_component(value: &str) -> String {
    let normalized = value.replace('+', " ");
    let bytes = normalized.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_oauth_callback_code_debug_is_redacted() {
        let callback = parse_google_oauth_callback(
            "GET /oauth/google/callback?code=secret-code&state=state-1 HTTP/1.1\r\n\r\n",
        )
        .expect("callback");
        let debug = format!("{callback:?}");

        assert_eq!(
            callback.code.as_ref().map(|code| code.as_str()),
            Some("secret-code")
        );
        assert!(debug.contains("redacted"));
        assert!(!debug.contains("secret-code"));
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
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
    let app_settings_json = oxideterm_settings::export_oxide_settings_snapshot_json(
        settings_store.settings(),
        None,
        true,
    )?;
    let plugin_settings =
        oxideterm_cloud_sync::plugin_settings::load_plugin_settings(settings_store.path())
            .map_err(anyhow::Error::msg)?;
    let saved_forwards = forwarding_registry.list_all_saved_forwards();
    // Rollback coverage must include settings-only local state, not just connections.
    let has_local_data = !connection_store.connections().is_empty()
        || !saved_forwards.is_empty()
        || !app_settings_json.trim().is_empty()
        || !plugin_settings.is_empty();
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
    let forwards = saved_forwards
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
            raw_tcp_profiles: metadata.raw_tcp_profiles_count.unwrap_or(0),
            sensitive_credentials: metadata.portable_secret_count.unwrap_or(0),
        }),
    }))
}
