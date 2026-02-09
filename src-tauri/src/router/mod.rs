//! Oxide-Next NodeRouter — Phase 2
//!
//! 节点路由器：将 nodeId 解析为具体的后端资源。
//! 所有 node_* Tauri IPC 命令经由 NodeRouter 寻址，前端不再传递
//! sessionId / connectionId。
//!
//! **Phase 2 变更**: NodeRouter 使用共享的 NodeEventEmitter，
//! 而非直接持有 NodeEventSequencer。Emitter 同时注入到
//! SshConnectionRegistry 和 set_tree_node_connection 命令。
//!
//! 参考: docs/OXIDE_NEXT_ARCHITECTURE.md §3.1, §3.4

mod emitter;
mod sequencer;
mod types;

pub use emitter::NodeEventEmitter;
pub use sequencer::NodeEventSequencer;
pub use types::*;

use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;
use tracing::debug;

use crate::commands::SessionTreeState;
use crate::session::SessionRegistry;
use crate::sftp::session::SftpSession;
use crate::ssh::{ConnectionState, SshConnectionRegistry};

/// 节点路由器：将 nodeId 解析为具体的后端资源。
///
/// **设计原则** (RFC §1):
/// - 前端只传 nodeId，所有 ID 解析在此完成
/// - SFTP session 的唯一真源在 `ConnectionEntry.sftp`（Phase 1.5）
/// - 路由解析在单个 async task 内完成，无中间状态暴露
/// - Phase 2: 事件推送通过共享的 `NodeEventEmitter` 完成
pub struct NodeRouter {
    session_tree: Arc<SessionTreeState>,
    connection_registry: Arc<SshConnectionRegistry>,
    session_registry: Arc<SessionRegistry>,
    emitter: Arc<NodeEventEmitter>,
}

impl NodeRouter {
    /// 创建 NodeRouter 实例
    pub fn new(
        session_tree: Arc<SessionTreeState>,
        connection_registry: Arc<SshConnectionRegistry>,
        session_registry: Arc<SessionRegistry>,
        emitter: Arc<NodeEventEmitter>,
    ) -> Self {
        Self {
            session_tree,
            connection_registry,
            session_registry,
            emitter,
        }
    }

    /// 获取共享的事件发射器
    pub fn emitter(&self) -> &Arc<NodeEventEmitter> {
        &self.emitter
    }

    /// 获取事件序列器引用（快捷方式）
    pub fn sequencer(&self) -> &NodeEventSequencer {
        self.emitter.sequencer()
    }

    // ========================================================================
    // 核心路由方法
    // ========================================================================

    /// 解析 nodeId 到 SSH connectionId。
    ///
    /// 内部处理：SessionTree 查找、状态门禁、重连等待。
    /// 返回 (connectionId, HandleController) 以供后续操作。
    pub async fn resolve_connection(
        &self,
        node_id: &str,
    ) -> Result<ResolvedConnection, RouteError> {
        // Step 1: nodeId → SessionNode
        let tree = self.session_tree.tree.read().await;
        let node = tree
            .get_node(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.into()))?;

        let conn_id = node
            .ssh_connection_id
            .as_ref()
            .ok_or_else(|| RouteError::NotConnected(node_id.into()))?
            .clone();

        let terminal_session_id = node.terminal_session_id.clone();
        let sftp_session_id = node.sftp_session_id.clone();

        // 释放 tree read lock
        drop(tree);

        // Step 2: connectionId → ConnectionEntry
        let entry = self
            .connection_registry
            .get_connection(&conn_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.into()))?;

        // Step 3: 状态门禁
        let state = entry.state().await;
        match state {
            ConnectionState::Active | ConnectionState::Idle => {
                Ok(ResolvedConnection {
                    connection_id: conn_id,
                    handle_controller: entry.handle_controller.clone(),
                    terminal_session_id,
                    sftp_session_id,
                })
            }
            ConnectionState::Reconnecting | ConnectionState::Connecting => {
                // 等待连接就绪（带超时）
                debug!(
                    "Node {} connection {} is {:?}, waiting...",
                    node_id, conn_id, state
                );
                self.wait_for_active(&conn_id, Duration::from_secs(15))
                    .await?;

                // 重新获取（连接可能已更新）
                let entry = self
                    .connection_registry
                    .get_connection(&conn_id)
                    .ok_or_else(|| RouteError::NotConnected(node_id.into()))?;

                Ok(ResolvedConnection {
                    connection_id: conn_id,
                    handle_controller: entry.handle_controller.clone(),
                    terminal_session_id,
                    sftp_session_id,
                })
            }
            ConnectionState::Error(msg) => Err(RouteError::ConnectionError(msg)),
            ConnectionState::LinkDown => {
                Err(RouteError::NotConnected(format!(
                    "Node {} connection {} is link_down",
                    node_id, conn_id
                )))
            }
            _ => Err(RouteError::NotConnected(node_id.into())),
        }
    }

    /// 获取或创建该节点的 SFTP session。
    ///
    /// **Phase 1.5**: 委托给 `ConnectionEntry.acquire_sftp()`。
    /// SFTP session 存在且仅存在于 `ConnectionEntry.sftp` 字段中，
    /// NodeRouter 不持有任何 SFTP 索引。
    ///
    /// **Phase 2**: 首次创建 SFTP 后发射 SftpReady 事件。
    pub async fn acquire_sftp(
        &self,
        node_id: &str,
    ) -> Result<Arc<tokio::sync::Mutex<SftpSession>>, RouteError> {
        let resolved = self.resolve_connection(node_id).await?;

        // 查询 SFTP 是否已存在（用于判断是否新建）
        let entry = self
            .connection_registry
            .get_connection(&resolved.connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.into()))?;

        let was_new = !entry.has_sftp().await;

        let sftp = entry
            .acquire_sftp()
            .await
            .map_err(|e| RouteError::CapabilityUnavailable(format!("SFTP init failed: {}", e)))?;

        // Phase 2: 如果是新建的 SFTP，发射 SftpReady 事件
        if was_new {
            let cwd = entry.sftp_cwd().await;
            self.emitter.emit_sftp_ready(
                &resolved.connection_id,
                true,
                cwd,
            );
        }

        Ok(sftp)
    }

    /// 获取该节点的 WebSocket 终端 URL。
    ///
    /// 如果终端 session 已销毁（session 漂移/重连后 PTY 未重建），
    /// 返回 NotConnected 错误。
    ///
    /// **Phase 0**: 不自动重建，仅查询。
    /// 终端自动重建将在 Phase 2+ 实现（需要 BridgeManager 配合）。
    pub async fn terminal_url(
        &self,
        node_id: &str,
    ) -> Result<TerminalEndpoint, RouteError> {
        let resolved = self.resolve_connection(node_id).await?;

        // 尝试获取现有终端 session
        if let Some(ref session_id) = resolved.terminal_session_id {
            let endpoint = self.session_registry.with_session(session_id, |entry| {
                match (entry.ws_port, entry.ws_token.as_ref()) {
                    (Some(port), Some(token)) => Some(TerminalEndpoint {
                        ws_port: port,
                        ws_token: token.clone(),
                        session_id: session_id.clone(),
                    }),
                    _ => None,
                }
            }).flatten();

            if let Some(ep) = endpoint {
                return Ok(ep);
            }
        }

        Err(RouteError::NotConnected(format!(
            "No active terminal session for node {}",
            node_id
        )))
    }

    /// 获取节点当前状态快照（含 generation，用于前端初始对齐）。
    pub async fn get_node_state(&self, node_id: &str) -> Result<NodeStateSnapshot, RouteError> {
        let tree = self.session_tree.tree.read().await;
        let node = tree
            .get_node(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.into()))?;

        // 收集节点 ID 信息后释放锁
        let conn_id = node.ssh_connection_id.clone();
        let _sftp_id = node.sftp_session_id.clone();
        let node_state = node.state.clone();
        drop(tree);

        // 确定 readiness
        let readiness = if let Some(ref cid) = conn_id {
            if let Some(entry) = self.connection_registry.get_connection(cid) {
                match entry.state().await {
                    ConnectionState::Active | ConnectionState::Idle => NodeReadiness::Ready,
                    ConnectionState::Connecting | ConnectionState::Reconnecting => {
                        NodeReadiness::Connecting
                    }
                    ConnectionState::Error(_msg) => NodeReadiness::Error,
                    ConnectionState::LinkDown => NodeReadiness::Error,
                    ConnectionState::Disconnected | ConnectionState::Disconnecting => {
                        NodeReadiness::Disconnected
                    }
                }
            } else {
                NodeReadiness::Disconnected
            }
        } else {
            // 没有 connection_id，看 SessionTree node state
            use crate::session::tree::NodeState as TreeNodeState;
            match node_state {
                TreeNodeState::Connecting => NodeReadiness::Connecting,
                TreeNodeState::Connected => NodeReadiness::Ready,
                TreeNodeState::Failed { .. } => NodeReadiness::Error,
                TreeNodeState::Disconnected => NodeReadiness::Disconnected,
                TreeNodeState::Pending => NodeReadiness::Disconnected,
            }
        };

        // SFTP 就绪状态 — Phase 1.5: 从 ConnectionEntry 查询
        let (sftp_ready, sftp_cwd) = if let Some(ref cid) = conn_id {
            if let Some(entry) = self.connection_registry.get_connection(cid) {
                let ready = entry.has_sftp().await;
                let cwd = entry.sftp_cwd().await;
                (ready, cwd)
            } else {
                (false, None)
            }
        } else {
            (false, None)
        };

        // 终端端点
        let ws_endpoint = if conn_id.is_some() {
            // 从 SessionTree 获取 terminal_session_id
            let tree = self.session_tree.tree.read().await;
            let tsid = tree
                .get_node(node_id)
                .and_then(|n| n.terminal_session_id.clone());
            drop(tree);

            if let Some(ref sid) = tsid {
                let endpoint = self.session_registry.with_session(sid, |entry| {
                    match (entry.ws_port, entry.ws_token.as_ref()) {
                        (Some(port), Some(token)) => Some(TerminalEndpoint {
                            ws_port: port,
                            ws_token: token.clone(),
                            session_id: sid.clone(),
                        }),
                        _ => None,
                    }
                }).flatten();
                endpoint
            } else {
                None
            }
        } else {
            None
        };

        let generation = self.sequencer().current(node_id);

        Ok(NodeStateSnapshot {
            state: NodeState {
                error: if readiness == NodeReadiness::Error {
                    // 从连接或树节点提取错误详情
                    if let Some(ref cid) = conn_id {
                        if let Some(entry) = self.connection_registry.get_connection(cid) {
                            match entry.state().await {
                                ConnectionState::Error(msg) => Some(msg),
                                ConnectionState::LinkDown => Some("Link down".into()),
                                _ => None,
                            }
                        } else { None }
                    } else {
                        // 从 TreeNodeState 提取
                        let tree2 = self.session_tree.tree.read().await;
                        tree2.get_node(node_id)
                            .and_then(|n| match &n.state {
                                crate::session::tree::NodeState::Failed { error } => Some(error.clone()),
                                _ => None,
                            })
                    }
                } else { None },
                readiness,
                sftp_ready,
                sftp_cwd,
                ws_endpoint,
            },
            generation,
        })
    }

    // ========================================================================
    // 内部辅助方法
    // ========================================================================

    /// 等待连接变为 Active 状态（带超时）。
    async fn wait_for_active(
        &self,
        connection_id: &str,
        max_wait: Duration,
    ) -> Result<(), RouteError> {
        let poll_interval = Duration::from_millis(200);

        let result = timeout(max_wait, async {
            loop {
                if let Some(entry) = self.connection_registry.get_connection(connection_id) {
                    let state = entry.state().await;
                    match state {
                        ConnectionState::Active | ConnectionState::Idle => return Ok(()),
                        ConnectionState::Error(msg) => {
                            return Err(RouteError::ConnectionError(msg))
                        }
                        ConnectionState::Disconnected | ConnectionState::Disconnecting => {
                            return Err(RouteError::NotConnected(connection_id.into()))
                        }
                        _ => {
                            // Still connecting/reconnecting, wait
                            tokio::time::sleep(poll_interval).await;
                        }
                    }
                } else {
                    return Err(RouteError::NotConnected(connection_id.into()));
                }
            }
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => Err(RouteError::ConnectionTimeout(format!(
                "Timed out waiting for connection {} to become active ({:?})",
                connection_id, max_wait
            ))),
        }
    }
}
