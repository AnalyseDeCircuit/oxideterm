// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native updater compatibility layer.
//!
//! OxideTerm's Tauri build reads release manifests through
//! `tauri-plugin-updater`. The GPUI/native build cannot rely on that runtime,
//! so this crate owns the shared channel endpoints, manifest parsing, platform
//! package selection, version comparison, and package download plan. UI code
//! should only trigger these operations and render their results.

mod channel;
mod download;
mod manifest;
mod platform;
mod version;

pub use channel::{
    BETA_UPDATE_ENDPOINT, GPUI_PREVIEW_UPDATE_ENDPOINT, STABLE_UPDATE_ENDPOINT, UpdateEndpoint,
    endpoint_for_channel,
};
pub use download::{
    DownloadProgress, NativeUpdateClient, NativeUpdateDownload, NativeUpdateError,
    NativeUpdateRequest, NativeUpdateStatus,
};
pub use manifest::{NativeUpdateAsset, NativeUpdateManifest, NativeUpdatePackage};
pub use platform::{PlatformTarget, current_platform_target};
pub use version::{VersionOrdering, compare_versions, is_update_newer};
