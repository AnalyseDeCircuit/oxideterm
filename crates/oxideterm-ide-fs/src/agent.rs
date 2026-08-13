// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Node-first IDE agent proxy.
//!
//! This ports Tauri's `agentService`/`node_agent_*` boundary into the native
//! file-system layer: the IDE asks for files and directories, this adapter uses
//! a remote OxideTerm agent when one is ready, and falls back to SFTP for the
//! operations that Tauri also treats as SFTP-compatible.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use base64::Engine;
use dashmap::DashMap;
use oxideterm_backend_classification::{BackendErrorClass, classify_message};
use oxideterm_ide_core::{
    AsyncIdeFileSystem, FileKind, FileStat, FileSystemCapabilities, FileTreeEntry, IdeFileData,
    IdeFileError, IdeFileErrorKind, IdeFsFuture, IdeLocation, IdePathStat, IdeProjectInfo,
    IdeSearchQuery, IdeWatchEvent, IdeWatchKey, SavedFileVersion, WriteMode,
};
use oxideterm_sftp::{SftpError, SftpExecChannelOpener};
use oxideterm_ssh::{
    ConnectionConsumer, NodeId, NodeRouter, ResolvedConnection, RouteError, SshConnectionHandle,
};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{Mutex, broadcast, mpsc, oneshot, watch},
    task::JoinHandle,
};
use tracing::{debug, info, warn};

use crate::NodeSftpIdeFileSystem;

const AGENT_REMOTE_DIR: &str = ".oxideterm";
const AGENT_BINARY_NAME: &str = "oxideterm-agent";
const AGENT_REMOTE_PATH: &str = "~/.oxideterm/oxideterm-agent";
const AGENT_RPC_TIMEOUT_SECS: u64 = 30;
const AGENT_COMPRESS_THRESHOLD: usize = 32 * 1024;
const LEGACY_AGENT_COMPATIBILITY_VERSION: u32 = 1;
const CURRENT_AGENT_COMPATIBILITY_VERSION: u32 = 3;
const INVALID_AGENT_COMPATIBILITY_VERSION: u32 = 0;

static NEXT_AGENT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_IDE_OWNER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_IDE_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NodeAgentMode {
    #[default]
    Ask,
    Enabled,
    Disabled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentStatus {
    NotDeployed,
    Deploying,
    Ready {
        version: String,
        arch: String,
        pid: u32,
    },
    Failed {
        reason: String,
    },
    UnsupportedArch {
        arch: String,
    },
    ManualUploadRequired {
        arch: String,
        remote_path: String,
    },
    ManualUpdateRequired {
        arch: String,
        remote_path: String,
        current_agent_version: String,
        current_compatibility_version: u32,
        expected_compatibility_version: u32,
    },
    SftpFallback,
}

impl AgentStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

#[derive(Clone)]
pub struct NodeAgentIdeFileSystem {
    router: NodeRouter,
    sftp: NodeSftpIdeFileSystem,
    registry: Arc<AgentRegistry>,
    // Each UI or AI owner keeps a node-scoped lease that can outlive terminal
    // panes. Owners share the NodeRouter transport but release only their own
    // consumer instead of relying on GPUI panes to remember low-level paths.
    ide_sessions: Arc<DashMap<IdeSessionKey, Arc<IdeRemoteSessionInner>>>,
    owner_id: u64,
    mode: NodeAgentMode,
    // Tauri computes node_agent_status by resolving node_id to the current SSH
    // connection id, then querying AgentRegistry by that connection. Keep the
    // same shape here so one node's agent result cannot overwrite another's.
    agent_statuses: Arc<DashMap<AgentStatusKey, AgentStatus>>,
    latest_agent_status: Arc<DashMap<String, AgentStatusKey>>,
    watch_subscriptions: Arc<DashMap<IdeOwnedWatchKey, Arc<IdeWatchShared>>>,
    watch_lifecycle_locks: Arc<DashMap<IdeWatchKey, Arc<Mutex<()>>>>,
    deploy_lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IdeConnectionLease {
    connection_id: String,
    consumer: ConnectionConsumer,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IdeSessionKey {
    owner_id: u64,
    node_id: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct IdeOwnedWatchKey {
    owner_id: u64,
    watch: IdeWatchKey,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AgentStatusKey {
    node_id: String,
    connection_id: String,
}

struct IdeWatchShared {
    connection_id: String,
    events_tx: broadcast::Sender<IdeWatchEvent>,
    shutdown_tx: watch::Sender<bool>,
    dispatcher_task: StdMutex<Option<JoinHandle<()>>>,
}

impl IdeWatchShared {
    fn new(connection_id: String) -> Self {
        let (events_tx, _) = broadcast::channel::<IdeWatchEvent>(1024);
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            connection_id,
            events_tx,
            shutdown_tx,
            dispatcher_task: StdMutex::new(None),
        }
    }

    fn start_dispatcher(
        &self,
        key: IdeWatchKey,
        mut events_rx: broadcast::Receiver<AgentWatchEvent>,
    ) {
        let events_tx = self.events_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let dispatcher_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    shutdown = shutdown_rx.changed() => {
                        if shutdown.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    event = events_rx.recv() => {
                        let Ok(event) = event else {
                            break;
                        };
                        let event_path = normalize_agent_watch_path(&event.path);
                        if event_path != key.path
                            && !event_path.starts_with(&format!(
                                "{}/",
                                key.path.trim_end_matches('/')
                            ))
                        {
                            continue;
                        }
                        let _ = events_tx.send(IdeWatchEvent {
                            path: event.path,
                            kind: event.kind,
                        });
                    }
                }
            }
        });
        *self
            .dispatcher_task
            .lock()
            .expect("IDE watch dispatcher task poisoned") = Some(dispatcher_task);
    }

    async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
        let dispatcher_task = self
            .dispatcher_task
            .lock()
            .expect("IDE watch dispatcher task poisoned")
            .take();
        if let Some(mut dispatcher_task) = dispatcher_task
            && tokio::time::timeout(Duration::from_secs(1), &mut dispatcher_task)
                .await
                .is_err()
        {
            // The dispatcher normally exits immediately through the shutdown
            // receiver. Abort is the bounded fallback during runtime teardown.
            dispatcher_task.abort();
            let _ = dispatcher_task.await;
        }
    }

    fn cancel_now(&self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(dispatcher_task) = self
            .dispatcher_task
            .lock()
            .expect("IDE watch dispatcher task poisoned")
            .take()
        {
            dispatcher_task.abort();
        }
    }
}

impl Drop for IdeWatchShared {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(dispatcher_task) = self
            .dispatcher_task
            .get_mut()
            .expect("IDE watch dispatcher task poisoned")
            .take()
        {
            dispatcher_task.abort();
        }
    }
}

pub struct IdeWatchSubscription {
    rx: broadcast::Receiver<IdeWatchEvent>,
}

impl IdeWatchSubscription {
    pub async fn recv(&mut self) -> Option<IdeWatchEvent> {
        loop {
            match self.rx.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

struct IdeRemoteSessionInner {
    node_id: NodeId,
    router: NodeRouter,
    // Session-unique identity prevents a stale async release from removing a
    // replacement session's consumer for the same logical node.
    consumer: ConnectionConsumer,
    state: StdMutex<IdeRemoteSessionState>,
}

#[derive(Default)]
struct IdeRemoteSessionState {
    lease: Option<IdeConnectionLease>,
    closed: bool,
}

impl IdeRemoteSessionInner {
    fn new(node_id: NodeId, router: NodeRouter) -> Self {
        let session_id = NEXT_IDE_SESSION_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            consumer: ConnectionConsumer::Ide(format!("{}:{session_id}", node_id.0)),
            node_id,
            router,
            state: StdMutex::new(IdeRemoteSessionState::default()),
        }
    }

    async fn acquire_connection(&self) -> Result<ResolvedConnection, RouteError> {
        if self
            .state
            .lock()
            .expect("IDE remote session state poisoned")
            .closed
        {
            return Err(RouteError::NotConnected(self.node_id.0.clone()));
        }
        let consumer = self.consumer.clone();
        let resolved = self
            .router
            .acquire_connection_wait(&self.node_id, consumer.clone(), Duration::from_secs(15))
            .await?;
        let next = IdeConnectionLease {
            connection_id: resolved.connection_id.clone(),
            consumer,
        };
        let previous = {
            let mut state = self
                .state
                .lock()
                .expect("IDE remote session state poisoned");
            if state.closed {
                drop(state);
                // A connection may become ready after the owning IDE surface
                // was released. Remove this session's unique consumer instead
                // of reviving the released runtime dependency.
                self.router
                    .release_consumer(&next.connection_id, &next.consumer);
                return Err(RouteError::NotConnected(self.node_id.0.clone()));
            }
            if state.lease.as_ref() == Some(&next) {
                None
            } else {
                state.lease.replace(next)
            }
        };
        if let Some(previous) = previous {
            self.router
                .release_consumer(&previous.connection_id, &previous.consumer);
        }
        Ok(resolved)
    }

    fn connection_id(&self) -> Option<String> {
        self.state
            .lock()
            .expect("IDE remote session state poisoned")
            .lease
            .as_ref()
            .map(|lease| lease.connection_id.clone())
    }

    fn close(&self) {
        let lease = {
            let mut state = self
                .state
                .lock()
                .expect("IDE remote session state poisoned");
            state.closed = true;
            state.lease.take()
        };
        if let Some(lease) = lease {
            self.router
                .release_consumer(&lease.connection_id, &lease.consumer);
        }
    }
}

impl Drop for IdeRemoteSessionInner {
    fn drop(&mut self) {
        self.close();
    }
}

include!("agent/filesystem.rs");
include!("agent/protocol.rs");
include!("agent/transport.rs");
include!("agent/session.rs");
include!("agent/registry.rs");
include!("agent/install.rs");
include!("agent/mapping.rs");
include!("agent/tests.rs");
