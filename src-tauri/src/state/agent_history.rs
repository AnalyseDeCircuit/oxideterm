// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Agent Task History persistence using redb (v2)
//!
//! Architecture:
//!   - task_meta_v2:   task_id → MessagePack(TaskMeta)        — lightweight metadata
//!   - task_steps_v2:  "task_id:NNNN" → zstd(JSON step)       — per-step storage
//!   - task_index_v2:  "index" → MessagePack(Vec<IndexEntry>)  — ordered index
//!   - checkpoint_v2:  "active" → zstd(JSON AgentTask)         — crash-recovery

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info, warn};

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Maximum tasks to keep (LRU eviction)
pub const MAX_TASKS: usize = 100;

/// Compression level for zstd (fast, reasonable ratio)
const ZSTD_LEVEL: i32 = 3;

/// Maximum steps stored per task
const MAX_STEPS_PER_TASK: usize = 500;

// ═══════════════════════════════════════════════════════════════════════════
// Table Definitions
// ═══════════════════════════════════════════════════════════════════════════

/// Table: task metadata (key: task_id, value: MessagePack TaskMeta)
const META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("task_meta_v2");

/// Table: task steps (key: "task_id:NNNN", value: zstd-compressed JSON step)
const STEPS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("task_steps_v2");

/// Table: ordered index (key: "index", value: MessagePack Vec<IndexEntry>)
const INDEX_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("task_index_v2");

/// Table: checkpoint for crash recovery (key: "active", value: zstd JSON)
const CHECKPOINT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("checkpoint_v2");

const INDEX_KEY: &str = "index";
const CHECKPOINT_KEY: &str = "active";

// ═══════════════════════════════════════════════════════════════════════════
// Data Structures
// ═══════════════════════════════════════════════════════════════════════════

/// Lightweight task metadata (no steps).
/// Serialized as MessagePack for compact storage without compression.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMeta {
    pub id: String,
    pub goal: String,
    pub status: String,
    pub autonomy_level: String,
    pub provider_id: String,
    pub model: String,
    pub current_round: u32,
    pub max_rounds: u32,
    pub created_at: f64,
    pub completed_at: Option<f64>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub step_count: u32,
    /// Plan description (if available)
    pub plan_description: Option<String>,
    /// Full plan JSON for plan reuse
    pub plan_json: Option<String>,
    /// Context tab type at task creation
    pub context_tab_type: Option<String>,
}

/// Compact index entry for fast listing without reading full metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskIndexEntry {
    pub id: String,
    /// First 100 chars of goal for display in list
    pub goal_preview: String,
    pub status: String,
    pub created_at: f64,
    pub completed_at: Option<f64>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Error Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum AgentHistoryError {
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),

    #[error("Table error: {0}")]
    Table(#[from] redb::TableError),

    #[error("Storage error: {0}")]
    Storage(#[from] redb::StorageError),

    #[error("Commit error: {0}")]
    Commit(#[from] redb::CommitError),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Task not found: {0}")]
    NotFound(String),
}

impl From<rmp_serde::encode::Error> for AgentHistoryError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        AgentHistoryError::Serialization(e.to_string())
    }
}

impl From<rmp_serde::decode::Error> for AgentHistoryError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        AgentHistoryError::Serialization(e.to_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Agent History Store
// ═══════════════════════════════════════════════════════════════════════════

/// Agent task history persistence store (v2 — metadata/steps separation)
pub struct AgentHistoryStore {
    db: Arc<Database>,
}

impl AgentHistoryStore {
    /// Open or create the agent history database at the given path
    pub fn new(path: PathBuf) -> Result<Self, AgentHistoryError> {
        let db = match Database::create(&path) {
            Ok(db) => {
                info!("Agent history database opened at {:?}", path);
                db
            }
            Err(e) => {
                warn!(
                    "Failed to open agent history database: {:?}, attempting recovery",
                    e
                );
                let backup_path = path.with_extension("redb.backup");
                if let Err(e) = std::fs::rename(&path, &backup_path) {
                    error!("Failed to backup corrupted agent history database: {:?}", e);
                } else {
                    info!(
                        "Backed up corrupted agent history database to {:?}",
                        backup_path
                    );
                }
                Database::create(&path)?
            }
        };

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            {
                warn!("Failed to set agent history database permissions: {:?}", e);
            }
        }

        // Initialize all tables
        let txn = db.begin_write()?;
        {
            let _ = txn.open_table(META_TABLE)?;
            let _ = txn.open_table(STEPS_TABLE)?;
            let _ = txn.open_table(INDEX_TABLE)?;
            let _ = txn.open_table(CHECKPOINT_TABLE)?;
        }
        txn.commit()?;

        Ok(Self { db: Arc::new(db) })
    }

    // ─── Task Metadata ───────────────────────────────────────────────────

    /// Save task metadata (without steps). Updates index.
    pub fn save_meta(&self, meta: &TaskMeta) -> Result<(), AgentHistoryError> {
        let meta_bytes = rmp_serde::to_vec(meta)?;

        let txn = self.db.begin_write()?;
        {
            let mut meta_table = txn.open_table(META_TABLE)?;
            meta_table.insert(meta.id.as_str(), meta_bytes.as_slice())?;

            // Update index
            let mut index_table = txn.open_table(INDEX_TABLE)?;
            let mut index = self.read_index_from_table(&index_table)?;

            // Remove existing entry (dedup)
            index.retain(|e| e.id != meta.id);
            index.insert(
                0,
                TaskIndexEntry {
                    id: meta.id.clone(),
                    goal_preview: truncate_str(&meta.goal, 100),
                    status: meta.status.clone(),
                    created_at: meta.created_at,
                    completed_at: meta.completed_at,
                },
            );

            // LRU eviction
            while index.len() > MAX_TASKS {
                if let Some(old) = index.pop() {
                    let _ = meta_table.remove(old.id.as_str());
                    // Clean up steps for evicted task
                    let mut steps_table = txn.open_table(STEPS_TABLE)?;
                    self.delete_steps_for_task(&mut steps_table, &old.id)?;
                }
            }

            let index_bytes = rmp_serde::to_vec(&index)?;
            index_table.insert(INDEX_KEY, index_bytes.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Update metadata for an existing task (e.g. status change, step_count increment).
    /// Does NOT update index ordering — use save_meta for new tasks.
    pub fn update_meta(&self, meta: &TaskMeta) -> Result<(), AgentHistoryError> {
        let meta_bytes = rmp_serde::to_vec(meta)?;

        let txn = self.db.begin_write()?;
        {
            let mut meta_table = txn.open_table(META_TABLE)?;
            meta_table.insert(meta.id.as_str(), meta_bytes.as_slice())?;

            // Update status in index entry
            let mut index_table = txn.open_table(INDEX_TABLE)?;
            let mut index = self.read_index_from_table(&index_table)?;
            if let Some(entry) = index.iter_mut().find(|e| e.id == meta.id) {
                entry.status = meta.status.clone();
                entry.completed_at = meta.completed_at;
            }
            let index_bytes = rmp_serde::to_vec(&index)?;
            index_table.insert(INDEX_KEY, index_bytes.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// List task metadata (newest first), up to `limit`.
    /// Optional filters: status, search query (substring match on goal).
    pub fn list_meta(
        &self,
        limit: usize,
        status_filter: Option<&str>,
        search_query: Option<&str>,
    ) -> Result<Vec<TaskMeta>, AgentHistoryError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let txn = self.db.begin_read()?;
        let index_table = txn.open_table(INDEX_TABLE)?;
        let meta_table = txn.open_table(META_TABLE)?;

        let index = self.read_index_from_table(&index_table)?;
        let query_lower = search_query.map(|q| q.to_lowercase());

        let mut results = Vec::new();
        for entry in &index {
            // Apply status filter
            if let Some(sf) = status_filter {
                if entry.status != sf {
                    continue;
                }
            }
            // Apply search query (substring on goal preview)
            if let Some(ref q) = query_lower {
                if !entry.goal_preview.to_lowercase().contains(q) {
                    continue;
                }
            }

            // Load full metadata
            match meta_table.get(entry.id.as_str())? {
                Some(data) => match rmp_serde::from_slice::<TaskMeta>(data.value()) {
                    Ok(meta) => {
                        // Double-check search against full goal (not just preview)
                        if let Some(ref q) = query_lower {
                            if !meta.goal.to_lowercase().contains(q) {
                                continue;
                            }
                        }
                        results.push(meta);
                    }
                    Err(e) => warn!("Skipping task {} (deserialization error): {}", entry.id, e),
                },
                None => warn!("Task {} in index but not in meta table", entry.id),
            }

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    /// Get a single task's metadata by ID.
    pub fn get_meta(&self, task_id: &str) -> Result<TaskMeta, AgentHistoryError> {
        let txn = self.db.begin_read()?;
        let meta_table = txn.open_table(META_TABLE)?;
        let entry = meta_table
            .get(task_id)?
            .ok_or_else(|| AgentHistoryError::NotFound(task_id.to_string()))?;
        let meta: TaskMeta = rmp_serde::from_slice(entry.value())?;
        Ok(meta)
    }

    // ─── Steps ───────────────────────────────────────────────────────────

    /// Append a single step to a task.
    pub fn append_step(
        &self,
        task_id: &str,
        step_index: u32,
        step_json: &str,
    ) -> Result<(), AgentHistoryError> {
        if step_index as usize >= MAX_STEPS_PER_TASK {
            return Ok(()); // silently drop steps beyond limit
        }

        let compressed = zstd::encode_all(step_json.as_bytes(), ZSTD_LEVEL)
            .map_err(|e| AgentHistoryError::Compression(format!("zstd encode failed: {}", e)))?;

        let key = step_key(task_id, step_index);
        let txn = self.db.begin_write()?;
        {
            let mut steps = txn.open_table(STEPS_TABLE)?;
            steps.insert(key.as_str(), compressed.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Save multiple steps at once (for bulk save after task completion).
    pub fn save_steps(
        &self,
        task_id: &str,
        steps_json: &[String],
    ) -> Result<(), AgentHistoryError> {
        let txn = self.db.begin_write()?;
        {
            let mut steps_table = txn.open_table(STEPS_TABLE)?;

            for (i, step_json) in steps_json.iter().enumerate() {
                if i >= MAX_STEPS_PER_TASK {
                    break;
                }
                let compressed =
                    zstd::encode_all(step_json.as_bytes(), ZSTD_LEVEL).map_err(|e| {
                        AgentHistoryError::Compression(format!("zstd encode failed: {}", e))
                    })?;
                let key = step_key(task_id, i as u32);
                steps_table.insert(key.as_str(), compressed.as_slice())?;
            }
        }
        txn.commit()?;
        Ok(())
    }

    /// Get steps for a task with pagination (offset + limit).
    pub fn get_steps(
        &self,
        task_id: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<String>, AgentHistoryError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let txn = self.db.begin_read()?;
        let steps_table = txn.open_table(STEPS_TABLE)?;

        let prefix = format!("{}:", task_id);
        let mut results = Vec::new();
        let mut skipped = 0u32;

        // Iterate using range scan on the key prefix
        let start_key = step_key(task_id, 0);
        // End key: task_id followed by a char after ':'
        let end_key = format!("{};", task_id); // ';' > ':' in ASCII

        let range = steps_table.range(start_key.as_str()..end_key.as_str())?;
        for entry in range {
            let (key_guard, value_guard) = entry?;
            let key = key_guard.value();
            if !key.starts_with(&prefix) {
                break;
            }

            if skipped < offset {
                skipped += 1;
                continue;
            }

            match zstd::decode_all(value_guard.value()) {
                Ok(decompressed) => match String::from_utf8(decompressed) {
                    Ok(json) => results.push(json),
                    Err(e) => warn!("Skipping step {} (UTF-8 error): {}", key, e),
                },
                Err(e) => warn!("Skipping step {} (decompression error): {}", key, e),
            }

            if results.len() >= limit as usize {
                break;
            }
        }

        Ok(results)
    }

    // ─── Checkpoint ──────────────────────────────────────────────────────

    /// Save a checkpoint of the running task (for crash recovery).
    pub fn save_checkpoint(&self, task_json: &str) -> Result<(), AgentHistoryError> {
        let compressed = zstd::encode_all(task_json.as_bytes(), ZSTD_LEVEL)
            .map_err(|e| AgentHistoryError::Compression(format!("zstd encode failed: {}", e)))?;

        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(CHECKPOINT_TABLE)?;
            table.insert(CHECKPOINT_KEY, compressed.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Load checkpoint (if any). Returns None if no checkpoint exists.
    pub fn load_checkpoint(&self) -> Result<Option<String>, AgentHistoryError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(CHECKPOINT_TABLE)?;
        match table.get(CHECKPOINT_KEY)? {
            Some(entry) => {
                let decompressed = zstd::decode_all(entry.value()).map_err(|e| {
                    AgentHistoryError::Compression(format!("zstd decode failed: {}", e))
                })?;
                let json = String::from_utf8(decompressed).map_err(|e| {
                    AgentHistoryError::Compression(format!("UTF-8 decode failed: {}", e))
                })?;
                Ok(Some(json))
            }
            None => Ok(None),
        }
    }

    /// Clear the checkpoint (after clean task completion).
    pub fn clear_checkpoint(&self) -> Result<(), AgentHistoryError> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(CHECKPOINT_TABLE)?;
            let _ = table.remove(CHECKPOINT_KEY);
        }
        txn.commit()?;
        Ok(())
    }

    // ─── Delete / Clear ──────────────────────────────────────────────────

    /// Delete a single task (metadata + all steps).
    pub fn delete_task(&self, task_id: &str) -> Result<(), AgentHistoryError> {
        let txn = self.db.begin_write()?;
        {
            let mut meta_table = txn.open_table(META_TABLE)?;
            meta_table.remove(task_id)?;

            let mut steps_table = txn.open_table(STEPS_TABLE)?;
            self.delete_steps_for_task(&mut steps_table, task_id)?;

            let mut index_table = txn.open_table(INDEX_TABLE)?;
            let mut index = self.read_index_from_table(&index_table)?;
            index.retain(|e| e.id != task_id);
            let index_bytes = rmp_serde::to_vec(&index)?;
            index_table.insert(INDEX_KEY, index_bytes.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Clear all tasks, steps, and checkpoint.
    pub fn clear(&self) -> Result<(), AgentHistoryError> {
        let txn = self.db.begin_write()?;
        {
            let index_table = txn.open_table(INDEX_TABLE)?;
            let index = self.read_index_from_table(&index_table)?;
            drop(index_table);

            let mut meta_table = txn.open_table(META_TABLE)?;
            for entry in &index {
                let _ = meta_table.remove(entry.id.as_str());
            }

            let mut steps_table = txn.open_table(STEPS_TABLE)?;
            for entry in &index {
                self.delete_steps_for_task(&mut steps_table, &entry.id)?;
            }

            let mut index_table = txn.open_table(INDEX_TABLE)?;
            let empty: Vec<TaskIndexEntry> = Vec::new();
            let index_bytes = rmp_serde::to_vec(&empty)?;
            index_table.insert(INDEX_KEY, index_bytes.as_slice())?;

            let mut cp_table = txn.open_table(CHECKPOINT_TABLE)?;
            let _ = cp_table.remove(CHECKPOINT_KEY);
        }
        txn.commit()?;
        info!("Agent history cleared (v2)");
        Ok(())
    }

    // ─── Internal helpers ────────────────────────────────────────────────

    fn read_index_from_table<T: ReadableTable<&'static str, &'static [u8]>>(
        &self,
        table: &T,
    ) -> Result<Vec<TaskIndexEntry>, AgentHistoryError> {
        match table.get(INDEX_KEY)? {
            Some(entry) => {
                let index: Vec<TaskIndexEntry> = rmp_serde::from_slice(entry.value())?;
                Ok(index)
            }
            None => Ok(Vec::new()),
        }
    }

    fn delete_steps_for_task(
        &self,
        steps_table: &mut redb::Table<&str, &[u8]>,
        task_id: &str,
    ) -> Result<(), AgentHistoryError> {
        // Delete step keys across the whole bounded range. Do not stop at the
        // first miss because sparse indices can exist after interrupted writes
        // or malformed callers, and we must not leave orphan step rows behind.
        for i in 0..MAX_STEPS_PER_TASK {
            let key = step_key(task_id, i as u32);
            match steps_table.remove(key.as_str()) {
                Ok(_) => {}
                Err(_) => continue,
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Build a step key: "task_id:0042"  (zero-padded for lexicographic ordering)
fn step_key(task_id: &str, index: u32) -> String {
    format!("{}:{:04}", task_id, index)
}

/// Truncate a string to `max_chars` (char boundary safe).
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}…", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_store() -> (TempDir, AgentHistoryStore) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("agent_history.redb");
        let store = AgentHistoryStore::new(db_path).unwrap();
        (temp_dir, store)
    }

    fn sample_meta(task_id: &str) -> TaskMeta {
        TaskMeta {
            id: task_id.to_string(),
            goal: format!("goal for {task_id}"),
            status: "completed".to_string(),
            autonomy_level: "balanced".to_string(),
            provider_id: "provider-1".to_string(),
            model: "model-1".to_string(),
            current_round: 3,
            max_rounds: 10,
            created_at: 1.0,
            completed_at: Some(2.0),
            summary: Some("done".to_string()),
            error: None,
            step_count: 1,
            plan_description: None,
            plan_json: None,
            context_tab_type: None,
        }
    }

    #[test]
    fn zero_limits_return_empty_results() {
        let (_temp_dir, store) = create_store();
        store.save_meta(&sample_meta("task-1")).unwrap();
        store.append_step("task-1", 0, r#"{"id":"s1"}"#).unwrap();

        let metas = store.list_meta(0, None, None).unwrap();
        let steps = store.get_steps("task-1", 0, 0).unwrap();

        assert!(metas.is_empty());
        assert!(steps.is_empty());
    }

    #[test]
    fn delete_task_removes_sparse_steps() {
        let (_temp_dir, store) = create_store();
        store.save_meta(&sample_meta("task-2")).unwrap();
        store
            .append_step("task-2", 3, r#"{"id":"late-step"}"#)
            .unwrap();

        assert_eq!(store.get_steps("task-2", 0, 10).unwrap().len(), 1);

        store.delete_task("task-2").unwrap();

        assert!(matches!(
            store.get_meta("task-2"),
            Err(AgentHistoryError::NotFound(_))
        ));
        assert!(store.get_steps("task-2", 0, 10).unwrap().is_empty());
    }
}
