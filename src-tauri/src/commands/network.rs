//! Network Status Commands
//!
//! Handles network status changes from the frontend.

use std::sync::Arc;
use tauri::State;
use tracing::info;

use crate::session::AutoReconnectService;

/// Handle network status change from frontend
#[tauri::command]
pub async fn network_status_changed(
    online: bool,
    reconnect_service: State<'_, Arc<AutoReconnectService>>,
) -> Result<(), String> {
    info!("Network status changed: online={}", online);

    reconnect_service.set_network_status(online);

    // 🛑 后端禁止自动重连：只记录状态，不做决策
    // ❌ 已删除: reconnect_service.reconnect_all_disconnected().await;
    // 前端监听网络状态变化事件，自行决定是否重连

    Ok(())
}

/// Cancel reconnection for a session
#[tauri::command]
pub async fn cancel_reconnect(
    session_id: String,
    reconnect_service: State<'_, Arc<AutoReconnectService>>,
) -> Result<(), String> {
    reconnect_service.cancel_reconnect(&session_id);
    Ok(())
}

/// Check if a session is currently reconnecting
#[tauri::command]
pub async fn is_reconnecting(
    session_id: String,
    reconnect_service: State<'_, Arc<AutoReconnectService>>,
) -> Result<bool, String> {
    Ok(reconnect_service.is_reconnecting(&session_id))
}
