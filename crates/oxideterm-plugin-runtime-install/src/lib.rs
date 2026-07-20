// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Downloading and safely installing the optional native Wasm runtime sidecar.
//!
//! This crate owns release delivery and filesystem installation. It deliberately
//! has no GPUI dependency and does not own Wasm execution.

mod archive;
mod compatibility;
mod http;
mod release;
mod target;

use std::path::{Path, PathBuf};

use archive::install_runtime_archive;
use compatibility::validate_runtime_index;
use http::download_asset_bytes;
use release::GithubRelease;

const WASM_RUNTIME_SIDECAR_LATEST_API_URL: &str =
    "https://api.github.com/repos/AnalyseDeCircuit/oxideterm-wasm-runtime/releases/latest";
const WASM_RUNTIME_SIDECAR_MAX_BYTES: u64 = 256 * 1024 * 1024;
const WASM_RUNTIME_HTTP_TIMEOUT_SECONDS: u64 = 180;
const WASM_RUNTIME_INSTALL_ROOT: &str = "native-runtimes";
const WASM_RUNTIME_INSTALL_DIRECTORY: &str = "wasm";

/// The installed runtime version and absolute executable path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WasmRuntimeInstallResult {
    pub version: String,
    pub path: PathBuf,
}

/// Downloads the latest compatible Wasm runtime sidecar and replaces its local install atomically.
pub async fn install_wasm_runtime_sidecar(
    settings_path: &Path,
) -> Result<WasmRuntimeInstallResult, String> {
    let client = oxideterm_network_proxy::application_http_client_builder()
        .map_err(|error| format!("Failed to apply application proxy: {error}"))?
        .timeout(std::time::Duration::from_secs(
            WASM_RUNTIME_HTTP_TIMEOUT_SECONDS,
        ))
        .user_agent(format!("OxideTerm/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("Failed to create HTTP client: {error}"))?;
    let release = fetch_wasm_runtime_latest_release(&client).await?;
    let runtime_index_asset = release
        .asset_named("runtime-index.json")
        .ok_or_else(|| "Wasm runtime release does not provide runtime-index.json".to_string())?;
    let runtime_index =
        download_text_asset(&client, &runtime_index_asset.browser_download_url).await?;
    validate_runtime_index(&runtime_index, env!("CARGO_PKG_VERSION"))?;

    let target = target::wasm_runtime_target_triple()?;
    let runtime_asset = release.select_runtime_asset(target)?;
    let checksums_asset = release
        .asset_named("SHA256SUMS")
        .ok_or_else(|| "Wasm runtime release does not provide SHA256SUMS".to_string())?;
    let checksums = download_text_asset(&client, &checksums_asset.browser_download_url).await?;
    let expected_sha256 = release::checksum_for_asset(&checksums, &runtime_asset.name)?;
    let archive = download_asset_bytes(
        &client,
        &runtime_asset.browser_download_url,
        WASM_RUNTIME_SIDECAR_MAX_BYTES,
    )
    .await?;
    release::verify_sha256(&archive, &expected_sha256, &runtime_asset.name)?;

    let install_dir = wasm_sidecar_install_dir(settings_path);
    let path = install_runtime_archive(
        &archive,
        &runtime_asset.name,
        &install_dir,
        target::wasm_runtime_binary_name(),
    )?;
    Ok(WasmRuntimeInstallResult {
        version: release.version(),
        path,
    })
}

async fn fetch_wasm_runtime_latest_release(
    client: &reqwest::Client,
) -> Result<GithubRelease, String> {
    let response = client
        .get(WASM_RUNTIME_SIDECAR_LATEST_API_URL)
        .send()
        .await
        .map_err(|error| format!("Failed to fetch Wasm runtime release: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Wasm runtime release API returned HTTP {}",
            response.status().as_u16()
        ));
    }
    response
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("Failed to parse Wasm runtime release: {error}"))
}

async fn download_text_asset(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let bytes = download_asset_bytes(client, url, WASM_RUNTIME_SIDECAR_MAX_BYTES).await?;
    String::from_utf8(bytes)
        .map_err(|error| format!("Downloaded runtime asset is not UTF-8: {error}"))
}

fn wasm_sidecar_install_dir(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(WASM_RUNTIME_INSTALL_ROOT)
        .join(WASM_RUNTIME_INSTALL_DIRECTORY)
}
