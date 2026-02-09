# Oxide-Next：节点主权架构方案

> **版本**: v2.0 (Post-Implementation)  
> **日期**: 2026-02-09  
> **状态**: ✅ 已实施（Phase 0 ~ Phase 5 全部完成）  
> **实施记录**:  
> - Phase 0: NodeRouter + NodeEventSequencer + types + 12 node_* commands  
> - Phase 1: 21 旧 sftp_* commands 标记 #[deprecated] + legacy_acquire_sftp 适配器  
> - Phase 1.5: SFTP 生命周期迁移到 ConnectionEntry  
> - Phase 2: NodeEventEmitter + 13 个状态变更点的事件发射  
> - Phase 3: 前端 API 层 (`nodeSftp*`) + `useNodeState` hook  
> - Phase 4: 全部前端组件迁移 (SFTPView, TransferQueue, IdeWorkspace 等)  
> - Phase 4.5: 插件 API 迁移 (nodeId 适配层 + sessionId @deprecated)  
> - Phase 5: 清理 — 删除 virtualSessionStore, useNodeSession, connectionGuard, 旧 sftp_* 命令, legacy_acquire_sftp  
> - Bug fix: RouteError 新增 SftpOperationError variant，正确区分操作错误与能力不可用  
> **v1.3 变更**: 消除重复 acquire_sftp 定义、修正 TypeScript 类型。  
> **v1.2 变更**: SFTP 唯一真源统一（移除 NodeRouter.sftp_registry）、generation 初始快照对齐、TerminalEndpointChanged 前端消费契约。  
> **v1.1 变更**: 修正插件兼容面、Phase 风险描述、事件有序性协议、终端重建分支、旧命令适配路径、验收矩阵。  
> **目标**: 从地基阶段消灭 Session 耦合、跨端状态不一致、僵尸通道三大类 Race Condition。
>
> ### 实施后残留事项
> - `refreshConnections()` 已 @deprecated 但仍有 ~10 处活跃调用（初始加载 + 显式刷新），后续版本逐步移除  
> - `SftpRegistry` 仍存在 — ide.rs, ssh.rs, connect_v2.rs 仍通过旧路径访问，待这些模块迁移  
> - 插件 sessionId 适配层保留至下一个大版本 (v2.0)  
> - 重连机制中 `topologyResolver` 仍使用 connectionId 做物理↔逻辑映射（这是连接基础设施层，非用户面代码）

---

## 0. 问题根因分析

当前架构的 Race Condition **不是实现 Bug，而是模型缺陷**。补丁越打越多，因为问题出在 ID 拓扑本身。

### 0.1 当前 ID 拓扑

```
Frontend (React)                         Backend (Rust)
─────────────────                        ──────────────
nodeId (稳定)                            nodeId (SessionTree)
  ├→ virtualSessionStore                   ├→ ssh_connection_id → ConnectionEntry
  │    ├→ activeSessionId ──────────────────┤→ terminal_session_id → SessionEntry
  │    ├→ connectionId ─────────────────────┤→ sftp_session_id → SftpRegistry
  │    └→ generation                       └→ forward_ids
  │
  └→ useNodeSession() hook
       └→ resolved { sessionId, connectionId }
            │
            ├→ SFTPView: api.sftpInit(sessionId) ← 这里断裂
            ├→ RemoteFileEditor: api.sftpWriteContent(sessionId) ← 这里断裂
            └→ IdeWorkspace: openProject(connectionId, sessionId) ← 这里断裂
```

### 0.2 五个结构性缺陷

| # | 缺陷 | 根因 | 症状 |
|---|------|------|------|
| **D1** | **前端持有 sessionId 作路由键** | SFTP/IDE API 以 `sessionId` 寻址，但 sessionId 是终端 PTY 句柄，不是 SFTP 通道句柄 | 闭包捕获旧 sessionId → 写入死会话 |
| **D2** | **前端负责 ID 解析** | `guardNodeCapability` 在前端做 nodeId→sessionId 解析，返回值须手动传播 | 20+ 处 handler 要逐个修复 |
| **D3** | **双 Store 同步缝隙** | `sessionTreeStore`（意图）与 `appStore`（事实）通过 `refreshConnections()` 异步同步 | `refreshConnections` 延迟窗口内决策错误 |
| **D4** | **SFTP 生命周期绑定终端** | `SftpRegistry` key = `sessionId`；SFTP 实际依赖 SSH 连接，不依赖终端 | 终端重建→SFTP 失效→僵尸通道 |
| **D5** | **无后端路由层** | 每个 IPC 命令自带 `session_id` 参数，后端不做 nodeId 查找 | 前端必须维护完整映射，任何缝隙都是 Race |

### 0.3 核心洞察

> **前端的 SFTP / IDE 操作不应该知道 sessionId / connectionId 的存在。**
>
> SFTP 和 IDE 全部通过 **nodeId** 寻址，由后端 NodeRouter 完成 ID 解析。  
> 当 SSH 重连导致 connectionId 改变时，SFTP/IDE 层无感知。
>
> **例外：终端层** — TerminalView 仍持有 `connectionIdRef` 用于过滤
> `connection_status_changed` 事件（link_down / reconnecting 锁屏）。  
> 这是基础设施层需求：事件载荷是 connectionId，终端必须匹配。  
> 终端的 WebSocket 连接管理（Key-Driven Reset）也依赖 session.connectionId 触发重建。  
> 这属于**连接基础设施层依赖**，非业务层 sessionId 耦合。

---

## 1. 设计原则

| 原则 | 含义 | 违反后果 |
|------|------|---------|
| **P1 — 单键主权** | 前端 SFTP/IDE API 调用只传 `nodeId`，不传 `sessionId` 或 `connectionId`（终端层因事件过滤仍持有 connectionId） | 消灭 SFTP/IDE 闭包过期问题 |
| **P2 — 后端路由** | Rust 侧 `NodeRouter` 负责 nodeId → 具体资源的解析 | Race Condition 全部在后端单线程 / Mutex 解决 |
| **P3 — 事件驱动** | 后端通过 typed event 推送状态变更，前端订阅消费 | 消灭 `refreshConnections()` 轮询缝隙 |
| **P4 — 能力抽象** | 前端请求 `capability`（sftp / terminal / forward），不请求底层 handle | 生命周期由后端自动管理 |
| **P5 — 连接级 SFTP** | SFTP session 挂在 Connection 下，不挂在 Terminal session 下 | 终端重建不影响 SFTP |

---

## 2. 目标 ID 拓扑

```
Frontend (React)                         Backend (Rust)
─────────────────                        ──────────────
nodeId (唯一身份)                        NodeRouter
  │                                        ├→ resolve(nodeId) → ConnectionEntry
  ├→ api.nodeSftpInit(nodeId)              │    ├→ HandleController (clone)
  ├→ api.nodeSftpListDir(nodeId, path)     │    ├→ SftpSession (auto-create)
  ├→ api.nodeSftpWrite(nodeId, path, ...)  │    ├→ TerminalSession (PTY)
  ├→ api.nodeTerminalUrl(nodeId)           │    └→ ForwardManager
  └→ subscribe("node:state", nodeId)       │
                                           ├→ ConnectionEntry.sftp: Option<SftpSession>
                                           │    (生命周期 = 连接生命周期)
                                           │
                                           └→ on reconnect:
                                                ├ new connectionId (内部)
                                                ├ rebuild SftpSession (自动)
                                                ├ update TerminalSession.ws_info
                                                └ emit "node:state" → { ready: true }
```

**前端永远不碰的东西：**
- `sessionId` — 后端内部概念
- `connectionId` — 后端内部概念
- 任何 ID 解析逻辑
- `guardNodeCapability` / `guardSessionConnection` — 被后端路由层替代

---

## 3. 后端改造

### 3.1 NodeRouter：核心路由层

```rust
// src-tauri/src/router/mod.rs (新模块)

use crate::ssh::SshConnectionRegistry;
use crate::session::{SessionRegistry, tree::SessionTree};

/// 节点路由器：将 nodeId 解析为具体的后端资源。
///
/// 所有 Tauri IPC 命令经由 NodeRouter 寻址，前端不再传递
/// sessionId / connectionId。
///
/// **v1.2 明确**：NodeRouter **不持有** SftpRegistry。
/// SFTP session 的唯一真源是 `ConnectionEntry.sftp`（见 §3.3）。
/// `acquire_sftp()` 委托给 `ConnectionEntry.acquire_sftp()`，
/// 不存在第二条创建/查找路径。
pub struct NodeRouter {
    session_tree: Arc<SessionTreeState>,
    connection_registry: Arc<SshConnectionRegistry>,
    session_registry: Arc<SessionRegistry>,
    // ❌ 不再持有 sftp_registry — SFTP 唯一真源在 ConnectionEntry.sftp
}

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("No active connection for node: {0}")]
    NotConnected(String),
    #[error("Connection in error state: {0}")]
    ConnectionError(String),
    #[error("Capability unavailable: {0}")]
    CapabilityUnavailable(String),
    #[error("SFTP operation error: {0}")]
    SftpOperationError(String),
    #[error("Connection timeout: {0}")]
    ConnectionTimeout(String),
}

impl NodeRouter {
    /// 解析 nodeId 到活跃的 SSH ConnectionEntry。
    /// 内部处理：state 检查、重连等待、连接池复用。
    pub async fn resolve_connection(
        &self,
        node_id: &str,
    ) -> Result<Arc<ConnectionEntry>, RouteError> {
        let node = self.session_tree
            .get_node(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.into()))?;

        let conn_id = node.ssh_connection_id
            .as_ref()
            .ok_or_else(|| RouteError::NotConnected(node_id.into()))?;

        let entry = self.connection_registry
            .get(conn_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.into()))?;

        // 状态门禁
        match entry.state() {
            ConnectionState::Active | ConnectionState::Idle => Ok(entry),
            ConnectionState::Reconnecting | ConnectionState::Connecting => {
                // 等待连接就绪（带超时）
                self.wait_for_active(conn_id, Duration::from_secs(15)).await?;
                self.connection_registry
                    .get(conn_id)
                    .ok_or_else(|| RouteError::NotConnected(node_id.into()))
            }
            ConnectionState::Error(msg) => {
                Err(RouteError::ConnectionError(msg.clone()))
            }
            _ => Err(RouteError::NotConnected(node_id.into())),
        }
    }

    /// 获取或创建该节点的 SFTP session。
    ///
    /// **v1.2 唯一路径**：直接委托给 `ConnectionEntry.acquire_sftp()`。
    /// SFTP session 存在且仅存在于 `ConnectionEntry.sftp` 字段中，
    /// NodeRouter 不持有任何 SFTP 索引。
    pub async fn acquire_sftp(
        &self,
        node_id: &str,
    ) -> Result<Arc<Mutex<SftpSession>>, RouteError> {
        let conn = self.resolve_connection(node_id).await?;
        // 唯一真源：ConnectionEntry.acquire_sftp()
        // 内部实现：双重检查锁 read → write，按需创建
        conn.acquire_sftp().await
            .map_err(|e| RouteError::CapabilityUnavailable(e.to_string()))
    }

    /// 获取该节点的 WebSocket 终端 URL。
    /// 如果终端 session 已销毁（session 漂移/重连后 PTY 未重建），
    /// 自动通过 HandleController 重建终端 session。
    pub async fn terminal_url(
        &self,
        node_id: &str,
    ) -> Result<TerminalEndpoint, RouteError> {
        let node = self.session_tree
            .get_node(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.into()))?;

        // 尝试获取现有终端 session
        if let Some(session_id) = node.terminal_session_id.as_ref() {
            if let Some(session) = self.session_registry.get(session_id) {
                if session.ws_port.is_some() {
                    return Ok(TerminalEndpoint {
                        ws_port: session.ws_port,
                        ws_token: session.ws_token.clone(),
                        session_id: session_id.clone(),
                    });
                }
            }
        }

        // 终端 session 不存在或无效 → 按需重建
        let conn = self.resolve_connection(node_id).await?;
        let handle = conn.handle_controller().clone();

        let (new_session_id, endpoint) = self.session_registry
            .create_terminal_for_connection(
                handle,
                conn.id(),
                &node.config,  // SSH 连接配置（shell、env 等）
            )
            .await
            .map_err(|e| RouteError::CapabilityUnavailable(
                format!("Terminal rebuild failed: {}", e),
            ))?;

        // 更新 SessionTree 的引用
        self.session_tree.update_terminal_session(node_id, new_session_id.clone());

        Ok(endpoint)
    }
}
```

### 3.2 IPC 命令签名变更

```rust
// ─── Before (当前) ───
#[tauri::command]
pub async fn sftp_init(session_id: String, ...) -> Result<String, SftpError>;

#[tauri::command]
pub async fn sftp_list_dir(session_id: String, path: String, ...) -> Result<Vec<FileInfo>, SftpError>;

#[tauri::command]
pub async fn sftp_write_content(session_id: String, path: String, content: String, encoding: String) -> ...;


// ─── After (Oxide-Next) ───
#[tauri::command]
pub async fn node_sftp_init(node_id: String, router: State<'_, Arc<NodeRouter>>) -> Result<String, RouteError>;

#[tauri::command]
pub async fn node_sftp_list_dir(node_id: String, path: String, router: State<'_, Arc<NodeRouter>>) -> Result<Vec<FileInfo>, RouteError>;

#[tauri::command]
pub async fn node_sftp_write(node_id: String, path: String, content: String, encoding: String, router: State<'_, Arc<NodeRouter>>) -> ...;
```

**向后兼容策略**：旧 `sftp_init(session_id)` 命令保留，标记 `#[deprecated]`，内部通过适配层查找到新路径。新命令以 `node_` 前缀区分。

#### 旧命令适配路径（精确定义）

旧命令需要从 `session_id` 反查到 `ConnectionEntry`，再委托给新的连接级 SFTP。单一适配路径如下：

```rust
/// 旧命令适配器：session_id → ConnectionEntry.sftp
/// 
/// 路径：session_id → SessionEntry.connection_id → ConnectionEntry → acquire_sftp()
/// 
/// 注意：SessionEntry.connection_id 是 Option<String>，未绑定时返回 SessionNotFound。
/// 此适配器保证旧命令和新命令最终调用同一个 acquire_sftp()，
/// 不存在双实现分叉。
async fn legacy_acquire_sftp(
    session_id: &str,
    session_registry: &SessionRegistry,
    connection_registry: &SshConnectionRegistry,
) -> Result<Arc<Mutex<SftpSession>>, SftpError> {
    // Step 1: session_id → connection_id
    let connection_id = session_registry
        .with_session(session_id, |entry| entry.connection_id.clone())
        .flatten()
        .ok_or_else(|| SftpError::SessionNotFound(session_id.into()))?;

    // Step 2: connection_id → ConnectionEntry
    let conn = connection_registry
        .get(&connection_id)
        .ok_or_else(|| SftpError::SessionNotFound(
            format!("Connection {} not found for session {}", connection_id, session_id),
        ))?;

    // Step 3: 委托给 ConnectionEntry.acquire_sftp()（与新命令共用同一实现）
    conn.acquire_sftp().await
}

// 旧命令改造示例：
#[deprecated(note = "Use node_sftp_list_dir instead")]
#[tauri::command]
pub async fn sftp_list_dir(
    session_id: String,
    path: String,
    session_registry: State<'_, Arc<SessionRegistry>>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<Vec<FileInfo>, SftpError> {
    let sftp = legacy_acquire_sftp(&session_id, &session_registry, &connection_registry).await?;
    let sftp = sftp.lock().await;
    sftp.list_dir(&path).await
}
```

**关键保证**：旧命令和新命令最终调用同一个 `ConnectionEntry.acquire_sftp()`，不存在两份 SFTP 创建/查找逻辑。

### 3.3 SFTP 生命周期改造

```
当前:
  SftpRegistry { key: sessionId → SftpSession }
  终端重建 → sessionId 可能更新 → 旧 SftpSession 变僵尸

Oxide-Next:
  ConnectionEntry {
      sftp: Option<Arc<Mutex<SftpSession>>>,  // 直接持有，不通过外部 Registry
  }
  连接断开 → sftp 自动 drop
  连接重连 → NodeRouter.acquire_sftp 按需重建
  终端重建 → sftp 不受影响（它挂在连接上，不挂在终端上）
```

```rust
// ConnectionEntry 改造 — SFTP 唯一真源（v1.2 明确）
pub struct ConnectionEntry {
    // ... existing fields ...
    
    /// SFTP session，生命周期与连接绑定。
    /// 连接断开时自动 drop，重连时按需重建。
    ///
    /// **这是 SFTP 的唯一存储位置**。
    /// NodeRouter.acquire_sftp() 和 legacy_acquire_sftp() 都委托到这里。
    /// 不存在外部 SftpRegistry 副本。
    sftp: RwLock<Option<Arc<Mutex<SftpSession>>>>,
}

impl ConnectionEntry {
    /// 获取或创建 SFTP session（双重检查锁）。
    ///
    /// 这是 **全系统唯一** 的 SFTP 创建入口：
    /// - NodeRouter.acquire_sftp(nodeId) → conn.acquire_sftp()
    /// - legacy_acquire_sftp(sessionId) → conn.acquire_sftp()
    /// 两条调用路径，同一个实现，零分叉。
    pub async fn acquire_sftp(&self) -> Result<Arc<Mutex<SftpSession>>, SftpError> {
        // 快速路径：read lock 检查
        {
            let guard = self.sftp.read();
            if let Some(sftp) = guard.as_ref() {
                // TODO: 可选健康检查（sftp.is_alive()）
                return Ok(Arc::clone(sftp));
            }
        }
        // 慢路径：write lock + 二次检查
        let mut guard = self.sftp.write();
        if let Some(sftp) = guard.as_ref() {
            return Ok(Arc::clone(sftp));
        }
        let handle = self.handle_controller().clone();
        let sftp = SftpSession::new(handle, self.id().to_string()).await?;
        let arc = Arc::new(Mutex::new(sftp));
        *guard = Some(Arc::clone(&arc));
        Ok(arc)
    }

    /// 连接断开时的清理
    fn on_disconnect(&self) {
        // SFTP session 随连接自动释放，无僵尸通道
        *self.sftp.write() = None;
    }
}
```

### 3.4 事件推送改造

#### 有序性协议

Tauri 事件系统不保证跨异步边界的递送顺序。网络抖动下 `link_down → reconnecting → active` 可能乱序到达，导致前端从 `ready` 回退到 `connecting`。因此每个事件携带单调递增的 `generation`，前端丢弃过期事件。

```rust
/// 后端状态变更事件（取代 refreshConnections 轮询）
///
/// 有序性保证：每个事件携带 generation（每节点单调递增计数器），
/// 前端必须丢弃 generation <= 已见最大值的事件。
#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum NodeStateEvent {
    /// 连接状态变更
    ConnectionStateChanged {
        node_id: String,
        /// 单调递增，防乱序覆盖
        generation: u64,
        state: NodeReadiness,
        /// 变更原因（用于 UI 显示）
        reason: String,
    },
    /// SFTP 就绪状态变更
    SftpReady {
        node_id: String,
        generation: u64,
        ready: bool,
        cwd: Option<String>,
    },
    /// 终端 WebSocket 信息变更（重连后 URL 可能变化）
    TerminalEndpointChanged {
        node_id: String,
        generation: u64,
        ws_port: u16,
        ws_token: String,
    },
}

/// 节点就绪状态（前端唯一需要关心的状态）
#[derive(Clone, Serialize)]
pub enum NodeReadiness {
    /// 完全就绪，可执行所有操作
    Ready,
    /// 正在连接/重连中
    Connecting,
    /// 连接错误（详情见 NodeState.error 字段）
    Error,
    /// 已断开
    Disconnected,
}
```

#### 后端 generation 管理

```rust
/// 每个节点维护一个独立的单调递增计数器。
/// 存储在 SessionTree 或 NodeRouter 上。
struct NodeEventSequencer {
    counters: DashMap<String, AtomicU64>,  // key = node_id
}

impl NodeEventSequencer {
    /// 获取下一个 generation（原子递增，线程安全）
    fn next(&self, node_id: &str) -> u64 {
        self.counters
            .entry(node_id.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst)
    }
}
```

#### 后端 node_get_state 命令（v1.2 快照对齐）

```rust
/// 返回节点当前状态 **含 generation**。
/// 前端用返回的 generation 初始化 maxGen，
/// 确保快照之前的事件不会被采纳。
#[tauri::command]
pub async fn node_get_state(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
    sequencer: State<'_, NodeEventSequencer>,
) -> Result<NodeStateSnapshot, RouteError> {
    let state = router.get_node_state(&node_id)?;
    let generation = sequencer.current(&node_id);
    Ok(NodeStateSnapshot { state, generation })
}

impl NodeEventSequencer {
    /// 获取当前 generation（不递增），用于快照查询。
    fn current(&self, node_id: &str) -> u64 {
        self.counters
            .get(node_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}
```

#### 前端乱序丢弃

```typescript
// useNodeState 内部维护已见最大 generation
const maxGenRef = useRef(0);

setState((prev) => {
  // 丢弃乱序事件
  if (payload.generation <= maxGenRef.current) {
    return prev; // 不更新
  }
  maxGenRef.current = payload.generation;

  switch (payload.type) {
    // ... 正常处理
  }
});
```
```

---

## 4. 前端改造

### 4.1 API 层：node-first

```typescript
// src/lib/api.ts (Oxide-Next)

// ─── 所有 SFTP 命令只接受 nodeId ───
export const nodeSftpInit = (nodeId: string) =>
  invoke<string>('node_sftp_init', { nodeId });

export const nodeSftpListDir = (nodeId: string, path: string) =>
  invoke<FileInfo[]>('node_sftp_list_dir', { nodeId, path });

export const nodeSftpWrite = (nodeId: string, path: string, content: string, encoding: string) =>
  invoke<{ mtime: number }>('node_sftp_write', { nodeId, path, content, encoding });

export const nodeSftpUpload = (nodeId: string, localPath: string, remotePath: string, transferId?: string) =>
  invoke<void>('node_sftp_upload', { nodeId, localPath, remotePath, transferId });

export const nodeSftpDownload = (nodeId: string, remotePath: string, localPath: string, transferId?: string) =>
  invoke<void>('node_sftp_download', { nodeId, remotePath, localPath, transferId });

export const nodeSftpDelete = (nodeId: string, path: string) =>
  invoke<void>('node_sftp_delete', { nodeId, path });

export const nodeSftpRename = (nodeId: string, oldPath: string, newPath: string) =>
  invoke<void>('node_sftp_rename', { nodeId, oldPath, newPath });

export const nodeSftpMkdir = (nodeId: string, path: string) =>
  invoke<void>('node_sftp_mkdir', { nodeId, path });

export const nodeSftpPreview = (nodeId: string, path: string) =>
  invoke<PreviewContent>('node_sftp_preview', { nodeId, path });

export const nodeTerminalUrl = (nodeId: string) =>
  invoke<{ wsPort: number; wsToken: string; sessionId: string }>('node_terminal_url', { nodeId });
```

### 4.2 Store 简化

```typescript
// ─── 删除这些 ───
// virtualSessionStore.ts       — 整个删除
// hooks/useNodeSession.ts      — 整个删除
// lib/connectionGuard.ts       — 整个删除

// ─── 新增：轻量 nodeState 订阅 ───
// src/hooks/useNodeState.ts

import { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';

export type NodeReadiness = 'ready' | 'connecting' | 'error' | 'disconnected';

interface NodeState {
  readiness: NodeReadiness;
  error?: string;
  sftpReady: boolean;
  sftpCwd?: string;
  /** 终端 WebSocket 端点，TerminalEndpointChanged 时更新 */
  wsEndpoint?: { wsPort: number; wsToken: string };
}

/** node_get_state 返回值：状态 + 当前 generation（v1.2 快照对齐） */
interface NodeStateSnapshot {
  state: NodeState;
  generation: number;
}

/**
 * 订阅后端推送的节点状态。
 * 
 * 没有轮询，没有 refreshConnections()，没有闭包过期。
 * 后端是唯一的 source of truth。
 */
export function useNodeState(nodeId: string): { state: NodeState; generation: number; ready: boolean } {
  const [state, setState] = useState<NodeState>({
    readiness: 'disconnected',
    sftpReady: false,
  });

  useEffect(() => {
    let cancelled = false;
    // 已见最大 generation（防乱序回退）
    let maxGen = 0;

    // 初始化：拉一次当前状态 **含 generation**（v1.2 快照对齐）
    // 后端返回 { state, generation }，确保 maxGen 对齐到快照时刻，
    // 避免初始窗口期吃到比快照更旧的事件。
    invoke<NodeStateSnapshot>('node_get_state', { nodeId }).then(({ state: s, generation: gen }) => {
      if (!cancelled) {
        setState(s);
        maxGen = gen;  // 关键：初始化 maxGen 为快照 generation
      }
    });

    // 后续：订阅增量事件
    const unlisten = listen<NodeStateEvent>('node:state', (event) => {
      if (cancelled) return;
      const { payload } = event;
      if (payload.nodeId !== nodeId) return;

      // 有序性保护：丢弃 generation <= 已见最大值的事件
      if (payload.generation <= maxGen) return;
      maxGen = payload.generation;

      setState((prev) => {
        switch (payload.type) {
          case 'connectionStateChanged':
            return {
              ...prev,
              readiness: payload.state as NodeReadiness,
              error: payload.state === 'error' ? payload.reason : undefined,
            };
          case 'sftpReady':
            return {
              ...prev,
              sftpReady: payload.ready,
              sftpCwd: payload.cwd ?? prev.sftpCwd,
            };
          case 'terminalEndpointChanged':  // v1.2：不再忽略
            return {
              ...prev,
              wsEndpoint: {
                wsPort: payload.ws_port,
                wsToken: payload.ws_token,
              },
            };
          default:
            return prev;
        }
      });
    });

    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
    };
  }, [nodeId]);

  return state;
}
```

### 4.3 组件改造对照

#### SFTPView

```tsx
// ─── Before (当前，2000+ 行的 Race Condition 重灾区) ───
const { resolved, resolveTick } = useNodeSession(nodeId);
const activeSessionId = resolved?.sessionId ?? '';
const activeSessionIdRef = useRef(activeSessionId);
// ...  20+ 处 api.sftpXxx(activeSessionIdRef.current, ...)
// ...  guardNodeCapability(nodeId, 'sftp')
// ...  每处都可能过期


// ─── After (Oxide-Next) ───
export function SFTPView({ nodeId }: { nodeId: string }) {
  const { readiness, sftpReady, sftpCwd } = useNodeState(nodeId);

  // 初始化 — 后端自动管理，前端只触发
  useEffect(() => {
    if (readiness !== 'ready') return;
    api.nodeSftpInit(nodeId).then(setCwd).catch(handleError);
  }, [nodeId, readiness]);

  // 列目录 — 没有 sessionId，没有 guard，没有 ref
  const refresh = useCallback(async () => {
    const files = await api.nodeSftpListDir(nodeId, remotePath);
    setRemoteFiles(files);
  }, [nodeId, remotePath]);

  // 删除 — 同上
  const handleDelete = async (filePath: string) => {
    await api.nodeSftpDelete(nodeId, filePath);
    await refresh();
  };

  // ... 所有 handler 只用 nodeId，零闭包风险
}
```

**变化量统计**：
- 删除 `activeSessionId` / `activeSessionIdRef` / `useNodeSession` / `guardNodeCapability`
- 20+ 处 `api.sftpXxx(activeSessionId, ...)` → `api.nodeSftpXxx(nodeId, ...)`
- 组件 props 从 `{ nodeId }` 不变（已是 nodeId-first）

#### RemoteFileEditor

```tsx
// ─── Before ───
const handleSave = useCallback(async () => {
  let effectiveSessionId = sessionId;
  if (nodeId) {
    const resolved = await guardNodeCapability(nodeId, 'sftp');
    effectiveSessionId = resolved.sessionId;
  }
  await api.sftpWriteContent(effectiveSessionId, filePath, content, encoding);
}, [sessionId, nodeId, filePath, content, currentEncoding, ...]);

// ─── After ───
const handleSave = useCallback(async () => {
  await api.nodeSftpWrite(nodeId, filePath, content, encoding);
}, [nodeId, filePath, content, encoding]);
// 没有 guard，没有 sessionId，没有闭包风险
```

#### IdeWorkspace

```tsx
// ─── Before ───
const { resolved, resolveTick } = useNodeSession(nodeId);
const activeConnectionId = resolved?.connectionId ?? '';
const activeSftpSessionId = resolved?.sessionId ?? '';
// openProject(activeConnectionId, activeSftpSessionId, rootPath)

// ─── After ───
const { readiness } = useNodeState(nodeId);
// openProject(nodeId, rootPath) — 后端 NodeRouter 解析
```

#### TerminalView（v1.2 新增：TerminalEndpointChanged 消费契约）

```tsx
/**
 * TerminalView 订阅 wsEndpoint 变更，自动重连 WebSocket。
 *
 * 流程：
 * 1. 后端重连/PTY重建 → 发射 TerminalEndpointChanged
 * 2. useNodeState 更新 wsEndpoint
 * 3. TerminalView useEffect 检测 wsEndpoint 变化 → 断开旧 WS、连接新 WS
 * 4. xterm.js attach 新 socket，无缝过渡
 */
export function TerminalView({ nodeId }: { nodeId: string }) {
  const { readiness, wsEndpoint } = useNodeState(nodeId);
  const xtermRef = useRef<Terminal | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  // 初始获取 + 端点变更时重连
  useEffect(() => {
    if (readiness !== 'ready' || !wsEndpoint) return;

    const { wsPort, wsToken } = wsEndpoint;
    const url = `ws://127.0.0.1:${wsPort}/ws?token=${wsToken}`;

    // 断开旧连接
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }

    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.binaryType = 'arraybuffer';
    ws.onopen = () => {
      // attach 到 xterm.js，已有终端不重建，仅重新挂载 socket
      attachToXterm(xtermRef.current!, ws);
    };
    ws.onerror = (err) => console.error('[TerminalView] WS error:', err);
    ws.onclose = () => {
      // 不手动重连——等待后端推送下一个 TerminalEndpointChanged
    };

    return () => {
      ws.close();
      wsRef.current = null;
    };
  }, [nodeId, readiness, wsEndpoint?.wsPort, wsEndpoint?.wsToken]);

  // ...
}
```

**关键保证**：
- TerminalView **不** 主动重连。后端是唯一的重连发起者，前端只响应事件。
- `wsEndpoint` 由 `useNodeState` 统一管理，generation 有序性保证不会收到过期端点。
- 旧 WS close 在新 WS open 之前，不会双开。
- 如果 `wsEndpoint` 未变（同端口同 token），`useEffect` deps 不触发，零抨动。

### 4.4 删除的代码

| 文件 | 行动 | 理由 |
|------|------|------|
| `src/store/virtualSessionStore.ts` | **删除** | 前端不再需要 nodeId→sessionId 映射 |
| `src/hooks/useNodeSession.ts` | **删除** | 被 `useNodeState` 替代 |
| `src/lib/connectionGuard.ts` | **删除** | 后端 NodeRouter 内置门禁 |
| `src/lib/faultInjection.ts` 的 `refreshDelay` | **删除** | 没有 refreshConnections 了 |
| `appStore.refreshConnections()` | **标记废弃** | 仅保留给旧组件过渡用 |

### 4.5 插件 API 迁移方案

> **v1.1 新增**：插件系统深度依赖 sessionId / connectionId，需要专门的迁移阶段。

#### 当前插件 API 中的 sessionId / connectionId 暴露点

| 文件 | API | 暴露的 ID | 迁移方案 |
|------|-----|-----------|----------|
| `pluginTerminalHooks.ts` | `InputInterceptor(data, { sessionId })` | sessionId | `{ sessionId, nodeId }` — 新增 nodeId，sessionId 标记废弃 |
| `pluginTerminalHooks.ts` | `OutputProcessor(data, { sessionId })` | sessionId | 同上 |
| `pluginContextFactory.ts` | `ctx.terminal.writeToTerminal(sessionId, text)` | sessionId | 新增 `ctx.terminal.writeToNode(nodeId, text)`；旧方法内部适配 |
| `pluginContextFactory.ts` | `ctx.terminal.getBuffer(sessionId)` | sessionId | 新增 `ctx.terminal.getNodeBuffer(nodeId)` |
| `pluginContextFactory.ts` | `ctx.terminal.getSelection(sessionId)` | sessionId | 新增 `ctx.terminal.getNodeSelection(nodeId)` |
| `pluginContextFactory.ts` | `ctx.connections.get(connectionId)` | connectionId | 新增 `ctx.connections.getByNode(nodeId)` |
| `pluginContextFactory.ts` | `ctx.events.onSessionCreated(handler)` | sessionId + connectionId | 新增 `ctx.events.onNodeReady(handler)` |
| `pluginContextFactory.ts` | `ctx.events.onSessionClosed(handler)` | sessionId | 新增 `ctx.events.onNodeDisconnected(handler)` |
| `pluginEventBridge.ts` | `session:created` event 发射 | sessionId + connectionId | 新增 `node:ready` event，旧事件保留一个大版本 |
| `pluginEventBridge.ts` | `session:closed` event 发射 | sessionId | 新增 `node:disconnected` event |

#### TerminalHookContext 改造

```typescript
// ─── Before ───
interface TerminalHookContext {
  sessionId: string;
}

// ─── After (Phase 4.5) ───
interface TerminalHookContext {
  /** @deprecated Use nodeId instead. Will be removed in next major version. */
  sessionId: string;
  /** Stable node identifier, survives reconnect. */
  nodeId: string;
}
```

#### 适配层实现

```typescript
// pluginContextFactory.ts 中的适配
terminal: {
  // 新 API（推荐）
  writeToNode: (nodeId: string, text: string) => {
    const sessionId = resolveNodeToSession(nodeId);  // NodeRouter 查找
    return writeToTerminal(sessionId, text);
  },
  // 旧 API（保留一个大版本）
  writeToTerminal: (sessionId: string, text: string) => {
    console.warn('[Plugin] writeToTerminal(sessionId) is deprecated, use writeToNode(nodeId)');
    return writeToTerminal(sessionId, text);
  },
},
```

#### 迁移时间线

- **Phase 4.5**：新增所有 `nodeId` 版本的 API + 适配层，旧 API 标记 `@deprecated`
- **Phase 5**：旧 API 保留但打 deprecation warning
- **下一个大版本 (v2.0)**：移除旧 sessionId API

---

## 5. 迁移策略

### 5.1 阶段划分

```
Phase 0: 后端 NodeRouter + NodeEventSequencer (纯新增，不破坏旧接口)
    ├── 新建 src-tauri/src/router/mod.rs
    ├── 实现 resolve_connection, acquire_sftp, terminal_url (含重建)
    ├── 实现 NodeEventSequencer (generation 管理，含 next + current)
    ├── 实现 node_get_state 命令（返回 NodeStateSnapshot { state, generation }）
    └── 注册为 Tauri managed state

Phase 1: 后端命令 node_sftp_* (新增 + 旧命令保留)
    ├── 新增 src-tauri/src/commands/node_sftp.rs
    ├── 所有 node_sftp_* 命令内部调用 NodeRouter
    ├── 旧 sftp_* 命令标记 #[deprecated] 但行为不变
    └── 旧命令通过 legacy_acquire_sftp 适配器桥接（见 §3.2）
    ⚠️ 此阶段旧命令仍使用 SftpRegistry[sessionId]，不改变其行为

Phase 1.5: SFTP 生命周期迁移 (有破坏性，需回归)
    ├── ConnectionEntry 新增 sftp: RwLock<Option<...>>
    ├── 新命令 node_sftp_* 使用 ConnectionEntry.acquire_sftp()
    ├── 旧命令切换到 legacy_acquire_sftp 适配器
    │   (session_id → SessionEntry.connection_id → ConnectionEntry.sftp)
    ├── SftpRegistry 降级为 legacy fallback，新增数据不再写入
    └── ⚠️ 需验证：旧前端 + 新后端的 SFTP 功能完整性

Phase 2: 事件推送 (新增)
    ├── NodeStateEvent 定义 (含 generation 字段) + emit
    ├── 在 connection state 变更点 emit "node:state"
    ├── 在 SFTP init/destroy 时 emit "node:state"
    └── 在终端重建时 emit TerminalEndpointChanged

Phase 3: 前端 API 层 + useNodeState (新增)
    ├── api.ts 新增 nodeSftpInit, nodeSftpListDir, ...
    ├── 新增 hooks/useNodeState.ts (含 generation 乱序丢弃 + 初始快照对齐 + wsEndpoint 订阅)
    └── 保留旧 hooks 供未迁移组件使用

Phase 4: 组件迁移 (逐个替换)
    ├── SFTPView: activeSessionId → nodeId
    ├── RemoteFileEditor: sessionId → nodeId
    ├── IdeWorkspace: connectionId/sessionId → nodeId
    ├── TerminalView: WS 管理改为事件驱动 (wsEndpoint)
    └── ForwardsView, TopologyMap, etc.

Phase 4.5: 插件 API 迁移 (见 §4.5)
    ├── 新增 nodeId 到 PluginContext
    ├── Terminal hooks 改为 nodeId 寻址
    ├── Session 事件改为 node 事件
    └── 旧 sessionId API 通过适配层保留一个大版本

Phase 5: 清理 (删除旧代码)
    ├── 删除 virtualSessionStore, useNodeSession, connectionGuard
    ├── 删除旧 sftp_* 命令 + SftpRegistry
    ├── 删除 refreshConnections 轮询模式
    └── 删除插件 sessionId 适配层 (下一个大版本)
```

### 5.2 风险控制

| 阶段 | 风险 | 缓解 |
|------|------|------|
| Phase 0 | 低：纯新增模块，旧路径不受影响 | NodeRouter 独立注册，旧命令不经过它 |
| Phase 1 | 低：新命令并行上线，旧命令完全不变 | 新旧命名前缀不同（`node_sftp_*` vs `sftp_*`） |
| Phase 1.5 | **中：SFTP 挂载点迁移是破坏性变更** | 旧命令通过 `legacy_acquire_sftp` 适配器桥接到新路径；需要完整 SFTP 回归测试（上传/下载/重命名/删除/预览） |
| Phase 2 | 低：新增事件推送，旧轮询模式并行运行 | 事件和轮询共存，互不冲突 |
| Phase 3 | 低：API 并存，可能混用 | `nodeSftp*` 命名前缀明确标识 |
| Phase 4 | 中：逐个组件迁移，可分批 PR | 每个组件迁移后独立测试 |
| Phase 4.5 | **高：插件 API breaking change** | 新增 nodeId API + 旧 sessionId 适配层并存；插件大版本升级窗口 |
| Phase 5 | 中：删除旧代码有回退风险 | Phase 4/4.5 全部验证通过后再执行 |

### 5.3 可验证的里程碑

| 里程碑 | 验证方式 |
|--------|---------|
| **M1: NodeRouter 冒烟** | `invoke('node_sftp_init', { nodeId })` 返回 cwd |
| **M2: SFTP 挂连接** | 断开终端 → SFTP 仍可用；断开连接 → SFTP 自动清理 |
| **M3: 事件驱动** | 手动断网 → 前端 `useNodeState` 收到 `connecting` → 重连后收到 `ready` |
| **M4: 事件有序性** | 快速断连/重连 3 次 → UI 最终状态与后端一致（不回退） |
| **M4.1: 快照对齐** | `node_get_state` 返回 `{ state, generation }` → useNodeState 初始 maxGen 非零 |
| **M5: SFTPView 无 sessionId** | `grep -r "sessionId" src/components/sftp/` 返回零结果 |
| **M6: 插件兼容** | 使用旧 sessionId API 的插件在适配层下正常工作 |

### 5.4 回归验收矩阵

除里程碑外，以下场景必须在 Phase 4 完成后全部通过：

| # | 场景 | 预期行为 | 覆盖缺陷 |
|---|------|---------|----------|
| R1 | 断网 10 分钟 → 恢复 → 打开 SFTP | SFTP 自动重建，列目录成功 | D4 僵尸通道 |
| R2 | link_down → 自动重连 → IDE 打开项目 | IDE 不报错，项目树正常加载 | D1 stale sessionId |
| R3 | 插件 terminal hook（InputInterceptor） | sessionId 旋转后回调收到新 ID（或 nodeId） | 插件兼容 |
| R4 | Forward 恢复 | 连接重建后转发规则自动恢复 | D5 路由缺失 |
| R5 | 跳板链级联断开 → 逐级重连 | 子连接 SFTP/IDE/Forward 全部恢复 | D3 sync gap |
| R6 | 并发 3 个 Tab 同时触发 `nodeSftpInit` | `acquire_sftp` 双重检查锁只创建一个 SFTP session | 并发安全 |
| R7 | 终端已销毁 → 调用 `nodeTerminalUrl` | 自动重建终端 session，返回有效 WS URL | session 漂移 |
| R8 | 事件乱序（注入延迟模拟） | 前端 generation 丢弃旧事件，不回退状态 | 有序性 |
| R9 | 旧 `sftp_list_dir(session_id)` 命令 | 通过 legacy 适配器正常工作 | 向后兼容 |
| R10 | 编码切换后保存文件 | `nodeSftpWrite` 使用正确编码写入 | 闭包过期 |
| R11 | 重连后终端 WS 端口变更 | TerminalView 自动断开旧 WS、连接新端点，无灰屏 | 端点旋转 |

---

## 6. 关键设计决策记录

### 6.1 为什么不在前端加更多 Ref？

Ref 是承认模型缺陷后的补丁。`activeSessionIdRef.current` 虽然解决了闭包过期，但：
- 每新增一个 handler 都要记得用 Ref → 遗忘就是 Bug
- Ref 不触发 re-render → 组件可能显示过期数据
- 20+ 处 Ref 调用 = 20+ 处隐含约定 = 不可维护

### 6.2 为什么不用 React Context 传递 resolved session？

Context 解决不了异步闭包问题。`handleSave` 是 `useCallback`，即使 Context 值更新，回调内的捕获值仍然是旧的。除非每个 handler 都从 Context 实时读取，那就等效于 Ref。

### 6.3 为什么把路由放后端而不是前端 Store？

Rust 的 `Mutex` / `RwLock` 天然解决并发。前端 Zustand 在 React render cycle 之间有同步缝隙（状态更新 → 下一次 render 才生效），这个缝隙就是 Race Condition 的温床。后端路由在同一个 async task 内完成 nodeId→resource 解析，没有中间状态暴露。

### 6.4 为什么 SFTP 应该挂在 Connection 而不是 Terminal 上？

当前模型：
```
Connection ← Terminal (sessionId) ← SFTP (keyed by sessionId)
```

SFTP 底层是在 SSH 连接上开一个新 channel（`open_session_channel` + `request_subsystem("sftp")`），与终端的 PTY channel 完全独立。将 SFTP 挂在 Terminal 上是历史遗留——早期没有连接池，一个连接只有一个终端，sessionId ≈ connectionId。

正确模型：
```
Connection
  ├── Terminal channel (PTY)
  ├── SFTP channel (独立)
  └── Forward channels (独立)
```

这样终端重建（PTY 重新分配）不影响 SFTP；SFTP 只在连接断开时才需要重建。

---

## 7. FAQ

**Q: 这个方案改动量大吗？**

后端：新增 ~300 行 NodeRouter + ~200 行 node_sftp 命令 + ~100 行事件推送 + ~50 行 NodeEventSequencer + ~30 行 node_get_state（含 generation 快照）。  
前端：useNodeState ~70 行（含 generation 保护 + wsEndpoint 订阅），API 层 ~40 行，SFTPView 改造约 -200 行（净减少），RemoteFileEditor ~-30 行，IdeWorkspace ~-20 行，TerminalView ~-40 行（类似量，删旧 WS 管理 + 新增事件驱动连接），**插件迁移 ~150 行**（pluginContextFactory 适配层 + pluginTerminalHooks nodeId 支持 + pluginEventBridge 新事件）。  
总计：净减少约 70 行前端代码，新增约 680 行后端代码，新增约 150 行插件适配层。复杂度大幅降低。

**Q: 旧命令什么时候删？**

Phase 4 全部组件迁移完毕 + 回归测试通过后。过渡期旧命令标记 `#[deprecated]` 但正常工作。

**Q: 本地终端（local-terminal feature）受影响吗？**

不受影响。本地终端不走 SSH 连接，不涉及 connectionId / SFTP。NodeRouter 仅处理远程节点。

**Q: 插件系统怎么办？**

~~插件 API 目前不暴露 sessionId（通过 plugin context 提供能力），迁移对插件透明。~~

**v1.1 修正**：插件 API 深度依赖 sessionId / connectionId（7 个暴露点），迁移**不透明**。详见 §4.5。

---

## 8. 对照表：当前 vs. Oxide-Next

| 维度 | 当前 | Oxide-Next |
|------|------|-----------|
| 前端持有的 ID | nodeId + sessionId + connectionId | SFTP/IDE: **仅 nodeId**；终端: nodeId + connectionId（事件过滤） |
| SFTP 寻址 | `api.sftpInit(sessionId)` | `api.nodeSftpInit(nodeId)` |
| 连接守卫 | `guardNodeCapability` (前端, 异步) | NodeRouter (后端, 同步) |
| 状态同步 | `refreshConnections()` 轮询 | **事件推送** |
| SFTP 生命周期 | 绑定 terminal sessionId | **绑定 connection** |
| 闭包过期风险 | 每个 handler 都需要 Ref | SFTP/IDE: **不存在**（nodeId 不变）；终端: connectionIdRef 仍存在但仅用于事件过滤 |
| 新 handler 心智负担 | 必须用 `activeSessionIdRef.current` | **直接用 `nodeId` prop** |
| Store 数量 | appStore + sessionTreeStore + virtualSessionStore + ideStore | appStore + sessionTreeStore + ideStore |
| 代码路径 | 前端 3 层解析 → IPC → 后端 | 前端 0 层解析 → IPC(nodeId) → 后端路由 |
| 事件有序性 | 无保证（refreshConnections 轮询隐含覆盖） | generation 单调递增 + 前端乱序丢弃 + **初始快照对齐** |
| 终端重建 | 前端报 NotConnected | 后端 NodeRouter 自动重建 PTY |
| 终端端点旋转 | 前端自行发现 + 手动重连 | **TerminalEndpointChanged 推送 + TerminalView 自动重连 WS** |
| 插件 API | sessionId 硬编码（7 暴露点） | nodeId-first + sessionId 适配层（过渡期） |

---

## 9. 结论

**当前问题不是实现层面的，是模型层面的。** 前端持有 sessionId 这个会变化的 ID 作为 API key，就注定了每个异步操作都是潜在的 Race Condition。补丁（guard + ref + store + invariant + fault injection）虽然缓解了症状，但每修一个点就暴露下一个点，因为根源没变。

Oxide-Next 的核心做了一件事：**让 nodeId 成为 SFTP/IDE 层唯一的通信键，把 ID 解析下沉到后端 Mutex 保护的同步路径里。** 终端层因事件载荷约束仍保有 connectionId（仅用于 `connection_status_changed` 事件过滤），这是基础设施层依赖，非业务层 sessionId 耦合。组件代码整体更简单。
