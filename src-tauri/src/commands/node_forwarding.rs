// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Node-first Forwarding commands — Oxide-Next Convergence
//!
//! 所有命令接受 nodeId 而非 sessionId。
//! 内部通过 NodeRouter 解析 nodeId → terminal_session_id，
//! 然后委托给 ForwardingRegistry。
//!
//! 参考: docs/reference/OXIDE_NEXT_ARCHITECTURE.md §3.2

use std::sync::Arc;

use tauri::State;
use tracing::{error, info, warn};

use crate::commands::forwarding::{
    ForwardResponse, ForwardRuleDto, ForwardStatsDto, ForwardingRegistry, PersistedForwardDto,
};
use crate::forwarding::{ForwardRule, ForwardRuleUpdate, ForwardStatus, ForwardType};
use crate::router::{NodeRouter, RouteError};
use crate::state::PersistedForward;

use super::session_tree::SessionTreeState;

fn parse_forward_type(forward_type: &str) -> Result<ForwardType, String> {
    match forward_type {
        "local" => Ok(ForwardType::Local),
        "remote" => Ok(ForwardType::Remote),
        "dynamic" => Ok(ForwardType::Dynamic),
        other => Err(format!("Unknown forward type: {}", other)),
    }
}

fn build_unreachable_port_error(target_host: &str, target_port: u16) -> String {
    format!(
        "Target port {}:{} is not reachable. Please ensure the service is running on the remote server.\n\nTroubleshooting:\n• Check if service is running: ss -tlnp | grep {}\n• Verify the port number is correct\n• Try connecting manually: nc -zv {} {}",
        target_host, target_port, target_port, target_host, target_port
    )
}

fn build_health_check_error_message(error: &str) -> String {
    format!(
        "Failed to check port availability: {}\n\nYou can skip this check with the 'Skip port availability check' option.",
        error
    )
}

/// 辅助函数：从 NodeRouter 获取 terminal_session_id
async fn resolve_terminal_session_id(
    router: &NodeRouter,
    node_id: &str,
) -> Result<String, RouteError> {
    let resolved = router.resolve_connection(node_id).await?;
    resolved.terminal_session_id.ok_or_else(|| {
        RouteError::NotConnected(format!("Node {} has no terminal session", node_id))
    })
}

async fn resolve_saved_connection_owner(
    session_tree_state: &SessionTreeState,
    node_id: &str,
) -> Option<String> {
    let tree = session_tree_state.tree.read().await;
    tree.get_node(node_id)
        .and_then(|node| node.origin.saved_connection_id().map(|id| id.to_string()))
}

async fn load_saved_forwards_for_node(
    node_id: &str,
    router: &NodeRouter,
    registry: &ForwardingRegistry,
    session_tree_state: &SessionTreeState,
) -> Result<Vec<PersistedForward>, RouteError> {
    if let Some(owner_connection_id) =
        resolve_saved_connection_owner(session_tree_state, node_id).await
    {
        return registry
            .load_owned_forwards(&owner_connection_id)
            .await
            .map_err(RouteError::ConnectionError);
    }

    let session_id = resolve_terminal_session_id(router, node_id).await?;
    registry
        .load_persisted_forwards(&session_id)
        .await
        .map_err(RouteError::ConnectionError)
}

async fn sync_node_owned_forward(
    node_id: &str,
    session_id: &str,
    rule: &ForwardRule,
    registry: &ForwardingRegistry,
    session_tree_state: &SessionTreeState,
) -> Result<(), RouteError> {
    let owner_connection_id = resolve_saved_connection_owner(session_tree_state, node_id).await;
    registry
        .sync_persisted_forward_rule(&rule.id, session_id, owner_connection_id, rule.clone())
        .await
        .map_err(RouteError::ConnectionError)
}

// ========================================================================
// Node-first forwarding commands
// ========================================================================

/// 列出节点的所有端口转发
#[tauri::command]
pub async fn node_list_forwards(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<Vec<ForwardRuleDto>, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;

    if let Some(mgr) = registry.get(&session_id).await {
        let rules = mgr.list_forwards().await;
        Ok(rules.into_iter().map(ForwardRuleDto::from).collect())
    } else {
        Ok(Vec::new())
    }
}

/// 创建端口转发
#[tauri::command]
pub async fn node_create_forward(
    node_id: String,
    forward_type: String,
    bind_address: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
    description: Option<String>,
    check_health: Option<bool>,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<crate::ssh::SshConnectionRegistry>>,
    session_tree_state: State<'_, Arc<SessionTreeState>>,
) -> Result<ForwardResponse, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    info!(
        "node_create_forward: nodeId={}, resolved session={}",
        node_id, session_id
    );

    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    let fwd_type = match parse_forward_type(&forward_type) {
        Ok(kind) => kind,
        Err(error) => {
            return Ok(ForwardResponse {
                success: false,
                forward: None,
                error: Some(error),
            });
        }
    };

    // Health check for non-dynamic forwards
    let do_check = check_health.unwrap_or(true);
    if do_check && fwd_type != ForwardType::Dynamic {
        info!(
            "Checking port availability: {}:{}",
            target_host, target_port
        );

        match mgr
            .check_port_available(&target_host, target_port, 3000)
            .await
        {
            Ok(true) => {
                info!("Port {}:{} is available", target_host, target_port);
            }
            Ok(false) => {
                let error_msg = build_unreachable_port_error(&target_host, target_port);
                error!("Port health check failed: {}", error_msg);
                return Ok(ForwardResponse {
                    success: false,
                    forward: None,
                    error: Some(error_msg),
                });
            }
            Err(e) => {
                let error_msg = build_health_check_error_message(&e.to_string());
                error!("Health check error: {}", error_msg);
                return Ok(ForwardResponse {
                    success: false,
                    forward: None,
                    error: Some(error_msg),
                });
            }
        }
    }

    let rule = ForwardRule {
        id: uuid::Uuid::new_v4().to_string(),
        forward_type: fwd_type,
        bind_address,
        bind_port,
        target_host,
        target_port,
        status: ForwardStatus::Starting,
        description,
    };

    match mgr.create_forward(rule).await {
        Ok(created) => {
            let forward_id = created.id.clone();
            info!("Port forward created: {}", forward_id);

            if let Err(e) = sync_node_owned_forward(
                &node_id,
                &session_id,
                &created,
                registry.inner().as_ref(),
                session_tree_state.inner().as_ref(),
            )
            .await
            {
                warn!("Failed to persist saved forward {}: {}", created.id, e);
            }

            // 更新 ConnectionRegistry 的 forward 列表
            if let Err(e) = connection_registry
                .add_forward(&session_id, forward_id)
                .await
            {
                warn!(
                    "Failed to update forward state in ConnectionRegistry: {}",
                    e
                );
            }

            Ok(ForwardResponse {
                success: true,
                forward: Some(created.into()),
                error: None,
            })
        }
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_forward_type_accepts_supported_variants() {
        assert!(matches!(
            parse_forward_type("local"),
            Ok(ForwardType::Local)
        ));
        assert!(matches!(
            parse_forward_type("remote"),
            Ok(ForwardType::Remote)
        ));
        assert!(matches!(
            parse_forward_type("dynamic"),
            Ok(ForwardType::Dynamic)
        ));
    }

    #[test]
    fn parse_forward_type_rejects_unknown_values() {
        let error = parse_forward_type("udp").unwrap_err();
        assert!(error.contains("Unknown forward type: udp"));
    }

    #[test]
    fn unreachable_port_error_includes_troubleshooting_details() {
        let message = build_unreachable_port_error("service.internal", 3000);
        assert!(message.contains("Target port service.internal:3000 is not reachable"));
        assert!(message.contains("ss -tlnp | grep 3000"));
        assert!(message.contains("nc -zv service.internal 3000"));
    }

    #[test]
    fn health_check_error_mentions_skip_option() {
        let message = build_health_check_error_message("timeout");
        assert!(message.contains("Failed to check port availability: timeout"));
        assert!(message.contains("Skip port availability check"));
    }
}

/// 停止端口转发
#[tauri::command]
pub async fn node_stop_forward(
    node_id: String,
    forward_id: String,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<ForwardResponse, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    match mgr.stop_forward(&forward_id).await {
        Ok(()) => Ok(ForwardResponse {
            success: true,
            forward: None,
            error: None,
        }),
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// 删除端口转发
#[tauri::command]
pub async fn node_delete_forward(
    node_id: String,
    forward_id: String,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
    connection_registry: State<'_, Arc<crate::ssh::SshConnectionRegistry>>,
) -> Result<ForwardResponse, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    match mgr.delete_forward(&forward_id).await {
        Ok(()) => {
            if let Err(e) = registry.delete_persisted_forward(forward_id.clone()).await {
                warn!("Failed to delete persisted forward {}: {}", forward_id, e);
            }

            // 从 ConnectionRegistry 移除 forward
            if let Err(e) = connection_registry
                .remove_forward(&session_id, &forward_id)
                .await
            {
                warn!("Failed to remove forward from ConnectionRegistry: {}", e);
            }
            Ok(ForwardResponse {
                success: true,
                forward: None,
                error: None,
            })
        }
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// 重启已停止的端口转发
#[tauri::command]
pub async fn node_restart_forward(
    node_id: String,
    forward_id: String,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_tree_state: State<'_, Arc<SessionTreeState>>,
) -> Result<ForwardResponse, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    match mgr.restart_forward(&forward_id).await {
        Ok(rule) => {
            if let Err(e) = sync_node_owned_forward(
                &node_id,
                &session_id,
                &rule,
                registry.inner().as_ref(),
                session_tree_state.inner().as_ref(),
            )
            .await
            {
                warn!(
                    "Failed to sync saved forward {} after restart: {}",
                    rule.id, e
                );
            }

            Ok(ForwardResponse {
                success: true,
                forward: Some(rule.into()),
                error: None,
            })
        }
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// 更新端口转发配置
#[tauri::command]
pub async fn node_update_forward(
    node_id: String,
    forward_id: String,
    bind_address: Option<String>,
    bind_port: Option<u16>,
    target_host: Option<String>,
    target_port: Option<u16>,
    description: Option<String>,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_tree_state: State<'_, Arc<SessionTreeState>>,
) -> Result<ForwardResponse, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    let updates = ForwardRuleUpdate {
        bind_address,
        bind_port,
        target_host,
        target_port,
        description,
    };

    match mgr.update_forward(&forward_id, updates).await {
        Ok(rule) => {
            if let Err(e) = sync_node_owned_forward(
                &node_id,
                &session_id,
                &rule,
                registry.inner().as_ref(),
                session_tree_state.inner().as_ref(),
            )
            .await
            {
                warn!(
                    "Failed to sync saved forward {} after update: {}",
                    rule.id, e
                );
            }

            Ok(ForwardResponse {
                success: true,
                forward: Some(rule.into()),
                error: None,
            })
        }
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// 获取端口转发统计信息
#[tauri::command]
pub async fn node_get_forward_stats(
    node_id: String,
    forward_id: String,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<Option<ForwardStatsDto>, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    Ok(mgr.get_forward_stats(&forward_id).await.map(|s| s.into()))
}

/// 停止节点的所有转发
#[tauri::command]
pub async fn node_stop_all_forwards(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<(), RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    if let Some(mgr) = registry.get(&session_id).await {
        mgr.stop_all().await;
    }
    Ok(())
}

/// 快捷转发 Jupyter
#[tauri::command]
pub async fn node_forward_jupyter(
    node_id: String,
    local_port: u16,
    remote_port: u16,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_tree_state: State<'_, Arc<SessionTreeState>>,
) -> Result<ForwardResponse, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    match mgr.forward_jupyter(local_port, remote_port).await {
        Ok(rule) => {
            if let Err(e) = sync_node_owned_forward(
                &node_id,
                &session_id,
                &rule,
                registry.inner().as_ref(),
                session_tree_state.inner().as_ref(),
            )
            .await
            {
                warn!("Failed to persist Jupyter forward {}: {}", rule.id, e);
            }

            Ok(ForwardResponse {
                success: true,
                forward: Some(rule.into()),
                error: None,
            })
        }
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// 快捷转发 TensorBoard
#[tauri::command]
pub async fn node_forward_tensorboard(
    node_id: String,
    local_port: u16,
    remote_port: u16,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_tree_state: State<'_, Arc<SessionTreeState>>,
) -> Result<ForwardResponse, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    match mgr.forward_tensorboard(local_port, remote_port).await {
        Ok(rule) => {
            if let Err(e) = sync_node_owned_forward(
                &node_id,
                &session_id,
                &rule,
                registry.inner().as_ref(),
                session_tree_state.inner().as_ref(),
            )
            .await
            {
                warn!("Failed to persist TensorBoard forward {}: {}", rule.id, e);
            }

            Ok(ForwardResponse {
                success: true,
                forward: Some(rule.into()),
                error: None,
            })
        }
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// 快捷转发 VS Code
#[tauri::command]
pub async fn node_forward_vscode(
    node_id: String,
    local_port: u16,
    remote_port: u16,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_tree_state: State<'_, Arc<SessionTreeState>>,
) -> Result<ForwardResponse, RouteError> {
    let session_id = resolve_terminal_session_id(&router, &node_id).await?;
    let mgr = registry.get(&session_id).await.ok_or_else(|| {
        RouteError::NotConnected(format!("No forwarding manager for node {}", node_id))
    })?;

    match mgr.forward_vscode(local_port, remote_port).await {
        Ok(rule) => {
            if let Err(e) = sync_node_owned_forward(
                &node_id,
                &session_id,
                &rule,
                registry.inner().as_ref(),
                session_tree_state.inner().as_ref(),
            )
            .await
            {
                warn!("Failed to persist VS Code forward {}: {}", rule.id, e);
            }

            Ok(ForwardResponse {
                success: true,
                forward: Some(rule.into()),
                error: None,
            })
        }
        Err(e) => Ok(ForwardResponse {
            success: false,
            forward: None,
            error: Some(e.to_string()),
        }),
    }
}

/// 列出节点的已保存转发
#[tauri::command]
pub async fn node_list_saved_forwards(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
    registry: State<'_, Arc<ForwardingRegistry>>,
    session_tree_state: State<'_, Arc<SessionTreeState>>,
) -> Result<Vec<PersistedForwardDto>, RouteError> {
    let forwards = load_saved_forwards_for_node(
        &node_id,
        router.inner().as_ref(),
        registry.inner().as_ref(),
        session_tree_state.inner().as_ref(),
    )
    .await?;

    Ok(forwards
        .into_iter()
        .map(|f| PersistedForwardDto {
            id: f.id,
            session_id: f.session_id,
            owner_connection_id: f.owner_connection_id,
            forward_type: format!("{:?}", f.forward_type).to_lowercase(),
            bind_address: f.rule.bind_address,
            bind_port: f.rule.bind_port,
            target_host: f.rule.target_host,
            target_port: f.rule.target_port,
            auto_start: f.auto_start,
            created_at: f.created_at.to_rfc3339(),
        })
        .collect())
}
