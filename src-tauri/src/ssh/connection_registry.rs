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
use crate::session::{AuthMethod, SessionConfig};

/// 默认空闲超时时间（30 分钟）
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// 心跳间隔（15 秒）
/// 配合 HEARTBEAT_FAIL_THRESHOLD=2，确保 30 秒内检测到断连
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// 心跳连续失败次数阈值，达到后标记为 LinkDown
/// 15s × 2 = 30s 内必触发重连
const HEARTBEAT_FAIL_THRESHOLD: u32 = 2;

/// 重连间隔（初始值，使用指数退避）
/// 优化：从 2s 降至 0.5s，加速短时断网恢复
const RECONNECT_INITIAL_DELAY: Duration = Duration::from_millis(500);

/// 首次重连延迟（快速首跳）
/// 设计：首次重连仅等待 200ms，瞬断场景近乎无感
const RECONNECT_FIRST_DELAY: Duration = Duration::from_millis(200);

/// 重连最大间隔
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);

/// 普通模式最大重连次数
const RECONNECT_MAX_ATTEMPTS: u32 = 5;

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
/// 5. `forward_ids` (RwLock)
/// 6. `last_emitted_status` (RwLock)
/// 7. `idle_timer` (Mutex)
/// 8. `heartbeat_task` (Mutex)
/// 9. `reconnect_task` (Mutex)
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
        }
    }

    /// 使用自定义配置创建
    pub fn with_config(config: ConnectionPoolConfig) -> Self {
        Self {
            connections: DashMap::new(),
            config: RwLock::new(config),
            app_handle: RwLock::new(None),
            pending_events: Mutex::new(Vec::new()),
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
            forward_ids: RwLock::new(Vec::new()),
            heartbeat_task: Mutex::new(None),
            heartbeat_failures: AtomicU32::new(0),
            reconnect_task: Mutex::new(None),
            is_reconnecting: AtomicBool::new(false),
            reconnect_attempts: AtomicU32::new(0),
            current_attempt_id: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            parent_connection_id: None, // 直连，无父连接
        });

        self.connections.insert(connection_id.clone(), entry);

        // 启动心跳检测
        self.start_heartbeat(&connection_id);

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
            inactivity_timeout: Some(std::time::Duration::from_secs(300)),
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
                let key = russh_keys::load_secret_key(key_path, passphrase.as_deref())
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Failed to load key: {}",
                            e
                        ))
                    })?;

                let key_with_hash =
                    russh_keys::key::PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), None)
                        .map_err(|e| {
                            ConnectionRegistryError::ConnectionFailed(format!(
                                "Failed to prepare key: {}",
                                e
                            ))
                        })?;

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
                let key = russh_keys::load_secret_key(key_path, passphrase.as_deref())
                    .map_err(|e| {
                        ConnectionRegistryError::ConnectionFailed(format!(
                            "Failed to load key: {}",
                            e
                        ))
                    })?;

                let cert = russh_keys::load_openssh_certificate(cert_path)
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
                agent.authenticate(&handle, &target_config.username).await.map_err(|e| {
                    ConnectionRegistryError::ConnectionFailed(format!(
                        "Agent authentication failed: {}",
                        e
                    ))
                })?;
                true
            }
            AuthMethod::KeyboardInteractive => {
                // KBI via proxy chain is not supported in MVP
                return Err(ConnectionRegistryError::ConnectionFailed(
                    "KeyboardInteractive authentication not supported via proxy chain".to_string(),
                ));
            }
        };

        if !authenticated {
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
            forward_ids: RwLock::new(Vec::new()),
            heartbeat_task: Mutex::new(None),
            heartbeat_failures: AtomicU32::new(0),
            reconnect_task: Mutex::new(None),
            is_reconnecting: AtomicBool::new(false),
            reconnect_attempts: AtomicU32::new(0),
            current_attempt_id: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            parent_connection_id: Some(parent_connection_id.to_string()), // 隧道连接，记录父连接
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

        let conn_clone = conn.clone();
        let connections = self.connections.clone();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(timeout).await;

            // 超时到期，检查是否仍然空闲
            if conn_clone.ref_count() == 0 {
                info!(
                    "Connection {} idle timeout expired, disconnecting",
                    connection_id
                );

                // 断开连接
                conn_clone.handle_controller.disconnect().await;
                conn_clone.set_state(ConnectionState::Disconnected).await;

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

        // 设置状态为断开中
        conn.set_state(ConnectionState::Disconnecting).await;

        // 断开 SSH 连接
        conn.handle_controller.disconnect().await;

        // 设置状态为已断开
        conn.set_state(ConnectionState::Disconnected).await;

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
        conn.set_state(ConnectionState::Disconnecting).await;
        conn.handle_controller.disconnect().await;
        conn.set_state(ConnectionState::Disconnected).await;
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
            forward_ids: RwLock::new(Vec::new()),
            heartbeat_task: Mutex::new(None),
            heartbeat_failures: AtomicU32::new(0),
            reconnect_task: Mutex::new(None),
            is_reconnecting: AtomicBool::new(false),
            reconnect_attempts: AtomicU32::new(0),
            current_attempt_id: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            parent_connection_id: None, // 从旧连接注册，无父连接
        });

        self.connections.insert(connection_id.clone(), entry);

        info!(
            "Connection {} registered, total connections: {}",
            connection_id,
            self.connections.len()
        );

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
                        // IO 错误，物理连接已断，立即触发重连
                        error!("Connection {} IO error detected, triggering immediate reconnect", connection_id);
                        conn.set_state(ConnectionState::LinkDown).await;
                        registry.emit_connection_status_changed(&connection_id, "link_down").await;
                        registry.start_reconnect(&connection_id).await;
                        break;
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
                            error!("Connection {} marked as LinkDown after {} heartbeat failures", 
                                   connection_id, failures);
                            conn.set_state(ConnectionState::LinkDown).await;

                            // 广播状态变更事件
                            registry.emit_connection_status_changed(&connection_id, "link_down").await;

                            // 启动重连
                            registry.start_reconnect(&connection_id).await;

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

    /// 启动连接重连任务
    async fn start_reconnect(self: &Arc<Self>, connection_id: &str) {
        let Some(entry) = self.connections.get(connection_id) else {
            return;
        };

        let conn = entry.value().clone();
        
        // 抢占式清理：取消旧的重连任务（如果存在）
        // 确保新任务不会与旧任务竞争
        if conn.is_reconnecting() {
            debug!("Connection {} cancelling previous reconnect task", connection_id);
            conn.cancel_reconnect().await;
        }

        // 生成新的 attempt_id（用于状态幂等检查）
        let attempt_id = conn.new_attempt_id();
        debug!("Connection {} starting reconnect with attempt_id={}", connection_id, attempt_id);

        let is_pinned = conn.is_keep_alive().await;
        let registry = Arc::clone(self);
        let connection_id = connection_id.to_string();
        let config = conn.config.clone();
        let conn_for_task = conn.clone();

        let task = tokio::spawn(async move {
            info!(
                "Reconnect task started for connection {} (pinned={}, attempt_id={})",
                connection_id, is_pinned, attempt_id
            );

            conn_for_task.set_state(ConnectionState::Reconnecting).await;
            registry.emit_connection_status_changed(&connection_id, "reconnecting").await;

            // 首跳提速：第一次重连使用短延迟，后续使用指数退避
            let mut delay = RECONNECT_FIRST_DELAY;
            let max_attempts = if is_pinned { u32::MAX } else { RECONNECT_MAX_ATTEMPTS };

            loop {
                // 状态幂等检查：如果 attempt_id 已经变化，说明新的重连任务已启动，当前任务应退出
                if conn_for_task.current_attempt_id() != attempt_id {
                    warn!(
                        "Connection {} reconnect task {} superseded by newer attempt {}, exiting",
                        connection_id, attempt_id, conn_for_task.current_attempt_id()
                    );
                    return;
                }

                let attempt = conn_for_task.increment_reconnect_attempts();
                info!(
                    "Connection {} reconnect attempt {}/{} (attempt_id={})",
                    connection_id,
                    attempt,
                    if is_pinned { "∞".to_string() } else { max_attempts.to_string() },
                    attempt_id
                );

                // 发送重连进度事件
                registry.emit_reconnect_progress(
                    &connection_id,
                    attempt,
                    if is_pinned { None } else { Some(max_attempts) },
                    delay.as_millis() as u64,
                ).await;

                // 等待延迟（首次 200ms，后续指数退避）
                tokio::time::sleep(delay).await;

                // 再次检查幂等性（延迟期间可能有新任务启动）
                if conn_for_task.current_attempt_id() != attempt_id {
                    warn!(
                        "Connection {} reconnect task {} superseded during delay, exiting",
                        connection_id, attempt_id
                    );
                    return;
                }

                // 尝试重连
                match registry.try_reconnect(&connection_id, &config).await {
                    Ok(new_controller) => {
                        // 最终幂等性检查：确保这个结果仍然有效
                        if conn_for_task.current_attempt_id() != attempt_id {
                            warn!(
                                "Connection {} reconnect task {} succeeded but superseded, discarding result",
                                connection_id, attempt_id
                            );
                            // 关闭新创建的连接，避免泄漏
                            drop(new_controller);
                            return;
                        }

                        info!("Connection {} reconnected successfully (attempt_id={})", connection_id, attempt_id);

                        // 获取关联的 terminal IDs 和 forward IDs（在更新前获取）
                        let terminal_ids = conn_for_task.terminal_ids().await;
                        let forward_ids = conn_for_task.forward_ids().await;

                        // 更新 handle_controller - 需要替换整个连接条目
                        // 注意：由于 ConnectionEntry 的字段是不可变的，我们需要创建新条目
                        // 这里简化处理：更新现有条目的状态，新的 handle_controller 通过事件传递
                        
                        conn_for_task.reset_heartbeat_failures();
                        conn_for_task.reset_reconnect_state();
                        conn_for_task.set_state(ConnectionState::Active).await;

                        // 用新的 HandleController 替换旧的连接条目
                        registry.replace_handle_controller(&connection_id, new_controller.clone()).await;

                        // 广播重连成功事件（包含需要恢复的 terminal 和 forward 信息）
                        registry.emit_connection_reconnected(
                            &connection_id,
                            terminal_ids,
                            forward_ids,
                        ).await;

                        // 广播状态变更事件
                        registry.emit_connection_status_changed(&connection_id, "connected").await;

                        // 重新启动心跳
                        registry.start_heartbeat(&connection_id);

                        // 🔴 新增：触发子连接级联重连
                        registry.cascade_reconnect_children(&connection_id).await;

                        break;
                    }
                    Err(e) => {
                        warn!("Connection {} reconnect attempt {} failed: {}", connection_id, attempt, e);

                        if !is_pinned && attempt >= max_attempts {
                            // 普通模式：达到最大重连次数，放弃
                            error!(
                                "Connection {} reconnect failed after {} attempts, giving up",
                                connection_id, attempt
                            );
                            conn_for_task.set_state(ConnectionState::Disconnected).await;
                            registry.emit_connection_status_changed(&connection_id, "disconnected").await;

                            // 清理连接
                            registry.connections.remove(&connection_id);
                            break;
                        }

                        // 增加延迟（指数退避）
                        // 首次失败后从 RECONNECT_INITIAL_DELAY 开始，然后倍增
                        if delay == RECONNECT_FIRST_DELAY {
                            delay = RECONNECT_INITIAL_DELAY;
                        } else {
                            delay = std::cmp::min(delay * 2, RECONNECT_MAX_DELAY);
                        }
                    }
                }
            }

            info!("Reconnect task stopped for connection {}", connection_id);
        });

        // 保存任务句柄
        tokio::spawn(async move {
            conn.set_reconnect_task(task).await;
        });
    }

    /// 尝试重连
    /// 
    /// 支持直连和隧道连接两种模式：
    /// - 直连：直接建立 SSH 连接
    /// - 隧道连接：先检查父连接状态，然后通过父连接建立 direct-tcpip 隧道
    async fn try_reconnect(
        &self,
        connection_id: &str,
        config: &SessionConfig,
    ) -> Result<HandleController, String> {
        // 检查是否为隧道连接
        let parent_connection_id = self.connections.get(connection_id)
            .and_then(|e| e.value().parent_connection_id.clone());

        if let Some(parent_id) = parent_connection_id {
            // 隧道连接：需要通过父连接重连
            return self.try_reconnect_tunneled(connection_id, &parent_id, config).await;
        }

        // 直连模式
        self.try_reconnect_direct(connection_id, config).await
    }

    /// 直连模式重连
    async fn try_reconnect_direct(
        &self,
        connection_id: &str,
        config: &SessionConfig,
    ) -> Result<HandleController, String> {
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
                    return Err(
                        "KeyboardInteractive sessions cannot be auto-reconnected. Please manually reconnect with 2FA."
                            .to_string(),
                    );
                }
            },
            timeout_secs: 30,
            cols: config.cols,
            rows: config.rows,
            proxy_chain: None,
            strict_host_key_checking: false,
        };

        // 尝试建立新连接
        let client = SshClient::new(ssh_config);
        let session = client
            .connect()
            .await
            .map_err(|e| e.to_string())?;

        // 启动 Handle Owner Task
        let handle_controller = session.start(connection_id.to_string());

        Ok(handle_controller)
    }

    /// 隧道连接模式重连
    async fn try_reconnect_tunneled(
        &self,
        connection_id: &str,
        parent_connection_id: &str,
        config: &SessionConfig,
    ) -> Result<HandleController, String> {
        // 1. 获取父连接
        let parent_entry = self.connections.get(parent_connection_id)
            .ok_or_else(|| format!("Parent connection {} not found", parent_connection_id))?;
        
        let parent_conn = parent_entry.value().clone();
        drop(parent_entry);

        // 2. 检查父连接状态
        let parent_state = parent_conn.state().await;
        if parent_state != ConnectionState::Active && parent_state != ConnectionState::Idle {
            return Err(format!(
                "Parent connection {} is not available (state: {:?}), cannot reconnect tunneled connection",
                parent_connection_id, parent_state
            ));
        }

        info!(
            "Reconnecting tunneled connection {} via parent {}",
            connection_id, parent_connection_id
        );

        // 3. 通过父连接打开 direct-tcpip 隧道
        let channel = parent_conn
            .handle_controller
            .open_direct_tcpip(
                &config.host,
                config.port as u32,
                "127.0.0.1",
                0,
            )
            .await
            .map_err(|e| format!("Failed to open direct-tcpip channel: {}", e))?;

        debug!("Direct-tcpip channel opened to {}:{}", config.host, config.port);

        // 4. 将 channel 转换为 stream 用于 SSH-over-SSH
        let stream = channel.into_stream();

        // 5. 在隧道上建立新的 SSH 连接
        let ssh_config = russh::client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(300)),
            keepalive_interval: Some(std::time::Duration::from_secs(30)),
            keepalive_max: 3,
            ..Default::default()
        };

        let handler = super::client::ClientHandler::new(
            config.host.clone(),
            config.port,
            false, // 隧道连接不严格检查主机密钥
        );

        // 使用 russh::connect_stream 在隧道上建立 SSH
        let mut handle = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            russh::client::connect_stream(std::sync::Arc::new(ssh_config), stream, handler),
        )
        .await
        .map_err(|_| format!(
            "Reconnection to {}:{} via tunnel timed out",
            config.host, config.port
        ))?
        .map_err(|e| format!("Failed to reconnect via tunnel: {}", e))?;

        debug!("SSH handshake via tunnel completed for reconnection");

        // 6. 认证
        let authenticated = match &config.auth {
            AuthMethod::Password { password } => {
                handle
                    .authenticate_password(&config.username, password)
                    .await
                    .map_err(|e| format!("Authentication failed: {}", e))?
            }
            AuthMethod::Key { key_path, passphrase } => {
                let key = russh_keys::load_secret_key(key_path, passphrase.as_deref())
                    .map_err(|e| format!("Failed to load key: {}", e))?;

                let key_with_hash =
                    russh_keys::key::PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), None)
                        .map_err(|e| format!("Failed to prepare key: {}", e))?;

                handle
                    .authenticate_publickey(&config.username, key_with_hash)
                    .await
                    .map_err(|e| format!("Authentication failed: {}", e))?
            }
            AuthMethod::Certificate { key_path, cert_path, passphrase } => {
                let key = russh_keys::load_secret_key(key_path, passphrase.as_deref())
                    .map_err(|e| format!("Failed to load key: {}", e))?;

                let cert = russh_keys::load_openssh_certificate(cert_path)
                    .map_err(|e| format!("Failed to load certificate: {}", e))?;

                handle
                    .authenticate_openssh_cert(&config.username, std::sync::Arc::new(key), cert)
                    .await
                    .map_err(|e| format!("Certificate authentication failed: {}", e))?
            }
            AuthMethod::Agent => {
                let mut agent = crate::ssh::agent::SshAgentClient::connect()
                    .await
                    .map_err(|e| format!("Failed to connect to SSH agent: {}", e))?;
                
                agent.authenticate(&handle, &config.username)
                    .await
                    .map_err(|e| format!("Agent authentication failed: {}", e))?;
                true
            }
            AuthMethod::KeyboardInteractive => {
                // KBI reconnection via proxy chain is not supported
                return Err(
                    "KeyboardInteractive sessions cannot be auto-reconnected via proxy chain"
                        .to_string(),
                );
            }
        };

        if !authenticated {
            return Err(format!("Authentication to {} rejected", config.host));
        }

        info!(
            "Tunneled connection {} reconnected successfully via {}",
            connection_id, parent_connection_id
        );

        // 7. 创建 SshSession 并启动 Handle Owner Task
        let session = super::session::SshSession::new(handle, config.cols, config.rows);
        let handle_controller = session.start(connection_id.to_string());

        Ok(handle_controller)
    }

    /// 广播连接状态变更事件
    /// 
    /// # 状态守卫
    /// 只有当状态真正变化时才发送事件，避免重复发送相同状态导致前端性能问题
    /// 
    /// # AppHandle 生命周期
    /// 如果 AppHandle 未就绪，事件会被缓存，待 AppHandle 设置后立即发送
    async fn emit_connection_status_changed(&self, connection_id: &str, status: &str) {
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
            // AppHandle 未就绪，缓存事件
            warn!("AppHandle not ready, caching event: {} -> {}", connection_id, status);
            let mut pending = self.pending_events.lock().await;
            pending.push((connection_id.to_string(), status.to_string()));
            debug!("Event cached, total pending: {}", pending.len());
        }
    }

    /// 替换连接的 HandleController（用于重连后更新）
    ///
    /// # 锁安全
    /// 此方法先收集所有需要的数据到局部变量，然后再释放 DashMap 引用，
    /// 避免在持有 DashMap 引用时获取多个 RwLock。
    async fn replace_handle_controller(&self, connection_id: &str, new_controller: HandleController) {
        // 先收集所有需要的数据到局部变量
        let old_data = if let Some(entry) = self.connections.get(connection_id) {
            let old_entry = entry.value();
            
            // 按锁顺序依次读取，每个 await 后锁自动释放
            let keep_alive = *old_entry.keep_alive.read().await;
            let terminal_ids = old_entry.terminal_ids.read().await.clone();
            let sftp_session_id = old_entry.sftp_session_id.read().await.clone();
            let forward_ids = old_entry.forward_ids.read().await.clone();
            
            Some((
                old_entry.id.clone(),
                old_entry.config.clone(),
                old_entry.ref_count.load(Ordering::SeqCst),
                old_entry.created_at,
                old_entry.current_attempt_id.load(Ordering::SeqCst),
                old_entry.parent_connection_id.clone(),
                keep_alive,
                terminal_ids,
                sftp_session_id,
                forward_ids,
            ))
        } else {
            None
        };
        
        // 在 DashMap 引用释放后，使用收集的数据创建新条目
        if let Some((id, config, ref_count, created_at, attempt_id, parent_id, keep_alive, terminal_ids, sftp_session_id, forward_ids)) = old_data {
            let new_entry = Arc::new(ConnectionEntry {
                id,
                config,
                handle_controller: new_controller,
                state: RwLock::new(ConnectionState::Active),
                ref_count: AtomicU32::new(ref_count),
                last_active: AtomicU64::new(Utc::now().timestamp() as u64),
                keep_alive: RwLock::new(keep_alive),
                created_at,
                idle_timer: Mutex::new(None),
                terminal_ids: RwLock::new(terminal_ids),
                sftp_session_id: RwLock::new(sftp_session_id),
                forward_ids: RwLock::new(forward_ids),
                heartbeat_task: Mutex::new(None),
                heartbeat_failures: AtomicU32::new(0),
                reconnect_task: Mutex::new(None),
                is_reconnecting: AtomicBool::new(false),
                reconnect_attempts: AtomicU32::new(0),
                current_attempt_id: AtomicU64::new(attempt_id),
                last_emitted_status: RwLock::new(None),
                parent_connection_id: parent_id,
            });
            
            // 替换条目
            self.connections.insert(connection_id.to_string(), new_entry);
            
            info!("Connection {} HandleController replaced after reconnect", connection_id);
        }
    }

    /// 广播连接重连成功事件（通知前端恢复 Shell 和 Forward）
    async fn emit_connection_reconnected(
        &self,
        connection_id: &str,
        terminal_ids: Vec<String>,
        forward_ids: Vec<String>,
    ) {
        let app_handle = self.app_handle.read().await;
        if let Some(handle) = app_handle.as_ref() {
            use tauri::Emitter;
            
            #[derive(Clone, serde::Serialize)]
            struct ConnectionReconnectedEvent {
                connection_id: String,
                terminal_ids: Vec<String>,
                forward_ids: Vec<String>,
            }

            let event = ConnectionReconnectedEvent {
                connection_id: connection_id.to_string(),
                terminal_ids,
                forward_ids,
            };

            if let Err(e) = handle.emit("connection_reconnected", event) {
                error!("Failed to emit connection_reconnected: {}", e);
            } else {
                info!("Emitted connection_reconnected for {}", connection_id);
            }
        }
    }

    /// 广播重连进度事件（让前端显示重连进度）
    async fn emit_reconnect_progress(
        &self,
        connection_id: &str,
        attempt: u32,
        max_attempts: Option<u32>,
        next_retry_ms: u64,
    ) {
        let app_handle = self.app_handle.read().await;
        if let Some(handle) = app_handle.as_ref() {
            use tauri::Emitter;
            
            #[derive(Clone, serde::Serialize)]
            struct ConnectionReconnectProgressEvent {
                connection_id: String,
                attempt: u32,
                max_attempts: Option<u32>,
                next_retry_ms: u64,
                timestamp: u64,
            }

            let event = ConnectionReconnectProgressEvent {
                connection_id: connection_id.to_string(),
                attempt,
                max_attempts,
                next_retry_ms,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };

            if let Err(e) = handle.emit("connection_reconnect_progress", event) {
                error!("Failed to emit connection_reconnect_progress: {}", e);
            } else {
                debug!("Emitted reconnect progress for {}: attempt {}", connection_id, attempt);
            }
        }
    }

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

    /// 父连接恢复后触发子连接级联重连
    /// 
    /// # Jitter 抖动
    /// 使用 50-200ms 随机延迟防止重连风暴（Reconnect Storm）
    async fn cascade_reconnect_children(self: &Arc<Self>, parent_connection_id: &str) {
        // 收集处于 LinkDown 状态的直接子连接
        let children: Vec<String> = self.connections.iter()
            .filter(|e| e.value().parent_connection_id.as_deref() == Some(parent_connection_id))
            .filter(|e| {
                // 只处理 LinkDown 状态的子连接
                // 使用 try_read 避免死锁，如果读取失败则跳过
                if let Ok(guard) = e.value().state.try_read() {
                    *guard == ConnectionState::LinkDown
                } else {
                    false
                }
            })
            .map(|e| e.key().clone())
            .collect();
        
        if children.is_empty() {
            return;
        }
        
        info!("Starting cascade reconnect for {} children of {}", children.len(), parent_connection_id);
        
        for child_id in children {
            let registry = Arc::clone(self);
            let child_id_clone = child_id.clone();
            
            tokio::spawn(async move {
                // 🔴 关键：随机抖动防止重连风暴
                let jitter = rand::random::<u64>() % 150 + 50; // 50-200ms
                tokio::time::sleep(Duration::from_millis(jitter)).await;
                
                info!("Cascade reconnecting child {} (jitter: {}ms)", child_id_clone, jitter);
                
                // 尝试级联重连
                if let Err(e) = registry.try_cascade_reconnect_single(&child_id_clone).await {
                    warn!("Cascade reconnect failed for {}: {}", child_id_clone, e);
                }
            });
        }
    }

    /// 单个子连接的级联重连
    async fn try_cascade_reconnect_single(&self, connection_id: &str) -> Result<(), String> {
        let entry = self.connections.get(connection_id)
            .ok_or_else(|| format!("Connection {} not found", connection_id))?;
        
        let conn = entry.value().clone();
        let config = conn.config.clone();
        let parent_id = conn.parent_connection_id.clone()
            .ok_or_else(|| "Not a tunneled connection".to_string())?;
        drop(entry);
        
        // 检查父连接状态
        let parent_entry = self.connections.get(&parent_id)
            .ok_or_else(|| format!("Parent connection {} not found", parent_id))?;
        let parent_state = parent_entry.value().state().await;
        if parent_state != ConnectionState::Active {
            return Err(format!("Parent {} is not active: {:?}", parent_id, parent_state));
        }
        drop(parent_entry);
        
        // 更新状态为重连中
        conn.set_state(ConnectionState::Reconnecting).await;
        self.emit_connection_status_changed(connection_id, "reconnecting").await;
        
        // 通过父连接重建隧道
        match self.try_reconnect(connection_id, &config).await {
            Ok(new_controller) => {
                info!("Cascade reconnect successful for {}", connection_id);
                
                // 获取关联资源
                let terminal_ids = conn.terminal_ids().await;
                let forward_ids = conn.forward_ids().await;
                
                // 重置状态
                conn.reset_heartbeat_failures();
                conn.reset_reconnect_state();
                conn.set_state(ConnectionState::Active).await;
                
                // 替换 HandleController
                self.replace_handle_controller(connection_id, new_controller).await;
                
                // 发送事件
                self.emit_connection_reconnected(connection_id, terminal_ids, forward_ids).await;
                self.emit_connection_status_changed(connection_id, "connected").await;
                
                // 注意：心跳由 on_reconnect_success 统一启动
                // 子连接的级联重连由 cascade_reconnect_children 递归处理
                
                Ok(())
            }
            Err(e) => {
                warn!("Cascade reconnect failed for {}: {}", connection_id, e);
                // 保持 LinkDown 状态，等待下次机会
                conn.set_state(ConnectionState::LinkDown).await;
                Err(e)
            }
        }
    }

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
            forward_ids: RwLock::new(Vec::new()),
            heartbeat_task: Mutex::new(None),
            heartbeat_failures: AtomicU32::new(0),
            reconnect_task: Mutex::new(None),
            is_reconnecting: AtomicBool::new(false),
            reconnect_attempts: AtomicU32::new(0),
            current_attempt_id: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            parent_connection_id: None,
        };

        assert_eq!(entry.ref_count(), 0);
        assert_eq!(entry.add_ref(), 1);
        assert_eq!(entry.add_ref(), 2);
        assert_eq!(entry.release(), 1);
        assert_eq!(entry.release(), 0);
    }
}
