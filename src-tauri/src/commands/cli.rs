// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! CLI companion install/uninstall commands.
//!
//! The `oxt` CLI binary is bundled with the app but not installed by default.
//! These commands handle creating/removing symlinks or copies in the user's PATH.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use tauri::Manager;

/// CLI installation status returned to the frontend.
#[derive(Debug, Serialize)]
pub struct CliStatus {
    /// Whether the CLI binary is bundled with this build
    pub bundled: bool,
    /// Whether the CLI is currently installed (found in PATH)
    pub installed: bool,
    /// Where the CLI is installed (symlink / copy target)
    pub install_path: Option<String>,
    /// Path to the bundled CLI binary inside the app
    pub bundle_path: Option<String>,
    /// Current app version (and bundled CLI version)
    pub app_version: String,
    /// Whether the installed CLI binary matches the bundled one
    pub matches_bundled: Option<bool>,
    /// Whether the installed CLI should be reinstalled from the bundled copy
    pub needs_reinstall: bool,
}

/// Get CLI installation status.
#[tauri::command]
pub async fn cli_get_status(app_handle: tauri::AppHandle) -> Result<CliStatus, String> {
    let bundle_path = find_bundled_cli(&app_handle);
    let bundled = bundle_path.is_some();
    let app_version = env!("CARGO_PKG_VERSION").to_string();

    let install_target = cli_install_path();
    let installed = cli_path_present(&install_target);
    let matches_bundled = match (bundle_path.as_ref(), installed) {
        (Some(bundle_path), true) => match installed_cli_matches_bundle(&install_target, bundle_path) {
            Ok(matches) => Some(matches),
            Err(error) => {
                tracing::warn!(
                    "Failed to verify installed CLI against bundled binary at {}: {}",
                    install_target.display(),
                    error
                );
                None
            }
        },
        _ => None,
    };
    let needs_reinstall = bundled && installed && matches_bundled == Some(false);

    Ok(CliStatus {
        bundled,
        installed,
        install_path: Some(install_target.display().to_string()),
        bundle_path: bundle_path.map(|p| p.display().to_string()),
        app_version,
        matches_bundled,
        needs_reinstall,
    })
}

fn cli_path_present(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

fn installed_cli_matches_bundle(install_path: &Path, bundle_path: &Path) -> Result<bool, String> {
    let install_metadata = install_path
        .symlink_metadata()
        .map_err(|e| format!("Failed to inspect {}: {e}", install_path.display()))?;

    if install_metadata.file_type().is_symlink() && !install_path.exists() {
        return Ok(false);
    }

    let install_canonical = install_path.canonicalize().ok();
    let bundle_canonical = bundle_path.canonicalize().ok();

    if let (Some(install_canonical), Some(bundle_canonical)) =
        (install_canonical.as_ref(), bundle_canonical.as_ref())
    {
        if install_canonical == bundle_canonical {
            return Ok(true);
        }
    }

    Ok(file_sha256(install_path)? == file_sha256(bundle_path)?)
}

fn file_sha256(path: &Path) -> Result<[u8; 32], String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let read = file
            .read(&mut buf)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(hasher.finalize().into())
}

/// Install the CLI by creating a symlink (macOS/Linux) or copying (Windows).
#[tauri::command]
pub async fn cli_install(app_handle: tauri::AppHandle) -> Result<String, String> {
    let bundle_path = find_bundled_cli(&app_handle)
        .ok_or("CLI binary not found in app bundle. This build may not include CLI support.")?;

    let target = cli_install_path();

    // Ensure target directory exists
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {:?}: {e}", parent))?;
    }

    // Remove existing file/symlink if present
    if target.exists() || target.symlink_metadata().is_ok() {
        std::fs::remove_file(&target)
            .map_err(|e| format!("Failed to remove existing {:?}: {e}", target))?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&bundle_path, &target)
            .map_err(|e| format!("Failed to create symlink: {e}"))?;
    }

    #[cfg(windows)]
    {
        std::fs::copy(&bundle_path, &target)
            .map_err(|e| format!("Failed to copy CLI binary: {e}"))?;
    }

    let msg = format!("CLI installed at {}", target.display());
    tracing::info!("{}", msg);
    Ok(msg)
}

/// Uninstall the CLI by removing the symlink/copy.
#[tauri::command]
pub async fn cli_uninstall() -> Result<String, String> {
    let target = cli_install_path();

    if !target.exists() && target.symlink_metadata().is_err() {
        return Ok("CLI is not installed".to_string());
    }

    std::fs::remove_file(&target).map_err(|e| format!("Failed to remove {:?}: {e}", target))?;

    let msg = format!("CLI uninstalled from {}", target.display());
    tracing::info!("{}", msg);
    Ok(msg)
}

/// Find the bundled CLI binary inside the app.
fn find_bundled_cli(app_handle: &tauri::AppHandle) -> Option<PathBuf> {
    let binary_name = cli_binary_name();

    // Try Tauri resource resolver (bundled in cli-bin/ subdirectory)
    if let Ok(path) = app_handle.path().resolve(
        format!("cli-bin/{binary_name}"),
        tauri::path::BaseDirectory::Resource,
    ) {
        if path.exists() {
            return Some(path);
        }
    }

    // Fallback: direct resource path
    if let Ok(path) = app_handle
        .path()
        .resolve(&binary_name, tauri::path::BaseDirectory::Resource)
    {
        if path.exists() {
            return Some(path);
        }
    }

    // Fallback: check next to the main executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join(&binary_name);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

/// Default install path for the CLI binary.
fn cli_install_path() -> PathBuf {
    #[cfg(unix)]
    {
        // Always use ~/.local/bin (user-writable, no sudo needed)
        if let Some(home) = dirs::home_dir() {
            return home.join(".local").join("bin").join("oxt");
        }
        PathBuf::from("/usr/local/bin/oxt")
    }

    #[cfg(windows)]
    {
        // Use %LOCALAPPDATA%\OxideTerm\bin
        if let Some(local_app_data) = dirs::data_local_dir() {
            return local_app_data.join("OxideTerm").join("bin").join("oxt.exe");
        }
        PathBuf::from("oxt.exe")
    }
}

/// Platform-appropriate CLI binary filename.
fn cli_binary_name() -> String {
    #[cfg(windows)]
    {
        "oxt.exe".to_string()
    }
    #[cfg(not(windows))]
    {
        "oxt".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{cli_path_present, installed_cli_matches_bundle};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn identical_files_match_bundled_copy() {
        let temp_dir = TempDir::new().unwrap();
        let installed_path = temp_dir.path().join("installed-oxt");
        let bundled_path = temp_dir.path().join("bundled-oxt");

        fs::write(&installed_path, b"same-cli-binary").unwrap();
        fs::write(&bundled_path, b"same-cli-binary").unwrap();

        assert!(installed_cli_matches_bundle(&installed_path, &bundled_path).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_is_still_treated_as_installed() {
        let temp_dir = TempDir::new().unwrap();
        let broken_target = temp_dir.path().join("missing-oxt");
        let install_path = temp_dir.path().join("oxt");

        std::os::unix::fs::symlink(&broken_target, &install_path).unwrap();

        assert!(cli_path_present(&install_path));
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_requires_reinstall() {
        let temp_dir = TempDir::new().unwrap();
        let bundled_path = temp_dir.path().join("bundled-oxt");
        let broken_target = temp_dir.path().join("missing-oxt");
        let install_path = temp_dir.path().join("oxt");

        fs::write(&bundled_path, b"bundled-cli-binary").unwrap();
        std::os::unix::fs::symlink(&broken_target, &install_path).unwrap();

        assert!(!installed_cli_matches_bundle(&install_path, &bundled_path).unwrap());
    }
}
