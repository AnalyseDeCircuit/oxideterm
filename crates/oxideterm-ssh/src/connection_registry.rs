// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use uuid::Uuid;

use crate::SshConfig;

pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
pub const HEARTBEAT_FAIL_THRESHOLD: u8 = 2;
pub const WS_BRIDGE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
pub const WS_BRIDGE_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(300);

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
    pub state: ConnectionState,
    pub ref_count: u64,
    pub consumers: Vec<ConnectionConsumer>,
    pub created_at: SystemTime,
    pub last_active_at: SystemTime,
    pub idle_timeout_secs: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeConnectionStatus {
    Alive,
    Dead,
    NotFound,
    NotApplicable,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpSessionState {
    pub ready: bool,
    pub cwd: Option<String>,
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
    state: RwLock<ConnectionState>,
    ref_count: AtomicU64,
    consumers: RwLock<Vec<ConnectionConsumer>>,
    physical: RwLock<Option<Arc<dyn Any + Send + Sync>>>,
    sftp_state: RwLock<SftpSessionState>,
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
            state: RwLock::new(ConnectionState::Connecting),
            ref_count: AtomicU64::new(0),
            consumers: RwLock::new(Vec::new()),
            physical: RwLock::new(None),
            sftp_state: RwLock::new(SftpSessionState::default()),
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
            state: self.state.read().clone(),
            ref_count: self.ref_count.load(Ordering::SeqCst),
            consumers: self.consumers.read().clone(),
            created_at: self.created_at,
            last_active_at: *self.last_active_at.read(),
            idle_timeout_secs: self.idle_timeout.map(|duration| duration.as_secs()),
        }
    }

    fn touch(&self) {
        *self.last_active_at.write() = SystemTime::now();
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

    pub fn state(&self) -> ConnectionState {
        self.entry.state.read().clone()
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

    pub fn clear_physical(&self) {
        *self.entry.physical.write() = None;
        self.entry.touch();
    }
}

#[derive(Clone, Debug)]
pub struct SshConnectionRegistry {
    config: ConnectionPoolConfig,
    by_key: Arc<DashMap<String, Arc<ConnectionEntry>>>,
    by_id: Arc<DashMap<String, String>>,
}

impl SshConnectionRegistry {
    pub fn new(config: ConnectionPoolConfig) -> Self {
        Self {
            config,
            by_key: Arc::new(DashMap::new()),
            by_id: Arc::new(DashMap::new()),
        }
    }

    pub fn acquire(&self, config: SshConfig, consumer: ConnectionConsumer) -> SshConnectionHandle {
        let key = config.connection_key();
        let entry = self
            .by_key
            .entry(key.clone())
            .or_insert_with(|| {
                let entry = Arc::new(ConnectionEntry::new(config, self.config));
                self.by_id.insert(entry.connection_id.clone(), key);
                entry
            })
            .clone();

        entry.ref_count.fetch_add(1, Ordering::SeqCst);
        entry.touch();
        {
            let mut consumers = entry.consumers.write();
            if !consumers.contains(&consumer) {
                consumers.push(consumer);
            }
        }
        *entry.state.write() = ConnectionState::Active;
        SshConnectionHandle { entry }
    }

    pub fn release(&self, connection_id: &str, consumer: &ConnectionConsumer) {
        let Some(key) = self.by_id.get(connection_id).map(|key| key.value().clone()) else {
            return;
        };
        let Some(entry) = self.by_key.get(&key).map(|entry| entry.clone()) else {
            return;
        };

        entry
            .consumers
            .write()
            .retain(|existing| existing != consumer);
        entry
            .ref_count
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                Some(count.saturating_sub(1))
            })
            .ok();
        entry.touch();
        if entry.ref_count.load(Ordering::SeqCst) == 0 {
            *entry.state.write() = ConnectionState::Idle;
        }
    }

    pub fn mark_state(
        &self,
        connection_id: &str,
        state: ConnectionState,
    ) -> Option<ConnectionInfo> {
        let key = self
            .by_id
            .get(connection_id)
            .map(|key| key.value().clone())?;
        let entry = self.by_key.get(&key)?.clone();
        *entry.state.write() = state;
        entry.touch();
        Some(entry.info())
    }

    pub fn mark_link_down_cascade(&self, root_connection_id: &str) -> Vec<ConnectionInfo> {
        let Some(root_key) = self
            .by_id
            .get(root_connection_id)
            .map(|key| key.value().clone())
        else {
            return Vec::new();
        };

        let root_host = self
            .by_key
            .get(&root_key)
            .map(|entry| entry.config.host.clone())
            .unwrap_or_default();
        let mut changed = Vec::new();
        for entry in self.by_key.iter() {
            let affects_entry = entry.key().as_str() == root_key.as_str()
                || entry
                    .config
                    .proxy_chain
                    .as_ref()
                    .is_some_and(|chain| chain.iter().any(|hop| hop.host == root_host));
            if affects_entry {
                *entry.state.write() = ConnectionState::LinkDown;
                entry.touch();
                changed.push(entry.info());
            }
        }
        changed
    }

    pub async fn probe_single_connection(
        &self,
        connection_id: &str,
        timeout: Duration,
    ) -> ProbeConnectionStatus {
        let Some(handle) = self.get(connection_id) else {
            return ProbeConnectionStatus::NotFound;
        };
        match handle.state() {
            ConnectionState::Active | ConnectionState::Idle | ConnectionState::LinkDown => {}
            ConnectionState::Connecting
            | ConnectionState::Reconnecting
            | ConnectionState::Disconnecting
            | ConnectionState::Disconnected
            | ConnectionState::Error(_) => return ProbeConnectionStatus::NotApplicable,
        }

        let mut alive = handle.probe_alive(timeout).await.is_ok();
        if !alive {
            sleep(Duration::from_millis(250)).await;
            alive = handle.probe_alive(timeout).await.is_ok();
        }

        if alive {
            let _ = self.mark_state(connection_id, ConnectionState::Active);
            ProbeConnectionStatus::Alive
        } else {
            let _ = self.mark_state(connection_id, ConnectionState::LinkDown);
            ProbeConnectionStatus::Dead
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
                handle.entry.ref_count.fetch_add(1, Ordering::SeqCst);
            }
        }
        *handle.entry.state.write() = ConnectionState::Active;
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
}

impl Default for SshConnectionRegistry {
    fn default() -> Self {
        Self::new(ConnectionPoolConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_one_connection_for_many_consumers() {
        let registry = SshConnectionRegistry::default();
        let config = SshConfig::password("host", 22, "me", "pw");
        let first = registry.acquire(config.clone(), ConnectionConsumer::Terminal("a".into()));
        let second = registry.acquire(config, ConnectionConsumer::Sftp("b".into()));

        assert_eq!(first.connection_id(), second.connection_id());
        assert_eq!(first.info().ref_count, 2);
        assert_eq!(registry.stats().active, 1);
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
}
