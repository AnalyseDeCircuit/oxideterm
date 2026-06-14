// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use oxideterm_connection_monitor::{
    ConnectionMonitorConsumerKind, ConnectionPoolEntryState, ConnectionPoolEntrySummary,
    ConnectionPoolMonitorStats, PoolConnectionMonitorSnapshot, PoolConnectionSummarySnapshot,
};
use oxideterm_sftp::{SftpError, SftpSession};
use oxideterm_topology::{
    ConnectionTopologyConsumerSummary, ConnectionTopologyEdge, ConnectionTopologyNode,
    ConnectionTopologySnapshot, ConnectionTopologyStatus,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle as TokioHandle;
use tokio::sync::{Mutex, Notify};
use tokio::time::sleep;
use uuid::Uuid;

use crate::SshConfig;
use crate::router::NodeEventEmitter;

pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
pub const HEARTBEAT_FAIL_THRESHOLD: u8 = 2;
pub const WS_BRIDGE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
pub const WS_BRIDGE_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(300);
const REMOTE_ENV_TOTAL_TIMEOUT: Duration = Duration::from_secs(8);
const REMOTE_ENV_PHASE_A_TIMEOUT: Duration = Duration::from_secs(3);
const REMOTE_ENV_PHASE_B_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_ENV_MAX_OUTPUT_SIZE: usize = 8192;
const REMOTE_ENV_PHASE_A_CMD: &str = "echo '===DETECT==='; if [ -n \"$PSModulePath\" ]; then echo 'PLATFORM=windows'; else echo \"PLATFORM=$(uname -s 2>/dev/null || echo unknown)\"; fi; echo '===END==='";
const REMOTE_ENV_PHASE_B_UNIX_CMD: &str = "echo '===ENV==='; uname -s 2>/dev/null; echo '===ARCH==='; uname -m 2>/dev/null; echo '===KERNEL==='; uname -r 2>/dev/null; echo '===SHELL==='; echo $SHELL 2>/dev/null; echo '===DISTRO==='; cat /etc/os-release 2>/dev/null | grep -E '^(PRETTY_NAME|ID)=' | head -2; echo '===END==='";
const REMOTE_ENV_PHASE_B_WINDOWS_CMD: &str = "echo '===ENV==='; [System.Environment]::OSVersion.VersionString; echo '===ARCH==='; $env:PROCESSOR_ARCHITECTURE; echo '===SHELL==='; \"PowerShell $($PSVersionTable.PSVersion)\"; echo '===END==='";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Active,
    Idle,
    LinkDown,
    Reconnecting,
    Disconnecting,
    Disconnected,
    Error(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionConsumer {
    Terminal(String),
    Sftp(String),
    PortForward(String),
    Ide(String),
    NodeRouter(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub connection_id: String,
    pub key: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub parent_connection_id: Option<String>,
    pub state: ConnectionState,
    pub ref_count: u64,
    pub keep_alive: bool,
    pub consumers: Vec<ConnectionConsumer>,
    pub created_at: SystemTime,
    pub last_active_at: SystemTime,
    pub idle_timeout_secs: Option<u64>,
    pub remote_env: Option<RemoteEnvInfo>,
}

/// Remote environment detected after SSH connection establishment.
///
/// This mirrors Tauri's `RemoteEnvInfo` payload so profiler and host tools can
/// choose platform-specific commands from registry state instead of probing in
/// every caller.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEnvInfo {
    pub os_type: String,
    pub os_version: Option<String>,
    pub kernel: Option<String>,
    pub arch: Option<String>,
    pub shell: Option<String>,
    pub detected_at: i64,
}

impl RemoteEnvInfo {
    pub fn unknown() -> Self {
        Self {
            os_type: "Unknown".to_string(),
            os_version: None,
            kernel: None,
            arch: None,
            shell: None,
            detected_at: remote_env_detected_at(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeConnectionStatus {
    Alive,
    Dead,
    NotFound,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeepaliveProbeResult {
    Ok,
    Timeout,
    IoError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTransportStatus {
    Open,
    Closed,
    Missing,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpSessionState {
    pub ready: bool,
    pub cwd: Option<String>,
}

#[derive(Clone)]
pub struct AcquiredSftpMeta {
    pub session: Arc<Mutex<SftpSession>>,
    pub was_new: bool,
    pub cwd: Option<String>,
}

enum SharedSftpState {
    Empty,
    Initializing {
        notify: Arc<Notify>,
        generation: u64,
    },
    Ready(Arc<Mutex<SftpSession>>),
}

impl fmt::Debug for SharedSftpState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Empty"),
            Self::Initializing { generation, .. } => formatter
                .debug_struct("Initializing")
                .field("generation", generation)
                .finish(),
            Self::Ready(_) => formatter.write_str("Ready(<sftp-session>)"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionPoolConfig {
    pub idle_timeout: Option<Duration>,
    pub max_connections: usize,
    pub protect_on_exit: bool,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            idle_timeout: Some(DEFAULT_IDLE_TIMEOUT),
            max_connections: 128,
            protect_on_exit: true,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionPoolStats {
    pub total: usize,
    pub active: usize,
    pub idle: usize,
    pub link_down: usize,
    pub reconnecting: usize,
    pub disconnected: usize,
    pub errored: usize,
}

#[derive(Debug)]
struct ConnectionEntry {
    connection_id: String,
    key: String,
    config: SshConfig,
    parent_connection_id: RwLock<Option<String>>,
    state: RwLock<ConnectionState>,
    ref_count: AtomicU64,
    keep_alive: AtomicBool,
    consumers: RwLock<Vec<ConnectionConsumer>>,
    physical: RwLock<Option<Arc<dyn Any + Send + Sync>>>,
    sftp: Mutex<SharedSftpState>,
    sftp_generation: AtomicU64,
    sftp_state: RwLock<SftpSessionState>,
    remote_env: std::sync::OnceLock<RemoteEnvInfo>,
    remote_env_detection_started: AtomicBool,
    heartbeat_failures: AtomicU64,
    idle_generation: AtomicU64,
    last_emitted_status: RwLock<Option<String>>,
    created_at: SystemTime,
    last_active_at: RwLock<SystemTime>,
    idle_timeout: Option<Duration>,
}

impl ConnectionEntry {
    fn new(config: SshConfig, pool_config: ConnectionPoolConfig) -> Self {
        let key = config.connection_key();
        Self {
            connection_id: Uuid::new_v4().to_string(),
            key,
            config,
            parent_connection_id: RwLock::new(None),
            state: RwLock::new(ConnectionState::Connecting),
            ref_count: AtomicU64::new(0),
            keep_alive: AtomicBool::new(false),
            consumers: RwLock::new(Vec::new()),
            physical: RwLock::new(None),
            sftp: Mutex::new(SharedSftpState::Empty),
            sftp_generation: AtomicU64::new(0),
            sftp_state: RwLock::new(SftpSessionState::default()),
            remote_env: std::sync::OnceLock::new(),
            remote_env_detection_started: AtomicBool::new(false),
            heartbeat_failures: AtomicU64::new(0),
            idle_generation: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            created_at: SystemTime::now(),
            last_active_at: RwLock::new(SystemTime::now()),
            idle_timeout: pool_config.idle_timeout,
        }
    }

    fn info(&self) -> ConnectionInfo {
        ConnectionInfo {
            connection_id: self.connection_id.clone(),
            key: self.key.clone(),
            host: self.config.host.clone(),
            port: self.config.port,
            username: self.config.username.clone(),
            parent_connection_id: self.parent_connection_id.read().clone(),
            state: self.state.read().clone(),
            ref_count: self.ref_count.load(Ordering::SeqCst),
            keep_alive: self.is_keep_alive(),
            consumers: self.consumers.read().clone(),
            created_at: self.created_at,
            last_active_at: *self.last_active_at.read(),
            idle_timeout_secs: self.idle_timeout.map(|duration| duration.as_secs()),
            remote_env: self.remote_env(),
        }
    }

    fn monitor_snapshot(&self) -> PoolConnectionMonitorSnapshot {
        let state = self.state.read().clone();
        let consumers = self
            .consumers
            .read()
            .iter()
            .map(ConnectionMonitorConsumerKind::from)
            .collect();

        PoolConnectionMonitorSnapshot {
            is_active: matches!(state, ConnectionState::Active),
            is_idle: matches!(state, ConnectionState::Idle),
            is_reconnecting: matches!(state, ConnectionState::Reconnecting),
            is_link_down: matches!(state, ConnectionState::LinkDown),
            ref_count: self.ref_count.load(Ordering::SeqCst),
            // Tauri counts one SFTP session per connection when the backend
            // entry owns a ready session, not one count per SFTP UI consumer.
            has_sftp_session: self.sftp_state.read().ready,
            consumers,
        }
    }

    fn summary_snapshot(&self) -> PoolConnectionSummarySnapshot {
        let consumers = self.consumers.read().clone();
        let counts = topology_consumer_summary(&consumers);
        PoolConnectionSummarySnapshot {
            id: self.connection_id.clone(),
            host: self.config.host.clone(),
            port: self.config.port,
            username: self.config.username.clone(),
            state: ConnectionPoolEntryState::from(&*self.state.read()),
            ref_count: self.ref_count.load(Ordering::SeqCst),
            keep_alive: self.is_keep_alive(),
            created_at: self.created_at,
            last_active_at: *self.last_active_at.read(),
            terminal_count: counts.terminals,
            has_sftp_session: self.sftp_state.read().ready,
            forward_count: counts.port_forwards,
            parent_connection_id: self.parent_connection_id.read().clone(),
        }
    }

    fn is_keep_alive(&self) -> bool {
        self.keep_alive.load(Ordering::Acquire)
    }

    fn set_keep_alive(&self, keep_alive: bool) {
        self.keep_alive.store(keep_alive, Ordering::Release);
    }

    fn touch(&self) {
        *self.last_active_at.write() = SystemTime::now();
    }

    fn remote_env(&self) -> Option<RemoteEnvInfo> {
        self.remote_env.get().cloned()
    }

    fn set_remote_env(&self, env: RemoteEnvInfo) -> bool {
        self.remote_env.set(env).is_ok()
    }

    fn try_begin_remote_env_detection(&self) -> bool {
        self.remote_env.get().is_none()
            && !self
                .remote_env_detection_started
                .swap(true, Ordering::AcqRel)
    }

    fn reset_heartbeat_failures(&self) {
        self.heartbeat_failures.store(0, Ordering::Relaxed);
    }

    fn increment_heartbeat_failures(&self) -> u64 {
        self.heartbeat_failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn cancel_idle_timer(&self) {
        self.idle_generation.fetch_add(1, Ordering::AcqRel);
    }

    fn idle_generation(&self) -> u64 {
        self.idle_generation.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug)]
pub struct SshConnectionHandle {
    entry: Arc<ConnectionEntry>,
}

impl SshConnectionHandle {
    pub fn connection_id(&self) -> &str {
        &self.entry.connection_id
    }

    pub fn key(&self) -> &str {
        &self.entry.key
    }

    pub fn info(&self) -> ConnectionInfo {
        self.entry.info()
    }

    pub fn remote_env(&self) -> Option<RemoteEnvInfo> {
        self.entry.remote_env()
    }

    pub fn set_remote_env(&self, env: RemoteEnvInfo) -> bool {
        self.entry.set_remote_env(env)
    }

    pub fn state(&self) -> ConnectionState {
        self.entry.state.read().clone()
    }

    /// Reports whether the registry entry contains any physical transport slot.
    ///
    /// This intentionally does not imply the transport is alive. Tauri keeps
    /// node liveness in the connection registry, so SFTP/forwarding callers
    /// must combine this with `transport_status()` before borrowing a handle.
    pub fn has_physical(&self) -> bool {
        self.entry.physical.read().is_some()
    }

    pub fn physical<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        self.entry
            .physical
            .read()
            .as_ref()
            .cloned()
            .and_then(|physical| Arc::downcast::<T>(physical).ok())
    }

    pub fn set_physical<T>(&self, physical: Arc<T>)
    where
        T: Any + Send + Sync + 'static,
    {
        *self.entry.physical.write() = Some(physical);
        self.entry.touch();
    }

    pub async fn clear_physical(&self) {
        *self.entry.physical.write() = None;
        self.entry.sftp_generation.fetch_add(1, Ordering::AcqRel);
        let mut guard = self.entry.sftp.lock().await;
        match std::mem::replace(&mut *guard, SharedSftpState::Empty) {
            SharedSftpState::Initializing { notify, .. } => notify.notify_waiters(),
            SharedSftpState::Empty | SharedSftpState::Ready(_) => {}
        }
        *self.entry.sftp_state.write() = SftpSessionState::default();
        self.entry.touch();
    }

    pub async fn acquire_sftp(&self) -> Result<Arc<Mutex<SftpSession>>, SftpError> {
        Ok(self.acquire_sftp_with_meta().await?.session)
    }

    pub async fn acquire_sftp_with_meta(&self) -> Result<AcquiredSftpMeta, SftpError> {
        loop {
            let initializing = {
                let mut guard = self.entry.sftp.lock().await;
                match &*guard {
                    SharedSftpState::Ready(session) => {
                        let session = Arc::clone(session);
                        drop(guard);
                        let cwd = {
                            let sftp = session.lock().await;
                            Some(sftp.cwd().to_string())
                        };
                        return Ok(AcquiredSftpMeta {
                            session,
                            was_new: false,
                            cwd,
                        });
                    }
                    SharedSftpState::Initializing { notify, .. } => Some(notify.clone()),
                    SharedSftpState::Empty => {
                        let generation = self.entry.sftp_generation.load(Ordering::Acquire);
                        let notify = Arc::new(Notify::new());
                        *guard = SharedSftpState::Initializing {
                            notify: notify.clone(),
                            generation,
                        };
                        None
                    }
                }
            };

            if let Some(notify) = initializing {
                notify.notified().await;
                continue;
            }

            let created = SftpSession::new(self.clone(), self.connection_id().to_string()).await;
            let mut guard = self.entry.sftp.lock().await;
            match created {
                Ok(sftp) => {
                    let cwd = Some(sftp.cwd().to_string());
                    let session = Arc::new(Mutex::new(sftp));
                    match &*guard {
                        SharedSftpState::Ready(existing) => {
                            let existing = Arc::clone(existing);
                            drop(guard);
                            let cwd = {
                                let sftp = existing.lock().await;
                                Some(sftp.cwd().to_string())
                            };
                            return Ok(AcquiredSftpMeta {
                                session: existing,
                                was_new: false,
                                cwd,
                            });
                        }
                        SharedSftpState::Initializing { notify, generation }
                            if *generation
                                == self.entry.sftp_generation.load(Ordering::Acquire) =>
                        {
                            let notify = notify.clone();
                            *guard = SharedSftpState::Ready(Arc::clone(&session));
                            notify.notify_waiters();
                            self.entry.touch();
                            return Ok(AcquiredSftpMeta {
                                session,
                                was_new: true,
                                cwd,
                            });
                        }
                        SharedSftpState::Initializing { notify, .. } => {
                            notify.clone().notify_waiters();
                            *guard = SharedSftpState::Empty;
                            continue;
                        }
                        SharedSftpState::Empty => continue,
                    }
                }
                Err(error) => {
                    if let SharedSftpState::Initializing { notify, .. } = &*guard {
                        let notify = notify.clone();
                        *guard = SharedSftpState::Empty;
                        notify.notify_waiters();
                    }
                    return Err(error);
                }
            }
        }
    }

    pub async fn acquire_transfer_sftp(&self) -> Result<SftpSession, SftpError> {
        SftpSession::new(self.clone(), self.connection_id().to_string()).await
    }

    pub async fn clear_sftp(&self) {
        let mut guard = self.entry.sftp.lock().await;
        self.entry.sftp_generation.fetch_add(1, Ordering::AcqRel);
        if let SharedSftpState::Initializing { notify, .. } =
            std::mem::replace(&mut *guard, SharedSftpState::Empty)
        {
            notify.notify_waiters();
        }
        *self.entry.sftp_state.write() = SftpSessionState::default();
        self.entry.touch();
    }

    pub async fn invalidate_sftp(&self) -> bool {
        let mut guard = self.entry.sftp.lock().await;
        self.entry.sftp_generation.fetch_add(1, Ordering::AcqRel);
        let had_sftp = match std::mem::replace(&mut *guard, SharedSftpState::Empty) {
            SharedSftpState::Empty => false,
            SharedSftpState::Initializing { notify, .. } => {
                notify.notify_waiters();
                true
            }
            SharedSftpState::Ready(_) => true,
        };
        if had_sftp {
            *self.entry.sftp_state.write() = SftpSessionState::default();
            self.entry.touch();
        }
        had_sftp
    }
}

#[derive(Clone, Debug)]
pub struct SshConnectionRegistry {
    config: Arc<RwLock<ConnectionPoolConfig>>,
    by_key: Arc<DashMap<String, Arc<ConnectionEntry>>>,
    by_id: Arc<DashMap<String, String>>,
    idle_task_runtime: Arc<RwLock<Option<TokioHandle>>>,
    node_event_emitter: Arc<RwLock<Option<NodeEventEmitter>>>,
}

impl SshConnectionRegistry {
    pub fn new(config: ConnectionPoolConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            by_key: Arc::new(DashMap::new()),
            by_id: Arc::new(DashMap::new()),
            idle_task_runtime: Arc::new(RwLock::new(None)),
            node_event_emitter: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set_task_runtime(&self, runtime: TokioHandle) {
        *self.idle_task_runtime.write() = Some(runtime);
    }

    pub fn set_idle_timeout(&self, idle_timeout: Option<Duration>) {
        self.config.write().idle_timeout = idle_timeout;
        for entry in self.by_key.iter() {
            entry.cancel_idle_timer();
            if matches!(*entry.state.read(), ConnectionState::Idle)
                && entry.ref_count.load(Ordering::SeqCst) == 0
                && !entry.is_keep_alive()
            {
                self.start_idle_timer_for_entry(entry.clone());
            }
        }
    }

    pub fn set_node_event_emitter(&self, emitter: NodeEventEmitter) {
        *self.node_event_emitter.write() = Some(emitter);
    }

    pub fn acquire(&self, config: SshConfig, consumer: ConnectionConsumer) -> SshConnectionHandle {
        let key = config.connection_key();
        let entry = self
            .by_key
            .entry(key.clone())
            .or_insert_with(|| {
                let entry = Arc::new(ConnectionEntry::new(config, *self.config.read()));
                self.by_id.insert(entry.connection_id.clone(), key);
                entry
            })
            .clone();

        entry.ref_count.fetch_add(1, Ordering::SeqCst);
        entry.cancel_idle_timer();
        entry.touch();
        {
            let mut consumers = entry.consumers.write();
            if !consumers.contains(&consumer) {
                consumers.push(consumer);
            }
        }
        // `acquire` only records a logical consumer. The physical SSH transport
        // is established by connect_tree_node / terminal connect paths and
        // marks the state Active after authentication succeeds. Marking Active
        // here made SFTP/forwarding believe a closed terminal-owned transport
        // was reusable, which diverges from Tauri's node-owned pool semantics.
        SshConnectionHandle { entry }
    }

    pub fn release(&self, connection_id: &str, consumer: &ConnectionConsumer) {
        let Some(key) = self.by_id.get(connection_id).map(|key| key.value().clone()) else {
            return;
        };
        let Some(entry) = self.by_key.get(&key).map(|entry| entry.clone()) else {
            return;
        };

        let removed = {
            let mut consumers = entry.consumers.write();
            let before = consumers.len();
            consumers.retain(|existing| existing != consumer);
            consumers.len() != before
        };
        if removed {
            entry
                .ref_count
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                    Some(count.saturating_sub(1))
                })
                .ok();
        }
        entry.touch();
        if entry.ref_count.load(Ordering::SeqCst) == 0 {
            if entry.is_keep_alive() {
                entry.cancel_idle_timer();
                *entry.state.write() = ConnectionState::Idle;
            } else {
                self.start_idle_timer_for_entry(entry);
            }
        }
    }

    pub fn mark_state(
        &self,
        connection_id: &str,
        state: ConnectionState,
    ) -> Option<ConnectionInfo> {
        self.mark_state_inner(connection_id, state, true, "connection state changed")
    }

    pub fn mark_state_without_event(
        &self,
        connection_id: &str,
        state: ConnectionState,
    ) -> Option<ConnectionInfo> {
        self.mark_state_inner(connection_id, state, false, "")
    }

    fn mark_state_inner(
        &self,
        connection_id: &str,
        state: ConnectionState,
        emit_node_event: bool,
        reason: &str,
    ) -> Option<ConnectionInfo> {
        let key = self
            .by_id
            .get(connection_id)
            .map(|key| key.value().clone())?;
        let entry = self.by_key.get(&key)?.clone();
        let became_active = matches!(state, ConnectionState::Active);
        *entry.state.write() = state;
        entry.touch();
        let info = entry.info();
        if emit_node_event && let Some(emitter) = self.node_event_emitter.read().clone() {
            // Match Tauri's registry-to-node event flow: low-level connection
            // state changes are translated through the shared NodeEventEmitter
            // whenever the connection has been registered to a node.
            let _ = emitter.emit_state_from_connection(&info.connection_id, &info.state, reason);
        }
        if became_active {
            self.maybe_spawn_remote_env_detection(entry);
        }
        Some(info)
    }

    fn maybe_spawn_remote_env_detection(&self, entry: Arc<ConnectionEntry>) {
        let runtime = self
            .idle_task_runtime
            .read()
            .clone()
            .or_else(|| TokioHandle::try_current().ok());
        let Some(runtime) = runtime else {
            return;
        };
        if !entry.try_begin_remote_env_detection() {
            return;
        }

        let handle = SshConnectionHandle { entry };
        let task = async move {
            let env = detect_remote_env_for_handle(&handle).await;
            let _ = handle.set_remote_env(env);
        };

        // Tauri stores remote env on the connection entry after connect. Native
        // uses the registry task runtime when available so detection is owned by
        // the same long-lived SSH runtime as keepalive and idle tasks.
        runtime.spawn(task);
    }

    fn emit_connection_status_changed(
        &self,
        connection_id: &str,
        status: &str,
        affected_children: Vec<String>,
    ) -> bool {
        let Some(handle) = self.get(connection_id) else {
            return false;
        };
        {
            let mut last_status = handle.entry.last_emitted_status.write();
            if last_status.as_deref() == Some(status) {
                return false;
            }
            *last_status = Some(status.to_string());
        }
        if let Some(emitter) = self.node_event_emitter.read().clone() {
            emitter.emit_connection_status_changed(
                connection_id.to_string(),
                status.to_string(),
                affected_children,
            );
        }
        true
    }

    pub fn set_parent_connection_id(
        &self,
        connection_id: &str,
        parent_connection_id: Option<String>,
    ) -> Option<ConnectionInfo> {
        let key = self
            .by_id
            .get(connection_id)
            .map(|key| key.value().clone())?;
        let entry = self.by_key.get(&key)?.clone();
        *entry.parent_connection_id.write() = parent_connection_id;
        entry.touch();
        Some(entry.info())
    }

    pub fn descendant_connection_infos(&self, root_connection_id: &str) -> Vec<ConnectionInfo> {
        if self.get(root_connection_id).is_none() {
            return Vec::new();
        }
        let mut descendants = Vec::new();
        let mut stack = vec![root_connection_id.to_string()];
        while let Some(parent_id) = stack.pop() {
            let children = self
                .by_key
                .iter()
                .filter(|entry| entry.parent_connection_id.read().as_deref() == Some(&parent_id))
                .map(|entry| entry.connection_id.clone())
                .collect::<Vec<_>>();
            for child_id in children {
                if let Some(handle) = self.get(&child_id) {
                    descendants.push(handle.info());
                }
                stack.push(child_id);
            }
        }
        descendants
    }

    pub fn retire_connection(&self, connection_id: &str) -> Option<ConnectionInfo> {
        let key = self
            .by_id
            .get(connection_id)
            .map(|key| key.value().clone())?;
        let entry = self.by_key.get(&key).map(|entry| entry.clone())?;
        entry.cancel_idle_timer();
        let info = entry.info();
        if entry.connection_id == connection_id {
            self.by_key.remove(&key);
        }
        self.by_id.remove(connection_id);
        Some(info)
    }

    pub fn mark_link_down_cascade(&self, root_connection_id: &str) -> Vec<ConnectionInfo> {
        if self.get(root_connection_id).is_none() {
            return Vec::new();
        }
        let affected_children = self
            .descendant_connection_infos(root_connection_id)
            .into_iter()
            .map(|info| info.connection_id)
            .collect::<Vec<_>>();
        let mut connection_ids = vec![root_connection_id.to_string()];
        connection_ids.extend(affected_children.iter().cloned());

        let mut changed = Vec::new();
        for connection_id in connection_ids {
            let Some(handle) = self.get(&connection_id) else {
                continue;
            };
            if !matches!(
                handle.state(),
                ConnectionState::Active
                    | ConnectionState::Idle
                    | ConnectionState::Connecting
                    | ConnectionState::Reconnecting
                    | ConnectionState::LinkDown
            ) {
                continue;
            }
            if let Some(info) =
                self.mark_state_inner(&connection_id, ConnectionState::LinkDown, false, "")
            {
                changed.push(info);
            }
        }
        if !changed.is_empty() {
            // Tauri emits one `connection_status_changed` event for the root
            // connection and carries descendant connection ids in
            // `affected_children`; child UI state is derived from that payload.
            let emitted_status = self.emit_connection_status_changed(
                root_connection_id,
                "link_down",
                affected_children,
            );
            if emitted_status && let Some(emitter) = self.node_event_emitter.read().clone() {
                let _ = emitter.emit_state_from_connection(
                    root_connection_id,
                    &ConnectionState::LinkDown,
                    "link down",
                );
            }
        }
        changed
    }

    pub async fn mark_transport_lost_cascade(
        &self,
        root_connection_id: &str,
        reason: impl AsRef<str>,
    ) -> Vec<ConnectionInfo> {
        let reason = reason.as_ref();
        let changed = self.mark_link_down_cascade(root_connection_id);
        for info in &changed {
            if let Some(handle) = self.get(&info.connection_id) {
                // A transport-level failure means the pooled handle can no
                // longer be trusted by SFTP, forwarding, or terminal recovery.
                // Clear it before reconnect code decides whether it can reuse
                // an existing physical connection.
                handle.clear_physical().await;
            }
        }
        if let Some(emitter) = self.node_event_emitter.read().clone() {
            let _ = emitter.emit_state_from_connection(
                root_connection_id,
                &ConnectionState::LinkDown,
                reason,
            );
        }
        changed
    }

    pub async fn probe_active_connections(&self, timeout: Duration) -> Vec<ConnectionInfo> {
        let connection_ids = self
            .list()
            .into_iter()
            .filter(|info| matches!(info.state, ConnectionState::Active | ConnectionState::Idle))
            .map(|info| info.connection_id)
            .collect::<Vec<_>>();
        let mut changed = Vec::new();
        for connection_id in connection_ids {
            if matches!(
                self.probe_active_connection(&connection_id, timeout).await,
                ProbeConnectionStatus::Dead
            ) {
                changed.extend(
                    self.mark_transport_lost_cascade(&connection_id, "keepalive probe failed")
                        .await,
                );
            }
        }
        changed
    }

    async fn probe_active_connection(
        &self,
        connection_id: &str,
        timeout: Duration,
    ) -> ProbeConnectionStatus {
        let Some(handle) = self.get(connection_id) else {
            return ProbeConnectionStatus::NotFound;
        };
        if !matches!(
            handle.state(),
            ConnectionState::Active | ConnectionState::Idle
        ) {
            return ProbeConnectionStatus::NotApplicable;
        }

        match handle.probe_alive(timeout).await {
            KeepaliveProbeResult::Ok => {
                handle.entry.reset_heartbeat_failures();
                handle.entry.touch();
                ProbeConnectionStatus::Alive
            }
            KeepaliveProbeResult::Timeout => {
                let failures = handle.entry.increment_heartbeat_failures();
                if failures < HEARTBEAT_FAIL_THRESHOLD as u64 {
                    return ProbeConnectionStatus::Alive;
                }
                ProbeConnectionStatus::Dead
            }
            KeepaliveProbeResult::IoError => {
                // Tauri's app-level heartbeat confirms an IO error with a
                // 1.5s quick probe before emitting link_down.
                if matches!(
                    handle.state(),
                    ConnectionState::Disconnecting | ConnectionState::Disconnected
                ) {
                    return ProbeConnectionStatus::NotApplicable;
                }
                sleep(Duration::from_millis(1500)).await;
                if matches!(
                    handle.state(),
                    ConnectionState::Disconnecting | ConnectionState::Disconnected
                ) {
                    return ProbeConnectionStatus::NotApplicable;
                }
                match handle.probe_alive(timeout).await {
                    KeepaliveProbeResult::Ok => {
                        handle.entry.reset_heartbeat_failures();
                        handle.entry.touch();
                        ProbeConnectionStatus::Alive
                    }
                    KeepaliveProbeResult::Timeout | KeepaliveProbeResult::IoError => {
                        ProbeConnectionStatus::Dead
                    }
                }
            }
        }
    }

    pub async fn probe_single_connection(
        &self,
        connection_id: &str,
        timeout: Duration,
    ) -> ProbeConnectionStatus {
        let Some(handle) = self.get(connection_id) else {
            return ProbeConnectionStatus::NotFound;
        };
        let state = handle.state();
        match state {
            ConnectionState::Active | ConnectionState::Idle | ConnectionState::LinkDown => {}
            ConnectionState::Connecting
            | ConnectionState::Reconnecting
            | ConnectionState::Disconnecting
            | ConnectionState::Disconnected
            | ConnectionState::Error(_) => return ProbeConnectionStatus::NotApplicable,
        }

        match handle.probe_alive(timeout).await {
            KeepaliveProbeResult::Ok => {
                if matches!(state, ConnectionState::LinkDown) {
                    handle.entry.reset_heartbeat_failures();
                    handle.entry.touch();
                    let _ = self.mark_state_without_event(connection_id, ConnectionState::Active);
                    self.emit_connection_status_changed(connection_id, "connected", Vec::new());
                }
                ProbeConnectionStatus::Alive
            }
            KeepaliveProbeResult::Timeout => {
                if matches!(state, ConnectionState::Active | ConnectionState::Idle) {
                    return ProbeConnectionStatus::Dead;
                }

                // LinkDown grace probing matches Tauri probe_single_connection:
                // a timeout gets one 1.5s retry before the old connection is
                // considered still dead.
                sleep(Duration::from_millis(1500)).await;
                match handle.probe_alive(timeout).await {
                    KeepaliveProbeResult::Ok => {
                        if matches!(state, ConnectionState::LinkDown) {
                            handle.entry.reset_heartbeat_failures();
                            handle.entry.touch();
                            let _ = self
                                .mark_state_without_event(connection_id, ConnectionState::Active);
                            self.emit_connection_status_changed(
                                connection_id,
                                "connected",
                                Vec::new(),
                            );
                        }
                        ProbeConnectionStatus::Alive
                    }
                    KeepaliveProbeResult::Timeout | KeepaliveProbeResult::IoError => {
                        let _ =
                            self.mark_state_without_event(connection_id, ConnectionState::LinkDown);
                        ProbeConnectionStatus::Dead
                    }
                }
            }
            KeepaliveProbeResult::IoError => ProbeConnectionStatus::Dead,
        }
    }

    pub fn acquire_sftp_session(
        &self,
        connection_id: &str,
        consumer_id: impl Into<String>,
    ) -> Option<SshConnectionHandle> {
        self.acquire_consumer_for_connection(
            connection_id,
            ConnectionConsumer::Sftp(consumer_id.into()),
        )
    }

    pub fn acquire_consumer_for_connection(
        &self,
        connection_id: &str,
        consumer: ConnectionConsumer,
    ) -> Option<SshConnectionHandle> {
        let handle = self.get(connection_id)?;
        {
            let mut consumers = handle.entry.consumers.write();
            if !consumers.contains(&consumer) {
                consumers.push(consumer);
                let previous = handle.entry.ref_count.fetch_add(1, Ordering::SeqCst);
                if previous == 0 {
                    handle.entry.cancel_idle_timer();
                    if matches!(*handle.entry.state.read(), ConnectionState::Idle)
                        && handle.has_physical()
                    {
                        *handle.entry.state.write() = ConnectionState::Active;
                    }
                }
            }
        }
        // Adding a consumer must not resurrect a dead transport. The caller has
        // already checked/waited for Active state before borrowing the handle.
        handle.entry.touch();
        Some(handle)
    }

    pub fn mark_sftp_session(
        &self,
        connection_id: &str,
        ready: bool,
        cwd: Option<String>,
    ) -> Option<SftpSessionState> {
        let handle = self.get(connection_id)?;
        let state = SftpSessionState { ready, cwd };
        *handle.entry.sftp_state.write() = state.clone();
        handle.entry.touch();
        Some(state)
    }

    pub fn sftp_session_state(&self, connection_id: &str) -> Option<SftpSessionState> {
        let handle = self.get(connection_id)?;
        Some(handle.entry.sftp_state.read().clone())
    }

    pub fn get(&self, connection_id: &str) -> Option<SshConnectionHandle> {
        let key = self
            .by_id
            .get(connection_id)
            .map(|key| key.value().clone())?;
        let entry = self.by_key.get(&key)?.clone();
        Some(SshConnectionHandle { entry })
    }

    pub fn list(&self) -> Vec<ConnectionInfo> {
        self.by_key.iter().map(|entry| entry.info()).collect()
    }

    pub fn list_connection_summaries(&self) -> Vec<ConnectionPoolEntrySummary> {
        let mut summaries = self
            .by_key
            .iter()
            .map(|entry| ConnectionPoolEntrySummary::from_snapshot(entry.summary_snapshot()))
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| {
            left.username
                .cmp(&right.username)
                .then_with(|| left.host.cmp(&right.host))
                .then_with(|| left.port.cmp(&right.port))
                .then_with(|| left.id.cmp(&right.id))
        });
        summaries
    }

    pub fn set_keep_alive(&self, connection_id: &str, keep_alive: bool) -> Option<ConnectionInfo> {
        let key = self
            .by_id
            .get(connection_id)
            .map(|key| key.value().clone())?;
        let entry = self.by_key.get(&key)?.clone();
        entry.set_keep_alive(keep_alive);
        if keep_alive {
            entry.cancel_idle_timer();
        } else if matches!(*entry.state.read(), ConnectionState::Idle)
            && entry.ref_count.load(Ordering::SeqCst) == 0
        {
            self.start_idle_timer_for_entry(entry.clone());
        }
        entry.touch();
        Some(entry.info())
    }

    fn idle_runtime(&self) -> Option<TokioHandle> {
        self.idle_task_runtime
            .read()
            .clone()
            .or_else(|| TokioHandle::try_current().ok())
    }

    fn start_idle_timer_for_entry(&self, entry: Arc<ConnectionEntry>) {
        let connection_id = entry.connection_id.clone();
        entry.cancel_idle_timer();
        let generation = entry.idle_generation();
        *entry.state.write() = ConnectionState::Idle;
        entry.touch();
        if let Some(emitter) = self.node_event_emitter.read().clone() {
            // Tauri immediately exposes Active -> Idle before the timeout
            // starts; the eventual timeout is a separate disconnected event.
            let _ = emitter.emit_state_from_connection(
                &connection_id,
                &ConnectionState::Idle,
                "idle (timer started)",
            );
        }

        let Some(timeout) = entry.idle_timeout else {
            return;
        };
        if timeout.is_zero() {
            return;
        }
        let Some(runtime) = self.idle_runtime() else {
            return;
        };

        let registry = self.clone();
        runtime.spawn(async move {
            sleep(timeout).await;
            registry
                .disconnect_if_idle_timeout(&connection_id, generation)
                .await;
        });
    }

    async fn disconnect_if_idle_timeout(&self, root_connection_id: &str, generation: u64) {
        let Some(root) = self.get(root_connection_id) else {
            return;
        };
        if root.entry.idle_generation() != generation
            || root.entry.ref_count.load(Ordering::SeqCst) != 0
            || root.entry.is_keep_alive()
            || !matches!(root.state(), ConnectionState::Idle)
        {
            return;
        }

        let affected_children = self
            .descendant_connection_infos(root_connection_id)
            .into_iter()
            .map(|info| info.connection_id)
            .collect::<Vec<_>>();
        for connection_id in affected_children.iter().rev() {
            self.disconnect_idle_timed_out_connection(
                connection_id,
                "ancestor idle timeout cascade",
            )
            .await;
        }
        if let Some(emitter) = self.node_event_emitter.read().clone() {
            emitter.emit_connection_status_changed(
                root_connection_id.to_string(),
                "disconnected".to_string(),
                affected_children,
            );
        }
        self.disconnect_idle_timed_out_connection(root_connection_id, "idle timeout")
            .await;
    }

    async fn disconnect_idle_timed_out_connection(&self, connection_id: &str, reason: &str) {
        let Some(handle) = self.get(connection_id) else {
            return;
        };
        if matches!(
            handle.state(),
            ConnectionState::Disconnected | ConnectionState::Disconnecting
        ) {
            return;
        }
        handle.entry.cancel_idle_timer();
        let info = handle.info();
        let emitter = self.node_event_emitter.read().clone();
        if let (Some(parent_connection_id), Some(emitter)) =
            (info.parent_connection_id.as_ref(), emitter.as_ref())
            && let Some(node_id) = emitter.node_id_for_connection(connection_id)
        {
            self.release(
                parent_connection_id,
                &ConnectionConsumer::NodeRouter(format!("{}:ancestor", node_id.0)),
            );
        }

        handle.clear_physical().await;
        let _ = self.mark_state_without_event(connection_id, ConnectionState::Disconnected);
        if let Some(emitter) = emitter {
            emitter.emit_sftp_ready(connection_id, false, None);
            let _ = emitter.emit_state_from_connection(
                connection_id,
                &ConnectionState::Disconnected,
                reason,
            );
            emitter.unregister(connection_id);
        }
        let _ = self.retire_connection(connection_id);
    }

    pub fn stats(&self) -> ConnectionPoolStats {
        let mut stats = ConnectionPoolStats {
            total: self.by_key.len(),
            ..ConnectionPoolStats::default()
        };
        for entry in self.by_key.iter() {
            match &*entry.state.read() {
                ConnectionState::Active => stats.active += 1,
                ConnectionState::Idle => stats.idle += 1,
                ConnectionState::LinkDown => stats.link_down += 1,
                ConnectionState::Reconnecting => stats.reconnecting += 1,
                ConnectionState::Disconnected | ConnectionState::Disconnecting => {
                    stats.disconnected += 1;
                }
                ConnectionState::Error(_) => stats.errored += 1,
                ConnectionState::Connecting => {}
            }
        }
        stats
    }

    pub fn monitor_stats(&self) -> ConnectionPoolMonitorStats {
        let idle_timeout_secs = self
            .config
            .read()
            .idle_timeout
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let snapshots = self
            .by_key
            .iter()
            .map(|entry| entry.monitor_snapshot())
            .collect::<Vec<_>>();

        ConnectionPoolMonitorStats::from_snapshots(
            snapshots,
            self.config.read().max_connections,
            idle_timeout_secs,
        )
    }

    pub fn connection_topology_snapshot(&self) -> ConnectionTopologySnapshot {
        let infos = self.list();
        let known_ids = infos
            .iter()
            .map(|info| info.connection_id.as_str())
            .collect::<HashSet<_>>();
        let depth_by_id = topology_depths(&infos);
        let mut nodes = infos
            .iter()
            .map(|info| ConnectionTopologyNode {
                connection_id: info.connection_id.clone(),
                parent_connection_id: info.parent_connection_id.clone(),
                host: info.host.clone(),
                port: info.port,
                username: info.username.clone(),
                status: ConnectionTopologyStatus::from(&info.state),
                depth: depth_by_id
                    .get(info.connection_id.as_str())
                    .copied()
                    .unwrap_or_default(),
                ref_count: info.ref_count,
                consumers: topology_consumer_summary(&info.consumers),
            })
            .collect::<Vec<_>>();
        nodes.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| left.parent_connection_id.cmp(&right.parent_connection_id))
                .then_with(|| left.host.cmp(&right.host))
                .then_with(|| left.connection_id.cmp(&right.connection_id))
        });

        let mut edges = infos
            .iter()
            .filter_map(|info| {
                let parent_id = info.parent_connection_id.as_ref()?;
                known_ids
                    .contains(parent_id.as_str())
                    .then(|| ConnectionTopologyEdge {
                        parent_connection_id: parent_id.clone(),
                        child_connection_id: info.connection_id.clone(),
                    })
            })
            .collect::<Vec<_>>();
        edges.sort_by(|left, right| {
            left.parent_connection_id
                .cmp(&right.parent_connection_id)
                .then_with(|| left.child_connection_id.cmp(&right.child_connection_id))
        });

        ConnectionTopologySnapshot::new(nodes, edges)
    }
}

fn topology_depths(infos: &[ConnectionInfo]) -> HashMap<&str, usize> {
    let parents = infos
        .iter()
        .map(|info| {
            (
                info.connection_id.as_str(),
                info.parent_connection_id.as_deref(),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut depths = HashMap::new();
    for info in infos {
        let depth = topology_depth_for(info.connection_id.as_str(), &parents, &mut HashSet::new());
        depths.insert(info.connection_id.as_str(), depth);
    }
    depths
}

fn topology_depth_for<'a>(
    connection_id: &'a str,
    parents: &HashMap<&'a str, Option<&'a str>>,
    seen: &mut HashSet<&'a str>,
) -> usize {
    if !seen.insert(connection_id) {
        return 0;
    }
    let Some(Some(parent_id)) = parents.get(connection_id) else {
        return 0;
    };
    if !parents.contains_key(parent_id) {
        return 0;
    }
    topology_depth_for(parent_id, parents, seen).saturating_add(1)
}

fn topology_consumer_summary(
    consumers: &[ConnectionConsumer],
) -> ConnectionTopologyConsumerSummary {
    let mut summary = ConnectionTopologyConsumerSummary::default();
    for consumer in consumers {
        match consumer {
            ConnectionConsumer::Terminal(_) => summary.terminals += 1,
            ConnectionConsumer::Sftp(_) => summary.sftp += 1,
            ConnectionConsumer::PortForward(_) => summary.port_forwards += 1,
            ConnectionConsumer::Ide(_) => summary.ide += 1,
            ConnectionConsumer::NodeRouter(_) => summary.node_router += 1,
        }
    }
    summary
}

async fn detect_remote_env_for_handle(handle: &SshConnectionHandle) -> RemoteEnvInfo {
    tokio::time::timeout(REMOTE_ENV_TOTAL_TIMEOUT, detect_remote_env_inner(handle))
        .await
        .unwrap_or_else(|_| RemoteEnvInfo::unknown())
}

async fn detect_remote_env_inner(handle: &SshConnectionHandle) -> RemoteEnvInfo {
    let phase_a_output = match handle
        .run_command(
            REMOTE_ENV_PHASE_A_CMD,
            REMOTE_ENV_PHASE_A_TIMEOUT,
            REMOTE_ENV_MAX_OUTPUT_SIZE,
        )
        .await
    {
        Ok(output) => output,
        Err(_) => {
            return handle
                .run_command(
                    REMOTE_ENV_PHASE_B_WINDOWS_CMD,
                    REMOTE_ENV_PHASE_B_TIMEOUT,
                    REMOTE_ENV_MAX_OUTPUT_SIZE,
                )
                .await
                .map(|output| parse_remote_windows_env(&output))
                .unwrap_or_else(|_| RemoteEnvInfo::unknown());
        }
    };

    let is_windows = phase_a_output.contains("PLATFORM=windows");
    let raw_platform = extract_between(&phase_a_output, "PLATFORM=", "\n")
        .unwrap_or_default()
        .trim()
        .to_string();
    let phase_b_command = if is_windows {
        REMOTE_ENV_PHASE_B_WINDOWS_CMD
    } else {
        REMOTE_ENV_PHASE_B_UNIX_CMD
    };

    match handle
        .run_command(
            phase_b_command,
            REMOTE_ENV_PHASE_B_TIMEOUT,
            REMOTE_ENV_MAX_OUTPUT_SIZE,
        )
        .await
    {
        Ok(output) if is_windows => parse_remote_windows_env(&output),
        Ok(output) => parse_remote_unix_env(&output, &raw_platform),
        Err(_) => RemoteEnvInfo {
            os_type: if is_windows {
                "Windows".to_string()
            } else {
                classify_remote_unix_os(&raw_platform)
            },
            os_version: None,
            kernel: None,
            arch: None,
            shell: None,
            detected_at: remote_env_detected_at(),
        },
    }
}

fn parse_remote_unix_env(output: &str, raw_platform: &str) -> RemoteEnvInfo {
    let os_type = classify_remote_unix_os(raw_platform);
    let env_value = extract_section_between(output, "===ENV===", "===ARCH===")
        .map(clean_remote_env_value)
        .filter(|value| !value.is_empty());
    let arch = extract_section_between(output, "===ARCH===", "===KERNEL===")
        .map(clean_remote_env_value)
        .filter(|value| !value.is_empty());
    let kernel = extract_section_between(output, "===KERNEL===", "===SHELL===")
        .map(clean_remote_env_value)
        .filter(|value| !value.is_empty());
    let shell = extract_section_between(output, "===SHELL===", "===DISTRO===")
        .map(clean_remote_env_value)
        .filter(|value| !value.is_empty());
    let distro_block =
        extract_section_between(output, "===DISTRO===", "===END===").unwrap_or_default();
    let os_version = extract_os_release_field(distro_block, "PRETTY_NAME")
        .or_else(|| extract_os_release_field(distro_block, "ID"))
        .or(env_value);

    RemoteEnvInfo {
        os_type,
        os_version,
        kernel,
        arch,
        shell,
        detected_at: remote_env_detected_at(),
    }
}

fn parse_remote_windows_env(output: &str) -> RemoteEnvInfo {
    RemoteEnvInfo {
        os_type: "Windows".to_string(),
        os_version: extract_section_between(output, "===ENV===", "===ARCH===")
            .map(clean_remote_env_value)
            .filter(|value| !value.is_empty()),
        kernel: None,
        arch: extract_section_between(output, "===ARCH===", "===SHELL===")
            .map(clean_remote_env_value)
            .filter(|value| !value.is_empty()),
        shell: extract_section_between(output, "===SHELL===", "===END===")
            .map(clean_remote_env_value)
            .filter(|value| !value.is_empty()),
        detected_at: remote_env_detected_at(),
    }
}

fn classify_remote_unix_os(uname_s: &str) -> String {
    let trimmed = uname_s.trim();
    let upper = trimmed.to_uppercase();
    if upper.starts_with("MINGW32") || upper.starts_with("MINGW64") {
        return "Windows_MinGW".to_string();
    }
    if upper.starts_with("MSYS") {
        return "Windows_MSYS".to_string();
    }
    if upper.starts_with("CYGWIN") {
        return "Windows_Cygwin".to_string();
    }

    match trimmed {
        "Linux" => "Linux".to_string(),
        "Darwin" => "macOS".to_string(),
        "FreeBSD" => "FreeBSD".to_string(),
        "OpenBSD" => "OpenBSD".to_string(),
        "NetBSD" => "NetBSD".to_string(),
        "SunOS" => "SunOS".to_string(),
        "" | "unknown" => "Unknown".to_string(),
        other => other.to_string(),
    }
}

fn extract_between(value: &str, start: &str, end: &str) -> Option<String> {
    let start_index = value.find(start)? + start.len();
    let rest = &value[start_index..];
    let end_index = rest.find(end).unwrap_or(rest.len());
    Some(rest[..end_index].to_string())
}

fn extract_section_between<'a>(value: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_index = value.find(start)? + start.len();
    let rest = &value[start_index..];
    let end_index = rest.find(end).unwrap_or(rest.len());
    Some(rest[..end_index].trim())
}

fn extract_os_release_field(block: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    block.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&prefix)?;
        Some(value.trim_matches('"').to_string())
    })
}

fn clean_remote_env_value(value: impl AsRef<str>) -> String {
    value.as_ref().trim().trim_matches('\r').trim().to_string()
}

fn remote_env_detected_at() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl From<&ConnectionState> for ConnectionTopologyStatus {
    fn from(state: &ConnectionState) -> Self {
        match state {
            ConnectionState::Connecting => Self::Connecting,
            ConnectionState::Active => Self::Active,
            ConnectionState::Idle => Self::Idle,
            ConnectionState::LinkDown => Self::LinkDown,
            ConnectionState::Reconnecting => Self::Reconnecting,
            ConnectionState::Disconnecting => Self::Disconnecting,
            ConnectionState::Disconnected => Self::Disconnected,
            ConnectionState::Error(_) => Self::Error,
        }
    }
}

impl From<&ConnectionState> for ConnectionPoolEntryState {
    fn from(state: &ConnectionState) -> Self {
        match state {
            ConnectionState::Connecting => Self::Connecting,
            ConnectionState::Active => Self::Active,
            ConnectionState::Idle => Self::Idle,
            ConnectionState::LinkDown => Self::LinkDown,
            ConnectionState::Reconnecting => Self::Reconnecting,
            ConnectionState::Disconnecting => Self::Disconnecting,
            ConnectionState::Disconnected => Self::Disconnected,
            ConnectionState::Error(error) => Self::Error(error.clone()),
        }
    }
}

impl From<&ConnectionConsumer> for ConnectionMonitorConsumerKind {
    fn from(consumer: &ConnectionConsumer) -> Self {
        match consumer {
            ConnectionConsumer::Terminal(_) => Self::Terminal,
            ConnectionConsumer::Sftp(_) => Self::Sftp,
            ConnectionConsumer::PortForward(_) => Self::PortForward,
            ConnectionConsumer::Ide(_) | ConnectionConsumer::NodeRouter(_) => Self::Other,
        }
    }
}

impl Default for SshConnectionRegistry {
    fn default() -> Self {
        Self::new(ConnectionPoolConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::{NodeId, NodeReadiness, NodeStateEvent};

    #[test]
    fn shares_one_connection_for_many_consumers() {
        let registry = SshConnectionRegistry::default();
        let config = SshConfig::password("host", 22, "me", "pw");
        let first = registry.acquire(config.clone(), ConnectionConsumer::Terminal("a".into()));
        let second = registry.acquire(config, ConnectionConsumer::Sftp("b".into()));

        assert_eq!(first.connection_id(), second.connection_id());
        assert_eq!(first.info().ref_count, 2);
        assert_eq!(first.state(), ConnectionState::Connecting);
    }

    #[test]
    fn remote_env_is_stored_once_on_connection_entry() {
        let registry = SshConnectionRegistry::default();
        let handle = registry.acquire(
            SshConfig::password("host", 22, "me", "pw"),
            ConnectionConsumer::Terminal("a".into()),
        );
        let first = RemoteEnvInfo {
            os_type: "Linux".to_string(),
            os_version: Some("Ubuntu".to_string()),
            kernel: Some("6.0".to_string()),
            arch: Some("x86_64".to_string()),
            shell: Some("/bin/bash".to_string()),
            detected_at: 1,
        };
        let second = RemoteEnvInfo {
            os_type: "macOS".to_string(),
            os_version: Some("14".to_string()),
            kernel: None,
            arch: None,
            shell: Some("/bin/zsh".to_string()),
            detected_at: 2,
        };

        assert!(handle.set_remote_env(first.clone()));
        assert!(!handle.set_remote_env(second));

        assert_eq!(handle.remote_env(), Some(first.clone()));
        assert_eq!(handle.info().remote_env, Some(first));
    }

    #[test]
    fn remote_env_parser_matches_tauri_unix_os_names() {
        assert_eq!(classify_remote_unix_os("Linux"), "Linux");
        assert_eq!(classify_remote_unix_os("Darwin"), "macOS");
        assert_eq!(classify_remote_unix_os("MINGW64_NT-10.0"), "Windows_MinGW");
        assert_eq!(classify_remote_unix_os(""), "Unknown");
    }

    #[test]
    fn release_moves_unused_connection_to_idle() {
        let registry = SshConnectionRegistry::default();
        let consumer = ConnectionConsumer::Terminal("a".into());
        let handle = registry.acquire(
            SshConfig::password("host", 22, "me", "pw"),
            consumer.clone(),
        );

        registry.release(handle.connection_id(), &consumer);

        assert_eq!(handle.info().ref_count, 0);
        assert_eq!(handle.state(), ConnectionState::Idle);
    }

    #[test]
    fn release_ignores_unknown_consumer_without_decrementing_ref_count() {
        let registry = SshConnectionRegistry::default();
        let consumer = ConnectionConsumer::Terminal("a".into());
        let handle = registry.acquire(
            SshConfig::password("host", 22, "me", "pw"),
            consumer.clone(),
        );

        registry.release(
            handle.connection_id(),
            &ConnectionConsumer::Sftp("missing".into()),
        );

        assert_eq!(handle.info().ref_count, 1);
        assert_eq!(handle.state(), ConnectionState::Connecting);
        registry.release(handle.connection_id(), &consumer);
        assert_eq!(handle.info().ref_count, 0);
    }

    #[test]
    fn monitor_stats_match_tauri_pool_stats_shape() {
        let registry = SshConnectionRegistry::new(ConnectionPoolConfig {
            idle_timeout: Some(Duration::from_secs(120)),
            max_connections: 9,
            protect_on_exit: true,
        });

        let active = registry.acquire(
            SshConfig::password("active.example", 22, "me", "pw"),
            ConnectionConsumer::Terminal("term-1".into()),
        );
        registry.mark_state(active.connection_id(), ConnectionState::Active);
        registry.acquire_consumer_for_connection(
            active.connection_id(),
            ConnectionConsumer::Terminal("term-2".into()),
        );
        registry.acquire_consumer_for_connection(
            active.connection_id(),
            ConnectionConsumer::Sftp("sftp-1".into()),
        );
        registry.mark_sftp_session(active.connection_id(), true, Some("/home/me".into()));
        registry.acquire_consumer_for_connection(
            active.connection_id(),
            ConnectionConsumer::PortForward("forward-1".into()),
        );

        let idle_consumer = ConnectionConsumer::NodeRouter("idle".into());
        let idle = registry.acquire(
            SshConfig::password("idle.example", 22, "me", "pw"),
            idle_consumer.clone(),
        );
        registry.release(idle.connection_id(), &idle_consumer);

        let link_down_consumer = ConnectionConsumer::NodeRouter("link-down".into());
        let link_down = registry.acquire(
            SshConfig::password("link-down.example", 22, "me", "pw"),
            link_down_consumer.clone(),
        );
        registry.release(link_down.connection_id(), &link_down_consumer);
        registry.mark_state(link_down.connection_id(), ConnectionState::LinkDown);

        let reconnecting_consumer = ConnectionConsumer::NodeRouter("reconnecting".into());
        let reconnecting = registry.acquire(
            SshConfig::password("reconnecting.example", 22, "me", "pw"),
            reconnecting_consumer.clone(),
        );
        registry.release(reconnecting.connection_id(), &reconnecting_consumer);
        registry.mark_state(reconnecting.connection_id(), ConnectionState::Reconnecting);

        let stats = registry.monitor_stats();

        assert_eq!(stats.total_connections, 4);
        assert_eq!(stats.active_connections, 1);
        assert_eq!(stats.idle_connections, 1);
        assert_eq!(stats.link_down_connections, 1);
        assert_eq!(stats.reconnecting_connections, 1);
        assert_eq!(stats.total_terminals, 2);
        assert_eq!(stats.total_sftp_sessions, 1);
        assert_eq!(stats.total_forwards, 1);
        assert_eq!(stats.total_ref_count, 4);
        assert_eq!(stats.pool_capacity, 9);
        assert_eq!(stats.idle_timeout_secs, 120);
    }

    #[test]
    fn connection_summaries_match_tauri_pool_card_fields() {
        let registry = SshConnectionRegistry::default();
        let handle = registry.acquire(
            SshConfig::password("pool.example", 2222, "alice", "pw"),
            ConnectionConsumer::Terminal("term-1".into()),
        );
        registry.mark_state(handle.connection_id(), ConnectionState::Active);
        registry.acquire_consumer_for_connection(
            handle.connection_id(),
            ConnectionConsumer::Terminal("term-2".into()),
        );
        registry.acquire_consumer_for_connection(
            handle.connection_id(),
            ConnectionConsumer::Sftp("sftp-1".into()),
        );
        registry.mark_sftp_session(handle.connection_id(), true, Some("/home/alice".into()));
        registry.acquire_consumer_for_connection(
            handle.connection_id(),
            ConnectionConsumer::PortForward("forward-1".into()),
        );
        registry.set_keep_alive(handle.connection_id(), true);

        let summary = registry
            .list_connection_summaries()
            .into_iter()
            .find(|summary| summary.id == handle.connection_id())
            .expect("summary exists");

        assert_eq!(summary.host, "pool.example");
        assert_eq!(summary.port, 2222);
        assert_eq!(summary.username, "alice");
        assert_eq!(summary.state, ConnectionPoolEntryState::Active);
        assert_eq!(summary.ref_count, 4);
        assert!(summary.keep_alive);
        assert_eq!(summary.terminal_count, 2);
        assert!(summary.has_sftp_session);
        assert_eq!(summary.forward_count, 1);
        assert!(summary.parent_connection_id.is_none());
    }

    #[test]
    fn connection_summaries_classify_unrecoverable_as_not_active() {
        let registry = SshConnectionRegistry::default();
        let handle = registry.acquire(
            SshConfig::password("dead.example", 22, "alice", "pw"),
            ConnectionConsumer::NodeRouter("dead".into()),
        );
        registry.mark_state(
            handle.connection_id(),
            ConnectionState::Error("auth failed".into()),
        );

        let summary = registry
            .list_connection_summaries()
            .into_iter()
            .find(|summary| summary.id == handle.connection_id())
            .expect("summary exists");

        assert!(!summary.state.is_counted_active());
        assert!(summary.is_displayed_in_pool());
        assert_eq!(
            summary.state,
            ConnectionPoolEntryState::Error("auth failed".into())
        );
    }

    #[tokio::test]
    async fn idle_timeout_disconnects_unused_connection() {
        let registry = SshConnectionRegistry::new(ConnectionPoolConfig {
            idle_timeout: Some(Duration::from_millis(10)),
            max_connections: 4,
            protect_on_exit: true,
        });
        registry.set_task_runtime(tokio::runtime::Handle::current());
        let consumer = ConnectionConsumer::Terminal("term-1".into());
        let handle = registry.acquire(
            SshConfig::password("idle-timeout.example", 22, "alice", "pw"),
            consumer.clone(),
        );
        registry.mark_state(handle.connection_id(), ConnectionState::Active);

        registry.release(handle.connection_id(), &consumer);
        sleep(Duration::from_millis(40)).await;

        assert!(registry.get(handle.connection_id()).is_none());
    }

    #[tokio::test]
    async fn keep_alive_cancels_idle_timeout_disconnect() {
        let registry = SshConnectionRegistry::new(ConnectionPoolConfig {
            idle_timeout: Some(Duration::from_millis(10)),
            max_connections: 4,
            protect_on_exit: true,
        });
        registry.set_task_runtime(tokio::runtime::Handle::current());
        let consumer = ConnectionConsumer::Terminal("term-1".into());
        let handle = registry.acquire(
            SshConfig::password("keepalive.example", 22, "alice", "pw"),
            consumer.clone(),
        );
        registry.mark_state(handle.connection_id(), ConnectionState::Active);
        registry.set_keep_alive(handle.connection_id(), true);

        registry.release(handle.connection_id(), &consumer);
        sleep(Duration::from_millis(40)).await;

        let info = registry.get(handle.connection_id()).unwrap().info();
        assert_eq!(info.state, ConnectionState::Idle);
        assert!(info.keep_alive);
    }

    #[tokio::test]
    async fn idle_timeout_updates_across_registry_clones() {
        let registry = SshConnectionRegistry::new(ConnectionPoolConfig {
            idle_timeout: Some(Duration::from_secs(60)),
            max_connections: 4,
            protect_on_exit: true,
        });
        registry.set_task_runtime(tokio::runtime::Handle::current());
        let clone = registry.clone();
        clone.set_idle_timeout(Some(Duration::from_millis(10)));

        let consumer = ConnectionConsumer::Terminal("term-1".into());
        let handle = registry.acquire(
            SshConfig::password("dynamic-timeout.example", 22, "alice", "pw"),
            consumer.clone(),
        );
        registry.mark_state(handle.connection_id(), ConnectionState::Active);

        clone.release(handle.connection_id(), &consumer);
        sleep(Duration::from_millis(40)).await;

        assert!(registry.get(handle.connection_id()).is_none());
    }

    #[test]
    fn connection_topology_snapshot_uses_registry_parent_edges_and_consumer_counts() {
        let registry = SshConnectionRegistry::default();
        let root = registry.acquire(
            SshConfig::password("jump.example", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("jump".into()),
        );
        registry.mark_state(root.connection_id(), ConnectionState::Active);
        let child = registry.acquire(
            SshConfig::password("target.example", 22, "me", "pw"),
            ConnectionConsumer::Terminal("term-target".into()),
        );
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(root.connection_id().to_string()),
        );
        registry.acquire_consumer_for_connection(
            child.connection_id(),
            ConnectionConsumer::Sftp("target:sftp".into()),
        );
        registry.acquire_consumer_for_connection(
            child.connection_id(),
            ConnectionConsumer::PortForward("target:forward".into()),
        );
        registry.acquire_consumer_for_connection(
            child.connection_id(),
            ConnectionConsumer::Ide("target:ide".into()),
        );

        let snapshot = registry.connection_topology_snapshot();

        assert_eq!(snapshot.root_count, 1);
        assert_eq!(snapshot.child_count, 1);
        assert_eq!(
            snapshot.edges,
            vec![ConnectionTopologyEdge {
                parent_connection_id: root.connection_id().to_string(),
                child_connection_id: child.connection_id().to_string(),
            }]
        );
        let root_node = snapshot
            .nodes
            .iter()
            .find(|node| node.connection_id == root.connection_id())
            .expect("root topology node");
        assert_eq!(root_node.depth, 0);
        assert_eq!(root_node.status, ConnectionTopologyStatus::Active);
        assert_eq!(root_node.consumers.node_router, 1);
        let child_node = snapshot
            .nodes
            .iter()
            .find(|node| node.connection_id == child.connection_id())
            .expect("child topology node");
        assert_eq!(child_node.depth, 1);
        assert_eq!(
            child_node.parent_connection_id.as_deref(),
            Some(root.connection_id())
        );
        assert_eq!(child_node.consumers.terminals, 1);
        assert_eq!(child_node.consumers.sftp, 1);
        assert_eq!(child_node.consumers.port_forwards, 1);
        assert_eq!(child_node.consumers.ide, 1);
        assert_eq!(child_node.consumers.total(), 4);
    }

    #[test]
    fn stores_one_physical_connection_slot_per_entry() {
        let registry = SshConnectionRegistry::default();
        let first = registry.acquire(
            SshConfig::password("host", 22, "me", "pw"),
            ConnectionConsumer::Terminal("a".into()),
        );
        let second = registry.acquire(
            SshConfig::password("host", 22, "me", "pw"),
            ConnectionConsumer::Sftp("b".into()),
        );
        first.set_physical(Arc::new(String::from("authenticated")));

        assert_eq!(
            second.physical::<String>().as_deref().map(String::as_str),
            Some("authenticated")
        );
    }

    #[test]
    fn link_down_cascade_follows_parent_connection_ids_not_proxy_hosts() {
        let registry = SshConnectionRegistry::default();
        let root = registry.acquire(
            SshConfig::password("jump", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("root".into()),
        );
        let child = registry.acquire(
            SshConfig::password("target", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("child".into()),
        );
        let unrelated = registry.acquire(
            SshConfig::password("target", 22, "other", "pw"),
            ConnectionConsumer::NodeRouter("unrelated".into()),
        );
        registry.mark_state(root.connection_id(), ConnectionState::Active);
        registry.mark_state(child.connection_id(), ConnectionState::Active);
        registry.mark_state(unrelated.connection_id(), ConnectionState::Active);
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(root.connection_id().to_string()),
        );

        let changed = registry.mark_link_down_cascade(root.connection_id());
        let changed_ids = changed
            .iter()
            .map(|info| info.connection_id.as_str())
            .collect::<Vec<_>>();

        assert!(changed_ids.contains(&root.connection_id()));
        assert!(changed_ids.contains(&child.connection_id()));
        assert!(!changed_ids.contains(&unrelated.connection_id()));
        assert_eq!(root.state(), ConnectionState::LinkDown);
        assert_eq!(child.state(), ConnectionState::LinkDown);
        assert_eq!(unrelated.state(), ConnectionState::Active);
    }

    #[tokio::test]
    async fn transport_lost_cascade_clears_stale_physical_slots() {
        let registry = SshConnectionRegistry::default();
        let root = registry.acquire(
            SshConfig::password("jump", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("root".into()),
        );
        let child = registry.acquire(
            SshConfig::password("target", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("child".into()),
        );
        let unrelated = registry.acquire(
            SshConfig::password("other", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("other".into()),
        );
        registry.mark_state(root.connection_id(), ConnectionState::Active);
        registry.mark_state(child.connection_id(), ConnectionState::Active);
        registry.mark_state(unrelated.connection_id(), ConnectionState::Active);
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(root.connection_id().to_string()),
        );
        root.set_physical(Arc::new(String::from("root-transport")));
        child.set_physical(Arc::new(String::from("child-transport")));
        unrelated.set_physical(Arc::new(String::from("unrelated-transport")));

        let changed = registry
            .mark_transport_lost_cascade(root.connection_id(), "terminal input write failed")
            .await;
        let changed_ids = changed
            .iter()
            .map(|info| info.connection_id.as_str())
            .collect::<Vec<_>>();

        assert!(changed_ids.contains(&root.connection_id()));
        assert!(changed_ids.contains(&child.connection_id()));
        assert!(!changed_ids.contains(&unrelated.connection_id()));
        assert_eq!(root.state(), ConnectionState::LinkDown);
        assert_eq!(child.state(), ConnectionState::LinkDown);
        assert_eq!(unrelated.state(), ConnectionState::Active);
        assert!(root.physical::<String>().is_none());
        assert!(child.physical::<String>().is_none());
        assert_eq!(
            unrelated
                .physical::<String>()
                .as_deref()
                .map(String::as_str),
            Some("unrelated-transport")
        );
    }

    #[test]
    fn tunneled_child_parent_ref_is_released_by_ancestor_consumer() {
        let registry = SshConnectionRegistry::default();
        let root = registry.acquire(
            SshConfig::password("jump", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("root".into()),
        );
        let parent_ref = ConnectionConsumer::NodeRouter("child:ancestor".into());
        let parent_for_child = registry
            .acquire_consumer_for_connection(root.connection_id(), parent_ref.clone())
            .unwrap();
        let child = registry.acquire(
            SshConfig::password("target", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("child".into()),
        );
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(parent_for_child.connection_id().to_string()),
        );

        assert_eq!(root.info().ref_count, 2);
        registry.release(root.connection_id(), &parent_ref);

        assert_eq!(root.info().ref_count, 1);
        assert!(
            root.info()
                .consumers
                .contains(&ConnectionConsumer::NodeRouter("root".into()))
        );
    }

    #[test]
    fn link_down_cascade_emits_tauri_shaped_status_event() {
        let registry = SshConnectionRegistry::default();
        let emitter = NodeEventEmitter::new();
        let (tx, rx) = std::sync::mpsc::channel();
        emitter.subscribe(tx);
        registry.set_node_event_emitter(emitter.clone());

        let root = registry.acquire(
            SshConfig::password("jump", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("root".into()),
        );
        let child = registry.acquire(
            SshConfig::password("target", 22, "me", "pw"),
            ConnectionConsumer::NodeRouter("child".into()),
        );
        emitter.register(root.connection_id(), NodeId::new("root"));
        emitter.register(child.connection_id(), NodeId::new("child"));
        registry.mark_state_without_event(root.connection_id(), ConnectionState::Active);
        registry.mark_state_without_event(child.connection_id(), ConnectionState::Active);
        registry.set_parent_connection_id(
            child.connection_id(),
            Some(root.connection_id().to_string()),
        );

        registry.mark_link_down_cascade(root.connection_id());

        match rx.recv().unwrap() {
            NodeStateEvent::ConnectionStatusChanged {
                connection_id,
                status,
                affected_children,
                ..
            } => {
                assert_eq!(connection_id, root.connection_id());
                assert_eq!(status, "link_down");
                assert_eq!(affected_children, vec![child.connection_id().to_string()]);
            }
            event => panic!("expected connection status event, got {event:?}"),
        }
        match rx.recv().unwrap() {
            NodeStateEvent::ConnectionStateChanged { node_id, state, .. } => {
                assert_eq!(node_id, "root");
                assert_eq!(state, NodeReadiness::Error);
            }
            event => panic!("expected root node state event, got {event:?}"),
        }

        registry.mark_link_down_cascade(root.connection_id());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn retiring_connection_allows_same_config_to_receive_new_id() {
        let registry = SshConnectionRegistry::default();
        let config = SshConfig::password("host", 22, "me", "pw");
        let first = registry.acquire(
            config.clone(),
            ConnectionConsumer::NodeRouter("node-a".into()),
        );
        let first_id = first.connection_id().to_string();

        let retired = registry.retire_connection(&first_id).unwrap();
        let second = registry.acquire(config, ConnectionConsumer::NodeRouter("node-a".into()));

        assert_eq!(retired.connection_id, first_id);
        assert_ne!(second.connection_id(), first_id);
        assert!(registry.get(&first_id).is_none());
        assert!(registry.get(second.connection_id()).is_some());
    }
}
