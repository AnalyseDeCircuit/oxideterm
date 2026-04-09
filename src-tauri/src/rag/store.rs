// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::rag::error::RagError;
use crate::rag::hnsw::{PersistedHnswIndex, hnsw_index_path};
use crate::rag::types::*;
use redb::{Database, ReadableTable, TableDefinition};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
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
const EMBEDDINGS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("embeddings");
const DOC_RAW_CONTENT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("doc_raw_content");

/// Compression threshold: compress chunks larger than 4 KB.
const COMPRESSION_THRESHOLD: usize = 4096;

// ═══════════════════════════════════════════════════════════════════════════
// RagStore
// ═══════════════════════════════════════════════════════════════════════════

pub struct RagStore {
    db: Arc<Database>,
    data_dir: PathBuf,
    hnsw_index: Arc<RwLock<Option<PersistedHnswIndex>>>,
}

impl RagStore {
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

        // Try to load persisted HNSW index
        let hnsw = PersistedHnswIndex::load(&hnsw_index_path(&data_dir));
        if let Some(ref idx) = hnsw {
            info!("Loaded HNSW index: {} points", idx.meta.point_count);
        }

        Ok(Self {
            db: Arc::new(db),
            data_dir: data_dir.to_path_buf(),
            hnsw_index: Arc::new(RwLock::new(hnsw)),
        })
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
        }
        txn.commit()?;

        // BM25 postings will be cleaned up via reindex
        info!(
            "Deleted collection {} ({} docs)",
            collection_id,
            doc_ids.len()
        );

        // Invalidate HNSW index since embeddings were removed
        self.invalidate_hnsw_index();

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
            let mut col_t = txn.open_table(COLLECTIONS_TABLE)?;
            let col_data = col_t
                .get(metadata.collection_id.as_str())?
                .map(|g| g.value().to_vec());
            if let Some(bytes) = col_data {
                let mut col: DocCollection = rmp_serde::from_slice(&bytes)?;
                col.updated_at = metadata.indexed_at;
                col_t.insert(
                    metadata.collection_id.as_str(),
                    rmp_serde::to_vec(&col)?.as_slice(),
                )?;
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
        }
        txn.commit()?;
        debug!("Removed document {}", doc_id);

        // Invalidate HNSW index since embeddings changed
        self.invalidate_hnsw_index();

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

    /// Get a reference to the HNSW index lock.
    pub fn hnsw_index(&self) -> &Arc<RwLock<Option<PersistedHnswIndex>>> {
        &self.hnsw_index
    }

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
        let embeddings = self.get_all_embeddings()?;

        let new_index = PersistedHnswIndex::build(&embeddings);

        // Persist to file if we have an index
        if let Some(ref idx) = new_index {
            idx.save(&hnsw_index_path(&self.data_dir))?;
        }

        // Swap into memory
        let mut guard = self
            .hnsw_index
            .write()
            .map_err(|e| RagError::HnswIndex(format!("lock poisoned: {e}")))?;
        *guard = new_index;

        Ok(())
    }

    /// Invalidate (clear) the in-memory HNSW index, marking it as stale.
    /// The next search will fall back to brute-force until rebuild.
    pub fn invalidate_hnsw_index(&self) {
        match self.hnsw_index.write() {
            Ok(mut guard) => {
                *guard = None;
                debug!("HNSW index invalidated");
            }
            Err(e) => {
                warn!("Failed to invalidate HNSW index (lock poisoned): {}", e);
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // BM25 Index Storage
    // ═══════════════════════════════════════════════════════════════════════

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
        }
        txn.commit()?;
        debug!(
            "Wrote BM25 index: {} terms, {} docs",
            postings.len(),
            stats.doc_count
        );
        Ok(())
    }

    /// Incrementally add a single chunk to the BM25 index.
    pub fn add_to_bm25_index(
        &self,
        terms_tf: &std::collections::HashMap<String, f32>,
        chunk_id: &str,
        doc_length: usize,
    ) -> Result<(), RagError> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(BM25_POSTINGS_TABLE)?;
            for (term, tf) in terms_tf {
                let existing = t.get(term.as_str())?.map(|g| g.value().to_vec());
                let mut entries: Vec<PostingEntry> = match existing {
                    Some(data) => rmp_serde::from_slice(&data)?,
                    None => Vec::new(),
                };
                entries.push(PostingEntry {
                    chunk_id: chunk_id.to_string(),
                    tf: *tf,
                    doc_length,
                });
                t.insert(term.as_str(), rmp_serde::to_vec(&entries)?.as_slice())?;
            }

            // Update stats
            let mut meta_t = txn.open_table(BM25_META_TABLE)?;
            let stats_data = meta_t.get("stats")?.map(|g| g.value().to_vec());
            let mut stats: Bm25Stats = match stats_data {
                Some(data) => rmp_serde::from_slice(&data)?,
                None => Bm25Stats {
                    doc_count: 0,
                    avg_dl: 0.0,
                },
            };
            let old_total = stats.avg_dl * stats.doc_count as f64;
            stats.doc_count += 1;
            stats.avg_dl = (old_total + doc_length as f64) / stats.doc_count as f64;
            meta_t.insert("stats", rmp_serde::to_vec(&stats)?.as_slice())?;
        }
        txn.commit()?;
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
        let meta = self
            .get_doc_metadata(doc_id)?
            .ok_or_else(|| RagError::DocumentNotFound(doc_id.to_string()))?;

        // Optimistic locking check
        if let Some(expected) = expected_version {
            if meta.version != expected {
                return Err(RagError::VersionConflict {
                    expected,
                    actual: meta.version,
                });
            }
        }

        let old_chunk_ids = self.get_chunk_ids_for_doc(doc_id)?;
        let new_chunk_ids: Vec<String> = new_chunks.iter().map(|c| c.id.clone()).collect();

        let updated_meta = DocMetadata {
            content_hash: content_hash.to_string(),
            indexed_at: now,
            chunk_count: new_chunks.len(),
            version: meta.version + 1,
            ..meta
        };
        let meta_bytes = rmp_serde::to_vec(&updated_meta)?;
        let chunk_idx_bytes = rmp_serde::to_vec(&new_chunk_ids)?;

        let txn = self.db.begin_write()?;
        {
            // Remove old chunks + embeddings
            let mut chunks_t = txn.open_table(DOC_CHUNKS_TABLE)?;
            let mut emb_t = txn.open_table(EMBEDDINGS_TABLE)?;
            for cid in &old_chunk_ids {
                let _ = chunks_t.remove(cid.as_str())?;
                let _ = emb_t.remove(cid.as_str())?;
            }

            // Store new chunks
            for chunk in new_chunks {
                let raw = rmp_serde::to_vec(chunk)?;
                let stored = Self::compress_bytes(&raw);
                chunks_t.insert(chunk.id.as_str(), stored.as_slice())?;
            }

            // Update chunk index
            let mut idx_t = txn.open_table(DOC_CHUNK_INDEX_TABLE)?;
            idx_t.insert(doc_id, chunk_idx_bytes.as_slice())?;

            // Update metadata
            let mut meta_t = txn.open_table(DOC_METADATA_TABLE)?;
            meta_t.insert(doc_id, meta_bytes.as_slice())?;

            // Update raw content
            let mut raw_t = txn.open_table(DOC_RAW_CONTENT_TABLE)?;
            let stored = Self::compress_bytes(new_content.as_bytes());
            raw_t.insert(doc_id, stored.as_slice())?;

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
        }
        txn.commit()?;
        debug!(
            "Updated document {} ({} → {} chunks)",
            doc_id,
            old_chunk_ids.len(),
            new_chunks.len()
        );

        // Invalidate HNSW index since embeddings changed
        self.invalidate_hnsw_index();

        Ok(updated_meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
