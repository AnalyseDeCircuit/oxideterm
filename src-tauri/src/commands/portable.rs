// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Portable mode inspection commands.

use serde::Serialize;

use super::config::ConfigState;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableInfoResponse {
    pub is_portable: bool,
    pub activation: crate::config::PortableActivationKind,
    pub host_kind: crate::config::PortableHostKind,
    pub exe_dir: String,
    pub host_dir: String,
    pub marker_path: String,
    pub config_path: String,
    pub data_dir: String,
    pub instance_lock_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableStatusResponse {
    pub is_portable: bool,
    pub activation: crate::config::PortableActivationKind,
    pub host_kind: crate::config::PortableHostKind,
    pub status: crate::config::PortableBootstrapStatus,
    pub can_launch_app: bool,
    pub has_keystore: bool,
    pub is_unlocked: bool,
    pub keystore_path: Option<String>,
    pub portable_root_dir: String,
    pub marker_path: String,
    pub config_path: String,
    pub instance_lock_path: Option<String>,
    pub supports_biometric_binding: bool,
    pub has_biometric_binding: bool,
    pub can_biometric_unlock: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableMigrationSummaryResponse {
    pub is_portable: bool,
    pub current_data_dir: String,
    pub portable_data_dir: String,
    pub exportable_portable_secret_count: usize,
}

fn build_status_response() -> Result<PortableStatusResponse, String> {
    let info = crate::config::portable_info().map_err(|e| e.to_string())?;
    let status = crate::config::portable_bootstrap_status().map_err(|e| e.to_string())?;
    let keystore_path = crate::config::portable_keystore::portable_keystore_file_path()
        .map_err(|e| e.to_string())?
        .map(|path| path.to_string_lossy().to_string());
    let supports_biometric_binding = crate::config::portable_keystore::supports_biometric_binding();
    let has_biometric_binding = crate::config::portable_keystore::has_biometric_binding()
        .map_err(|e| e.to_string())?;

    Ok(PortableStatusResponse {
        is_portable: info.is_portable,
        activation: info.activation,
        host_kind: info.host_kind,
        status,
        can_launch_app: status.can_launch_full_app(),
        has_keystore: status.has_keystore(),
        is_unlocked: crate::config::portable_keystore::is_portable_keystore_unlocked(),
        keystore_path,
        portable_root_dir: info.host_dir.to_string_lossy().to_string(),
        marker_path: info.marker_path.to_string_lossy().to_string(),
        config_path: info.config_path.to_string_lossy().to_string(),
        instance_lock_path: crate::config::portable_instance_lock_path()
            .map_err(|e| e.to_string())?
            .map(|path| path.to_string_lossy().to_string()),
        supports_biometric_binding,
        has_biometric_binding,
        can_biometric_unlock: status == crate::config::PortableBootstrapStatus::Locked
            && supports_biometric_binding
            && has_biometric_binding,
    })
}

#[tauri::command]
pub async fn get_portable_info() -> Result<PortableInfoResponse, String> {
    let info = crate::config::portable_info().map_err(|e| e.to_string())?;

    Ok(PortableInfoResponse {
        is_portable: info.is_portable,
        activation: info.activation,
        host_kind: info.host_kind,
        exe_dir: info.exe_dir.to_string_lossy().to_string(),
        host_dir: info.host_dir.to_string_lossy().to_string(),
        marker_path: info.marker_path.to_string_lossy().to_string(),
        config_path: info.config_path.to_string_lossy().to_string(),
        data_dir: info.data_dir.to_string_lossy().to_string(),
        instance_lock_path: info.instance_lock_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn get_portable_status() -> Result<PortableStatusResponse, String> {
    build_status_response()
}

#[tauri::command]
pub async fn get_portable_migration_summary(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
) -> Result<PortableMigrationSummaryResponse, String> {
    let portable_info = crate::config::portable_info().map_err(|e| e.to_string())?;
    let data_dir_info = crate::config::storage::get_data_dir_info().map_err(|e| e.to_string())?;

    Ok(PortableMigrationSummaryResponse {
        is_portable: portable_info.is_portable,
        current_data_dir: data_dir_info.effective.to_string_lossy().to_string(),
        portable_data_dir: portable_info.data_dir.to_string_lossy().to_string(),
        exportable_portable_secret_count: state.count_exportable_ai_provider_keys(&app_handle)?,
    })
}

#[tauri::command]
pub async fn setup_portable_keystore(
    state: State<'_, Arc<ConfigState>>,
    password: String,
) -> Result<PortableStatusResponse, String> {
    state.setup_portable_keystore(&password).await?;
    build_status_response()
}

#[tauri::command]
pub async fn unlock_portable_keystore(
    state: State<'_, Arc<ConfigState>>,
    password: String,
) -> Result<PortableStatusResponse, String> {
    state.unlock_portable_keystore(&password).await?;
    build_status_response()
}

#[tauri::command]
pub async fn reset_portable_keystore(
    state: State<'_, Arc<ConfigState>>,
) -> Result<PortableStatusResponse, String> {
    state.reset_portable_keystore().await?;
    build_status_response()
}

#[tauri::command]
pub async fn unlock_portable_keystore_with_biometrics(
    state: State<'_, Arc<ConfigState>>,
) -> Result<PortableStatusResponse, String> {
    state.unlock_portable_keystore_with_biometrics().await?;
    build_status_response()
}

#[tauri::command]
pub async fn change_portable_keystore_password(
    state: State<'_, Arc<ConfigState>>,
    current_password: String,
    new_password: String,
) -> Result<PortableStatusResponse, String> {
    state
        .change_portable_keystore_password(&current_password, &new_password)
        .await?;
    build_status_response()
}

#[tauri::command]
pub async fn enable_portable_biometric_unlock(
    state: State<'_, Arc<ConfigState>>,
    password: String,
) -> Result<PortableStatusResponse, String> {
    state.enable_portable_biometric_unlock(&password).await?;
    build_status_response()
}

#[tauri::command]
pub async fn disable_portable_biometric_unlock(
    state: State<'_, Arc<ConfigState>>,
) -> Result<PortableStatusResponse, String> {
    state.disable_portable_biometric_unlock().await?;
    build_status_response()
}
