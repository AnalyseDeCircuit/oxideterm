// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! RAG (Retrieval-Augmented Generation) Tauri Commands
//!
//! Provides commands for managing documentation collections,
//! indexing documents, storing embeddings, and searching.

use crate::config;
use crate::rag::bm25;
use crate::rag::chunker;
use crate::rag::embedding;
use crate::rag::search::{self, SearchMode};
use crate::rag::store::RagStore;
use crate::rag::types::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, State};
use tracing::info;

/// Global cancellation flag for reindex operations.
static REINDEX_CANCEL: std::sync::LazyLock<AtomicBool> =
    std::sync::LazyLock::new(|| AtomicBool::new(false));

/// Guard to prevent concurrent reindex operations.
static REINDEX_RUNNING: std::sync::LazyLock<AtomicBool> =
    std::sync::LazyLock::new(|| AtomicBool::new(false));

#[derive(Default)]
struct HnswRebuildState {
    running: bool,
    rerun_requested: bool,
}

#[derive(Default)]
struct HnswRebuildCoordinator {
    state: Mutex<HnswRebuildState>,
}

impl HnswRebuildCoordinator {
    fn request_or_queue(&self) -> bool {
        let mut state = self.state.lock().expect("hnsw rebuild lock poisoned");
        if state.running {
            state.rerun_requested = true;
            false
        } else {
            state.running = true;
            state.rerun_requested = false;
            true
        }
    }

    fn finish_cycle(&self) -> bool {
        let mut state = self.state.lock().expect("hnsw rebuild lock poisoned");
        if state.rerun_requested {
            state.rerun_requested = false;
            true
        } else {
            state.running = false;
            false
        }
    }
}

static HNSW_REBUILD_COORDINATOR: std::sync::LazyLock<HnswRebuildCoordinator> =
    std::sync::LazyLock::new(HnswRebuildCoordinator::default);

/// Event payload emitted during BM25 reindex progress.
#[derive(Clone, Serialize)]
struct RagReindexProgress {
    current: usize,
    total: usize,
}

/// Compute a stable, deterministic content hash using SHA-256 (128-bit / 32 hex chars).
fn content_hash(text: &str) -> String {
    let hash = Sha256::digest(text.as_bytes());
    format!(
        "{:032x}",
        u128::from_be_bytes(hash[..16].try_into().unwrap())
    )
}

/// Build a contextual header from document title and section path.
/// This lightweight approach (inspired by Anthropic Contextual Retrieval)
/// prepends document-level context to each chunk, improving BM25 and
/// embedding retrieval by 20-35% without any LLM cost.
fn build_context_prefix(title: &str, section_path: Option<&str>) -> String {
    match section_path {
        Some(path) if !path.is_empty() => {
            format!("From document '{}', section: {}.", title, path)
        }
        _ => {
            format!("From document '{}'.", title)
        }
    }
}

/// Max allowed length for titles, names, and similar short text fields.
const MAX_NAME_LENGTH: usize = 1000;
/// Max allowed document content size (10 MB).
const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;
/// Max allowed search query length (10 000 characters).
const MAX_QUERY_LENGTH: usize = 10_000;

// ═══════════════════════════════════════════════════════════════════════════
// Request / Response Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectionRequest {
    pub name: String,
    pub scope: DocScopeRequest,
}

#[derive(Debug, Deserialize)]
pub enum DocScopeRequest {
    Global,
    Connection { connection_id: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDocumentRequest {
    pub collection_id: String,
    pub title: String,
    pub content: String,
    pub format: String,
    pub source_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreEmbeddingsRequest {
    pub embeddings: Vec<EmbeddingInputRequest>,
    pub model_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingInputRequest {
    pub chunk_id: String,
    pub vector: Vec<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub collection_ids: Vec<String>,
    pub query_vector: Option<Vec<f32>>,
    pub top_k: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionResponse {
    pub id: String,
    pub name: String,
    pub scope: DocScope,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentResponse {
    pub id: String,
    pub collection_id: String,
    pub title: String,
    pub source_path: Option<String>,
    pub format: String,
    pub chunk_count: usize,
    pub indexed_at: i64,
    pub version: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    pub doc_count: usize,
    pub chunk_count: usize,
    pub embedded_chunk_count: usize,
    pub last_updated: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingEmbeddingResponse {
    pub chunk_id: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultResponse {
    pub chunk_id: String,
    pub doc_id: String,
    pub doc_title: String,
    pub section_path: Option<String>,
    pub content: String,
    pub score: f64,
    pub source: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Extract the RAG store from optional state, returning an error if unavailable.
fn require_rag_store<'a>(
    state: &'a State<'_, Option<Arc<RagStore>>>,
) -> Result<&'a Arc<RagStore>, String> {
    state
        .as_ref()
        .ok_or_else(|| "RAG store not available. Knowledge base features are disabled.".to_string())
}

fn queue_hnsw_rebuild(store: Arc<RagStore>) {
    if !HNSW_REBUILD_COORDINATOR.request_or_queue() {
        tracing::debug!("HNSW rebuild already running; queued another pass");
        return;
    }

    tokio::task::spawn_blocking(move || {
        loop {
            if let Err(e) = store.rebuild_hnsw_index() {
                tracing::warn!("Async HNSW rebuild failed: {}", e);
            }

            if !HNSW_REBUILD_COORDINATOR.finish_cycle() {
                break;
            }
        }
    });
}

#[tauri::command]
pub async fn rag_create_collection(
    store: State<'_, Option<Arc<RagStore>>>,
    request: CreateCollectionRequest,
) -> Result<CollectionResponse, String> {
    let store = require_rag_store(&store)?;
    if request.name.len() > MAX_NAME_LENGTH {
        return Err("Collection name too long".to_string());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let scope = match request.scope {
        DocScopeRequest::Global => DocScope::Global,
        DocScopeRequest::Connection { connection_id } => DocScope::Connection(connection_id),
    };

    let collection = DocCollection {
        id: id.clone(),
        name: request.name,
        scope: scope.clone(),
        created_at: now,
        updated_at: now,
    };

    store
        .create_collection(&collection)
        .map_err(|e| e.to_string())?;
    info!("Created RAG collection: {}", id);

    Ok(CollectionResponse {
        id: collection.id,
        name: collection.name,
        scope,
        created_at: now,
        updated_at: now,
    })
}

#[tauri::command]
pub async fn rag_list_collections(
    store: State<'_, Option<Arc<RagStore>>>,
    scope_filter: Option<String>,
) -> Result<Vec<CollectionResponse>, String> {
    let store = require_rag_store(&store)?;
    let collections = store
        .list_collections(scope_filter.as_deref())
        .map_err(|e| e.to_string())?;

    Ok(collections
        .into_iter()
        .map(|c| CollectionResponse {
            id: c.id,
            name: c.name,
            scope: c.scope,
            created_at: c.created_at,
            updated_at: c.updated_at,
        })
        .collect())
}

#[tauri::command]
pub async fn rag_delete_collection(
    store: State<'_, Option<Arc<RagStore>>>,
    collection_id: String,
) -> Result<(), String> {
    let store = require_rag_store(&store)?;
    store
        .delete_collection(&collection_id)
        .map_err(|e| e.to_string())?;
    // Rebuild global BM25 index to remove stale postings
    bm25::reindex_all(&store, None, None).map_err(|e| e.to_string())?;
    info!("Deleted RAG collection: {}", collection_id);
    Ok(())
}

#[tauri::command]
pub async fn rag_get_collection_stats(
    store: State<'_, Option<Arc<RagStore>>>,
    collection_id: String,
) -> Result<StatsResponse, String> {
    let store = require_rag_store(&store)?;
    let stats = store
        .get_collection_stats(&collection_id)
        .map_err(|e| e.to_string())?;

    Ok(StatsResponse {
        doc_count: stats.doc_count,
        chunk_count: stats.chunk_count,
        embedded_chunk_count: stats.embedded_chunk_count,
        last_updated: stats.last_updated,
    })
}

#[tauri::command]
pub async fn rag_add_document(
    store: State<'_, Option<Arc<RagStore>>>,
    request: AddDocumentRequest,
) -> Result<DocumentResponse, String> {
    let store = require_rag_store(&store)?;
    if request.title.len() > MAX_NAME_LENGTH {
        return Err("Document title too long".to_string());
    }
    if request.content.len() > MAX_CONTENT_SIZE {
        return Err("Document content too large".to_string());
    }
    let doc_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    let format = match request.format.as_str() {
        "markdown" => DocFormat::Markdown,
        "plaintext" | "txt" => DocFormat::PlainText,
        other => return Err(format!("Unsupported format: {}", other)),
    };

    // Chunk the document
    let chunks = chunker::chunk_document(&doc_id, &request.content, &format);
    let hash = content_hash(&request.content);

    // Content deduplication: reject if same content already exists in collection
    if store
        .check_content_hash_exists(&request.collection_id, &hash)
        .map_err(|e| e.to_string())?
    {
        return Err(format!(
            "Duplicate document: identical content already exists in this collection (hash: {})",
            &hash[..8]
        ));
    }

    // Generate contextual headers for each chunk (Anthropic Contextual Retrieval)
    let chunks: Vec<_> = chunks
        .into_iter()
        .map(|mut chunk| {
            let prefix = build_context_prefix(&request.title, chunk.section_path.as_deref());
            chunk.context_prefix = Some(prefix);
            chunk
        })
        .collect();

    let metadata = DocMetadata {
        id: doc_id.clone(),
        collection_id: request.collection_id.clone(),
        title: request.title.clone(),
        source_path: request.source_path,
        format: format.clone(),
        content_hash: hash,
        indexed_at: now,
        chunk_count: chunks.len(),
        version: 0,
    };

    // Store document + chunks + raw content
    store
        .add_document(&metadata, &chunks, Some(&request.content))
        .map_err(|e| e.to_string())?;

    // Index chunks for BM25 (context-aware: includes context_prefix)
    for chunk in &chunks {
        bm25::index_chunk(
            &store,
            &chunk.id,
            &chunk.content,
            chunk.context_prefix.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    }

    let format_str = match format {
        DocFormat::Markdown => "markdown",
        DocFormat::PlainText => "plaintext",
    };

    info!(
        "Added document '{}' ({} chunks) to collection {}",
        request.title,
        chunks.len(),
        request.collection_id
    );

    Ok(DocumentResponse {
        id: doc_id,
        collection_id: request.collection_id,
        title: request.title,
        source_path: metadata.source_path,
        format: format_str.to_string(),
        chunk_count: chunks.len(),
        indexed_at: now,
        version: 0,
    })
}

#[tauri::command]
pub async fn rag_remove_document(
    store: State<'_, Option<Arc<RagStore>>>,
    doc_id: String,
) -> Result<(), String> {
    let store = require_rag_store(&store)?;
    store.remove_document(&doc_id).map_err(|e| e.to_string())?;
    // Rebuild global BM25 index to remove stale postings
    bm25::reindex_all(&store, None, None).map_err(|e| e.to_string())?;
    info!("Removed RAG document: {}", doc_id);
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedDocuments {
    pub documents: Vec<DocumentResponse>,
    pub total: usize,
}

#[tauri::command]
pub async fn rag_list_documents(
    store: State<'_, Option<Arc<RagStore>>>,
    collection_id: String,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<PaginatedDocuments, String> {
    let store = require_rag_store(&store)?;
    let doc_ids = store
        .get_collection_doc_ids(&collection_id)
        .map_err(|e| e.to_string())?;
    let total = doc_ids.len();

    let start = offset.unwrap_or(0).min(total);
    let end = limit.map_or(total, |l| (start + l).min(total));

    let mut docs = Vec::new();
    for doc_id in &doc_ids[start..end] {
        if let Some(meta) = store.get_doc_metadata(doc_id).map_err(|e| e.to_string())? {
            let format_str = match meta.format {
                DocFormat::Markdown => "markdown",
                DocFormat::PlainText => "plaintext",
            };
            docs.push(DocumentResponse {
                id: meta.id,
                collection_id: meta.collection_id,
                title: meta.title,
                source_path: meta.source_path,
                format: format_str.to_string(),
                chunk_count: meta.chunk_count,
                indexed_at: meta.indexed_at,
                version: meta.version,
            });
        }
    }
    Ok(PaginatedDocuments {
        documents: docs,
        total,
    })
}

#[tauri::command]
pub async fn rag_get_pending_embeddings(
    store: State<'_, Option<Arc<RagStore>>>,
    collection_id: String,
    limit: Option<usize>,
) -> Result<Vec<PendingEmbeddingResponse>, String> {
    let store = require_rag_store(&store)?;
    let pending = embedding::get_pending_embeddings(&store, &collection_id, limit.unwrap_or(50))
        .map_err(|e| e.to_string())?;

    Ok(pending
        .into_iter()
        .map(|(chunk_id, content)| PendingEmbeddingResponse { chunk_id, content })
        .collect())
}

#[tauri::command]
pub async fn rag_store_embeddings(
    store: State<'_, Option<Arc<RagStore>>>,
    request: StoreEmbeddingsRequest,
) -> Result<usize, String> {
    let store = require_rag_store(&store)?;
    let records: Vec<EmbeddingRecord> = request
        .embeddings
        .into_iter()
        .map(|e| {
            let dimensions = e.vector.len();
            EmbeddingRecord {
                chunk_id: e.chunk_id,
                vector: e.vector,
                model_name: request.model_name.clone(),
                dimensions,
            }
        })
        .collect();

    // Validate: all vectors in batch must have the same dimensions
    if let Some(first) = records.first() {
        let expected_dim = first.dimensions;
        if expected_dim == 0 {
            return Err("Embedding vectors must not be empty".to_string());
        }
        if let Some(bad) = records.iter().find(|r| r.dimensions != expected_dim) {
            return Err(format!(
                "Dimension mismatch: expected {} but chunk {} has {}",
                expected_dim, bad.chunk_id, bad.dimensions
            ));
        }
    }

    let count = embedding::store_embeddings(&store, records).map_err(|e| e.to_string())?;

    queue_hnsw_rebuild(Arc::clone(store));

    Ok(count)
}

#[tauri::command]
pub async fn rag_search(
    store: State<'_, Option<Arc<RagStore>>>,
    request: SearchRequest,
) -> Result<Vec<SearchResultResponse>, String> {
    let store = require_rag_store(&store)?;
    if request.query.len() > MAX_QUERY_LENGTH {
        return Err("Search query too long".to_string());
    }
    if let Some(ref vec) = request.query_vector {
        if vec.is_empty() {
            return Err("Query vector must not be empty".to_string());
        }
    }
    let top_k = request.top_k.unwrap_or(5);

    let mode = match request.query_vector {
        Some(vec) => SearchMode::Hybrid { query_vector: vec },
        None => SearchMode::KeywordOnly,
    };

    let results = search::search(&store, &request.query, &request.collection_ids, mode, top_k)
        .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|r| {
            let source_str = match r.source {
                SearchSource::Bm25Only => "bm25",
                SearchSource::VectorOnly => "vector",
                SearchSource::Both => "both",
            };
            SearchResultResponse {
                chunk_id: r.chunk_id,
                doc_id: r.doc_id,
                doc_title: r.doc_title,
                section_path: r.section_path,
                content: r.content,
                score: r.score,
                source: source_str.to_string(),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn rag_reindex_collection(
    app: AppHandle,
    store: State<'_, Option<Arc<RagStore>>>,
    collection_id: String,
) -> Result<usize, String> {
    let store = require_rag_store(&store)?;
    // Prevent concurrent reindex
    if REINDEX_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("Reindex already in progress".to_string());
    }

    // Reset cancel flag
    REINDEX_CANCEL.store(false, Ordering::Relaxed);

    let app_clone = app.clone();
    let mut last_emitted: usize = 0;
    let mut on_progress = |current: usize, total: usize| {
        // Emit at most every 10 chunks to avoid flooding
        if current == total || current - last_emitted >= 10 {
            let _ = app_clone.emit(
                "rag_reindex_progress",
                RagReindexProgress { current, total },
            );
            last_emitted = current;
        }
    };

    let result = bm25::reindex_all(&store, Some(&REINDEX_CANCEL), Some(&mut on_progress));
    REINDEX_RUNNING.store(false, Ordering::SeqCst);

    let count = result.map_err(|e| e.to_string())?;
    info!(
        "Re-indexed BM25 (triggered by collection {}): {} chunks",
        collection_id, count
    );
    Ok(count)
}

#[tauri::command]
pub async fn rag_cancel_reindex() -> Result<(), String> {
    REINDEX_CANCEL.store(true, Ordering::Relaxed);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Document Content & Editing Commands
// ═══════════════════════════════════════════════════════════════════════════

#[tauri::command]
pub async fn rag_get_document_content(
    store: State<'_, Option<Arc<RagStore>>>,
    doc_id: String,
) -> Result<String, String> {
    let store = require_rag_store(&store)?;
    store
        .get_raw_content(&doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No raw content stored for document {}", doc_id))
}

#[tauri::command]
pub async fn rag_update_document(
    store: State<'_, Option<Arc<RagStore>>>,
    doc_id: String,
    content: String,
    expected_version: Option<u64>,
) -> Result<DocumentResponse, String> {
    let store = require_rag_store(&store)?;
    if content.len() > MAX_CONTENT_SIZE {
        return Err("Document content too large".to_string());
    }
    let meta = store
        .get_doc_metadata(&doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {}", doc_id))?;

    let now = chrono::Utc::now().timestamp_millis();
    let chunks = chunker::chunk_document(&doc_id, &content, &meta.format);
    let hash = content_hash(&content);

    if hash != meta.content_hash
        && store
            .check_content_hash_exists_excluding_doc(&meta.collection_id, &hash, &doc_id)
            .map_err(|e| e.to_string())?
    {
        return Err(format!(
            "Duplicate document: identical content already exists in this collection (hash: {})",
            &hash[..8]
        ));
    }

    // Generate contextual headers for updated chunks
    let chunks: Vec<_> = chunks
        .into_iter()
        .map(|mut chunk| {
            let prefix = build_context_prefix(&meta.title, chunk.section_path.as_deref());
            chunk.context_prefix = Some(prefix);
            chunk
        })
        .collect();

    let updated = store
        .update_document(&doc_id, &content, &chunks, &hash, now, expected_version)
        .map_err(|e| e.to_string())?;

    // Rebuild global BM25 index to purge stale postings from old chunks
    bm25::reindex_all(&store, None, None).map_err(|e| e.to_string())?;

    let format_str = match updated.format {
        DocFormat::Markdown => "markdown",
        DocFormat::PlainText => "plaintext",
    };

    info!(
        "Updated document '{}' ({} chunks)",
        updated.title,
        chunks.len()
    );

    Ok(DocumentResponse {
        id: updated.id,
        collection_id: updated.collection_id,
        title: updated.title,
        source_path: updated.source_path,
        format: format_str.to_string(),
        chunk_count: chunks.len(),
        indexed_at: now,
        version: updated.version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hnsw_rebuild_coordinator_queues_one_follow_up_pass() {
        let coordinator = HnswRebuildCoordinator::default();

        assert!(coordinator.request_or_queue());
        assert!(!coordinator.request_or_queue());
        assert!(coordinator.finish_cycle());
        assert!(!coordinator.finish_cycle());
        assert!(coordinator.request_or_queue());
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBlankDocumentRequest {
    pub collection_id: String,
    pub title: String,
    pub format: String,
}

#[tauri::command]
pub async fn rag_create_blank_document(
    store: State<'_, Option<Arc<RagStore>>>,
    request: CreateBlankDocumentRequest,
) -> Result<DocumentResponse, String> {
    let store = require_rag_store(&store)?;
    if request.title.len() > MAX_NAME_LENGTH {
        return Err("Document title too long".to_string());
    }
    let doc_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    let format = match request.format.as_str() {
        "markdown" => DocFormat::Markdown,
        "plaintext" | "txt" => DocFormat::PlainText,
        other => return Err(format!("Unsupported format: {}", other)),
    };

    let metadata = DocMetadata {
        id: doc_id.clone(),
        collection_id: request.collection_id.clone(),
        title: request.title.clone(),
        source_path: None,
        format: format.clone(),
        content_hash: String::new(),
        indexed_at: now,
        chunk_count: 0,
        version: 0,
    };

    // Store with empty content — no chunks needed
    store
        .add_document(&metadata, &[], Some(""))
        .map_err(|e| e.to_string())?;

    let format_str = match format {
        DocFormat::Markdown => "markdown",
        DocFormat::PlainText => "plaintext",
    };

    info!(
        "Created blank document '{}' in {}",
        request.title, request.collection_id
    );

    Ok(DocumentResponse {
        id: doc_id,
        collection_id: request.collection_id,
        title: request.title,
        source_path: None,
        format: format_str.to_string(),
        chunk_count: 0,
        indexed_at: now,
        version: 0,
    })
}

#[tauri::command]
pub async fn rag_open_document_external(
    app: tauri::AppHandle,
    store: State<'_, Option<Arc<RagStore>>>,
    doc_id: String,
) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    let store = require_rag_store(&store)?;

    // Validate doc_id is a valid UUID to prevent path traversal
    uuid::Uuid::parse_str(&doc_id).map_err(|_| "Invalid document ID".to_string())?;

    let meta = store
        .get_doc_metadata(&doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {}", doc_id))?;

    let content = store
        .get_raw_content(&doc_id)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    let ext = match meta.format {
        DocFormat::Markdown => "md",
        DocFormat::PlainText => "txt",
    };

    let dir = config::storage::config_dir()
        .map_err(|e| e.to_string())?
        .join("rag-edit");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Restrict directory permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }

    let file_path = dir.join(format!("{}.{}", doc_id, ext));
    std::fs::write(&file_path, &content).map_err(|e| e.to_string())?;

    // Restrict file permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }

    let path_str = file_path.to_string_lossy().to_string();
    app.opener()
        .open_path(&path_str, None::<&str>)
        .map_err(|e| e.to_string())?;

    info!(
        "Opened document '{}' externally at {:?}",
        meta.title, file_path
    );
    Ok(path_str)
}

#[tauri::command]
pub async fn rag_rebuild_hnsw_index(
    store: State<'_, Option<Arc<RagStore>>>,
) -> Result<String, String> {
    let store = require_rag_store(&store)?;
    let store_clone = store.clone();
    tokio::task::spawn_blocking(move || store_clone.rebuild_hnsw_index())
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())?;

    let msg = if let Ok(guard) = store.hnsw_index().read() {
        match &*guard {
            Some(idx) => format!(
                "HNSW index rebuilt: {} points, {} dimensions",
                idx.meta.point_count, idx.meta.dimensions
            ),
            None => "HNSW index cleared (no embeddings)".to_string(),
        }
    } else {
        "HNSW index rebuilt".to_string()
    };

    info!("{}", msg);
    Ok(msg)
}
