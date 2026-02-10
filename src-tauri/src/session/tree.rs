//! Session Tree - 动态交互式跳板机架构
//!
//! 支持三种跳板机模式:
//! 1. **静态全手工** - 用户手动配置 `proxy_chain: ["A", "B"]`
//! 2. **静态自动计算** - 系统根据网络拓扑自动推算路径
//! 3. **动态钻入** - 运行时用户从已连接节点钻入新服务器
//!
//! # 架构
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │                     SessionTree (会话树)                    │
//! ├────────────────────────────────────────────────────────────┤
//! │  RootNode (本地)                                           │
//! │    ├── [静态手工] jump-01 → bastion → internal-db          │
//! │    ├── [静态自动] A → B → C → gpu-01                       │
//! │    └── [动态钻入] ServerA                                  │
//! │              └── ServerB                                   │
//! │                    └── ServerC                             │
//! └────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::types::AuthMethod;

// ============================================================================
// 核心数据结构
// ============================================================================

/// 会话节点 - 树的基本单元
#[derive(Debug, Clone)]
pub struct SessionNode {
    /// 唯一标识
    pub id: String,

    /// 父节点 ID（None = 直连本地）
    pub parent_id: Option<String>,

    /// 子节点 ID 列表
    pub children_ids: Vec<String>,

    /// 树深度（0 = 直连，1 = 一级跳板，...）
    pub depth: u32,

    /// 连接信息
    pub connection: NodeConnection,

    /// 节点状态
    pub state: NodeState,

    /// 来源类型（区分三种模式）
    pub origin: NodeOrigin,

    /// 关联的终端会话 ID（如果有打开的终端）
    pub terminal_session_id: Option<String>,

    /// 关联的 SFTP 会话 ID（如果有打开的 SFTP）
    pub sftp_session_id: Option<String>,

    /// 关联的 SSH 连接 ID（来自 SshConnectionRegistry）
    pub ssh_connection_id: Option<String>,

    /// 创建时间
    pub created_at: chrono::DateTime<Utc>,
}

/// 节点连接信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConnection {
    /// 主机名或 IP
    pub host: String,
    /// 端口
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 认证方式（密码/密钥/Agent）
    pub auth: AuthMethod,
    /// 可选的显示名称
    pub display_name: Option<String>,
}

impl NodeConnection {
    /// 创建新的节点连接信息
    pub fn new(host: impl Into<String>, port: u16, username: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            auth: AuthMethod::Agent,
            display_name: None,
        }
    }

    /// 设置密码认证
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.auth = AuthMethod::Password {
            password: password.into(),
        };
        self
    }

    /// 设置密钥认证
    pub fn with_key(mut self, key_path: impl Into<String>, passphrase: Option<String>) -> Self {
        self.auth = AuthMethod::Key {
            key_path: key_path.into(),
            passphrase,
        };
        self
    }

    /// 设置显示名称
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// 获取显示标签
    pub fn display_label(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", self.username, self.host))
    }
}

/// 节点来源 - 区分三种跳板机模式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeOrigin {
    /// 【模式1】静态全手工 - 用户手动配置的跳板链
    ManualPreset {
        /// 保存的连接 ID
        saved_connection_id: String,
        /// 在链中的位置（0 = 第一跳，最后 = 目标）
        hop_index: u32,
    },

    /// 【模式2】静态自动计算 - 系统根据网络拓扑自动生成
    AutoRoute {
        /// 目标服务器
        target_host: String,
        /// 计算出的路径 ID（用于缓存/调试）
        route_id: String,
        /// 在路径中的位置
        hop_index: u32,
    },

    /// 【模式3】动态钻入 - 用户运行时手动钻入
    DrillDown {
        /// 钻入时间戳
        timestamp: i64,
    },

    /// 直接连接（无跳板）
    Direct,

    /// 从配置恢复（应用重启后）
    Restored {
        /// 保存的连接 ID
        saved_connection_id: String,
    },
}

impl NodeOrigin {
    /// 获取来源类型字符串（用于前端显示）
    pub fn origin_type(&self) -> &'static str {
        match self {
            NodeOrigin::ManualPreset { .. } => "manual_preset",
            NodeOrigin::AutoRoute { .. } => "auto_route",
            NodeOrigin::DrillDown { .. } => "drill_down",
            NodeOrigin::Direct => "direct",
            NodeOrigin::Restored { .. } => "restored",
        }
    }
}

/// 节点状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NodeState {
    /// 未连接（预设节点，尚未建立连接）
    Pending,
    /// 连接中
    Connecting,
    /// 已连接
    Connected,
    /// 连接断开，可重连
    Disconnected,
    /// 连接失败
    Failed { error: String },
}

impl NodeState {
    /// 是否已连接
    pub fn is_connected(&self) -> bool {
        matches!(self, NodeState::Connected)
    }

    /// 是否可以发起连接
    pub fn can_connect(&self) -> bool {
        matches!(
            self,
            NodeState::Pending | NodeState::Disconnected | NodeState::Failed { .. }
        )
    }
}

// ============================================================================
// 会话树
// ============================================================================

/// 会话树 - 管理所有会话节点
pub struct SessionTree {
    /// 所有节点（ID -> Node）
    nodes: HashMap<String, SessionNode>,

    /// 根节点 ID 列表（depth=0 的节点）
    root_ids: Vec<String>,
}

impl Default for SessionTree {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionTree {
    /// 创建空的会话树
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root_ids: Vec::new(),
        }
    }

    /// 获取节点数量
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// 获取所有节点 ID 的迭代器
    pub fn node_ids(&self) -> impl Iterator<Item = String> + '_ {
        self.nodes.keys().cloned()
    }

    /// 获取节点
    pub fn get_node(&self, id: &str) -> Option<&SessionNode> {
        self.nodes.get(id)
    }

    /// 获取节点（可变）
    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut SessionNode> {
        self.nodes.get_mut(id)
    }

    /// 获取所有根节点
    pub fn root_nodes(&self) -> Vec<&SessionNode> {
        self.root_ids
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .collect()
    }

    /// 添加直连节点（depth=0）
    pub fn add_root_node(&mut self, connection: NodeConnection, origin: NodeOrigin) -> String {
        let id = Uuid::new_v4().to_string();
        let node = SessionNode {
            id: id.clone(),
            parent_id: None,
            children_ids: Vec::new(),
            depth: 0,
            connection,
            state: NodeState::Pending,
            origin,
            terminal_session_id: None,
            sftp_session_id: None,
            ssh_connection_id: None,
            created_at: Utc::now(),
        };
        self.nodes.insert(id.clone(), node);
        self.root_ids.push(id.clone());
        id
    }

    /// 从父节点钻入，添加子节点（模式3: 动态钻入）
    pub fn drill_down(
        &mut self,
        parent_id: &str,
        connection: NodeConnection,
    ) -> Result<String, TreeError> {
        let parent = self
            .nodes
            .get(parent_id)
            .ok_or_else(|| TreeError::NodeNotFound(parent_id.to_string()))?;

        if !parent.state.is_connected() {
            return Err(TreeError::ParentNotConnected(parent_id.to_string()));
        }

        let depth = parent.depth + 1;
        let id = Uuid::new_v4().to_string();

        let node = SessionNode {
            id: id.clone(),
            parent_id: Some(parent_id.to_string()),
            children_ids: Vec::new(),
            depth,
            connection,
            state: NodeState::Pending,
            origin: NodeOrigin::DrillDown {
                timestamp: Utc::now().timestamp(),
            },
            terminal_session_id: None,
            sftp_session_id: None,
            ssh_connection_id: None,
            created_at: Utc::now(),
        };

        self.nodes.insert(id.clone(), node);

        // 更新父节点的 children_ids
        if let Some(parent) = self.nodes.get_mut(parent_id) {
            parent.children_ids.push(id.clone());
        }

        Ok(id)
    }

    /// 展开静态手工预设链为树节点（模式1）
    pub fn expand_manual_preset(
        &mut self,
        saved_connection_id: &str,
        hops: Vec<NodeConnection>,
        target: NodeConnection,
    ) -> Result<String, TreeError> {
        self.expand_preset_chain_internal(hops, target, |hop_index| NodeOrigin::ManualPreset {
            saved_connection_id: saved_connection_id.to_string(),
            hop_index,
        })
    }

    /// 展开自动计算路径为树节点（模式2）
    pub fn expand_auto_route(
        &mut self,
        target_host: &str,
        route_id: &str,
        hops: Vec<NodeConnection>,
        target: NodeConnection,
    ) -> Result<String, TreeError> {
        self.expand_preset_chain_internal(hops, target, |hop_index| NodeOrigin::AutoRoute {
            target_host: target_host.to_string(),
            route_id: route_id.to_string(),
            hop_index,
        })
    }

    /// 内部：展开预设链为树节点
    fn expand_preset_chain_internal<F>(
        &mut self,
        hops: Vec<NodeConnection>,
        target: NodeConnection,
        origin_factory: F,
    ) -> Result<String, TreeError>
    where
        F: Fn(u32) -> NodeOrigin,
    {
        if hops.is_empty() {
            // 无跳板，直接添加目标
            return Ok(self.add_root_node(target, NodeOrigin::Direct));
        }

        // 第一跳作为根节点
        let first_hop_id = self.add_root_node(hops[0].clone(), origin_factory(0));
        let mut current_id = first_hop_id;

        // 后续跳板
        for (index, hop) in hops.iter().skip(1).enumerate() {
            let hop_index = (index + 1) as u32;
            let new_id = Uuid::new_v4().to_string();

            let node = SessionNode {
                id: new_id.clone(),
                parent_id: Some(current_id.clone()),
                children_ids: Vec::new(),
                depth: hop_index,
                connection: hop.clone(),
                state: NodeState::Pending,
                origin: origin_factory(hop_index),
                terminal_session_id: None,
                sftp_session_id: None,
                ssh_connection_id: None,
                created_at: Utc::now(),
            };

            if let Some(parent) = self.nodes.get_mut(&current_id) {
                parent.children_ids.push(new_id.clone());
            }
            self.nodes.insert(new_id.clone(), node);
            current_id = new_id;
        }

        // 目标服务器
        let target_hop_index = hops.len() as u32;
        let target_id = Uuid::new_v4().to_string();

        let target_node = SessionNode {
            id: target_id.clone(),
            parent_id: Some(current_id.clone()),
            children_ids: Vec::new(),
            depth: target_hop_index,
            connection: target,
            state: NodeState::Pending,
            origin: origin_factory(target_hop_index),
            terminal_session_id: None,
            sftp_session_id: None,
            ssh_connection_id: None,
            created_at: Utc::now(),
        };

        if let Some(parent) = self.nodes.get_mut(&current_id) {
            parent.children_ids.push(target_id.clone());
        }
        self.nodes.insert(target_id.clone(), target_node);

        Ok(target_id)
    }

    /// 更新节点状态
    pub fn update_state(&mut self, node_id: &str, new_state: NodeState) -> Result<(), TreeError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| TreeError::NodeNotFound(node_id.to_string()))?;
        node.state = new_state;
        Ok(())
    }

    /// 关联 SSH 连接 ID
    pub fn set_ssh_connection_id(
        &mut self,
        node_id: &str,
        connection_id: String,
    ) -> Result<(), TreeError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| TreeError::NodeNotFound(node_id.to_string()))?;
        node.ssh_connection_id = Some(connection_id);
        Ok(())
    }

    /// 关联终端会话 ID
    pub fn set_terminal_session_id(
        &mut self,
        node_id: &str,
        session_id: String,
    ) -> Result<(), TreeError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| TreeError::NodeNotFound(node_id.to_string()))?;
        node.terminal_session_id = Some(session_id);
        Ok(())
    }

    /// 关联 SFTP 会话 ID
    pub fn set_sftp_session_id(
        &mut self,
        node_id: &str,
        session_id: String,
    ) -> Result<(), TreeError> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| TreeError::NodeNotFound(node_id.to_string()))?;
        node.sftp_session_id = Some(session_id);
        Ok(())
    }

    /// 移除节点（递归移除所有子节点）
    pub fn remove_node(&mut self, node_id: &str) -> Result<Vec<String>, TreeError> {
        let mut removed_ids = Vec::new();
        self.remove_node_recursive(node_id, &mut removed_ids)?;
        Ok(removed_ids)
    }

    fn remove_node_recursive(
        &mut self,
        node_id: &str,
        removed_ids: &mut Vec<String>,
    ) -> Result<(), TreeError> {
        let node = self
            .nodes
            .get(node_id)
            .ok_or_else(|| TreeError::NodeNotFound(node_id.to_string()))?;

        let children_ids = node.children_ids.clone();
        let parent_id = node.parent_id.clone();

        // 递归移除子节点
        for child_id in children_ids {
            self.remove_node_recursive(&child_id, removed_ids)?;
        }

        // 从父节点的 children_ids 中移除
        if let Some(parent_id) = parent_id {
            if let Some(parent) = self.nodes.get_mut(&parent_id) {
                parent.children_ids.retain(|id| id != node_id);
            }
        } else {
            // 是根节点，从 root_ids 中移除
            self.root_ids.retain(|id| id != node_id);
        }

        // 移除节点
        self.nodes.remove(node_id);
        removed_ids.push(node_id.to_string());

        Ok(())
    }

    /// 获取节点的所有祖先（从父到根）
    pub fn get_ancestors(&self, node_id: &str) -> Vec<&SessionNode> {
        let mut ancestors = Vec::new();
        let mut current_id = node_id;

        while let Some(node) = self.nodes.get(current_id) {
            if let Some(ref parent_id) = node.parent_id {
                if let Some(parent) = self.nodes.get(parent_id) {
                    ancestors.push(parent);
                    current_id = parent_id;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        ancestors
    }

    /// 获取节点的所有后代（深度优先）
    pub fn get_descendants(&self, node_id: &str) -> Vec<&SessionNode> {
        let mut descendants = Vec::new();
        self.collect_descendants(node_id, &mut descendants);
        descendants
    }

    fn collect_descendants<'a>(&'a self, node_id: &str, result: &mut Vec<&'a SessionNode>) {
        if let Some(node) = self.nodes.get(node_id) {
            for child_id in &node.children_ids {
                if let Some(child) = self.nodes.get(child_id) {
                    result.push(child);
                    self.collect_descendants(child_id, result);
                }
            }
        }
    }

    /// 获取从根到目标节点的完整路径
    pub fn get_path_to_node(&self, node_id: &str) -> Vec<&SessionNode> {
        let mut path = self.get_ancestors(node_id);
        path.reverse(); // 从根到目标

        if let Some(node) = self.nodes.get(node_id) {
            path.push(node);
        }

        path
    }

    /// 扁平化输出（用于前端渲染）
    pub fn flatten(&self) -> Vec<FlatNode> {
        let mut result = Vec::new();

        for root_id in &self.root_ids {
            self.flatten_recursive(root_id, &mut result);
        }

        result
    }

    fn flatten_recursive(&self, node_id: &str, result: &mut Vec<FlatNode>) {
        if let Some(node) = self.nodes.get(node_id) {
            result.push(FlatNode::from_node(node, self.is_last_child(node)));

            for child_id in &node.children_ids {
                self.flatten_recursive(child_id, result);
            }
        }
    }

    fn is_last_child(&self, node: &SessionNode) -> bool {
        if let Some(parent_id) = &node.parent_id {
            if let Some(parent) = self.nodes.get(parent_id) {
                return parent.children_ids.last() == Some(&node.id);
            }
        }
        // 根节点也检查是否是最后一个
        self.root_ids.last() == Some(&node.id)
    }
}

// ============================================================================
// 扁平化节点（用于前端）
// ============================================================================

/// 扁平化节点 - 用于前端渲染
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatNode {
    /// 节点 ID
    pub id: String,
    /// 父节点 ID
    pub parent_id: Option<String>,
    /// 树深度
    pub depth: u32,
    /// 主机名
    pub host: String,
    /// 端口
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 显示名称
    pub display_name: Option<String>,
    /// 节点状态
    pub state: FlatNodeState,
    /// 是否有子节点
    pub has_children: bool,
    /// 是否是最后一个子节点
    pub is_last_child: bool,
    /// 来源类型
    pub origin_type: String,
    /// 关联的终端会话 ID
    pub terminal_session_id: Option<String>,
    /// 关联的 SFTP 会话 ID
    pub sftp_session_id: Option<String>,
    /// 关联的 SSH 连接 ID
    pub ssh_connection_id: Option<String>,
}

/// 扁平化节点状态（简化版，用于前端）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum FlatNodeState {
    Pending,
    Connecting,
    Connected,
    Disconnected,
    Failed { error: String },
}

impl From<&NodeState> for FlatNodeState {
    fn from(state: &NodeState) -> Self {
        match state {
            NodeState::Pending => FlatNodeState::Pending,
            NodeState::Connecting => FlatNodeState::Connecting,
            NodeState::Connected => FlatNodeState::Connected,
            NodeState::Disconnected => FlatNodeState::Disconnected,
            NodeState::Failed { error } => FlatNodeState::Failed {
                error: error.clone(),
            },
        }
    }
}

impl FlatNode {
    /// 从 SessionNode 创建 FlatNode
    pub fn from_node(node: &SessionNode, is_last_child: bool) -> Self {
        Self {
            id: node.id.clone(),
            parent_id: node.parent_id.clone(),
            depth: node.depth,
            host: node.connection.host.clone(),
            port: node.connection.port,
            username: node.connection.username.clone(),
            display_name: node.connection.display_name.clone(),
            state: FlatNodeState::from(&node.state),
            has_children: !node.children_ids.is_empty(),
            is_last_child,
            origin_type: node.origin.origin_type().to_string(),
            terminal_session_id: node.terminal_session_id.clone(),
            sftp_session_id: node.sftp_session_id.clone(),
            ssh_connection_id: node.ssh_connection_id.clone(),
        }
    }
}

// ============================================================================
// 错误类型
// ============================================================================

/// 会话树错误
#[derive(Debug, Clone, thiserror::Error, Serialize)]
pub enum TreeError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Parent node not connected: {0}")]
    ParentNotConnected(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_connection(host: &str) -> NodeConnection {
        NodeConnection::new(host, 22, "user")
    }

    #[test]
    fn test_add_root_node() {
        let mut tree = SessionTree::new();
        let id = tree.add_root_node(make_connection("server-a"), NodeOrigin::Direct);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree.root_ids.len(), 1);

        let node = tree.get_node(&id).unwrap();
        assert_eq!(node.depth, 0);
        assert!(node.parent_id.is_none());
    }

    #[test]
    fn test_drill_down() {
        let mut tree = SessionTree::new();
        let root_id = tree.add_root_node(make_connection("server-a"), NodeOrigin::Direct);

        // 模拟连接成功
        tree.update_state(&root_id, NodeState::Connected).unwrap();

        // 钻入
        let child_id = tree
            .drill_down(&root_id, make_connection("server-b"))
            .unwrap();

        assert_eq!(tree.len(), 2);

        let child = tree.get_node(&child_id).unwrap();
        assert_eq!(child.depth, 1);
        assert_eq!(child.parent_id, Some(root_id.clone()));

        let root = tree.get_node(&root_id).unwrap();
        assert!(root.children_ids.contains(&child_id));
    }

    #[test]
    fn test_drill_down_not_connected() {
        let mut tree = SessionTree::new();
        let root_id = tree.add_root_node(make_connection("server-a"), NodeOrigin::Direct);

        // 未连接时尝试钻入
        let result = tree.drill_down(&root_id, make_connection("server-b"));
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_manual_preset() {
        let mut tree = SessionTree::new();

        let hops = vec![make_connection("jump-01"), make_connection("bastion")];
        let target = make_connection("internal-db");

        let target_id = tree
            .expand_manual_preset("saved-conn-123", hops, target)
            .unwrap();

        assert_eq!(tree.len(), 3);
        assert_eq!(tree.root_ids.len(), 1);

        let target_node = tree.get_node(&target_id).unwrap();
        assert_eq!(target_node.depth, 2);
        assert_eq!(target_node.connection.host, "internal-db");

        // 检查路径
        let path = tree.get_path_to_node(&target_id);
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].connection.host, "jump-01");
        assert_eq!(path[1].connection.host, "bastion");
        assert_eq!(path[2].connection.host, "internal-db");
    }

    #[test]
    fn test_flatten() {
        let mut tree = SessionTree::new();

        // 创建一个简单的树
        let root_id = tree.add_root_node(make_connection("server-a"), NodeOrigin::Direct);
        tree.update_state(&root_id, NodeState::Connected).unwrap();

        let child1_id = tree
            .drill_down(&root_id, make_connection("server-b"))
            .unwrap();
        tree.update_state(&child1_id, NodeState::Connected).unwrap();

        let _child2_id = tree
            .drill_down(&root_id, make_connection("server-c"))
            .unwrap();

        let flat = tree.flatten();
        assert_eq!(flat.len(), 3);

        // 检查深度
        assert_eq!(flat[0].depth, 0);
        assert_eq!(flat[1].depth, 1);
        assert_eq!(flat[2].depth, 1);

        // 检查 is_last_child
        assert!(flat[0].is_last_child); // 唯一的根节点
        assert!(!flat[1].has_children); // server-b 没有子节点
    }

    #[test]
    fn test_remove_node_cascade() {
        let mut tree = SessionTree::new();

        let root_id = tree.add_root_node(make_connection("server-a"), NodeOrigin::Direct);
        tree.update_state(&root_id, NodeState::Connected).unwrap();

        let child_id = tree
            .drill_down(&root_id, make_connection("server-b"))
            .unwrap();
        tree.update_state(&child_id, NodeState::Connected).unwrap();

        let grandchild_id = tree
            .drill_down(&child_id, make_connection("server-c"))
            .unwrap();

        assert_eq!(tree.len(), 3);

        // 移除子节点（应该级联移除孙节点）
        let removed = tree.remove_node(&child_id).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&child_id));
        assert!(removed.contains(&grandchild_id));

        assert_eq!(tree.len(), 1);

        // 检查根节点的 children_ids 已更新
        let root = tree.get_node(&root_id).unwrap();
        assert!(root.children_ids.is_empty());
    }
}
