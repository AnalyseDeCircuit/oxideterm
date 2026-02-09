//! NodeEventEmitter — Oxide-Next Phase 2 事件推送
//!
//! 将后端状态变更以 `NodeStateEvent` 格式推送到前端，
//! 前端通过 `useNodeState` hook 订阅。
//!
//! 设计要点：
//! - 每个事件携带 `generation`（单调递增），前端丢弃乱序事件
//! - 通过 `conn_to_node` 反向映射将 connectionId 转换为 nodeId
//! - AppHandle 延迟设置（Tauri setup 阶段），未就绪时事件丢弃（不缓存，
//!   因为前端初始化时会通过 `node_get_state` 获取快照）
//!
//! 参考: docs/OXIDE_NEXT_ARCHITECTURE.md §3.4

use dashmap::DashMap;
use tauri::AppHandle;
use tracing::{debug, warn};

use super::sequencer::NodeEventSequencer;
use super::types::{NodeReadiness, NodeStateEvent};

use crate::ssh::ConnectionState;

/// 将 ConnectionState 转换为 (NodeReadiness, 错误描述)
///
/// Error 详情通过返回的 reason 字符串传递，不再嵌入 NodeReadiness 枚举。
fn state_to_readiness(state: &ConnectionState) -> (NodeReadiness, Option<String>) {
    match state {
        ConnectionState::Active | ConnectionState::Idle => (NodeReadiness::Ready, None),
        ConnectionState::Connecting | ConnectionState::Reconnecting => {
            (NodeReadiness::Connecting, None)
        }
        ConnectionState::Error(msg) => (NodeReadiness::Error, Some(msg.clone())),
        ConnectionState::LinkDown => {
            (NodeReadiness::Error, Some("Link down".to_string()))
        }
        ConnectionState::Disconnecting | ConnectionState::Disconnected => {
            (NodeReadiness::Disconnected, None)
        }
    }
}

/// 节点事件发射器
///
/// 共享实例（`Arc<NodeEventEmitter>`）分别注入到：
/// - `NodeRouter` — 用于 `get_node_state` 的 generation 查询
/// - `SshConnectionRegistry` — 用于连接状态变更时发射事件
/// - `set_tree_node_connection` 命令 — 用于注册 connectionId → nodeId 映射
pub struct NodeEventEmitter {
    /// Tauri AppHandle，延迟设置
    app_handle: parking_lot::RwLock<Option<AppHandle>>,

    /// generation 管理器
    sequencer: NodeEventSequencer,

    /// connectionId → nodeId 反向映射
    ///
    /// 注册时机：`set_tree_node_connection` 命令
    /// 注销时机：断开连接时
    conn_to_node: DashMap<String, String>,
}

impl NodeEventEmitter {
    pub fn new() -> Self {
        Self {
            app_handle: parking_lot::RwLock::new(None),
            sequencer: NodeEventSequencer::new(),
            conn_to_node: DashMap::new(),
        }
    }

    /// 设置 AppHandle（Tauri setup 阶段调用）
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.write() = Some(handle);
        debug!("NodeEventEmitter: AppHandle set");
    }

    /// 获取事件序列器引用（用于 `node_get_state` 的 generation 查询）
    pub fn sequencer(&self) -> &NodeEventSequencer {
        &self.sequencer
    }

    // ========================================================================
    // 映射管理
    // ========================================================================

    /// 注册 connectionId → nodeId 映射
    ///
    /// 由 `set_tree_node_connection` 命令在前端关联连接到节点时调用。
    pub fn register(&self, connection_id: &str, node_id: &str) {
        debug!(
            "NodeEventEmitter: register {} -> {}",
            connection_id, node_id
        );
        self.conn_to_node
            .insert(connection_id.to_string(), node_id.to_string());
    }

    /// 注销 connectionId 的映射
    ///
    /// 由断开连接流程调用。
    pub fn unregister(&self, connection_id: &str) {
        if let Some((_, node_id)) = self.conn_to_node.remove(connection_id) {
            debug!(
                "NodeEventEmitter: unregister {} (was node {})",
                connection_id, node_id
            );
        }
    }

    /// 通过 connectionId 查询 nodeId
    pub fn get_node_id(&self, connection_id: &str) -> Option<String> {
        self.conn_to_node
            .get(connection_id)
            .map(|r| r.value().clone())
    }

    // ========================================================================
    // 事件发射
    // ========================================================================

    /// 发射连接状态变更事件
    ///
    /// 由 `SshConnectionRegistry` 在 `set_state()` 后调用。
    /// 如果 connectionId 未注册映射，静默跳过（如连接池内部连接）。
    pub fn emit_connection_state_changed(
        &self,
        connection_id: &str,
        state: NodeReadiness,
        reason: &str,
    ) {
        let node_id = match self.get_node_id(connection_id) {
            Some(id) => id,
            None => {
                debug!(
                    "NodeEventEmitter: no nodeId mapping for connection {}, skipping",
                    connection_id
                );
                return;
            }
        };

        let generation = self.sequencer.next(&node_id);

        let event = NodeStateEvent::ConnectionStateChanged {
            node_id,
            generation,
            state,
            reason: reason.to_string(),
        };

        self.emit_event(&event);
    }

    /// 便捷方法：直接从 ConnectionState 发射事件
    ///
    /// 内部转换 ConnectionState → NodeReadiness，错误详情自动提取到 reason。
    pub fn emit_state_from_connection(
        &self,
        connection_id: &str,
        conn_state: &ConnectionState,
        reason: &str,
    ) {
        let (readiness, error_detail) = state_to_readiness(conn_state);
        // 如果 state 自带错误信息且调用者未提供具体 reason，使用错误详情
        let effective_reason = if let Some(detail) = error_detail {
            if reason.is_empty() { detail } else { format!("{}: {}", reason, detail) }
        } else {
            reason.to_string()
        };
        self.emit_connection_state_changed(connection_id, readiness, &effective_reason);
    }

    /// 发射 SFTP 就绪状态变更事件
    ///
    /// 连接级调用（通过 connectionId 查 nodeId）。
    pub fn emit_sftp_ready(
        &self,
        connection_id: &str,
        ready: bool,
        cwd: Option<String>,
    ) {
        let node_id = match self.get_node_id(connection_id) {
            Some(id) => id,
            None => {
                debug!(
                    "NodeEventEmitter: no nodeId mapping for connection {} (sftp), skipping",
                    connection_id
                );
                return;
            }
        };

        let generation = self.sequencer.next(&node_id);

        let event = NodeStateEvent::SftpReady {
            node_id,
            generation,
            ready,
            cwd,
        };

        self.emit_event(&event);
    }

    /// 发射终端端点变更事件
    ///
    /// 由终端重建或重连后调用。
    pub fn emit_terminal_endpoint_changed(
        &self,
        connection_id: &str,
        ws_port: u16,
        ws_token: &str,
    ) {
        let node_id = match self.get_node_id(connection_id) {
            Some(id) => id,
            None => {
                debug!(
                    "NodeEventEmitter: no nodeId mapping for connection {} (terminal), skipping",
                    connection_id
                );
                return;
            }
        };

        let generation = self.sequencer.next(&node_id);

        let event = NodeStateEvent::TerminalEndpointChanged {
            node_id,
            generation,
            ws_port,
            ws_token: ws_token.to_string(),
        };

        self.emit_event(&event);
    }

    /// 内部：通过 AppHandle 发射事件
    fn emit_event(&self, event: &NodeStateEvent) {
        let handle = self.app_handle.read();
        if let Some(ref handle) = *handle {
            use tauri::Emitter;
            if let Err(e) = handle.emit("node:state", event) {
                warn!("NodeEventEmitter: failed to emit node:state: {}", e);
            } else {
                debug!("NodeEventEmitter: emitted {:?}", event);
            }
        } else {
            // AppHandle 未就绪 — 不缓存，前端初始化时通过 node_get_state 快照对齐
            debug!("NodeEventEmitter: AppHandle not ready, dropping event");
        }
    }
}

impl Default for NodeEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_unregister() {
        let emitter = NodeEventEmitter::new();

        emitter.register("conn-1", "node-a");
        assert_eq!(emitter.get_node_id("conn-1"), Some("node-a".to_string()));

        emitter.unregister("conn-1");
        assert_eq!(emitter.get_node_id("conn-1"), None);
    }

    #[test]
    fn test_register_overwrite() {
        let emitter = NodeEventEmitter::new();

        emitter.register("conn-1", "node-a");
        emitter.register("conn-1", "node-b");
        assert_eq!(emitter.get_node_id("conn-1"), Some("node-b".to_string()));
    }

    #[test]
    fn test_emit_without_mapping_is_noop() {
        let emitter = NodeEventEmitter::new();

        // Should not panic, just skip
        emitter.emit_connection_state_changed("unknown-conn", NodeReadiness::Ready, "test");
        emitter.emit_sftp_ready("unknown-conn", true, None);
        emitter.emit_terminal_endpoint_changed("unknown-conn", 8080, "token");
    }

    #[test]
    fn test_emit_without_app_handle_is_noop() {
        let emitter = NodeEventEmitter::new();

        emitter.register("conn-1", "node-a");

        // Should not panic, event dropped
        emitter.emit_connection_state_changed("conn-1", NodeReadiness::Ready, "connected");
        emitter.emit_sftp_ready("conn-1", true, Some("/home/user".to_string()));
    }

    #[test]
    fn test_generation_increments_per_node() {
        let emitter = NodeEventEmitter::new();

        emitter.register("conn-1", "node-a");
        emitter.register("conn-2", "node-b");

        // No AppHandle — events are dropped but generation still increments
        emitter.emit_connection_state_changed("conn-1", NodeReadiness::Connecting, "init");
        assert_eq!(emitter.sequencer().current("node-a"), 1);

        emitter.emit_sftp_ready("conn-1", true, None);
        assert_eq!(emitter.sequencer().current("node-a"), 2);

        // Different node — independent counter
        emitter.emit_connection_state_changed("conn-2", NodeReadiness::Ready, "up");
        assert_eq!(emitter.sequencer().current("node-b"), 1);
        assert_eq!(emitter.sequencer().current("node-a"), 2);
    }
}
