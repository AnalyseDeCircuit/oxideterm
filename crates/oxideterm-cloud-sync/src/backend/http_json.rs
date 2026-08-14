// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! HTTP JSON provider request construction, authentication, parsing, and errors.

use super::*;

impl CloudSyncBackend {
    pub(super) async fn fetch_http_json_metadata(
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
                .headers(self.http_json_auth_headers(config, secrets)?),
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

    pub(super) async fn upload_http_json_snapshot(
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
        let mut headers = self.http_json_auth_headers(config, secrets)?;
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

    pub(super) async fn write_http_json_object(
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
        let mut headers = self.http_json_auth_headers(config, secrets)?;
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

    pub(super) async fn read_http_json_object(
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
                .headers(self.http_json_auth_headers(config, secrets)?),
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

    pub(super) async fn write_http_json_metadata(
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
        let mut headers = self.http_json_auth_headers(config, secrets)?;
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let response = execute_cloud_request(
            self.client
                .put(url)
                .headers(headers)
                .body(serde_json::to_vec(metadata)?),
        )
        .await?;
        let status = response.status();
        let response_etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let value = response.json::<Value>().await.unwrap_or(Value::Null);
        if !status.is_success() || value.get("ok").and_then(Value::as_bool) == Some(false) {
            return Err(http_json_value_error(
                status,
                &value,
                "http_meta",
                "Failed to write metadata",
            ));
        }
        Ok(RemoteWriteResult {
            revision: value
                .get("revision")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            // The official server returns the metadata ETag in JSON; proxies may expose it as a header instead.
            etag: value
                .get("etag")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or(response_etag),
        })
    }

    pub(super) async fn download_http_json_snapshot_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteObject> {
        let url = join_url(
            &config.endpoint,
            &format!("v1/namespaces/{}/blob", encode_component(&config.namespace)),
        );
        let response = execute_cloud_request(
            self.client
                .get(url)
                .headers(self.http_json_auth_headers(config, secrets)?),
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
        response_to_object(response, "HTTP JSON blob").await
    }

    fn http_json_auth_headers(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<HeaderMap> {
        provider_http_auth_headers(config, secrets)
    }
}

async fn http_json_error(
    response: HttpResponseSnapshot,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_json_error_uses_remote_payload_or_local_fallback() {
        let cases = [
            (
                StatusCode::UNAUTHORIZED,
                Value::Null,
                "Failed to fetch remote metadata",
                "http_401: Failed to fetch remote metadata (401)",
            ),
            (
                StatusCode::PRECONDITION_FAILED,
                json!({
                    "error": {
                        "code": "etag_conflict_detected",
                        "message": "Remote changed"
                    }
                }),
                "Failed to upload snapshot",
                "etag_conflict_detected: Remote changed",
            ),
        ];

        for (status, payload, fallback, expected) in cases {
            assert_eq!(
                http_json_value_error(status, &payload, "http", fallback).to_string(),
                expected
            );
        }
    }
}
