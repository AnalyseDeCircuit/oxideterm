//! SSH Connection Registry
//!
//! 独立的 SSH 连接池管理，与前端界面完全解耦。
//!
//! # 架构
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │  SshConnectionRegistry                                       │
//! │  ┌────────────────────────────────────────────────────────┐  │
//! │  │  ConnectionEntry                                        │  │
//! │  │  ├── handle_controller: HandleController               │  │
//! │  │  ├── config: SessionConfig                              │  │
//! │  │  ├── ref_count: AtomicU32                               │  │
//! │  │  └── idle_timer: Option<JoinHandle>                     │  │
//! │  └────────────────────────────────────────────────────────┘  │
//! └──────────────────────────────────────────────────────────────┘
//!          │
//!          │  HandleController (clone)
//!          │
//!    ┌─────┴─────┬─────────────┬─────────────┐
//!    ▼           ▼             ▼             ▼
//! Terminal   Terminal      SFTP       Forwarding
//!  Tab 1      Tab 2
//! ```
//!
//! # 空闲超时策略
//!
//! - 引用计数归零时，启动空闲计时器（默认 30 分钟）
//! - 计时器到期前有新使用者：取消计时器，复用连接
//! - 计时器到期：断开连接，释放资源
//! - keep_alive=true：忽略空闲超时

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};


use super::handle_owner::HandleController;
use super::{AuthMethod as SshAuthMethod, SshClient, SshConfig};
use crate::session::{AuthMethod, RemoteEnvInfo, SessionConfig};
use crate::sftp::error::SftpError;
use crate::sftp::session::SftpSession;

/// 默认空闲超时时间（30 分钟）
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// 心跳间隔（15 秒）
/// 配合 HEARTBEAT_FAIL_THRESHOLD=2，确保 30 秒内检测到断连
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// 心跳连续失败次数阈值，达到后标记为 LinkDown
/// 15s × 2 = 30s 内必触发重连
const HEARTBEAT_FAIL_THRESHOLD: u32 = 2;

// ═══════════════════════════════════════════════════════════════════════════════
// 🛑 RECONNECT CONSTANTS - REMOVED
// ═══════════════════════════════════════════════════════════════════════════════
// 以下常量已被移除（自动重连引擎已被物理删除）：
// - RECONNECT_INITIAL_DELAY
// - RECONNECT_FIRST_DELAY  
// - RECONNECT_MAX_DELAY
// - RECONNECT_MAX_ATTEMPTS
// ═══════════════════════════════════════════════════════════════════════════════

/// 连接池配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    /// 空闲超时时间（秒）
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,

    /// 最大连接数（0 = 无限制）
    #[serde(default)]
    pub max_connections: usize,

    /// 是否在应用退出时保护连接（graceful shutdown）
    #[serde(default = "default_true")]
    pub protect_on_exit: bool,
}

fn default_idle_timeout_secs() -> u64 {
    DEFAULT_IDLE_TIMEOUT.as_secs()
}

fn default_true() -> bool {
    true
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT.as_secs(),
            max_connections: 0,
            protect_on_exit: true,
        }
    }
}

/// 连接状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// 连接中
    Connecting,
    /// 已连接，有活跃使用者
    Active,
    /// 已连接，无使用者，等待超时
    Idle,
    /// 链路断开（心跳失败），等待重连
    LinkDown,
    /// 正在重连
    Reconnecting,
    /// 正在断开
    Disconnecting,
    /// 已断开
    Disconnected,
    /// 连接错误
    Error(String),
}

/// SSH 连接信息（用于前端显示）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub state: ConnectionState,
    pub ref_count: u32,
    pub keep_alive: bool,
    pub created_at: String,
    pub last_active: String,
    /// 关联的 session IDs
    pub terminal_ids: Vec<String>,
    /// 关联的 SFTP session ID
    pub sftp_session_id: Option<String>,
    /// 关联的 forward IDs
    pub forward_ids: Vec<String>,
    /// 父连接 ID（隧道连接时非空）
    pub parent_connection_id: Option<String>,
    /// 远程环境信息（SSH 连接建立后异步检测，可能为 None）
    pub remote_env: Option<RemoteEnvInfo>,
}

/// 连接池统计信息（用于监控面板）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPoolStats {
    /// 总连接数
    pub total_connections: usize,
    /// 活跃连接数（有终端/SFTP/转发在用）
    pub active_connections: usize,
    /// 空闲连接数（无使用者，等待超时）
    pub idle_connections: usize,
    /// 重连中的连接数
    pub reconnecting_connections: usize,
    /// 链路断开的连接数（等待重连）
    pub link_down_connections: usize,
    /// 总终端数
    pub total_terminals: usize,
    /// 总 SFTP 会话数
    pub total_sftp_sessions: usize,
    /// 总端口转发数
    pub total_forwards: usize,
    /// 总引用计数
    pub total_ref_count: u32,
    /// 连接池容量（0 = 无限制）
    pub pool_capacity: usize,
    /// 空闲超时时间（秒）
    pub idle_timeout_secs: u64,
}

/// 单个 SSH 连接条目
///
/// # 锁获取顺序约定
///
/// 为避免死锁，当需要同时获取多个锁时，必须按以下顺序获取：
///
/// 1. `state` (RwLock)
/// 2. `keep_alive` (RwLock)
/// 3. `terminal_ids` (RwLock)
/// 4. `sftp_session_id` (RwLock)
/// 5. `sftp` (tokio::sync::Mutex) — Oxide-Next Phase 1.5
/// 6. `forward_ids` (RwLock)
/// 7. `last_emitted_status` (RwLock)
/// 8. `idle_timer` (Mutex)
/// 9. `heartbeat_task` (Mutex)
/// 10. `reconnect_task` (Mutex)
///
/// 注意：大多数方法只获取单个锁，无需担心顺序。此约定仅在需要
/// 同时持有多个锁时适用（目前代码中几乎不存在这种情况）。
pub struct ConnectionEntry {
    /// 连接唯一 ID
    pub id: String,

    /// 连接配置
    pub config: SessionConfig,

    /// Handle 控制器（可克隆，用于打开 channel）
    pub handle_controller: HandleController,

    /// 连接状态
    state: RwLock<ConnectionState>,

    /// 引用计数（Terminal + SFTP + Forwarding）
    ref_count: AtomicU32,

    /// 最后活动时间戳（Unix 时间戳，秒）
    last_active: AtomicU64,

    /// 是否保持连接（用户设置）
    keep_alive: RwLock<bool>,

    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// 空闲计时器句柄（用于取消）
    idle_timer: Mutex<Option<JoinHandle<()>>>,

    /// 关联的 terminal session IDs
    terminal_ids: RwLock<Vec<String>>,

    /// 关联的 SFTP session ID
    sftp_session_id: RwLock<Option<String>>,

    /// SFTP session 实例 — Oxide-Next Phase 1.5 唯一真源
    ///
    /// SFTP session 的生命周期与连接绑定：
    /// - 连接断开时自动 drop（`clear_sftp()`）
    /// - 连接重连后按需重建（`acquire_sftp()`）
    /// - 终端重建不影响 SFTP
    ///
    /// 使用 `tokio::sync::Mutex` 包裹 Option，确保 acquire_sftp()
    /// 的双重检查锁在 await 点安全。
    sftp: tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<SftpSession>>>>,

    /// 关联的 forward IDs
    forward_ids: RwLock<Vec<String>>,

    /// 心跳任务句柄
    heartbeat_task: Mutex<Option<JoinHandle<()>>>,

    /// 连续心跳失败次数
    heartbeat_failures: AtomicU32,

    /// 重连任务句柄
    reconnect_task: Mutex<Option<JoinHandle<()>>>,

    /// 是否正在重连
    is_reconnecting: AtomicBool,

    /// 重连尝试次数
    reconnect_attempts: AtomicU32,

    /// 当前重连任务 ID（用于状态幂等检查，防止旧任务结果覆盖新任务）
    current_attempt_id: AtomicU64,

    /// 最后一次发送的状态事件（用于状态守卫，避免重复发送）
    last_emitted_status: RwLock<Option<String>>,

    /// 父连接 ID（用于隧道连接，通过父连接的 direct-tcpip 建立）
    /// None = 直连本地
    /// Some(id) = 通过父连接的隧道建立
    parent_connection_id: Option<String>,

    /// 远程环境信息（异步检测结果，可能为 None 表示检测中或失败）
    remote_env: RwLock<Option<RemoteEnvInfo>>,
}

impl ConnectionEntry {
    /// 增加引用计数
    pub fn add_ref(&self) -> u32 {
        let current = self.ref_count.load(Ordering::SeqCst);
        // 防止溢出
        if current >= u32::MAX - 1 {
            warn!("Connection {} ref count at maximum, not incrementing", self.id);
            return current;
        }
        let count = self.ref_count.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        debug!("Connection {} ref count increased to {}", self.id, count);
        self.update_activity();
        count
    }

    /// 减少引用计数
    pub fn release(&self) -> u32 {
        let current = self.ref_count.load(Ordering::SeqCst);
        // 防止下溢
        if current == 0 {
            warn!("Connection {} ref count already 0, not decrementing", self.id);
            return 0;
        }
        let prev = self.ref_count.fetch_sub(1, Ordering::SeqCst);
        let count = prev.saturating_sub(1);
        debug!("Connection {} ref count decreased to {}", self.id, count);
        self.update_activity();
        count
    }

    /// 获取当前引用计数
    pub fn ref_count(&self) -> u32 {
        self.ref_count.load(Ordering::SeqCst)
    }

    /// 更新活动时间
    pub fn update_activity(&self) {
        let now = Utc::now().timestamp() as u64;
        self.last_active.store(now, Ordering::SeqCst);
    }

    /// 获取最后活动时间
    pub fn last_active(&self) -> i64 {
        self.last_active.load(Ordering::SeqCst) as i64
    }

    /// 获取连接状态
    pub async fn state(&self) -> ConnectionState {
        self.state.read().await.clone()
    }

    /// 设置连接状态
    pub async fn set_state(&self, state: ConnectionState) {
        *self.state.write().await = state;
    }

    /// 获取 keep_alive 标志
    pub async fn is_keep_alive(&self) -> bool {
        *self.keep_alive.read().await
    }

    /// 设置 keep_alive 标志
    pub async fn set_keep_alive(&self, keep_alive: bool) {
        *self.keep_alive.write().await = keep_alive;
    }

    /// 取消空闲计时器
    pub async fn cancel_idle_timer(&self) {
        let mut timer = self.idle_timer.lock().await;
        if let Some(handle) = timer.take() {
            handle.abort();
            debug!("Connection {} idle timer cancelled", self.id);
        }
    }

    /// 设置空闲计时器
    pub async fn set_idle_timer(&self, handle: JoinHandle<()>) {
        let mut timer = self.idle_timer.lock().await;
        // 取消之前的计时器
        if let Some(old_handle) = timer.take() {
            old_handle.abort();
        }
        *timer = Some(handle);
    }

    /// 添加关联的 terminal session ID
    pub async fn add_terminal(&self, session_id: String) {
        self.terminal_ids.write().await.push(session_id);
    }

    /// 移除关联的 terminal session ID
    pub async fn remove_terminal(&self, session_id: &str) {
        self.terminal_ids.write().await.retain(|id| id != session_id);
    }

    /// 获取关联的 terminal session IDs
    pub async fn terminal_ids(&self) -> Vec<String> {
        self.terminal_ids.read().await.clone()
    }

    /// 设置关联的 SFTP session ID
    pub async fn set_sftp_session(&self, session_id: Option<String>) {
        *self.sftp_session_id.write().await = session_id;
    }

    /// 获取关联的 SFTP session ID
    pub async fn sftp_session_id(&self) -> Option<String> {
        self.sftp_session_id.read().await.clone()
    }

    // ========================================================================
    // Oxide-Next Phase 1.5: SFTP 连接级生命周期管理
    // ========================================================================

    /// 获取或创建 SFTP session（单锁保护）。
    ///
    /// 这是 **全系统唯一** 的 SFTP 创建入口：
    /// - `NodeRouter.acquire_sftp(nodeId)` → `conn.acquire_sftp()`
    /// 所有 SFTP 操作通过 NodeRouter 路由到此方法。
    ///
    /// 参考: docs/OXIDE_NEXT_ARCHITECTURE.md §3.3
    pub async fn acquire_sftp(&self) -> Result<Arc<tokio::sync::Mutex<SftpSession>>, SftpError> {
        // 持有外层锁贯穿整个创建过程，防止并发创建多个 SSH channel。
        // tokio::sync::Mutex 允许跨 await 点持有。
        let mut guard = self.sftp.lock().await;

        // 快速路径：已有 SFTP session
        if let Some(ref sftp) = *guard {
            return Ok(Arc::clone(sftp));
        }

        // 慢路径：在锁内创建新 SFTP session，确保同连接只创建一次
        let new_sftp = SftpSession::new(
            self.handle_controller.clone(),
            self.id.clone(),
        ).await?;

        let arc = Arc::new(tokio::sync::Mutex::new(new_sftp));
        *guard = Some(Arc::clone(&arc));
        info!("Created SFTP session for connection {}", self.id);
        Ok(arc)
    }

    /// 清除 SFTP session（连接断开时调用）。
    ///
    /// SFTP session 随连接自动释放，无僵尸通道。
    pub async fn clear_sftp(&self) {
        let mut guard = self.sftp.lock().await;
        if guard.is_some() {
            *guard = None;
            info!("Cleared SFTP session for connection {}", self.id);
        }
    }

    /// 失效并清除 SFTP session（静默重建时调用）
    ///
    /// 与 `clear_sftp()` 的区别：
    /// - `clear_sftp()`: 用于连接断开时的清理，表示"不再需要"
    /// - `invalidate_sftp()`: 用于静默重建时的清理，表示"准备重新创建"
    ///
    /// 内部实现相同，但语义不同便于代码阅读和日志追踪。
    ///
    /// # Returns
    /// - `true`: 存在 SFTP session 且已清除
    /// - `false`: 不存在 SFTP session
    pub async fn invalidate_sftp(&self) -> bool {
        let mut guard = self.sftp.lock().await;
        if guard.is_some() {
            *guard = None;
            info!(
                "Invalidated SFTP session for connection {} (preparing rebuild)",
                self.id
            );
            true
        } else {
            false
        }
    }

    /// 检查是否有活跃的 SFTP session
    pub async fn has_sftp(&self) -> bool {
        self.sftp.lock().await.is_some()
    }

    /// 获取 SFTP session 的 cwd（如果存在）
    pub async fn sftp_cwd(&self) -> Option<String> {
        let guard = self.sftp.lock().await;
        if let Some(ref sftp_arc) = *guard {
            let sftp = sftp_arc.lock().await;
            Some(sftp.cwd().to_string())
        } else {
            None
        }
    }

    /// 添加关联的 forward ID
    pub async fn add_forward(&self, forward_id: String) {
        self.forward_ids.write().await.push(forward_id);
    }

    /// 移除关联的 forward ID
    pub async fn remove_forward(&self, forward_id: &str) {
        self.forward_ids.write().await.retain(|id| id != forward_id);
    }

    /// 获取关联的 forward IDs
    pub async fn forward_ids(&self) -> Vec<String> {
        self.forward_ids.read().await.clone()
    }

    /// 获取远程环境信息
    pub async fn remote_env(&self) -> Option<RemoteEnvInfo> {
        self.remote_env.read().await.clone()
    }

    /// 设置远程环境信息
    pub async fn set_remote_env(&self, env: RemoteEnvInfo) {
        *self.remote_env.write().await = Some(env);
    }

    /// 转换为 ConnectionInfo
    pub async fn to_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            id: self.id.clone(),
            host: self.config.host.clone(),
            port: self.config.port,
            username: self.config.username.clone(),
            state: self.state().await,
            ref_count: self.ref_count(),
            keep_alive: self.is_keep_alive().await,
            created_at: self.created_at.to_rfc3339(),
            last_active: chrono::DateTime::from_timestamp(self.last_active(), 0)
                .unwrap_or_default()
                .to_rfc3339(),
            terminal_ids: self.terminal_ids().await,
            sftp_session_id: self.sftp_session_id().await,
            forward_ids: self.forward_ids().await,
            parent_connection_id: self.parent_connection_id.clone(),
            remote_env: self.remote_env().await,
        }
    }

    /// 获取父连接 ID
    pub fn parent_connection_id(&self) -> Option<&str> {
        self.parent_connection_id.as_deref()
    }

    /// 重置心跳失败计数
    pub fn reset_heartbeat_failures(&self) {
        self.heartbeat_failures.store(0, Ordering::SeqCst);
    }

    /// 增加心跳失败计数并返回新值
    pub fn increment_heartbeat_failures(&self) -> u32 {
        self.heartbeat_failures.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 获取心跳失败计数
    pub fn heartbeat_failures(&self) -> u32 {
        self.heartbeat_failures.load(Ordering::SeqCst)
    }

    /// 取消心跳任务
    pub async fn cancel_heartbeat(&self) {
        let mut task = self.heartbeat_task.lock().await;
        if let Some(handle) = task.take() {
            handle.abort();
            debug!("Connection {} heartbeat task cancelled", self.id);
        }
    }

    /// 设置心跳任务句柄
    pub async fn set_heartbeat_task(&self, handle: JoinHandle<()>) {
        let mut task = self.heartbeat_task.lock().await;
        if let Some(old_handle) = task.take() {
            old_handle.abort();
        }
        *task = Some(handle);
    }

    /// 取消重连任务
    pub async fn cancel_reconnect(&self) {
        let mut task = self.reconnect_task.lock().await;
        if let Some(handle) = task.take() {
            handle.abort();
            debug!("Connection {} reconnect task cancelled", self.id);
        }
        self.is_reconnecting.store(false, Ordering::SeqCst);
        self.reconnect_attempts.store(0, Ordering::SeqCst);
    }

    /// 设置重连任务句柄
    pub async fn set_reconnect_task(&self, handle: JoinHandle<()>) {
        let mut task = self.reconnect_task.lock().await;
        if let Some(old_handle) = task.take() {
            old_handle.abort();
        }
        *task = Some(handle);
        self.is_reconnecting.store(true, Ordering::SeqCst);
    }

    /// 检查是否正在重连
    pub fn is_reconnecting(&self) -> bool {
        self.is_reconnecting.load(Ordering::SeqCst)
    }

    /// 增加重连尝试次数并返回新值
    pub fn increment_reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 获取重连尝试次数
    pub fn reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts.load(Ordering::SeqCst)
    }

    /// 重置重连状态
    pub fn reset_reconnect_state(&self) {
        self.is_reconnecting.store(false, Ordering::SeqCst);
        self.reconnect_attempts.store(0, Ordering::SeqCst);
    }

    /// 生成新的重连尝试 ID 并返回
    pub fn new_attempt_id(&self) -> u64 {
        self.current_attempt_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 获取当前重连尝试 ID
    pub fn current_attempt_id(&self) -> u64 {
        self.current_attempt_id.load(Ordering::SeqCst)
    }
}

/// SSH 连接注册表错误
#[derive(Debug, thiserror::Error)]
pub enum ConnectionRegistryError {
    #[error("Connection not found: {0}")]
    NotFound(String),

    #[error("Connection limit reached: {current}/{max}")]
    LimitReached { current: usize, max: usize },

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Already disconnected")]
    AlreadyDisconnected,

    #[error("Invalid state transition: {0}")]
    InvalidState(String),
}

/// SSH 连接注册表
pub struct SshConnectionRegistry {
    /// 所有活跃的 SSH 连接
    connections: DashMap<String, Arc<ConnectionEntry>>,

    /// 连接池配置
    config: RwLock<ConnectionPoolConfig>,

    /// Tauri App Handle（用于发送事件）
    app_handle: RwLock<Option<AppHandle>>,

    /// 待发送的事件（AppHandle 未就绪时缓存）
    pending_events: Mutex<Vec<(String, String)>>,

    /// Oxide-Next Phase 2: 节点事件发射器
    node_event_emitter: parking_lot::RwLock<Option<Arc<crate::router::NodeEventEmitter>>>,
}

impl Default for SshConnectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SshConnectionRegistry {
    /// 创建新的连接注册表
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            config: RwLock::new(ConnectionPoolConfig::default()),
            app_handle: RwLock::new(None),
            pending_events: Mutex::new(Vec::new()),
            node_event_emitter: parking_lot::RwLock::new(None),
        }
    }

    /// 使用自定义配置创建
    pub fn with_config(config: ConnectionPoolConfig) -> Self {
        Self {
            connections: DashMap::new(),
            config: RwLock::new(config),
            app_handle: RwLock::new(None),
            pending_events: Mutex::new(Vec::new()),
            node_event_emitter: parking_lot::RwLock::new(None),
        }
    }

    /// 设置 AppHandle（用于发送事件）
    /// 
    /// 设置后会立即处理所有缓存的事件
    pub async fn set_app_handle(&self, handle: AppHandle) {
        use tauri::Emitter;
        
        // 先取出所有缓存的事件
        let pending = {
            let mut events = self.pending_events.lock().await;
            std::mem::take(&mut *events)
        };
        
        // 发送所有缓存的事件
        if !pending.is_empty() {
            info!("AppHandle ready, flushing {} cached events", pending.len());
            
            #[derive(Clone, serde::Serialize)]
            struct ConnectionStatusEvent {
                connection_id: String,
                status: String,
            }
            
            for (connection_id, status) in pending {
                let event = ConnectionStatusEvent {
                    connection_id: connection_id.clone(),
                    status: status.clone(),
                };
                
                if let Err(e) = handle.emit("connection_status_changed", event) {
                    error!("Failed to emit cached event: {}", e);
                } else {
                    debug!("Emitted cached event: {} -> {}", connection_id, status);
                }
            }
        }
        
        // 设置 AppHandle
        *self.app_handle.write().await = Some(handle);
        info!("AppHandle registered and ready");
    }

    /// 设置 NodeEventEmitter（Phase 2 事件推送）
    ///
    /// 在 Tauri setup 阶段调用，NodeRouter 创建之后。
    pub fn set_node_event_emitter(&self, emitter: Arc<crate::router::NodeEventEmitter>) {
        *self.node_event_emitter.write() = Some(emitter);
        info!("NodeEventEmitter injected into SshConnectionRegistry");
    }

    /// 获取 NodeEventEmitter 引用（内部使用）
    pub(crate) fn node_emitter(&self) -> Option<Arc<crate::router::NodeEventEmitter>> {
        self.node_event_emitter.read().clone()
    }

    /// 获取配置
    pub async fn config(&self) -> ConnectionPoolConfig {
        self.config.read().await.clone()
    }

    /// 更新配置
    pub async fn set_config(&self, config: ConnectionPoolConfig) {
        *self.config.write().await = config;
    }

    /// 获取空闲超时时间
    pub async fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.config.read().await.idle_timeout_secs)
    }

    /// 获取连接池统计信息
    ///
    /// 用于监控面板实时显示连接池状态
    pub async fn get_stats(&self) -> ConnectionPoolStats {
        let config = self.config.read().await;
        let pool_capacity = config.max_connections;
        let idle_timeout_secs = config.idle_timeout_secs;
        drop(config);

        let mut active_connections = 0;
        let mut idle_connections = 0;
        let mut reconnecting_connections = 0;
        let mut link_down_connections = 0;
        let mut total_terminals = 0;
        let mut total_sftp_sessions = 0;
        let mut total_forwards = 0;
        let mut total_ref_count: u32 = 0;

        for entry in self.connections.iter() {
            let conn = entry.value();
            let state = conn.state().await;

            match state {
                ConnectionState::Active => active_connections += 1,
                ConnectionState::Idle => idle_connections += 1,
                ConnectionState::Reconnecting => reconnecting_connections += 1,
                ConnectionState::LinkDown => link_down_connections += 1,
                _ => {}
            }

            total_terminals += conn.terminal_ids.read().await.len();
            if conn.sftp_session_id.read().await.is_some() {
                total_sftp_sessions += 1;
            }
            total_forwards += conn.forward_ids.read().await.len();
            total_ref_count = total_ref_count.saturating_add(conn.ref_count());
        }

        ConnectionPoolStats {
            total_connections: self.connections.len(),
            active_connections,
            idle_connections,
            reconnecting_connections,
            link_down_connections,
            total_terminals,
            total_sftp_sessions,
            total_forwards,
            total_ref_count,
            pool_capacity,
            idle_timeout_secs,
        }
    }

    /// 创建新的 SSH 连接
    ///
    /// # Arguments
    /// * `config` - SSH 连接配置
    ///
    /// # Returns
    /// * `Ok(connection_id)` - 连接成功，返回连接 ID
    /// * `Err(e)` - 连接失败
    pub async fn connect(
        self: &Arc<Self>,
        config: SessionConfig,
    ) -> Result<String, ConnectionRegistryError> {
        // 检查连接数限制
        let pool_config = self.config.read().await;
        if pool_config.max_connections > 0
            && self.connections.len() >= pool_config.max_connections
        {
            return Err(ConnectionRegistryError::LimitReached {
                current: self.connections.len(),
                max: pool_config.max_connections,
            });
        }
        drop(pool_config);

        let connection_id = uuid::Uuid::new_v4().to_string();

        info!(
            "Creating SSH connection {} -> {}@{}:{}",
            connection_id, config.username, config.host, config.port
        );

        // 转换 SessionConfig 到 SshConfig
        let ssh_config = SshConfig {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            auth: match &config.auth {
                AuthMethod::Password { password } => SshAuthMethod::Password { password: password.clone() },
                AuthMethod::Key {
                    key_path,
                    passphrase,
                } => SshAuthMethod::Key {
                    key_path: key_path.clone(),
                    passphrase: passphrase.clone(),
                },
                AuthMethod::Certificate {
                    key_path,
                    cert_path,
                    passphrase,
                } => SshAuthMethod::Certificate {
                    key_path: key_path.clone(),
                    cert_path: cert_path.clone(),
                    passphrase: passphrase.clone(),
                },
                AuthMethod::Agent => SshAuthMethod::Agent,
                AuthMethod::KeyboardInteractive => {
                    // KBI sessions must use the dedicated ssh_connect_kbi command
                    return Err(ConnectionRegistryError::ConnectionFailed(
                        "KeyboardInteractive must use ssh_connect_kbi command".to_string(),
                    ));
                }
            },
            timeout_secs: 30,
            cols: config.cols,
            rows: config.rows,
            proxy_chain: None,
            strict_host_key_checking: false,
            trust_host_key: None, // Connection pool uses known_hosts, no TOFU here
        };

        // 建立 SSH 连接
        let client = SshClient::new(ssh_config);
        let session = client
            .connect()
            .await
            .map_err(|e| ConnectionRegistryError::ConnectionFailed(e.to_string()))?;

        info!("SSH connection {} established", connection_id);

        // 启动 Handle Owner Task，获取 HandleController
        let handle_controller = session.start(connection_id.clone());

        // 创建连接条目
        let entry = Arc::new(ConnectionEntry {
            id: connection_id.clone(),
            config,
            handle_controller,
            state: RwLock::new(ConnectionState::Active),
            ref_count: AtomicU32::new(0),
            last_active: AtomicU64::new(Utc::now().timestamp() as u64),
            keep_alive: RwLock::new(false),
            created_at: Utc::now(),
            idle_timer: Mutex::new(None),
            terminal_ids: RwLock::new(Vec::new()),
            sftp_session_id: RwLock::new(None),
            sftp: tokio::sync::Mutex::new(None),
            forward_ids: RwLock::new(Vec::new()),
            heartbeat_task: Mutex::new(None),
            heartbeat_failures: AtomicU32::new(0),
            reconnect_task: Mutex::new(None),
            is_reconnecting: AtomicBool::new(false),
            reconnect_attempts: AtomicU32::new(0),
            current_attempt_id: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            parent_connection_id: None, // 直连，无父连接
            remote_env: RwLock::new(None), // 待异步检测
        });

        self.connections.insert(connection_id.clone(), entry);

        // 启动心跳检测
        self.start_heartbeat(&connection_id);

        // 启动远程环境检测（异步，不阻塞）
        self.spawn_env_detection(&connection_id);

        // Oxide-Next Phase 2: 发射连接就绪事件
        // 注：初次连接时 conn_to_node 映射通常尚未注册（前端在 connect 返回后才调用
        // set_tree_node_connection），因此此处 emit 通常是 no-op。
        // 但对重连场景（映射已存在），此处 emit 有效。
        if let Some(emitter) = self.node_emitter() {
            emitter.emit_state_from_connection(
                &connection_id,
                &ConnectionState::Active,
                "connected",
            );
        }

        Ok(connection_id)
    }

    /// 通过已有连接建立隧道连接（用于动态钻入跳板机）
    ///
    /// # 工作原理
    ///
    /// ```text
    /// [本地] --SSH--> [父连接] --direct-tcpip--> [目标主机]
    ///                    ↓                           ↓
    ///              parent_connection_id         新 SSH 连接
    /// ```
    ///
    /// # Arguments
    /// * `parent_connection_id` - 父连接 ID（必须是已连接状态）
    /// * `target_config` - 目标服务器配置
    ///
    /// # Returns
    /// * `Ok(connection_id)` - 新的隧道连接 ID
    pub async fn establish_tunneled_connection(
        self: &Arc<Self>,
        parent_connection_id: &str,
        target_config: SessionConfig,
    ) -> Result<String, ConnectionRegistryError> {
        // 1. 获取父连接
        let parent_entry = self
            .connections
            .get(parent_connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(parent_connection_id.to_string()))?;

        let parent_conn = parent_entry.value().clone();
        drop(parent_entry); // 释放 DashMap 锁

        // 检查父连接状态
        let parent_state = parent_conn.state().await;
        if parent_state != ConnectionState::Active && parent_state != ConnectionState::Idle {
            return Err(ConnectionRegistryError::InvalidState(format!(
                "Parent connection {} is not in Active/Idle state: {:?}",
                parent_connection_id, parent_state
            )));
        }

        info!(
            "Establishing tunneled connection via {} -> {}@{}:{}",
            parent_connection_id, target_config.username, target_config.host, target_config.port
        );

        // 2. 通过父连接打开 direct-tcpip 隧道
        let channel = parent_conn
            .handle_controller
            .open_direct_tcpip(
                &target_config.host,
                target_config.port as u32,
                "127.0.0.1", // originator_host
                0,           // originator_port (local)
            )
            .await
            .map_err(|e| {
                ConnectionRegistryError::ConnectionFailed(format!(
                    "Failed to open direct-tcpip channel: {}",
                    e
                ))
            })?;

        debug!("Direct-tcpip channel opened to {}:{}", target_config.host, target_config.port);

        // 3. 将 channel 转换为 stream 用于 SSH-over-SSH
        let stream = channel.into_stream();

        // 4. 在隧道上建立新的 SSH 连接
        let connection_id = uuid::Uuid::new_v4().to_string();

        // 创建 SSH 配置（非严格主机密钥检查，因为是隧道连接）
        let ssh_config = russh::client::Config {
            inactivity_timeout: None, // Disabled: app-level heartbeat handles liveness
            keepalive_interval: Some(std::time::Duration::from_secs(30)),
            keepalive_max: 3,
            ..Default::default()
        };

        let handler = super::client::ClientHandler::new(
            target_config.host.clone(),
            target_config.port,
            false, // 隧道连接不严格检查主机密钥
        );

        // 使用 russh::connect_stream 在隧道上建立 SSH
        let mut handle = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            russh::client::connect_stream(std::sync::Arc::new(ssh_config), stream, handler),
        )
        .await
        .map_err(|_| {
            ConnectionRegistryError::ConnectionFailed(format!(
                "Connection to {}:{} via tunnel timed out",
                target_config.host, target_config.port
            ))
        })?
        .map_err(|e| {
            ConnectionRegistryError::ConnectionFailed(format!(
                "Failed to connect via tunnel: {}",
                e
            ))
        })?;

        debug!("SSH handshake via tunnel completed");

        // 5. 认证
        let authenticated = match &target_config.auth {
            AuthMethod::Password { password } => {
                handle
                    .authenticate_password(&target_config.username, password)
                    .await
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Authentication failed: {}",
                            e
                        ))
                    })?
            }
            AuthMethod::Key {
                key_path,
                passphrase,
            } => {
                let key = russh::keys::load_secret_key(key_path, passphrase.as_deref())
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Failed to load key: {}",
                            e
                        ))
                    })?;

                let key_with_hash =
                    russh::keys::key::PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), None);

                handle
                    .authenticate_publickey(&target_config.username, key_with_hash)
                    .await
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Authentication failed: {}",
                            e
                        ))
                    })?
            }
            AuthMethod::Certificate {
                key_path,
                cert_path,
                passphrase,
            } => {
                let key = russh::keys::load_secret_key(key_path, passphrase.as_deref())
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Failed to load key: {}",
                            e
                        ))
                    })?;

                let cert = russh::keys::load_openssh_certificate(cert_path)
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Failed to load certificate: {}",
                            e
                        ))
                    })?;

                handle
                    .authenticate_openssh_cert(&target_config.username, std::sync::Arc::new(key), cert)
                    .await
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Certificate authentication failed: {}",
                            e
                        ))
                    })?
            }
            AuthMethod::Agent => {
                let mut agent = crate::ssh::agent::SshAgentClient::connect()
                    .await
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Failed to connect to SSH agent: {}",
                            e
                        ))
                    })?;
                agent.authenticate(&mut handle, target_config.username.clone()).await.map_err(|e| {
                    ConnectionRegistryError::ConnectionFailed(format!(
                        "Agent authentication failed: {}",
                        e
                    ))
                })?;
                russh::client::AuthResult::Success
            }
            AuthMethod::KeyboardInteractive => {
                // KBI via proxy chain is not supported in MVP
                return Err(ConnectionRegistryError::ConnectionFailed(
                    "KeyboardInteractive authentication not supported via proxy chain".to_string(),
                ));
            }
        };

        if !authenticated.success() {
            return Err(ConnectionRegistryError::ConnectionFailed(format!(
                "Authentication to {} rejected",
                target_config.host
            )));
        }

        info!(
            "Tunneled SSH connection {} established via {}",
            connection_id, parent_connection_id
        );

        // 6. 创建 SshSession 并启动 Handle Owner Task
        let session = super::session::SshSession::new(handle, target_config.cols, target_config.rows);
        let handle_controller = session.start(connection_id.clone());

        // 7. 创建连接条目（带父连接 ID）
        let entry = Arc::new(ConnectionEntry {
            id: connection_id.clone(),
            config: target_config,
            handle_controller,
            state: RwLock::new(ConnectionState::Active),
            ref_count: AtomicU32::new(0),
            last_active: AtomicU64::new(Utc::now().timestamp() as u64),
            keep_alive: RwLock::new(false),
            created_at: Utc::now(),
            idle_timer: Mutex::new(None),
            terminal_ids: RwLock::new(Vec::new()),
            sftp_session_id: RwLock::new(None),
            sftp: tokio::sync::Mutex::new(None),
            forward_ids: RwLock::new(Vec::new()),
            heartbeat_task: Mutex::new(None),
            heartbeat_failures: AtomicU32::new(0),
            reconnect_task: Mutex::new(None),
            is_reconnecting: AtomicBool::new(false),
            reconnect_attempts: AtomicU32::new(0),
            current_attempt_id: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            parent_connection_id: Some(parent_connection_id.to_string()), // 隧道连接，记录父连接
            remote_env: RwLock::new(None), // 待异步检测
        });

        self.connections.insert(connection_id.clone(), entry);

        // 8. 增加父连接的引用计数（隧道连接依赖父连接）
        parent_conn.add_ref();
        debug!(
            "Parent connection {} ref_count increased (tunneled child: {})",
            parent_connection_id, connection_id
        );

        // 启动心跳检测
        self.start_heartbeat(&connection_id);

        // 启动远程环境检测（异步，不阻塞）
        self.spawn_env_detection(&connection_id);

        // Oxide-Next Phase 2: 发射隧道连接就绪事件（同 connect，通常 no-op）
        if let Some(emitter) = self.node_emitter() {
            emitter.emit_state_from_connection(
                &connection_id,
                &ConnectionState::Active,
                "tunnel connected",
            );
        }

        Ok(connection_id)
    }

    /// 根据配置查找已存在的连接
    ///
    /// 用于复用已有连接
    pub fn find_by_config(&self, config: &SessionConfig) -> Option<String> {
        for entry in self.connections.iter() {
            let conn = entry.value();
            if conn.config.host == config.host
                && conn.config.port == config.port
                && conn.config.username == config.username
            {
                // 检查连接是否还活着
                if conn.handle_controller.is_connected() {
                    return Some(entry.key().clone());
                }
            }
        }
        None
    }

    /// 精细化连接复用查找
    ///
    /// 比 `find_by_config` 更严格，额外检查：
    /// - 认证方式兼容性
    /// - 连接状态必须健康（Active/Idle）
    /// - 心跳失败次数必须为 0
    ///
    /// # Returns
    /// * `Some((connection_id, reuse_quality))` - 找到可复用连接，quality 0-100
    /// * `None` - 没有合适的复用连接
    pub async fn find_reusable_connection(&self, config: &SessionConfig) -> Option<(String, u8)> {
        let mut best_match: Option<(String, u8)> = None;

        for entry in self.connections.iter() {
            let conn = entry.value();
            let conn_id = entry.key().clone();

            // 1. 基础匹配：host + port + username
            if conn.config.host != config.host
                || conn.config.port != config.port
                || conn.config.username != config.username
            {
                continue;
            }

            // 2. 认证方式兼容性检查
            if !Self::auth_compatible(&conn.config.auth, &config.auth) {
                debug!(
                    "Connection {} auth not compatible, skipping reuse",
                    conn_id
                );
                continue;
            }

            // 3. 连接状态必须健康
            let state = conn.state().await;
            if state != ConnectionState::Active && state != ConnectionState::Idle {
                debug!(
                    "Connection {} state {:?} not healthy, skipping reuse",
                    conn_id, state
                );
                continue;
            }

            // 4. 底层连接必须活着
            if !conn.handle_controller.is_connected() {
                debug!("Connection {} handle disconnected, skipping reuse", conn_id);
                continue;
            }

            // 5. 心跳失败次数必须为 0
            let failures = conn.heartbeat_failures();
            if failures > 0 {
                debug!(
                    "Connection {} has {} heartbeat failures, skipping reuse",
                    conn_id, failures
                );
                continue;
            }

            // 计算复用质量分数 (0-100)
            let quality = self.calculate_reuse_quality(conn).await;

            // 选择质量最高的连接
            if best_match.is_none() || quality > best_match.as_ref().unwrap().1 {
                best_match = Some((conn_id, quality));
            }
        }

        if let Some((ref id, quality)) = best_match {
            info!(
                "Found reusable connection {} with quality {}",
                id, quality
            );
        }

        best_match
    }

    /// 检查两个认证方式是否兼容（可安全复用）
    fn auth_compatible(a: &AuthMethod, b: &AuthMethod) -> bool {
        match (a, b) {
            // 密码认证：必须完全相同
            (
                AuthMethod::Password { password: p1 },
                AuthMethod::Password { password: p2 },
            ) => p1 == p2,
            
            // 密钥认证：路径必须相同（passphrase 不比较，因为密钥已加载）
            (
                AuthMethod::Key { key_path: k1, .. },
                AuthMethod::Key { key_path: k2, .. },
            ) => k1 == k2,
            
            // Agent 认证：总是兼容
            (AuthMethod::Agent, AuthMethod::Agent) => true,
            
            // 不同类型不兼容
            _ => false,
        }
    }

    /// 计算连接复用质量分数
    async fn calculate_reuse_quality(&self, conn: &ConnectionEntry) -> u8 {
        let mut score: u8 = 100;

        // 状态评估：Active 最优，Idle 次之
        let state = conn.state().await;
        if state == ConnectionState::Idle {
            score = score.saturating_sub(10); // Idle 扣 10 分
        }

        // 引用计数评估：引用越少越好（资源争用少）
        let ref_count = conn.ref_count();
        if ref_count > 5 {
            score = score.saturating_sub(20);
        } else if ref_count > 2 {
            score = score.saturating_sub(10);
        }

        // 空闲时间评估：最近活动的更好
        let now = Utc::now().timestamp() as u64;
        let last_active = conn.last_active.load(Ordering::SeqCst);
        let idle_secs = now.saturating_sub(last_active);
        if idle_secs > 300 {
            // 空闲超过 5 分钟
            score = score.saturating_sub(15);
        } else if idle_secs > 60 {
            // 空闲超过 1 分钟
            score = score.saturating_sub(5);
        }

        score
    }

    /// 获取连接（增加引用计数）
    ///
    /// 调用者使用完后必须调用 `release`
    pub async fn acquire(
        &self,
        connection_id: &str,
    ) -> Result<HandleController, ConnectionRegistryError> {
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        let conn = entry.value();

        // 检查连接状态
        let state = conn.state().await;
        if state == ConnectionState::Disconnected || state == ConnectionState::Disconnecting {
            return Err(ConnectionRegistryError::AlreadyDisconnected);
        }

        // 增加引用计数
        let prev_count = conn.ref_count();
        conn.add_ref();

        // 如果从 0 变为 1，取消空闲计时器，状态变为 Active
        if prev_count == 0 {
            conn.cancel_idle_timer().await;
            conn.set_state(ConnectionState::Active).await;
            info!(
                "Connection {} reactivated (ref_count: 0 -> 1)",
                connection_id
            );

            // Oxide-Next Phase 2: Idle → Active 事件
            if let Some(emitter) = self.node_emitter() {
                emitter.emit_state_from_connection(
                    connection_id,
                    &ConnectionState::Active,
                    "reactivated",
                );
            }
        }

        Ok(conn.handle_controller.clone())
    }

    /// 释放连接引用（减少引用计数）
    ///
    /// 当引用计数归零时，启动空闲计时器
    pub async fn release(&self, connection_id: &str) -> Result<(), ConnectionRegistryError> {
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        let conn = entry.value().clone();
        drop(entry); // 释放 DashMap 锁

        // 减少引用计数
        let new_count = conn.release();

        // 如果引用计数归零，启动空闲计时器
        if new_count == 0 {
            let keep_alive = conn.is_keep_alive().await;
            if keep_alive {
                info!(
                    "Connection {} idle but keep_alive=true, not starting timer",
                    connection_id
                );
                conn.set_state(ConnectionState::Idle).await;

                // Oxide-Next Phase 2: Active → Idle 事件
                if let Some(emitter) = self.node_emitter() {
                    emitter.emit_state_from_connection(
                        connection_id,
                        &ConnectionState::Idle,
                        "idle (keep_alive)",
                    );
                }
            } else {
                self.start_idle_timer(&conn).await;
            }
        }

        Ok(())
    }

    /// 启动空闲计时器
    async fn start_idle_timer(&self, conn: &Arc<ConnectionEntry>) {
        let connection_id = conn.id.clone();
        let timeout = self.idle_timeout().await;

        info!(
            "Connection {} idle, starting {} minute timer",
            connection_id,
            timeout.as_secs() / 60
        );

        conn.set_state(ConnectionState::Idle).await;

        // Oxide-Next Phase 2: Active → Idle 事件
        if let Some(emitter) = self.node_emitter() {
            emitter.emit_state_from_connection(
                &connection_id,
                &ConnectionState::Idle,
                "idle (timer started)",
            );
        }

        let conn_clone = conn.clone();
        let connections = self.connections.clone();
        let node_emitter = self.node_emitter();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(timeout).await;

            // 超时到期，检查是否仍然空闲
            if conn_clone.ref_count() == 0 {
                info!(
                    "Connection {} idle timeout expired, disconnecting",
                    connection_id
                );

                // 断开连接
                conn_clone.clear_sftp().await; // Oxide-Next Phase 1.5: 清理 SFTP
                conn_clone.handle_controller.disconnect().await;
                conn_clone.set_state(ConnectionState::Disconnected).await;

                // Oxide-Next Phase 2: 空闲超时 → Disconnected 事件
                if let Some(ref emitter) = node_emitter {
                    emitter.emit_sftp_ready(&connection_id, false, None);
                    emitter.emit_state_from_connection(
                        &connection_id,
                        &ConnectionState::Disconnected,
                        "idle timeout",
                    );
                    // 注销映射
                    emitter.unregister(&connection_id);
                }

                // 从注册表移除
                connections.remove(&connection_id);

                info!("Connection {} removed from registry", connection_id);
            } else {
                debug!(
                    "Connection {} idle timer expired but ref_count > 0, ignoring",
                    connection_id
                );
            }
        });

        conn.set_idle_timer(handle).await;
    }

    /// 强制断开连接
    /// 
    /// 如果此连接有子连接（隧道连接），会先断开所有子连接。
    /// 如果此连接是子连接，会减少父连接的引用计数。
    pub async fn disconnect(
        &self,
        connection_id: &str,
    ) -> Result<(), ConnectionRegistryError> {
        // 1. 收集所有依赖此连接的子连接
        let child_ids: Vec<String> = self
            .connections
            .iter()
            .filter(|e| e.value().parent_connection_id.as_deref() == Some(connection_id))
            .map(|e| e.key().clone())
            .collect();

        // 2. 先批量减少当前连接的引用计数（因为这些子连接即将断开）
        // 这样避免了递归断开时的竞态条件
        if !child_ids.is_empty() {
            if let Some(entry) = self.connections.get(connection_id) {
                let conn = entry.value();
                for _ in &child_ids {
                    conn.release();
                }
                debug!(
                    "Pre-released {} ref_counts for connection {} (children about to disconnect)",
                    child_ids.len(),
                    connection_id
                );
            }
        }

        // 3. 断开所有子连接（子连接断开时不再减少父引用计数，因为已经预先减少）
        for child_id in &child_ids {
            info!(
                "Disconnecting child connection {} (parent: {})",
                child_id, connection_id
            );
            // 递归断开子连接，但跳过引用计数减少（使用内部方法）
            if let Err(e) = Box::pin(self.disconnect_without_parent_release(child_id)).await {
                warn!("Failed to disconnect child connection {}: {}", child_id, e);
            }
        }

        // 4. 断开当前连接
        self.disconnect_single(connection_id).await
    }

    /// 断开单个连接（内部方法，处理引用计数）
    async fn disconnect_single(
        &self,
        connection_id: &str,
    ) -> Result<(), ConnectionRegistryError> {
        // 获取当前连接
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        let conn = entry.value().clone();
        let parent_id = conn.parent_connection_id.clone();
        drop(entry);

        info!("Force disconnecting connection {}", connection_id);

        // 取消空闲计时器
        conn.cancel_idle_timer().await;

        // 取消心跳任务（避免断开后心跳任务继续运行报错）
        conn.cancel_heartbeat().await;

        // 取消重连任务（如果有）
        conn.cancel_reconnect().await;

        // Oxide-Next Phase 1.5: 清理 SFTP session
        conn.clear_sftp().await;

        // 设置状态为断开中
        conn.set_state(ConnectionState::Disconnecting).await;

        // 断开 SSH 连接
        conn.handle_controller.disconnect().await;

        // 设置状态为已断开
        conn.set_state(ConnectionState::Disconnected).await;

        // Oxide-Next Phase 2: 发射断开事件 + SFTP 销毁 + 注销映射
        if let Some(emitter) = self.node_emitter() {
            emitter.emit_sftp_ready(connection_id, false, None);
            emitter.emit_state_from_connection(
                connection_id,
                &ConnectionState::Disconnected,
                "force disconnect",
            );
            emitter.unregister(connection_id);
        }

        // 从注册表移除
        self.connections.remove(connection_id);

        info!("Connection {} disconnected and removed", connection_id);

        // 如果是隧道连接，减少父连接的引用计数
        if let Some(parent_id) = parent_id {
            if let Some(parent_entry) = self.connections.get(&parent_id) {
                let parent_conn = parent_entry.value();
                parent_conn.release();
                debug!(
                    "Parent connection {} ref_count decreased (child {} disconnected)",
                    parent_id, connection_id
                );
            }
        }

        Ok(())
    }

    /// 断开连接但不减少父连接引用计数（用于批量断开时已预先减少的情况）
    async fn disconnect_without_parent_release(
        &self,
        connection_id: &str,
    ) -> Result<(), ConnectionRegistryError> {
        // 先递归处理子连接
        let child_ids: Vec<String> = self
            .connections
            .iter()
            .filter(|e| e.value().parent_connection_id.as_deref() == Some(connection_id))
            .map(|e| e.key().clone())
            .collect();

        // 预先减少引用计数
        if !child_ids.is_empty() {
            if let Some(entry) = self.connections.get(connection_id) {
                let conn = entry.value();
                for _ in &child_ids {
                    conn.release();
                }
            }
        }

        // 递归断开子连接
        for child_id in &child_ids {
            if let Err(e) = Box::pin(self.disconnect_without_parent_release(child_id)).await {
                warn!("Failed to disconnect child connection {}: {}", child_id, e);
            }
        }

        // 断开当前连接（不减少父引用计数）
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        let conn = entry.value().clone();
        drop(entry);

        conn.cancel_idle_timer().await;
        conn.cancel_heartbeat().await;
        conn.cancel_reconnect().await;
        conn.clear_sftp().await; // Oxide-Next Phase 1.5
        conn.set_state(ConnectionState::Disconnecting).await;
        conn.handle_controller.disconnect().await;
        conn.set_state(ConnectionState::Disconnected).await;

        // Oxide-Next Phase 2: 发射断开事件 + SFTP 销毁 + 注销映射
        if let Some(emitter) = self.node_emitter() {
            emitter.emit_sftp_ready(connection_id, false, None);
            emitter.emit_state_from_connection(
                connection_id,
                &ConnectionState::Disconnected,
                "cascade disconnect",
            );
            emitter.unregister(connection_id);
        }

        self.connections.remove(connection_id);

        info!("Connection {} disconnected and removed (no parent release)", connection_id);
        Ok(())
    }

    /// 断开所有连接（应用退出时调用）
    pub async fn disconnect_all(&self) {
        info!("Disconnecting all SSH connections...");

        let connection_ids: Vec<String> = self.connections.iter().map(|e| e.key().clone()).collect();

        for connection_id in connection_ids {
            if let Err(e) = self.disconnect(&connection_id).await {
                warn!("Failed to disconnect {}: {}", connection_id, e);
            }
        }

        info!("All SSH connections disconnected");
    }

    /// 检查连接是否存活
    pub fn is_alive(&self, connection_id: &str) -> bool {
        self.connections
            .get(connection_id)
            .map(|e| e.handle_controller.is_connected())
            .unwrap_or(false)
    }

    /// 获取连接信息
    pub async fn get_info(
        &self,
        connection_id: &str,
    ) -> Option<ConnectionInfo> {
        let entry = self.connections.get(connection_id)?;
        Some(entry.value().to_info().await)
    }

    /// 列出所有连接
    pub async fn list_connections(&self) -> Vec<ConnectionInfo> {
        let mut result = Vec::with_capacity(self.connections.len());
        for entry in self.connections.iter() {
            result.push(entry.value().to_info().await);
        }
        result
    }

    /// 注册已存在的连接（用于 connect_v2 集成）
    ///
    /// 将 connect_v2 创建的 HandleController 注册到连接池，
    /// 使连接池面板能够显示这些连接。
    ///
    /// # Arguments
    /// * `connection_id` - 连接 ID（通常使用 session_id）
    /// * `config` - 会话配置
    /// * `handle_controller` - 已创建的 HandleController
    /// * `session_id` - 关联的 terminal session ID
    ///
    /// # Returns
    /// * 返回连接 ID
    pub async fn register_existing(
        &self,
        connection_id: String,
        config: SessionConfig,
        handle_controller: HandleController,
        session_id: String,
    ) -> String {
        info!(
            "Registering existing connection {} for session {}",
            connection_id, session_id
        );

        // 创建连接条目
        let entry = Arc::new(ConnectionEntry {
            id: connection_id.clone(),
            config,
            handle_controller,
            state: RwLock::new(ConnectionState::Active),
            ref_count: AtomicU32::new(1), // 初始引用计数为 1（对应 terminal）
            last_active: AtomicU64::new(Utc::now().timestamp() as u64),
            keep_alive: RwLock::new(false),
            created_at: Utc::now(),
            idle_timer: Mutex::new(None),
            terminal_ids: RwLock::new(vec![session_id]),
            sftp_session_id: RwLock::new(None),
            sftp: tokio::sync::Mutex::new(None),
            forward_ids: RwLock::new(Vec::new()),
            heartbeat_task: Mutex::new(None),
            heartbeat_failures: AtomicU32::new(0),
            reconnect_task: Mutex::new(None),
            is_reconnecting: AtomicBool::new(false),
            reconnect_attempts: AtomicU32::new(0),
            current_attempt_id: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            parent_connection_id: None, // 从旧连接注册，无父连接
            remote_env: RwLock::new(None), // 待异步检测
        });

        self.connections.insert(connection_id.clone(), entry.clone());

        info!(
            "Connection {} registered, total connections: {}",
            connection_id,
            self.connections.len()
        );

        // 启动远程环境检测（异步，不阻塞）
        // 使用 inner 版本因为 register_existing 没有 Arc<Self>
        let app_handle = self.app_handle.blocking_read().clone();
        Self::spawn_env_detection_inner(entry, connection_id.clone(), app_handle);

        connection_id
    }

    /// 获取连接数量
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// 设置 keep_alive 标志
    pub async fn set_keep_alive(
        &self,
        connection_id: &str,
        keep_alive: bool,
    ) -> Result<(), ConnectionRegistryError> {
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        let conn = entry.value();
        conn.set_keep_alive(keep_alive).await;

        info!(
            "Connection {} keep_alive set to {}",
            connection_id, keep_alive
        );

        // 如果当前是空闲状态且 keep_alive=true，取消计时器
        if keep_alive && conn.state().await == ConnectionState::Idle {
            conn.cancel_idle_timer().await;
        }

        Ok(())
    }

    /// 获取 HandleController（不增加引用计数）
    ///
    /// 用于内部操作，调用者需要自行管理生命周期
    pub fn get_handle_controller(&self, connection_id: &str) -> Option<HandleController> {
        self.connections
            .get(connection_id)
            .map(|e| e.handle_controller.clone())
    }

    /// 添加关联的 terminal session
    pub async fn add_terminal(
        &self,
        connection_id: &str,
        session_id: String,
    ) -> Result<(), ConnectionRegistryError> {
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        entry.value().add_terminal(session_id).await;
        Ok(())
    }

    /// 移除关联的 terminal session
    pub async fn remove_terminal(
        &self,
        connection_id: &str,
        session_id: &str,
    ) -> Result<(), ConnectionRegistryError> {
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        entry.value().remove_terminal(session_id).await;
        Ok(())
    }

    /// 设置关联的 SFTP session
    pub async fn set_sftp_session(
        &self,
        connection_id: &str,
        session_id: Option<String>,
    ) -> Result<(), ConnectionRegistryError> {
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        entry.value().set_sftp_session(session_id).await;
        Ok(())
    }

    /// 添加关联的 forward
    pub async fn add_forward(
        &self,
        connection_id: &str,
        forward_id: String,
    ) -> Result<(), ConnectionRegistryError> {
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        entry.value().add_forward(forward_id).await;
        Ok(())
    }

    /// 移除关联的 forward
    pub async fn remove_forward(
        &self,
        connection_id: &str,
        forward_id: &str,
    ) -> Result<(), ConnectionRegistryError> {
        let entry = self
            .connections
            .get(connection_id)
            .ok_or_else(|| ConnectionRegistryError::NotFound(connection_id.to_string()))?;

        entry.value().remove_forward(forward_id).await;
        Ok(())
    }

    /// 根据 session_id 查找 connection_id
    pub async fn find_by_terminal(&self, session_id: &str) -> Option<String> {
        for entry in self.connections.iter() {
            let terminal_ids = entry.value().terminal_ids().await;
            if terminal_ids.contains(&session_id.to_string()) {
                return Some(entry.key().clone());
            }
        }
        None
    }

    /// 启动连接的心跳监控任务
    ///
    /// 每 15 秒发送一次心跳，连续 2 次失败后标记为 LinkDown 并启动重连
    pub fn start_heartbeat(self: &Arc<Self>, connection_id: &str) {
        let Some(entry) = self.connections.get(connection_id) else {
            warn!("Cannot start heartbeat for non-existent connection {}", connection_id);
            return;
        };

        let conn = entry.value().clone();
        let registry = Arc::clone(self);
        let connection_id = connection_id.to_string();
        let node_emitter = self.node_emitter(); // Oxide-Next Phase 2

        let task = tokio::spawn(async move {
            info!("Heartbeat task started for connection {} (interval={}s, threshold={})", 
                  connection_id, HEARTBEAT_INTERVAL.as_secs(), HEARTBEAT_FAIL_THRESHOLD);
            let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);

            loop {
                interval.tick().await;
                debug!("Heartbeat tick for connection {}", connection_id);

                // 检查连接状态，如果正在重连或已断开，停止心跳
                let state = conn.state().await;
                if matches!(state, ConnectionState::Reconnecting | ConnectionState::Disconnecting | ConnectionState::Disconnected) {
                    debug!("Connection {} state is {:?}, stopping heartbeat", connection_id, state);
                    break;
                }

                // 发送心跳 ping
                let ping_result = conn.handle_controller.ping().await;
                debug!("Connection {} ping result: {:?}", connection_id, ping_result);

                match ping_result {
                    crate::ssh::handle_owner::PingResult::Ok => {
                        // 心跳成功，重置失败计数
                        conn.reset_heartbeat_failures();
                        conn.update_activity();
                        debug!("Connection {} heartbeat OK", connection_id);
                    }
                    crate::ssh::handle_owner::PingResult::IoError => {
                        // IO 错误检测到 — 执行 quick probe 确认（Smart Butler 模式）
                        // 延迟 1.5s 后二次探测，避免瞬态网络抖动导致误判
                        warn!("Connection {} IO error detected, initiating quick probe confirmation", connection_id);
                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

                        // 检查连接是否已被其他路径处理（如用户主动断开）
                        let state_after_delay = conn.state().await;
                        if matches!(state_after_delay, ConnectionState::Disconnecting | ConnectionState::Disconnected) {
                            info!("Connection {} already disconnecting/disconnected during probe delay, stopping heartbeat", connection_id);
                            break;
                        }

                        // 二次探测 — 如果成功则证明是瞬态抖动，恢复正常心跳
                        let probe_result = conn.handle_controller.ping().await;
                        match probe_result {
                            crate::ssh::handle_owner::PingResult::Ok => {
                                info!("Connection {} quick probe succeeded — transient glitch, resuming heartbeat", connection_id);
                                conn.reset_heartbeat_failures();
                                conn.update_activity();
                                continue;
                            }
                            _ => {
                                // 二次探测也失败，确认链路断开
                                // 🛑 后端禁止自动重连：只广播事件，等待前端指令
                                error!("Connection {} quick probe also failed ({:?}), confirmed link_down", connection_id, probe_result);
                                conn.set_state(ConnectionState::LinkDown).await;
                                registry.emit_connection_status_changed(&connection_id, "link_down").await;

                                // Oxide-Next Phase 2: node:state 事件
                                if let Some(ref emitter) = node_emitter {
                                    emitter.emit_state_from_connection(
                                        &connection_id,
                                        &ConnectionState::LinkDown,
                                        "heartbeat IO error (confirmed after probe)",
                                    );
                                }

                                break;
                            }
                        }
                    }
                    crate::ssh::handle_owner::PingResult::Timeout => {
                        // 超时，累计失败次数
                        let failures = conn.increment_heartbeat_failures();
                        warn!(
                            "Connection {} heartbeat timeout ({}/{})",
                            connection_id, failures, HEARTBEAT_FAIL_THRESHOLD
                        );

                        if failures >= HEARTBEAT_FAIL_THRESHOLD {
                            // 达到失败阈值，标记为 LinkDown
                            // 🛑 后端禁止自动重连：只广播事件，等待前端指令
                            error!("Connection {} marked as LinkDown after {} heartbeat failures", 
                                   connection_id, failures);
                            conn.set_state(ConnectionState::LinkDown).await;

                            // 广播状态变更事件
                            registry.emit_connection_status_changed(&connection_id, "link_down").await;

                            // Oxide-Next Phase 2: node:state 事件
                            if let Some(ref emitter) = node_emitter {
                                emitter.emit_state_from_connection(
                                    &connection_id,
                                    &ConnectionState::LinkDown,
                                    "heartbeat timeout threshold",
                                );
                            }

                            // ❌ 已删除: registry.start_reconnect(&connection_id).await;
                            // 后端只广播，前端决定是否重连

                            break;
                        }
                    }
                }
            }

            info!("Heartbeat task stopped for connection {}", connection_id);
        });

        // 保存任务句柄（需要在 spawn 之后异步设置）
        let conn = entry.value().clone();
        tokio::spawn(async move {
            conn.set_heartbeat_task(task).await;
        });
    }

    /// Spawn remote environment detection task
    ///
    /// Runs asynchronously after connection establishment. Results are cached
    /// in ConnectionEntry and emitted as `env:detected:{connection_id}` event.
    pub fn spawn_env_detection(self: &Arc<Self>, connection_id: &str) {
        use crate::session::env_detector::detect_remote_env;
        use tauri::Emitter;

        let Some(entry) = self.connections.get(connection_id) else {
            warn!("Cannot spawn env detection for non-existent connection {}", connection_id);
            return;
        };

        let conn = entry.value().clone();
        let registry = Arc::clone(self);
        let connection_id = connection_id.to_string();
        let controller = conn.handle_controller.clone();

        tokio::spawn(async move {
            info!("[EnvDetector] Starting detection for connection {}", connection_id);

            // Run detection
            let env_info = detect_remote_env(&controller, &connection_id).await;

            info!(
                "[EnvDetector] Detection complete for {}: os_type={}",
                connection_id, env_info.os_type
            );

            // Cache result in ConnectionEntry
            conn.set_remote_env(env_info.clone()).await;

            // Emit event to frontend
            let app_handle = registry.app_handle.read().await;
            if let Some(handle) = app_handle.as_ref() {
                #[derive(Clone, serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct EnvDetectedEvent {
                    connection_id: String,
                    #[serde(flatten)]
                    env: RemoteEnvInfo,
                }
                
                let event = EnvDetectedEvent {
                    connection_id: connection_id.clone(),
                    env: env_info,
                };
                
                if let Err(e) = handle.emit("env:detected", &event) {
                    error!("[EnvDetector] Failed to emit env:detected for {}: {}", connection_id, e);
                } else {
                    debug!("[EnvDetector] Emitted env:detected event for {}", connection_id);
                }
            } else {
                warn!("[EnvDetector] AppHandle not available, event not emitted for {}", connection_id);
            }
        });
    }

    /// Spawn env detection without needing Arc<Self> (for `register_existing`)
    ///
    /// Like `spawn_env_detection` but doesn't need self. Uses provided entry and app_handle.
    fn spawn_env_detection_inner(
        conn: Arc<ConnectionEntry>,
        connection_id: String,
        app_handle: Option<AppHandle>,
    ) {
        use crate::session::env_detector::detect_remote_env;
        use tauri::Emitter;

        let controller = conn.handle_controller.clone();

        tokio::spawn(async move {
            info!("[EnvDetector] Starting detection for connection {}", connection_id);

            let env_info = detect_remote_env(&controller, &connection_id).await;

            info!(
                "[EnvDetector] Detection complete for {}: os_type={}",
                connection_id, env_info.os_type
            );

            conn.set_remote_env(env_info.clone()).await;

            if let Some(handle) = app_handle {
                #[derive(Clone, serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct EnvDetectedEvent {
                    connection_id: String,
                    #[serde(flatten)]
                    env: RemoteEnvInfo,
                }
                
                let event = EnvDetectedEvent {
                    connection_id: connection_id.clone(),
                    env: env_info,
                };
                
                if let Err(e) = handle.emit("env:detected", &event) {
                    error!("[EnvDetector] Failed to emit env:detected for {}: {}", connection_id, e);
                } else {
                    debug!("[EnvDetector] Emitted env:detected event for {}", connection_id);
                }
            } else {
                warn!("[EnvDetector] AppHandle not available, event not emitted for {}", connection_id);
            }
        });
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // 🛑 AUTO-RECONNECT ENGINE - PHYSICALLY REMOVED
    // ═══════════════════════════════════════════════════════════════════════════════
    //
    // 以下函数已被物理删除，后端禁止自主重连：
    // - start_reconnect: 启动重连任务
    // - try_reconnect: 尝试重连（路由）
    // - try_reconnect_direct: 直连重连
    // - try_reconnect_tunneled: 隧道重连
    //
    // 设计原则：后端是纯执行器，只响应前端的 connect_tree_node 命令。
    // 所有重连逻辑必须由前端的 connectNodeWithAncestors 驱动。
    // ═══════════════════════════════════════════════════════════════════════════════

    /// 🛑 REMOVED: start_reconnect
    /// 
    /// 此函数已被物理删除。后端禁止自主启动重连任务。
    /// 前端应通过 connect_tree_node 命令发起重连。
    #[allow(dead_code)]
    pub async fn start_reconnect(self: &Arc<Self>, _connection_id: &str) {
        // 🛑 NO-OP: 后端禁止自主重连
        tracing::warn!("🛑 start_reconnect called but DISABLED - backend cannot auto-reconnect");
    }

    /// 广播连接状态变更事件
    /// 
    /// # 状态守卫
    /// 只有当状态真正变化时才发送事件，避免重复发送相同状态导致前端性能问题
    /// 
    /// # AppHandle 生命周期
    /// 如果 AppHandle 未就绪，事件会被缓存，待 AppHandle 设置后立即发送
    pub async fn emit_connection_status_changed(&self, connection_id: &str, status: &str) {
        // 对于 link_down 状态，使用带子连接的版本
        if status == "link_down" {
            let affected_children = self.collect_all_children(connection_id);
            self.emit_connection_status_changed_with_children(connection_id, status, affected_children).await;
            return;
        }
        
        // 其他状态使用空的 affected_children
        self.emit_connection_status_changed_with_children(connection_id, status, vec![]).await;
    }

    /// 广播连接状态变更事件（带受影响的子连接列表）
    /// 
    /// # 状态守卫
    /// 只有当状态真正变化时才发送事件，避免重复发送相同状态导致前端性能问题
    async fn emit_connection_status_changed_with_children(
        &self, 
        connection_id: &str, 
        status: &str,
        affected_children: Vec<String>,
    ) {
        // === 状态守卫：检查是否需要发送 ===
        if let Some(entry) = self.connections.get(connection_id) {
            let conn = entry.value();
            let mut last_status = conn.last_emitted_status.write().await;
            
            // 如果状态未变化，跳过发送
            if let Some(ref prev) = *last_status {
                if prev == status {
                    debug!("Status unchanged for connection {}: {}, skipping emit", connection_id, status);
                    return;
                }
            }
            
            // 更新最后发送的状态
            *last_status = Some(status.to_string());
        }
        
        // === 尝试发送事件 ===
        let app_handle = self.app_handle.read().await;
        if let Some(handle) = app_handle.as_ref() {
            use tauri::Emitter;
            
            #[derive(Clone, serde::Serialize)]
            struct ConnectionStatusEvent {
                connection_id: String,
                status: String,
                affected_children: Vec<String>,
                timestamp: u64,
            }

            let event = ConnectionStatusEvent {
                connection_id: connection_id.to_string(),
                status: status.to_string(),
                affected_children,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };

            if let Err(e) = handle.emit("connection_status_changed", event) {
                error!("Failed to emit connection_status_changed: {}", e);
            } else {
                debug!("Emitted connection_status_changed: {} -> {}", connection_id, status);
            }
        } else {
            // AppHandle 未就绪，缓存事件（上限 1000 条防止无限堆积）
            warn!("AppHandle not ready, caching event: {} -> {}", connection_id, status);
            let mut pending = self.pending_events.lock().await;
            if pending.len() < 1000 {
                pending.push((connection_id.to_string(), status.to_string()));
                debug!("Event cached, total pending: {}", pending.len());
            } else {
                warn!("Pending events buffer full (1000), dropping event: {} -> {}", connection_id, status);
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // 🛑 RECONNECT HELPER FUNCTIONS - PHYSICALLY REMOVED
    // ═══════════════════════════════════════════════════════════════════════════════
    //
    // 以下辅助函数已被物理删除（它们只服务于已删除的重连逻辑）：
    // - replace_handle_controller: 重连后替换 HandleController
    // - emit_connection_reconnected: 广播重连成功事件
    // - emit_reconnect_progress: 广播重连进度事件
    // ═══════════════════════════════════════════════════════════════════════════════

    /// 收集所有后代连接（递归）
    /// 用于级联传播 link-down 状态
    fn collect_all_children(&self, connection_id: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut stack = vec![connection_id.to_string()];
        
        while let Some(current_id) = stack.pop() {
            for entry in self.connections.iter() {
                if entry.value().parent_connection_id.as_deref() == Some(&current_id) {
                    let child_id = entry.key().clone();
                    result.push(child_id.clone());
                    stack.push(child_id);
                }
            }
        }
        
        result
    }

    // ❌ 已删除: cascade_reconnect_children 函数
    // ❌ 已删除: try_cascade_reconnect_single 函数
    // 🛑 后端禁止级联重连，所有重连决策由前端驱动

    /// 获取连接条目（用于外部访问）
    pub fn get_connection(&self, connection_id: &str) -> Option<Arc<ConnectionEntry>> {
        self.connections.get(connection_id).map(|e| e.value().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_pool_config_default() {
        let config = ConnectionPoolConfig::default();
        assert_eq!(config.idle_timeout_secs, 30 * 60);
        assert_eq!(config.max_connections, 0);
        assert!(config.protect_on_exit);
    }

    #[tokio::test]
    async fn test_ref_count() {
        let entry = ConnectionEntry {
            id: "test".to_string(),
            config: SessionConfig {
                host: "localhost".to_string(),
                port: 22,
                username: "user".to_string(),
                auth: AuthMethod::Password {
                    password: "pass".to_string(),
                },
                name: None,
                color: None,
                cols: 80,
                rows: 24,
            },
            handle_controller: {
                // 创建一个 mock controller
                let (tx, _rx) = tokio::sync::mpsc::channel(1);
                HandleController::new(tx)
            },
            state: RwLock::new(ConnectionState::Active),
            ref_count: AtomicU32::new(0),
            last_active: AtomicU64::new(0),
            keep_alive: RwLock::new(false),
            created_at: Utc::now(),
            idle_timer: Mutex::new(None),
            terminal_ids: RwLock::new(Vec::new()),
            sftp_session_id: RwLock::new(None),
            sftp: tokio::sync::Mutex::new(None),
            forward_ids: RwLock::new(Vec::new()),
            heartbeat_task: Mutex::new(None),
            heartbeat_failures: AtomicU32::new(0),
            reconnect_task: Mutex::new(None),
            is_reconnecting: AtomicBool::new(false),
            reconnect_attempts: AtomicU32::new(0),
            current_attempt_id: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            parent_connection_id: None,
            remote_env: RwLock::new(None),
        };

        assert_eq!(entry.ref_count(), 0);
        assert_eq!(entry.add_ref(), 1);
        assert_eq!(entry.add_ref(), 2);
        assert_eq!(entry.release(), 1);
        assert_eq!(entry.release(), 0);
    }
}
