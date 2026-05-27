// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use parking_lot::RwLock;
use serde::Serialize;

use crate::{PortableActivationKind, PortableError, PortableHostKind, portable_info};

static PORTABLE_BOOTSTRAP_STATUS: std::sync::LazyLock<RwLock<Option<PortableBootstrapStatus>>> =
    std::sync::LazyLock::new(|| RwLock::new(None));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableStatusSnapshot {
    pub is_portable: bool,
    pub activation: PortableActivationKind,
    pub host_kind: PortableHostKind,
    pub status: PortableBootstrapStatus,
    pub can_launch_app: bool,
    pub has_keystore: bool,
    pub is_unlocked: bool,
    pub portable_root_dir: String,
    pub marker_path: String,
    pub config_path: String,
    pub data_dir: String,
    pub instance_lock_path: Option<String>,
    pub keystore_path: Option<String>,
}

fn detect_portable_bootstrap_status() -> Result<PortableBootstrapStatus, PortableError> {
    let info = portable_info()?;
    if !info.is_portable {
        return Ok(PortableBootstrapStatus::Disabled);
    }

    Ok(if crate::keystore::is_portable_keystore_unlocked() {
        PortableBootstrapStatus::Unlocked
    } else if info.keystore_path.exists() {
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

pub fn portable_status_snapshot() -> Result<PortableStatusSnapshot, PortableError> {
    let info = portable_info()?;
    let status = portable_bootstrap_status()?;
    Ok(PortableStatusSnapshot {
        is_portable: info.is_portable,
        activation: info.activation,
        host_kind: info.host_kind,
        status,
        can_launch_app: status.can_launch_full_app(),
        has_keystore: status.has_keystore(),
        is_unlocked: crate::keystore::is_portable_keystore_unlocked(),
        portable_root_dir: info.host_dir.display().to_string(),
        marker_path: info.marker_path.display().to_string(),
        config_path: info.config_path.display().to_string(),
        data_dir: info.data_dir.display().to_string(),
        instance_lock_path: info
            .is_portable
            .then(|| info.instance_lock_path.display().to_string()),
        keystore_path: info
            .is_portable
            .then(|| info.keystore_path.display().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_status_helpers_match_tauri_expectations() {
        assert!(PortableBootstrapStatus::Disabled.can_launch_full_app());
        assert!(!PortableBootstrapStatus::NeedsSetup.can_launch_full_app());
        assert!(!PortableBootstrapStatus::Locked.can_launch_full_app());
        assert!(PortableBootstrapStatus::Unlocked.can_launch_full_app());
        assert!(!PortableBootstrapStatus::NeedsSetup.has_keystore());
        assert!(PortableBootstrapStatus::Locked.has_keystore());
    }
}
