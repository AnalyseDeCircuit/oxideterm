// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

/// Scope determines which sessions can see a collection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DocScope {
    /// Visible to all sessions.
    Global,
    /// Visible only when connected to the given connection_id.
    Connection(String),
}

/// A chunk of text split from a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocChunk {
    pub id: String,
    pub doc_id: String,
    /// Heading path for Markdown, e.g. "Deployment > Docker > Troubleshooting"
    pub section_path: Option<String>,
    pub content: String,
    pub tokens_estimate: usize,
    /// Character offset in the original document.
    pub offset: usize,
    /// Content length in characters.
    pub length: usize,
    /// Contextual header derived from document title + section_path.
    /// Prepended to content for BM25 indexing and embedding to improve
    /// retrieval quality (inspired by Anthropic Contextual Retrieval).
    #[serde(default)]
    pub context_prefix: Option<String>,
}

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
pub struct DocumentContentResponse {
    pub document: DocumentResponse,
    pub content: String,
    pub semantic_index: SemanticIndexState,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum KeywordIndexState {
    Ready,
    Pending,
    Rebuilding,
    Failed { message: String },
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum SemanticIndexState {
    Ready,
    Pending { chunk_count: usize },
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSaveOutcome {
    pub document: DocumentResponse,
    pub keyword_index: KeywordIndexState,
    pub semantic_index: SemanticIndexState,
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
