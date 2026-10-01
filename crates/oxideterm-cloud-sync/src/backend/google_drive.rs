// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Google Drive provider request construction, authentication, parsing, and errors.

use super::*;
mod publications;
use publications::google_drive_scoped_object_name;

const GOOGLE_DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";
const GOOGLE_DRIVE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";
const GOOGLE_DRIVE_APPDATA_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";

impl CloudSyncBackend {
    pub(super) async fn fetch_google_drive_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        let paths = google_drive_paths();
        let Some(object) = self
            .read_google_drive_object(config, secrets, &paths.metadata_path)
            .await?
        else {
            return Ok(RemoteMetadata::missing());
        };
        let value = serde_json::from_slice::<Value>(&object.bytes)?;
        let mut metadata = normalize_remote_metadata(value, object.etag)?;
        metadata.blob_path.get_or_insert(paths.blob_path);
        Ok(metadata)
    }

    pub(super) async fn upload_google_drive_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        let metadata_path = google_drive_paths().metadata_path;
        let blob_path = google_drive_blob_path(&payload);
        let mut metadata = payload.metadata_json_with_blob_path(&blob_path);
        metadata["namespace"] = Value::String(config.namespace.clone());
        self.write_google_drive_object(
            config,
            secrets,
            &blob_path,
            payload.bytes,
            Some(OXIDE_CONTENT_TYPE),
            None,
        )
        .await?;
        let result = self
            .write_google_drive_object(
                config,
                secrets,
                &metadata_path,
                serde_json::to_vec(&metadata)?,
                Some("application/json"),
                payload.previous_etag.as_deref(),
            )
            .await?;
        self.cleanup_google_drive_objects(config, secrets, &metadata)
            .await?;
        Ok(RemoteWriteResult {
            revision: payload.revision,
            etag: result.etag.or(payload.etag),
        })
    }

    pub(super) async fn read_google_drive_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        let Some(file) = self
            .find_google_drive_file(
                secrets,
                &google_drive_scoped_object_name(config, relative_path),
            )
            .await?
        else {
            return Ok(None);
        };
        let response = execute_cloud_request(
            self.client
                .get(google_drive_media_url(&file.id))
                .headers(google_drive_headers(secrets)?),
        )
        .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status();
            let value = response.json::<Value>().await.unwrap_or(Value::Null);
            return Err(google_drive_value_error(
                status,
                &value,
                "google_drive_download",
                "Failed to download Google Drive content",
            ));
        }
        let mut object =
            response_to_object(response, &format!("Google Drive object {relative_path}")).await?;
        object.etag = file.head_revision_id.or(object.etag);
        object.last_modified = file.modified_time.or(object.last_modified);
        object.content_type = file.mime_type.or(object.content_type);
        Ok(Some(object))
    }

    pub(super) async fn write_google_drive_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
        content_type: Option<&str>,
        expected_etag: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        let name = google_drive_scoped_object_name(config, relative_path);
        let current = self.find_google_drive_file(secrets, &name).await?;
        if let Some(expected_etag) = expected_etag {
            let current_etag = current
                .as_ref()
                .and_then(|file| file.head_revision_id.as_deref());
            if current_etag != Some(expected_etag) {
                bail!(
                    "etag_conflict_detected: Google Drive metadata changed before upload completed"
                );
            }
        } else if relative_path == google_drive_paths().metadata_path && current.is_some() {
            bail!("etag_conflict_detected: Google Drive metadata already exists");
        }

        let file = if let Some(file) = current {
            self.update_google_drive_file(
                secrets,
                &file.id,
                bytes,
                content_type.unwrap_or("application/octet-stream"),
            )
            .await?
        } else {
            self.create_google_drive_file(
                secrets,
                &name,
                bytes,
                content_type.unwrap_or("application/octet-stream"),
            )
            .await?
        };

        Ok(RemoteWriteResult {
            revision: file.head_revision_id.clone().unwrap_or_default(),
            etag: file.head_revision_id,
        })
    }

    async fn cleanup_google_drive_objects(
        &self,
        _config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &Value,
    ) -> Result<()> {
        let files = self
            .list_google_drive_files(secrets, "name contains 'objects__'")
            .await?;
        let keep = google_drive_keep_object_names(metadata);
        for file in files {
            let Some(name) = file.name.as_deref() else {
                continue;
            };
            if !name.ends_with(".oxide") || keep.contains(name) {
                continue;
            }
            let response = execute_cloud_request(
                self.client
                    .delete(format!(
                        "{GOOGLE_DRIVE_API_BASE}/files/{}",
                        encode_component(&file.id)
                    ))
                    .headers(google_drive_headers(secrets)?),
            )
            .await?;
            let status = response.status();
            if status == StatusCode::NOT_FOUND {
                continue;
            }
            if !status.is_success() {
                let value = response.json::<Value>().await.unwrap_or(Value::Null);
                return Err(google_drive_value_error(
                    status,
                    &value,
                    "google_drive_cleanup",
                    "Failed to remove old Google Drive object",
                ));
            }
        }
        Ok(())
    }

    async fn find_google_drive_file(
        &self,
        secrets: &CloudSyncSecrets,
        name: &str,
    ) -> Result<Option<GoogleDriveFile>> {
        let query = format!(
            "name = {} and 'appDataFolder' in parents and trashed = false",
            google_drive_query_literal(name)
        );
        let files = self.list_google_drive_files(secrets, &query).await?;
        Ok(files.into_iter().next())
    }

    async fn list_google_drive_files(
        &self,
        secrets: &CloudSyncSecrets,
        query: &str,
    ) -> Result<Vec<GoogleDriveFile>> {
        let response = execute_cloud_request(
            self.client
                .get(format!("{GOOGLE_DRIVE_API_BASE}/files"))
                .headers(google_drive_headers(secrets)?)
                .query(&[
                    ("spaces", "appDataFolder"),
                    (
                        "fields",
                        "files(id,name,headRevisionId,modifiedTime,mimeType)",
                    ),
                    ("pageSize", "1000"),
                    ("q", query),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(google_drive_value_error(
                status,
                &value,
                "google_drive_list",
                "Failed to list Google Drive app data files",
            ));
        }
        let list = serde_json::from_value::<GoogleDriveFileList>(value)?;
        Ok(list.files)
    }

    async fn create_google_drive_file(
        &self,
        secrets: &CloudSyncSecrets,
        name: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<GoogleDriveFile> {
        let boundary = format!("oxideterm-{}", digest_hex(name.as_bytes()));
        let body = google_drive_multipart_body(name, content_type, &boundary, &bytes)?;
        let response = execute_cloud_request(
            self.client
                .post(format!("{GOOGLE_DRIVE_UPLOAD_BASE}/files"))
                .headers(google_drive_headers(secrets)?)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/related; boundary={boundary}"),
                )
                .query(&[
                    ("uploadType", "multipart"),
                    ("fields", "id,name,headRevisionId,modifiedTime,mimeType"),
                ])
                .body(body),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(google_drive_value_error(
                status,
                &value,
                "google_drive_write",
                "Failed to create Google Drive app data file",
            ));
        }
        Ok(serde_json::from_value(value)?)
    }

    async fn update_google_drive_file(
        &self,
        secrets: &CloudSyncSecrets,
        file_id: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<GoogleDriveFile> {
        let response = execute_cloud_request(
            self.client
                .patch(format!(
                    "{GOOGLE_DRIVE_UPLOAD_BASE}/files/{}",
                    encode_component(file_id)
                ))
                .headers(google_drive_headers(secrets)?)
                .header(CONTENT_TYPE, content_type)
                .query(&[
                    ("uploadType", "media"),
                    ("fields", "id,name,headRevisionId,modifiedTime,mimeType"),
                ])
                .body(bytes),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(google_drive_value_error(
                status,
                &value,
                "google_drive_write",
                "Failed to update Google Drive app data file",
            ));
        }
        Ok(serde_json::from_value(value)?)
    }

    pub(super) async fn write_google_drive_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &Value,
        expected_etag: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        let paths = google_drive_paths();
        let result = self
            .write_google_drive_object(
                config,
                secrets,
                &paths.metadata_path,
                serde_json::to_vec(metadata)?,
                Some("application/json"),
                expected_etag,
            )
            .await?;
        self.cleanup_google_drive_objects(config, secrets, metadata)
            .await?;
        Ok(result)
    }

    pub(super) async fn download_google_drive_snapshot_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &RemoteMetadata,
    ) -> Result<RemoteObject> {
        let path = metadata
            .blob_path
            .as_deref()
            .unwrap_or(&google_drive_paths().blob_path)
            .to_string();
        self.read_google_drive_object(config, secrets, &path)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("remote_not_found: no remote Google Drive snapshot found")
            })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleDriveFileList {
    #[serde(default)]
    files: Vec<GoogleDriveFile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleDriveFile {
    id: String,
    name: Option<String>,
    head_revision_id: Option<String>,
    modified_time: Option<String>,
    mime_type: Option<String>,
}

fn google_drive_multipart_body(
    name: &str,
    content_type: &str,
    boundary: &str,
    bytes: &[u8],
) -> Result<Vec<u8>> {
    let metadata = serde_json::to_vec(&json!({
        "name": name,
        "parents": ["appDataFolder"],
        "mimeType": content_type,
    }))?;
    let mut body = Vec::with_capacity(metadata.len() + bytes.len() + boundary.len() * 4 + 256);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
    body.extend_from_slice(&metadata);
    body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok(body)
}

struct GoogleDrivePaths {
    metadata_path: String,
    blob_path: String,
}

fn google_drive_paths() -> GoogleDrivePaths {
    GoogleDrivePaths {
        metadata_path: "metadata.json".to_string(),
        blob_path: "objects__latest.oxide".to_string(),
    }
}

fn google_drive_blob_path(payload: &RemoteSnapshotUpload) -> String {
    let stable_name = payload
        .etag
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&payload.revision);
    format!(
        "objects__{}.oxide",
        sanitize_remote_object_name(stable_name)
    )
}

fn google_drive_object_name(relative_path: &str) -> String {
    let path = trim_slashes(relative_path);
    if path.is_empty() {
        return google_drive_paths().metadata_path;
    }
    if path == google_drive_paths().metadata_path {
        return path;
    }
    sanitize_remote_object_name(&path.replace('/', "__"))
}

fn google_drive_media_url(file_id: &str) -> String {
    format!(
        "{GOOGLE_DRIVE_API_BASE}/files/{}?alt=media",
        encode_component(file_id)
    )
}

fn google_drive_keep_object_names(metadata: &Value) -> BTreeSet<String> {
    let mut keep = BTreeSet::new();
    if let Some(blob_path) = metadata.get("blobPath").and_then(Value::as_str) {
        keep.insert(google_drive_object_name(blob_path));
    }
    keep.insert(google_drive_paths().blob_path);
    keep
}

fn google_drive_headers(secrets: &CloudSyncSecrets) -> Result<HeaderMap> {
    let token = secrets
        .token
        .as_ref()
        .map(|token| token.as_str())
        .filter(|token| !token.is_empty())
        .context("missing_backend_token: Google Drive access token is not configured")?;
    let mut headers = HeaderMap::new();
    insert_header(&mut headers, ACCEPT.as_str(), "application/json")?;
    headers.insert(USER_AGENT, HeaderValue::from_static("OxideTerm"));
    insert_bearer_auth_header(&mut headers, token)?;
    Ok(headers)
}

fn google_drive_value_error(
    status: StatusCode,
    value: &Value,
    code_prefix: &str,
    fallback: &str,
) -> anyhow::Error {
    let status_code = status.as_u16();
    let message = google_error_message(value).unwrap_or(fallback);
    let reason = google_error_reason(value).unwrap_or_default();
    let code = match status {
        StatusCode::BAD_REQUEST => {
            if google_drive_api_disabled(&reason, message) {
                "google_drive_api_not_enabled".to_string()
            } else {
                "google_drive_bad_request".to_string()
            }
        }
        StatusCode::UNAUTHORIZED => "google_drive_bad_credentials".to_string(),
        StatusCode::FORBIDDEN => {
            if google_drive_api_disabled(&reason, message) {
                "google_drive_api_not_enabled".to_string()
            } else if google_drive_quota_error(&reason, message) {
                "google_drive_quota_exceeded".to_string()
            } else if google_drive_rate_limit_error(&reason, message) {
                "google_drive_rate_limited".to_string()
            } else if google_drive_scope_error(&reason, message) {
                "google_drive_missing_scope".to_string()
            } else {
                "google_drive_access_denied".to_string()
            }
        }
        StatusCode::TOO_MANY_REQUESTS => "google_drive_rate_limited".to_string(),
        StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
            "google_drive_service_unavailable".to_string()
        }
        status if status.as_u16() == 507 => "google_drive_quota_exceeded".to_string(),
        _ => format!("{code_prefix}_{status_code}"),
    };
    anyhow::anyhow!("{code}: {message}")
}

fn google_error_message(value: &Value) -> Option<&str> {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| value.get("error_description").and_then(Value::as_str))
        .or_else(|| value.get("message").and_then(Value::as_str))
}

fn google_error_reason(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| error.get("errors"))
        .and_then(Value::as_array)
        .and_then(|errors| errors.first())
        .and_then(|error| error.get("reason"))
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("error")
                .and_then(|error| error.get("status"))
                .and_then(Value::as_str)
        })
        .or_else(|| value.get("error").and_then(Value::as_str))
        .map(str::to_string)
}

fn google_drive_api_disabled(reason: &str, message: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    reason.contains("accessnotconfigured")
        || reason.contains("servicedisabled")
        || message.contains("drive api has not been used")
        || message.contains("drive.googleapis.com")
        || message.contains("api has not been enabled")
}

fn google_drive_quota_error(reason: &str, message: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    reason.contains("quota")
        || reason.contains("dailylimitexceeded")
        || reason.contains("storagelimitexceeded")
        || message.contains("quota")
}

fn google_drive_rate_limit_error(reason: &str, message: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    reason.contains("ratelimit")
        || reason.contains("userratelimit")
        || message.contains("rate limit")
        || message.contains("too many requests")
}

fn google_drive_scope_error(reason: &str, message: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    reason.contains("insufficientpermissions")
        || message.contains("insufficient permission")
        || message.contains(GOOGLE_DRIVE_APPDATA_SCOPE)
        || message.contains("scope")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_drive_paths_use_flat_appdata_names() {
        let upload = RemoteSnapshotUpload {
            revision: "rev/one".to_string(),
            device_id: "device".to_string(),
            uploaded_at: "2026-06-13T00:00:00Z".to_string(),
            bytes: Vec::new(),
            etag: Some("hash:abc/def".to_string()),
            previous_etag: None,
            section_revisions: None,
        };

        assert_eq!(google_drive_paths().metadata_path, "metadata.json");
        assert_eq!(google_drive_paths().blob_path, "objects__latest.oxide");
        assert_eq!(
            google_drive_blob_path(&upload),
            "objects__hash-abc-def.oxide"
        );
        assert_eq!(
            google_drive_object_name("objects/hash:abc.oxide"),
            "objects__hash-abc.oxide"
        );
        assert_eq!(google_drive_object_name("metadata.json"), "metadata.json");
    }

    #[test]
    fn google_drive_error_mapping_distinguishes_setup_scope_and_quota_failures() {
        let api_disabled = google_drive_value_error(
            StatusCode::FORBIDDEN,
            &json!({
                "error": {
                    "errors": [{ "reason": "accessNotConfigured" }],
                    "message": "Google Drive API has not been used in project"
                }
            }),
            "google_drive",
            "fallback",
        )
        .to_string();
        let missing_scope = google_drive_value_error(
            StatusCode::FORBIDDEN,
            &json!({
                "error": {
                    "errors": [{ "reason": "insufficientPermissions" }],
                    "message": "Request is missing https://www.googleapis.com/auth/drive.appdata"
                }
            }),
            "google_drive",
            "fallback",
        )
        .to_string();
        let quota = google_drive_value_error(
            StatusCode::FORBIDDEN,
            &json!({
                "error": {
                    "errors": [{ "reason": "storageQuotaExceeded" }],
                    "message": "Quota exceeded"
                }
            }),
            "google_drive",
            "fallback",
        )
        .to_string();
        let rate = google_drive_value_error(
            StatusCode::TOO_MANY_REQUESTS,
            &json!({ "error": { "message": "Rate Limit Exceeded" } }),
            "google_drive",
            "fallback",
        )
        .to_string();

        assert!(api_disabled.starts_with("google_drive_api_not_enabled:"));
        assert!(missing_scope.starts_with("google_drive_missing_scope:"));
        assert!(quota.starts_with("google_drive_quota_exceeded:"));
        assert!(rate.starts_with("google_drive_rate_limited:"));
    }
}
