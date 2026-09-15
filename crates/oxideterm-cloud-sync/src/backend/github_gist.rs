// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GitHub Gist provider request construction, authentication, parsing, and errors.

use super::*;

const DEFAULT_GIT_API_ENDPOINT: &str = "https://api.github.com";
const GITHUB_API_VERSION: &str = "2022-11-28";
const GIST_OBJECT_PREFIX: &str = "OXIDETERM-GIST-BLOB-V1\n";

impl CloudSyncBackend {
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

    pub(super) async fn fetch_gist_metadata(
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

    pub(super) async fn upload_gist_snapshot(
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

    pub(super) async fn write_gist_object(
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

    pub(super) async fn write_gist_metadata(
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

    pub(super) async fn read_gist_object(
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
        response.text().await
    }

    pub(super) async fn download_gist_snapshot_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &RemoteMetadata,
    ) -> Result<RemoteObject> {
        let path = metadata
            .blob_path
            .as_deref()
            .unwrap_or(&gist_paths(config).blob_path)
            .to_string();
        self.read_gist_object(config, secrets, &path)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("remote_not_found: no remote GitHub Gist snapshot found")
            })
    }
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
            .rfind(|part| !part.is_empty())
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

fn github_rate_limit_exceeded(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("rate limit") || lower.contains("secondary rate limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gist_settings(gist_id: &str) -> CloudSyncSettings {
        CloudSyncSettings {
            backend_type: BackendType::GithubGist,
            git_repository: gist_id.to_string(),
            namespace: "team/default".to_string(),
            ..CloudSyncSettings::default()
        }
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
}
