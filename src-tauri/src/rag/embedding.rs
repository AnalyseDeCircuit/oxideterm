// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::rag::error::RagError;
use crate::rag::store::RagStore;
use crate::rag::types::EmbeddingRecord;
use std::collections::HashSet;
use tracing::debug;

// ═══════════════════════════════════════════════════════════════════════════
// Vector Search
// ═══════════════════════════════════════════════════════════════════════════

/// A scored chunk from vector similarity search.
#[derive(Debug, Clone)]
pub struct VectorHit {
    pub chunk_id: String,
    pub score: f64,
}

/// Search by cosine similarity against stored embeddings.
/// `query_vector` comes from the provider's embedding API.
/// Only chunks within `collection_ids` are considered.
///
/// If an HNSW index is provided and its dimensions match, uses approximate
/// nearest neighbor search (O(log n)). Otherwise falls back to brute-force
/// cosine scan (O(n)).
pub fn search_vector(
    store: &RagStore,
    query_vector: &[f32],
    collection_ids: &[String],
    top_k: usize,
) -> Result<Vec<VectorHit>, RagError> {
    if query_vector.is_empty() {
        return Ok(Vec::new());
    }

    // Match BM25 semantics: an empty collection filter means search across all collections.
    let chunk_ids = if collection_ids.is_empty() {
        let all_collection_ids = store.get_all_collection_ids()?;
        store.get_chunk_ids_in_collections(&all_collection_ids)?
    } else {
        store.get_chunk_ids_in_collections(collection_ids)?
    };
    if chunk_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Try HNSW path first
    if let Ok(guard) = store.hnsw_index().read() {
        if let Some(ref index) = *guard {
            if index.is_compatible(query_vector.len()) {
                let allowed: HashSet<String> = chunk_ids.iter().cloned().collect();
                let results = index.search(query_vector, top_k, Some(&allowed));
                debug!(
                    "HNSW search returned {} results (allowed set: {}, top_k: {})",
                    results.len(),
                    allowed.len(),
                    top_k
                );
                if !results.is_empty() {
                    return Ok(results);
                }
                // If HNSW returned nothing (all filtered out), fall through to brute-force
            }
        }
    }

    // Brute-force fallback
    search_vector_bruteforce(store, query_vector, &chunk_ids, top_k)
}

/// Brute-force cosine similarity scan (original O(n) path).
fn search_vector_bruteforce(
    store: &RagStore,
    query_vector: &[f32],
    chunk_ids: &[String],
    top_k: usize,
) -> Result<Vec<VectorHit>, RagError> {
    // Fetch embeddings for these chunks
    let embeddings = store.get_embeddings_for_chunks(chunk_ids)?;
    if embeddings.is_empty() {
        return Ok(Vec::new());
    }

    // Pre-compute query norm
    let query_norm = l2_norm(query_vector);
    if query_norm == 0.0 {
        return Ok(Vec::new());
    }

    // Score each embedding
    let mut hits: Vec<VectorHit> = embeddings
        .iter()
        .filter_map(|emb| {
            if emb.vector.len() != query_vector.len() {
                return None; // dimension mismatch
            }
            let score = cosine_similarity(query_vector, &emb.vector, query_norm);
            Some(VectorHit {
                chunk_id: emb.chunk_id.clone(),
                score,
            })
        })
        .collect();

    // Sort descending by score
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(top_k);

    Ok(hits)
}

/// Get chunk IDs that need embedding (not yet embedded).
pub fn get_pending_embeddings(
    store: &RagStore,
    collection_id: &str,
    limit: usize,
) -> Result<Vec<(String, String)>, RagError> {
    store.get_unembedded_chunk_ids(collection_id, limit)
}

/// Store embedding results from the provider.
/// Invalidates the HNSW index since the embedding set has changed.
pub fn store_embeddings(
    store: &RagStore,
    embeddings: Vec<EmbeddingRecord>,
) -> Result<usize, RagError> {
    let count = embeddings.len();
    store.store_embeddings_batch(&embeddings)?;
    // Mark HNSW index as stale — will be rebuilt asynchronously
    store.invalidate_hnsw_index();
    Ok(count)
}

// ═══════════════════════════════════════════════════════════════════════════
// Math Utilities
// ═══════════════════════════════════════════════════════════════════════════

/// Cosine similarity with pre-computed query norm.
fn cosine_similarity(a: &[f32], b: &[f32], a_norm: f32) -> f64 {
    let b_norm = l2_norm(b);
    if b_norm == 0.0 {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    (dot / (a_norm * b_norm)) as f64
}

/// L2 (Euclidean) norm of a vector.
fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::store::RagStore;
    use crate::rag::types::{DocCollection, DocFormat, DocMetadata, DocScope};

    fn temp_store(test_name: &str) -> RagStore {
        let dir = std::env::temp_dir().join(format!(
            "oxideterm_rag_embedding_{}_{}_{}",
            test_name,
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        RagStore::new(&dir).unwrap()
    }

    fn add_chunk_with_embedding(
        store: &RagStore,
        collection_id: &str,
        doc_id: &str,
        chunk_id: &str,
        content: &str,
        vector: Vec<f32>,
    ) {
        let now = chrono::Utc::now().timestamp_millis();
        store
            .create_collection(&DocCollection {
                id: collection_id.to_string(),
                name: collection_id.to_string(),
                scope: DocScope::Global,
                created_at: now,
                updated_at: now,
            })
            .unwrap();

        let metadata = DocMetadata {
            id: doc_id.to_string(),
            collection_id: collection_id.to_string(),
            title: doc_id.to_string(),
            source_path: None,
            format: DocFormat::PlainText,
            content_hash: format!("hash-{doc_id}"),
            indexed_at: now,
            chunk_count: 1,
            version: 0,
        };
        let chunk = crate::rag::types::DocChunk {
            id: chunk_id.to_string(),
            doc_id: doc_id.to_string(),
            section_path: None,
            content: content.to_string(),
            tokens_estimate: 1,
            offset: 0,
            length: content.len(),
            context_prefix: None,
        };
        store
            .add_document(&metadata, &[chunk], Some(content))
            .unwrap();
        store
            .store_embeddings_batch(&[EmbeddingRecord {
                chunk_id: chunk_id.to_string(),
                dimensions: vector.len(),
                model_name: "test-model".to_string(),
                vector,
            }])
            .unwrap();
    }

    #[test]
    fn test_cosine_identical() {
        let a = vec![1.0, 0.0, 1.0];
        let norm = l2_norm(&a);
        let sim = cosine_similarity(&a, &a, norm);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let norm = l2_norm(&a);
        let sim = cosine_similarity(&a, &b, norm);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let norm = l2_norm(&a);
        let sim = cosine_similarity(&a, &b, norm);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_norm() {
        let v = vec![3.0, 4.0];
        assert!((l2_norm(&v) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_zero_vector() {
        let a = vec![1.0, 2.0];
        let b = vec![0.0, 0.0];
        let norm = l2_norm(&a);
        let sim = cosine_similarity(&a, &b, norm);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_search_vector_empty_collection_filter_searches_all_collections() {
        let store = temp_store("all_collections");
        add_chunk_with_embedding(&store, "col-a", "doc-a", "chunk-a", "alpha", vec![1.0, 0.0]);
        add_chunk_with_embedding(&store, "col-b", "doc-b", "chunk-b", "beta", vec![0.0, 1.0]);

        let results = search_vector(&store, &[0.9, 0.1], &[], 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].chunk_id, "chunk-a");
        assert!(results.iter().any(|hit| hit.chunk_id == "chunk-b"));
    }
}
