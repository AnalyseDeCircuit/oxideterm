// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

pub const PORTABLE_MARKER_FILENAME: &str = "portable";
pub const PORTABLE_CONFIG_FILENAME: &str = "portable.json";
pub const PORTABLE_DEFAULT_DATA_DIRNAME: &str = "data";
pub const PORTABLE_KEYSTORE_FILENAME: &str = "keystore.vault";
pub(crate) const PORTABLE_INSTANCE_LOCK_FILENAME: &str = ".portable.lock";

static PORTABLE_INFO: OnceLock<PortableInfo> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortableActivationKind {
    Disabled,
    Marker,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
    pub keystore_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum PortableError {
    #[error("Failed to determine current executable path: {0}")]
    CurrentExe(#[source] std::io::Error),

    #[error("Executable path has no parent directory: {0}")]
    MissingExeParent(PathBuf),

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
}

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
    } else if is_portable_marker_file(&marker_path) {
        activation = PortableActivationKind::Marker;
    }

    let data_dir = candidate.host_dir.join(data_dir_name);
    Ok(PortableInfo {
        is_portable: activation != PortableActivationKind::Disabled,
        activation,
        host_kind: candidate.host_kind,
        exe_dir,
        host_dir: candidate.host_dir.clone(),
        marker_path,
        config_path,
        instance_lock_path: data_dir.join(PORTABLE_INSTANCE_LOCK_FILENAME),
        keystore_path: data_dir.join(PORTABLE_KEYSTORE_FILENAME),
        data_dir,
    })
}

fn is_portable_marker_file(marker_path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(marker_path) else {
        return false;
    };
    // Regression guard for GitHub #143: Linux packages may install an unrelated
    // executable named `portable` beside `oxideterm` under a shared bin dir.
    // Only an intentionally-created empty marker enables portable mode.
    metadata.is_file() && metadata.len() == 0
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

pub fn portable_ssh_dir() -> Result<Option<PathBuf>, PortableError> {
    let info = portable_info()?;
    // SSH files are runtime data in portable mode, not resources copied into
    // the executable directory. Keeping them under data preserves portability.
    Ok(info.is_portable.then(|| info.data_dir.join(".ssh")))
}

pub fn portable_keystore_file_path() -> Result<Option<PathBuf>, PortableError> {
    let info = portable_info()?;
    Ok(info.is_portable.then(|| info.keystore_path.clone()))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

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
        assert_eq!(
            info.data_dir,
            temp.path().join(PORTABLE_DEFAULT_DATA_DIRNAME)
        );
    }

    #[test]
    fn non_empty_portable_file_does_not_enable_portable_mode() {
        let temp = tempdir().unwrap();
        let exe_path = temp.path().join("oxideterm");
        std::fs::write(&exe_path, b"").unwrap();
        std::fs::write(
            temp.path().join(PORTABLE_MARKER_FILENAME),
            b"#!/usr/bin/env sh\n",
        )
        .unwrap();

        let info = detect_portable_info_from_exe(&exe_path).unwrap();

        assert!(!info.is_portable);
        assert_eq!(info.activation, PortableActivationKind::Disabled);
    }

    #[test]
    fn issue_143_portable_binary_name_does_not_enable_portable_mode() {
        let temp = tempdir().unwrap();
        let exe_path = temp.path().join("oxideterm");
        std::fs::write(&exe_path, b"").unwrap();
        std::fs::write(temp.path().join(PORTABLE_MARKER_FILENAME), b"\x7fELF...").unwrap();

        let info = detect_portable_info_from_exe(&exe_path).unwrap();

        assert!(!info.is_portable);
        assert_eq!(info.activation, PortableActivationKind::Disabled);
        assert_eq!(info.host_kind, PortableHostKind::ExecutableDir);
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
        assert_eq!(
            info.keystore_path,
            temp.path()
                .join("portable-store")
                .join(PORTABLE_KEYSTORE_FILENAME)
        );
    }

    #[test]
    fn portable_json_rejects_parent_relative_data_dir() {
        let temp = tempdir().unwrap();
        let exe_path = temp.path().join("OxideTerm");
        std::fs::write(&exe_path, b"").unwrap();
        std::fs::write(
            temp.path().join(PORTABLE_CONFIG_FILENAME),
            br#"{"enabled":true,"dataDir":"../escape"}"#,
        )
        .unwrap();

        let error = detect_portable_info_from_exe(&exe_path).unwrap_err();

        assert!(matches!(error, PortableError::InvalidPortableDataDir(_)));
    }

    #[test]
    fn disabled_portable_json_takes_precedence_over_marker() {
        let temp = tempdir().unwrap();
        let exe_path = temp.path().join("OxideTerm");
        std::fs::write(&exe_path, b"").unwrap();
        std::fs::write(temp.path().join(PORTABLE_MARKER_FILENAME), b"").unwrap();
        std::fs::write(
            temp.path().join(PORTABLE_CONFIG_FILENAME),
            br#"{"enabled":false}"#,
        )
        .unwrap();

        let info = detect_portable_info_from_exe(&exe_path).unwrap();

        assert!(!info.is_portable);
        assert_eq!(info.activation, PortableActivationKind::Disabled);
    }
}
