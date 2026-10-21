// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt,
    sync::{
        Arc, Weak,
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
use parking_lot::{Mutex as ParkingMutex, RwLock};
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
// SSH exec uses the login shell, which may not understand POSIX conditionals.
const REMOTE_ENV_PHASE_A_CMD: &str = r#"sh -c 'echo "===DETECT==="; if [ -n "$PSModulePath" ]; then echo "PLATFORM=windows"; else echo "PLATFORM=$(uname -s 2>/dev/null || echo unknown)"; fi; echo "===END==="'"#;
const REMOTE_ENV_PHASE_B_UNIX_CMD: &str = "echo '===ENV==='; uname -s 2>/dev/null; echo '===ARCH==='; uname -m 2>/dev/null; echo '===KERNEL==='; uname -r 2>/dev/null; echo '===SHELL==='; echo $SHELL 2>/dev/null; echo '===HOME==='; echo $HOME 2>/dev/null; echo '===ZDOTDIR==='; echo $ZDOTDIR 2>/dev/null; echo '===XDG_CONFIG_HOME==='; echo $XDG_CONFIG_HOME 2>/dev/null; echo '===DISTRO==='; cat /etc/os-release 2>/dev/null | grep -E '^(PRETTY_NAME|ID)=' | head -2; echo '===END==='";
const REMOTE_ENV_PHASE_B_WINDOWS_CMD: &str = "echo '===PLATFORM==='; [System.Environment]::OSVersion.Platform; echo '===ENV==='; [System.Environment]::OSVersion.VersionString; echo '===ARCH==='; $env:PROCESSOR_ARCHITECTURE; echo '===SHELL==='; \"PowerShell $($PSVersionTable.PSVersion)\"; echo '===HOME==='; $HOME; echo '===ZDOTDIR==='; echo '===XDG_CONFIG_HOME==='; echo '===END==='";

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

fn connection_state_label(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connecting => "connecting",
        ConnectionState::Active => "active",
        ConnectionState::Idle => "idle",
        ConnectionState::LinkDown => "link_down",
        ConnectionState::Reconnecting => "reconnecting",
        ConnectionState::Disconnecting => "disconnecting",
        ConnectionState::Disconnected => "disconnected",
        ConnectionState::Error(_) => "error",
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionConsumer {
    Terminal(String),
    Sftp(String),
    PortForward(String),
    Monitor(String),
    X11Forward(String),
    Ide(String),
    NodeRouter(String),
    PublicMcp(String),
    MoshBootstrap(String),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zdotdir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xdg_config_home: Option<String>,
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
            home: None,
            zdotdir: None,
            xdg_config_home: None,
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
    pub generation: u64,
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

#[derive(Debug, Default)]
struct PhysicalConnection {
    transport: Option<Arc<dyn Any + Send + Sync>>,
    audit: Option<oxideterm_audit::AuditContext>,
}

#[derive(Debug)]
struct ConnectionEntry {
    audit: Option<oxideterm_audit::AuditContext>,
    connection_id: String,
    key: String,
    config: SshConfig,
    ownership_transition: ParkingMutex<()>,
    parent_connection_id: RwLock<Option<String>>,
    parent_connection_consumer: RwLock<Option<ConnectionConsumer>>,
    state: RwLock<ConnectionState>,
    ref_count: AtomicU64,
    keep_alive: AtomicBool,
    consumers: RwLock<Vec<ConnectionConsumer>>,
    physical: RwLock<PhysicalConnection>,
    sftp: Mutex<SharedSftpState>,
    active_sftp: ParkingMutex<Vec<(u64, Weak<SftpSession>)>>,
    sftp_generation: AtomicU64,
    sftp_state: RwLock<SftpSessionState>,
    remote_env: RwLock<Option<RemoteEnvInfo>>,
    remote_env_detection_started: AtomicBool,
    first_visible_terminal_started: AtomicBool,
    heartbeat_failures: AtomicU64,
    idle_generation: AtomicU64,
    last_emitted_status: RwLock<Option<String>>,
    created_at: SystemTime,
    last_active_at: RwLock<SystemTime>,
    idle_timeout: Option<Duration>,
    retire_when_unused: bool,
}

impl ConnectionEntry {
    fn new(
        config: SshConfig,
        pool_config: ConnectionPoolConfig,
        audit: Option<&oxideterm_audit::AuditContext>,
    ) -> Self {
        let key = config.connection_key();
        Self::new_with_key(config, key, pool_config, false, audit)
    }

    fn new_with_key(
        config: SshConfig,
        key: String,
        pool_config: ConnectionPoolConfig,
        retire_when_unused: bool,
        audit: Option<&oxideterm_audit::AuditContext>,
    ) -> Self {
        let connection_id = Uuid::new_v4().to_string();
        let audit = audit.map(|context| {
            let mut context = context.session(
                "ssh",
                &format!("{}@{}:{}", config.username, config.host, config.port),
            );
            context.remote_account = Some(oxideterm_audit::redact(&config.username));
            context.connection_id = Some(zeroize::Zeroizing::new(connection_id.clone()));
            context
        });
        Self {
            audit,
            connection_id,
            key,
            config,
            ownership_transition: ParkingMutex::new(()),
            parent_connection_id: RwLock::new(None),
            parent_connection_consumer: RwLock::new(None),
            state: RwLock::new(ConnectionState::Connecting),
            ref_count: AtomicU64::new(0),
            keep_alive: AtomicBool::new(false),
            consumers: RwLock::new(Vec::new()),
            physical: RwLock::new(PhysicalConnection::default()),
            sftp: Mutex::new(SharedSftpState::Empty),
            active_sftp: ParkingMutex::new(Vec::new()),
            sftp_generation: AtomicU64::new(0),
            sftp_state: RwLock::new(SftpSessionState::default()),
            remote_env: RwLock::new(None),
            remote_env_detection_started: AtomicBool::new(false),
            first_visible_terminal_started: AtomicBool::new(false),
            heartbeat_failures: AtomicU64::new(0),
            idle_generation: AtomicU64::new(0),
            last_emitted_status: RwLock::new(None),
            created_at: SystemTime::now(),
            last_active_at: RwLock::new(SystemTime::now()),
            idle_timeout: pool_config.idle_timeout,
            retire_when_unused,
        }
    }

    fn set_state(&self, state: ConnectionState) {
        let transition = {
            let mut current = self.state.write();
            let transition = (*current != state).then(|| {
                (
                    connection_state_label(&current),
                    connection_state_label(&state),
                )
            });
            *current = state;
            transition
        };
        if let Some((from, to)) = transition {
            self.audit_observation(
                "ssh_connection_state",
                &serde_json::json!({"from": from, "to": to}).to_string(),
            );
        }
    }

    fn audit_observation(&self, action: &str, detail: &str) {
        if let Some(mut context) = self
            .physical
            .read()
            .audit
            .clone()
            .or_else(|| self.audit.clone())
        {
            context.source = oxideterm_audit::AuditSource::System;
            context.observe(
                oxideterm_audit::AuditCategory::Connection,
                action,
                Some(detail),
                oxideterm_audit::AuditOutcome::Succeeded,
                oxideterm_audit::AuditEvidence::Lifecycle,
                oxideterm_audit::AuditAuthorization::NotRequired,
            );
        }
    }

    fn audit_consumer(&self, action: &str, consumer: &ConnectionConsumer) {
        if let Ok(detail) = serde_json::to_string(consumer) {
            self.audit_observation(action, &detail);
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
        self.remote_env.read().clone()
    }

    fn set_remote_env(&self, env: RemoteEnvInfo) -> bool {
        if env.os_type.eq_ignore_ascii_case("unknown") {
            // A timeout or failed probe is transient. Do not make that failure
            // the immutable platform identity for the lifetime of the connection.
            self.remote_env_detection_started
                .store(false, Ordering::Release);
            return false;
        }
        let mut cached = self.remote_env.write();
        if cached.is_some() {
            return false;
        }
        *cached = Some(env);
        true
    }

    fn try_begin_remote_env_detection(&self) -> bool {
        self.remote_env.read().is_none()
            && !self
                .remote_env_detection_started
                .swap(true, Ordering::AcqRel)
    }

    fn mark_first_visible_terminal_started(&self) -> bool {
        !self
            .first_visible_terminal_started
            .swap(true, Ordering::AcqRel)
    }

    fn first_visible_terminal_started(&self) -> bool {
        self.first_visible_terminal_started.load(Ordering::Acquire)
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

/// Owns one registry-managed physical connection for a single logical consumer.
pub struct DedicatedConnectionLease {
    registry: SshConnectionRegistry,
    handle: SshConnectionHandle,
    consumer: ConnectionConsumer,
}

impl DedicatedConnectionLease {
    pub(crate) fn new(
        registry: SshConnectionRegistry,
        handle: SshConnectionHandle,
        consumer: ConnectionConsumer,
    ) -> Self {
        Self {
            registry,
            handle,
            consumer,
        }
    }

    pub fn handle(&self) -> &SshConnectionHandle {
        &self.handle
    }

    pub fn connection_id(&self) -> &str {
        self.handle.connection_id()
    }
}

impl Drop for DedicatedConnectionLease {
    fn drop(&mut self) {
        // The lease is the consumer lifetime boundary; the registry retires the
        // isolated transport without changing the parent node's readiness.
        self.registry
            .release(self.handle.connection_id(), &self.consumer);
    }
}

impl SshConnectionHandle {
    pub fn audit_context(&self) -> Option<oxideterm_audit::AuditContext> {
        self.entry
            .physical
            .read()
            .audit
            .clone()
            .or_else(|| self.entry.audit.clone())
    }

    pub(crate) fn base_audit_context(&self) -> Option<oxideterm_audit::AuditContext> {
        self.entry.audit.clone()
    }

    pub fn connection_id(&self) -> &str {
        &self.entry.connection_id
    }

    pub fn key(&self) -> &str {
        &self.entry.key
    }

    pub(crate) fn config(&self) -> &SshConfig {
        // The registry remains the sole owner of authentication material while
        // consumers borrow non-reconnect shell settings from the live entry.
        &self.entry.config
    }

    pub fn info(&self) -> ConnectionInfo {
        self.entry.info()
    }

    pub fn remote_env(&self) -> Option<RemoteEnvInfo> {
        self.entry.remote_env()
    }

    pub fn remote_env_detection_running(&self) -> bool {
        self.entry.remote_env.read().is_none()
            && self
                .entry
                .remote_env_detection_started
                .load(Ordering::Acquire)
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
        self.entry.physical.read().transport.is_some()
    }

    pub fn physical<T>(&self) -> Option<Arc<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        self.entry
            .physical
            .read()
            .transport
            .as_ref()
            .cloned()
            .and_then(|physical| Arc::downcast::<T>(physical).ok())
    }

    pub fn set_physical<T>(&self, physical: Arc<T>)
    where
        T: Any + Send + Sync + 'static,
    {
        self.set_physical_with_audit(physical, None);
    }

    pub(crate) fn set_physical_with_audit<T>(
        &self,
        physical: Arc<T>,
        audit: Option<oxideterm_audit::AuditContext>,
    ) where
        T: Any + Send + Sync + 'static,
    {
        *self.entry.physical.write() = PhysicalConnection {
            transport: Some(physical),
            audit,
        };
        self.entry.touch();
    }

    pub async fn clear_physical(&self) {
        // Keep the retired transport identity for its final lifecycle observations.
        self.entry.physical.write().transport = None;
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

    /// Returns only the already-open shared channel for the exact owner
    /// generation. It never creates or substitutes a replacement session.
    pub async fn acquire_existing_sftp_generation(
        &self,
        expected_generation: u64,
    ) -> Option<Arc<Mutex<SftpSession>>> {
        let guard = self.entry.sftp.lock().await;
        if self.entry.sftp_generation.load(Ordering::Acquire) != expected_generation {
            return None;
        }
        match &*guard {
            SharedSftpState::Ready(session) => Some(Arc::clone(session)),
            SharedSftpState::Empty | SharedSftpState::Initializing { .. } => None,
        }
    }

    pub async fn acquire_sftp_with_meta(&self) -> Result<AcquiredSftpMeta, SftpError> {
        loop {
            let initializing = {
                let mut guard = self.entry.sftp.lock().await;
                match &*guard {
                    SharedSftpState::Ready(session) => {
                        let generation = self.entry.sftp_generation.load(Ordering::Acquire);
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
                            generation,
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

            let created = self.acquire_transfer_sftp().await;
            let mut guard = self.entry.sftp.lock().await;
            match created {
                Ok(sftp) => {
                    let cwd = Some(sftp.cwd().to_string());
                    let session = Arc::new(Mutex::new(sftp));
                    match &*guard {
                        SharedSftpState::Ready(existing) => {
                            let generation = self.entry.sftp_generation.load(Ordering::Acquire);
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
                                generation,
                            });
                        }
                        SharedSftpState::Initializing { notify, generation }
                            if *generation
                                == self.entry.sftp_generation.load(Ordering::Acquire) =>
                        {
                            let generation = *generation;
                            let notify = notify.clone();
                            *guard = SharedSftpState::Ready(Arc::clone(&session));
                            notify.notify_waiters();
                            self.entry.touch();
                            return Ok(AcquiredSftpMeta {
                                session,
                                was_new: true,
                                cwd,
                                generation,
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
        let generation = self.entry.sftp_generation.load(Ordering::Acquire);
        for attempt in 0..6 {
            match self.create_sftp_session().await {
                Ok(session) => {
                    let session = Arc::new(session);
                    let mut active = self.entry.active_sftp.lock();
                    let current_generation = self.entry.sftp_generation.load(Ordering::Acquire);
                    active.retain(|(g, session)| {
                        *g == current_generation && session.strong_count() > 0
                    });
                    if generation == current_generation {
                        active.push((generation, Arc::downgrade(&session)));
                    }
                    // Only callers retain the channel; the registry must not keep idle
                    // transfer channels (or their physical connection owner) alive.
                    return Ok(session.share().with_connection_owner(session));
                }
                Err(error @ SftpError::ChannelOpenRejected(_)) => {
                    if generation != self.entry.sftp_generation.load(Ordering::Acquire) {
                        return Err(error);
                    }
                    let existing = {
                        let active = self.entry.active_sftp.lock();
                        active
                            .iter()
                            .filter(|(g, _)| *g == generation)
                            .filter_map(|(_, session)| session.upgrade())
                            .find(|session| !session.is_closed())
                    };
                    if let Some(session) = existing {
                        return Ok(session
                            .share()
                            .with_single_channel_transport()
                            .with_connection_owner(session));
                    }
                    if attempt == 5 {
                        return Err(error);
                    }
                    // The last consumer may have just dropped a channel. Its close
                    // reaches sshd asynchronously; retry only rejected opens, never writes.
                    sleep(Duration::from_millis(20 << attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("last rejected open returns its error")
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
    audit: Option<oxideterm_audit::AuditContext>,
    config: Arc<RwLock<ConnectionPoolConfig>>,
    by_key: Arc<DashMap<String, Arc<ConnectionEntry>>>,
    by_id: Arc<DashMap<String, String>>,
    idle_task_runtime: Arc<RwLock<Option<TokioHandle>>>,
    node_event_emitter: Arc<RwLock<Option<NodeEventEmitter>>>,
}

impl SshConnectionRegistry {
    pub fn audit_context(&self) -> Option<oxideterm_audit::AuditContext> {
        self.audit.clone()
    }

    pub fn new(config: ConnectionPoolConfig) -> Self {
        Self::with_audit(config, oxideterm_audit::AuditContext::current())
    }

    pub fn with_audit(
        config: ConnectionPoolConfig,
        audit: Option<oxideterm_audit::AuditContext>,
    ) -> Self {
        Self {
            audit,
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
                let entry = Arc::new(ConnectionEntry::new(
                    config,
                    *self.config.read(),
                    self.audit.as_ref(),
                ));
                self.by_id.insert(entry.connection_id.clone(), key);
                entry
            })
            .clone();

        entry.cancel_idle_timer();
        entry.touch();
        let added = {
            let mut consumers = entry.consumers.write();
            if consumers.contains(&consumer) {
                false
            } else {
                consumers.push(consumer.clone());
                entry.ref_count.fetch_add(1, Ordering::SeqCst);
                true
            }
        };
        if added {
            entry.audit_consumer("ssh_consumer_attached", &consumer);
        }
        // `acquire` only records a logical consumer. The physical SSH transport
        // is established by connect_tree_node / terminal connect paths and
        // marks the state Active after authentication succeeds. Marking Active
        // here made SFTP/forwarding believe a closed terminal-owned transport
        // was reusable, which diverges from Tauri's node-owned pool semantics.
        SshConnectionHandle { entry }
    }

    pub fn acquire_dedicated(
        &self,
        config: SshConfig,
        consumer: ConnectionConsumer,
    ) -> SshConnectionHandle {
        let pool_key = format!("{}|dedicated={}", config.connection_key(), Uuid::new_v4());
        // Dedicated consumers remain registry-owned without joining the shared
        // node pool. Their transport retires when the explicit owner releases it.
        let entry = Arc::new(ConnectionEntry::new_with_key(
            config,
            pool_key.clone(),
            *self.config.read(),
            true,
            self.audit.as_ref(),
        ));
        entry.consumers.write().push(consumer.clone());
        entry.ref_count.store(1, Ordering::SeqCst);
        self.by_id
            .insert(entry.connection_id.clone(), pool_key.clone());
        self.by_key.insert(pool_key, entry.clone());
        entry.audit_consumer("ssh_consumer_attached", &consumer);
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
            entry.audit_consumer("ssh_consumer_detached", consumer);
        }
        entry.touch();
        if entry.ref_count.load(Ordering::SeqCst) == 0 {
            if entry.retire_when_unused {
                let _ = self.retire_connection(connection_id);
                return;
            }
            if entry.is_keep_alive() {
                entry.cancel_idle_timer();
                entry.set_state(ConnectionState::Idle);
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
        entry.set_state(state);
        entry.touch();
        let info = entry.info();
        if emit_node_event && let Some(emitter) = self.node_event_emitter.read().clone() {
            // Match Tauri's registry-to-node event flow: low-level connection
            // state changes are translated through the shared NodeEventEmitter
            // whenever the connection has been registered to a node.
            let _ = emitter.emit_state_from_connection(&info.connection_id, &info.state, reason);
        }
        if became_active && entry.first_visible_terminal_started() && !entry.retire_when_unused {
            // Match Tauri's environment detector gate: hidden exec/shell probes
            // must not be the first session on a fresh SSH login because PAM
            // MOTD/lastlog output belongs to the user's first visible terminal.
            self.maybe_spawn_remote_env_detection(entry);
        }
        Some(info)
    }

    pub fn mark_visible_terminal_ready(&self, connection_id: &str) -> Option<bool> {
        let key = self
            .by_id
            .get(connection_id)
            .map(|key| key.value().clone())?;
        let entry = self.by_key.get(&key)?.clone();
        let first = entry.mark_first_visible_terminal_started();
        if first && !entry.retire_when_unused {
            // A one-terminal connection must not start an environment probe
            // that opens a second channel on servers requiring isolated login.
            self.maybe_spawn_remote_env_detection(entry);
        }
        Some(first)
    }

    pub fn retry_remote_env_detection(&self, connection_id: &str) {
        let Some(handle) = self.get(connection_id) else {
            return;
        };
        // Preserve first-terminal output and single-channel connection isolation.
        if handle.entry.first_visible_terminal_started() && !handle.entry.retire_when_unused {
            self.maybe_spawn_remote_env_detection(handle.entry);
        }
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
        let _ownership_transition = entry.ownership_transition.lock();
        if !self.connection_entry_is_registered(connection_id, &key) {
            return None;
        }
        let released_parent_ownership = if parent_connection_id.is_none() {
            let parent_consumer = entry.parent_connection_consumer.write().take();
            entry
                .parent_connection_id
                .read()
                .clone()
                .zip(parent_consumer)
        } else {
            None
        };
        *entry.parent_connection_id.write() = parent_connection_id;
        entry.touch();
        if let Some((parent_id, parent_consumer)) = released_parent_ownership {
            self.release(&parent_id, &parent_consumer);
        }
        Some(entry.info())
    }

    pub fn set_parent_connection_ownership(
        &self,
        connection_id: &str,
        parent_connection_id: String,
        parent_consumer: ConnectionConsumer,
    ) -> Option<ConnectionInfo> {
        let key = self
            .by_id
            .get(connection_id)
            .map(|key| key.value().clone())?;
        let entry = self.by_key.get(&key)?.clone();
        let _ownership_transition = entry.ownership_transition.lock();
        if !self.connection_entry_is_registered(connection_id, &key) {
            return None;
        }
        // Parent ownership is linked under the same lifecycle lock used by retirement.
        *entry.parent_connection_id.write() = Some(parent_connection_id);
        *entry.parent_connection_consumer.write() = Some(parent_consumer);
        entry.touch();
        Some(entry.info())
    }

    fn connection_entry_is_registered(&self, connection_id: &str, key: &str) -> bool {
        self.by_id
            .get(connection_id)
            .is_some_and(|registered_key| registered_key.value() == key)
            && self
                .by_key
                .get(key)
                .is_some_and(|registered_entry| registered_entry.connection_id == connection_id)
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
        let _ownership_transition = entry.ownership_transition.lock();
        if !self.connection_entry_is_registered(connection_id, &key) {
            return None;
        }
        entry.cancel_idle_timer();
        let info = entry.info();
        let parent_consumer = entry.parent_connection_consumer.write().take();
        let parent_ownership = entry
            .parent_connection_id
            .read()
            .clone()
            .zip(parent_consumer);
        if entry.connection_id == connection_id {
            self.by_key.remove(&key);
        }
        self.by_id.remove(connection_id);
        if let Some((parent_id, parent_consumer)) = parent_ownership {
            self.release(&parent_id, &parent_consumer);
        }
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
        if handle
            .config()
            .ssh_channel_strategy
            .requires_dedicated_consumers()
        {
            // Some one-channel appliances treat keepalive global requests as
            // an unsupported second operation and close an otherwise healthy link.
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
        if handle
            .config()
            .ssh_channel_strategy
            .requires_dedicated_consumers()
        {
            return ProbeConnectionStatus::NotApplicable;
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
                        handle.entry.set_state(ConnectionState::Active);
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
        entry.set_state(ConnectionState::Idle);
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
            ConnectionConsumer::PortForward(_) | ConnectionConsumer::X11Forward(_) => {
                summary.port_forwards += 1
            }
            // Monitor leases are visible as isolated transports but do not
            // masquerade as terminal, SFTP, IDE, or forwarding ownership.
            ConnectionConsumer::Monitor(_) => {}
            ConnectionConsumer::Ide(_) => summary.ide += 1,
            ConnectionConsumer::NodeRouter(_) => summary.node_router += 1,
            ConnectionConsumer::PublicMcp(_) => summary.public_mcp += 1,
            // Mosh only borrows an isolated SSH transport long enough to
            // start mosh-server, so it is not a persistent topology capability.
            ConnectionConsumer::MoshBootstrap(_) => {}
        }
    }
    summary
}

pub(crate) async fn detect_remote_env_for_handle(handle: &SshConnectionHandle) -> RemoteEnvInfo {
    let env = tokio::time::timeout(
        REMOTE_ENV_TOTAL_TIMEOUT,
        detect_remote_env_inner(handle, None),
    )
    .await
    .unwrap_or_else(|_| RemoteEnvInfo::unknown());
    if !env.os_type.eq_ignore_ascii_case("unknown") {
        return env;
    }
    // Probe through registry-owned channels so gateways can require a PTY
    // without borrowing terminal input or triggering another authentication.
    for with_pty in [false, true] {
        let opened = handle
            .open_sampling_shell_channel("", with_pty, REMOTE_ENV_TOTAL_TIMEOUT)
            .await;
        let Ok(mut shell) = opened else {
            continue;
        };
        let detected = tokio::time::timeout(
            REMOTE_ENV_TOTAL_TIMEOUT,
            detect_remote_env_inner(handle, Some(&mut shell)),
        )
        .await
        .unwrap_or_else(|_| RemoteEnvInfo::unknown());
        // Retain the channel outside the timeout so cancellation still closes it.
        let _ = tokio::time::timeout(REMOTE_ENV_PHASE_A_TIMEOUT, shell.close()).await;
        if !detected.os_type.eq_ignore_ascii_case("unknown") {
            return detected;
        }
    }
    env
}

async fn run_remote_env_probe(
    handle: &SshConnectionHandle,
    shell: &mut Option<&mut crate::SshShellChannel>,
    command: &str,
    timeout: Duration,
) -> Result<String, crate::SshTransportError> {
    match shell.as_deref_mut() {
        Some(shell) => {
            shell
                .sample_until(
                    &format!("{command}\n"),
                    "===END===",
                    timeout,
                    REMOTE_ENV_MAX_OUTPUT_SIZE,
                )
                .await
        }
        None => {
            handle
                .run_command(command, timeout, REMOTE_ENV_MAX_OUTPUT_SIZE)
                .await
        }
    }
}

async fn detect_remote_env_inner(
    handle: &SshConnectionHandle,
    mut shell: Option<&mut crate::SshShellChannel>,
) -> RemoteEnvInfo {
    let phase_a_output = match run_remote_env_probe(
        handle,
        &mut shell,
        REMOTE_ENV_PHASE_A_CMD,
        REMOTE_ENV_PHASE_A_TIMEOUT,
    )
    .await
    {
        Ok(output) => output,
        Err(_) => {
            return run_remote_env_probe(
                handle,
                &mut shell,
                REMOTE_ENV_PHASE_B_WINDOWS_CMD,
                REMOTE_ENV_PHASE_B_TIMEOUT,
            )
            .await
            .map(|output| parse_remote_windows_env(&output))
            .unwrap_or_else(|_| RemoteEnvInfo::unknown());
        }
    };

    let raw_platform = parse_remote_platform(&phase_a_output).unwrap_or_default();
    let is_windows = raw_platform.eq_ignore_ascii_case("windows");
    let phase_b_command = if is_windows {
        REMOTE_ENV_PHASE_B_WINDOWS_CMD
    } else {
        REMOTE_ENV_PHASE_B_UNIX_CMD
    };

    match run_remote_env_probe(
        handle,
        &mut shell,
        phase_b_command,
        REMOTE_ENV_PHASE_B_TIMEOUT,
    )
    .await
    {
        Ok(output) if is_windows => parse_remote_windows_env(&output),
        Ok(output) => parse_remote_unix_env(&output, raw_platform),
        Err(_) => RemoteEnvInfo {
            os_type: if is_windows {
                "Windows".to_string()
            } else {
                classify_remote_unix_os(raw_platform)
            },
            os_version: None,
            kernel: None,
            arch: None,
            shell: None,
            home: None,
            zdotdir: None,
            xdg_config_home: None,
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
    let shell = extract_section_between(output, "===SHELL===", "===HOME===")
        .map(clean_remote_env_value)
        .filter(|value| !value.is_empty());
    let home = extract_section_between(output, "===HOME===", "===ZDOTDIR===")
        .map(clean_remote_env_value)
        .filter(|value| !value.is_empty());
    let zdotdir = extract_section_between(output, "===ZDOTDIR===", "===XDG_CONFIG_HOME===")
        .map(clean_remote_env_value)
        .filter(|value| !value.is_empty());
    let xdg_config_home = extract_section_between(output, "===XDG_CONFIG_HOME===", "===DISTRO===")
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
        home,
        zdotdir,
        xdg_config_home,
        detected_at: remote_env_detected_at(),
    }
}

fn parse_remote_windows_env(output: &str) -> RemoteEnvInfo {
    // POSIX shells can echo the remaining PowerShell probe after its first
    // command fails. Delimiters alone do not establish a Windows platform.
    if extract_section_between(output, "===PLATFORM===", "===ENV===") != Some("Win32NT") {
        return RemoteEnvInfo::unknown();
    }
    RemoteEnvInfo {
        os_type: "Windows".to_string(),
        os_version: extract_section_between(output, "===ENV===", "===ARCH===")
            .map(clean_remote_env_value)
            .filter(|value| !value.is_empty()),
        kernel: None,
        arch: extract_section_between(output, "===ARCH===", "===SHELL===")
            .map(clean_remote_env_value)
            .filter(|value| !value.is_empty()),
        shell: extract_section_between(output, "===SHELL===", "===HOME===")
            .map(clean_remote_env_value)
            .filter(|value| !value.is_empty()),
        home: extract_section_between(output, "===HOME===", "===ZDOTDIR===")
            .map(clean_remote_env_value)
            .filter(|value| !value.is_empty()),
        zdotdir: None,
        xdg_config_home: None,
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

fn parse_remote_platform(output: &str) -> Option<&str> {
    extract_section_between(output, "===DETECT===", "===END===")?
        .split(['\r', '\n'])
        .find_map(|line| line.trim().strip_prefix("PLATFORM="))
        .map(str::trim)
}

fn extract_section_between<'a>(value: &'a str, start: &str, end: &str) -> Option<&'a str> {
    // Gateways can echo probe commands containing every delimiter. Only output
    // lines containing a standalone delimiter establish a result boundary.
    let mut section_start = None;
    let mut offset = 0;
    for line in value.split_inclusive(['\r', '\n']) {
        if let Some(start_index) = section_start {
            if line.trim() == end {
                return Some(value[start_index..offset].trim());
            }
        } else if line.trim() == start {
            section_start = Some(offset + line.len());
        }
        offset += line.len();
    }
    None
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
            ConnectionConsumer::PortForward(_) | ConnectionConsumer::X11Forward(_) => {
                Self::PortForward
            }
            ConnectionConsumer::Monitor(_) => Self::Other,
            ConnectionConsumer::Ide(_)
            | ConnectionConsumer::NodeRouter(_)
            | ConnectionConsumer::PublicMcp(_)
            | ConnectionConsumer::MoshBootstrap(_) => Self::Other,
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

    #[tokio::test]
    async fn shell_only_otp_server_keeps_sampling_on_shared_connection_despite_command_echo() {
        assert_shell_only_sampling(false).await;
    }

    #[tokio::test]
    async fn shell_only_otp_server_requiring_pty_keeps_sampling_on_shared_connection() {
        assert_shell_only_sampling(true).await;
    }

    async fn assert_shell_only_sampling(require_sampling_pty: bool) {
        use oxideterm_connection_monitor::ResourceSampler;
        use russh::{Channel, ChannelId, server};
        struct ShellOnlyPeer {
            channels: HashMap<ChannelId, Channel<server::Msg>>,
            pty_channels: HashSet<ChannelId>,
            initialized_channels: HashSet<ChannelId>,
            readiness_attempts: HashMap<ChannelId, usize>,
            require_sampling_pty: bool,
        }
        struct OtpPrompt(Arc<std::sync::atomic::AtomicUsize>);
        impl crate::SshPromptHandler for OtpPrompt {
            fn keyboard_interactive(
                &self,
                _: crate::KeyboardInteractivePromptRequest,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<
                                crate::KeyboardInteractiveResponses,
                                crate::SshPromptError,
                            >,
                        > + Send
                        + '_,
                >,
            > {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Box::pin(async { Ok(zeroize::Zeroizing::new(vec!["synthetic-otp".into()])) })
            }
        }
        impl server::Handler for ShellOnlyPeer {
            type Error = russh::Error;
            async fn auth_keyboard_interactive<'a>(
                &'a mut self,
                _: &str,
                _: &str,
                response: Option<server::Response<'a>>,
            ) -> Result<server::Auth, Self::Error> {
                Ok(match response {
                    None => server::Auth::Partial {
                        name: "MFA".into(),
                        instructions: "".into(),
                        prompts: vec![("OTP Code:".into(), false)].into(),
                    },
                    Some(mut responses) => {
                        if responses.next().as_deref() == Some(b"synthetic-otp".as_slice()) {
                            server::Auth::Accept
                        } else {
                            server::Auth::reject()
                        }
                    }
                })
            }
            async fn channel_open_session(
                &mut self,
                channel: Channel<server::Msg>,
                reply: server::ChannelOpenHandle,
                _: &mut server::Session,
            ) -> Result<(), Self::Error> {
                self.channels.insert(channel.id(), channel);
                reply.accept().await;
                Ok(())
            }
            async fn pty_request(
                &mut self,
                channel: ChannelId,
                _: &str,
                _: u32,
                _: u32,
                _: u32,
                _: u32,
                _: &[(russh::Pty, u32)],
                session: &mut server::Session,
            ) -> Result<(), Self::Error> {
                self.pty_channels.insert(channel);
                session.channel_success(channel)
            }
            async fn exec_request(
                &mut self,
                channel: ChannelId,
                _: &[u8],
                session: &mut server::Session,
            ) -> Result<(), Self::Error> {
                session.channel_failure(channel)?;
                session.close(channel)
            }
            async fn shell_request(
                &mut self,
                channel: ChannelId,
                session: &mut server::Session,
            ) -> Result<(), Self::Error> {
                session.channel_success(channel)
            }
            async fn data(
                &mut self,
                channel: ChannelId,
                data: &[u8],
                session: &mut server::Session,
            ) -> Result<(), Self::Error> {
                let command = std::str::from_utf8(data).unwrap();
                if self.require_sampling_pty
                    && self.pty_channels.contains(&channel)
                    && !data.ends_with(b"\r")
                    && !data.ends_with(b"\r\n")
                {
                    // Terminal gateways can require the same Enter byte as interactive input.
                    return Ok(());
                }
                if self.require_sampling_pty
                    && (!self.pty_channels.contains(&channel)
                        || !self.initialized_channels.contains(&channel))
                    && (command.contains("sample-ok")
                        || command.contains("===STAT===")
                        || command.contains("===DETECT===")
                        || command.contains("===ENV==="))
                {
                    return session.close(channel);
                }
                let output = if command.contains("__OXIDE_MONITOR_READY__") {
                    assert!(
                        command.contains("stty -echo -icanon")
                            || command.starts_with("echo __OXIDE_MONITOR_READY__")
                    );
                    let attempts = self.readiness_attempts.entry(channel).or_default();
                    *attempts += 1;
                    if *attempts == 1 {
                        // Gateways can discard early input while target login is still running.
                        "Connecting to target...\r\n"
                    } else {
                        self.initialized_channels.insert(channel);
                        "\r\n__OXIDE_MONITOR_READY__\r\n"
                    }
                } else if command.contains("===DETECT===") {
                    session.data(channel, data.to_vec())?;
                    "===DETECT===\nPLATFORM=Linux\n===END===\n"
                } else if command.contains("===ENV===") {
                    session.data(channel, data.to_vec())?;
                    "===ENV===\nLinux\n===ARCH===\nx86_64\n===KERNEL===\n6.8\n===SHELL===\n/bin/bash\n===HOME===\n/home/fixture\n===ZDOTDIR===\n===XDG_CONFIG_HOME===\n===DISTRO===\nPRETTY_NAME=Ubuntu\n===END===\n"
                } else if command.contains("sample-ok") {
                    // A gateway can echo the command in its own SSH data message before executing it.
                    session.data(channel, data.to_vec())?;
                    "sample-ok\n===END===\n"
                } else if command.contains("===STAT===") {
                    session.data(channel, data.to_vec())?;
                    "===MEMINFO===\nMemTotal: 1024 kB\nMemAvailable: 512 kB\n===END===\n"
                } else {
                    ""
                };
                // End markers can span SSH messages independently of command echo.
                let split = output.len().saturating_sub(3);
                session.data(channel, output.as_bytes()[..split].to_vec())?;
                session.data(channel, output.as_bytes()[split..].to_vec())
            }
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let key =
            russh::keys::PrivateKey::random(&mut rand10::rng(), russh::keys::Algorithm::Ed25519)
                .unwrap();
        let fingerprint = key
            .public_key()
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let session = server::run_stream(
                Arc::new(server::Config {
                    keys: vec![key],
                    ..Default::default()
                }),
                stream,
                ShellOnlyPeer {
                    channels: HashMap::new(),
                    pty_channels: HashSet::new(),
                    initialized_channels: HashSet::new(),
                    readiness_attempts: HashMap::new(),
                    require_sampling_pty,
                },
            )
            .await
            .unwrap();
            let _ = session.await;
        });
        let mut config = SshConfig::password("127.0.0.1", port, "fixture", "fixture-password");
        config.auth = crate::AuthMethod::KeyboardInteractive;
        config.trust_host_key = Some(false);
        config.expected_host_key_fingerprint = Some(fingerprint);
        let registry = SshConnectionRegistry::default();
        let prompts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handle = crate::SshTransportClient::new(config)
            .with_prompt_handler(Arc::new(OtpPrompt(prompts.clone())))
            .connect_node_with_registry(
                registry.clone(),
                ConnectionConsumer::NodeRouter("fixture".into()),
            )
            .await
            .unwrap();
        let env = detect_remote_env_for_handle(&handle).await;
        assert_eq!(env.os_type, "Linux");
        assert_eq!(env.arch.as_deref(), Some("x86_64"));
        assert_eq!(env.shell.as_deref(), Some("/bin/bash"));
        assert_eq!(env.home.as_deref(), Some("/home/fixture"));
        let mut sample = handle.open_shell("", Duration::from_secs(2)).await.unwrap();
        assert_eq!(
            sample
                .sample_until(
                    "echo sample-ok; echo '===END==='\n",
                    "===END===",
                    Duration::from_secs(2),
                    1024
                )
                .await
                .unwrap(),
            "echo sample-ok; echo '===END==='\nsample-ok\n===END===\n"
        );
        sample.close().await.unwrap();
        let profiler = oxideterm_connection_monitor::ProfilerRegistry::new();
        let (updates, mut updated) = tokio::sync::mpsc::channel(1);
        profiler.start_with_sampler_on_config(
            "fixture",
            Arc::new(handle.clone()),
            "Linux",
            oxideterm_connection_monitor::ResourceSamplingConfig {
                system: true,
                gpu: false,
                processes: false,
                docker: false,
            },
            Some(updates),
            tokio::runtime::Handle::current(),
        );
        tokio::time::timeout(Duration::from_secs(5), updated.recv())
            .await
            .unwrap()
            .unwrap();
        let metrics = profiler.latest("fixture").unwrap();
        assert_eq!(metrics.memory_total, Some(1_048_576));
        assert_eq!(metrics.memory_used, Some(524_288));
        assert_eq!(profiler.failure("fixture"), None);
        assert_eq!(prompts.load(std::sync::atomic::Ordering::SeqCst), 1);
        profiler.stop("fixture");
        assert_eq!(
            handle.info().consumers,
            vec![ConnectionConsumer::NodeRouter("fixture".into())]
        );
        handle.clear_physical().await;
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn audit_follows_shared_connection_after_terminal_release_and_new_connection() {
        use oxideterm_audit::*;
        struct Keys;
        impl AuditKeyProvider for Keys {
            fn load(&self, _: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, AuditError> {
                Ok(zeroize::Zeroizing::new(vec![9; 32]))
            }
            fn create(&self, id: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, AuditError> {
                self.load(id)
            }
        }
        let directory = tempfile::tempdir().unwrap();
        oxideterm_audit::AuditStore::open(&directory.path().join("audit.db"), &Keys)
            .unwrap()
            .set_policy(oxideterm_audit::AuditPolicy {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let service =
            AuditService::with_key_provider(directory.path().join("audit.db"), Keys).unwrap();
        let registry = SshConnectionRegistry::with_audit(
            ConnectionPoolConfig::default(),
            Some(AuditContext::new(service.client(), AuditSource::User)),
        );
        let config = SshConfig::password("old.example", 22, "alice", "secret-not-for-audit");
        let terminal_consumer = ConnectionConsumer::Terminal("term".into());
        let terminal = registry.acquire(config.clone(), terminal_consumer.clone());
        registry.acquire(config.clone(), terminal_consumer.clone());
        let sftp = registry.acquire(config, ConnectionConsumer::Sftp("files".into()));
        let terminal_context = terminal.audit_context().unwrap().consumer();
        let sftp_context = sftp.audit_context().unwrap().consumer();
        assert_eq!(terminal_context.session_id, sftp_context.session_id);
        assert_ne!(terminal_context.consumer_id, sftp_context.consumer_id);
        let upload =
            sftp_context.operation(AuditCategory::File, "file_upload", Some("/tmp/report"));
        registry.release(terminal.connection_id(), &terminal_consumer);
        registry.release(sftp.connection_id(), &terminal_consumer);
        drop(terminal);
        assert_eq!(sftp.info().ref_count, 1);
        let next = registry.acquire(
            SshConfig::password("new.example", 2222, "bob", "other-secret"),
            ConnectionConsumer::Terminal("next".into()),
        );
        let next_context = next.audit_context().unwrap().consumer();
        assert_ne!(sftp_context.session_id, next_context.session_id);
        next_context
            .operation(AuditCategory::Command, "command_execute", Some("pwd"))
            .finish(
                AuditOutcome::Succeeded,
                AuditEvidence::Protocol,
                Some(0),
                None,
            );
        upload.finish(
            AuditOutcome::Succeeded,
            AuditEvidence::Protocol,
            None,
            Some(42),
        );
        registry.mark_state(sftp.connection_id(), ConnectionState::Active);
        registry.mark_state(sftp.connection_id(), ConnectionState::Active);
        registry.mark_state(sftp.connection_id(), ConnectionState::LinkDown);
        registry.mark_state(
            sftp.connection_id(),
            ConnectionState::Error("sensitive-state-error".into()),
        );
        let page = service
            .client()
            .query(AuditQuery {
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        let rows = page
            .records
            .iter()
            .filter(|record| {
                matches!(
                    record.category,
                    AuditCategory::File | AuditCategory::Command
                )
            })
            .map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                (
                    record.details.target.as_ref().unwrap().as_str(),
                    record.details.connection_id.as_ref().unwrap().as_str(),
                    op.session_id.as_deref(),
                    op.consumer_id.as_deref(),
                    op.outcome,
                    op.bytes,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            [
                (
                    "alice@old.example:22",
                    sftp.connection_id(),
                    sftp_context.session_id.as_deref(),
                    sftp_context.consumer_id.as_deref(),
                    AuditOutcome::Succeeded,
                    Some(42)
                ),
                (
                    "bob@new.example:2222",
                    next.connection_id(),
                    next_context.session_id.as_deref(),
                    next_context.consumer_id.as_deref(),
                    AuditOutcome::Succeeded,
                    None
                ),
                (
                    "bob@new.example:2222",
                    next.connection_id(),
                    next_context.session_id.as_deref(),
                    next_context.consumer_id.as_deref(),
                    AuditOutcome::Started,
                    None
                ),
                (
                    "alice@old.example:22",
                    sftp.connection_id(),
                    sftp_context.session_id.as_deref(),
                    sftp_context.consumer_id.as_deref(),
                    AuditOutcome::Started,
                    None
                ),
            ]
        );
        let consumers = page
            .records
            .iter()
            .rev()
            .filter_map(|record| {
                let op = record.details.operation.as_ref().unwrap();
                op.action.starts_with("ssh_consumer_").then(|| {
                    (
                        op.action.as_str(),
                        serde_json::from_str::<ConnectionConsumer>(
                            record.details.detail.as_ref().unwrap(),
                        )
                        .unwrap(),
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            consumers,
            [
                (
                    "ssh_consumer_attached",
                    ConnectionConsumer::Terminal("term".into())
                ),
                (
                    "ssh_consumer_attached",
                    ConnectionConsumer::Sftp("files".into())
                ),
                (
                    "ssh_consumer_detached",
                    ConnectionConsumer::Terminal("term".into())
                ),
                (
                    "ssh_consumer_attached",
                    ConnectionConsumer::Terminal("next".into())
                ),
            ]
        );
        let states = page
            .records
            .iter()
            .rev()
            .filter(|record| {
                record.details.operation.as_ref().unwrap().action == "ssh_connection_state"
            })
            .map(|record| {
                serde_json::from_str::<serde_json::Value>(record.details.detail.as_ref().unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            [
                serde_json::json!({"from":"connecting","to":"active"}),
                serde_json::json!({"from":"active","to":"link_down"}),
                serde_json::json!({"from":"link_down","to":"error"})
            ]
        );
        assert!(
            !serde_json::to_string(&page.records)
                .unwrap()
                .contains("sensitive-state-error")
        );
        let debug = format!("{sftp_context:?}");
        assert!(
            !debug.contains("alice")
                && !debug.contains("old.example")
                && !debug.contains("secret-not-for-audit")
        );
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
            home: Some("/home/me".to_string()),
            zdotdir: None,
            xdg_config_home: None,
            detected_at: 1,
        };
        let second = RemoteEnvInfo {
            os_type: "macOS".to_string(),
            os_version: Some("14".to_string()),
            kernel: None,
            arch: None,
            shell: Some("/bin/zsh".to_string()),
            home: Some("/Users/me".to_string()),
            zdotdir: None,
            xdg_config_home: None,
            detected_at: 2,
        };

        assert!(handle.set_remote_env(first.clone()));
        assert!(!handle.set_remote_env(second));

        assert_eq!(handle.remote_env(), Some(first.clone()));
        assert_eq!(handle.info().remote_env, Some(first));
    }

    #[test]
    fn unknown_remote_env_is_not_cached_and_detection_can_retry() {
        let registry = SshConnectionRegistry::default();
        let handle = registry.acquire(
            SshConfig::password("host", 22, "me", "pw"),
            ConnectionConsumer::Terminal("a".into()),
        );

        assert!(handle.entry.try_begin_remote_env_detection());
        assert!(handle.remote_env_detection_running());
        assert!(!handle.set_remote_env(RemoteEnvInfo::unknown()));
        assert_eq!(handle.remote_env(), None);
        assert!(!handle.remote_env_detection_running());
        assert!(handle.entry.try_begin_remote_env_detection());
    }

    #[test]
    fn visible_terminal_ready_is_recorded_once() {
        let registry = SshConnectionRegistry::default();
        let handle = registry.acquire(
            SshConfig::password("host", 22, "me", "pw"),
            ConnectionConsumer::Terminal("a".into()),
        );

        assert_eq!(
            registry.mark_visible_terminal_ready(handle.connection_id()),
            Some(true)
        );
        assert_eq!(
            registry.mark_visible_terminal_ready(handle.connection_id()),
            Some(false)
        );
        assert_eq!(registry.mark_visible_terminal_ready("missing"), None);
    }

    #[test]
    fn remote_platform_parser_ignores_echoed_probe_branches() {
        for (output, expected) in [
            (
                format!(
                    "{REMOTE_ENV_PHASE_A_CMD}\r\n===DETECT===\r\nPLATFORM=Linux\r\n===END===\r\n"
                ),
                Some("Linux"),
            ),
            (
                format!("{REMOTE_ENV_PHASE_A_CMD}\n===DETECT===\nPLATFORM=windows\n===END===\n"),
                Some("windows"),
            ),
            (format!("{REMOTE_ENV_PHASE_A_CMD}\n"), None),
            ("===DETECT===\nPLATFORM=Linux\n".to_owned(), None),
        ] {
            assert_eq!(parse_remote_platform(&output), expected);
        }
    }

    #[test]
    fn windows_environment_probe_requires_windows_platform_evidence() {
        let unix_output = format!(
            "{REMOTE_ENV_PHASE_B_WINDOWS_CMD}\n===PLATFORM===\n===ENV===\n===ARCH===\n:PROCESSOR_ARCHITECTURE\n===SHELL===\nPowerShell\n===HOME===\n/home/fixture\n===ZDOTDIR===\n===XDG_CONFIG_HOME===\n===END===\n"
        );
        assert_eq!(parse_remote_windows_env(&unix_output).os_type, "Unknown");
        let windows_output = format!(
            "{REMOTE_ENV_PHASE_B_WINDOWS_CMD}\r\n===PLATFORM===\r\nWin32NT\r\n===ENV===\r\nMicrosoft Windows NT 10.0.22631.0\r\n===ARCH===\r\nAMD64\r\n===SHELL===\r\nPowerShell 7.5.0\r\n===HOME===\r\nC:\\Users\\fixture\r\n===ZDOTDIR===\r\n===XDG_CONFIG_HOME===\r\n===END===\r\n"
        );
        let windows = parse_remote_windows_env(&windows_output);
        assert_eq!(windows.os_type, "Windows");
        assert_eq!(windows.arch.as_deref(), Some("AMD64"));
        assert_eq!(windows.shell.as_deref(), Some("PowerShell 7.5.0"));
    }

    #[test]
    fn remote_env_parser_preserves_shell_configuration_directories() {
        let output = "===ENV===\nLinux\n===ARCH===\nx86_64\n===KERNEL===\n6.8\n===SHELL===\n/bin/zsh\n===HOME===\n/home/alice\n===ZDOTDIR===\n/home/alice/.config/zsh\n===XDG_CONFIG_HOME===\n/home/alice/.config\n===DISTRO===\nPRETTY_NAME=Ubuntu\nID=ubuntu\n===END===\n";
        let parsed = parse_remote_unix_env(output, "Linux");

        assert_eq!(parsed.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(parsed.home.as_deref(), Some("/home/alice"));
        assert_eq!(parsed.zdotdir.as_deref(), Some("/home/alice/.config/zsh"));
        assert_eq!(
            parsed.xdg_config_home.as_deref(),
            Some("/home/alice/.config")
        );
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "requires Fish installed"]
    fn remote_env_probe_runs_under_fish() {
        let run = |command: &str| {
            let output = std::process::Command::new("fish")
                .args(["--no-config", "-c", command])
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                .env("SHELL", "/usr/bin/fish")
                .env("HOME", "/home/fish-test")
                .env("XDG_CONFIG_HOME", "/home/fish-test/config")
                .output()
                .expect("run Fish environment probe");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        };
        let platform_output = run(REMOTE_ENV_PHASE_A_CMD);
        let platform = parse_remote_platform(&platform_output).unwrap();
        assert!(matches!(
            platform,
            "Linux" | "Darwin" | "FreeBSD" | "OpenBSD" | "NetBSD"
        ));
        let env = parse_remote_unix_env(&run(REMOTE_ENV_PHASE_B_UNIX_CMD), platform);
        assert_eq!(env.shell.as_deref(), Some("/usr/bin/fish"));
        assert_eq!(env.home.as_deref(), Some("/home/fish-test"));
        assert_eq!(
            env.xdg_config_home.as_deref(),
            Some("/home/fish-test/config")
        );
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
        assert_eq!(handle.state(), ConnectionState::Idle);
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

    #[test]
    fn acquire_is_idempotent_for_the_same_consumer() {
        let registry = SshConnectionRegistry::default();
        let config = SshConfig::password("idempotent.example", 22, "alice", "pw");
        let consumer = ConnectionConsumer::Terminal("term-1".into());

        let first = registry.acquire(config.clone(), consumer.clone());
        let second = registry.acquire(config, consumer.clone());

        assert_eq!(first.connection_id(), second.connection_id());
        assert_eq!(first.info().ref_count, 1);
        assert_eq!(first.info().consumers, vec![consumer.clone()]);

        registry.release(first.connection_id(), &consumer);
        assert_eq!(first.info().ref_count, 0);
    }

    #[test]
    fn acquire_dedicated_isolates_a_terminal_from_the_shared_pool_entry() {
        let registry = SshConnectionRegistry::default();
        let config = SshConfig::password("dedicated.example", 22, "alice", "pw");
        let node_consumer = ConnectionConsumer::NodeRouter("node-1".into());
        let terminal_consumer = ConnectionConsumer::Terminal("term-1".into());

        let shared = registry.acquire(config.clone(), node_consumer.clone());
        let dedicated = registry.acquire_dedicated(config.clone(), terminal_consumer.clone());
        let pooled_again = registry.acquire(config, node_consumer);

        assert_ne!(shared.connection_id(), dedicated.connection_id());
        assert_eq!(shared.connection_id(), pooled_again.connection_id());
        assert_eq!(dedicated.info().consumers, vec![terminal_consumer.clone()]);
        assert!(dedicated.key().contains("|dedicated="));
        registry.release(dedicated.connection_id(), &terminal_consumer);
        assert!(registry.get(dedicated.connection_id()).is_none());
        assert_eq!(
            registry
                .get(shared.connection_id())
                .unwrap()
                .info()
                .consumers,
            vec![ConnectionConsumer::NodeRouter("node-1".into())]
        );
    }

    #[test]
    fn dedicated_lease_retires_connection_on_drop() {
        let registry = SshConnectionRegistry::default();
        let consumer = ConnectionConsumer::Sftp("node-1:browse".into());
        let handle = registry.acquire_dedicated(
            SshConfig::password("dedicated.example", 22, "alice", "pw"),
            consumer.clone(),
        );
        let connection_id = handle.connection_id().to_string();
        let lease = DedicatedConnectionLease::new(registry.clone(), handle, consumer);

        drop(lease);

        assert!(registry.get(&connection_id).is_none());
    }

    #[test]
    fn dedicated_child_retirement_releases_parent_ownership() {
        let registry = SshConnectionRegistry::default();
        let parent_owner = ConnectionConsumer::NodeRouter("parent".into());
        let parent = registry.acquire(
            SshConfig::password("jump.example", 22, "alice", "pw"),
            parent_owner.clone(),
        );
        let terminal_consumer = ConnectionConsumer::Terminal("term-1".into());
        let child = registry.acquire_dedicated(
            SshConfig::password("target.example", 22, "alice", "pw"),
            terminal_consumer.clone(),
        );
        let ancestor_consumer =
            ConnectionConsumer::NodeRouter(format!("{}:ancestor", child.connection_id()));
        registry
            .acquire_consumer_for_connection(parent.connection_id(), ancestor_consumer.clone())
            .expect("parent connection");
        registry
            .set_parent_connection_ownership(
                child.connection_id(),
                parent.connection_id().to_string(),
                ancestor_consumer,
            )
            .expect("dedicated child ownership");

        registry.release(child.connection_id(), &terminal_consumer);

        assert!(registry.get(child.connection_id()).is_none());
        assert_eq!(parent.info().consumers, vec![parent_owner]);
        assert_eq!(parent.info().ref_count, 1);
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
    async fn single_channel_connections_skip_global_keepalive_probes() {
        let registry = SshConnectionRegistry::default();
        let consumer = ConnectionConsumer::NodeRouter("single-channel".into());
        let handle = registry.acquire(
            SshConfig {
                ssh_channel_strategy:
                    oxideterm_connections::SshChannelStrategy::DedicatedPerConsumer,
                ..SshConfig::default()
            },
            consumer,
        );
        registry.mark_state(handle.connection_id(), ConnectionState::Active);

        let status = registry
            .probe_single_connection(handle.connection_id(), Duration::from_millis(1))
            .await;

        assert_eq!(status, ProbeConnectionStatus::NotApplicable);
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
        assert_eq!(first.connection_id(), second.connection_id());
        assert_eq!(first.info().ref_count, 2);
        assert_eq!(first.state(), ConnectionState::Connecting);
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
