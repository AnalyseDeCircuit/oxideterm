//! SFTP Transfer Control Commands
//!
//! Node-independent transfer management (cancel, pause, resume, stats, settings).
//! These operate on `transfer_id` or global state, not on specific SSH sessions.
//!
//! **Oxide-Next Phase 5**: All session_id-based legacy commands removed.
//! Node-routed SFTP operations are in [`super::node_sftp`].

use std::sync::Arc;
use tauri::State;

use crate::sftp::error::SftpError;

// ============ Transfer Control Commands ============

/// Cancel a specific transfer
#[tauri::command]
pub async fn sftp_cancel_transfer(
    transfer_id: String,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<bool, SftpError> {
    Ok(transfer_manager.cancel(&transfer_id))
}

/// Pause a specific transfer
#[tauri::command]
pub async fn sftp_pause_transfer(
    transfer_id: String,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<bool, SftpError> {
    Ok(transfer_manager.pause(&transfer_id))
}

/// Resume a specific transfer
#[tauri::command]
pub async fn sftp_resume_transfer(
    transfer_id: String,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<bool, SftpError> {
    Ok(transfer_manager.resume(&transfer_id))
}

/// Get transfer manager stats
#[tauri::command]
pub async fn sftp_transfer_stats(
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(usize, usize), SftpError> {
    Ok((
        transfer_manager.active_count(),
        transfer_manager.max_concurrent(),
    ))
}

/// Update transfer settings (concurrent limit and speed limit)
#[tauri::command]
pub async fn sftp_update_settings(
    max_concurrent: Option<usize>,
    speed_limit_kbps: Option<usize>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(), SftpError> {
    if let Some(max) = max_concurrent {
        transfer_manager.set_max_concurrent(max);
    }
    if let Some(kbps) = speed_limit_kbps {
        transfer_manager.set_speed_limit_kbps(kbps);
    }
    Ok(())
}

// ============ Shared Types ============

/// Frontend type for incomplete transfer info
/// Used by both legacy and node-first commands
#[derive(serde::Serialize)]
pub struct IncompleteTransferInfo {
    pub transfer_id: String,
    pub transfer_type: &'static str,
    pub source_path: String,
    pub destination_path: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub status: &'static str,
    pub session_id: String,
    pub error: Option<String>,
    pub progress_percent: f64,
    pub can_resume: bool,
}
