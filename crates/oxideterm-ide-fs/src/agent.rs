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
use oxideterm_ssh::{NodeId, NodeRouter, SshConnectionHandle};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::NodeSftpIdeFileSystem;

const AGENT_REMOTE_DIR: &str = ".oxideterm";
const AGENT_BINARY_NAME: &str = "oxideterm-agent";
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
    mode: NodeAgentMode,
    status: Arc<RwLock<AgentStatus>>,
    deploy_lock: Arc<Mutex<()>>,
}

impl NodeAgentIdeFileSystem {
    pub fn new(router: NodeRouter, mode: NodeAgentMode) -> Self {
        Self {
            sftp: NodeSftpIdeFileSystem::new(router.clone()),
            router,
            registry: Arc::new(AgentRegistry::default()),
            mode,
            status: Arc::new(RwLock::new(AgentStatus::SftpFallback)),
            deploy_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn set_mode(&mut self, mode: NodeAgentMode) {
        self.mode = mode;
        if mode == NodeAgentMode::Disabled {
            self.set_status(AgentStatus::SftpFallback);
        }
    }

    pub fn status(&self) -> AgentStatus {
        self.status
            .read()
            .map(|status| status.clone())
            .unwrap_or(AgentStatus::SftpFallback)
    }

    pub async fn deploy_agent_for_node(&self, node_id: impl Into<String>) -> AgentStatus {
        self.ensure_agent(&NodeId::new(node_id.into())).await
    }

    pub async fn refresh_agent_status(&self, node_id: impl Into<String>) -> AgentStatus {
        if self.mode == NodeAgentMode::Disabled {
            self.set_status(AgentStatus::SftpFallback);
            return AgentStatus::SftpFallback;
        }

        let node_id = NodeId::new(node_id.into());
        let status = match self.probe_agent_status(&node_id).await {
            Ok(status) => status,
            Err(error) => AgentStatus::Failed {
                reason: error.to_string(),
            },
        };
        self.set_status(status.clone());
        status
    }

    pub async fn remove_agent_for_node(
        &self,
        node_id: impl Into<String>,
    ) -> Result<(), IdeFileError> {
        let node_id = NodeId::new(node_id.into());
        let resolved = self
            .router
            .resolve_connection(&node_id)
            .map_err(|error| IdeFileError::new(IdeFileErrorKind::Other, error.to_string()))?;
        self.registry.remove(&resolved.connection_id).await;
        let remote_path = remote_agent_path(&resolved.handle)
            .await
            .map_err(|error| IdeFileError::new(IdeFileErrorKind::Other, error.to_string()))?;
        resolved
            .handle
            .run_command(
                &format!("rm -f -- '{}'", shell_single_quote(&remote_path)),
                Duration::from_secs(15),
                2048,
            )
            .await
            .map_err(|error| IdeFileError::new(IdeFileErrorKind::Other, error.to_string()))?;
        self.set_status(AgentStatus::SftpFallback);
        Ok(())
    }

    pub async fn open_project(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<IdeProjectInfo, IdeFileError> {
        let node_id = node_id.into();
        if self.mode == NodeAgentMode::Enabled {
            let _ = self.ensure_agent(&NodeId::new(node_id.clone())).await;
        } else {
            self.set_status(AgentStatus::SftpFallback);
        }
        self.sftp.open_project(node_id, path).await
    }

    pub async fn check_file(
        &self,
        node_id: impl Into<String>,
        path: impl Into<String>,
    ) -> Result<oxideterm_ide_core::IdeFileCheck, IdeFileError> {
        self.sftp.check_file(node_id, path).await
    }

    pub async fn batch_stat(
        &self,
        node_id: impl Into<String>,
        paths: Vec<String>,
    ) -> Result<Vec<Option<IdePathStat>>, IdeFileError> {
        self.sftp.batch_stat(node_id, paths).await
    }

    async fn agent_session(&self, node_id: &NodeId) -> Option<Arc<AgentSession>> {
        if self.mode == NodeAgentMode::Disabled {
            self.set_status(AgentStatus::SftpFallback);
            return None;
        }
        if self.mode == NodeAgentMode::Enabled {
            let _ = self.ensure_agent(node_id).await;
        }

        let resolved = self.router.resolve_connection(node_id).ok()?;
        let session = self.registry.get(&resolved.connection_id)?;
        if session.is_alive() {
            self.set_status(session.status());
            Some(session)
        } else {
            self.registry
                .remove_without_shutdown(&resolved.connection_id);
            self.set_status(AgentStatus::SftpFallback);
            None
        }
    }

    async fn ensure_agent(&self, node_id: &NodeId) -> AgentStatus {
        let _guard = self.deploy_lock.lock().await;
        if let Ok(resolved) = self.router.resolve_connection(node_id)
            && let Some(session) = self.registry.get(&resolved.connection_id)
            && session.is_alive()
        {
            let status = session.status();
            self.set_status(status.clone());
            return status;
        }

        self.set_status(AgentStatus::Deploying);
        let status = match self.deploy_agent(node_id).await {
            Ok(status) => status,
            Err(error) => AgentStatus::Failed {
                reason: error.to_string(),
            },
        };
        self.set_status(status.clone());
        status
    }

    async fn deploy_agent(&self, node_id: &NodeId) -> Result<AgentStatus, AgentError> {
        let resolved = self.router.resolve_connection(node_id)?;
        let arch = detect_arch(&resolved.handle).await?;
        let remote_path = remote_agent_path(&resolved.handle).await?;
        let target = arch_to_target(&arch);
        let install_state = probe_remote_install(&resolved.handle, &remote_path).await;

        match target {
            Ok(target) => {
                if !matches!(install_state, RemoteAgentInstallState::Current) {
                    let binary = resolve_agent_binary(target)?;
                    upload_agent(
                        &resolved.handle,
                        &self.router,
                        node_id,
                        &remote_path,
                        &binary,
                    )
                    .await?;
                }
            }
            Err(AgentError::UnsupportedArch(_)) => match install_state {
                RemoteAgentInstallState::Missing => {
                    return Ok(AgentStatus::ManualUploadRequired { arch, remote_path });
                }
                RemoteAgentInstallState::Current => {}
                RemoteAgentInstallState::Incompatible(version) => {
                    return Ok(AgentStatus::ManualUpdateRequired {
                        arch,
                        remote_path,
                        current_agent_version: version.version,
                        current_compatibility_version: version.compatibility_version,
                        expected_compatibility_version: CURRENT_AGENT_COMPATIBILITY_VERSION,
                    });
                }
            },
            Err(error) => {
                return Err(error);
            }
        }

        let channel = resolved.handle.open_exec_channel().await?;
        let transport = AgentTransport::new(channel, &remote_path).await?;
        let info = handshake_agent(&transport).await?;
        let status = AgentStatus::Ready {
            version: info.version.clone(),
            arch: info.arch.clone(),
            pid: info.pid,
        };
        self.registry
            .register(resolved.connection_id, AgentSession::new(transport, info));
        Ok(status)
    }

    async fn probe_agent_status(&self, node_id: &NodeId) -> Result<AgentStatus, AgentError> {
        let resolved = self.router.resolve_connection(node_id)?;
        if let Some(session) = self.registry.get(&resolved.connection_id) {
            if session.is_alive() {
                return Ok(session.status());
            }
            self.registry
                .remove_without_shutdown(&resolved.connection_id);
        }

        let arch = detect_arch(&resolved.handle).await?;
        let remote_path = remote_agent_path(&resolved.handle).await?;
        let install_state = probe_remote_install(&resolved.handle, &remote_path).await;
        match arch_to_target(&arch) {
            Ok(_) => Ok(AgentStatus::NotDeployed),
            Err(AgentError::UnsupportedArch(_)) => match install_state {
                RemoteAgentInstallState::Missing => {
                    Ok(AgentStatus::ManualUploadRequired { arch, remote_path })
                }
                RemoteAgentInstallState::Current => Ok(AgentStatus::NotDeployed),
                RemoteAgentInstallState::Incompatible(version) => {
                    Ok(AgentStatus::ManualUpdateRequired {
                        arch,
                        remote_path,
                        current_agent_version: version.version,
                        current_compatibility_version: version.compatibility_version,
                        expected_compatibility_version: CURRENT_AGENT_COMPATIBILITY_VERSION,
                    })
                }
            },
            Err(error) => Err(error),
        }
    }

    fn set_status(&self, status: AgentStatus) {
        if let Ok(mut current) = self.status.write() {
            *current = status;
        }
    }
}

impl AsyncIdeFileSystem for NodeAgentIdeFileSystem {
    fn capabilities(&self) -> FileSystemCapabilities {
        FileSystemCapabilities {
            atomic_write: true,
            directory_listing: true,
            conflict_detection: true,
        }
    }

    fn read_file<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, IdeFileData> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            if let Some(session) = self.agent_session(&node_id).await {
                match session.read_file(&path).await {
                    Ok(result) => return Ok(ide_file_data_from_agent(result)),
                    Err(error) => {
                        warn!("[ide-agent] read via agent failed, falling back to SFTP: {error}");
                        self.set_status(AgentStatus::SftpFallback);
                    }
                }
            }
            self.sftp.read_file(location).await
        })
    }

    fn stat<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, FileStat> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            if let Some(session) = self.agent_session(&node_id).await {
                match session.stat(&path).await {
                    Ok(stat) if stat.exists => {
                        return Ok(FileStat {
                            version: version_from_agent_stat(&stat),
                            is_read_only: stat
                                .permissions
                                .as_deref()
                                .and_then(|raw| u32::from_str_radix(raw, 8).ok())
                                .map(|mode| mode & 0o200 == 0)
                                .unwrap_or(false),
                        });
                    }
                    Ok(_) => return Err(IdeFileError::new(IdeFileErrorKind::NotFound, path)),
                    Err(error) => {
                        warn!("[ide-agent] stat via agent failed, falling back to SFTP: {error}");
                        self.set_status(AgentStatus::SftpFallback);
                    }
                }
            }
            self.sftp.stat(location).await
        })
    }

    fn list_dir<'a>(&'a self, location: &'a IdeLocation) -> IdeFsFuture<'a, Vec<FileTreeEntry>> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            if let Some(session) = self.agent_session(&node_id).await {
                match session.list_dir(&path).await {
                    Ok(entries) => {
                        return Ok(entries
                            .into_iter()
                            .map(|entry| file_tree_entry_from_agent(&node_id, entry))
                            .collect());
                    }
                    Err(error) => {
                        warn!(
                            "[ide-agent] directory listing via agent failed, falling back to SFTP: {error}"
                        );
                        self.set_status(AgentStatus::SftpFallback);
                    }
                }
            }
            self.sftp.list_dir(location).await
        })
    }

    fn write_file<'a>(
        &'a self,
        location: &'a IdeLocation,
        text: &'a str,
        expected_version: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> IdeFsFuture<'a, SavedFileVersion> {
        Box::pin(async move {
            let (node_id, path) = remote_location(location)?;
            if mode == WriteMode::CreateNew {
                return self
                    .sftp
                    .write_file(location, text, expected_version, mode)
                    .await;
            }

            if let Some(session) = self.agent_session(&node_id).await {
                let expect_hash = expected_version.and_then(|version| version.etag.as_deref());
                match session.write_file(&path, text, expect_hash).await {
                    Ok(result) => return Ok(version_from_agent_write(&result)),
                    Err(AgentError::Rpc { code, message })
                        if is_agent_conflict_parts(code, &message) =>
                    {
                        return Err(IdeFileError::new(IdeFileErrorKind::Conflict, message));
                    }
                    Err(error) => {
                        self.set_status(AgentStatus::Failed {
                            reason: error.to_string(),
                        });
                        return Err(map_agent_error(error));
                    }
                }
            }

            self.sftp
                .write_file(location, text, expected_version, mode)
                .await
        })
    }
}

#[derive(Debug, Serialize)]
struct AgentRequest {
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AgentResponse {
    id: u64,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<AgentRpcError>,
}

#[derive(Clone, Debug, Deserialize)]
struct AgentRpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Deserialize)]
struct AgentNotification {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AgentMessage {
    Response(AgentResponse),
    Notification(AgentNotification),
}

#[derive(Debug, Deserialize, Serialize)]
struct ReadFileResult {
    content: String,
    hash: String,
    size: u64,
    mtime: u64,
    #[serde(default = "plain_encoding")]
    encoding: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct WriteFileResult {
    hash: String,
    size: u64,
    mtime: u64,
    atomic: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct StatResult {
    exists: bool,
    file_type: Option<String>,
    size: Option<u64>,
    mtime: Option<u64>,
    permissions: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FileEntry {
    name: String,
    path: String,
    file_type: String,
    #[serde(default)]
    is_symlink: bool,
    symlink_target: Option<String>,
    target_file_type: Option<String>,
    size: u64,
    mtime: Option<u64>,
    permissions: Option<String>,
    children: Option<Vec<FileEntry>>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct SysInfoResult {
    version: String,
    #[serde(default = "legacy_agent_compatibility")]
    compatibility_version: u32,
    arch: String,
    os: String,
    pid: u32,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RemoteAgentVersionInfo {
    version: String,
    compatibility_version: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RemoteAgentInstallState {
    Missing,
    Current,
    Incompatible(RemoteAgentVersionInfo),
}

fn plain_encoding() -> String {
    "plain".to_string()
}

fn legacy_agent_compatibility() -> u32 {
    LEGACY_AGENT_COMPATIBILITY_VERSION
}

type PendingMap =
    Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, AgentRpcError>>>>>;

struct AgentTransport {
    write_tx: mpsc::Sender<String>,
    pending: PendingMap,
    shutdown_tx: mpsc::Sender<()>,
    alive: Arc<AtomicBool>,
}

impl AgentTransport {
    async fn new(
        mut channel: russh::Channel<russh::client::Msg>,
        agent_command: &str,
    ) -> Result<Self, AgentError> {
        channel
            .exec(true, agent_command)
            .await
            .map_err(|error| AgentError::Ssh(format!("Failed to exec agent: {error}")))?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        let (write_tx, mut write_rx) = mpsc::channel::<String>(256);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        let pending_for_task = pending.clone();
        let alive_for_task = alive.clone();
        tokio::spawn(async move {
            let mut buffer = String::new();
            loop {
                tokio::select! {
                    Some(line) = write_rx.recv() => {
                        let data = format!("{line}\n");
                        if channel.data(data.as_bytes()).await.is_err() {
                            warn!("[ide-agent] write failed; channel closed");
                            break;
                        }
                    }
                    message = channel.wait() => {
                        match message {
                            Some(ChannelMsg::Data { data }) => {
                                buffer.push_str(&String::from_utf8_lossy(&data));
                                while let Some(newline) = buffer.find('\n') {
                                    let line = buffer[..newline].trim().to_string();
                                    buffer = buffer[newline + 1..].to_string();
                                    if line.is_empty() {
                                        continue;
                                    }
                                    handle_agent_line(&pending_for_task, &line).await;
                                }
                            }
                            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                                for line in String::from_utf8_lossy(&data).lines() {
                                    debug!("[ide-agent-stderr] {line}");
                                }
                            }
                            Some(ChannelMsg::ExitStatus { exit_status }) => {
                                info!("[ide-agent] exited with status {exit_status}");
                                break;
                            }
                            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                            _ => {}
                        }
                    }
                    _ = shutdown_rx.recv() => break,
                }
            }

            alive_for_task.store(false, Ordering::Relaxed);
            let mut pending = pending_for_task.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err(AgentRpcError {
                    code: -32603,
                    message: "Agent channel closed".to_string(),
                }));
            }
        });

        Ok(Self {
            write_tx,
            pending,
            shutdown_tx,
            alive,
        })
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AgentError> {
        self.call_with_timeout(method, params, AGENT_RPC_TIMEOUT_SECS)
            .await
    }

    async fn call_with_timeout(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, AgentError> {
        if !self.is_alive() {
            return Err(AgentError::ChannelClosed);
        }

        let id = NEXT_AGENT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = AgentRequest {
            id,
            method: method.to_string(),
            params,
        };
        let json = serde_json::to_string(&request)
            .map_err(|error| AgentError::Serialize(error.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write_tx
            .send(json)
            .await
            .map_err(|_| AgentError::ChannelClosed)?;

        match tokio::time::timeout(Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(error))) => Err(AgentError::from(error)),
            Ok(Err(_)) => Err(AgentError::ChannelClosed),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(AgentError::Timeout(timeout_secs))
            }
        }
    }

    async fn shutdown(&self) {
        let _ = self
            .call_with_timeout("sys/shutdown", serde_json::json!({}), 5)
            .await;
        let _ = self.shutdown_tx.send(()).await;
    }
}

async fn handle_agent_line(pending: &PendingMap, line: &str) {
    match serde_json::from_str::<AgentMessage>(line) {
        Ok(AgentMessage::Response(response)) => {
            let mut pending = pending.lock().await;
            if let Some(tx) = pending.remove(&response.id) {
                let result = if let Some(error) = response.error {
                    Err(error)
                } else {
                    Ok(response.result.unwrap_or_default())
                };
                let _ = tx.send(result);
            }
        }
        Ok(AgentMessage::Notification(notification)) => {
            debug!(
                "[ide-agent] notification {} {}",
                notification.method, notification.params
            );
        }
        Err(error) => debug!("[ide-agent] ignored non-JSON line: {line} ({error})"),
    }
}

struct AgentSession {
    transport: AgentTransport,
    info: SysInfoResult,
}

impl AgentSession {
    fn new(transport: AgentTransport, info: SysInfoResult) -> Self {
        Self { transport, info }
    }

    fn is_alive(&self) -> bool {
        self.transport.is_alive()
    }

    fn status(&self) -> AgentStatus {
        if self.is_alive() {
            AgentStatus::Ready {
                version: self.info.version.clone(),
                arch: self.info.arch.clone(),
                pid: self.info.pid,
            }
        } else {
            AgentStatus::Failed {
                reason: "Agent channel closed".to_string(),
            }
        }
    }

    fn supports_capability(&self, capability: &str) -> bool {
        self.info
            .capabilities
            .iter()
            .any(|available| available == capability)
    }

    async fn read_file(&self, path: &str) -> Result<ReadFileResult, AgentError> {
        let value = self
            .transport
            .call("fs/readFile", serde_json::json!({ "path": path }))
            .await?;
        let mut result: ReadFileResult = serde_json::from_value(value)
            .map_err(|error| AgentError::Deserialize(error.to_string()))?;
        if result.encoding == "zstd+base64" {
            let compressed = base64::engine::general_purpose::STANDARD
                .decode(&result.content)
                .map_err(|error| {
                    AgentError::Deserialize(format!("Base64 decode error: {error}"))
                })?;
            let decompressed =
                zstd::stream::decode_all(compressed.as_slice()).map_err(|error| {
                    AgentError::Deserialize(format!("Zstd decompress error: {error}"))
                })?;
            result.content = String::from_utf8_lossy(&decompressed).into_owned();
            result.encoding = plain_encoding();
        }
        Ok(result)
    }

    async fn write_file(
        &self,
        path: &str,
        content: &str,
        expect_hash: Option<&str>,
    ) -> Result<WriteFileResult, AgentError> {
        let (content, encoding) =
            if self.supports_capability("zstd") && content.len() > AGENT_COMPRESS_THRESHOLD {
                let compressed = zstd::stream::encode_all(content.as_bytes(), 3)
                    .map_err(|error| AgentError::Serialize(error.to_string()))?;
                if compressed.len() < content.len() {
                    (
                        base64::engine::general_purpose::STANDARD.encode(compressed),
                        "zstd+base64",
                    )
                } else {
                    (content.to_string(), "plain")
                }
            } else {
                (content.to_string(), "plain")
            };

        let mut params = serde_json::json!({
            "path": path,
            "content": content,
            "encoding": encoding,
        });
        if let Some(hash) = expect_hash {
            params["expect_hash"] = serde_json::Value::String(hash.to_string());
        }

        let value = self.transport.call("fs/writeFile", params).await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn stat(&self, path: &str) -> Result<StatResult, AgentError> {
        let value = self
            .transport
            .call("fs/stat", serde_json::json!({ "path": path }))
            .await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>, AgentError> {
        let value = self
            .transport
            .call("fs/listDir", serde_json::json!({ "path": path }))
            .await?;
        serde_json::from_value(value).map_err(|error| AgentError::Deserialize(error.to_string()))
    }

    async fn shutdown(&self) {
        self.transport.shutdown().await;
    }
}

#[derive(Default)]
struct AgentRegistry {
    sessions: DashMap<String, Arc<AgentSession>>,
}

impl AgentRegistry {
    fn register(&self, connection_id: String, session: AgentSession) {
        self.sessions.insert(connection_id, Arc::new(session));
    }

    fn get(&self, connection_id: &str) -> Option<Arc<AgentSession>> {
        self.sessions
            .get(connection_id)
            .map(|session| session.value().clone())
    }

    fn remove_without_shutdown(&self, connection_id: &str) {
        self.sessions.remove(connection_id);
    }

    async fn remove(&self, connection_id: &str) {
        if let Some((_, session)) = self.sessions.remove(connection_id) {
            session.shutdown().await;
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum AgentError {
    #[error("Agent channel closed")]
    ChannelClosed,
    #[error("Agent RPC timeout after {0}s")]
    Timeout(u64),
    #[error("Failed to serialize agent request: {0}")]
    Serialize(String),
    #[error("Failed to deserialize agent response: {0}")]
    Deserialize(String),
    #[error("Agent RPC error {code}: {message}")]
    Rpc { code: i32, message: String },
    #[error("SSH error: {0}")]
    Ssh(String),
    #[error("SFTP error: {0}")]
    Sftp(String),
    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),
    #[error("Agent binary not found: {0}")]
    BinaryNotFound(String),
    #[error("Local I/O error: {0}")]
    LocalIo(String),
    #[error("Route error: {0}")]
    Route(String),
    #[error("Handshake failed: {0}")]
    Handshake(String),
}

impl From<AgentRpcError> for AgentError {
    fn from(error: AgentRpcError) -> Self {
        Self::Rpc {
            code: error.code,
            message: error.message,
        }
    }
}

impl From<SftpError> for AgentError {
    fn from(error: SftpError) -> Self {
        Self::Sftp(error.to_string())
    }
}

impl From<oxideterm_ssh::RouteError> for AgentError {
    fn from(error: oxideterm_ssh::RouteError) -> Self {
        Self::Route(error.to_string())
    }
}

async fn detect_arch(handle: &SshConnectionHandle) -> Result<String, AgentError> {
    let arch = handle
        .run_command("uname -m", Duration::from_secs(10), 512)
        .await
        .map_err(|error| AgentError::Ssh(error.to_string()))?
        .trim()
        .to_string();
    if arch.is_empty() {
        Err(AgentError::UnsupportedArch("unknown".to_string()))
    } else {
        Ok(arch)
    }
}

async fn remote_agent_path(handle: &SshConnectionHandle) -> Result<String, AgentError> {
    let home = handle
        .run_command("echo \"$HOME\"", Duration::from_secs(10), 1024)
        .await
        .map_err(|error| AgentError::Ssh(error.to_string()))?
        .trim()
        .to_string();
    if home.is_empty() || !home.starts_with('/') {
        return Err(AgentError::Ssh(format!(
            "Cannot resolve HOME directory on remote host (got {home:?})"
        )));
    }
    Ok(format!("{home}/{AGENT_REMOTE_DIR}/{AGENT_BINARY_NAME}"))
}

fn arch_to_target(arch: &str) -> Result<&'static str, AgentError> {
    match arch {
        "x86_64" | "amd64" => Ok("x86_64-linux-musl"),
        "aarch64" | "arm64" => Ok("aarch64-linux-musl"),
        other => Err(AgentError::UnsupportedArch(other.to_string())),
    }
}

async fn probe_remote_install(
    handle: &SshConnectionHandle,
    remote_path: &str,
) -> RemoteAgentInstallState {
    let command = format!(
        "'{}' --version 2>/dev/null || echo 'NOT_FOUND'",
        shell_single_quote(remote_path)
    );
    match handle
        .run_command(&command, Duration::from_secs(5), 2048)
        .await
    {
        Ok(output) => parse_remote_version_output(output.trim()),
        Err(_) => RemoteAgentInstallState::Missing,
    }
}

fn parse_remote_version_output(output: &str) -> RemoteAgentInstallState {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed.contains("NOT_FOUND") {
        return RemoteAgentInstallState::Missing;
    }

    let mut parts = trimmed.split_whitespace();
    let _binary_name = parts.next();
    let version = parts.next().unwrap_or(trimmed).to_string();
    let mut compatibility_version = LEGACY_AGENT_COMPATIBILITY_VERSION;
    let mut saw_compat_marker = false;
    while let Some(part) = parts.next() {
        if part == "compat" {
            saw_compat_marker = true;
            compatibility_version = parts
                .next()
                .and_then(|raw| raw.parse::<u32>().ok())
                .unwrap_or(INVALID_AGENT_COMPATIBILITY_VERSION);
            break;
        }
    }
    if !saw_compat_marker {
        compatibility_version = LEGACY_AGENT_COMPATIBILITY_VERSION;
    }

    if compatibility_version == CURRENT_AGENT_COMPATIBILITY_VERSION {
        RemoteAgentInstallState::Current
    } else {
        RemoteAgentInstallState::Incompatible(RemoteAgentVersionInfo {
            version,
            compatibility_version,
        })
    }
}

fn resolve_agent_binary(target: &str) -> Result<PathBuf, AgentError> {
    let file_name = format!("oxideterm-agent-{target}");
    for dir in agent_resource_dirs() {
        let candidate = dir.join(&file_name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AgentError::BinaryNotFound(format!(
        "agents/{file_name}; set OXIDETERM_AGENT_DIR or package it in app resources"
    )))
}

fn agent_resource_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("OXIDETERM_AGENT_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        // Tauri resolves from app resources. Native keeps the same contract:
        // package the prebuilt remote-agent binaries under `agents/`, and only
        // use source-tree locations as developer fallbacks.
        dirs.push(exe_dir.join("../Resources/agents"));
        dirs.push(exe_dir.join("resources/agents"));
        dirs.push(exe_dir.join("agents"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("crates/oxideterm-gpui-app/resources/agents"));
        dirs.push(cwd.join("tauri版本代码/src-tauri/agents"));
    }
    dirs
}

async fn upload_agent(
    handle: &SshConnectionHandle,
    router: &NodeRouter,
    node_id: &NodeId,
    remote_path: &str,
    binary_path: &PathBuf,
) -> Result<(), AgentError> {
    let remote_dir = remote_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .ok_or_else(|| AgentError::Ssh(format!("Invalid remote agent path: {remote_path}")))?;
    handle
        .run_command(
            &format!("mkdir -p -- '{}'", shell_single_quote(remote_dir)),
            Duration::from_secs(30),
            2048,
        )
        .await
        .map_err(|error| AgentError::Ssh(error.to_string()))?;

    let sftp = router.acquire_sftp(node_id).await?;
    let sftp = sftp.lock().await;
    let binary = tokio::fs::read(binary_path)
        .await
        .map_err(|error| AgentError::LocalIo(error.to_string()))?;
    sftp.write_content(remote_path, &binary).await?;
    handle
        .run_command(
            &format!("chmod +x -- '{}'", shell_single_quote(remote_path)),
            Duration::from_secs(30),
            2048,
        )
        .await
        .map_err(|error| AgentError::Ssh(error.to_string()))?;
    Ok(())
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}

async fn handshake_agent(transport: &AgentTransport) -> Result<SysInfoResult, AgentError> {
    transport
        .call_with_timeout("sys/ping", serde_json::json!({}), 10)
        .await
        .map_err(|error| AgentError::Handshake(format!("Ping failed: {error}")))?;
    let info_value = transport
        .call_with_timeout("sys/info", serde_json::json!({}), 10)
        .await
        .map_err(|error| AgentError::Handshake(format!("sys/info failed: {error}")))?;
    let info: SysInfoResult = serde_json::from_value(info_value)
        .map_err(|error| AgentError::Handshake(format!("Invalid sys/info response: {error}")))?;
    if info.compatibility_version != CURRENT_AGENT_COMPATIBILITY_VERSION {
        return Err(AgentError::Handshake(format!(
            "Agent compatibility mismatch: got {}, expected {}",
            info.compatibility_version, CURRENT_AGENT_COMPATIBILITY_VERSION
        )));
    }
    Ok(info)
}

fn remote_location(location: &IdeLocation) -> Result<(NodeId, String), IdeFileError> {
    match location {
        IdeLocation::Remote { node_id, path } => Ok((NodeId::new(node_id.clone()), path.clone())),
        IdeLocation::Local { .. } => Err(IdeFileError::new(
            IdeFileErrorKind::Unsupported,
            "Node agent IDE filesystem cannot read local locations",
        )),
    }
}

fn ide_file_data_from_agent(result: ReadFileResult) -> IdeFileData {
    IdeFileData {
        text: result.content,
        version: SavedFileVersion {
            size_bytes: Some(result.size),
            modified_millis: Some(result.mtime as i64),
            etag: Some(result.hash),
        },
    }
}

fn version_from_agent_write(result: &WriteFileResult) -> SavedFileVersion {
    SavedFileVersion {
        size_bytes: Some(result.size),
        modified_millis: Some(result.mtime as i64),
        etag: Some(result.hash.clone()),
    }
}

fn version_from_agent_stat(stat: &StatResult) -> SavedFileVersion {
    SavedFileVersion {
        size_bytes: stat.size,
        modified_millis: stat.mtime.map(|mtime| mtime as i64),
        etag: None,
    }
}

fn file_tree_entry_from_agent(node_id: &NodeId, entry: FileEntry) -> FileTreeEntry {
    let kind = match (entry.file_type.as_str(), entry.target_file_type.as_deref()) {
        ("directory" | "dir", _) => FileKind::Directory,
        ("file", _) => FileKind::File,
        ("symlink", Some("directory" | "dir")) => FileKind::Directory,
        ("symlink", _) => FileKind::Symlink,
        _ => FileKind::Other,
    };
    FileTreeEntry {
        location: IdeLocation::remote(node_id.0.clone(), entry.path),
        kind,
        name: entry.name,
        version: SavedFileVersion {
            size_bytes: Some(entry.size),
            modified_millis: entry.mtime.map(|mtime| mtime as i64),
            etag: None,
        },
    }
}

#[cfg(test)]
fn is_agent_conflict(error: &AgentRpcError) -> bool {
    is_agent_conflict_parts(error.code, &error.message)
}

fn is_agent_conflict_parts(code: i32, message: &str) -> bool {
    code == -4
        || message.contains("CONFLICT")
        || message.contains("hash mismatch")
        || message.contains("modified externally")
}

fn map_agent_error(error: AgentError) -> IdeFileError {
    let message = error.to_string();
    let kind = match &error {
        AgentError::Timeout(_) => IdeFileErrorKind::Timeout,
        AgentError::Rpc { code: -2, .. } => IdeFileErrorKind::NotFound,
        AgentError::Rpc { code: -3, .. } => IdeFileErrorKind::PermissionDenied,
        AgentError::Rpc { message, .. } if message.contains("CONFLICT") => {
            IdeFileErrorKind::Conflict
        }
        AgentError::ChannelClosed | AgentError::Ssh(_) | AgentError::Route(_) => {
            IdeFileErrorKind::Disconnected
        }
        AgentError::UnsupportedArch(_) | AgentError::BinaryNotFound(_) => {
            IdeFileErrorKind::Unsupported
        }
        AgentError::Serialize(_)
        | AgentError::Deserialize(_)
        | AgentError::Rpc { .. }
        | AgentError::LocalIo(_)
        | AgentError::Sftp(_)
        | AgentError::Handshake(_) => IdeFileErrorKind::Other,
    };
    IdeFileError::new(kind, message)
}

#[cfg(test)]
fn file_tree_entry_from_sftp(node_id: &NodeId, entry: FileInfo) -> FileTreeEntry {
    FileTreeEntry {
        location: IdeLocation::remote(node_id.0.clone(), entry.path),
        kind: match entry.file_type {
            FileType::File => FileKind::File,
            FileType::Directory => FileKind::Directory,
            FileType::Symlink => FileKind::Symlink,
            FileType::Unknown => FileKind::Other,
        },
        name: entry.name,
        version: SavedFileVersion {
            size_bytes: Some(entry.size),
            modified_millis: (entry.modified > 0).then_some(entry.modified * 1000),
            etag: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_agent_entries_to_core_file_kinds() {
        let node_id = NodeId::new("node-1");
        let entry = FileEntry {
            name: "src".to_string(),
            path: "/repo/src".to_string(),
            file_type: "directory".to_string(),
            is_symlink: false,
            symlink_target: None,
            target_file_type: None,
            size: 0,
            mtime: Some(12),
            permissions: None,
            children: None,
            truncated: false,
        };

        let mapped = file_tree_entry_from_agent(&node_id, entry);
        assert_eq!(mapped.kind, FileKind::Directory);
        assert_eq!(mapped.location, IdeLocation::remote("node-1", "/repo/src"));
    }

    #[test]
    fn maps_agent_symlink_directories_as_directories() {
        let node_id = NodeId::new("node-1");
        let entry = FileEntry {
            name: "current".to_string(),
            path: "/repo/current".to_string(),
            file_type: "symlink".to_string(),
            is_symlink: true,
            symlink_target: Some("/repo/releases/current".to_string()),
            target_file_type: Some("directory".to_string()),
            size: 0,
            mtime: Some(12),
            permissions: None,
            children: None,
            truncated: false,
        };

        let mapped = file_tree_entry_from_agent(&node_id, entry);
        assert_eq!(mapped.kind, FileKind::Directory);
        assert_eq!(
            mapped.location,
            IdeLocation::remote("node-1", "/repo/current")
        );
    }

    #[test]
    fn recognizes_agent_write_conflicts() {
        assert!(is_agent_conflict(&AgentRpcError {
            code: -4,
            message: "File modified externally".to_string(),
        }));
        assert!(is_agent_conflict(&AgentRpcError {
            code: -1,
            message: "hash mismatch".to_string(),
        }));
    }

    #[test]
    fn maps_sftp_entries_like_tauri_file_info() {
        let node_id = NodeId::new("node-1");
        let entry = FileInfo {
            name: "main.rs".to_string(),
            path: "/repo/main.rs".to_string(),
            file_type: FileType::File,
            size: 128,
            modified: 7,
            permissions: "644".to_string(),
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        };

        let mapped = file_tree_entry_from_sftp(&node_id, entry);
        assert_eq!(mapped.kind, FileKind::File);
        assert_eq!(mapped.version.modified_millis, Some(7000));
    }

    #[test]
    fn drops_agent_registry_without_tokio_reactor() {
        let registry = AgentRegistry::default();
        let (write_tx, _write_rx) = mpsc::channel::<String>(1);
        let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>(1);
        let transport = AgentTransport {
            write_tx,
            pending: Arc::new(Mutex::new(HashMap::new())),
            shutdown_tx,
            alive: Arc::new(AtomicBool::new(false)),
        };
        registry.register(
            "conn-1".to_string(),
            AgentSession::new(
                transport,
                SysInfoResult {
                    version: "0.12.1".to_string(),
                    compatibility_version: CURRENT_AGENT_COMPATIBILITY_VERSION,
                    arch: "x86_64".to_string(),
                    os: "linux".to_string(),
                    pid: 42,
                    capabilities: Vec::new(),
                },
            ),
        );

        drop(registry);
    }

    #[test]
    fn parses_remote_agent_version_like_tauri() {
        assert_eq!(
            parse_remote_version_output("NOT_FOUND"),
            RemoteAgentInstallState::Missing
        );
        assert_eq!(
            parse_remote_version_output(&format!(
                "oxideterm-agent 0.12.1 compat {CURRENT_AGENT_COMPATIBILITY_VERSION}"
            )),
            RemoteAgentInstallState::Current
        );
        assert_eq!(
            parse_remote_version_output("oxideterm-agent 0.12.1 compat abc"),
            RemoteAgentInstallState::Incompatible(RemoteAgentVersionInfo {
                version: "0.12.1".to_string(),
                compatibility_version: INVALID_AGENT_COMPATIBILITY_VERSION,
            })
        );
    }
}
