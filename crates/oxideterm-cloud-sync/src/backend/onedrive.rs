// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OneDrive provider request construction, authentication, parsing, and errors.

use super::*;

const MICROSOFT_GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
const MICROSOFT_GRAPH_CONFLICT_BEHAVIOR: &str = "@microsoft.graph.conflictBehavior";
const MICROSOFT_GRAPH_REQUEST_ID_HEADER: &str = "request-id";
const ONEDRIVE_CREATE_CONFLICT_BEHAVIOR: &str = "fail";

impl CloudSyncBackend {
    pub(super) async fn fetch_onedrive_metadata(
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

    pub(super) async fn upload_onedrive_snapshot(
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

    pub(super) async fn read_onedrive_object(
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
        let request_id = onedrive_response_request_id(metadata_response.headers());
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
                request_id.as_deref(),
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
            let request_id = onedrive_response_request_id(response.headers());
            let value = response.json::<Value>().await.unwrap_or(Value::Null);
            return Err(onedrive_value_error(
                status,
                &value,
                "onedrive_download",
                "Failed to download OneDrive content",
                request_id.as_deref(),
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

    pub(super) async fn write_onedrive_object(
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
        let metadata_path = onedrive_paths(config).metadata_path;
        let create_metadata = expected_etag.is_none() && relative_path == metadata_path;
        if let Some(expected_etag) = expected_etag {
            insert_header(&mut headers, "If-Match", expected_etag)?;
        }
        let operation = if relative_path == metadata_path {
            "onedrive_metadata_upload"
        } else {
            "onedrive_object_upload"
        };
        let response = execute_cloud_request(
            self.client
                .put(onedrive_upload_url(config, relative_path, create_metadata)?)
                .headers(headers)
                .body(bytes),
        )
        .await?;
        let status = response.status();
        let request_id = onedrive_response_request_id(response.headers());
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if onedrive_conflict_error(status, &value) {
            let message = onedrive_error_message(&value)
                .unwrap_or("OneDrive object changed before upload completed");
            bail!("etag_conflict_detected: {message}");
        }
        if !status.is_success() {
            return Err(onedrive_value_error(
                status,
                &value,
                operation,
                "Failed to upload OneDrive content",
                request_id.as_deref(),
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
        let request_id = onedrive_response_request_id(response.headers());
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(onedrive_value_error(
                status,
                &value,
                "onedrive_cleanup",
                "Failed to list old OneDrive objects",
                request_id.as_deref(),
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
                let request_id = onedrive_response_request_id(response.headers());
                let value = response.json::<Value>().await.unwrap_or(Value::Null);
                return Err(onedrive_value_error(
                    status,
                    &value,
                    "onedrive_cleanup",
                    "Failed to remove old OneDrive object",
                    request_id.as_deref(),
                ));
            }
        }
        Ok(())
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
            let request_id = onedrive_response_request_id(response.headers());
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
                    request_id.as_deref(),
                ));
            }
            parent.push((*segment).to_string());
        }
        Ok(())
    }

    pub(super) async fn write_onedrive_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &Value,
        expected_etag: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        let paths = onedrive_paths(config);
        self.write_onedrive_object(
            config,
            secrets,
            &paths.metadata_path,
            serde_json::to_vec(metadata)?,
            Some("application/json"),
            expected_etag,
        )
        .await
    }

    pub(super) async fn download_onedrive_snapshot_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &RemoteMetadata,
    ) -> Result<RemoteObject> {
        let path = metadata
            .blob_path
            .as_deref()
            .unwrap_or(&onedrive_paths(config).blob_path)
            .to_string();
        self.read_onedrive_object(config, secrets, &path)
            .await?
            .ok_or_else(|| anyhow::anyhow!("remote_not_found: no remote OneDrive snapshot found"))
    }
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

fn onedrive_item_url(_config: &CloudSyncSettings, relative_path: &str) -> String {
    format!(
        "{MICROSOFT_GRAPH_BASE}/me/drive/special/approot:/{}",
        encode_path_segments(&trim_slashes(relative_path))
    )
}

fn onedrive_content_url(config: &CloudSyncSettings, relative_path: &str) -> String {
    format!("{}:/content", onedrive_item_url(config, relative_path))
}

fn onedrive_upload_url(
    config: &CloudSyncSettings,
    relative_path: &str,
    fail_on_conflict: bool,
) -> Result<Url> {
    let mut url = Url::parse(&onedrive_content_url(config, relative_path))
        .context("failed to construct OneDrive upload URL")?;
    if fail_on_conflict {
        // Graph documents conflictBehavior on create operations; using it
        // preserves create-only metadata semantics without an unsupported header.
        url.query_pairs_mut().append_pair(
            MICROSOFT_GRAPH_CONFLICT_BEHAVIOR,
            ONEDRIVE_CREATE_CONFLICT_BEHAVIOR,
        );
    }
    Ok(url)
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

fn onedrive_value_error(
    status: StatusCode,
    value: &Value,
    code_prefix: &str,
    fallback: &str,
    response_request_id: Option<&str>,
) -> anyhow::Error {
    if onedrive_conflict_error(status, value) {
        let message = onedrive_error_message(value).unwrap_or("OneDrive object changed");
        return anyhow::anyhow!("etag_conflict_detected: {message}");
    }
    let status_code = status.as_u16();
    let message = onedrive_error_message(value).unwrap_or(fallback);
    let graph_code = onedrive_error_code(value).unwrap_or_default();
    let code = match status {
        StatusCode::BAD_REQUEST if onedrive_scope_or_permission_error(&graph_code, message) => {
            "onedrive_missing_scope".to_string()
        }
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
    let request_id = response_request_id.or_else(|| onedrive_error_request_id(value));
    let graph_code = if graph_code.is_empty() {
        "unknown"
    } else {
        graph_code.as_str()
    };
    let request_id = request_id.unwrap_or("unavailable");
    anyhow::anyhow!(
        "{code}: {message} [operation={code_prefix}, status={status_code}, graph_code={graph_code}, request_id={request_id}]"
    )
}

fn onedrive_conflict_error(status: StatusCode, value: &Value) -> bool {
    status == StatusCode::PRECONDITION_FAILED
        || (status == StatusCode::CONFLICT
            && onedrive_error_code(value).as_deref() == Some("nameAlreadyExists"))
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

fn onedrive_error_request_id(value: &Value) -> Option<&str> {
    let error = value.get("error")?;
    let inner_error = error
        .get("innerError")
        .or_else(|| error.get("innererror"))?;
    inner_error
        .get("request-id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn onedrive_response_request_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get(MICROSOFT_GRAPH_REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn onedrive_scope_or_permission_error(graph_code: &str, message: &str) -> bool {
    // Graph does not always use a stable status/code pair for app-folder
    // permission failures, so classify by both machine code and safe text.
    let graph_code = graph_code.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    matches!(
        graph_code.as_str(),
        "invalidscope" | "authorization_requestdenied"
    ) || message.contains("files.readwrite.appfolder")
        || message.contains("insufficient privileges")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn onedrive_metadata_create_uses_graph_conflict_behavior() {
        let settings = CloudSyncSettings {
            backend_type: BackendType::OneDrive,
            ..CloudSyncSettings::default()
        };

        let create_url = onedrive_upload_url(&settings, "metadata.json", true).unwrap();
        let replace_url = onedrive_upload_url(&settings, "metadata.json", false).unwrap();

        assert_eq!(
            create_url
                .query_pairs()
                .find(|(key, _)| key == MICROSOFT_GRAPH_CONFLICT_BEHAVIOR)
                .map(|(_, value)| value.into_owned()),
            Some(ONEDRIVE_CREATE_CONFLICT_BEHAVIOR.to_string())
        );
        assert!(replace_url.query().is_none());
    }

    #[test]
    fn onedrive_error_mapping_distinguishes_scope_rate_and_conflict() {
        let scope_error = onedrive_value_error(
            StatusCode::FORBIDDEN,
            &json!({ "error": { "message": "Missing Files.ReadWrite.AppFolder" } }),
            "onedrive",
            "fallback",
            None,
        )
        .to_string();
        let rate_error = onedrive_value_error(
            StatusCode::TOO_MANY_REQUESTS,
            &json!({ "error": { "message": "Too many requests" } }),
            "onedrive",
            "fallback",
            None,
        )
        .to_string();
        let conflict_error = onedrive_value_error(
            StatusCode::PRECONDITION_FAILED,
            &json!({ "error": { "message": "ETag changed" } }),
            "onedrive",
            "fallback",
            None,
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
            None,
        )
        .to_string();
        let bad_request_error = onedrive_value_error(
            StatusCode::BAD_REQUEST,
            &json!({ "error": { "message": "Invalid app folder request" } }),
            "onedrive",
            "fallback",
            None,
        )
        .to_string();
        let locked_error = onedrive_value_error(
            StatusCode::from_u16(423).unwrap(),
            &json!({ "error": { "message": "Resource is locked" } }),
            "onedrive",
            "fallback",
            None,
        )
        .to_string();
        let service_error = onedrive_value_error(
            StatusCode::SERVICE_UNAVAILABLE,
            &json!({ "error": { "message": "Service unavailable" } }),
            "onedrive",
            "fallback",
            None,
        )
        .to_string();

        assert!(access_error.starts_with("onedrive_access_denied:"));
        assert!(bad_request_error.starts_with("onedrive_bad_request:"));
        assert!(locked_error.starts_with("onedrive_locked:"));
        assert!(service_error.starts_with("onedrive_service_unavailable:"));
    }

    #[test]
    fn onedrive_error_keeps_safe_graph_diagnostics() {
        let mut headers = HeaderMap::new();
        headers.insert(
            MICROSOFT_GRAPH_REQUEST_ID_HEADER,
            HeaderValue::from_static("request-123"),
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer representative-secret"),
        );
        let request_id = onedrive_response_request_id(&headers);

        let error = onedrive_value_error(
            StatusCode::BAD_REQUEST,
            &json!({ "error": { "code": "badRequest", "message": "Invalid request" } }),
            "onedrive_metadata_upload",
            "fallback",
            request_id.as_deref(),
        )
        .to_string();

        assert!(error.contains("operation=onedrive_metadata_upload"));
        assert!(error.contains("status=400"));
        assert!(error.contains("graph_code=badRequest"));
        assert!(error.contains("request_id=request-123"));
        assert!(!error.contains("representative-secret"));
    }

    #[test]
    fn onedrive_scope_mapping_ignores_ambiguous_permission_words() {
        let ambiguous = onedrive_value_error(
            StatusCode::FORBIDDEN,
            &json!({
                "error": {
                    "code": "accessDenied",
                    "message": "The permission state could not be evaluated"
                }
            }),
            "onedrive_object_upload",
            "fallback",
            None,
        )
        .to_string();
        let explicit = onedrive_value_error(
            StatusCode::FORBIDDEN,
            &json!({
                "error": {
                    "code": "Authorization_RequestDenied",
                    "message": "Request denied"
                }
            }),
            "onedrive_object_upload",
            "fallback",
            None,
        )
        .to_string();

        assert!(ambiguous.starts_with("onedrive_access_denied:"));
        assert!(explicit.starts_with("onedrive_missing_scope:"));
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
}
