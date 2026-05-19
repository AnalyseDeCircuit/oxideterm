// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use reqwest::{
    Client, Method, RequestBuilder, Response, StatusCode, Url,
    header::{
        ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, ETAG, HeaderMap, HeaderName,
        HeaderValue,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    BackendType, CloudSyncSettings, OXIDE_CONTENT_TYPE, StructuredSectionRevisions,
    secrets::{CloudSyncSecrets, backend_uses_basic, backend_uses_token},
};

const DROPBOX_API_BASE: &str = "https://api.dropboxapi.com/2";
const DROPBOX_CONTENT_BASE: &str = "https://content.dropboxapi.com/2";
const DEFAULT_GIT_API_ENDPOINT: &str = "https://api.github.com";

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

    pub async fn fetch_remote_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        validate_namespace(config)?;
        match config.backend_type {
            BackendType::HttpJson => self.fetch_http_json_metadata(config, secrets).await,
            BackendType::Dropbox => self.fetch_dropbox_metadata(config, secrets).await,
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
    ) -> Result<RemoteWriteResult> {
        if matches!(config.backend_type, BackendType::HttpJson) {
            return self
                .write_http_json_metadata(config, secrets, metadata)
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
                    bail!(
                        "http_blob_{}: failed to download snapshot",
                        response.status()
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
                    bail!(
                        "webdav_blob_{}: failed to download snapshot",
                        response.status()
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
                    bail!(
                        "s3_blob_{}: failed to download S3 snapshot",
                        response.status()
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
            return Err(http_json_error(response, "http", "failed to fetch remote metadata").await);
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
            bail!(
                "webdav_{}: failed to fetch WebDAV metadata",
                response.status()
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
                "failed to upload snapshot",
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
            bail!(
                "webdav_blob_{}: failed to upload WebDAV blob",
                blob_response.status()
            );
        }
        let mut metadata = payload.metadata_json();
        metadata["namespace"] = Value::String(config.namespace.clone());
        self.write_remote_metadata(config, secrets, &metadata)
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
            return Err(http_json_error(response, "http_object", "failed to upload object").await);
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
                http_json_error(response, "http_object", "failed to download object").await,
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
            bail!(
                "webdav_object_{}: failed to upload WebDAV object",
                response.status()
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
            return Err(http_json_error(response, "http_meta", "failed to write metadata").await);
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
            bail!(
                "{}_{}: failed to read object",
                error_prefix,
                response.status()
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
            if let Some(token) = secrets.token.as_deref() {
                insert_header(
                    &mut headers,
                    AUTHORIZATION.as_str(),
                    &format!("Bearer {token}"),
                )?;
            }
        }
        if backend_uses_basic(&config.backend_type, &config.auth_mode)
            && let (Some(username), Some(password)) = (
                secrets.basic_username.as_deref(),
                secrets.basic_password.as_deref(),
            )
        {
            let encoded = BASE64.encode(format!("{username}:{password}"));
            insert_header(
                &mut headers,
                AUTHORIZATION.as_str(),
                &format!("Basic {encoded}"),
            )?;
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
                        "namespace_create_failed: failed to prepare WebDAV namespace ({})",
                        parent_response.status()
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
                "namespace_create_failed: failed to prepare WebDAV namespace ({})",
                retry.status()
            );
        }
        bail!(
            "namespace_create_failed: failed to prepare WebDAV namespace ({})",
            response.status()
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
            bail!(
                "dropbox_folder_{}: failed to prepare Dropbox namespace",
                response.status()
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
            bail!(
                "dropbox_download_{}: failed to download Dropbox file",
                response.status()
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
            bail!(
                "dropbox_upload_{}: failed to upload Dropbox file",
                response.status()
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
            bail!("git_{}: failed to fetch Git content", response.status());
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
            bail!(
                "git_blob_{}: failed to download Git content",
                response.status()
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
        if secrets.git_token.as_deref().unwrap_or_default().is_empty() {
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
        if matches!(response.status().as_u16(), 409 | 422) {
            bail!("etag_conflict_detected: remote Git snapshot changed during upload");
        }
        if !response.status().is_success() {
            bail!(
                "git_write_{}: failed to update Git content",
                response.status()
            );
        }
        Ok(response.json::<Value>().await.unwrap_or(Value::Null))
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
        && !matches!(config.backend_type, BackendType::S3 | BackendType::Git)
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
    let fallback_code = format!("{code_prefix}_{}", status.as_u16());
    let fallback_message = format!("{fallback} ({status})");
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
    request.send().await.map_err(normalize_network_error)
}

fn normalize_network_error(error: reqwest::Error) -> anyhow::Error {
    if error.is_connect() || error.is_timeout() || error.is_request() {
        anyhow::anyhow!("network_request_failed: {}", error)
    } else {
        anyhow::Error::new(error)
    }
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
        .as_deref()
        .filter(|token| !token.is_empty())
        .context("missing_backend_token: Dropbox access token is not configured")?;
    let mut headers = HeaderMap::new();
    insert_header(
        &mut headers,
        AUTHORIZATION.as_str(),
        &format!("Bearer {token}"),
    )?;
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
        .as_deref()
        .filter(|token| !token.is_empty())
    {
        insert_header(
            &mut headers,
            AUTHORIZATION.as_str(),
            &format!("Bearer {token}"),
        )?;
    }
    Ok(headers)
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
