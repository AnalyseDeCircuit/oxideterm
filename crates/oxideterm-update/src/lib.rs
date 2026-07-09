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
mod install;
mod integrity;
mod manifest;
mod platform;
mod state;
mod version;
mod windows_update_helper;

pub use channel::{
    BETA_UPDATE_ENDPOINT, GPUI_PREVIEW_UPDATE_ENDPOINT, STABLE_UPDATE_ENDPOINT, UpdateEndpoint,
    endpoint_for_channel,
};
pub use download::{
    DownloadProgress, NativeUpdateClient, NativeUpdateDownload, NativeUpdateError,
    NativeUpdateRequest, NativeUpdateStatus, prune_resumable_update_cache,
};
pub use install::{
    InstallActionKind, InstallPackageKind, InstallStrategy, NativeInstallContext,
    NativeInstallOutcome, NativeInstallPlan, NativeInstallStatus, execute_install_plan,
    plan_native_install,
};
pub use manifest::{NativeUpdateAsset, NativeUpdateManifest, NativeUpdatePackage};
pub use platform::{InstallFlavor, PlatformTarget, current_platform_target};
pub use state::{
    NativeUpdateStage, PersistedUpdateState, ResumableUpdateStatus, TauriUpdaterEvent,
};
pub use version::{VersionOrdering, compare_versions, is_update_newer};
pub use windows_update_helper::{
    WINDOWS_UPDATE_HELPER_RELATIVE, WINDOWS_UPDATE_OLD_DIR, WINDOWS_UPDATE_STAGING_DIR,
    WindowsUpdateHelperOptions, apply_staged_windows_update, confirm_applied_windows_update,
    parse_windows_update_helper_options, run_windows_update_helper,
    windows_update_helper_arguments, windows_update_helper_path,
};
