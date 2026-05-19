// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use anyhow::{Context, Result, bail};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{
    Method, StatusCode, Url,
    header::{AUTHORIZATION, CONTENT_TYPE, ETAG, HeaderMap, HeaderValue},
};
use serde_json::Value;
use sha2::Sha256;

use super::{
    CloudSyncBackend, RemoteMetadata, RemoteObject, RemoteSnapshotUpload, RemoteWriteResult,
    assert_snapshot_size, digest_hex, encode_component, encode_path_segments,
    execute_cloud_request, insert_header, normalize_remote_metadata, response_to_object,
    response_write_result, trim_slashes,
};
use crate::{CloudSyncSettings, OXIDE_CONTENT_TYPE, secrets::CloudSyncSecrets};

type HmacSha256 = Hmac<Sha256>;

impl CloudSyncBackend {
    pub(super) async fn fetch_s3_metadata(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
    ) -> Result<RemoteMetadata> {
        let response = self
            .s3_request(
                Method::GET,
                &s3_metadata_url(config)?,
                config,
                secrets,
                None,
                HeaderMap::new(),
            )
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(RemoteMetadata::missing());
        }
        if !response.status().is_success() {
            bail!("s3_{}: failed to fetch S3 metadata", response.status());
        }
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let mut metadata = normalize_remote_metadata(response.json::<Value>().await?, etag)?;
        metadata.blob_key.get_or_insert(s3_paths(config).blob_key);
        Ok(metadata)
    }

    pub(super) async fn upload_s3_snapshot(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        payload: RemoteSnapshotUpload,
    ) -> Result<RemoteWriteResult> {
        let blob_key = s3_revision_blob_key(config, &payload.revision);
        let mut blob_headers = HeaderMap::new();
        blob_headers.insert(CONTENT_TYPE, HeaderValue::from_static(OXIDE_CONTENT_TYPE));
        let blob_response = self
            .s3_request(
                Method::PUT,
                &s3_blob_url(config, &blob_key)?,
                config,
                secrets,
                Some(payload.bytes.clone()),
                blob_headers,
            )
            .await?;
        if !blob_response.status().is_success() {
            bail!(
                "s3_blob_{}: failed to upload S3 blob",
                blob_response.status()
            );
        }

        let mut metadata = payload.metadata_json_with_blob_path(&blob_key);
        metadata["namespace"] = Value::String(config.namespace.clone());
        metadata["blobKey"] = Value::String(blob_key);
        let mut metadata_headers = HeaderMap::new();
        metadata_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(previous_etag) = payload.previous_etag.as_deref() {
            insert_header(&mut metadata_headers, "If-Match", previous_etag)?;
        } else {
            insert_header(&mut metadata_headers, "If-None-Match", "*")?;
        }
        let metadata_response = self
            .s3_request(
                Method::PUT,
                &s3_metadata_url(config)?,
                config,
                secrets,
                Some(serde_json::to_vec(&metadata)?),
                metadata_headers,
            )
            .await?;
        if metadata_response.status() == StatusCode::PRECONDITION_FAILED {
            bail!("etag_conflict_detected: remote S3 snapshot changed before upload completed");
        }
        if !metadata_response.status().is_success() {
            bail!(
                "s3_meta_{}: failed to upload S3 metadata",
                metadata_response.status()
            );
        }
        Ok(RemoteWriteResult {
            revision: payload.revision,
            etag: metadata_response
                .headers()
                .get(ETAG)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
                .or(payload.etag),
        })
    }

    pub(super) async fn write_s3_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
        bytes: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<RemoteWriteResult> {
        let mut headers = HeaderMap::new();
        insert_header(
            &mut headers,
            CONTENT_TYPE.as_str(),
            content_type.unwrap_or("application/octet-stream"),
        )?;
        let response = self
            .s3_request(
                Method::PUT,
                &s3_object_url(config, relative_path)?,
                config,
                secrets,
                Some(bytes),
                headers,
            )
            .await?;
        if !response.status().is_success() {
            bail!(
                "s3_object_{}: failed to upload S3 object",
                response.status()
            );
        }
        Ok(response_write_result(response).await)
    }

    pub(super) async fn read_s3_object(
        &self,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        relative_path: &str,
    ) -> Result<Option<RemoteObject>> {
        let response = self
            .s3_request(
                Method::GET,
                &s3_object_url(config, relative_path)?,
                config,
                secrets,
                None,
                HeaderMap::new(),
            )
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            bail!(
                "s3_object_{}: failed to download S3 object",
                response.status()
            );
        }
        let object = response_to_object(response, &format!("S3 object {relative_path}")).await?;
        assert_snapshot_size(
            object.bytes.len() as u64,
            &format!("S3 object {relative_path}"),
        )?;
        Ok(Some(object))
    }

    pub(super) async fn s3_request(
        &self,
        method: Method,
        url: &Url,
        config: &CloudSyncSettings,
        secrets: &CloudSyncSecrets,
        body: Option<Vec<u8>>,
        headers: HeaderMap,
    ) -> Result<reqwest::Response> {
        validate_s3_config(config, secrets)?;
        let bytes = body.unwrap_or_default();
        let signed_headers = s3_signed_headers(
            method.as_str(),
            url,
            &config.s3_region,
            secrets,
            headers,
            &bytes,
        )?;
        let mut request = self
            .client
            .request(method, url.clone())
            .headers(signed_headers);
        if !bytes.is_empty() {
            request = request.body(bytes);
        }
        execute_cloud_request(request).await
    }
}

pub(super) fn s3_paths(config: &CloudSyncSettings) -> crate::SnapshotObjectPaths {
    crate::snapshot_object_paths(&config.namespace)
}

pub(super) fn s3_blob_url(config: &CloudSyncSettings, blob_key: &str) -> Result<Url> {
    join_s3_object_url(&config.endpoint, &config.s3_bucket, blob_key)
}

fn validate_s3_config(config: &CloudSyncSettings, secrets: &CloudSyncSecrets) -> Result<()> {
    if config.endpoint.trim().is_empty() {
        bail!("missing_endpoint: S3 endpoint is not configured");
    }
    if config.s3_bucket.trim().is_empty() {
        bail!("missing_s3_bucket: S3 bucket is not configured");
    }
    if config.s3_region.trim().is_empty() {
        bail!("missing_s3_region: S3 region is not configured");
    }
    if secrets
        .access_key_id
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        bail!("missing_s3_access_key_id: S3 access key ID is not configured");
    }
    if secrets
        .secret_access_key
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        bail!("missing_s3_secret_access_key: S3 secret access key is not configured");
    }
    Ok(())
}

fn s3_metadata_url(config: &CloudSyncSettings) -> Result<Url> {
    join_s3_object_url(
        &config.endpoint,
        &config.s3_bucket,
        &s3_paths(config).metadata_key,
    )
}

fn s3_object_url(config: &CloudSyncSettings, relative_path: &str) -> Result<Url> {
    let prefix = trim_slashes(&config.namespace);
    let relative_path = trim_slashes(relative_path);
    let key = [prefix.as_str(), relative_path.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    join_s3_object_url(&config.endpoint, &config.s3_bucket, &key)
}

fn s3_revision_blob_key(config: &CloudSyncSettings, revision: &str) -> String {
    crate::s3_revision_blob_key(&config.namespace, revision)
}

fn join_s3_object_url(endpoint: &str, bucket: &str, key: &str) -> Result<Url> {
    let mut url = Url::parse(endpoint).context("missing_endpoint: invalid S3 endpoint")?;
    let base_path = trim_slashes(url.path());
    let encoded_key = encode_path_segments(key);
    let path = [
        base_path.as_str(),
        encode_component(bucket).as_str(),
        encoded_key.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("/");
    url.set_path(&format!("/{path}"));
    Ok(url)
}

fn s3_signed_headers(
    method: &str,
    url: &Url,
    region: &str,
    secrets: &CloudSyncSecrets,
    mut headers: HeaderMap,
    body: &[u8],
) -> Result<HeaderMap> {
    let access_key_id = secrets
        .access_key_id
        .as_deref()
        .context("missing_s3_access_key_id: S3 access key ID is not configured")?;
    let secret_access_key = secrets
        .secret_access_key
        .as_deref()
        .context("missing_s3_secret_access_key: S3 secret access key is not configured")?;
    let payload_hash = digest_hex(body);
    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    insert_header(&mut headers, "x-amz-content-sha256", &payload_hash)?;
    insert_header(&mut headers, "x-amz-date", &amz_date)?;
    if let Some(session_token) = secrets
        .session_token
        .as_deref()
        .filter(|token| !token.is_empty())
    {
        insert_header(&mut headers, "x-amz-security-token", session_token)?;
    }

    let mut signing_entries = headers
        .iter()
        .map(|(name, value)| {
            Ok((
                name.as_str().to_ascii_lowercase(),
                value
                    .to_str()?
                    .trim()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    signing_entries.push(("host".to_string(), s3_host_header(url)));
    signing_entries.sort_by(|left, right| left.0.cmp(&right.0));
    let canonical_headers = signing_entries
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join("\n");
    let signed_headers = signing_entries
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_request = [
        method.to_ascii_uppercase(),
        canonicalize_s3_path(url),
        canonicalize_query(url),
        format!("{canonical_headers}\n"),
        signed_headers.clone(),
        payload_hash,
    ]
    .join("\n");
    let credential_scope = format!("{date_stamp}/{region}/s3/aws4_request");
    let string_to_sign = [
        "AWS4-HMAC-SHA256".to_string(),
        amz_date,
        credential_scope.clone(),
        digest_hex(canonical_request.as_bytes()),
    ]
    .join("\n");
    let signing_key = s3_signature_key(secret_access_key, &date_stamp, region)?;
    let signature = hmac_sha256_hex(&signing_key, string_to_sign.as_bytes())?;
    insert_header(
        &mut headers,
        AUTHORIZATION.as_str(),
        &format!(
            "AWS4-HMAC-SHA256 Credential={access_key_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
        ),
    )?;
    Ok(headers)
}

fn s3_signature_key(secret_access_key: &str, date_stamp: &str, region: &str) -> Result<Vec<u8>> {
    let date_key = hmac_sha256(
        format!("AWS4{secret_access_key}").as_bytes(),
        date_stamp.as_bytes(),
    )?;
    let region_key = hmac_sha256(&date_key, region.as_bytes())?;
    let service_key = hmac_sha256(&region_key, b"s3")?;
    hmac_sha256(&service_key, b"aws4_request")
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> Result<String> {
    Ok(bytes_hex(&hmac_sha256(key, message)?))
}

fn bytes_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn canonicalize_s3_path(url: &Url) -> String {
    if url.path().is_empty() {
        "/".to_string()
    } else {
        url.path().to_string()
    }
}

fn canonicalize_query(url: &Url) -> String {
    let mut pairs = url
        .query_pairs()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{}={}", encode_component(&key), encode_component(&value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn s3_host_header(url: &Url) -> String {
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BackendType;

    #[test]
    fn builds_s3_object_urls_like_cloud_sync_plugin() {
        let settings = CloudSyncSettings {
            backend_type: BackendType::S3,
            endpoint: "https://s3.example.test/root/".to_string(),
            s3_bucket: "oxide bucket".to_string(),
            namespace: "team/default".to_string(),
            ..CloudSyncSettings::default()
        };

        assert_eq!(
            s3_metadata_url(&settings).unwrap().as_str(),
            "https://s3.example.test/root/oxide%20bucket/team/default/latest.json"
        );
        assert_eq!(
            s3_object_url(&settings, "structured/connections/rev 1.json")
                .unwrap()
                .as_str(),
            "https://s3.example.test/root/oxide%20bucket/team/default/structured/connections/rev%201.json"
        );
    }
}
