// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::rag::error::RagError;
use crate::rag::hnsw::{HnswLoadOutcome, PersistedHnswIndex, hnsw_index_path};
use crate::rag::types::*;
use redb::{Database, ReadableTable, TableDefinition};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;
use tracing::{debug, info, warn};

// ═══════════════════════════════════════════════════════════════════════════
// Table Definitions
// ═══════════════════════════════════════════════════════════════════════════

const COLLECTIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("doc_collections");
const COLLECTION_DOCS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("collection_docs");
const DOC_METADATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("doc_metadata");
const DOC_CHUNKS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("doc_chunks");
const DOC_CHUNK_INDEX_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("doc_chunk_index");
const BM25_POSTINGS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("bm25_postings");
const BM25_META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("bm25_meta");
const BM25_CONTENT_REVISION_KEY: &str = "content_revision";
const BM25_INDEXED_REVISION_KEY: &str = "indexed_revision";
const EMBEDDINGS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("embeddings");
const DOC_RAW_CONTENT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("doc_raw_content");

/// Compression threshold: compress chunks larger than 4 KB.
const COMPRESSION_THRESHOLD: usize = 4096;
const LOCK_PROFILE_ENV: &str = "OXIDETERM_PROFILE_LOCKS";

fn rag_lock_profiling_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| cfg!(debug_assertions) && std::env::var_os(LOCK_PROFILE_ENV).is_some())
}

fn lock_with_diagnostics<'a, T>(
    mutex: &'a Mutex<T>,
    context: &'static str,
) -> Result<MutexGuard<'a, T>, RagError> {
    let started = Instant::now();
    let guard = mutex
        .lock()
        .map_err(|e| RagError::HnswIndex(format!("lock poisoned: {e}")))?;
    if rag_lock_profiling_enabled() {
        debug!(
            "[RAG] lock wait context={} waited_us={}",
            context,
            started.elapsed().as_micros()
        );
    }
    Ok(guard)
}

// ═══════════════════════════════════════════════════════════════════════════
// RagStore
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HnswIndexStatus {
    Unloaded,
    Loading,
    Ready {
        point_count: usize,
        dimensions: usize,
    },
    Stale,
    Missing,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bm25IndexStatus {
    Ready,
    Pending,
    Rebuilding,
    Failed(String),
}

struct Bm25RuntimeState {
    status: Bm25IndexStatus,
    worker_running: bool,
}

struct Bm25Runtime {
    state: Mutex<Bm25RuntimeState>,
    changed: Condvar,
    operation: Mutex<()>,
    requested_generation: AtomicU64,
    #[cfg(test)]
    fail_next_rebuild: AtomicBool,
}

impl Bm25Runtime {
    fn new() -> Self {
        Self {
            state: Mutex::new(Bm25RuntimeState {
                status: Bm25IndexStatus::Ready,
                worker_running: false,
            }),
            changed: Condvar::new(),
            operation: Mutex::new(()),
            requested_generation: AtomicU64::new(0),
            #[cfg(test)]
            fail_next_rebuild: AtomicBool::new(false),
        }
    }
}

enum HnswRuntimeState {
    Unloaded,
    Loading,
    Ready(Arc<PersistedHnswIndex>),
    Stale,
    Missing,
    Failed(String),
}

struct HnswRuntime {
    state: Mutex<HnswRuntimeState>,
    ready: Condvar,
    operation: Mutex<()>,
    generation: AtomicU64,
}

impl HnswRuntime {
    fn new() -> Self {
        Self {
            state: Mutex::new(HnswRuntimeState::Unloaded),
            ready: Condvar::new(),
            operation: Mutex::new(()),
            generation: AtomicU64::new(0),
        }
    }
}

pub struct RagStore {
    db: Arc<Database>,
    data_dir: PathBuf,
    hnsw_index: Arc<HnswRuntime>,
    bm25_index: Arc<Bm25Runtime>,
}

impl Clone for RagStore {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            data_dir: self.data_dir.clone(),
            hnsw_index: self.hnsw_index.clone(),
            bm25_index: self.bm25_index.clone(),
        }
    }
}

impl RagStore {
    fn bump_bm25_content_revision(txn: &redb::WriteTransaction) -> Result<u64, RagError> {
        let mut bm25_meta = txn.open_table(BM25_META_TABLE)?;
        let revision = bm25_meta
            .get(BM25_CONTENT_REVISION_KEY)?
            .map(|value| value.value().to_vec())
            .map(|bytes| rmp_serde::from_slice::<u64>(&bytes))
            .transpose()?
            .unwrap_or_default()
            .wrapping_add(1);
        bm25_meta.insert(
            BM25_CONTENT_REVISION_KEY,
            rmp_serde::to_vec(&revision)?.as_slice(),
        )?;
        Ok(revision)
    }

    fn hnsw_operation_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, RagError> {
        lock_with_diagnostics(&self.hnsw_index.operation, "hnsw.operation")
    }

    fn set_hnsw_failed_state(&self, message: String) {
        match lock_with_diagnostics(&self.hnsw_index.state, "hnsw.set_failed_state") {
            Ok(mut guard) => {
                *guard = HnswRuntimeState::Failed(message);
                self.hnsw_index.ready.notify_all();
            }
            Err(e) => {
                warn!("Failed to set HNSW failure state (lock poisoned): {}", e);
            }
        }
    }

    fn remove_hnsw_index_file(&self) -> Result<(), RagError> {
        let hnsw_path = hnsw_index_path(&self.data_dir);
        match std::fs::remove_file(&hnsw_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(RagError::HnswIndex(format!(
                "failed to remove HNSW index file {:?}: {}",
                hnsw_path, e
            ))),
        }
    }

    /// Open (or create) the RAG index database.
    pub fn new(data_dir: &Path) -> Result<Self, RagError> {
        let db_path = data_dir.join("rag_index.redb");
        info!("Opening RAG index database at {:?}", db_path);

        let db = Database::create(&db_path).map_err(|e| {
            warn!("Failed to open RAG DB, attempting backup recovery: {}", e);
            e
        })?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600));
        }

        // Initialise all tables (no-op if they already exist)
        let txn = db.begin_write()?;
        {
            let _ = txn.open_table(COLLECTIONS_TABLE)?;
            let _ = txn.open_table(COLLECTION_DOCS_TABLE)?;
            let _ = txn.open_table(DOC_METADATA_TABLE)?;
            let _ = txn.open_table(DOC_CHUNKS_TABLE)?;
            let _ = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
            let _ = txn.open_table(BM25_POSTINGS_TABLE)?;
            let _ = txn.open_table(BM25_META_TABLE)?;
            let _ = txn.open_table(EMBEDDINGS_TABLE)?;
            let _ = txn.open_table(DOC_RAW_CONTENT_TABLE)?;
        }
        txn.commit()?;

        let store = Self {
            db: Arc::new(db),
            data_dir: data_dir.to_path_buf(),
            hnsw_index: Arc::new(HnswRuntime::new()),
            bm25_index: Arc::new(Bm25Runtime::new()),
        };
        if store.bm25_rebuild_required()?
            && let Err(error) = store.queue_bm25_rebuild()
        {
            warn!("Failed to resume pending BM25 rebuild: {}", error);
        }
        Ok(store)
    }

    pub fn ensure_hnsw_loaded(&self) -> Result<Option<Arc<PersistedHnswIndex>>, RagError> {
        let load_generation;
        let mut state =
            lock_with_diagnostics(&self.hnsw_index.state, "hnsw.ensure_loaded.initial")?;

        loop {
            match &*state {
                HnswRuntimeState::Ready(index) => return Ok(Some(index.clone())),
                HnswRuntimeState::Stale
                | HnswRuntimeState::Missing
                | HnswRuntimeState::Failed(_) => return Ok(None),
                HnswRuntimeState::Loading => {
                    state = self
                        .hnsw_index
                        .ready
                        .wait(state)
                        .map_err(|e| RagError::HnswIndex(format!("lock poisoned: {e}")))?;
                }
                HnswRuntimeState::Unloaded => {
                    *state = HnswRuntimeState::Loading;
                    load_generation = self.hnsw_index.generation.load(Ordering::SeqCst);
                    break;
                }
            }
        }

        drop(state);

        let _operation_guard = self.hnsw_operation_lock()?;

        let load_result = PersistedHnswIndex::load_detailed(&hnsw_index_path(&self.data_dir));

        let mut state =
            lock_with_diagnostics(&self.hnsw_index.state, "hnsw.ensure_loaded.resolve")?;

        let resolved = match (&*state, load_result) {
            (HnswRuntimeState::Loading, HnswLoadOutcome::Loaded(index))
                if self.hnsw_index.generation.load(Ordering::SeqCst) == load_generation =>
            {
                let index = Arc::new(index);
                *state = HnswRuntimeState::Ready(index.clone());
                Some(index)
            }
            (HnswRuntimeState::Loading, HnswLoadOutcome::Missing)
                if self.hnsw_index.generation.load(Ordering::SeqCst) == load_generation =>
            {
                *state = HnswRuntimeState::Missing;
                None
            }
            (HnswRuntimeState::Loading, HnswLoadOutcome::Failed(error))
                if self.hnsw_index.generation.load(Ordering::SeqCst) == load_generation =>
            {
                *state = HnswRuntimeState::Failed(error);
                None
            }
            (HnswRuntimeState::Ready(index), _) => Some(index.clone()),
            _ => None,
        };

        self.hnsw_index.ready.notify_all();
        Ok(resolved)
    }

    pub fn hnsw_status(&self) -> HnswIndexStatus {
        match lock_with_diagnostics(&self.hnsw_index.state, "hnsw.status") {
            Ok(state) => match &*state {
                HnswRuntimeState::Unloaded => HnswIndexStatus::Unloaded,
                HnswRuntimeState::Loading => HnswIndexStatus::Loading,
                HnswRuntimeState::Ready(index) => HnswIndexStatus::Ready {
                    point_count: index.meta.point_count,
                    dimensions: index.meta.dimensions,
                },
                HnswRuntimeState::Stale => HnswIndexStatus::Stale,
                HnswRuntimeState::Missing => HnswIndexStatus::Missing,
                HnswRuntimeState::Failed(error) => HnswIndexStatus::Failed(error.clone()),
            },
            Err(e) => HnswIndexStatus::Failed(format!("lock poisoned: {e}")),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Collections
    // ═══════════════════════════════════════════════════════════════════════

    pub fn create_collection(&self, collection: &DocCollection) -> Result<(), RagError> {
        let data = rmp_serde::to_vec(collection)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(COLLECTIONS_TABLE)?;
            t.insert(collection.id.as_str(), data.as_slice())?;
            // Initialise empty doc list
            let mut cd = txn.open_table(COLLECTION_DOCS_TABLE)?;
            let empty: Vec<String> = Vec::new();
            cd.insert(
                collection.id.as_str(),
                rmp_serde::to_vec(&empty)?.as_slice(),
            )?;
        }
        txn.commit()?;
        debug!("Created collection: {}", collection.id);
        Ok(())
    }

    pub fn get_collection(&self, collection_id: &str) -> Result<DocCollection, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(COLLECTIONS_TABLE)?;
        match t.get(collection_id)? {
            Some(guard) => Ok(rmp_serde::from_slice(guard.value())?),
            None => Err(RagError::CollectionNotFound(collection_id.to_string())),
        }
    }

    pub fn list_collections(
        &self,
        scope_filter: Option<&str>,
    ) -> Result<Vec<DocCollection>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(COLLECTIONS_TABLE)?;
        let mut result = Vec::new();
        for entry in t.iter()? {
            let (_, v) = entry?;
            let col: DocCollection = rmp_serde::from_slice(v.value())?;
            match scope_filter {
                Some("global") => {
                    if col.scope == DocScope::Global {
                        result.push(col);
                    }
                }
                Some(conn_id) => {
                    if col.scope == DocScope::Global
                        || col.scope == DocScope::Connection(conn_id.to_string())
                    {
                        result.push(col);
                    }
                }
                None => result.push(col),
            }
        }
        Ok(result)
    }

    pub fn delete_collection(&self, collection_id: &str) -> Result<(), RagError> {
        // First gather all doc_ids in this collection
        let doc_ids = self.get_collection_doc_ids(collection_id)?;

        let txn = self.db.begin_write()?;
        {
            // Remove all chunks, chunk indices, embeddings for each doc
            let mut chunks_t = txn.open_table(DOC_CHUNKS_TABLE)?;
            let mut idx_t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
            let mut emb_t = txn.open_table(EMBEDDINGS_TABLE)?;
            let mut meta_t = txn.open_table(DOC_METADATA_TABLE)?;
            let mut raw_t = txn.open_table(DOC_RAW_CONTENT_TABLE)?;

            for doc_id in &doc_ids {
                // Get chunk ids for this doc
                let chunk_ids_data = idx_t.get(doc_id.as_str())?.map(|g| g.value().to_vec());
                if let Some(data) = chunk_ids_data {
                    let chunk_ids: Vec<String> = rmp_serde::from_slice(&data)?;
                    for cid in &chunk_ids {
                        let _ = chunks_t.remove(cid.as_str())?;
                        let _ = emb_t.remove(cid.as_str())?;
                    }
                }
                let _ = idx_t.remove(doc_id.as_str())?;
                let _ = meta_t.remove(doc_id.as_str())?;
                let _ = raw_t.remove(doc_id.as_str())?;
            }

            // Remove collection and its doc list
            let mut col_t = txn.open_table(COLLECTIONS_TABLE)?;
            col_t.remove(collection_id)?;
            let mut cd_t = txn.open_table(COLLECTION_DOCS_TABLE)?;
            cd_t.remove(collection_id)?;
            if !doc_ids.is_empty() {
                Self::bump_bm25_content_revision(&txn)?;
            }
        }
        txn.commit()?;

        // BM25 postings will be cleaned up via reindex
        info!(
            "Deleted collection {} ({} docs)",
            collection_id,
            doc_ids.len()
        );

        // Invalidate HNSW index since embeddings were removed
        if let Err(e) = self.invalidate_hnsw_index() {
            warn!(
                "Collection {} deleted but HNSW invalidation failed: {}",
                collection_id, e
            );
        }

        Ok(())
    }

    pub fn get_collection_stats(&self, collection_id: &str) -> Result<CollectionStats, RagError> {
        let col = self.get_collection(collection_id)?;
        let doc_ids = self.get_collection_doc_ids(collection_id)?;
        let mut chunk_count = 0usize;
        let mut embedded_count = 0usize;

        let txn = self.db.begin_read()?;
        let idx_t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
        let emb_t = txn.open_table(EMBEDDINGS_TABLE)?;

        for doc_id in &doc_ids {
            if let Some(guard) = idx_t.get(doc_id.as_str())? {
                let cids: Vec<String> = rmp_serde::from_slice(guard.value())?;
                chunk_count += cids.len();
                for cid in &cids {
                    if emb_t.get(cid.as_str())?.is_some() {
                        embedded_count += 1;
                    }
                }
            }
        }

        Ok(CollectionStats {
            doc_count: doc_ids.len(),
            chunk_count,
            embedded_chunk_count: embedded_count,
            last_updated: col.updated_at,
        })
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Documents
    // ═══════════════════════════════════════════════════════════════════════

    /// Check whether a document with the given content hash already exists
    /// in the specified collection. Used for content deduplication.
    pub fn check_content_hash_exists(
        &self,
        collection_id: &str,
        content_hash: &str,
    ) -> Result<bool, RagError> {
        let doc_ids = self.get_collection_doc_ids(collection_id)?;
        let txn = self.db.begin_read()?;
        let meta_t = txn.open_table(DOC_METADATA_TABLE)?;

        for doc_id in &doc_ids {
            if let Some(guard) = meta_t.get(doc_id.as_str())? {
                let meta: DocMetadata = rmp_serde::from_slice(guard.value())?;
                if meta.content_hash == content_hash {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Check whether a matching content hash exists in the collection, excluding
    /// the provided document ID. Used when updating a document so the current
    /// document does not trip the duplicate-content guard.
    pub fn check_content_hash_exists_excluding_doc(
        &self,
        collection_id: &str,
        content_hash: &str,
        excluded_doc_id: &str,
    ) -> Result<bool, RagError> {
        let doc_ids = self.get_collection_doc_ids(collection_id)?;
        let txn = self.db.begin_read()?;
        let meta_t = txn.open_table(DOC_METADATA_TABLE)?;

        for doc_id in &doc_ids {
            if doc_id == excluded_doc_id {
                continue;
            }
            if let Some(guard) = meta_t.get(doc_id.as_str())? {
                let meta: DocMetadata = rmp_serde::from_slice(guard.value())?;
                if meta.content_hash == content_hash {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Add a document (metadata + chunks) and update the collection's doc list.
    pub fn add_document(
        &self,
        metadata: &DocMetadata,
        chunks: &[DocChunk],
        raw_content: Option<&str>,
    ) -> Result<(), RagError> {
        let meta_bytes = rmp_serde::to_vec(metadata)?;
        let chunk_ids: Vec<String> = chunks.iter().map(|c| c.id.clone()).collect();
        let chunk_idx_bytes = rmp_serde::to_vec(&chunk_ids)?;

        let txn = self.db.begin_write()?;
        {
            // Validate the parent inside the same write transaction that inserts the document.
            // A preflight read would still allow a concurrent collection deletion to create an
            // unreachable document and a dangling collection-docs entry.
            let mut collections_t = txn.open_table(COLLECTIONS_TABLE)?;
            let collection_bytes = collections_t
                .get(metadata.collection_id.as_str())?
                .map(|guard| guard.value().to_vec())
                .ok_or_else(|| RagError::CollectionNotFound(metadata.collection_id.clone()))?;

            // Store metadata
            let mut meta_t = txn.open_table(DOC_METADATA_TABLE)?;
            meta_t.insert(metadata.id.as_str(), meta_bytes.as_slice())?;

            // Store chunk index
            let mut idx_t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
            idx_t.insert(metadata.id.as_str(), chunk_idx_bytes.as_slice())?;

            // Store each chunk (with optional compression)
            let mut chunks_t = txn.open_table(DOC_CHUNKS_TABLE)?;
            for chunk in chunks {
                let raw = rmp_serde::to_vec(chunk)?;
                let stored = if raw.len() > COMPRESSION_THRESHOLD {
                    let mut compressed = vec![1u8]; // prefix: 1 = compressed
                    let body = zstd::encode_all(raw.as_slice(), 3)
                        .map_err(|e| RagError::Compression(e.to_string()))?;
                    compressed.extend_from_slice(&body);
                    compressed
                } else {
                    let mut uncompressed = vec![0u8]; // prefix: 0 = raw
                    uncompressed.extend_from_slice(&raw);
                    uncompressed
                };
                chunks_t.insert(chunk.id.as_str(), stored.as_slice())?;
            }

            // Store raw document content (for editing)
            if let Some(content) = raw_content {
                let mut raw_t = txn.open_table(DOC_RAW_CONTENT_TABLE)?;
                let stored = Self::compress_bytes(content.as_bytes());
                raw_t.insert(metadata.id.as_str(), stored.as_slice())?;
            }

            // Append doc_id to collection's doc list
            let mut cd_t = txn.open_table(COLLECTION_DOCS_TABLE)?;
            let cd_data = cd_t
                .get(metadata.collection_id.as_str())?
                .map(|g| g.value().to_vec());
            let mut doc_ids: Vec<String> = match cd_data {
                Some(data) => rmp_serde::from_slice(&data)?,
                None => Vec::new(),
            };
            if !doc_ids.contains(&metadata.id) {
                doc_ids.push(metadata.id.clone());
            }
            cd_t.insert(
                metadata.collection_id.as_str(),
                rmp_serde::to_vec(&doc_ids)?.as_slice(),
            )?;

            // Update collection timestamp
            let mut collection: DocCollection = rmp_serde::from_slice(&collection_bytes)?;
            collection.updated_at = metadata.indexed_at;
            collections_t.insert(
                metadata.collection_id.as_str(),
                rmp_serde::to_vec(&collection)?.as_slice(),
            )?;
            if !chunks.is_empty() {
                Self::bump_bm25_content_revision(&txn)?;
            }
        }
        txn.commit()?;
        debug!(
            "Added document {} ({} chunks) to collection {}",
            metadata.id,
            chunks.len(),
            metadata.collection_id
        );
        Ok(())
    }

    pub fn remove_document(&self, doc_id: &str) -> Result<(), RagError> {
        // Read metadata to get collection_id
        let meta = self
            .get_doc_metadata(doc_id)?
            .ok_or_else(|| RagError::DocumentNotFound(doc_id.to_string()))?;
        let chunk_ids = self.get_chunk_ids_for_doc(doc_id)?;

        let txn = self.db.begin_write()?;
        {
            let mut chunks_t = txn.open_table(DOC_CHUNKS_TABLE)?;
            let mut emb_t = txn.open_table(EMBEDDINGS_TABLE)?;
            for cid in &chunk_ids {
                let _ = chunks_t.remove(cid.as_str())?;
                let _ = emb_t.remove(cid.as_str())?;
            }

            let mut idx_t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
            let _ = idx_t.remove(doc_id)?;

            let mut meta_t = txn.open_table(DOC_METADATA_TABLE)?;
            let _ = meta_t.remove(doc_id)?;

            let mut raw_t = txn.open_table(DOC_RAW_CONTENT_TABLE)?;
            let _ = raw_t.remove(doc_id)?;

            // Remove from collection doc list
            let mut cd_t = txn.open_table(COLLECTION_DOCS_TABLE)?;
            let cd_data = cd_t
                .get(meta.collection_id.as_str())?
                .map(|g| g.value().to_vec());
            if let Some(bytes) = cd_data {
                let mut ids: Vec<String> = rmp_serde::from_slice(&bytes)?;
                ids.retain(|id| id != doc_id);
                cd_t.insert(
                    meta.collection_id.as_str(),
                    rmp_serde::to_vec(&ids)?.as_slice(),
                )?;
            }
            if !chunk_ids.is_empty() {
                Self::bump_bm25_content_revision(&txn)?;
            }
        }
        txn.commit()?;
        debug!("Removed document {}", doc_id);

        // Invalidate HNSW index since embeddings changed
        if let Err(e) = self.invalidate_hnsw_index() {
            warn!(
                "Document {} removed but HNSW invalidation failed: {}",
                doc_id, e
            );
        }

        Ok(())
    }

    pub fn get_doc_metadata(&self, doc_id: &str) -> Result<Option<DocMetadata>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOC_METADATA_TABLE)?;
        match t.get(doc_id)? {
            Some(guard) => Ok(Some(rmp_serde::from_slice(guard.value())?)),
            None => Ok(None),
        }
    }

    pub fn get_chunk(&self, chunk_id: &str) -> Result<Option<DocChunk>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOC_CHUNKS_TABLE)?;
        match t.get(chunk_id)? {
            Some(guard) => {
                let raw = guard.value();
                let chunk = self.decode_chunk(raw)?;
                Ok(Some(chunk))
            }
            None => Ok(None),
        }
    }

    /// Batch-load multiple chunks in a single read transaction.
    pub fn get_chunks_batch(
        &self,
        chunk_ids: &[String],
    ) -> Result<HashMap<String, DocChunk>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOC_CHUNKS_TABLE)?;
        let mut result = HashMap::with_capacity(chunk_ids.len());
        for cid in chunk_ids {
            if let Some(guard) = t.get(cid.as_str())? {
                let chunk = self.decode_chunk(guard.value())?;
                result.insert(cid.clone(), chunk);
            }
        }
        Ok(result)
    }

    /// Batch-load multiple document metadata in a single read transaction.
    pub fn get_doc_metadata_batch(
        &self,
        doc_ids: &[String],
    ) -> Result<HashMap<String, DocMetadata>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOC_METADATA_TABLE)?;
        let mut result = HashMap::with_capacity(doc_ids.len());
        for did in doc_ids {
            if let Some(guard) = t.get(did.as_str())? {
                let meta: DocMetadata = rmp_serde::from_slice(guard.value())?;
                result.insert(did.clone(), meta);
            }
        }
        Ok(result)
    }

    pub fn get_chunks_for_doc(&self, doc_id: &str) -> Result<Vec<DocChunk>, RagError> {
        let chunk_ids = self.get_chunk_ids_for_doc(doc_id)?;
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOC_CHUNKS_TABLE)?;
        let mut result = Vec::with_capacity(chunk_ids.len());
        for cid in &chunk_ids {
            if let Some(guard) = t.get(cid.as_str())? {
                result.push(self.decode_chunk(guard.value())?);
            }
        }
        Ok(result)
    }

    /// Get all chunks across a set of collections (for search scoping).
    pub fn get_chunk_ids_in_collections(
        &self,
        collection_ids: &[String],
    ) -> Result<Vec<String>, RagError> {
        let mut all_chunk_ids = Vec::new();
        let txn = self.db.begin_read()?;
        let cd_t = txn.open_table(COLLECTION_DOCS_TABLE)?;
        let idx_t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;

        for col_id in collection_ids {
            if let Some(guard) = cd_t.get(col_id.as_str())? {
                let doc_ids: Vec<String> = rmp_serde::from_slice(guard.value())?;
                for doc_id in &doc_ids {
                    if let Some(idx_guard) = idx_t.get(doc_id.as_str())? {
                        let cids: Vec<String> = rmp_serde::from_slice(idx_guard.value())?;
                        all_chunk_ids.extend(cids);
                    }
                }
            }
        }
        Ok(all_chunk_ids)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Embeddings
    // ═══════════════════════════════════════════════════════════════════════

    pub fn store_embedding(&self, record: &EmbeddingRecord) -> Result<(), RagError> {
        let data = rmp_serde::to_vec(record)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(EMBEDDINGS_TABLE)?;
            t.insert(record.chunk_id.as_str(), data.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn store_embeddings_batch(&self, records: &[EmbeddingRecord]) -> Result<(), RagError> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(EMBEDDINGS_TABLE)?;
            for record in records {
                let data = rmp_serde::to_vec(record)?;
                t.insert(record.chunk_id.as_str(), data.as_slice())?;
            }
        }
        txn.commit()?;
        debug!("Stored {} embeddings", records.len());
        Ok(())
    }

    pub fn get_embedding(&self, chunk_id: &str) -> Result<Option<EmbeddingRecord>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(EMBEDDINGS_TABLE)?;
        match t.get(chunk_id)? {
            Some(guard) => Ok(Some(rmp_serde::from_slice(guard.value())?)),
            None => Ok(None),
        }
    }

    /// Get embeddings for a set of chunk_ids (for vector search).
    pub fn get_embeddings_for_chunks(
        &self,
        chunk_ids: &[String],
    ) -> Result<Vec<EmbeddingRecord>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(EMBEDDINGS_TABLE)?;
        let mut result = Vec::new();
        for cid in chunk_ids {
            if let Some(guard) = t.get(cid.as_str())? {
                result.push(rmp_serde::from_slice(guard.value())?);
            }
        }
        Ok(result)
    }

    /// Get chunk_ids in a collection that have no embedding yet.
    pub fn get_unembedded_chunk_ids(
        &self,
        collection_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, String)>, RagError> {
        let doc_ids = self.get_collection_doc_ids(collection_id)?;
        let txn = self.db.begin_read()?;
        let idx_t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
        let emb_t = txn.open_table(EMBEDDINGS_TABLE)?;
        let chunks_t = txn.open_table(DOC_CHUNKS_TABLE)?;

        let mut result = Vec::new();
        'outer: for doc_id in &doc_ids {
            if let Some(guard) = idx_t.get(doc_id.as_str())? {
                let cids: Vec<String> = rmp_serde::from_slice(guard.value())?;
                for cid in cids {
                    if emb_t.get(cid.as_str())?.is_none() {
                        // Get the content, prepending context_prefix for richer embeddings
                        if let Some(chunk_guard) = chunks_t.get(cid.as_str())? {
                            let chunk = self.decode_chunk(chunk_guard.value())?;
                            let text = match &chunk.context_prefix {
                                Some(prefix) if !prefix.is_empty() => {
                                    format!("{} {}", prefix, chunk.content)
                                }
                                _ => chunk.content,
                            };
                            result.push((cid, text));
                            if result.len() >= limit {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // HNSW Index Management
    // ═══════════════════════════════════════════════════════════════════════

    /// Get the data directory path.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Retrieve all stored embeddings (full table scan). Used for HNSW rebuild.
    pub fn get_all_embeddings(&self) -> Result<Vec<EmbeddingRecord>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(EMBEDDINGS_TABLE)?;
        let mut result = Vec::new();
        for entry in t.iter()? {
            let (_, v) = entry?;
            let record: EmbeddingRecord = rmp_serde::from_slice(v.value())?;
            result.push(record);
        }
        debug!("Loaded {} embeddings for HNSW rebuild", result.len());
        Ok(result)
    }

    /// Rebuild the HNSW index from all stored embeddings and persist to disk.
    pub fn rebuild_hnsw_index(&self) -> Result<(), RagError> {
        let rebuild_generation = self.hnsw_index.generation.load(Ordering::SeqCst);
        let embeddings = self.get_all_embeddings()?;

        let new_index = PersistedHnswIndex::build(&embeddings);
        let hnsw_path = hnsw_index_path(&self.data_dir);
        let _operation_guard = self.hnsw_operation_lock()?;

        if self.hnsw_index.generation.load(Ordering::SeqCst) != rebuild_generation {
            debug!("Discarded rebuilt HNSW index because it was invalidated during rebuild");
            return Ok(());
        }

        // Persist to file if we have an index
        if let Some(ref idx) = new_index {
            if let Err(error) = idx.save(&hnsw_path) {
                self.set_hnsw_failed_state(error.to_string());
                return Err(error);
            }
        } else if let Err(error) = self.remove_hnsw_index_file() {
            self.set_hnsw_failed_state(error.to_string());
            return Err(error);
        }

        // Swap into memory
        let mut guard = self
            .hnsw_index
            .state
            .lock()
            .map_err(|e| RagError::HnswIndex(format!("lock poisoned: {e}")))?;
        *guard = match new_index {
            Some(index) => HnswRuntimeState::Ready(Arc::new(index)),
            None => HnswRuntimeState::Missing,
        };
        self.hnsw_index.ready.notify_all();
        debug!("HNSW index rebuild applied successfully");

        Ok(())
    }

    /// Invalidate (clear) the in-memory HNSW index, marking it as stale.
    /// The next search will fall back to brute-force until rebuild.
    pub fn invalidate_hnsw_index(&self) -> Result<(), RagError> {
        let _operation_guard = self.hnsw_operation_lock()?;

        {
            let mut guard = self
                .hnsw_index
                .state
                .lock()
                .map_err(|e| RagError::HnswIndex(format!("lock poisoned: {e}")))?;
            self.hnsw_index.generation.fetch_add(1, Ordering::SeqCst);
            *guard = HnswRuntimeState::Stale;
            self.hnsw_index.ready.notify_all();
        }

        if let Err(error) = self.remove_hnsw_index_file() {
            self.set_hnsw_failed_state(error.to_string());
            return Err(error);
        }

        debug!("HNSW index invalidated");
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════════
    // BM25 Index Storage
    // ═══════════════════════════════════════════════════════════════════════

    /// Returns the observable state of the coalesced keyword-index worker.
    pub fn bm25_index_status(&self) -> Result<Bm25IndexStatus, RagError> {
        let state = lock_with_diagnostics(&self.bm25_index.state, "bm25.status")?;
        Ok(state.status.clone())
    }

    pub(crate) fn bm25_operation_lock(&self) -> Result<MutexGuard<'_, ()>, RagError> {
        lock_with_diagnostics(&self.bm25_index.operation, "bm25.operation")
    }

    pub(crate) fn bm25_content_revision(&self) -> Result<u64, RagError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(BM25_META_TABLE)?;
        table
            .get(BM25_CONTENT_REVISION_KEY)?
            .map(|value| rmp_serde::from_slice(value.value()).map_err(RagError::from))
            .transpose()
            .map(|revision| revision.unwrap_or_default())
    }

    fn bm25_rebuild_required(&self) -> Result<bool, RagError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(BM25_META_TABLE)?;
        let content_revision = table
            .get(BM25_CONTENT_REVISION_KEY)?
            .map(|value| rmp_serde::from_slice::<u64>(value.value()))
            .transpose()?
            .unwrap_or_default();
        let indexed_revision = table
            .get(BM25_INDEXED_REVISION_KEY)?
            .map(|value| rmp_serde::from_slice::<u64>(value.value()))
            .transpose()?
            .unwrap_or_default();
        Ok(content_revision != indexed_revision)
    }

    pub(crate) fn record_manual_bm25_rebuild(&self, result: &Result<usize, RagError>) {
        if matches!(result, Err(RagError::Cancelled)) {
            return;
        }
        if let Ok(mut state) = self.bm25_index.state.lock()
            && !state.worker_running
        {
            state.status = match result {
                Ok(_) => Bm25IndexStatus::Ready,
                Err(error) => Bm25IndexStatus::Failed(error.to_string()),
            };
            self.bm25_index.changed.notify_all();
        }
    }

    /// Queues one global rebuild and coalesces further requests while it is running.
    pub fn queue_bm25_rebuild(&self) -> Result<(), RagError> {
        self.bm25_index
            .requested_generation
            .fetch_add(1, Ordering::SeqCst);
        let mut state = lock_with_diagnostics(&self.bm25_index.state, "bm25.queue")?;
        state.status = Bm25IndexStatus::Pending;
        self.bm25_index.changed.notify_all();
        if state.worker_running {
            return Ok(());
        }
        state.worker_running = true;
        drop(state);

        let store = self.clone();
        let runtime = self.bm25_index.clone();
        std::thread::Builder::new()
            .name("oxideterm-bm25-rebuild".to_string())
            .spawn(move || store.run_bm25_rebuild_worker())
            .map_err(|error| {
                if let Ok(mut state) = runtime.state.lock() {
                    state.worker_running = false;
                    state.status = Bm25IndexStatus::Failed(error.to_string());
                    runtime.changed.notify_all();
                }
                RagError::Io(error)
            })?;
        Ok(())
    }

    fn run_bm25_rebuild_worker(&self) {
        loop {
            let rebuild_generation = self.bm25_index.requested_generation.load(Ordering::SeqCst);
            if let Ok(mut state) = self.bm25_index.state.lock() {
                state.status = Bm25IndexStatus::Rebuilding;
                self.bm25_index.changed.notify_all();
            } else {
                return;
            }

            #[cfg(test)]
            let force_failure = self
                .bm25_index
                .fail_next_rebuild
                .swap(false, Ordering::SeqCst);
            #[cfg(not(test))]
            let force_failure = false;
            let result = if force_failure {
                Err(RagError::InvalidInput(
                    "forced BM25 rebuild failure".to_string(),
                ))
            } else {
                crate::rag::bm25::reindex_all(self, None, None)
            };
            let requested_generation = self.bm25_index.requested_generation.load(Ordering::SeqCst);
            let Ok(mut state) = self.bm25_index.state.lock() else {
                return;
            };
            if requested_generation != rebuild_generation {
                // A save landed while the snapshot was rebuilding. The stale result may have
                // committed briefly, so immediately replace it with the newest database state.
                state.status = Bm25IndexStatus::Pending;
                self.bm25_index.changed.notify_all();
                drop(state);
                continue;
            }
            state.status = match result {
                Ok(_) => Bm25IndexStatus::Ready,
                Err(error) => Bm25IndexStatus::Failed(error.to_string()),
            };
            state.worker_running = false;
            self.bm25_index.changed.notify_all();
            return;
        }
    }

    #[cfg(test)]
    pub(crate) fn wait_for_bm25_rebuild(&self, timeout: Duration) -> Bm25IndexStatus {
        let deadline = Instant::now() + timeout;
        let mut state = self.bm25_index.state.lock().unwrap();
        while state.worker_running {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, wait_result) = self
                .bm25_index
                .changed
                .wait_timeout(state, remaining)
                .unwrap();
            state = next;
            if wait_result.timed_out() {
                break;
            }
        }
        state.status.clone()
    }

    #[cfg(test)]
    pub(crate) fn fail_next_bm25_rebuild(&self) {
        self.bm25_index
            .fail_next_rebuild
            .store(true, Ordering::SeqCst);
    }

    pub fn get_bm25_postings(&self, term: &str) -> Result<Vec<PostingEntry>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(BM25_POSTINGS_TABLE)?;
        match t.get(term)? {
            Some(guard) => Ok(rmp_serde::from_slice(guard.value())?),
            None => Ok(Vec::new()),
        }
    }

    pub fn get_bm25_stats(&self) -> Result<Option<Bm25Stats>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(BM25_META_TABLE)?;
        match t.get("stats")? {
            Some(guard) => Ok(Some(rmp_serde::from_slice(guard.value())?)),
            None => Ok(None),
        }
    }

    /// Rebuild BM25 index for all collections.
    /// Called by `bm25::reindex_all()`.
    pub fn write_bm25_index(
        &self,
        postings: &std::collections::HashMap<String, Vec<PostingEntry>>,
        stats: &Bm25Stats,
        source_revision: u64,
    ) -> Result<(), RagError> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(BM25_POSTINGS_TABLE)?;

            // Drain old postings to ensure stale terms are removed
            let old_keys: Vec<String> = {
                let mut keys = Vec::new();
                for entry in t.iter()? {
                    let (k, _) = entry?;
                    keys.push(k.value().to_string());
                }
                keys
            };
            for key in &old_keys {
                t.remove(key.as_str())?;
            }

            // Write new postings
            for (term, entries) in postings {
                let data = rmp_serde::to_vec(entries)?;
                t.insert(term.as_str(), data.as_slice())?;
            }

            let mut meta_t = txn.open_table(BM25_META_TABLE)?;
            meta_t.insert("stats", rmp_serde::to_vec(stats)?.as_slice())?;
            meta_t.insert(
                BM25_INDEXED_REVISION_KEY,
                rmp_serde::to_vec(&source_revision)?.as_slice(),
            )?;
        }
        txn.commit()?;
        debug!(
            "Wrote BM25 index: {} terms, {} docs",
            postings.len(),
            stats.doc_count
        );
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Internal Helpers
    // ═══════════════════════════════════════════════════════════════════════

    pub fn get_collection_doc_ids(&self, collection_id: &str) -> Result<Vec<String>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(COLLECTION_DOCS_TABLE)?;
        match t.get(collection_id)? {
            Some(guard) => Ok(rmp_serde::from_slice(guard.value())?),
            None => Ok(Vec::new()),
        }
    }

    /// Return IDs of all existing collections.
    pub fn get_all_collection_ids(&self) -> Result<Vec<String>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(COLLECTIONS_TABLE)?;
        let mut ids = Vec::new();
        for entry in t.iter()? {
            let (k, _) = entry?;
            ids.push(k.value().to_string());
        }
        Ok(ids)
    }

    fn get_chunk_ids_for_doc(&self, doc_id: &str) -> Result<Vec<String>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
        match t.get(doc_id)? {
            Some(guard) => Ok(rmp_serde::from_slice(guard.value())?),
            None => Ok(Vec::new()),
        }
    }

    /// Loads editable content, metadata, and embedding freshness from one read snapshot.
    pub fn get_document_for_editing(
        &self,
        doc_id: &str,
    ) -> Result<(DocMetadata, String, usize), RagError> {
        let txn = self.db.begin_read()?;
        let metadata = txn
            .open_table(DOC_METADATA_TABLE)?
            .get(doc_id)?
            .map(|guard| rmp_serde::from_slice::<DocMetadata>(guard.value()))
            .transpose()?
            .ok_or_else(|| RagError::DocumentNotFound(doc_id.to_string()))?;
        let content = txn
            .open_table(DOC_RAW_CONTENT_TABLE)?
            .get(doc_id)?
            .map(|guard| Self::decompress_bytes(guard.value()))
            .transpose()?
            .ok_or_else(|| RagError::DocumentNotFound(doc_id.to_string()))?;
        let content = String::from_utf8(content)
            .map_err(|error| RagError::Serialization(error.to_string()))?;
        let chunk_ids = txn
            .open_table(DOC_CHUNK_INDEX_TABLE)?
            .get(doc_id)?
            .map(|guard| rmp_serde::from_slice::<Vec<String>>(guard.value()))
            .transpose()?
            .unwrap_or_default();
        let embeddings = txn.open_table(EMBEDDINGS_TABLE)?;
        let mut pending = 0usize;
        for chunk_id in chunk_ids {
            if embeddings.get(chunk_id.as_str())?.is_none() {
                pending += 1;
            }
        }
        Ok((metadata, content, pending))
    }

    pub fn get_pending_embedding_count_for_doc(&self, doc_id: &str) -> Result<usize, RagError> {
        let txn = self.db.begin_read()?;
        let chunk_index = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
        let embeddings = txn.open_table(EMBEDDINGS_TABLE)?;
        let chunk_ids = chunk_index
            .get(doc_id)?
            .map(|value| rmp_serde::from_slice::<Vec<String>>(value.value()))
            .transpose()?
            .ok_or_else(|| RagError::DocumentNotFound(doc_id.to_string()))?;
        let mut pending = 0usize;
        for chunk_id in chunk_ids {
            if embeddings.get(chunk_id.as_str())?.is_none() {
                pending += 1;
            }
        }
        Ok(pending)
    }

    /// Decode a chunk from stored bytes (handles compression prefix).
    fn decode_chunk(&self, raw: &[u8]) -> Result<DocChunk, RagError> {
        if raw.is_empty() {
            return Err(RagError::Serialization("Empty chunk data".to_string()));
        }
        let bytes = if raw[0] == 1 {
            // zstd compressed
            zstd::decode_all(&raw[1..]).map_err(|e| RagError::Compression(e.to_string()))?
        } else {
            raw[1..].to_vec()
        };
        Ok(rmp_serde::from_slice(&bytes)?)
    }

    /// Compress bytes with prefix: 0 = raw, 1 = zstd.
    fn compress_bytes(data: &[u8]) -> Vec<u8> {
        if data.len() > COMPRESSION_THRESHOLD {
            if let Ok(body) = zstd::encode_all(data, 3) {
                let mut out = vec![1u8];
                out.extend_from_slice(&body);
                return out;
            }
        }
        let mut out = vec![0u8];
        out.extend_from_slice(data);
        out
    }

    /// Decompress bytes with prefix: 0 = raw, 1 = zstd.
    fn decompress_bytes(stored: &[u8]) -> Result<Vec<u8>, RagError> {
        if stored.is_empty() {
            return Err(RagError::Serialization("Empty stored data".to_string()));
        }
        if stored[0] == 1 {
            zstd::decode_all(&stored[1..]).map_err(|e| RagError::Compression(e.to_string()))
        } else {
            Ok(stored[1..].to_vec())
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Raw Document Content
    // ═══════════════════════════════════════════════════════════════════════

    /// Retrieve the original raw content of a document.
    pub fn get_raw_content(&self, doc_id: &str) -> Result<Option<String>, RagError> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOC_RAW_CONTENT_TABLE)?;
        match t.get(doc_id)? {
            Some(guard) => {
                let bytes = Self::decompress_bytes(guard.value())?;
                Ok(Some(
                    String::from_utf8(bytes).map_err(|e| RagError::Serialization(e.to_string()))?,
                ))
            }
            None => Ok(None),
        }
    }

    /// Update a document's content: replace raw content, re-chunk, and update metadata.
    /// Returns the updated metadata. Caller is responsible for BM25 re-indexing.
    /// If `expected_version` is Some, performs optimistic locking check.
    pub fn update_document(
        &self,
        doc_id: &str,
        new_content: &str,
        new_chunks: &[DocChunk],
        content_hash: &str,
        now: i64,
        expected_version: Option<u64>,
    ) -> Result<DocMetadata, RagError> {
        let new_chunk_ids: Vec<String> = new_chunks.iter().map(|c| c.id.clone()).collect();
        let chunk_idx_bytes = rmp_serde::to_vec(&new_chunk_ids)?;
        let stored_content = Self::compress_bytes(new_content.as_bytes());
        let stored_chunks = new_chunks
            .iter()
            .map(|chunk| {
                let raw = rmp_serde::to_vec(chunk)?;
                Ok((chunk.id.clone(), Self::compress_bytes(&raw)))
            })
            .collect::<Result<Vec<_>, RagError>>()?;

        let txn = self.db.begin_write()?;
        let (updated_meta, old_chunk_count) = {
            // The version comparison belongs to the same serialized write transaction as the
            // replacement. Reading it before begin_write would let two same-version writers pass
            // the check and overwrite each other.
            let mut meta_t = txn.open_table(DOC_METADATA_TABLE)?;
            let meta_bytes = meta_t.get(doc_id)?.map(|guard| guard.value().to_vec());
            let meta: DocMetadata = meta_bytes
                .as_deref()
                .ok_or_else(|| RagError::DocumentNotFound(doc_id.to_string()))
                .and_then(|bytes| rmp_serde::from_slice(bytes).map_err(RagError::from))?;
            if let Some(expected) = expected_version
                && meta.version != expected
            {
                return Err(RagError::VersionConflict {
                    expected,
                    actual: meta.version,
                });
            }

            let mut idx_t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
            let old_chunk_ids = idx_t
                .get(doc_id)?
                .map(|guard| guard.value().to_vec())
                .map(|bytes| rmp_serde::from_slice::<Vec<String>>(&bytes))
                .transpose()?
                .unwrap_or_default();

            let updated_meta = DocMetadata {
                content_hash: content_hash.to_string(),
                indexed_at: now,
                chunk_count: new_chunks.len(),
                version: meta.version + 1,
                ..meta
            };

            // Remove old chunks + embeddings
            let mut chunks_t = txn.open_table(DOC_CHUNKS_TABLE)?;
            let mut emb_t = txn.open_table(EMBEDDINGS_TABLE)?;
            for cid in &old_chunk_ids {
                let _ = chunks_t.remove(cid.as_str())?;
                let _ = emb_t.remove(cid.as_str())?;
            }

            // Store new chunks
            for (chunk_id, stored) in &stored_chunks {
                chunks_t.insert(chunk_id.as_str(), stored.as_slice())?;
            }

            // Update chunk index
            idx_t.insert(doc_id, chunk_idx_bytes.as_slice())?;

            // Update metadata
            let updated_meta_bytes = rmp_serde::to_vec(&updated_meta)?;
            meta_t.insert(doc_id, updated_meta_bytes.as_slice())?;

            // Update raw content
            let mut raw_t = txn.open_table(DOC_RAW_CONTENT_TABLE)?;
            raw_t.insert(doc_id, stored_content.as_slice())?;

            // Persist the content revision in the same transaction as the document so a crash
            // before the background rebuild is detected and resumed on the next application run.
            Self::bump_bm25_content_revision(&txn)?;

            // Update collection timestamp
            let mut col_t = txn.open_table(COLLECTIONS_TABLE)?;
            let col_data = col_t
                .get(updated_meta.collection_id.as_str())?
                .map(|g| g.value().to_vec());
            if let Some(bytes) = col_data {
                let mut col: DocCollection = rmp_serde::from_slice(&bytes)?;
                col.updated_at = now;
                col_t.insert(
                    updated_meta.collection_id.as_str(),
                    rmp_serde::to_vec(&col)?.as_slice(),
                )?;
            }
            (updated_meta, old_chunk_ids.len())
        };
        txn.commit()?;
        debug!(
            "Updated document {} ({} → {} chunks)",
            doc_id,
            old_chunk_count,
            new_chunks.len()
        );

        // Invalidate HNSW index since embeddings changed
        if let Err(e) = self.invalidate_hnsw_index() {
            warn!(
                "Document {} updated but HNSW invalidation failed: {}",
                doc_id, e
            );
        }

        Ok(updated_meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn temp_store(test_name: &str) -> RagStore {
        let dir = std::env::temp_dir().join(format!(
            "oxideterm_rag_store_{}_{}_{}",
            test_name,
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        RagStore::new(&dir).unwrap()
    }

    fn make_collection(id: &str) -> DocCollection {
        let now = chrono::Utc::now().timestamp_millis();
        DocCollection {
            id: id.to_string(),
            name: id.to_string(),
            scope: DocScope::Global,
            created_at: now,
            updated_at: now,
        }
    }

    fn make_doc(doc_id: &str, collection_id: &str, hash: &str) -> DocMetadata {
        DocMetadata {
            id: doc_id.to_string(),
            collection_id: collection_id.to_string(),
            title: doc_id.to_string(),
            source_path: None,
            format: DocFormat::PlainText,
            content_hash: hash.to_string(),
            indexed_at: chrono::Utc::now().timestamp_millis(),
            chunk_count: 0,
            version: 0,
        }
    }

    #[test]
    fn check_content_hash_exists_excluding_doc_ignores_same_doc_and_detects_other_docs() {
        let store = temp_store("content_hash_excluding_doc");
        store.create_collection(&make_collection("col-1")).unwrap();
        store
            .add_document(&make_doc("doc-1", "col-1", "hash-a"), &[], Some("alpha"))
            .unwrap();
        store
            .add_document(&make_doc("doc-2", "col-1", "hash-b"), &[], Some("beta"))
            .unwrap();

        assert!(
            !store
                .check_content_hash_exists_excluding_doc("col-1", "hash-a", "doc-1")
                .unwrap()
        );
        assert!(
            store
                .check_content_hash_exists_excluding_doc("col-1", "hash-b", "doc-1")
                .unwrap()
        );
    }

    #[test]
    fn stale_document_version_leaves_committed_content_unchanged() {
        let store = temp_store("stale_document_version");
        store.create_collection(&make_collection("col-1")).unwrap();
        store
            .add_document(&make_doc("doc-1", "col-1", "hash-0"), &[], Some("initial"))
            .unwrap();

        let updated = store
            .update_document("doc-1", "first", &[], "hash-1", 10, Some(0))
            .unwrap();
        let error = store
            .update_document("doc-1", "stale", &[], "hash-stale", 11, Some(0))
            .unwrap_err();

        assert!(matches!(
            error,
            RagError::VersionConflict {
                expected: 0,
                actual: 1
            }
        ));
        assert_eq!(updated.version, 1);
        assert_eq!(
            store.get_raw_content("doc-1").unwrap().as_deref(),
            Some("first")
        );
        let metadata = store.get_doc_metadata("doc-1").unwrap().unwrap();
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.content_hash, "hash-1");
    }

    #[test]
    fn concurrent_same_version_document_updates_allow_only_one_commit() {
        let store = temp_store("concurrent_document_version");
        store.create_collection(&make_collection("col-1")).unwrap();
        store
            .add_document(&make_doc("doc-1", "col-1", "hash-0"), &[], Some("initial"))
            .unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let handles = [("alpha", "hash-alpha"), ("beta", "hash-beta")]
            .into_iter()
            .map(|(content, hash)| {
                let store = store.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store.update_document("doc-1", content, &[], hash, 10, Some(0))
                })
            })
            .collect::<Vec<_>>();

        barrier.wait();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(RagError::VersionConflict { .. })))
                .count(),
            1
        );
        let metadata = store.get_doc_metadata("doc-1").unwrap().unwrap();
        let content = store.get_raw_content("doc-1").unwrap().unwrap();
        assert_eq!(metadata.version, 1);
        assert!(matches!(content.as_str(), "alpha" | "beta"));
    }
}
