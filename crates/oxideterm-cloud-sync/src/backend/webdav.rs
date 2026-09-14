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
        let endpoint = trim_trailing_slash(&config.endpoint);
        if webdav_namespace_url(config) == endpoint {
            // A supplied WebDAV endpoint is an existing collection, not ours to create.
            return Ok(());
        }
        self.ensure_webdav_child_collections(&endpoint, &config.namespace, config, secrets)
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
        self.ensure_webdav_child_collections(
            &webdav_namespace_url(config),
            &parent,
            config,
            secrets,
        )
        .await
    }

    async fn ensure_webdav_child_collections(
        &self,
        base: &str,
        relative_path: &str,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<()> {
        let mut url = trim_trailing_slash(base);
        // MKCOL cannot create missing ancestors. Stay below the configured base
        // and create parents first instead of retrying from the server root.
        for segment in relative_path
            .split('/')
            .filter(|segment| !segment.is_empty())
        {
            if matches!(segment, "." | "..") {
                bail!("namespace_create_failed: Invalid relative WebDAV collection path");
            }
            url = join_url(&url, &encode_component(segment));
            self.ensure_webdav_collection(&url, config, secrets).await?;
        }
        Ok(())
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
        if response.status() == StatusCode::CONFLICT
            && self
                .webdav_collection_exists(url, headers.clone())
                .await
                .unwrap_or(false)
        {
            return Ok(());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, sync::Mutex};

    struct ScopedWebdav {
        collections: Mutex<HashSet<String>>,
        requests: Mutex<Vec<(String, String)>>,
    }

    impl HttpExecutor for ScopedWebdav {
        fn execute(&self, request: HttpRequestSpec) -> HttpExecuteFuture<'_> {
            Box::pin(async move {
                let path = request.url.path().to_string();
                self.requests
                    .lock()
                    .unwrap()
                    .push((request.method.to_string(), path.clone()));
                let mut collections = self.collections.lock().unwrap();
                let parent = path
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .unwrap_or("");
                let status = match request.method.as_str() {
                    "MKCOL" if path == "/dav" || path == "/dav/forbidden" => StatusCode::FORBIDDEN,
                    "MKCOL" if collections.contains(&path) => StatusCode::METHOD_NOT_ALLOWED,
                    "MKCOL" if !collections.contains(parent) => StatusCode::CONFLICT,
                    "MKCOL" => {
                        collections.insert(path);
                        StatusCode::CREATED
                    }
                    "PROPFIND" if collections.contains(&path) => StatusCode::MULTI_STATUS,
                    "PROPFIND" => StatusCode::NOT_FOUND,
                    "PUT" if collections.contains(parent) => {
                        let HttpRequestBody::Bytes(bytes) = request.body else {
                            panic!("expected object bytes")
                        };
                        assert_eq!(bytes.as_slice(), b"connection snapshot");
                        StatusCode::CREATED
                    }
                    _ => StatusCode::FORBIDDEN,
                };
                Ok(HttpResponseSnapshot::new(
                    status,
                    HeaderMap::new(),
                    Vec::new(),
                ))
            })
        }
    }

    #[tokio::test]
    async fn webdav_upload_creates_nested_collections_without_touching_service_root() {
        let executor = Arc::new(ScopedWebdav {
            collections: Mutex::new(
                ["/dav".into(), "/dav/oxideterm-sync".into()]
                    .into_iter()
                    .collect(),
            ),
            requests: Mutex::new(Vec::new()),
        });
        let backend = CloudSyncBackend::with_http_executor(executor.clone());
        let config = CloudSyncSettings {
            backend_type: BackendType::Webdav,
            auth_mode: crate::AuthMode::None,
            endpoint: "https://example.test/dav".into(),
            namespace: "oxideterm-sync".into(),
            ..Default::default()
        };
        backend
            .write_remote_object(
                &config,
                &CloudSyncSecrets::default(),
                "structured/connections/revision.json",
                b"connection snapshot".to_vec(),
                Some("application/json"),
            )
            .await
            .expect("nested upload should work when MKCOL on the service root is forbidden");
        assert_eq!(
            *executor.requests.lock().unwrap(),
            [
                ("MKCOL".into(), "/dav/oxideterm-sync".into()),
                ("MKCOL".into(), "/dav/oxideterm-sync/structured".into()),
                (
                    "MKCOL".into(),
                    "/dav/oxideterm-sync/structured/connections".into()
                ),
                (
                    "PUT".into(),
                    "/dav/oxideterm-sync/structured/connections/revision.json".into()
                ),
            ]
        );
    }

    #[tokio::test]
    async fn webdav_namespace_respects_endpoint_boundary() {
        for (endpoint, namespace, expected_paths) in [
            (
                "https://example.test/dav",
                "team/nested",
                vec!["/dav/team", "/dav/team/nested"],
            ),
            ("https://example.test/dav", "", vec![]),
            ("https://dav.jianguoyun.com/dav/team", "team", vec![]),
        ] {
            let executor = Arc::new(ScopedWebdav {
                collections: Mutex::new(["/dav".into(), "/dav/team".into()].into_iter().collect()),
                requests: Mutex::new(Vec::new()),
            });
            let backend = CloudSyncBackend::with_http_executor(executor.clone());
            let config = CloudSyncSettings {
                backend_type: BackendType::Webdav,
                auth_mode: crate::AuthMode::None,
                endpoint: endpoint.into(),
                namespace: namespace.into(),
                ..Default::default()
            };
            backend
                .ensure_webdav_namespace(&config, &CloudSyncSecrets::default())
                .await
                .unwrap();
            assert_eq!(
                *executor.requests.lock().unwrap(),
                expected_paths
                    .into_iter()
                    .map(|path| ("MKCOL".into(), path.to_string()))
                    .collect::<Vec<_>>(),
                "{endpoint}, {namespace}"
            );
        }
    }

    #[tokio::test]
    async fn webdav_upload_stops_on_real_namespace_permission_error() {
        let executor = Arc::new(ScopedWebdav {
            collections: Mutex::new(["/dav".into()].into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        });
        let backend = CloudSyncBackend::with_http_executor(executor.clone());
        let config = CloudSyncSettings {
            backend_type: BackendType::Webdav,
            auth_mode: crate::AuthMode::None,
            endpoint: "https://example.test/dav".into(),
            namespace: "forbidden".into(),
            ..Default::default()
        };
        let error = backend
            .write_remote_object(
                &config,
                &CloudSyncSecrets::default(),
                "structured/connections/revision.json",
                b"connection snapshot".to_vec(),
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "namespace_create_failed: Failed to prepare WebDAV namespace (403)"
        );
        assert_eq!(
            *executor.requests.lock().unwrap(),
            [("MKCOL".into(), "/dav/forbidden".into())]
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
}
