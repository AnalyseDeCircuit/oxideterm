//! Agent Registry — manages active agent sessions per SSH connection.
//!
//! Each SSH connection can have at most one agent session.
//! The registry provides thread-safe access and automatic cleanup
//! when connections are closed.

use std::sync::Arc;

use dashmap::DashMap;
use tracing::info;

use super::protocol::{
    AgentStatus, FileEntry, GitStatusResult, GrepMatch, ListTreeResult, ReadFileResult,
    StatResult, SymbolIndexResult, SymbolInfo, SysInfoResult, WatchEvent, WriteFileResult,
};
use super::transport::{AgentTransport, TransportError};

/// An active agent session for a single SSH connection.
pub struct AgentSession {
    /// JSON-RPC transport layer.
    transport: AgentTransport,

    /// Agent system info (from handshake).
    info: SysInfoResult,
}

impl AgentSession {
    /// Create a new agent session.
    pub fn new(transport: AgentTransport, info: SysInfoResult) -> Self {
        Self { transport, info }
    }

    /// Get the agent's system info.
    pub fn info(&self) -> &SysInfoResult {
        &self.info
    }

    /// Check if the agent is still alive.
    pub fn is_alive(&self) -> bool {
        self.transport.is_alive()
    }

    /// Get the agent status.
    pub fn status(&self) -> AgentStatus {
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

    // ═══════════════════════════════════════════════════════════════════
    // fs/* operations
    // ═══════════════════════════════════════════════════════════════════

    /// Read a file with content hash (auto-decompresses zstd+base64 responses).
    pub async fn read_file(&self, path: &str) -> Result<ReadFileResult, TransportError> {
        let result = self
            .transport
            .call(
                "fs/readFile",
                serde_json::json!({ "path": path }),
            )
            .await?;

        let mut file_result: ReadFileResult = serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))?;

        // Transparently decompress zstd+base64 encoded content
        if file_result.encoding == "zstd+base64" {
            use base64::Engine;
            let compressed = base64::engine::general_purpose::STANDARD
                .decode(&file_result.content)
                .map_err(|e| TransportError::DeserializeError(format!("Base64 decode error: {}", e)))?;
            let decompressed = zstd::stream::decode_all(compressed.as_slice())
                .map_err(|e| TransportError::DeserializeError(format!("Zstd decompress error: {}", e)))?;
            file_result.content = String::from_utf8_lossy(&decompressed).into_owned();
            file_result.encoding = "plain".to_string();
        }

        Ok(file_result)
    }

    /// Atomic write with optional optimistic locking (auto-compresses large content).
    pub async fn write_file(
        &self,
        path: &str,
        content: &str,
        expect_hash: Option<&str>,
    ) -> Result<WriteFileResult, TransportError> {
        const COMPRESS_THRESHOLD: usize = 32 * 1024;

        let (send_content, encoding) = if content.len() > COMPRESS_THRESHOLD {
            // Try zstd compression
            match zstd::stream::encode_all(content.as_bytes(), 3) {
                Ok(compressed) if compressed.len() < content.len() => {
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&compressed);
                    (encoded, "zstd+base64")
                }
                _ => (content.to_string(), "plain"),
            }
        } else {
            (content.to_string(), "plain")
        };

        let mut params = serde_json::json!({
            "path": path,
            "content": send_content,
            "encoding": encoding,
        });

        if let Some(hash) = expect_hash {
            params["expect_hash"] = serde_json::Value::String(hash.to_string());
        }

        let result = self.transport.call("fs/writeFile", params).await?;

        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    /// Get file/directory metadata.
    pub async fn stat(&self, path: &str) -> Result<StatResult, TransportError> {
        let result = self
            .transport
            .call("fs/stat", serde_json::json!({ "path": path }))
            .await?;

        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    /// List directory contents (single level).
    pub async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>, TransportError> {
        let result = self
            .transport
            .call("fs/listDir", serde_json::json!({ "path": path }))
            .await?;

        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    /// List directory tree (recursive) — returns entries + truncation metadata.
    pub async fn list_tree(
        &self,
        path: &str,
        max_depth: Option<u32>,
        max_entries: Option<u32>,
    ) -> Result<ListTreeResult, TransportError> {
        let mut params = serde_json::json!({ "path": path });
        if let Some(d) = max_depth {
            params["max_depth"] = serde_json::json!(d);
        }
        if let Some(e) = max_entries {
            params["max_entries"] = serde_json::json!(e);
        }

        let result = self.transport.call("fs/listTree", params).await?;

        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    /// Create a directory.
    pub async fn mkdir(&self, path: &str, recursive: bool) -> Result<(), TransportError> {
        self.transport
            .call(
                "fs/mkdir",
                serde_json::json!({ "path": path, "recursive": recursive }),
            )
            .await?;
        Ok(())
    }

    /// Remove a file or directory.
    pub async fn remove(&self, path: &str, recursive: bool) -> Result<(), TransportError> {
        self.transport
            .call(
                "fs/remove",
                serde_json::json!({ "path": path, "recursive": recursive }),
            )
            .await?;
        Ok(())
    }

    /// Rename/move a file or directory.
    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<(), TransportError> {
        self.transport
            .call(
                "fs/rename",
                serde_json::json!({ "old_path": old_path, "new_path": new_path }),
            )
            .await?;
        Ok(())
    }

    /// Change file permissions.
    pub async fn chmod(&self, path: &str, mode: &str) -> Result<(), TransportError> {
        self.transport
            .call(
                "fs/chmod",
                serde_json::json!({ "path": path, "mode": mode }),
            )
            .await?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════
    // search/* operations
    // ═══════════════════════════════════════════════════════════════════

    /// Search files for a pattern (grep).
    pub async fn grep(
        &self,
        pattern: &str,
        path: &str,
        case_sensitive: bool,
        max_results: Option<u32>,
    ) -> Result<Vec<GrepMatch>, TransportError> {
        let mut params = serde_json::json!({
            "pattern": pattern,
            "path": path,
            "case_sensitive": case_sensitive,
        });
        if let Some(max) = max_results {
            params["max_results"] = serde_json::json!(max);
        }

        let result = self.transport.call("search/grep", params).await?;

        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    // ═══════════════════════════════════════════════════════════════════
    // git/* operations
    // ═══════════════════════════════════════════════════════════════════

    /// Get git status for a project directory.
    pub async fn git_status(&self, path: &str) -> Result<GitStatusResult, TransportError> {
        let result = self
            .transport
            .call("git/status", serde_json::json!({ "path": path }))
            .await?;

        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    // ═══════════════════════════════════════════════════════════════════
    // watch/* operations
    // ═══════════════════════════════════════════════════════════════════

    /// Start watching a directory for changes.
    pub async fn watch_start(
        &self,
        path: &str,
        ignore: Vec<String>,
    ) -> Result<(), TransportError> {
        self.transport
            .call(
                "watch/start",
                serde_json::json!({ "path": path, "ignore": ignore }),
            )
            .await?;
        Ok(())
    }

    /// Stop watching a directory.
    pub async fn watch_stop(&self, path: &str) -> Result<(), TransportError> {
        self.transport
            .call("watch/stop", serde_json::json!({ "path": path }))
            .await?;
        Ok(())
    }

    /// Take the watch event receiver (can only be called once).
    pub async fn take_watch_rx(
        &self,
    ) -> Option<tokio::sync::mpsc::Receiver<WatchEvent>> {
        self.transport.take_watch_rx().await
    }

    // ═══════════════════════════════════════════════════════════════════
    // symbols/* operations
    // ═══════════════════════════════════════════════════════════════════

    /// Index all symbols in a directory (recursive).
    pub async fn symbol_index(
        &self,
        path: &str,
        max_files: Option<u32>,
    ) -> Result<SymbolIndexResult, TransportError> {
        let mut params = serde_json::json!({ "path": path });
        if let Some(mf) = max_files {
            params["max_files"] = serde_json::json!(mf);
        }
        let result = self.transport.call("symbols/index", params).await?;
        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    /// Autocomplete a symbol prefix.
    pub async fn symbol_complete(
        &self,
        path: &str,
        prefix: &str,
        limit: Option<u32>,
    ) -> Result<Vec<SymbolInfo>, TransportError> {
        let mut params = serde_json::json!({ "path": path, "prefix": prefix });
        if let Some(l) = limit {
            params["limit"] = serde_json::json!(l);
        }
        let result = self.transport.call("symbols/complete", params).await?;
        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    /// Find all definitions of a symbol by name.
    pub async fn symbol_definitions(
        &self,
        path: &str,
        name: &str,
    ) -> Result<Vec<SymbolInfo>, TransportError> {
        let result = self
            .transport
            .call(
                "symbols/definitions",
                serde_json::json!({ "path": path, "name": name }),
            )
            .await?;
        serde_json::from_value(result)
            .map_err(|e| TransportError::DeserializeError(e.to_string()))
    }

    // ═══════════════════════════════════════════════════════════════════
    // sys/* operations
    // ═══════════════════════════════════════════════════════════════════

    /// Ping the agent (health check).
    pub async fn ping(&self) -> Result<(), TransportError> {
        self.transport
            .call("sys/ping", serde_json::json!({}))
            .await?;
        Ok(())
    }

    /// Gracefully shut down the agent.
    pub async fn shutdown(&self) {
        self.transport.shutdown().await;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Registry
// ═══════════════════════════════════════════════════════════════════════════

/// Registry of active agent sessions, keyed by connection ID.
pub struct AgentRegistry {
    agents: DashMap<String, Arc<AgentSession>>,
}

impl AgentRegistry {
    /// Create a new agent registry.
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
        }
    }

    /// Register an agent session for a connection.
    pub fn register(&self, connection_id: String, session: AgentSession) {
        info!(
            "[agent-registry] Registered agent for connection {}: {}",
            connection_id,
            session.status()
        );
        self.agents.insert(connection_id, Arc::new(session));
    }

    /// Get an agent session by connection ID.
    pub fn get(&self, connection_id: &str) -> Option<Arc<AgentSession>> {
        self.agents.get(connection_id).map(|r| r.value().clone())
    }

    /// Check if a connection has an active agent.
    pub fn has_agent(&self, connection_id: &str) -> bool {
        self.agents
            .get(connection_id)
            .map(|a| a.is_alive())
            .unwrap_or(false)
    }

    /// Remove and shut down an agent session.
    pub async fn remove(&self, connection_id: &str) {
        if let Some((_, session)) = self.agents.remove(connection_id) {
            info!(
                "[agent-registry] Removing agent for connection {}",
                connection_id
            );
            session.shutdown().await;
        }
    }

    /// Remove all agent sessions (for app shutdown).
    pub async fn close_all(&self) {
        let keys: Vec<String> = self.agents.iter().map(|r| r.key().clone()).collect();
        for key in keys {
            self.remove(&key).await;
        }
        info!("[agent-registry] All agents shut down");
    }

    /// Get status for all connections.
    pub fn all_statuses(&self) -> Vec<(String, AgentStatus)> {
        self.agents
            .iter()
            .map(|r| (r.key().clone(), r.value().status()))
            .collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
