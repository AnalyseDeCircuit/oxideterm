// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Dropbox provider request construction, authentication, parsing, and errors.

use super::*;
mod publications;

const DROPBOX_API_BASE: &str = "https://api.dropboxapi.com/2";
const DROPBOX_CONTENT_BASE: &str = "https://content.dropboxapi.com/2";

impl CloudSyncBackend {
    pub(super) async fn fetch_dropbox_metadata(
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

    pub(super) async fn upload_dropbox_snapshot(
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

    pub(super) async fn write_dropbox_object(
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

    pub(super) async fn read_dropbox_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        self.download_dropbox_file(&dropbox_object_path(config, relative_path), secrets)
            .await
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

    pub(super) async fn download_dropbox_snapshot_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteObject> {
        let paths = dropbox_paths(config);
        self.download_dropbox_file(&paths.blob_path, secrets)
            .await?
            .ok_or_else(|| anyhow::anyhow!("remote_not_found: no remote Dropbox snapshot found"))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropbox_paths_are_namespace_scoped() {
        let settings = CloudSyncSettings {
            backend_type: BackendType::Dropbox,
            namespace: "team/default".to_string(),
            ..CloudSyncSettings::default()
        };
        let paths = dropbox_paths(&settings);

        assert_eq!(paths.namespace_path, "/team/default");
        assert_eq!(paths.metadata_path, "/team/default/latest.json");
        assert_eq!(paths.blob_path, "/team/default/latest.oxide");
    }
}
