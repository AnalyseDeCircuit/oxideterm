// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Git repository provider request construction, authentication, parsing, and errors.

use super::*;
mod publications;

const DEFAULT_GIT_API_ENDPOINT: &str = "https://api.github.com";

impl CloudSyncBackend {
    pub(super) async fn fetch_git_metadata(
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

    pub(super) async fn upload_git_snapshot(
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

    pub(super) async fn write_git_object(
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

    pub(super) async fn read_git_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        self.fetch_git_raw_file(config, secrets, &git_object_path(config, relative_path))
            .await
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

    pub(super) async fn download_git_snapshot_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        metadata: &RemoteMetadata,
    ) -> Result<RemoteObject> {
        let path = metadata
            .blob_path
            .as_deref()
            .unwrap_or(&git_paths(config).blob_path)
            .to_string();
        self.fetch_git_raw_file(config, secrets, &path)
            .await?
            .ok_or_else(|| anyhow::anyhow!("remote_not_found: no remote Git snapshot found"))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitContentFile {
    sha: Option<String>,
    encoding: Option<String>,
    #[serde(default)]
    content: String,
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
}
