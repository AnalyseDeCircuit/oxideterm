// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Unified cloud backend interface, provider dispatch, and shared HTTP execution.

use std::{collections::BTreeSet, fmt, future::Future, pin::Pin, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD},
};
use chrono::{DateTime, Utc};
use rand::RngCore;
use reqwest::{
    IntoUrl, Method, StatusCode, Url,
    header::{
        ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, ETAG, HeaderMap, HeaderName,
        HeaderValue, RETRY_AFTER, USER_AGENT,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    BackendType, CloudSyncSettings, OXIDE_CONTENT_TYPE, StructuredSectionRevisions,
    secrets::{CloudSyncSecrets, backend_uses_basic, backend_uses_token},
};

const CLOUD_REQUEST_MAX_RETRY_ATTEMPTS: usize = 3;
const CLOUD_REQUEST_MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

type HttpExecuteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HttpResponseSnapshot>> + Send + 'a>>;

/// Executes an owned, replayable HTTP request without exposing reqwest types to providers.
trait HttpExecutor: Send + Sync {
    fn execute(&self, request: HttpRequestSpec) -> HttpExecuteFuture<'_>;
}

#[derive(Clone)]
struct HttpRequestSpec {
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: HttpRequestBody,
}

#[derive(Clone, Default)]
enum HttpRequestBody {
    #[default]
    Empty,
    Bytes(Zeroizing<Vec<u8>>),
    Form(Vec<(Zeroizing<String>, Zeroizing<String>)>),
}

impl fmt::Debug for HttpRequestSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_names = self
            .headers
            .keys()
            .map(HeaderName::as_str)
            .collect::<Vec<_>>();
        let body = match &self.body {
            HttpRequestBody::Empty => "empty".to_string(),
            HttpRequestBody::Bytes(bytes) => format!("bytes({})", bytes.len()),
            HttpRequestBody::Form(fields) => format!("form({} fields)", fields.len()),
        };
        formatter
            .debug_struct("HttpRequestSpec")
            .field("method", &self.method)
            .field("url", &redacted_request_url(&self.url))
            .field("header_names", &header_names)
            .field("body", &body)
            .finish()
    }
}

struct HttpResponseSnapshot {
    status: StatusCode,
    headers: HeaderMap,
    body: Zeroizing<Vec<u8>>,
}

impl HttpResponseSnapshot {
    fn new(status: StatusCode, headers: HeaderMap, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body: Zeroizing::new(body),
        }
    }

    fn status(&self) -> StatusCode {
        self.status
    }

    fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    async fn json<T: DeserializeOwned>(self) -> serde_json::Result<T> {
        serde_json::from_slice(self.body.as_slice())
    }

    async fn bytes(mut self) -> Result<Vec<u8>> {
        Ok(std::mem::take(&mut *self.body))
    }

    async fn text(self) -> Result<String> {
        Ok(String::from_utf8_lossy(self.body.as_slice()).into_owned())
    }
}

impl fmt::Debug for HttpResponseSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_names = self
            .headers
            .keys()
            .map(HeaderName::as_str)
            .collect::<Vec<_>>();
        formatter
            .debug_struct("HttpResponseSnapshot")
            .field("status", &self.status)
            .field("header_names", &header_names)
            .field("body_len", &self.body.len())
            .finish()
    }
}

#[derive(Clone)]
struct CloudHttpClient {
    executor: Arc<dyn HttpExecutor>,
}

impl CloudHttpClient {
    fn new(executor: Arc<dyn HttpExecutor>) -> Self {
        Self { executor }
    }

    fn get<U: IntoUrl>(&self, url: U) -> PendingHttpRequest {
        self.request(Method::GET, url)
    }

    fn post<U: IntoUrl>(&self, url: U) -> PendingHttpRequest {
        self.request(Method::POST, url)
    }

    fn put<U: IntoUrl>(&self, url: U) -> PendingHttpRequest {
        self.request(Method::PUT, url)
    }

    fn patch<U: IntoUrl>(&self, url: U) -> PendingHttpRequest {
        self.request(Method::PATCH, url)
    }

    fn delete<U: IntoUrl>(&self, url: U) -> PendingHttpRequest {
        self.request(Method::DELETE, url)
    }

    fn request<U: IntoUrl>(&self, method: Method, url: U) -> PendingHttpRequest {
        let request = url
            .into_url()
            .map(|url| HttpRequestSpec {
                method,
                url,
                headers: HeaderMap::new(),
                body: HttpRequestBody::Empty,
            })
            .map_err(normalize_network_error);
        PendingHttpRequest {
            executor: Arc::clone(&self.executor),
            request,
        }
    }
}

struct PendingHttpRequest {
    executor: Arc<dyn HttpExecutor>,
    request: Result<HttpRequestSpec>,
}

impl PendingHttpRequest {
    fn headers(mut self, headers: HeaderMap) -> Self {
        if let Ok(request) = &mut self.request {
            request.headers.extend(headers);
        }
        self
    }

    fn header<N, V>(mut self, name: N, value: V) -> Self
    where
        N: IntoHttpHeaderName,
        V: IntoHttpHeaderValue,
    {
        self.request = self.request.and_then(|mut request| {
            let name = name.into_http_header_name()?;
            let mut value = value.into_http_header_value()?;
            if name == AUTHORIZATION {
                value.set_sensitive(true);
            }
            request.headers.insert(name, value);
            Ok(request)
        });
        self
    }

    fn body(mut self, body: Vec<u8>) -> Self {
        if let Ok(request) = &mut self.request {
            // Request bodies may contain OAuth credentials or encrypted snapshots;
            // every replay copy therefore retains zeroizing ownership.
            request.body = HttpRequestBody::Bytes(Zeroizing::new(body));
        }
        self
    }

    fn form(mut self, fields: &[(&str, &str)]) -> Self {
        if let Ok(request) = &mut self.request {
            request
                .headers
                .entry(CONTENT_TYPE)
                .or_insert(HeaderValue::from_static(
                    "application/x-www-form-urlencoded",
                ));
            // OAuth form values can be authorization codes or refresh tokens, so
            // the replayable representation zeroizes every owned field.
            request.body = HttpRequestBody::Form(
                fields
                    .iter()
                    .map(|(name, value)| {
                        (
                            Zeroizing::new((*name).to_string()),
                            Zeroizing::new((*value).to_string()),
                        )
                    })
                    .collect(),
            );
        }
        self
    }

    fn query(mut self, pairs: &[(&str, &str)]) -> Self {
        if let Ok(request) = &mut self.request {
            request
                .url
                .query_pairs_mut()
                .extend_pairs(pairs.iter().copied());
        }
        self
    }

    fn into_parts(self) -> Result<(Arc<dyn HttpExecutor>, HttpRequestSpec)> {
        Ok((self.executor, self.request?))
    }
}

trait IntoHttpHeaderName {
    fn into_http_header_name(self) -> Result<HeaderName>;
}

impl IntoHttpHeaderName for HeaderName {
    fn into_http_header_name(self) -> Result<HeaderName> {
        Ok(self)
    }
}

impl IntoHttpHeaderName for &str {
    fn into_http_header_name(self) -> Result<HeaderName> {
        Ok(HeaderName::from_bytes(self.as_bytes())?)
    }
}

trait IntoHttpHeaderValue {
    fn into_http_header_value(self) -> Result<HeaderValue>;
}

impl IntoHttpHeaderValue for HeaderValue {
    fn into_http_header_value(self) -> Result<HeaderValue> {
        Ok(self)
    }
}

impl IntoHttpHeaderValue for &str {
    fn into_http_header_value(self) -> Result<HeaderValue> {
        Ok(HeaderValue::from_str(self)?)
    }
}

impl IntoHttpHeaderValue for String {
    fn into_http_header_value(self) -> Result<HeaderValue> {
        Ok(HeaderValue::from_str(&self)?)
    }
}

struct ReqwestHttpExecutor;

impl HttpExecutor for ReqwestHttpExecutor {
    fn execute(&self, request: HttpRequestSpec) -> HttpExecuteFuture<'_> {
        Box::pin(async move {
            let HttpRequestSpec {
                method,
                url,
                headers,
                body,
            } = request;
            let client = oxideterm_network_proxy::application_http_client()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let mut request = client.request(method, url).headers(headers);
            request = match body {
                HttpRequestBody::Empty => request,
                HttpRequestBody::Bytes(mut bytes) => request.body(std::mem::take(&mut *bytes)),
                HttpRequestBody::Form(fields) => {
                    let fields = fields
                        .iter()
                        .map(|(name, value)| (name.as_str(), value.as_str()))
                        .collect::<Vec<_>>();
                    request.form(&fields)
                }
            };
            let response = request
                .timeout(Duration::from_secs(30))
                .send()
                .await
                .map_err(normalize_network_error)?;
            let status = response.status();
            let headers = response.headers().clone();
            let body = response
                .bytes()
                .await
                .map_err(normalize_network_error)?
                .to_vec();
            Ok(HttpResponseSnapshot::new(status, headers, body))
        })
    }
}

fn redacted_request_url(url: &Url) -> String {
    // Origin excludes user info while path-only formatting excludes query and
    // fragment credentials without cloning the original secret-bearing URL.
    format!("{}{}", url.origin().ascii_serialization(), url.path())
}

mod auth;
mod dropbox;
mod git;
mod github_gist;
mod google_drive;
mod http_json;
mod onedrive;
mod s3;
mod webdav;

pub use auth::{
    GithubDeviceCode, GithubDeviceTokenPoll, GoogleOauthStart, GoogleTokenRefresh,
    MicrosoftDeviceCode, MicrosoftDeviceTokenPoll, MicrosoftTokenRefresh,
};

#[derive(Clone)]
pub struct CloudSyncBackend {
    client: CloudHttpClient,
}

impl fmt::Debug for CloudSyncBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("CloudSyncBackend").finish()
    }
}

impl Default for CloudSyncBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudSyncBackend {
    pub fn new() -> Self {
        Self {
            client: CloudHttpClient::new(Arc::new(ReqwestHttpExecutor)),
        }
    }

    #[cfg(test)]
    fn with_http_executor(executor: Arc<dyn HttpExecutor>) -> Self {
        Self {
            client: CloudHttpClient::new(executor),
        }
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
            BackendType::GoogleDrive => self.fetch_google_drive_metadata(config, secrets).await,
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
            BackendType::GoogleDrive => {
                self.upload_google_drive_snapshot(config, secrets, payload)
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
            BackendType::GoogleDrive => {
                self.write_google_drive_object(
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
            BackendType::GoogleDrive => {
                self.read_google_drive_object(config, secrets, relative_path)
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
        metadata: &Value,
        expected_etag: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        match config.backend_type {
            BackendType::HttpJson => {
                self.write_http_json_metadata(config, secrets, metadata)
                    .await
            }
            BackendType::OneDrive => {
                self.write_onedrive_metadata(config, secrets, metadata, expected_etag)
                    .await
            }
            BackendType::GoogleDrive => {
                self.write_google_drive_metadata(config, secrets, metadata, expected_etag)
                    .await
            }
            BackendType::GithubGist => {
                self.write_gist_metadata(config, secrets, metadata, expected_etag)
                    .await
            }
            _ => {
                self.write_remote_object(
                    config,
                    secrets,
                    "latest.json",
                    serde_json::to_vec(metadata)?,
                    Some("application/json"),
                )
                .await
            }
        }
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
            BackendType::GoogleDrive => "Google Drive metadata",
            BackendType::GithubGist => "GitHub Gist metadata",
            BackendType::Git => "Git metadata",
            BackendType::S3 => "S3 metadata",
            BackendType::Webdav => "WebDAV metadata",
        };
        assert_snapshot_size(metadata.content_length.unwrap_or(0), metadata_source)?;
        let object = match config.backend_type {
            BackendType::HttpJson => {
                self.download_http_json_snapshot_object(config, secrets)
                    .await?
            }
            BackendType::Dropbox => {
                self.download_dropbox_snapshot_object(config, secrets)
                    .await?
            }
            BackendType::OneDrive => {
                self.download_onedrive_snapshot_object(config, secrets, &metadata)
                    .await?
            }
            BackendType::GoogleDrive => {
                self.download_google_drive_snapshot_object(config, secrets, &metadata)
                    .await?
            }
            BackendType::GithubGist => {
                self.download_gist_snapshot_object(config, secrets, &metadata)
                    .await?
            }
            BackendType::Git => {
                self.download_git_snapshot_object(config, secrets, &metadata)
                    .await?
            }
            BackendType::S3 => {
                self.download_s3_snapshot_object(config, secrets, &metadata)
                    .await?
            }
            BackendType::Webdav => {
                self.download_webdav_snapshot_object(config, secrets)
                    .await?
            }
        };
        Ok(RemoteSnapshotDownload {
            metadata,
            bytes: object.bytes,
            response_etag: object.etag,
            last_modified: object.last_modified,
        })
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

fn validate_namespace(config: &CloudSyncSettings) -> Result<()> {
    if config.namespace.trim().is_empty()
        && !matches!(
            config.backend_type,
            BackendType::S3 | BackendType::Git | BackendType::GithubGist | BackendType::GoogleDrive
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

async fn response_to_object(response: HttpResponseSnapshot, source: &str) -> Result<RemoteObject> {
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

async fn execute_cloud_request(request: PendingHttpRequest) -> Result<HttpResponseSnapshot> {
    let (executor, request) = request.into_parts()?;
    for attempt in 0..CLOUD_REQUEST_MAX_RETRY_ATTEMPTS {
        let response = executor.execute(request.clone()).await?;
        if !cloud_response_should_retry(response.status()) {
            return Ok(response);
        }
        if attempt + 1 >= CLOUD_REQUEST_MAX_RETRY_ATTEMPTS {
            return Ok(response);
        }
        let delay = cloud_retry_delay(&response, attempt);
        tokio::time::sleep(delay).await;
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

fn cloud_retry_delay(response: &HttpResponseSnapshot, attempt: usize) -> Duration {
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

async fn response_write_result(response: HttpResponseSnapshot) -> RemoteWriteResult {
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
    let name = HeaderName::from_bytes(name.as_bytes())?;
    let mut value = HeaderValue::from_str(value)?;
    if name == AUTHORIZATION {
        // Mark auth values sensitive so even incidental HeaderMap Debug output
        // redacts credentials before the request-spec boundary takes ownership.
        value.set_sensitive(true);
    }
    headers.insert(name, value);
    Ok(())
}

fn digest_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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

fn sanitize_remote_object_name(value: &str) -> String {
    // OneDrive and Google Drive both encode revision identifiers as portable
    // object names, so the normalization rule belongs to their shared boundary.
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
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

fn google_drive_query_literal(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

fn provider_http_auth_headers(
    config: &CloudSyncSettings,
    secrets: &CloudSyncSecrets,
) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if backend_uses_token(&config.backend_type, &config.auth_mode)
        && let Some(token) = secrets.token.as_ref().map(|value| value.as_str())
    {
        insert_bearer_auth_header(&mut headers, token)?;
    }
    if backend_uses_basic(&config.backend_type, &config.auth_mode)
        && let (Some(username), Some(password)) = (
            secrets.basic_username.as_ref().map(|value| value.as_str()),
            secrets.basic_password.as_ref().map(|value| value.as_str()),
        )
    {
        insert_basic_auth_header(&mut headers, username, password)?;
    }
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use super::*;

    #[derive(Clone)]
    struct FakeHttpExecutor {
        state: Arc<Mutex<FakeHttpState>>,
    }

    struct FakeHttpState {
        requests: Vec<HttpRequestSpec>,
        responses: VecDeque<HttpResponseSnapshot>,
    }

    impl FakeHttpExecutor {
        fn new(responses: impl IntoIterator<Item = HttpResponseSnapshot>) -> Self {
            Self {
                state: Arc::new(Mutex::new(FakeHttpState {
                    requests: Vec::new(),
                    responses: responses.into_iter().collect(),
                })),
            }
        }

        fn requests(&self) -> Vec<HttpRequestSpec> {
            self.state.lock().unwrap().requests.clone()
        }
    }

    impl HttpExecutor for FakeHttpExecutor {
        fn execute(&self, request: HttpRequestSpec) -> HttpExecuteFuture<'_> {
            Box::pin(async move {
                let mut state = self.state.lock().unwrap();
                state.requests.push(request);
                state
                    .responses
                    .pop_front()
                    .context("fake HTTP executor has no queued response")
            })
        }
    }

    fn response(status: StatusCode, headers: HeaderMap, body: Value) -> HttpResponseSnapshot {
        HttpResponseSnapshot::new(status, headers, serde_json::to_vec(&body).unwrap())
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

    #[tokio::test]
    async fn http_json_upload_builds_authenticated_replayable_request() {
        let mut response_headers = HeaderMap::new();
        response_headers.insert(ETAG, HeaderValue::from_static("revision-etag"));
        let executor = Arc::new(FakeHttpExecutor::new([response(
            StatusCode::CREATED,
            response_headers,
            Value::Null,
        )]));
        let backend = CloudSyncBackend::with_http_executor(executor.clone());
        let config = CloudSyncSettings {
            backend_type: BackendType::HttpJson,
            auth_mode: crate::AuthMode::Bearer,
            endpoint: "https://sync.example.test/root".to_string(),
            namespace: "team/default".to_string(),
            ..CloudSyncSettings::default()
        };
        let secrets = CloudSyncSecrets {
            token: Some(Zeroizing::new("provider-secret-token".to_string())),
            ..CloudSyncSecrets::default()
        };

        let result = backend
            .write_remote_object(
                &config,
                &secrets,
                "objects/item.json",
                b"encrypted-object".to_vec(),
                Some("application/json"),
            )
            .await
            .unwrap();

        assert_eq!(result.etag.as_deref(), Some("revision-etag"));
        let requests = executor.requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.method, Method::PUT);
        assert_eq!(
            request.url.as_str(),
            "https://sync.example.test/root/v1/namespaces/team%2Fdefault/objects/objects/item.json"
        );
        assert_eq!(
            request
                .headers
                .get(AUTHORIZATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer provider-secret-token"
        );
        assert!(request.headers.get(AUTHORIZATION).unwrap().is_sensitive());
        assert_eq!(
            request.headers.get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        match &request.body {
            HttpRequestBody::Bytes(bytes) => assert_eq!(bytes.as_slice(), b"encrypted-object"),
            _ => panic!("HTTP JSON upload must use a byte body"),
        }
        let debug = format!("{request:?} {:?}", request.headers);
        assert!(!debug.contains("provider-secret-token"));
        assert!(!debug.contains("encrypted-object"));
    }

    #[tokio::test]
    async fn cloud_request_retries_retry_after_response_without_network() {
        let mut retry_headers = HeaderMap::new();
        retry_headers.insert(RETRY_AFTER, HeaderValue::from_static("0"));
        let executor = Arc::new(FakeHttpExecutor::new([
            response(
                StatusCode::SERVICE_UNAVAILABLE,
                retry_headers,
                json!({ "error": { "message": "try again" } }),
            ),
            response(
                StatusCode::OK,
                HeaderMap::new(),
                json!({ "revision": "revision-after-retry" }),
            ),
        ]));
        let backend = CloudSyncBackend::with_http_executor(executor.clone());
        let config = CloudSyncSettings {
            backend_type: BackendType::HttpJson,
            auth_mode: crate::AuthMode::None,
            endpoint: "https://sync.example.test".to_string(),
            namespace: "default".to_string(),
            ..CloudSyncSettings::default()
        };

        let metadata = backend
            .fetch_remote_metadata(&config, &CloudSyncSecrets::default())
            .await
            .unwrap();

        assert_eq!(metadata.revision.as_deref(), Some("revision-after-retry"));
        let requests = executor.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, Method::GET);
        assert_eq!(requests[0].url, requests[1].url);
        assert!(matches!(requests[0].body, HttpRequestBody::Empty));
        assert!(matches!(requests[1].body, HttpRequestBody::Empty));
    }
}
