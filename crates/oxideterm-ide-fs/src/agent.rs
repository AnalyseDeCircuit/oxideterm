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
#[cfg(test)]
use oxideterm_sftp::{FileInfo, FileType};
use oxideterm_sftp::{SftpError, SftpExecChannelOpener};
use oxideterm_ssh::{
    ConnectionConsumer, NodeId, NodeRouter, ResolvedConnection, RouteError, SshConnectionHandle,
};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::NodeSftpIdeFileSystem;

const AGENT_REMOTE_DIR: &str = ".oxideterm";
const AGENT_BINARY_NAME: &str = "oxideterm-agent";
const AGENT_REMOTE_PATH: &str = "~/.oxideterm/oxideterm-agent";
const AGENT_RPC_TIMEOUT_SECS: u64 = 30;
const AGENT_COMPRESS_THRESHOLD: usize = 32 * 1024;
const LEGACY_AGENT_COMPATIBILITY_VERSION: u32 = 1;
const CURRENT_AGENT_COMPATIBILITY_VERSION: u32 = 2;
const INVALID_AGENT_COMPATIBILITY_VERSION: u32 = 0;

static NEXT_AGENT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

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
    // Tauri's IDE is node-scoped: it can outlive terminal panes and should keep
    // using the node connection until the IDE project/tab is closed. Native now
    // models that as an explicit remote session handle whose Drop releases the
    // NodeRouter consumer, instead of relying on GPUI panes to remember every
    // low-level release path.
    ide_sessions: Arc<DashMap<String, Arc<IdeRemoteSessionInner>>>,
    mode: NodeAgentMode,
    // Tauri computes node_agent_status by resolving node_id to the current SSH
    // connection id, then querying AgentRegistry by that connection. Keep the
    // same shape here so one node's agent result cannot overwrite another's.
    agent_statuses: Arc<DashMap<AgentStatusKey, AgentStatus>>,
    latest_agent_status: Arc<DashMap<String, AgentStatusKey>>,
    watch_subscriptions: Arc<DashMap<IdeWatchKey, Arc<IdeWatchShared>>>,
    deploy_lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IdeConnectionLease {
    connection_id: String,
    consumer: ConnectionConsumer,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AgentStatusKey {
    node_id: String,
    connection_id: String,
}

struct IdeWatchShared {
    connection_id: String,
    events_tx: broadcast::Sender<IdeWatchEvent>,
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
    lease: StdMutex<Option<IdeConnectionLease>>,
}

impl IdeRemoteSessionInner {
    fn new(node_id: NodeId, router: NodeRouter) -> Self {
        Self {
            node_id,
            router,
            lease: StdMutex::new(None),
        }
    }

    async fn acquire_connection(&self) -> Result<ResolvedConnection, RouteError> {
        let consumer = ConnectionConsumer::Ide(self.node_id.0.clone());
        let resolved = self
            .router
            .acquire_connection_wait(&self.node_id, consumer.clone(), Duration::from_secs(15))
            .await?;
        let next = IdeConnectionLease {
            connection_id: resolved.connection_id.clone(),
            consumer,
        };
        let previous = {
            let mut lease = self
                .lease
                .lock()
                .expect("IDE remote session lease poisoned");
            if lease.as_ref() == Some(&next) {
                None
            } else {
                lease.replace(next)
            }
        };
        if let Some(previous) = previous {
            self.router
                .release_consumer(&previous.connection_id, &previous.consumer);
        }
        Ok(resolved)
    }

    fn connection_id(&self) -> Option<String> {
        self.lease
            .lock()
            .expect("IDE remote session lease poisoned")
            .as_ref()
            .map(|lease| lease.connection_id.clone())
    }

    fn close(&self) {
        if let Some(lease) = self
            .lease
            .lock()
            .expect("IDE remote session lease poisoned")
            .take()
        {
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
