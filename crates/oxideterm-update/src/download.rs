// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};

use futures_util::StreamExt as _;
use oxideterm_settings::UpdateChannel;
use sha2::{Digest, Sha256};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::{
    NativeUpdateManifest, NativeUpdatePackage, PlatformTarget, current_platform_target,
    endpoint_for_channel,
};

#[derive(Debug, thiserror::Error)]
pub enum NativeUpdateError {
    #[error("Failed to build update HTTP client: {0}")]
    Client(#[source] reqwest::Error),

    #[error("Failed to fetch update manifest: {0}")]
    ManifestFetch(#[source] reqwest::Error),

    #[error("Update manifest returned HTTP {status}: {url}")]
    ManifestStatus {
        status: reqwest::StatusCode,
        url: String,
    },

    #[error("Failed to parse update manifest: {0}")]
    ManifestJson(#[source] serde_json::Error),

    #[error("No update package found for platform {os}/{arch}")]
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },

    #[error("Failed to download update package: {0}")]
    PackageFetch(#[source] reqwest::Error),

    #[error("Update package returned HTTP {status}: {url}")]
    PackageStatus {
        status: reqwest::StatusCode,
        url: String,
    },

    #[error("Failed to create update directory {path}: {source}")]
    CreateUpdateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write update package {path}: {source}")]
    WritePackage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeUpdateRequest {
    pub channel: UpdateChannel,
    pub current_version: String,
    pub target: PlatformTarget,
}

impl NativeUpdateRequest {
    pub fn current(channel: UpdateChannel, current_version: impl Into<String>) -> Self {
        Self {
            channel,
            current_version: current_version.into(),
            target: current_platform_target(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeUpdateStatus {
    UpToDate,
    Available(NativeUpdatePackage),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeUpdateDownload {
    pub package: NativeUpdatePackage,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Clone)]
pub struct NativeUpdateClient {
    http: reqwest::Client,
}

impl NativeUpdateClient {
    pub fn new() -> Result<Self, NativeUpdateError> {
        let http = reqwest::Client::builder()
            .user_agent(format!("OxideTerm/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(NativeUpdateError::Client)?;
        Ok(Self { http })
    }

    pub async fn check(
        &self,
        request: NativeUpdateRequest,
    ) -> Result<NativeUpdateStatus, NativeUpdateError> {
        let endpoint = endpoint_for_channel(request.channel);
        let response = self
            .http
            .get(endpoint.url)
            .send()
            .await
            .map_err(NativeUpdateError::ManifestFetch)?;
        if !response.status().is_success() {
            return Err(NativeUpdateError::ManifestStatus {
                status: response.status(),
                url: endpoint.url.to_string(),
            });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(NativeUpdateError::ManifestFetch)?;
        let manifest: NativeUpdateManifest =
            serde_json::from_slice(&bytes).map_err(NativeUpdateError::ManifestJson)?;

        match manifest.select_package(&request.current_version, &request.target) {
            Some(package) => Ok(NativeUpdateStatus::Available(package)),
            None if manifest.platforms.is_empty() => Ok(NativeUpdateStatus::UpToDate),
            None if crate::is_update_newer(&manifest.version, &request.current_version) => {
                Err(NativeUpdateError::UnsupportedPlatform {
                    os: request.target.os(),
                    arch: request.target.arch(),
                })
            }
            None => Ok(NativeUpdateStatus::UpToDate),
        }
    }

    pub async fn download_package<F>(
        &self,
        package: NativeUpdatePackage,
        directory: &Path,
        mut progress: F,
    ) -> Result<NativeUpdateDownload, NativeUpdateError>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        tokio::fs::create_dir_all(directory)
            .await
            .map_err(|source| NativeUpdateError::CreateUpdateDir {
                path: directory.to_path_buf(),
                source,
            })?;

        let response = self
            .http
            .get(&package.url)
            .send()
            .await
            .map_err(NativeUpdateError::PackageFetch)?;
        if !response.status().is_success() {
            return Err(NativeUpdateError::PackageStatus {
                status: response.status(),
                url: package.url.clone(),
            });
        }

        let total_bytes = response.content_length();
        let path = directory.join(package_file_name(&package));
        let mut file =
            File::create(&path)
                .await
                .map_err(|source| NativeUpdateError::WritePackage {
                    path: path.clone(),
                    source,
                })?;
        let mut stream = response.bytes_stream();
        let mut downloaded_bytes = 0_u64;
        let mut hasher = Sha256::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(NativeUpdateError::PackageFetch)?;
            downloaded_bytes += chunk.len() as u64;
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|source| NativeUpdateError::WritePackage {
                    path: path.clone(),
                    source,
                })?;
            progress(DownloadProgress {
                downloaded_bytes,
                total_bytes,
            });
        }

        file.flush()
            .await
            .map_err(|source| NativeUpdateError::WritePackage {
                path: path.clone(),
                source,
            })?;

        Ok(NativeUpdateDownload {
            package,
            path,
            bytes: downloaded_bytes,
            sha256: format!("{:x}", hasher.finalize()),
        })
    }
}

fn package_file_name(package: &NativeUpdatePackage) -> String {
    let source_name = package
        .url
        .rsplit('/')
        .next()
        .map(|name| name.split(['?', '#']).next().unwrap_or(name))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("oxideterm-update");
    let sanitized = source_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}-{}", package.version, sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_file_name_keeps_version_and_removes_path_unsafe_chars() {
        let name = package_file_name(&NativeUpdatePackage {
            version: "1.2.0-gpui-preview.1".into(),
            current_version: "1.2.0-gpui-preview.0".into(),
            body: None,
            date: None,
            platform_key: "darwin-aarch64".into(),
            url: "https://example.invalid/download/OxideTerm Preview.dmg?token=secret".into(),
            signature: None,
        });

        assert!(name.starts_with("1.2.0-gpui-preview.1-"));
        assert!(!name.contains('/'));
        assert!(!name.contains('?'));
    }
}
