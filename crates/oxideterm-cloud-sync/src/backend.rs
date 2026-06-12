// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeSet, time::Duration};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use reqwest::{
    Client, Method, RequestBuilder, Response, StatusCode, Url,
    header::{
        ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, ETAG, HeaderMap, HeaderName,
        HeaderValue, RETRY_AFTER, USER_AGENT,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    BackendType, CloudSyncSettings, OXIDE_CONTENT_TYPE, StructuredSectionRevisions,
    secrets::{CloudSyncSecrets, backend_uses_basic, backend_uses_token},
};

const DROPBOX_API_BASE: &str = "https://api.dropboxapi.com/2";
const DROPBOX_CONTENT_BASE: &str = "https://content.dropboxapi.com/2";
const MICROSOFT_GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
const MICROSOFT_AUTH_TENANT: &str = "common";
const MICROSOFT_ONEDRIVE_SCOPE: &str = "offline_access Files.ReadWrite.AppFolder";
const CLOUD_REQUEST_MAX_RETRY_ATTEMPTS: usize = 3;
const CLOUD_REQUEST_MAX_RETRY_AFTER: Duration = Duration::from_secs(30);
const DEFAULT_GIT_API_ENDPOINT: &str = "https://api.github.com";
const GITHUB_API_VERSION: &str = "2022-11-28";
const GIST_OBJECT_PREFIX: &str = "OXIDETERM-GIST-BLOB-V1\n";

mod s3;
use s3::{s3_blob_url, s3_paths};

#[derive(Clone, Debug)]
pub struct CloudSyncBackend {
    client: Client,
}

impl Default for CloudSyncBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudSyncBackend {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    pub async fn create_github_gist(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<String> {
        let namespace = gist_namespace(config);
        let filename = format!(
            "oxideterm-{}-readme.txt",
            gist_safe_filename_component(&namespace)
        );
        let response = execute_cloud_request(
            self.client
                .post(format!("{DEFAULT_GIT_API_ENDPOINT}/gists"))
                .headers(gist_headers(secrets)?)
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&json!({
                    "description": format!("OxideTerm Cloud Sync ({namespace})"),
                    "public": false,
                    "files": {
                        filename: {
                            "content": "OxideTerm Cloud Sync storage. Do not edit these files manually.\n",
                        }
                    }
                }))?),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(gist_value_error(
                status,
                &value,
                "gist_create",
                "Failed to create GitHub Gist",
            ));
        }
        value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .context("gist_create_missing_id: GitHub did not return a Gist ID")
    }

    pub async fn start_github_device_flow(&self, client_id: &str) -> Result<GithubDeviceCode> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_github_oauth_client_id: GitHub OAuth client ID is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post("https://github.com/login/device/code")
                .header(ACCEPT, "application/json")
                .form(&[("client_id", client_id), ("scope", "gist")]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<GithubDeviceCodeResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success() {
            bail!("github_oauth_start_failed: GitHub rejected the device authorization request");
        }
        Ok(GithubDeviceCode {
            device_code: value.device_code,
            user_code: value.user_code,
            verification_uri: value.verification_uri,
            expires_in: value.expires_in,
            interval: value.interval,
        })
    }

    pub async fn poll_github_device_flow(
        &self,
        client_id: &str,
        device_code: &str,
        interval: u64,
    ) -> Result<GithubDeviceTokenPoll> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_github_oauth_client_id: GitHub OAuth client ID is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post("https://github.com/login/oauth/access_token")
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<GithubDeviceTokenResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success() {
            bail!("github_oauth_poll_failed: GitHub rejected the device token request");
        }
        if let Some(access_token) = value.access_token {
            // The OAuth access token leaves the HTTP response as a String only
            // long enough to move into a zeroizing owner for keychain storage.
            return Ok(GithubDeviceTokenPoll::Token {
                access_token: Zeroizing::new(access_token),
            });
        }
        match value.error.as_deref() {
            Some("authorization_pending") => Ok(GithubDeviceTokenPoll::Pending {
                interval: value.interval.unwrap_or(interval),
            }),
            Some("slow_down") => Ok(GithubDeviceTokenPoll::SlowDown {
                interval: value.interval.unwrap_or(interval + 5),
            }),
            Some("expired_token") => bail!("github_oauth_expired: GitHub device code expired"),
            Some("access_denied") => bail!("github_oauth_denied: GitHub authorization was denied"),
            Some("incorrect_client_credentials") => {
                bail!("github_oauth_bad_client: GitHub OAuth client ID is invalid")
            }
            Some(error) => {
                let description = value
                    .error_description
                    .as_deref()
                    .unwrap_or("GitHub OAuth device flow failed");
                bail!("github_oauth_{error}: {description}")
            }
            None => bail!("github_oauth_empty_response: GitHub did not return an access token"),
        }
    }

    pub async fn start_microsoft_device_flow(
        &self,
        client_id: &str,
    ) -> Result<MicrosoftDeviceCode> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_microsoft_oauth_client_id: Microsoft OAuth client ID is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post(microsoft_device_code_url())
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("scope", MICROSOFT_ONEDRIVE_SCOPE),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(anyhow::Error::new)?;
        if !status.is_success() {
            return Err(microsoft_oauth_value_error(
                &value,
                "microsoft_oauth_start_failed",
            ));
        }
        let value = serde_json::from_value::<MicrosoftDeviceCodeResponse>(value)
            .map_err(anyhow::Error::new)?;
        Ok(MicrosoftDeviceCode {
            device_code: value.device_code,
            user_code: value.user_code,
            verification_uri: value.verification_uri,
            expires_in: value.expires_in,
            interval: value.interval,
        })
    }

    pub async fn poll_microsoft_device_flow(
        &self,
        client_id: &str,
        device_code: &str,
        interval: u64,
    ) -> Result<MicrosoftDeviceTokenPoll> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_microsoft_oauth_client_id: Microsoft OAuth client ID is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post(microsoft_token_url())
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<MicrosoftTokenResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success()
            && !matches!(
                value.error.as_deref(),
                Some("authorization_pending" | "slow_down")
            )
        {
            return Err(microsoft_oauth_error(&value, "microsoft_oauth_poll_failed"));
        }
        if let Some(access_token) = value.access_token {
            let refresh_token = value.refresh_token.context(
                "microsoft_oauth_empty_response: Microsoft did not return a refresh token",
            )?;
            // Microsoft returns opaque tokens; move both into zeroizing owners
            // immediately and never derive behavior from token contents.
            return Ok(MicrosoftDeviceTokenPoll::Token {
                access_token: Zeroizing::new(access_token),
                refresh_token: Zeroizing::new(refresh_token),
            });
        }
        match value.error.as_deref() {
            Some("authorization_pending") => Ok(MicrosoftDeviceTokenPoll::Pending {
                interval: value.interval.unwrap_or(interval),
            }),
            Some("slow_down") => Ok(MicrosoftDeviceTokenPoll::SlowDown {
                interval: value.interval.unwrap_or(interval + 5),
            }),
            _ => Err(microsoft_oauth_error(
                &value,
                "microsoft_oauth_empty_response",
            )),
        }
    }

    pub async fn refresh_microsoft_access_token(
        &self,
        client_id: &str,
        refresh_token: &str,
    ) -> Result<MicrosoftTokenRefresh> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_microsoft_oauth_client_id: Microsoft OAuth client ID is not configured");
        }
        if refresh_token.trim().is_empty() {
            bail!("missing_microsoft_refresh_token: Microsoft refresh token is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post(microsoft_token_url())
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("scope", MICROSOFT_ONEDRIVE_SCOPE),
                    ("refresh_token", refresh_token),
                    ("grant_type", "refresh_token"),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<MicrosoftTokenResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success() {
            return Err(microsoft_oauth_error(
                &value,
                "microsoft_oauth_refresh_failed",
            ));
        }
        let access_token = value
            .access_token
            .context("microsoft_oauth_empty_response: Microsoft did not return an access token")?;
        Ok(MicrosoftTokenRefresh {
            access_token: Zeroizing::new(access_token),
            refresh_token: value.refresh_token.map(Zeroizing::new),
        })
    }

    pub async fn fetch_remote_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        validate_namespace(config)?;
        match config.backend_type {
            BackendType::HttpJson => self.fetch_http_json_metadata(config, secrets).await,
            BackendType::Dropbox => self.fetch_dropbox_metadata(config, secrets).await,
            BackendType::OneDrive => self.fetch_onedrive_metadata(config, secrets).await,
            BackendType::GithubGist => self.fetch_gist_metadata(config, secrets).await,
            BackendType::Git => self.fetch_git_metadata(config, secrets).await,
            BackendType::S3 => self.fetch_s3_metadata(config, secrets).await,
            BackendType::Webdav => self.fetch_webdav_metadata(config, secrets).await,
        }
    }

    pub async fn upload_remote_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        match config.backend_type {
            BackendType::HttpJson => {
                self.upload_http_json_snapshot(config, secrets, payload)
                    .await
            }
            BackendType::Dropbox => self.upload_dropbox_snapshot(config, secrets, payload).await,
            BackendType::OneDrive => {
                self.upload_onedrive_snapshot(config, secrets, payload)
                    .await
            }
            BackendType::GithubGist => self.upload_gist_snapshot(config, secrets, payload).await,
            BackendType::Git => self.upload_git_snapshot(config, secrets, payload).await,
            BackendType::S3 => self.upload_s3_snapshot(config, secrets, payload).await,
            BackendType::Webdav => self.upload_webdav_snapshot(config, secrets, payload).await,
        }
    }

    pub async fn write_remote_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        match config.backend_type {
            BackendType::HttpJson => {
                self.write_http_json_object(config, secrets, relative_path, bytes, content_type)
                    .await
            }
            BackendType::Dropbox => {
                self.write_dropbox_object(config, secrets, relative_path, bytes, content_type)
                    .await
            }
            BackendType::OneDrive => {
                self.write_onedrive_object(
                    config,
                    secrets,
                    relative_path,
                    bytes,
                    content_type,
                    None,
                )
                .await
            }
            BackendType::GithubGist => {
                self.write_gist_object(config, secrets, relative_path, bytes)
                    .await
            }
            BackendType::Git => {
                self.write_git_object(config, secrets, relative_path, bytes)
                    .await
            }
            BackendType::S3 => {
                self.write_s3_object(config, secrets, relative_path, bytes, content_type)
                    .await
            }
            BackendType::Webdav => {
                self.write_webdav_object(config, secrets, relative_path, bytes, content_type)
                    .await
            }
        }
    }

    pub async fn read_remote_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        match config.backend_type {
            BackendType::HttpJson => {
                self.read_http_json_object(config, secrets, relative_path)
                    .await
            }
            BackendType::Dropbox => {
                self.read_dropbox_object(config, secrets, relative_path)
                    .await
            }
            BackendType::OneDrive => {
                self.read_onedrive_object(config, secrets, relative_path)
                    .await
            }
            BackendType::GithubGist => self.read_gist_object(config, secrets, relative_path).await,
            BackendType::Git => self.read_git_object(config, secrets, relative_path).await,
            BackendType::S3 => self.read_s3_object(config, secrets, relative_path).await,
            BackendType::Webdav => {
                self.read_webdav_object(config, secrets, relative_path)
                    .await
            }
        }
    }

    pub async fn write_remote_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &serde_json::Value,
        expected_etag: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        if matches!(config.backend_type, BackendType::HttpJson) {
            return self
                .write_http_json_metadata(config, secrets, metadata)
                .await;
        }
        if matches!(config.backend_type, BackendType::GithubGist) {
            return self
                .write_gist_metadata(config, secrets, metadata, expected_etag)
                .await;
        }
        if matches!(config.backend_type, BackendType::OneDrive) {
            let paths = onedrive_paths(config);
            return self
                .write_onedrive_object(
                    config,
                    secrets,
                    &paths.metadata_path,
                    serde_json::to_vec(metadata)?,
                    Some("application/json"),
                    expected_etag,
                )
                .await;
        }
        self.write_remote_object(
            config,
            secrets,
            "latest.json",
            serde_json::to_vec(metadata)?,
            Some("application/json"),
        )
        .await
    }

    pub async fn write_gist_objects_and_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        objects: &[RemoteUploadObject],
        metadata: &Value,
        expected_etag: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        let current = self.fetch_gist_value(config, secrets).await?;
        if let Some(expected_etag) = expected_etag
            && current
                .as_ref()
                .and_then(gist_revision_from_response)
                .as_deref()
                != Some(expected_etag)
        {
            bail!("etag_conflict_detected: GitHub Gist changed before upload started");
        }

        let mut files = serde_json::Map::new();
        for object in objects {
            files.insert(
                gist_object_filename(config, &object.path),
                json!({ "content": encode_gist_object_content(&object.bytes) }),
            );
        }
        let metadata_path = gist_paths(config).metadata_path;
        files.insert(
            gist_object_filename(config, &metadata_path),
            json!({ "content": encode_gist_object_content(&serde_json::to_vec(metadata)?) }),
        );

        if let Some(gist) = current.as_ref() {
            let prefix = gist_filename_prefix(config);
            let keep = gist_keep_filenames_from_metadata(config, metadata);
            if let Some(existing_files) = gist.get("files").and_then(Value::as_object) {
                for filename in existing_files.keys() {
                    if filename.starts_with(&prefix) && !keep.contains(filename) {
                        files.insert(filename.clone(), Value::Null);
                    }
                }
            }
        }

        let response = execute_cloud_request(
            self.client
                .patch(gist_url(config)?)
                .headers(gist_headers(secrets)?)
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&json!({ "files": files }))?),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(gist_value_error(
                status,
                &value,
                "gist_write",
                "Failed to update GitHub Gist content",
            ));
        }
        Ok(RemoteWriteResult {
            revision: gist_revision_from_response(&value).unwrap_or_default(),
            etag: gist_revision_from_response(&value),
        })
    }

    pub async fn download_remote_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteSnapshotDownload> {
        let metadata = self.fetch_remote_metadata(config, secrets).await?;
        if !metadata.exists {
            bail!("remote_not_found: no remote snapshot found");
        }
        let metadata_source = match config.backend_type {
            BackendType::HttpJson => "HTTP JSON metadata",
            BackendType::Dropbox => "Dropbox metadata",
            BackendType::OneDrive => "OneDrive metadata",
            BackendType::GithubGist => "GitHub Gist metadata",
            BackendType::Git => "Git metadata",
            BackendType::S3 => "S3 metadata",
            BackendType::Webdav => "WebDAV metadata",
        };
        assert_snapshot_size(metadata.content_length.unwrap_or(0), metadata_source)?;
        let object = match config.backend_type {
            BackendType::HttpJson => {
                let url = join_url(
                    &config.endpoint,
                    &format!("v1/namespaces/{}/blob", encode_component(&config.namespace)),
                );
                let response = execute_cloud_request(
                    self.client
                        .get(url)
                        .headers(self.http_auth_headers(config, secrets)?),
                )
                .await?;
                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    bail!(
                        "http_blob_{}: Failed to download snapshot ({})",
                        status,
                        status
                    );
                }
                response_to_object(response, "HTTP JSON blob").await?
            }
            BackendType::Webdav => {
                let response = execute_cloud_request(
                    self.client
                        .get(join_url(&webdav_namespace_url(config), "latest.oxide"))
                        .headers(self.http_auth_headers(config, secrets)?),
                )
                .await?;
                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    bail!(
                        "webdav_blob_{}: Failed to download WebDAV snapshot ({})",
                        status,
                        status
                    );
                }
                response_to_object(response, "WebDAV blob").await?
            }
            BackendType::Dropbox => {
                let paths = dropbox_paths(config);
                self.download_dropbox_file(&paths.blob_path, secrets)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!("remote_not_found: no remote Dropbox snapshot found")
                    })?
            }
            BackendType::OneDrive => {
                let path = metadata
                    .blob_path
                    .as_deref()
                    .unwrap_or(&onedrive_paths(config).blob_path)
                    .to_string();
                self.read_onedrive_object(config, secrets, &path)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!("remote_not_found: no remote OneDrive snapshot found")
                    })?
            }
            BackendType::GithubGist => {
                let path = metadata
                    .blob_path
                    .as_deref()
                    .unwrap_or(&gist_paths(config).blob_path)
                    .to_string();
                self.read_gist_object(config, secrets, &path)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!("remote_not_found: no remote GitHub Gist snapshot found")
                    })?
            }
            BackendType::Git => {
                let path = metadata
                    .blob_path
                    .as_deref()
                    .unwrap_or(&git_paths(config).blob_path)
                    .to_string();
                self.fetch_git_raw_file(config, secrets, &path)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!("remote_not_found: no remote Git snapshot found")
                    })?
            }
            BackendType::S3 => {
                let path = metadata
                    .blob_key
                    .as_deref()
                    .unwrap_or(&s3_paths(config).blob_key)
                    .to_string();
                let response = self
                    .s3_request(
                        Method::GET,
                        &s3_blob_url(config, &path)?,
                        config,
                        secrets,
                        None,
                        HeaderMap::new(),
                    )
                    .await?;
                if response.status() == StatusCode::NOT_FOUND {
                    bail!("remote_not_found: no remote S3 snapshot found");
                }
                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    bail!(
                        "s3_blob_{}: Failed to download S3 snapshot ({})",
                        status,
                        status
                    );
                }
                response_to_object(response, "S3 blob").await?
            }
        };
        Ok(RemoteSnapshotDownload {
            metadata,
            bytes: object.bytes,
            response_etag: object.etag,
            last_modified: object.last_modified,
        })
    }

    async fn fetch_http_json_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        require_endpoint(config)?;
        let url = join_url(
            &config.endpoint,
            &format!(
                "v1/namespaces/{}/metadata",
                encode_component(&config.namespace)
            ),
        );
        let response = execute_cloud_request(
            self.client
                .get(url)
                .headers(self.http_auth_headers(config, secrets)?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(RemoteMetadata::missing());
        }
        if !response.status().is_success() {
            return Err(http_json_error(response, "http", "Failed to fetch remote metadata").await);
        }
        normalize_remote_metadata(response.json::<Value>().await?, None)
    }

    async fn fetch_webdav_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        require_endpoint(config)?;
        let response = execute_cloud_request(
            self.client
                .get(join_url(&webdav_namespace_url(config), "latest.json"))
                .headers(self.http_auth_headers(config, secrets)?),
        )
        .await?;
        if matches!(
            response.status(),
            StatusCode::NOT_FOUND | StatusCode::CONFLICT
        ) {
            return Ok(RemoteMetadata::missing());
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            bail!(
                "webdav_{}: Failed to fetch WebDAV metadata ({})",
                status,
                status
            );
        }
        normalize_remote_metadata(response.json::<Value>().await?, None)
    }

    async fn fetch_dropbox_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        let paths = dropbox_paths(config);
        let Some(downloaded) = self
            .download_dropbox_file(&paths.metadata_path, secrets)
            .await?
        else {
            return Ok(RemoteMetadata::missing());
        };
        let mut value = serde_json::from_slice::<Value>(&downloaded.bytes)?;
        if value.get("uploadedAt").and_then(Value::as_str).is_none()
            && let Some(last_modified) = downloaded.last_modified.as_deref()
        {
            value["uploadedAt"] = Value::String(last_modified.to_string());
        }
        normalize_remote_metadata(value, downloaded.etag)
    }

    async fn fetch_onedrive_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        let paths = onedrive_paths(config);
        let Some(object) = self
            .read_onedrive_object(config, secrets, &paths.metadata_path)
            .await?
        else {
            return Ok(RemoteMetadata::missing());
        };
        let value = serde_json::from_slice::<Value>(&object.bytes)?;
        let mut metadata = normalize_remote_metadata(value, object.etag)?;
        metadata.blob_path.get_or_insert(paths.blob_path);
        Ok(metadata)
    }

    async fn fetch_gist_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        let paths = gist_paths(config);
        let Some(object) = self
            .read_gist_object(config, secrets, &paths.metadata_path)
            .await?
        else {
            return Ok(RemoteMetadata::missing());
        };
        let value = serde_json::from_slice::<Value>(&object.bytes)?;
        let mut metadata = normalize_remote_metadata(value, object.etag)?;
        metadata.blob_path.get_or_insert(paths.blob_path);
        Ok(metadata)
    }

    async fn fetch_git_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        let paths = git_paths(config);
        let Some(file) = self
            .fetch_git_file(config, secrets, &paths.metadata_path)
            .await?
        else {
            return Ok(RemoteMetadata::missing());
        };
        let bytes = if file.encoding.as_deref() == Some("base64") {
            BASE64.decode(file.content.replace('\n', ""))?
        } else {
            file.content.into_bytes()
        };
        let value = serde_json::from_slice::<Value>(&bytes)?;
        let mut metadata = normalize_remote_metadata(value, file.sha)?;
        metadata.blob_path.get_or_insert(paths.blob_path);
        Ok(metadata)
    }

    async fn upload_http_json_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        require_endpoint(config)?;
        let url = join_url(
            &config.endpoint,
            &format!("v1/namespaces/{}/blob", encode_component(&config.namespace)),
        );
        let mut headers = self.http_auth_headers(config, secrets)?;
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(OXIDE_CONTENT_TYPE));
        insert_header(&mut headers, "X-OxideTerm-Revision", &payload.revision)?;
        insert_header(&mut headers, "X-OxideTerm-Device-Id", &payload.device_id)?;
        if let Some(section_revisions) = payload.section_revisions.as_ref() {
            insert_header(
                &mut headers,
                "X-OxideTerm-Section-Revisions",
                &serde_json::to_string(section_revisions)?,
            )?;
        }
        if let Some(previous) = payload.previous_etag.as_deref() {
            insert_header(&mut headers, "If-Match", previous)?;
        } else {
            headers.insert("If-None-Match", HeaderValue::from_static("*"));
        }
        let response =
            execute_cloud_request(self.client.put(url).headers(headers).body(payload.bytes))
                .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if status == StatusCode::PRECONDITION_FAILED {
            let message = value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("remote snapshot changed before upload completed");
            bail!("etag_conflict_detected: {message}");
        }
        if !status.is_success() || value.get("ok").and_then(Value::as_bool) == Some(false) {
            return Err(http_json_value_error(
                status,
                &value,
                "http",
                "Failed to upload snapshot",
            ));
        }
        Ok(RemoteWriteResult {
            revision: value
                .get("revision")
                .and_then(Value::as_str)
                .unwrap_or(&payload.revision)
                .to_string(),
            etag: value
                .get("etag")
                .and_then(Value::as_str)
                .or(payload.etag.as_deref())
                .map(str::to_string),
        })
    }

    async fn upload_webdav_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        self.ensure_webdav_namespace(config, secrets).await?;
        let namespace = webdav_namespace_url(config);
        let mut blob_headers = self.http_auth_headers(config, secrets)?;
        blob_headers.insert(CONTENT_TYPE, HeaderValue::from_static(OXIDE_CONTENT_TYPE));
        if let Some(previous) = payload.previous_etag.as_deref() {
            insert_header(&mut blob_headers, "If-Match", previous)?;
        }
        let blob_response = execute_cloud_request(
            self.client
                .put(join_url(&namespace, "latest.oxide"))
                .headers(blob_headers)
                .body(payload.bytes.clone()),
        )
        .await?;
        if blob_response.status() == StatusCode::PRECONDITION_FAILED {
            bail!("etag_conflict_detected: remote WebDAV snapshot changed before upload completed");
        }
        if !blob_response.status().is_success() {
            let status = blob_response.status().as_u16();
            bail!(
                "webdav_blob_{}: Failed to upload WebDAV blob ({})",
                status,
                status
            );
        }
        let mut metadata = payload.metadata_json();
        metadata["namespace"] = Value::String(config.namespace.clone());
        self.write_remote_metadata(config, secrets, &metadata, None)
            .await?;
        Ok(RemoteWriteResult {
            revision: payload.revision,
            etag: payload.etag,
        })
    }

    async fn upload_dropbox_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        self.ensure_dropbox_namespace(config, secrets).await?;
        let paths = dropbox_paths(config);
        let mut metadata = payload.metadata_json();
        metadata["namespace"] = Value::String(config.namespace.clone());
        let metadata_bytes = serde_json::to_vec(&metadata)?;
        self.upload_dropbox_file(
            &paths.blob_path,
            payload.bytes,
            secrets,
            "application/octet-stream",
        )
        .await?;
        self.upload_dropbox_file(
            &paths.metadata_path,
            metadata_bytes,
            secrets,
            "application/json",
        )
        .await?;
        Ok(RemoteWriteResult {
            revision: payload.revision,
            etag: payload.etag,
        })
    }

    async fn upload_onedrive_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        let metadata_path = onedrive_paths(config).metadata_path;
        let blob_path = onedrive_blob_path(&payload);
        let mut metadata = payload.metadata_json_with_blob_path(&blob_path);
        metadata["namespace"] = Value::String(config.namespace.clone());
        self.write_onedrive_object(
            config,
            secrets,
            &blob_path,
            payload.bytes,
            Some(OXIDE_CONTENT_TYPE),
            None,
        )
        .await?;
        let result = self
            .write_onedrive_object(
                config,
                secrets,
                &metadata_path,
                serde_json::to_vec(&metadata)?,
                Some("application/json"),
                payload.previous_etag.as_deref(),
            )
            .await?;
        self.cleanup_onedrive_objects(config, secrets, &metadata)
            .await?;
        Ok(RemoteWriteResult {
            revision: payload.revision,
            etag: result.etag.or(payload.etag),
        })
    }

    async fn upload_gist_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        let blob_path = gist_revision_blob_path(&payload.revision);
        let mut metadata = payload.metadata_json_with_blob_path(&blob_path);
        metadata["namespace"] = Value::String(gist_namespace(config));
        self.write_gist_object(config, secrets, &blob_path, payload.bytes)
            .await?;
        let metadata_result = self
            .write_gist_object(
                config,
                secrets,
                &gist_paths(config).metadata_path,
                serde_json::to_vec(&metadata)?,
            )
            .await?;
        Ok(RemoteWriteResult {
            revision: payload.revision,
            etag: metadata_result.etag.or(payload.etag),
        })
    }

    async fn upload_git_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        let blob_path = git_revision_blob_path(config, &payload.revision);
        let mut metadata = payload.metadata_json_with_blob_path(&blob_path);
        metadata["namespace"] = Value::String(config.namespace.clone());
        let metadata_bytes = serde_json::to_vec(&metadata)?;
        self.put_git_file(
            config,
            secrets,
            &blob_path,
            payload.bytes,
            None,
            &format!("[Oxide Cloud Sync] blob {}", payload.revision),
        )
        .await?;
        let response = self
            .put_git_file(
                config,
                secrets,
                &git_paths(config).metadata_path,
                metadata_bytes,
                payload.previous_etag.as_deref(),
                &format!("[Oxide Cloud Sync] snapshot {}", payload.revision),
            )
            .await?;
        Ok(RemoteWriteResult {
            revision: payload.revision,
            etag: response
                .get("content")
                .and_then(|content| content.get("sha"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or(payload.etag),
        })
    }

    async fn write_http_json_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        let url = join_url(
            &config.endpoint,
            &format!(
                "v1/namespaces/{}/objects/{}",
                encode_component(&config.namespace),
                encode_path_segments(relative_path)
            ),
        );
        let mut headers = self.http_auth_headers(config, secrets)?;
        insert_header(
            &mut headers,
            CONTENT_TYPE.as_str(),
            content_type.unwrap_or("application/octet-stream"),
        )?;
        let response =
            execute_cloud_request(self.client.put(url).headers(headers).body(bytes)).await?;
        if !response.status().is_success() {
            return Err(http_json_error(response, "http_object", "Failed to upload object").await);
        }
        Ok(response_write_result(response).await)
    }

    async fn read_http_json_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        let url = join_url(
            &config.endpoint,
            &format!(
                "v1/namespaces/{}/objects/{}",
                encode_component(&config.namespace),
                encode_path_segments(relative_path)
            ),
        );
        let response = execute_cloud_request(
            self.client
                .get(url)
                .headers(self.http_auth_headers(config, secrets)?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(
                http_json_error(response, "http_object", "Failed to download object").await,
            );
        }
        response_to_object(response, &format!("HTTP JSON object {relative_path}"))
            .await
            .map(Some)
    }

    async fn write_webdav_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        self.ensure_webdav_namespace(config, secrets).await?;
        self.ensure_webdav_object_parent(config, secrets, relative_path)
            .await?;
        let mut headers = self.http_auth_headers(config, secrets)?;
        insert_header(
            &mut headers,
            CONTENT_TYPE.as_str(),
            content_type.unwrap_or("application/octet-stream"),
        )?;
        let response = execute_cloud_request(
            self.client
                .put(webdav_object_url(config, relative_path))
                .headers(headers)
                .body(bytes),
        )
        .await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            bail!(
                "webdav_object_{}: Failed to upload WebDAV object ({})",
                status,
                status
            );
        }
        Ok(response_write_result(response).await)
    }

    async fn read_webdav_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        self.read_object_response(
            execute_cloud_request(
                self.client
                    .get(webdav_object_url(config, relative_path))
                    .headers(self.http_auth_headers(config, secrets)?),
            )
            .await?,
            "webdav_object",
            &format!("WebDAV object {relative_path}"),
        )
        .await
    }

    async fn write_dropbox_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        self.ensure_dropbox_namespace(config, secrets).await?;
        let value = self
            .upload_dropbox_file(
                &dropbox_object_path(config, relative_path),
                bytes,
                secrets,
                content_type.unwrap_or("application/octet-stream"),
            )
            .await?;
        Ok(RemoteWriteResult {
            revision: String::new(),
            etag: value.get("rev").and_then(Value::as_str).map(str::to_string),
        })
    }

    async fn read_dropbox_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        self.download_dropbox_file(&dropbox_object_path(config, relative_path), secrets)
            .await
    }

    async fn read_onedrive_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        let metadata_response = execute_cloud_request(
            self.client
                .get(onedrive_item_url(config, relative_path))
                .headers(onedrive_headers(secrets)?),
        )
        .await?;
        if metadata_response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let status = metadata_response.status();
        let metadata = metadata_response
            .json::<Value>()
            .await
            .unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(onedrive_value_error(
                status,
                &metadata,
                "onedrive_download",
                "Failed to fetch OneDrive item metadata",
            ));
        }
        let response = execute_cloud_request(
            self.client
                .get(onedrive_content_url(config, relative_path))
                .headers(onedrive_headers(secrets)?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status();
            let value = response.json::<Value>().await.unwrap_or(Value::Null);
            return Err(onedrive_value_error(
                status,
                &value,
                "onedrive_download",
                "Failed to download OneDrive content",
            ));
        }
        let mut object =
            response_to_object(response, &format!("OneDrive object {relative_path}")).await?;
        object.etag = metadata
            .get("eTag")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(object.etag);
        object.last_modified = metadata
            .get("lastModifiedDateTime")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(object.last_modified);
        object.content_type = metadata
            .get("file")
            .and_then(|file| file.get("mimeType"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(object.content_type);
        Ok(Some(object))
    }

    async fn write_onedrive_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
        content_type: Option<&str>,
        expected_etag: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        self.ensure_onedrive_parent(config, secrets, relative_path)
            .await?;
        let mut headers = onedrive_headers(secrets)?;
        insert_header(
            &mut headers,
            CONTENT_TYPE.as_str(),
            content_type.unwrap_or("application/octet-stream"),
        )?;
        if let Some(expected_etag) = expected_etag {
            insert_header(&mut headers, "If-Match", expected_etag)?;
        } else if relative_path == onedrive_paths(config).metadata_path {
            headers.insert("If-None-Match", HeaderValue::from_static("*"));
        }
        let response = execute_cloud_request(
            self.client
                .put(onedrive_content_url(config, relative_path))
                .headers(headers)
                .body(bytes),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if status == StatusCode::PRECONDITION_FAILED {
            let message = onedrive_error_message(&value)
                .unwrap_or("OneDrive object changed before upload completed");
            bail!("etag_conflict_detected: {message}");
        }
        if !status.is_success() {
            return Err(onedrive_value_error(
                status,
                &value,
                "onedrive_write",
                "Failed to upload OneDrive content",
            ));
        }
        Ok(RemoteWriteResult {
            revision: value
                .get("eTag")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            etag: value
                .get("eTag")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    async fn cleanup_onedrive_objects(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &Value,
    ) -> Result<()> {
        let response = execute_cloud_request(
            self.client
                .get(onedrive_children_url(config, "objects"))
                .headers(onedrive_headers(secrets)?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(onedrive_value_error(
                status,
                &value,
                "onedrive_cleanup",
                "Failed to list old OneDrive objects",
            ));
        }
        let keep = onedrive_keep_object_paths(metadata);
        let removals = value
            .get("value")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("name").and_then(Value::as_str))
                    .filter(|name| name.ends_with(".oxide"))
                    .map(|name| format!("objects/{name}"))
                    .filter(|path| !keep.contains(path))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for path in removals {
            let response = execute_cloud_request(
                self.client
                    .delete(onedrive_item_url(config, &path))
                    .headers(onedrive_headers(secrets)?),
            )
            .await?;
            let status = response.status();
            if status == StatusCode::NOT_FOUND {
                continue;
            }
            if !status.is_success() {
                let value = response.json::<Value>().await.unwrap_or(Value::Null);
                return Err(onedrive_value_error(
                    status,
                    &value,
                    "onedrive_cleanup",
                    "Failed to remove old OneDrive object",
                ));
            }
        }
        Ok(())
    }

    async fn write_gist_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
    ) -> Result<RemoteWriteResult> {
        let filename = gist_object_filename(config, relative_path);
        let response = execute_cloud_request(
            self.client
                .patch(gist_url(config)?)
                .headers(gist_headers(secrets)?)
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&json!({
                    "files": {
                        filename: {
                            "content": encode_gist_object_content(&bytes),
                        }
                    }
                }))?),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(gist_value_error(
                status,
                &value,
                "gist_write",
                "Failed to update GitHub Gist content",
            ));
        }
        Ok(RemoteWriteResult {
            revision: gist_revision_from_response(&value).unwrap_or_default(),
            etag: gist_revision_from_response(&value),
        })
    }

    async fn write_gist_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &Value,
        expected_etag: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        if let Some(expected_etag) = expected_etag
            && let Some(remote) = self.fetch_gist_value(config, secrets).await?
            && gist_revision_from_response(&remote).as_deref() != Some(expected_etag)
        {
            bail!("etag_conflict_detected: GitHub Gist changed before metadata upload completed");
        }
        let result = self
            .write_gist_object(
                config,
                secrets,
                &gist_paths(config).metadata_path,
                serde_json::to_vec(metadata)?,
            )
            .await?;
        self.cleanup_gist_objects(config, secrets, metadata).await?;
        Ok(result)
    }

    async fn read_gist_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        let Some(value) = self.fetch_gist_value(config, secrets).await? else {
            return Ok(None);
        };
        let filename = gist_object_filename(config, relative_path);
        let Some(file) = value
            .get("files")
            .and_then(Value::as_object)
            .and_then(|files| files.get(&filename))
        else {
            return Ok(None);
        };
        let content = match file.get("content").and_then(Value::as_str) {
            Some(content) => content.to_string(),
            None => {
                let raw_url = file
                    .get("raw_url")
                    .and_then(Value::as_str)
                    .context("gist_missing_content: GitHub Gist file content is unavailable")?;
                self.fetch_gist_raw_content(secrets, raw_url).await?
            }
        };
        let bytes = decode_gist_object_content(&content)?;
        assert_snapshot_size(
            bytes.len() as u64,
            &format!("GitHub Gist object {relative_path}"),
        )?;
        Ok(Some(RemoteObject {
            bytes,
            etag: gist_revision_from_response(&value),
            last_modified: value
                .get("updated_at")
                .and_then(Value::as_str)
                .map(str::to_string),
            content_type: file.get("type").and_then(Value::as_str).map(str::to_string),
        }))
    }

    async fn fetch_gist_value(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<Option<Value>> {
        let response = execute_cloud_request(
            self.client
                .get(gist_url(config)?)
                .headers(gist_headers(secrets)?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(gist_value_error(
                status,
                &value,
                "gist_download",
                "Failed to download GitHub Gist content",
            ));
        }
        Ok(Some(value))
    }

    async fn cleanup_gist_objects(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &Value,
    ) -> Result<()> {
        let Some(gist) = self.fetch_gist_value(config, secrets).await? else {
            return Ok(());
        };
        let prefix = gist_filename_prefix(config);
        let keep = gist_keep_filenames_from_metadata(config, metadata);
        let removals = gist
            .get("files")
            .and_then(Value::as_object)
            .map(|files| {
                files
                    .keys()
                    .filter(|filename| filename.starts_with(&prefix) && !keep.contains(*filename))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if removals.is_empty() {
            return Ok(());
        }
        let mut files = serde_json::Map::new();
        for filename in removals {
            files.insert(filename, Value::Null);
        }
        let response = execute_cloud_request(
            self.client
                .patch(gist_url(config)?)
                .headers(gist_headers(secrets)?)
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&json!({ "files": files }))?),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(gist_value_error(
                status,
                &value,
                "gist_cleanup",
                "Failed to clean old GitHub Gist objects",
            ));
        }
        Ok(())
    }

    async fn write_git_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
    ) -> Result<RemoteWriteResult> {
        let value = self
            .put_git_file(
                config,
                secrets,
                &git_object_path(config, relative_path),
                bytes,
                None,
                &format!("[Oxide Cloud Sync] object {relative_path}"),
            )
            .await?;
        Ok(RemoteWriteResult {
            revision: String::new(),
            etag: value
                .get("content")
                .and_then(|content| content.get("sha"))
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    async fn read_git_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        self.fetch_git_raw_file(config, secrets, &git_object_path(config, relative_path))
            .await
    }

    async fn write_http_json_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &Value,
    ) -> Result<RemoteWriteResult> {
        let url = join_url(
            &config.endpoint,
            &format!(
                "v1/namespaces/{}/metadata",
                encode_component(&config.namespace)
            ),
        );
        let mut headers = self.http_auth_headers(config, secrets)?;
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let response = execute_cloud_request(
            self.client
                .put(url)
                .headers(headers)
                .body(serde_json::to_vec(metadata)?),
        )
        .await?;
        if !response.status().is_success() {
            return Err(http_json_error(response, "http_meta", "Failed to write metadata").await);
        }
        Ok(response_write_result(response).await)
    }

    async fn read_object_response(
        &self,
        response: reqwest::Response,
        error_prefix: &str,
        source: &str,
    ) -> Result<Option<RemoteObject>> {
        if matches!(
            response.status(),
            StatusCode::NOT_FOUND | StatusCode::CONFLICT
        ) {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            bail!(
                "{}_{}: Failed to download WebDAV object ({})",
                error_prefix,
                status,
                status
            );
        }
        response_to_object(response, source).await.map(Some)
    }

    fn http_auth_headers(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        if backend_uses_token(&config.backend_type, &config.auth_mode) {
            if let Some(token) = secrets.token.as_ref().map(|token| token.as_str()) {
                insert_bearer_auth_header(&mut headers, token)?;
            }
        }
        if backend_uses_basic(&config.backend_type, &config.auth_mode)
            && let (Some(username), Some(password)) = (
                secrets
                    .basic_username
                    .as_ref()
                    .map(|username| username.as_str()),
                secrets
                    .basic_password
                    .as_ref()
                    .map(|password| password.as_str()),
            )
        {
            insert_basic_auth_header(&mut headers, username, password)?;
        }
        Ok(headers)
    }

    async fn ensure_webdav_namespace(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<()> {
        self.ensure_webdav_collection(&webdav_namespace_url(config), config, secrets)
            .await
    }

    async fn ensure_webdav_object_parent(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<()> {
        let Some(parent) = webdav_parent_object_path(relative_path) else {
            return Ok(());
        };
        self.ensure_webdav_collection(&webdav_object_url(config, &parent), config, secrets)
            .await
    }

    async fn ensure_webdav_collection(
        &self,
        url: &str,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<()> {
        let headers = self.http_auth_headers(config, secrets)?;
        let response = self.mkcol_webdav_collection(url, headers.clone()).await?;
        if matches!(response.status().as_u16(), 200 | 201 | 204 | 301 | 405) {
            return Ok(());
        }
        if response.status() == StatusCode::CONFLICT {
            if self
                .webdav_collection_exists(url, headers.clone())
                .await
                .unwrap_or(false)
            {
                return Ok(());
            }

            let chain = webdav_collection_chain(url);
            if chain.len() > 1 {
                for parent in chain.iter().take(chain.len() - 1) {
                    let parent_response = self
                        .mkcol_webdav_collection(parent, headers.clone())
                        .await?;
                    if matches!(
                        parent_response.status().as_u16(),
                        200 | 201 | 204 | 301 | 405
                    ) {
                        continue;
                    }
                    if parent_response.status() == StatusCode::CONFLICT
                        && self
                            .webdav_collection_exists(parent, headers.clone())
                            .await
                            .unwrap_or(false)
                    {
                        continue;
                    }
                    bail!(
                        "namespace_create_failed: Failed to prepare WebDAV namespace ({})",
                        parent_response.status().as_u16()
                    );
                }
            }

            let retry = self.mkcol_webdav_collection(url, headers.clone()).await?;
            if matches!(retry.status().as_u16(), 200 | 201 | 204 | 301 | 405)
                || (retry.status() == StatusCode::CONFLICT
                    && self
                        .webdav_collection_exists(url, headers)
                        .await
                        .unwrap_or(false))
            {
                return Ok(());
            }
            bail!(
                "namespace_create_failed: Failed to prepare WebDAV namespace ({})",
                retry.status().as_u16()
            );
        }
        bail!(
            "namespace_create_failed: Failed to prepare WebDAV namespace ({})",
            response.status().as_u16()
        )
    }

    async fn mkcol_webdav_collection(
        &self,
        url: &str,
        headers: HeaderMap,
    ) -> Result<reqwest::Response> {
        execute_cloud_request(
            self.client
                .request(Method::from_bytes(b"MKCOL")?, trim_trailing_slash(url))
                .headers(headers),
        )
        .await
    }

    async fn webdav_collection_exists(&self, url: &str, mut headers: HeaderMap) -> Result<bool> {
        insert_header(&mut headers, "Depth", "0")?;
        let response = execute_cloud_request(
            self.client
                .request(Method::from_bytes(b"PROPFIND")?, trim_trailing_slash(url))
                .headers(headers),
        )
        .await?;
        Ok(matches!(response.status().as_u16(), 200 | 207 | 301 | 405))
    }

    async fn ensure_dropbox_namespace(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<()> {
        let paths = dropbox_paths(config);
        let parts = trim_slashes(&paths.namespace_path)
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut current = String::new();
        for part in parts {
            current.push('/');
            current.push_str(&part);
            let response = execute_cloud_request(
                self.client
                    .post(format!("{DROPBOX_API_BASE}/files/create_folder_v2"))
                    .headers(dropbox_headers(secrets)?)
                    .header(CONTENT_TYPE, "application/json")
                    .body(serde_json::to_vec(
                        &json!({ "path": current, "autorename": false }),
                    )?),
            )
            .await?;
            if response.status().is_success() || response.status() == StatusCode::CONFLICT {
                continue;
            }
            let status = response.status().as_u16();
            bail!(
                "dropbox_folder_{}: Failed to prepare Dropbox namespace ({})",
                status,
                status
            );
        }
        Ok(())
    }

    async fn download_dropbox_file(
        &self,
        path: &str,
        secrets: &CloudSyncSecrets,
    ) -> Result<Option<RemoteObject>> {
        let response = execute_cloud_request(
            self.client
                .post(format!("{DROPBOX_CONTENT_BASE}/files/download"))
                .headers(dropbox_headers(secrets)?)
                .header(
                    "Dropbox-API-Arg",
                    serde_json::to_string(&json!({ "path": path }))?,
                ),
        )
        .await?;
        if response.status() == StatusCode::CONFLICT {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            bail!(
                "dropbox_download_{}: Failed to download Dropbox file ({})",
                status,
                status
            );
        }
        let dropbox_metadata = response
            .headers()
            .get("Dropbox-API-Result")
            .and_then(|header| header.to_str().ok())
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        let etag = dropbox_metadata
            .as_ref()
            .and_then(|value| value.get("rev").and_then(Value::as_str))
            .map(str::to_string);
        let last_modified = dropbox_metadata
            .as_ref()
            .and_then(|value| value.get("server_modified").and_then(Value::as_str))
            .map(str::to_string);
        let bytes = response.bytes().await?.to_vec();
        assert_snapshot_size(bytes.len() as u64, &format!("Dropbox object {path}"))?;
        Ok(Some(RemoteObject {
            bytes,
            etag,
            last_modified,
            content_type: None,
        }))
    }

    async fn upload_dropbox_file(
        &self,
        path: &str,
        bytes: Vec<u8>,
        secrets: &CloudSyncSecrets,
        content_type: &str,
    ) -> Result<Value> {
        let response = execute_cloud_request(
            self.client
                .post(format!("{DROPBOX_CONTENT_BASE}/files/upload"))
                .headers(dropbox_headers(secrets)?)
                .header(CONTENT_TYPE, content_type)
                .header(
                    "Dropbox-API-Arg",
                    serde_json::to_string(&json!({
                        "path": path,
                        "mode": "overwrite",
                        "autorename": false,
                        "mute": true,
                        "strict_conflict": false,
                    }))?,
                )
                .body(bytes),
        )
        .await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            bail!(
                "dropbox_upload_{}: Failed to upload Dropbox file ({})",
                status,
                status
            );
        }
        Ok(response.json::<Value>().await.unwrap_or(Value::Null))
    }

    async fn fetch_git_file(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<Option<GitContentFile>> {
        let response = execute_cloud_request(
            self.client
                .get(git_contents_url(config, path, true)?)
                .headers(git_headers(secrets, "application/vnd.github.object+json")?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            bail!("git_{}: Failed to fetch Git content ({})", status, status);
        }
        Ok(Some(response.json::<GitContentFile>().await?))
    }

    async fn fetch_git_raw_file(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
    ) -> Result<Option<RemoteObject>> {
        let response = execute_cloud_request(
            self.client
                .get(git_contents_url(config, path, true)?)
                .headers(git_headers(secrets, "application/vnd.github.raw+json")?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            bail!(
                "git_blob_{}: Failed to download Git content ({})",
                status,
                status
            );
        }
        response_to_object(response, "Git blob").await.map(Some)
    }

    async fn put_git_file(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        path: &str,
        bytes: Vec<u8>,
        sha: Option<&str>,
        message: &str,
    ) -> Result<Value> {
        if secrets
            .git_token
            .as_ref()
            .map(|token| token.as_str())
            .unwrap_or_default()
            .is_empty()
        {
            bail!("missing_backend_token: Git access token is not configured");
        }
        let mut body = json!({
            "message": message,
            "content": BASE64.encode(bytes),
            "branch": git_branch(config),
        });
        if let Some(sha) = sha {
            body["sha"] = Value::String(sha.to_string());
        }
        let response = execute_cloud_request(
            self.client
                .put(git_contents_url(config, path, false)?)
                .headers(git_headers(secrets, "application/vnd.github+json")?)
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&body)?),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        let response_message = value.get("message").and_then(Value::as_str);
        if matches!(status.as_u16(), 409 | 422) {
            let message = response_message.unwrap_or("Remote Git snapshot changed during upload");
            bail!("etag_conflict_detected: {message}");
        }
        if !status.is_success() {
            let status = status.as_u16();
            let message = response_message
                .map(str::to_string)
                .unwrap_or_else(|| format!("Failed to update Git content ({status})"));
            bail!("git_write_{}: {}", status, message);
        }
        Ok(value)
    }

    async fn fetch_gist_raw_content(
        &self,
        secrets: &CloudSyncSecrets,
        raw_url: &str,
    ) -> Result<String> {
        let response =
            execute_cloud_request(self.client.get(raw_url).headers(gist_headers(secrets)?)).await?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            bail!(
                "gist_raw_{}: Failed to download GitHub Gist raw content ({})",
                status,
                status
            );
        }
        Ok(response.text().await?)
    }

    async fn ensure_onedrive_parent(
        &self,
        _config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<()> {
        let trimmed_path = trim_slashes(relative_path);
        let parts = trimmed_path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() <= 1 {
            return Ok(());
        }
        let mut parent = Vec::<String>::new();
        for segment in parts.iter().take(parts.len() - 1) {
            let children_url = if parent.is_empty() {
                format!("{MICROSOFT_GRAPH_BASE}/me/drive/special/approot/children")
            } else {
                format!(
                    "{MICROSOFT_GRAPH_BASE}/me/drive/special/approot:/{}:/children",
                    encode_path_segments(&parent.join("/"))
                )
            };
            let response = execute_cloud_request(
                self.client
                    .post(children_url)
                    .headers(onedrive_headers(secrets)?)
                    .header(CONTENT_TYPE, "application/json")
                    .body(serde_json::to_vec(&json!({
                        "name": segment,
                        "folder": {},
                        "@microsoft.graph.conflictBehavior": "fail",
                    }))?),
            )
            .await?;
            let status = response.status();
            let value = response.json::<Value>().await.unwrap_or(Value::Null);
            if !status.is_success()
                && !(status == StatusCode::CONFLICT
                    && onedrive_error_code(&value).as_deref() == Some("nameAlreadyExists"))
            {
                return Err(onedrive_value_error(
                    status,
                    &value,
                    "onedrive_folder",
                    "Failed to create OneDrive folder",
                ));
            }
            parent.push((*segment).to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMetadata {
    pub exists: bool,
    pub format: Option<String>,
    pub revision: Option<String>,
    pub etag: Option<String>,
    pub content_hash: Option<String>,
    pub uploaded_at: Option<String>,
    pub device_id: Option<String>,
    pub content_length: Option<u64>,
    pub section_revisions: Option<StructuredSectionRevisions>,
    pub scope: Option<crate::SyncScope>,
    pub sections: Option<Value>,
    pub content_type: Option<String>,
    pub blob_path: Option<String>,
    pub blob_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RemoteUploadObject {
    pub path: String,
    pub bytes: Vec<u8>,
}

impl RemoteMetadata {
    pub fn missing() -> Self {
        Self {
            exists: false,
            content_length: Some(0),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct RemoteSnapshotUpload {
    pub revision: String,
    pub device_id: String,
    pub uploaded_at: String,
    pub bytes: Vec<u8>,
    pub etag: Option<String>,
    pub previous_etag: Option<String>,
    pub section_revisions: Option<StructuredSectionRevisions>,
}

impl RemoteSnapshotUpload {
    fn metadata_json(&self) -> Value {
        json!({
            "revision": self.revision,
            "etag": self.etag,
            "deviceId": self.device_id,
            "uploadedAt": self.uploaded_at,
            "contentLength": self.bytes.len(),
            "sectionRevisions": self.section_revisions,
            "contentType": OXIDE_CONTENT_TYPE,
            "encryption": { "scheme": "oxide-v1" },
        })
    }

    fn metadata_json_with_blob_path(&self, blob_path: &str) -> Value {
        let mut metadata = self.metadata_json();
        metadata["blobPath"] = Value::String(blob_path.to_string());
        metadata
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RemoteWriteResult {
    pub revision: String,
    pub etag: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RemoteObject {
    pub bytes: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RemoteSnapshotDownload {
    pub metadata: RemoteMetadata,
    pub bytes: Vec<u8>,
    pub response_etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct GithubDeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Clone, Eq, PartialEq)]
pub struct MicrosoftDeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

impl std::fmt::Debug for MicrosoftDeviceCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MicrosoftDeviceCode")
            .field("device_code", &"[redacted device code]")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

impl std::fmt::Debug for GithubDeviceCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GithubDeviceCode")
            .field("device_code", &"[redacted device code]")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub enum GithubDeviceTokenPoll {
    Pending { interval: u64 },
    SlowDown { interval: u64 },
    Token { access_token: Zeroizing<String> },
}

impl std::fmt::Debug for GithubDeviceTokenPoll {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending { interval } => formatter
                .debug_struct("Pending")
                .field("interval", interval)
                .finish(),
            Self::SlowDown { interval } => formatter
                .debug_struct("SlowDown")
                .field("interval", interval)
                .finish(),
            Self::Token { .. } => formatter
                .debug_struct("Token")
                .field("access_token", &"[redacted token]")
                .finish(),
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum MicrosoftDeviceTokenPoll {
    Pending {
        interval: u64,
    },
    SlowDown {
        interval: u64,
    },
    Token {
        access_token: Zeroizing<String>,
        refresh_token: Zeroizing<String>,
    },
}

impl std::fmt::Debug for MicrosoftDeviceTokenPoll {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending { interval } => formatter
                .debug_struct("Pending")
                .field("interval", interval)
                .finish(),
            Self::SlowDown { interval } => formatter
                .debug_struct("SlowDown")
                .field("interval", interval)
                .finish(),
            Self::Token { .. } => formatter
                .debug_struct("Token")
                .field("access_token", &"[redacted token]")
                .field("refresh_token", &"[redacted token]")
                .finish(),
        }
    }
}

#[derive(Eq, PartialEq)]
pub struct MicrosoftTokenRefresh {
    pub access_token: Zeroizing<String>,
    pub refresh_token: Option<Zeroizing<String>>,
}

impl std::fmt::Debug for MicrosoftTokenRefresh {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MicrosoftTokenRefresh")
            .field("access_token", &"[redacted token]")
            .field(
                "refresh_token",
                &self
                    .refresh_token
                    .as_ref()
                    .map(|_| "[redacted token]")
                    .unwrap_or("None"),
            )
            .finish()
    }
}

#[derive(Debug, Deserialize)]
struct GithubDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_github_device_interval")]
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct GithubDeviceTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_microsoft_device_interval")]
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitContentFile {
    sha: Option<String>,
    encoding: Option<String>,
    #[serde(default)]
    content: String,
}

fn validate_namespace(config: &CloudSyncSettings) -> Result<()> {
    if config.namespace.trim().is_empty()
        && !matches!(
            config.backend_type,
            BackendType::S3 | BackendType::Git | BackendType::GithubGist
        )
    {
        bail!("missing_namespace: cloud sync namespace is not configured");
    }
    Ok(())
}

fn require_endpoint(config: &CloudSyncSettings) -> Result<()> {
    if config.endpoint.trim().is_empty() {
        bail!("missing_endpoint: cloud sync endpoint is not configured");
    }
    Ok(())
}

fn normalize_remote_metadata(value: Value, etag: Option<String>) -> Result<RemoteMetadata> {
    let section_revisions = value
        .get("sectionRevisions")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;
    Ok(RemoteMetadata {
        exists: value.get("exists").and_then(Value::as_bool).unwrap_or(true),
        format: value
            .get("format")
            .and_then(Value::as_str)
            .map(str::to_string),
        revision: value
            .get("revision")
            .and_then(Value::as_str)
            .map(str::to_string),
        etag: etag.or_else(|| {
            value
                .get("etag")
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
        content_hash: value
            .get("etag")
            .and_then(Value::as_str)
            .map(str::to_string),
        uploaded_at: value
            .get("uploadedAt")
            .and_then(Value::as_str)
            .map(str::to_string),
        device_id: value
            .get("deviceId")
            .and_then(Value::as_str)
            .map(str::to_string),
        content_length: value.get("contentLength").and_then(Value::as_u64),
        section_revisions,
        scope: value
            .get("scope")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        sections: value.get("sections").cloned(),
        content_type: value
            .get("contentType")
            .and_then(Value::as_str)
            .map(str::to_string),
        blob_path: value
            .get("blobPath")
            .and_then(Value::as_str)
            .map(str::to_string),
        blob_key: value
            .get("blobKey")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

async fn response_to_object(response: reqwest::Response, source: &str) -> Result<RemoteObject> {
    if let Some(content_length) = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        assert_snapshot_size(content_length, source)?;
    }
    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let last_modified = response
        .headers()
        .get("Last-Modified")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = response.bytes().await?.to_vec();
    assert_snapshot_size(bytes.len() as u64, source)?;
    Ok(RemoteObject {
        bytes,
        etag,
        last_modified,
        content_type,
    })
}

async fn http_json_error(
    response: reqwest::Response,
    code_prefix: &str,
    fallback: &str,
) -> anyhow::Error {
    let status = response.status();
    let value = response.json::<Value>().await.unwrap_or(Value::Null);
    http_json_value_error(status, &value, code_prefix, fallback)
}

fn http_json_value_error(
    status: StatusCode,
    value: &Value,
    code_prefix: &str,
    fallback: &str,
) -> anyhow::Error {
    let status_code = status.as_u16();
    let fallback_code = format!("{code_prefix}_{status_code}");
    let fallback_message = format!("{fallback} ({status_code})");
    let code = value
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or(&fallback_code);
    let message = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(&fallback_message);
    anyhow::anyhow!("{code}: {message}")
}

pub(super) async fn execute_cloud_request(request: RequestBuilder) -> Result<Response> {
    let mut request = request;
    for attempt in 0..CLOUD_REQUEST_MAX_RETRY_ATTEMPTS {
        let retry_request = request.try_clone();
        let response = request.send().await.map_err(normalize_network_error)?;
        if !cloud_response_should_retry(response.status()) {
            return Ok(response);
        }
        let Some(next_request) = retry_request else {
            return Ok(response);
        };
        if attempt + 1 >= CLOUD_REQUEST_MAX_RETRY_ATTEMPTS {
            return Ok(response);
        }
        let delay = cloud_retry_delay(&response, attempt);
        tokio::time::sleep(delay).await;
        request = next_request;
    }
    unreachable!("cloud request retry loop always returns");
}

fn normalize_network_error(error: reqwest::Error) -> anyhow::Error {
    // Remote endpoints can be user-provided and may contain credential-bearing
    // query strings; strip URLs before surfacing reqwest transport errors.
    let error = error.without_url();
    if error.is_connect() || error.is_timeout() || error.is_request() {
        anyhow::anyhow!("network_request_failed: {}", error)
    } else {
        anyhow::Error::new(error)
    }
}

fn cloud_response_should_retry(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn cloud_retry_delay(response: &Response, attempt: usize) -> Duration {
    response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_retry_after)
        .unwrap_or_else(|| Duration::from_secs(1 << attempt))
        .min(CLOUD_REQUEST_MAX_RETRY_AFTER)
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let retry_at = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let now = Utc::now();
    if retry_at <= now {
        return Some(Duration::ZERO);
    }
    retry_at.signed_duration_since(now).to_std().ok()
}

async fn response_write_result(response: reqwest::Response) -> RemoteWriteResult {
    RemoteWriteResult {
        revision: String::new(),
        etag: response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
    }
}

fn assert_snapshot_size(size: u64, source: &str) -> Result<()> {
    if size > crate::MAX_REMOTE_SNAPSHOT_BYTES as u64 {
        bail!(
            "snapshot_too_large: remote snapshot from {source} is too large ({size} bytes, max {} bytes)",
            crate::MAX_REMOTE_SNAPSHOT_BYTES
        );
    }
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn insert_header(headers: &mut HeaderMap, name: &str, value: &str) -> Result<()> {
    headers.insert(
        HeaderName::from_bytes(name.as_bytes())?,
        HeaderValue::from_str(value)?,
    );
    Ok(())
}

fn dropbox_headers(secrets: &CloudSyncSecrets) -> Result<HeaderMap> {
    let token = secrets
        .token
        .as_ref()
        .map(|token| token.as_str())
        .filter(|token| !token.is_empty())
        .context("missing_backend_token: Dropbox access token is not configured")?;
    let mut headers = HeaderMap::new();
    insert_bearer_auth_header(&mut headers, token)?;
    Ok(headers)
}

fn digest_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn git_headers(secrets: &CloudSyncSecrets, accept: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    insert_header(&mut headers, ACCEPT.as_str(), accept)?;
    if let Some(token) = secrets
        .git_token
        .as_ref()
        .map(|token| token.as_str())
        .filter(|token| !token.is_empty())
    {
        insert_bearer_auth_header(&mut headers, token)?;
    }
    Ok(headers)
}

fn insert_bearer_auth_header(headers: &mut HeaderMap, token: &str) -> Result<()> {
    // Authorization headers must be copied into reqwest's HeaderMap, but the
    // formatted bearer string should not remain in a plain temporary String.
    let value = Zeroizing::new(format!("Bearer {token}"));
    insert_header(headers, AUTHORIZATION.as_str(), value.as_str())
}

fn insert_basic_auth_header(headers: &mut HeaderMap, username: &str, password: &str) -> Result<()> {
    // Basic auth material briefly combines username and password before base64
    // encoding; keep both staging strings zeroized after HeaderMap takes a copy.
    let credentials = Zeroizing::new(format!("{username}:{password}"));
    let encoded = Zeroizing::new(BASE64.encode(credentials.as_bytes()));
    let value = Zeroizing::new(format!("Basic {}", encoded.as_str()));
    insert_header(headers, AUTHORIZATION.as_str(), value.as_str())
}

fn trim_trailing_slash(value: &str) -> String {
    value.trim_end_matches('/').to_string()
}

fn trim_slashes(value: &str) -> String {
    value.trim_matches('/').to_string()
}

fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        trim_trailing_slash(base),
        path.trim_start_matches('/')
    )
}

fn encode_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn encode_path_segments(path: &str) -> String {
    trim_slashes(path)
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(encode_component)
        .collect::<Vec<_>>()
        .join("/")
}

fn webdav_namespace_url(config: &CloudSyncSettings) -> String {
    let endpoint = trim_trailing_slash(&config.endpoint);
    let namespace = encode_path_segments(&config.namespace);
    if namespace.is_empty() {
        endpoint
    } else if webdav_endpoint_already_scoped(&endpoint, &config.namespace) {
        endpoint
    } else {
        join_url(&endpoint, &namespace)
    }
}

fn webdav_endpoint_already_scoped(endpoint: &str, namespace: &str) -> bool {
    let Ok(url) = Url::parse(endpoint) else {
        return false;
    };
    if url.host_str() != Some("dav.jianguoyun.com") {
        return false;
    }
    let endpoint_path = trim_slashes(&percent_decode_lossy(url.path())).to_ascii_lowercase();
    let namespace_path = trim_slashes(namespace).to_ascii_lowercase();
    !namespace_path.is_empty()
        && (endpoint_path == namespace_path
            || endpoint_path.ends_with(&format!("/{namespace_path}")))
}

fn percent_decode_lossy(value: &str) -> String {
    let bytes = value.as_bytes();
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
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn webdav_object_url(config: &CloudSyncSettings, relative_path: &str) -> String {
    join_url(
        &webdav_namespace_url(config),
        &encode_path_segments(relative_path),
    )
}

fn webdav_parent_object_path(relative_path: &str) -> Option<String> {
    let mut segments = trim_slashes(relative_path)
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if segments.len() <= 1 {
        return None;
    }
    segments.pop();
    Some(segments.join("/"))
}

fn webdav_collection_chain(url: &str) -> Vec<String> {
    let Ok(mut parsed) = Url::parse(url) else {
        return vec![trim_trailing_slash(url)];
    };
    let path_segments = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if path_segments.is_empty() {
        return vec![trim_trailing_slash(url)];
    }

    let mut urls = Vec::with_capacity(path_segments.len());
    for index in 0..path_segments.len() {
        parsed.set_path(&path_segments[..=index].join("/"));
        urls.push(trim_trailing_slash(parsed.as_str()));
    }
    urls
}

struct DropboxPaths {
    namespace_path: String,
    metadata_path: String,
    blob_path: String,
}

fn dropbox_paths(config: &CloudSyncSettings) -> DropboxPaths {
    let prefix = trim_slashes(if config.namespace.is_empty() {
        "default"
    } else {
        &config.namespace
    });
    let parts = prefix
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    DropboxPaths {
        namespace_path: if parts.is_empty() {
            String::new()
        } else {
            format!("/{}", parts.join("/"))
        },
        metadata_path: format!(
            "/{}",
            [parts.clone(), vec!["latest.json"]].concat().join("/")
        ),
        blob_path: format!("/{}", [parts, vec!["latest.oxide"]].concat().join("/")),
    }
}

fn dropbox_object_path(config: &CloudSyncSettings, relative_path: &str) -> String {
    let prefix = trim_slashes(if config.namespace.is_empty() {
        "default"
    } else {
        &config.namespace
    });
    let mut parts = prefix
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    parts.extend(
        trim_slashes(relative_path)
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string),
    );
    format!("/{}", parts.join("/"))
}

struct OneDrivePaths {
    metadata_path: String,
    blob_path: String,
}

fn onedrive_paths(_config: &CloudSyncSettings) -> OneDrivePaths {
    OneDrivePaths {
        metadata_path: "metadata.json".to_string(),
        blob_path: "objects/latest.oxide".to_string(),
    }
}

fn onedrive_blob_path(payload: &RemoteSnapshotUpload) -> String {
    let stable_name = payload
        .etag
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&payload.revision);
    format!("objects/{}.oxide", sanitize_remote_object_name(stable_name))
}

fn sanitize_remote_object_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "snapshot".to_string()
    } else {
        sanitized
    }
}

fn onedrive_item_url(_config: &CloudSyncSettings, relative_path: &str) -> String {
    format!(
        "{MICROSOFT_GRAPH_BASE}/me/drive/special/approot:/{}",
        encode_path_segments(&trim_slashes(relative_path))
    )
}

fn onedrive_content_url(config: &CloudSyncSettings, relative_path: &str) -> String {
    format!("{}:/content", onedrive_item_url(config, relative_path))
}

fn onedrive_children_url(config: &CloudSyncSettings, relative_path: &str) -> String {
    format!("{}:/children", onedrive_item_url(config, relative_path))
}

fn onedrive_keep_object_paths(metadata: &Value) -> BTreeSet<String> {
    let mut keep = BTreeSet::new();
    if let Some(blob_path) = metadata.get("blobPath").and_then(Value::as_str)
        && trim_slashes(blob_path).starts_with("objects/")
    {
        keep.insert(trim_slashes(blob_path));
    }
    keep.insert("objects/latest.oxide".to_string());
    keep
}

struct GistPaths {
    metadata_path: String,
    blob_path: String,
}

fn gist_paths(_config: &CloudSyncSettings) -> GistPaths {
    GistPaths {
        metadata_path: "latest.json".to_string(),
        blob_path: "latest.oxide".to_string(),
    }
}

fn gist_revision_blob_path(revision: &str) -> String {
    format!("blobs/{revision}.oxide")
}

fn gist_namespace(config: &CloudSyncSettings) -> String {
    let namespace = trim_slashes(&config.namespace);
    if namespace.is_empty() {
        "default".to_string()
    } else {
        namespace
    }
}

fn gist_object_filename(config: &CloudSyncSettings, relative_path: &str) -> String {
    let prefix = gist_filename_prefix(config);
    let path = trim_slashes(relative_path);
    let readable = gist_safe_filename_component(
        path.rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty())
            .unwrap_or("object"),
    );
    format!(
        "{}-{}-{}.b64",
        prefix,
        readable,
        digest_hex(path.as_bytes())
    )
}

fn gist_filename_prefix(config: &CloudSyncSettings) -> String {
    let namespace = gist_safe_filename_component(&gist_namespace(config));
    format!("oxideterm-{namespace}")
}

fn gist_safe_filename_component(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            output.push(ch);
        } else {
            output.push('-');
        }
    }
    let trimmed = output.trim_matches('-');
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

fn encode_gist_object_content(bytes: &[u8]) -> String {
    format!("{GIST_OBJECT_PREFIX}{}", BASE64.encode(bytes))
}

fn decode_gist_object_content(content: &str) -> Result<Vec<u8>> {
    if let Some(encoded) = content.strip_prefix(GIST_OBJECT_PREFIX) {
        return Ok(BASE64.decode(encoded.split_whitespace().collect::<String>())?);
    }
    let trimmed = content.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Ok(content.as_bytes().to_vec());
    }
    bail!("gist_invalid_encoding: GitHub Gist object is not an OxideTerm encoded file")
}

fn parse_gist_id(config: &CloudSyncSettings) -> Result<String> {
    let input = config
        .git_repository
        .trim()
        .strip_prefix("gist:")
        .unwrap_or_else(|| config.git_repository.trim())
        .trim()
        .to_string();
    if input.is_empty() {
        bail!("missing_gist_id: GitHub Gist ID is not configured");
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        let url = Url::parse(&input).context("missing_gist_id: invalid GitHub Gist URL")?;
        let path = trim_slashes(url.path());
        let id = path
            .split('/')
            .filter(|part| !part.is_empty())
            .next_back()
            .unwrap_or_default()
            .trim_end_matches(".git")
            .to_string();
        if id.is_empty() {
            bail!("missing_gist_id: GitHub Gist URL must include a Gist ID");
        }
        return Ok(id);
    }
    let id = input.trim_end_matches(".git");
    if id.contains('/') {
        bail!("missing_gist_id: GitHub Gist ID must not contain path separators");
    }
    Ok(id.to_string())
}

fn gist_url(config: &CloudSyncSettings) -> Result<String> {
    Ok(format!(
        "{DEFAULT_GIT_API_ENDPOINT}/gists/{}",
        encode_component(&parse_gist_id(config)?)
    ))
}

fn gist_headers(secrets: &CloudSyncSecrets) -> Result<HeaderMap> {
    let token = secrets
        .git_token
        .as_ref()
        .map(|token| token.as_str())
        .filter(|token| !token.is_empty())
        .context("missing_backend_token: GitHub Gist access token is not configured")?;
    let mut headers = HeaderMap::new();
    insert_header(&mut headers, ACCEPT.as_str(), "application/vnd.github+json")?;
    insert_header(&mut headers, "X-GitHub-Api-Version", GITHUB_API_VERSION)?;
    headers.insert(USER_AGENT, HeaderValue::from_static("OxideTerm"));
    insert_bearer_auth_header(&mut headers, token)?;
    Ok(headers)
}

fn onedrive_headers(secrets: &CloudSyncSecrets) -> Result<HeaderMap> {
    let token = secrets
        .token
        .as_ref()
        .map(|token| token.as_str())
        .filter(|token| !token.is_empty())
        .context("missing_backend_token: Microsoft Graph access token is not configured")?;
    let mut headers = HeaderMap::new();
    insert_header(&mut headers, ACCEPT.as_str(), "application/json")?;
    headers.insert(USER_AGENT, HeaderValue::from_static("OxideTerm"));
    insert_bearer_auth_header(&mut headers, token)?;
    Ok(headers)
}

fn microsoft_device_code_url() -> String {
    format!("https://login.microsoftonline.com/{MICROSOFT_AUTH_TENANT}/oauth2/v2.0/devicecode")
}

fn microsoft_token_url() -> String {
    format!("https://login.microsoftonline.com/{MICROSOFT_AUTH_TENANT}/oauth2/v2.0/token")
}

fn gist_revision_from_response(value: &Value) -> Option<String> {
    value
        .get("history")
        .and_then(Value::as_array)
        .and_then(|history| history.first())
        .and_then(|entry| entry.get("version"))
        .and_then(Value::as_str)
        .or_else(|| value.get("updated_at").and_then(Value::as_str))
        .map(str::to_string)
}

fn gist_keep_filenames_from_metadata(
    config: &CloudSyncSettings,
    metadata: &Value,
) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    paths.insert(gist_paths(config).metadata_path);
    if let Some(blob_path) = metadata.get("blobPath").and_then(Value::as_str) {
        paths.insert(blob_path.to_string());
    }
    collect_gist_object_paths(metadata, &mut paths);
    paths
        .into_iter()
        .map(|path| gist_object_filename(config, &path))
        .collect()
}

fn collect_gist_object_paths(value: &Value, paths: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(path) = object.get("path").and_then(Value::as_str) {
                paths.insert(path.to_string());
            }
            for value in object.values() {
                collect_gist_object_paths(value, paths);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_gist_object_paths(value, paths);
            }
        }
        _ => {}
    }
}

fn gist_value_error(
    status: StatusCode,
    value: &Value,
    code_prefix: &str,
    fallback: &str,
) -> anyhow::Error {
    let status_code = status.as_u16();
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or(fallback);
    let code = match status {
        StatusCode::UNAUTHORIZED => "github_gist_bad_credentials".to_string(),
        StatusCode::FORBIDDEN => {
            if github_rate_limit_exceeded(message) {
                "github_gist_rate_limited".to_string()
            } else {
                "github_gist_missing_scope".to_string()
            }
        }
        StatusCode::NOT_FOUND => "missing_gist_id".to_string(),
        StatusCode::TOO_MANY_REQUESTS => "github_gist_rate_limited".to_string(),
        _ => format!("{code_prefix}_{status_code}"),
    };
    anyhow::anyhow!("{code}: {message}")
}

fn onedrive_value_error(
    status: StatusCode,
    value: &Value,
    code_prefix: &str,
    fallback: &str,
) -> anyhow::Error {
    if status == StatusCode::PRECONDITION_FAILED {
        let message = onedrive_error_message(value).unwrap_or("OneDrive object changed");
        return anyhow::anyhow!("etag_conflict_detected: {message}");
    }
    let status_code = status.as_u16();
    let message = onedrive_error_message(value).unwrap_or(fallback);
    let graph_code = onedrive_error_code(value).unwrap_or_default();
    let code = match status {
        StatusCode::BAD_REQUEST => "onedrive_bad_request".to_string(),
        StatusCode::UNAUTHORIZED => "onedrive_bad_credentials".to_string(),
        StatusCode::FORBIDDEN => {
            if onedrive_scope_or_permission_error(&graph_code, message) {
                "onedrive_missing_scope".to_string()
            } else {
                "onedrive_access_denied".to_string()
            }
        }
        StatusCode::TOO_MANY_REQUESTS => "onedrive_rate_limited".to_string(),
        StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
            "onedrive_service_unavailable".to_string()
        }
        status if status.as_u16() == 423 => "onedrive_locked".to_string(),
        status if status.as_u16() == 507 => "onedrive_quota_exceeded".to_string(),
        _ => format!("{code_prefix}_{status_code}"),
    };
    anyhow::anyhow!("{code}: {message}")
}

fn onedrive_error_message(value: &Value) -> Option<&str> {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
}

fn onedrive_error_code(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn onedrive_scope_or_permission_error(graph_code: &str, message: &str) -> bool {
    // Graph does not always use a stable status/code pair for app-folder
    // permission failures, so classify by both machine code and safe text.
    let graph_code = graph_code.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    graph_code.contains("invalidscope")
        || message.contains("files.readwrite.appfolder")
        || message.contains("insufficient privileges")
        || message.contains("permission")
        || message.contains("scope")
}

fn microsoft_oauth_value_error(value: &Value, fallback_code: &str) -> anyhow::Error {
    // Device authorization failures share the token endpoint error shape,
    // but are received before any token exists, so normalize them here.
    let response = MicrosoftTokenResponse {
        access_token: None,
        refresh_token: None,
        error: value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string),
        error_description: value
            .get("error_description")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
            })
            .map(str::to_string),
        interval: None,
    };
    microsoft_oauth_error(&response, fallback_code)
}

fn microsoft_oauth_error(value: &MicrosoftTokenResponse, fallback_code: &str) -> anyhow::Error {
    let code = match value.error.as_deref() {
        Some("authorization_declined") | Some("access_denied") => "microsoft_oauth_denied",
        Some("expired_token") => "microsoft_oauth_expired",
        Some("bad_verification_code") => "microsoft_oauth_bad_code",
        Some("invalid_client") | Some("unauthorized_client") => "microsoft_oauth_bad_client",
        Some("invalid_grant") => "microsoft_oauth_refresh_failed",
        Some("invalid_scope") => "microsoft_oauth_missing_scope",
        Some("consent_required") | Some("interaction_required") => {
            "microsoft_oauth_consent_required"
        }
        Some("invalid_request") => "microsoft_oauth_invalid_request",
        _ => fallback_code,
    };
    let message = value
        .error_description
        .as_deref()
        .unwrap_or("Microsoft OAuth failed");
    anyhow::anyhow!("{code}: {message}")
}

fn github_rate_limit_exceeded(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("rate limit") || lower.contains("secondary rate limit")
}

fn default_github_device_interval() -> u64 {
    5
}

fn default_microsoft_device_interval() -> u64 {
    5
}

struct GitPaths {
    metadata_path: String,
    blob_path: String,
}

fn git_paths(config: &CloudSyncSettings) -> GitPaths {
    let prefix = trim_slashes(&config.namespace);
    GitPaths {
        metadata_path: [prefix.as_str(), "latest.json"]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("/"),
        blob_path: [prefix.as_str(), "latest.oxide"]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("/"),
    }
}

fn git_revision_blob_path(config: &CloudSyncSettings, revision: &str) -> String {
    let prefix = trim_slashes(&config.namespace);
    [prefix.as_str(), "blobs", &format!("{revision}.oxide")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn git_object_path(config: &CloudSyncSettings, relative_path: &str) -> String {
    let prefix = trim_slashes(&config.namespace);
    [prefix.as_str(), trim_slashes(relative_path).as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn parse_git_repository(config: &CloudSyncSettings) -> Result<(String, String)> {
    let input = config
        .git_repository
        .trim()
        .trim_end_matches(".git")
        .to_string();
    if input.is_empty() {
        bail!("missing_git_repository: Git repository is not configured");
    }

    let normalized = input
        .strip_prefix("github:")
        .unwrap_or(input.as_str())
        .to_string();
    let maybe_url = if normalized.starts_with("http://") || normalized.starts_with("https://") {
        Some(normalized.as_str())
    } else if normalized
        .split('/')
        .next()
        .is_some_and(|host| host.contains('.'))
    {
        None
    } else {
        None
    };
    if let Some(url) = maybe_url {
        let url = Url::parse(url).context("missing_git_repository: invalid Git repository URL")?;
        let path = trim_slashes(url.path());
        let parts = path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() < 2 {
            bail!("missing_git_repository: Git repository must include owner and name");
        }
        if parts.len() > 2 {
            bail!("missing_git_repository: Git repository URL must point to the repository root");
        }
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    let normalized = if normalized
        .split('/')
        .next()
        .is_some_and(|host| host.contains('.'))
    {
        format!("https://{normalized}")
    } else {
        normalized
    };
    if normalized.starts_with("http://") || normalized.starts_with("https://") {
        let url = Url::parse(&normalized)
            .context("missing_git_repository: invalid Git repository URL")?;
        let path = trim_slashes(url.path());
        let parts = path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() < 2 {
            bail!("missing_git_repository: Git repository must include owner and name");
        }
        if parts.len() > 2 {
            bail!("missing_git_repository: Git repository URL must point to the repository root");
        }
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    let parts = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        bail!("missing_git_repository: Git repository must be in owner/repo format");
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn git_branch(config: &CloudSyncSettings) -> String {
    if config.git_branch.trim().is_empty() {
        "main".to_string()
    } else {
        config.git_branch.trim().to_string()
    }
}

fn git_contents_url(config: &CloudSyncSettings, path: &str, include_ref: bool) -> Result<String> {
    let (owner, repo) = parse_git_repository(config)?;
    let endpoint = if config.endpoint.trim().is_empty() {
        DEFAULT_GIT_API_ENDPOINT.to_string()
    } else {
        trim_trailing_slash(&config.endpoint)
    };
    let mut url = format!(
        "{endpoint}/repos/{}/{}/contents/{}",
        encode_component(&owner),
        encode_component(&repo),
        encode_path_segments(path)
    );
    if include_ref {
        url.push_str("?ref=");
        url.push_str(&encode_component(&git_branch(config)));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_settings(repository: &str) -> CloudSyncSettings {
        CloudSyncSettings {
            backend_type: BackendType::Git,
            git_repository: repository.to_string(),
            ..CloudSyncSettings::default()
        }
    }

    fn gist_settings(gist_id: &str) -> CloudSyncSettings {
        CloudSyncSettings {
            backend_type: BackendType::GithubGist,
            git_repository: gist_id.to_string(),
            namespace: "team/default".to_string(),
            ..CloudSyncSettings::default()
        }
    }

    #[test]
    fn parses_git_repository_inputs_like_cloud_sync_plugin() {
        for input in [
            "owner/repo",
            "owner/repo.git",
            "github:owner/repo",
            "github.com/owner/repo",
            "https://github.com/owner/repo",
            "https://github.com/owner/repo.git",
        ] {
            assert_eq!(
                parse_git_repository(&git_settings(input)).unwrap(),
                ("owner".to_string(), "repo".to_string()),
                "{input}"
            );
        }
    }

    #[test]
    fn rejects_git_repository_paths_below_repo_root() {
        let error = parse_git_repository(&git_settings("https://github.com/owner/repo/tree/main"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("repository root"));
    }

    #[test]
    fn parses_gist_id_inputs() {
        for input in [
            "abcdef123456",
            "gist:abcdef123456",
            "https://gist.github.com/owner/abcdef123456",
            "https://gist.github.com/abcdef123456",
        ] {
            assert_eq!(
                parse_gist_id(&gist_settings(input)).unwrap(),
                "abcdef123456"
            );
        }
    }

    #[test]
    fn gist_filenames_are_namespace_scoped_and_path_stable() {
        let first = gist_settings("abcdef123456");
        let second = CloudSyncSettings {
            namespace: "other".to_string(),
            ..first.clone()
        };

        assert_eq!(
            gist_object_filename(&first, "latest.json"),
            gist_object_filename(&first, "latest.json")
        );
        assert_ne!(
            gist_object_filename(&first, "latest.json"),
            gist_object_filename(&second, "latest.json")
        );
    }

    #[test]
    fn gist_content_roundtrips_binary_bytes() {
        let bytes = vec![0, 1, 2, b'O', b'X', b'I', b'D', b'E', 255];
        let encoded = encode_gist_object_content(&bytes);

        assert_eq!(decode_gist_object_content(&encoded).unwrap(), bytes);
    }

    #[test]
    fn gist_cleanup_keeps_current_manifest_objects_by_real_filename() {
        let settings = gist_settings("abcdef123456");
        let manifest = json!({
            "blobPath": "blobs/rev-1.oxide",
            "sections": {
                "connections": {
                    "path": "objects/connections/rev-1.json"
                }
            }
        });
        let keep = gist_keep_filenames_from_metadata(&settings, &manifest);

        assert!(keep.contains(&gist_object_filename(&settings, "latest.json")));
        assert!(keep.contains(&gist_object_filename(&settings, "blobs/rev-1.oxide")));
        assert!(keep.contains(&gist_object_filename(
            &settings,
            "objects/connections/rev-1.json"
        )));
        assert!(!keep.contains(&gist_object_filename(&settings, "blobs/rev-old.oxide")));
    }

    #[test]
    fn gist_error_mapping_distinguishes_scope_and_rate_limit() {
        let scope_error = gist_value_error(
            StatusCode::FORBIDDEN,
            &json!({ "message": "Resource not accessible by personal access token" }),
            "gist",
            "fallback",
        )
        .to_string();
        let rate_error = gist_value_error(
            StatusCode::FORBIDDEN,
            &json!({ "message": "API rate limit exceeded for user" }),
            "gist",
            "fallback",
        )
        .to_string();

        assert!(scope_error.starts_with("github_gist_missing_scope:"));
        assert!(rate_error.starts_with("github_gist_rate_limited:"));
    }

    #[test]
    fn github_device_code_debug_redacts_device_code() {
        let code = GithubDeviceCode {
            device_code: "secret-device-code".to_string(),
            user_code: "ABCD-EFGH".to_string(),
            verification_uri: "https://github.com/login/device".to_string(),
            expires_in: 900,
            interval: 5,
        };
        let debug = format!("{code:?}");

        assert!(debug.contains("redacted"));
        assert!(!debug.contains("secret-device-code"));
        assert!(debug.contains("ABCD-EFGH"));
    }

    #[test]
    fn microsoft_device_code_debug_redacts_device_code() {
        let code = MicrosoftDeviceCode {
            device_code: "secret-microsoft-device-code".to_string(),
            user_code: "WXYZ-1234".to_string(),
            verification_uri: "https://microsoft.com/devicelogin".to_string(),
            expires_in: 900,
            interval: 5,
        };
        let debug = format!("{code:?}");

        assert!(debug.contains("redacted"));
        assert!(!debug.contains("secret-microsoft-device-code"));
        assert!(debug.contains("WXYZ-1234"));
    }

    #[test]
    fn oauth_token_debug_redacts_token_values() {
        let github = GithubDeviceTokenPoll::Token {
            access_token: Zeroizing::new("github-secret-token".to_string()),
        };
        let microsoft = MicrosoftDeviceTokenPoll::Token {
            access_token: Zeroizing::new("microsoft-access-token".to_string()),
            refresh_token: Zeroizing::new("microsoft-refresh-token".to_string()),
        };
        let refreshed = MicrosoftTokenRefresh {
            access_token: Zeroizing::new("refreshed-access-token".to_string()),
            refresh_token: Some(Zeroizing::new("refreshed-refresh-token".to_string())),
        };
        let debug = format!("{github:?} {microsoft:?} {refreshed:?}");

        assert!(debug.contains("redacted"));
        assert!(!debug.contains("github-secret-token"));
        assert!(!debug.contains("microsoft-access-token"));
        assert!(!debug.contains("microsoft-refresh-token"));
        assert!(!debug.contains("refreshed-access-token"));
        assert!(!debug.contains("refreshed-refresh-token"));
    }

    #[test]
    fn onedrive_paths_use_graph_app_folder_layout() {
        let settings = CloudSyncSettings {
            backend_type: BackendType::OneDrive,
            namespace: "ignored".to_string(),
            ..CloudSyncSettings::default()
        };
        let upload = RemoteSnapshotUpload {
            revision: "rev/one".to_string(),
            device_id: "device".to_string(),
            uploaded_at: "2026-06-13T00:00:00Z".to_string(),
            bytes: Vec::new(),
            etag: Some("hash:abc".to_string()),
            previous_etag: None,
            section_revisions: None,
        };

        assert_eq!(onedrive_paths(&settings).metadata_path, "metadata.json");
        assert_eq!(onedrive_blob_path(&upload), "objects/hash-abc.oxide");
        assert_eq!(
            onedrive_content_url(&settings, "objects/hash-abc.oxide"),
            "https://graph.microsoft.com/v1.0/me/drive/special/approot:/objects/hash-abc.oxide:/content"
        );
    }

    #[test]
    fn onedrive_error_mapping_distinguishes_scope_rate_and_conflict() {
        let scope_error = onedrive_value_error(
            StatusCode::FORBIDDEN,
            &json!({ "error": { "message": "Missing Files.ReadWrite.AppFolder" } }),
            "onedrive",
            "fallback",
        )
        .to_string();
        let rate_error = onedrive_value_error(
            StatusCode::TOO_MANY_REQUESTS,
            &json!({ "error": { "message": "Too many requests" } }),
            "onedrive",
            "fallback",
        )
        .to_string();
        let conflict_error = onedrive_value_error(
            StatusCode::PRECONDITION_FAILED,
            &json!({ "error": { "message": "ETag changed" } }),
            "onedrive",
            "fallback",
        )
        .to_string();

        assert!(scope_error.starts_with("onedrive_missing_scope:"));
        assert!(rate_error.starts_with("onedrive_rate_limited:"));
        assert!(conflict_error.starts_with("etag_conflict_detected:"));
    }

    #[test]
    fn onedrive_error_mapping_distinguishes_graph_configuration_failures() {
        let access_error = onedrive_value_error(
            StatusCode::FORBIDDEN,
            &json!({ "error": { "code": "accessDenied", "message": "Tenant policy blocked this app" } }),
            "onedrive",
            "fallback",
        )
        .to_string();
        let bad_request_error = onedrive_value_error(
            StatusCode::BAD_REQUEST,
            &json!({ "error": { "message": "Invalid app folder request" } }),
            "onedrive",
            "fallback",
        )
        .to_string();
        let locked_error = onedrive_value_error(
            StatusCode::from_u16(423).unwrap(),
            &json!({ "error": { "message": "Resource is locked" } }),
            "onedrive",
            "fallback",
        )
        .to_string();
        let service_error = onedrive_value_error(
            StatusCode::SERVICE_UNAVAILABLE,
            &json!({ "error": { "message": "Service unavailable" } }),
            "onedrive",
            "fallback",
        )
        .to_string();

        assert!(access_error.starts_with("onedrive_access_denied:"));
        assert!(bad_request_error.starts_with("onedrive_bad_request:"));
        assert!(locked_error.starts_with("onedrive_locked:"));
        assert!(service_error.starts_with("onedrive_service_unavailable:"));
    }

    #[test]
    fn microsoft_oauth_error_mapping_distinguishes_configuration_failures() {
        let scope_error = microsoft_oauth_value_error(
            &json!({
                "error": "invalid_scope",
                "error_description": "Files.ReadWrite.AppFolder is not configured"
            }),
            "microsoft_oauth_start_failed",
        )
        .to_string();
        let consent_error = microsoft_oauth_error(
            &MicrosoftTokenResponse {
                access_token: None,
                refresh_token: None,
                error: Some("consent_required".to_string()),
                error_description: Some("Admin consent is required".to_string()),
                interval: None,
            },
            "microsoft_oauth_poll_failed",
        )
        .to_string();
        let invalid_request_error = microsoft_oauth_error(
            &MicrosoftTokenResponse {
                access_token: None,
                refresh_token: None,
                error: Some("invalid_request".to_string()),
                error_description: Some("Device flow is not enabled".to_string()),
                interval: None,
            },
            "microsoft_oauth_poll_failed",
        )
        .to_string();

        assert!(scope_error.starts_with("microsoft_oauth_missing_scope:"));
        assert!(consent_error.starts_with("microsoft_oauth_consent_required:"));
        assert!(invalid_request_error.starts_with("microsoft_oauth_invalid_request:"));
    }

    #[test]
    fn onedrive_cleanup_keeps_current_blob_and_legacy_latest_only() {
        let keep = onedrive_keep_object_paths(&json!({
            "blobPath": "objects/current.oxide"
        }));

        assert!(keep.contains("objects/current.oxide"));
        assert!(keep.contains("objects/latest.oxide"));
        assert!(!keep.contains("objects/old.oxide"));
    }

    #[test]
    fn retry_after_parser_accepts_seconds_and_caps_delay() {
        assert_eq!(parse_retry_after("2"), Some(Duration::from_secs(2)));
        assert_eq!(
            parse_retry_after("120")
                .unwrap()
                .min(CLOUD_REQUEST_MAX_RETRY_AFTER),
            CLOUD_REQUEST_MAX_RETRY_AFTER
        );
        assert!(parse_retry_after("not a retry-after").is_none());
    }

    #[test]
    fn http_json_error_fallback_uses_tauri_numeric_status_text() {
        let error = http_json_value_error(
            StatusCode::UNAUTHORIZED,
            &Value::Null,
            "http",
            "Failed to fetch remote metadata",
        )
        .to_string();

        assert_eq!(error, "http_401: Failed to fetch remote metadata (401)");
    }

    #[test]
    fn http_json_error_prefers_remote_error_payload_like_tauri() {
        let value = json!({
            "error": {
                "code": "etag_conflict_detected",
                "message": "Remote changed"
            }
        });
        let error = http_json_value_error(
            StatusCode::PRECONDITION_FAILED,
            &value,
            "http",
            "Failed to upload snapshot",
        )
        .to_string();

        assert_eq!(error, "etag_conflict_detected: Remote changed");
    }

    #[test]
    fn webdav_namespace_url_matches_tauri_jianguoyun_duplicate_guard() {
        let settings = CloudSyncSettings {
            backend_type: BackendType::Webdav,
            endpoint: "https://dav.jianguoyun.com/dav/oxideterm".to_string(),
            namespace: "oxideterm".to_string(),
            ..CloudSyncSettings::default()
        };

        assert_eq!(
            webdav_namespace_url(&settings),
            "https://dav.jianguoyun.com/dav/oxideterm"
        );
    }

    #[test]
    fn webdav_namespace_url_appends_namespace_for_regular_endpoints() {
        let settings = CloudSyncSettings {
            backend_type: BackendType::Webdav,
            endpoint: "https://example.com/dav/".to_string(),
            namespace: "team/default".to_string(),
            ..CloudSyncSettings::default()
        };

        assert_eq!(
            webdav_namespace_url(&settings),
            "https://example.com/dav/team/default"
        );
    }

    #[test]
    fn webdav_collection_chain_builds_parent_first_paths() {
        assert_eq!(
            webdav_collection_chain("https://example.com/dav/team/default"),
            vec![
                "https://example.com/dav",
                "https://example.com/dav/team",
                "https://example.com/dav/team/default"
            ]
        );
    }
}
