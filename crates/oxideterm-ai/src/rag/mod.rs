// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

pub mod bm25;
pub mod chunker;
pub mod embedding;
pub mod error;
pub mod hnsw;
pub mod search;
pub mod store;
pub mod types;

use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use store::RagStore;
pub use types::{DocCollection, DocFormat, DocMetadata, DocScope, EmbeddingRecord, SearchSource};

const MAX_NAME_LENGTH: usize = 1000;
const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;
const MAX_QUERY_LENGTH: usize = 10_000;

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

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionResponse {
    pub id: String,
    pub name: String,
    pub scope: DocScope,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
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

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    pub doc_count: usize,
    pub chunk_count: usize,
    pub embedded_chunk_count: usize,
    pub last_updated: i64,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingEmbeddingResponse {
    pub chunk_id: String,
    pub content: String,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
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

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedDocuments {
    pub documents: Vec<DocumentResponse>,
    pub total: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBlankDocumentRequest {
    pub collection_id: String,
    pub title: String,
    pub format: String,
}

fn content_hash(text: &str) -> String {
    let hash = Sha256::digest(text.as_bytes());
    format!(
        "{:032x}",
        u128::from_be_bytes(hash[..16].try_into().unwrap())
    )
}

fn build_context_prefix(title: &str, section_path: Option<&str>) -> String {
    match section_path {
        Some(path) if !path.is_empty() => {
            format!("From document '{}', section: {}.", title, path)
        }
        _ => format!("From document '{}'.", title),
    }
}

fn doc_format_from_wire(value: &str) -> Result<DocFormat, String> {
    match value {
        "markdown" => Ok(DocFormat::Markdown),
        "plaintext" | "txt" => Ok(DocFormat::PlainText),
        other => Err(format!("Unsupported format: {other}")),
    }
}

fn doc_format_to_wire(format: &DocFormat) -> &'static str {
    match format {
        DocFormat::Markdown => "markdown",
        DocFormat::PlainText => "plaintext",
    }
}

fn document_response(meta: DocMetadata) -> DocumentResponse {
    DocumentResponse {
        id: meta.id,
        collection_id: meta.collection_id,
        title: meta.title,
        source_path: meta.source_path,
        format: doc_format_to_wire(&meta.format).to_string(),
        chunk_count: meta.chunk_count,
        indexed_at: meta.indexed_at,
        version: meta.version,
    }
}

pub fn rag_create_collection(
    store: &RagStore,
    request: CreateCollectionRequest,
) -> Result<CollectionResponse, String> {
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
    Ok(CollectionResponse {
        id,
        name: collection.name,
        scope,
        created_at: now,
        updated_at: now,
    })
}

pub fn rag_list_collections(
    store: &RagStore,
    scope_filter: Option<&str>,
) -> Result<Vec<CollectionResponse>, String> {
    store
        .list_collections(scope_filter)
        .map_err(|e| e.to_string())
        .map(|collections| {
            collections
                .into_iter()
                .map(|c| CollectionResponse {
                    id: c.id,
                    name: c.name,
                    scope: c.scope,
                    created_at: c.created_at,
                    updated_at: c.updated_at,
                })
                .collect()
        })
}

pub fn rag_delete_collection(store: &RagStore, collection_id: &str) -> Result<(), String> {
    store
        .delete_collection(collection_id)
        .map_err(|e| e.to_string())?;
    bm25::reindex_all(store, None, None).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn rag_get_collection_stats(
    store: &RagStore,
    collection_id: &str,
) -> Result<StatsResponse, String> {
    let stats = store
        .get_collection_stats(collection_id)
        .map_err(|e| e.to_string())?;
    Ok(StatsResponse {
        doc_count: stats.doc_count,
        chunk_count: stats.chunk_count,
        embedded_chunk_count: stats.embedded_chunk_count,
        last_updated: stats.last_updated,
    })
}

pub fn rag_add_document(
    store: &RagStore,
    request: AddDocumentRequest,
) -> Result<DocumentResponse, String> {
    if request.title.len() > MAX_NAME_LENGTH {
        return Err("Document title too long".to_string());
    }
    if request.content.len() > MAX_CONTENT_SIZE {
        return Err("Document content too large".to_string());
    }
    let doc_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let format = doc_format_from_wire(&request.format)?;
    let mut chunks = chunker::chunk_document(&doc_id, &request.content, &format);
    let hash = content_hash(&request.content);
    if store
        .check_content_hash_exists(&request.collection_id, &hash)
        .map_err(|e| e.to_string())?
    {
        return Err(format!(
            "Duplicate document: identical content already exists in this collection (hash: {})",
            &hash[..8]
        ));
    }
    for chunk in &mut chunks {
        chunk.context_prefix = Some(build_context_prefix(
            &request.title,
            chunk.section_path.as_deref(),
        ));
    }
    let metadata = DocMetadata {
        id: doc_id.clone(),
        collection_id: request.collection_id.clone(),
        title: request.title.clone(),
        source_path: request.source_path,
        format,
        content_hash: hash,
        indexed_at: now,
        chunk_count: chunks.len(),
        version: 0,
    };
    store
        .add_document(&metadata, &chunks, Some(&request.content))
        .map_err(|e| e.to_string())?;
    for chunk in &chunks {
        bm25::index_chunk(
            store,
            &chunk.id,
            &chunk.content,
            chunk.context_prefix.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(document_response(metadata))
}

pub fn rag_remove_document(store: &RagStore, doc_id: &str) -> Result<(), String> {
    store.remove_document(doc_id).map_err(|e| e.to_string())?;
    bm25::reindex_all(store, None, None).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn rag_reindex_collection(store: &RagStore, collection_id: &str) -> Result<usize, String> {
    bm25::reindex_collection(store, collection_id).map_err(|e| e.to_string())
}

pub fn rag_reindex_collection_with_progress(
    store: &RagStore,
    _collection_id: &str,
    cancel: Option<&AtomicBool>,
    on_progress: Option<&mut dyn FnMut(usize, usize)>,
) -> Result<usize, String> {
    bm25::reindex_all(store, cancel, on_progress).map_err(|e| e.to_string())
}

pub fn rag_list_documents(
    store: &RagStore,
    collection_id: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<PaginatedDocuments, String> {
    let doc_ids = store
        .get_collection_doc_ids(collection_id)
        .map_err(|e| e.to_string())?;
    let total = doc_ids.len();
    let start = offset.unwrap_or(0).min(total);
    let end = limit.map_or(total, |limit| (start + limit).min(total));
    let mut documents = Vec::new();
    for doc_id in &doc_ids[start..end] {
        if let Some(meta) = store.get_doc_metadata(doc_id).map_err(|e| e.to_string())? {
            documents.push(document_response(meta));
        }
    }
    Ok(PaginatedDocuments { documents, total })
}

pub fn rag_get_pending_embeddings(
    store: &RagStore,
    collection_id: &str,
    limit: Option<usize>,
) -> Result<Vec<PendingEmbeddingResponse>, String> {
    embedding::get_pending_embeddings(store, collection_id, limit.unwrap_or(50))
        .map_err(|e| e.to_string())
        .map(|pending| {
            pending
                .into_iter()
                .map(|(chunk_id, content)| PendingEmbeddingResponse { chunk_id, content })
                .collect()
        })
}

pub fn rag_store_embeddings(
    store: &RagStore,
    request: StoreEmbeddingsRequest,
) -> Result<usize, String> {
    let records: Vec<EmbeddingRecord> = request
        .embeddings
        .into_iter()
        .map(|input| {
            let dimensions = input.vector.len();
            EmbeddingRecord {
                chunk_id: input.chunk_id,
                vector: input.vector,
                model_name: request.model_name.clone(),
                dimensions,
            }
        })
        .collect();
    if let Some(first) = records.first() {
        let expected_dim = first.dimensions;
        if expected_dim == 0 {
            return Err("Embedding vectors must not be empty".to_string());
        }
        if let Some(bad) = records
            .iter()
            .find(|record| record.dimensions != expected_dim)
        {
            return Err(format!(
                "Dimension mismatch: expected {} but chunk {} has {}",
                expected_dim, bad.chunk_id, bad.dimensions
            ));
        }
    }
    let count = embedding::store_embeddings(store, records).map_err(|e| e.to_string())?;
    let _ = store.rebuild_hnsw_index();
    Ok(count)
}

pub fn rag_search(
    store: &RagStore,
    request: SearchRequest,
) -> Result<Vec<SearchResultResponse>, String> {
    if request.query.len() > MAX_QUERY_LENGTH {
        return Err("Search query too long".to_string());
    }
    if let Some(query_vector) = &request.query_vector {
        if query_vector.is_empty() {
            return Err("Query vector must not be empty".to_string());
        }
    }
    let top_k = request.top_k.unwrap_or(5);
    let mode = match request.query_vector {
        Some(query_vector) => search::SearchMode::Hybrid { query_vector },
        None => search::SearchMode::KeywordOnly,
    };
    search::search(store, &request.query, &request.collection_ids, mode, top_k)
        .map_err(|e| e.to_string())
        .map(|results| {
            results
                .into_iter()
                .map(|result| SearchResultResponse {
                    chunk_id: result.chunk_id,
                    doc_id: result.doc_id,
                    doc_title: result.doc_title,
                    section_path: result.section_path,
                    content: result.content,
                    score: result.score,
                    source: match result.source {
                        SearchSource::Bm25Only => "bm25",
                        SearchSource::VectorOnly => "vector",
                        SearchSource::Both => "both",
                    }
                    .to_string(),
                })
                .collect()
        })
}

pub fn rag_get_document_content(store: &RagStore, doc_id: &str) -> Result<String, String> {
    store
        .get_raw_content(doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No raw content stored for document {doc_id}"))
}

pub fn rag_update_document(
    store: &RagStore,
    doc_id: &str,
    content: String,
    expected_version: Option<u64>,
) -> Result<DocumentResponse, String> {
    if content.len() > MAX_CONTENT_SIZE {
        return Err("Document content too large".to_string());
    }
    let meta = store
        .get_doc_metadata(doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {doc_id}"))?;
    let now = chrono::Utc::now().timestamp_millis();
    let mut chunks = chunker::chunk_document(doc_id, &content, &meta.format);
    let hash = content_hash(&content);
    if hash != meta.content_hash
        && store
            .check_content_hash_exists_excluding_doc(&meta.collection_id, &hash, doc_id)
            .map_err(|e| e.to_string())?
    {
        return Err(format!(
            "Duplicate document: identical content already exists in this collection (hash: {})",
            &hash[..8]
        ));
    }
    for chunk in &mut chunks {
        chunk.context_prefix = Some(build_context_prefix(
            &meta.title,
            chunk.section_path.as_deref(),
        ));
    }
    let mut updated = store
        .update_document(doc_id, &content, &chunks, &hash, now, expected_version)
        .map_err(|e| e.to_string())?;
    bm25::reindex_all(store, None, None).map_err(|e| e.to_string())?;
    updated.chunk_count = chunks.len();
    Ok(document_response(updated))
}

pub fn rag_create_blank_document(
    store: &RagStore,
    request: CreateBlankDocumentRequest,
) -> Result<DocumentResponse, String> {
    if request.title.len() > MAX_NAME_LENGTH {
        return Err("Document title too long".to_string());
    }
    let doc_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let format = doc_format_from_wire(&request.format)?;
    let metadata = DocMetadata {
        id: doc_id,
        collection_id: request.collection_id,
        title: request.title,
        source_path: None,
        format,
        content_hash: String::new(),
        indexed_at: now,
        chunk_count: 0,
        version: 0,
    };
    store
        .add_document(&metadata, &[], Some(""))
        .map_err(|e| e.to_string())?;
    Ok(document_response(metadata))
}
