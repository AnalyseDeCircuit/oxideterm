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
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use base64::Engine;
use dashmap::DashMap;
use oxideterm_ide_core::{
    AsyncIdeFileSystem, FileKind, FileStat, FileSystemCapabilities, FileTreeEntry, IdeFileData,
    IdeFileError, IdeFileErrorKind, IdeFsFuture, IdeLocation, IdePathStat, IdeProjectInfo,
    SavedFileVersion, WriteMode,
};
#[cfg(test)]
use oxideterm_sftp::{FileInfo, FileType};
use oxideterm_sftp::{SftpError, SftpExecChannelOpener};
use oxideterm_ssh::{
    ConnectionConsumer, NodeId, NodeRouter, ResolvedConnection, RouteError, SshConnectionHandle,
};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc, oneshot};
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
    // using the node connection until the IDE project/tab is closed. Native
    // records that as an explicit registry consumer instead of borrowing
    // liveness from SFTP channels or a terminal tab.
    ide_consumers: Arc<DashMap<String, IdeConnectionLease>>,
    mode: NodeAgentMode,
    status: Arc<RwLock<AgentStatus>>,
    deploy_lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IdeConnectionLease {
    connection_id: String,
    consumer: ConnectionConsumer,
}

include!("agent/filesystem.rs");
include!("agent/protocol.rs");
include!("agent/transport.rs");
include!("agent/session.rs");
include!("agent/registry.rs");
include!("agent/install.rs");
include!("agent/mapping.rs");
include!("agent/tests.rs");
