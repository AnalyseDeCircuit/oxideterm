// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! WebDAV provider request construction, authentication, parsing, and errors.

use super::*;

impl CloudSyncBackend {
    pub(super) async fn fetch_webdav_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        require_endpoint(config)?;
        let response = execute_cloud_request(
            self.client
                .get(join_url(&webdav_namespace_url(config), "latest.json"))
                .headers(self.webdav_auth_headers(config, secrets)?),
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

    pub(super) async fn upload_webdav_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        self.ensure_webdav_namespace(config, secrets).await?;
        let namespace = webdav_namespace_url(config);
        let mut blob_headers = self.webdav_auth_headers(config, secrets)?;
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

    pub(super) async fn write_webdav_object(
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
        let mut headers = self.webdav_auth_headers(config, secrets)?;
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

    pub(super) async fn read_webdav_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        self.read_object_response(
            execute_cloud_request(
                self.client
                    .get(webdav_object_url(config, relative_path))
                    .headers(self.webdav_auth_headers(config, secrets)?),
            )
            .await?,
            "webdav_object",
            &format!("WebDAV object {relative_path}"),
        )
        .await
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
        let headers = self.webdav_auth_headers(config, secrets)?;
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
    ) -> Result<HttpResponseSnapshot> {
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

    pub(super) async fn download_webdav_snapshot_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteObject> {
        let response = execute_cloud_request(
            self.client
                .get(join_url(&webdav_namespace_url(config), "latest.oxide"))
                .headers(self.webdav_auth_headers(config, secrets)?),
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
        response_to_object(response, "WebDAV blob").await
    }

    async fn read_object_response(
        &self,
        response: HttpResponseSnapshot,
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

    fn webdav_auth_headers(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<HeaderMap> {
        provider_http_auth_headers(config, secrets)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
