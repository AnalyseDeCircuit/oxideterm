// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Portable mode detection and path helpers.

use fs2::FileExt;
use parking_lot::RwLock;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, OnceLock};
use tauri::Manager;

const PORTABLE_MARKER_FILENAME: &str = "portable";
const PORTABLE_CONFIG_FILENAME: &str = "portable.json";
const PORTABLE_DEFAULT_DATA_DIRNAME: &str = "data";
const PORTABLE_INSTANCE_LOCK_FILENAME: &str = ".portable.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortableActivationKind {
    Disabled,
    Marker,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortableHostKind {
    ExecutableDir,
    MacAppSibling,
    MacAppBundle,
    LinuxAppImageDir,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PortableConfigFile {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default, alias = "data_dir")]
    data_dir: Option<String>,
}

#[derive(Debug, Clone)]
struct PortableCandidate {
    host_kind: PortableHostKind,
    host_dir: PathBuf,
}

#[derive(Debug)]
struct PortableInstanceLock {
    _file: File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortableBootstrapStatus {
    Disabled,
    NeedsSetup,
    Locked,
    Unlocked,
}

impl PortableBootstrapStatus {
    pub fn can_launch_full_app(self) -> bool {
        matches!(self, Self::Disabled | Self::Unlocked)
    }

    pub fn has_keystore(self) -> bool {
        matches!(self, Self::Locked | Self::Unlocked)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableInfo {
    pub is_portable: bool,
    pub activation: PortableActivationKind,
    pub host_kind: PortableHostKind,
    pub exe_dir: PathBuf,
    pub host_dir: PathBuf,
    pub marker_path: PathBuf,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub instance_lock_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum PortableError {
    #[error("Failed to determine current executable path: {0}")]
    CurrentExe(#[source] std::io::Error),

    #[error("Executable path has no parent directory: {0}")]
    MissingExeParent(PathBuf),

    #[error("Failed to get app data dir: {0}")]
    AppDataDir(String),

    #[error("Failed to read portable.json: {0}")]
    PortableConfig(#[source] std::io::Error),

    #[error("Failed to parse portable.json: {0}")]
    PortableConfigJson(#[from] serde_json::Error),

    #[error("portable.json dataDir must be a non-empty relative path without '.' or '..': {0}")]
    InvalidPortableDataDir(String),

    #[error("Portable instance is already running for data dir: {0}")]
    InstanceLocked(PathBuf),

    #[error("Failed to create portable instance lock: {0}")]
    InstanceLockIo(#[source] std::io::Error),

    #[error("Portable keystore error: {0}")]
    Keystore(String),
}

static PORTABLE_INFO: OnceLock<PortableInfo> = OnceLock::new();
static PORTABLE_BOOTSTRAP_STATUS: LazyLock<RwLock<Option<PortableBootstrapStatus>>> =
    LazyLock::new(|| RwLock::new(None));
static PORTABLE_INSTANCE_LOCK: LazyLock<RwLock<Option<PortableInstanceLock>>> =
    LazyLock::new(|| RwLock::new(None));

fn portable_relative_data_dir(value: &str) -> Result<PathBuf, PortableError> {
    let candidate = PathBuf::from(value);
    if candidate.as_os_str().is_empty() || candidate.is_absolute() {
        return Err(PortableError::InvalidPortableDataDir(value.to_string()));
    }

    for component in candidate.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(PortableError::InvalidPortableDataDir(value.to_string()));
        }
    }

    Ok(candidate)
}

#[cfg(target_os = "macos")]
fn macos_bundle_candidates(exe_path: &Path) -> Vec<PortableCandidate> {
    let mut candidates = Vec::new();
    let mut cursor = exe_path.parent();
    while let Some(dir) = cursor {
        if dir.extension().and_then(|ext| ext.to_str()) == Some("app") {
            if let Some(bundle_parent) = dir.parent() {
                candidates.push(PortableCandidate {
                    host_kind: PortableHostKind::MacAppSibling,
                    host_dir: bundle_parent.to_path_buf(),
                });
            }
            candidates.push(PortableCandidate {
                host_kind: PortableHostKind::MacAppBundle,
                host_dir: dir.to_path_buf(),
            });
            break;
        }
        cursor = dir.parent();
    }
    candidates
}

#[cfg(not(target_os = "macos"))]
fn macos_bundle_candidates(_exe_path: &Path) -> Vec<PortableCandidate> {
    Vec::new()
}

fn portable_candidates(
    exe_path: &Path,
    appimage_path: Option<&Path>,
) -> Result<Vec<PortableCandidate>, PortableError> {
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| PortableError::MissingExeParent(exe_path.to_path_buf()))?
        .to_path_buf();

    let mut candidates = macos_bundle_candidates(exe_path);

    #[cfg(not(target_os = "linux"))]
    let _ = appimage_path;

    #[cfg(target_os = "linux")]
    {
        if let Some(appimage) = appimage_path {
            if let Some(parent) = appimage.parent() {
                candidates.push(PortableCandidate {
                    host_kind: PortableHostKind::LinuxAppImageDir,
                    host_dir: parent.to_path_buf(),
                });
            }
        }
    }

    candidates.push(PortableCandidate {
        host_kind: PortableHostKind::ExecutableDir,
        host_dir: exe_dir,
    });

    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.host_dir.clone()));
    Ok(candidates)
}

fn resolve_candidate_info(
    exe_dir: PathBuf,
    candidate: &PortableCandidate,
) -> Result<PortableInfo, PortableError> {
    let marker_path = candidate.host_dir.join(PORTABLE_MARKER_FILENAME);
    let config_path = candidate.host_dir.join(PORTABLE_CONFIG_FILENAME);

    let mut activation = PortableActivationKind::Disabled;
    let mut data_dir_name = PathBuf::from(PORTABLE_DEFAULT_DATA_DIRNAME);

    if config_path.exists() {
        let config_bytes = std::fs::read(&config_path).map_err(PortableError::PortableConfig)?;
        let portable_config: PortableConfigFile = serde_json::from_slice(&config_bytes)?;
        if portable_config.enabled.unwrap_or(true) {
            activation = PortableActivationKind::Config;
            if let Some(data_dir) = portable_config.data_dir.as_deref() {
                data_dir_name = portable_relative_data_dir(data_dir)?;
            }
        }
    } else if marker_path.exists() {
        activation = PortableActivationKind::Marker;
    }

    let data_dir = candidate.host_dir.join(&data_dir_name);

    Ok(PortableInfo {
        is_portable: activation != PortableActivationKind::Disabled,
        activation,
        host_kind: candidate.host_kind,
        exe_dir,
        host_dir: candidate.host_dir.clone(),
        marker_path,
        config_path,
        instance_lock_path: data_dir.join(PORTABLE_INSTANCE_LOCK_FILENAME),
        data_dir,
    })
}

pub fn detect_portable_info_from_exe(exe_path: &Path) -> Result<PortableInfo, PortableError> {
    let appimage_path = std::env::var_os("APPIMAGE").map(PathBuf::from);
    detect_portable_info_from_exe_with_appimage(exe_path, appimage_path.as_deref())
}

pub fn detect_portable_info_from_exe_with_appimage(
    exe_path: &Path,
    appimage_path: Option<&Path>,
) -> Result<PortableInfo, PortableError> {
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| PortableError::MissingExeParent(exe_path.to_path_buf()))?
        .to_path_buf();
    let candidates = portable_candidates(exe_path, appimage_path)?;

    let mut first_disabled = None;
    for candidate in candidates {
        let info = resolve_candidate_info(exe_dir.clone(), &candidate)?;
        if info.is_portable {
            return Ok(info);
        }
        if first_disabled.is_none() {
            first_disabled = Some(info);
        }
    }

    first_disabled.ok_or_else(|| PortableError::MissingExeParent(exe_path.to_path_buf()))
}

pub fn portable_info() -> Result<&'static PortableInfo, PortableError> {
    if let Some(info) = PORTABLE_INFO.get() {
        return Ok(info);
    }

    let exe_path = std::env::current_exe().map_err(PortableError::CurrentExe)?;
    let detected = detect_portable_info_from_exe(&exe_path)?;
    Ok(PORTABLE_INFO.get_or_init(|| detected))
}

pub fn is_portable_mode() -> Result<bool, PortableError> {
    Ok(portable_info()?.is_portable)
}

pub fn portable_data_dir() -> Result<Option<PathBuf>, PortableError> {
    let info = portable_info()?;
    Ok(info.is_portable.then(|| info.data_dir.clone()))
}

pub fn portable_instance_lock_path() -> Result<Option<PathBuf>, PortableError> {
    let info = portable_info()?;
    Ok(info.is_portable.then(|| info.instance_lock_path.clone()))
}

pub fn acquire_portable_instance_lock() -> Result<(), PortableError> {
    let info = portable_info()?;
    if !info.is_portable {
        return Ok(());
    }

    if PORTABLE_INSTANCE_LOCK.read().is_some() {
        return Ok(());
    }

    std::fs::create_dir_all(&info.data_dir).map_err(PortableError::InstanceLockIo)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&info.instance_lock_path)
        .map_err(PortableError::InstanceLockIo)?;

    match file.try_lock_exclusive() {
        Ok(()) => {
            *PORTABLE_INSTANCE_LOCK.write() = Some(PortableInstanceLock { _file: file });
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
            Err(PortableError::InstanceLocked(info.data_dir.clone()))
        }
        Err(err) => Err(PortableError::InstanceLockIo(err)),
    }
}

fn detect_portable_bootstrap_status() -> Result<PortableBootstrapStatus, PortableError> {
    let info = portable_info()?;
    if !info.is_portable {
        return Ok(PortableBootstrapStatus::Disabled);
    }

    let has_keystore = crate::config::portable_keystore::portable_keystore_exists()
        .map_err(|e| PortableError::Keystore(e.to_string()))?;

    Ok(if has_keystore {
        PortableBootstrapStatus::Locked
    } else {
        PortableBootstrapStatus::NeedsSetup
    })
}

pub fn initialize_portable_runtime() -> Result<PortableBootstrapStatus, PortableError> {
    let status = detect_portable_bootstrap_status()?;
    *PORTABLE_BOOTSTRAP_STATUS.write() = Some(status);
    Ok(status)
}

pub fn portable_bootstrap_status() -> Result<PortableBootstrapStatus, PortableError> {
    if let Some(status) = *PORTABLE_BOOTSTRAP_STATUS.read() {
        return Ok(status);
    }
    initialize_portable_runtime()
}

pub fn set_portable_bootstrap_status(status: PortableBootstrapStatus) -> Result<(), PortableError> {
    *PORTABLE_BOOTSTRAP_STATUS.write() = Some(status);
    Ok(())
}

pub fn portable_can_launch_full_app() -> Result<bool, PortableError> {
    Ok(portable_bootstrap_status()?.can_launch_full_app())
}

pub fn portable_aware_app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, PortableError> {
    if let Some(data_dir) = portable_data_dir()? {
        return Ok(data_dir);
    }

    app.path()
        .app_data_dir()
        .map_err(|e: tauri::Error| PortableError::AppDataDir(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn marker_enables_portable_mode_for_executable_dir() {
        let temp = tempdir().unwrap();
        let exe_path = temp.path().join("OxideTerm.exe");
        std::fs::write(&exe_path, b"").unwrap();
        std::fs::write(temp.path().join(PORTABLE_MARKER_FILENAME), b"").unwrap();

        let info = detect_portable_info_from_exe(&exe_path).unwrap();

        assert!(info.is_portable);
        assert_eq!(info.activation, PortableActivationKind::Marker);
        assert_eq!(info.host_kind, PortableHostKind::ExecutableDir);
        assert_eq!(info.exe_dir, temp.path());
        assert_eq!(info.host_dir, temp.path());
        assert_eq!(info.marker_path, temp.path().join(PORTABLE_MARKER_FILENAME));
        assert_eq!(info.config_path, temp.path().join(PORTABLE_CONFIG_FILENAME));
        assert_eq!(info.data_dir, temp.path().join(PORTABLE_DEFAULT_DATA_DIRNAME));
    }

    #[test]
    fn missing_marker_stays_in_normal_mode() {
        let temp = tempdir().unwrap();
        let exe_path = temp.path().join("OxideTerm.exe");
        std::fs::write(&exe_path, b"").unwrap();

        let info = detect_portable_info_from_exe(&exe_path).unwrap();

        assert!(!info.is_portable);
        assert_eq!(info.activation, PortableActivationKind::Disabled);
        assert_eq!(info.marker_path, temp.path().join(PORTABLE_MARKER_FILENAME));
        assert_eq!(info.config_path, temp.path().join(PORTABLE_CONFIG_FILENAME));
        assert_eq!(info.data_dir, temp.path().join(PORTABLE_DEFAULT_DATA_DIRNAME));
    }

    #[test]
    fn portable_json_enables_portable_and_custom_data_dir() {
        let temp = tempdir().unwrap();
        let exe_path = temp.path().join("OxideTerm");
        std::fs::write(&exe_path, b"").unwrap();
        std::fs::write(
            temp.path().join(PORTABLE_CONFIG_FILENAME),
            br#"{"enabled":true,"dataDir":"portable-store"}"#,
        )
        .unwrap();

        let info = detect_portable_info_from_exe(&exe_path).unwrap();

        assert!(info.is_portable);
        assert_eq!(info.activation, PortableActivationKind::Config);
        assert_eq!(info.data_dir, temp.path().join("portable-store"));
    }

    #[test]
    fn macos_bundle_prefers_bundle_sibling() {
        let temp = tempdir().unwrap();
        let app_dir = temp.path().join("OxideTerm.app");
        let exe_dir = app_dir.join("Contents/MacOS");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let exe_path = exe_dir.join("OxideTerm");
        std::fs::write(&exe_path, b"").unwrap();
        std::fs::write(temp.path().join(PORTABLE_MARKER_FILENAME), b"").unwrap();

        let info = detect_portable_info_from_exe_with_appimage(&exe_path, None).unwrap();

        #[cfg(target_os = "macos")]
        {
            assert!(info.is_portable);
            assert_eq!(info.host_kind, PortableHostKind::MacAppSibling);
            assert_eq!(info.host_dir, temp.path());
        }
    }

    #[test]
    fn linux_appimage_prefers_outer_appimage_dir() {
        let temp = tempdir().unwrap();
        let mounted_dir = temp.path().join("squashfs-root/usr/bin");
        std::fs::create_dir_all(&mounted_dir).unwrap();
        let exe_path = mounted_dir.join("oxideterm");
        let appimage_path = temp.path().join("OxideTerm.AppImage");
        std::fs::write(&exe_path, b"").unwrap();
        std::fs::write(&appimage_path, b"").unwrap();
        std::fs::write(temp.path().join(PORTABLE_MARKER_FILENAME), b"").unwrap();

        let _info =
            detect_portable_info_from_exe_with_appimage(&exe_path, Some(&appimage_path)).unwrap();

        #[cfg(target_os = "linux")]
        {
            assert!(_info.is_portable);
            assert_eq!(_info.host_kind, PortableHostKind::LinuxAppImageDir);
            assert_eq!(_info.host_dir, temp.path());
        }
    }

    #[test]
    fn portable_bootstrap_status_helpers_match_expectations() {
        assert!(PortableBootstrapStatus::Disabled.can_launch_full_app());
        assert!(!PortableBootstrapStatus::NeedsSetup.can_launch_full_app());
        assert!(!PortableBootstrapStatus::Locked.can_launch_full_app());
        assert!(PortableBootstrapStatus::Unlocked.can_launch_full_app());
        assert!(!PortableBootstrapStatus::NeedsSetup.has_keystore());
        assert!(PortableBootstrapStatus::Locked.has_keystore());
    }
}
