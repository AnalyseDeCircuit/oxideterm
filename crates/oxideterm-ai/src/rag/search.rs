// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::rag::bm25::{Bm25Hit, search_bm25};
use crate::rag::embedding::{VectorHit, search_vector};
use crate::rag::error::RagError;
use crate::rag::store::RagStore;
use crate::rag::types::{SearchResult, SearchSource};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// Configuration
// ═══════════════════════════════════════════════════════════════════════════

/// Reciprocal Rank Fusion constant.
const RRF_K: f64 = 60.0;
/// Number of candidates from each retrieval path before fusion.
const CANDIDATES_PER_PATH: usize = 20;
/// MMR λ parameter: 0.0 = pure diversity, 1.0 = pure relevance.
const MMR_LAMBDA: f64 = 0.7;
/// Minimum relevance threshold: results with normalized score below this
/// fraction of the top score are discarded.
const MIN_RELEVANCE_RATIO: f64 = 0.15;

// ═══════════════════════════════════════════════════════════════════════════
// Hybrid Search
// ═══════════════════════════════════════════════════════════════════════════

/// Search mode determining which retrieval paths to use.
pub enum SearchMode {
    /// BM25 keyword search only (no embeddings required).
    KeywordOnly,
    /// Full hybrid: BM25 + vector similarity with RRF fusion.
    Hybrid { query_vector: Vec<f32> },
}

/// Perform a hybrid search across the given collections.
///
/// Pipeline:
///   1. BM25 keyword search → Top-20 candidates
///   2. (If hybrid) Vector similarity → Top-20 candidates
///   3. Reciprocal Rank Fusion (k=60) → merged ranking
///   4. Minimum relevance threshold → filter low-quality results
///   5. (If hybrid) MMR diversity reranking → diverse top-K
///   6. Enrich with chunk metadata → final results
pub fn search(
    store: &RagStore,
    query: &str,
    collection_ids: &[String],
    mode: SearchMode,
    top_k: usize,
) -> Result<Vec<SearchResult>, RagError> {
    let is_hybrid = matches!(mode, SearchMode::Hybrid { .. });

    // Phase 1: BM25
    let bm25_hits = search_bm25(store, query, collection_ids, CANDIDATES_PER_PATH)?;

    // Phase 2: Vector (optional)
    let vector_hits = match &mode {
        SearchMode::KeywordOnly => Vec::new(),
        SearchMode::Hybrid { query_vector } => {
            search_vector(store, query_vector, collection_ids, CANDIDATES_PER_PATH)?
        }
    };

    // Phase 3: RRF fusion
    let mut fused = rrf_fuse(&bm25_hits, &vector_hits);

    // Phase 4: Minimum relevance threshold
    if let Some(max_score) = fused.first().map(|(_, s, _)| *s) {
        if max_score > 0.0 {
            let threshold = max_score * MIN_RELEVANCE_RATIO;
            fused.retain(|(_, score, _)| *score >= threshold);
        }
    }

    // Phase 5: MMR diversity reranking (hybrid mode only)
    // Overfetch 2×top_k candidates, then diversify to top_k
    let selected = if is_hybrid && fused.len() > top_k {
        let candidates: Vec<(String, f64, SearchSource)> =
            fused.into_iter().take(top_k * 2).collect();
        apply_mmr(store, &candidates, top_k)?
    } else {
        fused.into_iter().take(top_k).collect()
    };

    // Phase 6: Batch-load chunks and metadata, then assemble results
    let top_chunk_ids: Vec<String> = selected.iter().map(|(id, _, _)| id.clone()).collect();
    let chunks_map = store.get_chunks_batch(&top_chunk_ids)?;

    let doc_ids: Vec<String> = chunks_map
        .values()
        .map(|c| c.doc_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let meta_map = store.get_doc_metadata_batch(&doc_ids)?;

    let mut results = Vec::with_capacity(selected.len());
    for (chunk_id, score, source) in &selected {
        if let Some(chunk) = chunks_map.get(chunk_id) {
            let doc_title = meta_map
                .get(&chunk.doc_id)
                .map(|m| m.title.clone())
                .unwrap_or_default();

            results.push(SearchResult {
                chunk_id: chunk_id.clone(),
                doc_id: chunk.doc_id.clone(),
                doc_title,
                section_path: chunk.section_path.clone(),
                content: chunk.content.clone(),
                score: *score,
                source: source.clone(),
            });
        }
    }

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════════════
// Reciprocal Rank Fusion
// ═══════════════════════════════════════════════════════════════════════════

/// Reciprocal Rank Fusion: merge two ranked lists.
/// Returns (chunk_id, fused_score, source) sorted by fused score descending.
fn rrf_fuse(bm25_hits: &[Bm25Hit], vector_hits: &[VectorHit]) -> Vec<(String, f64, SearchSource)> {
    let mut scores: HashMap<String, (f64, bool, bool)> = HashMap::new();

    // BM25 contributions
    for (rank, hit) in bm25_hits.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = scores
            .entry(hit.chunk_id.clone())
            .or_insert((0.0, false, false));
        entry.0 += rrf_score;
        entry.1 = true; // seen in BM25
    }

    // Vector contributions
    for (rank, hit) in vector_hits.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = scores
            .entry(hit.chunk_id.clone())
            .or_insert((0.0, false, false));
        entry.0 += rrf_score;
        entry.2 = true; // seen in vector
    }

    let mut results: Vec<(String, f64, SearchSource)> = scores
        .into_iter()
        .map(|(id, (score, in_bm25, in_vector))| {
            let source = match (in_bm25, in_vector) {
                (true, true) => SearchSource::Both,
                (true, false) => SearchSource::Bm25Only,
                (false, true) => SearchSource::VectorOnly,
                (false, false) => unreachable!(),
            };
            (id, score, source)
        })
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

// ═══════════════════════════════════════════════════════════════════════════
// MMR Diversity Reranking
// ═══════════════════════════════════════════════════════════════════════════

/// Maximal Marginal Relevance: select `top_k` results that balance
/// relevance (from RRF score) and diversity (low inter-chunk cosine similarity).
///
/// Falls back to simple truncation when no embeddings are available.
fn apply_mmr(
    store: &RagStore,
    candidates: &[(String, f64, SearchSource)],
    top_k: usize,
) -> Result<Vec<(String, f64, SearchSource)>, RagError> {
    if candidates.len() <= top_k {
        return Ok(candidates.to_vec());
    }

    // Load embeddings for candidate chunks
    let chunk_ids: Vec<String> = candidates.iter().map(|(id, _, _)| id.clone()).collect();
    let embeddings = store.get_embeddings_for_chunks(&chunk_ids)?;

    // Build map: chunk_id → vector
    let emb_map: HashMap<String, &[f32]> = embeddings
        .iter()
        .map(|e| (e.chunk_id.clone(), e.vector.as_slice()))
        .collect();

    // If few embeddings available, fall back to simple truncation
    if emb_map.len() < 2 {
        return Ok(candidates.iter().take(top_k).cloned().collect());
    }

    // Normalize candidate scores to [0, 1] for MMR formula
    let max_score = candidates
        .iter()
        .map(|(_, s, _)| *s)
        .fold(0.0_f64, f64::max);
    let norm = if max_score > 0.0 { max_score } else { 1.0 };

    let mut selected: Vec<(String, f64, SearchSource)> = Vec::with_capacity(top_k);
    let mut remaining: Vec<usize> = (0..candidates.len()).collect();

    while selected.len() < top_k && !remaining.is_empty() {
        let mut best_idx_in_remaining = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (ri, &ci) in remaining.iter().enumerate() {
            let (ref cand_id, cand_score, _) = candidates[ci];
            let relevance = cand_score / norm;

            // Max cosine similarity to already selected items
            let max_sim = if selected.is_empty() {
                0.0
            } else if let Some(cand_vec) = emb_map.get(cand_id) {
                selected
                    .iter()
                    .filter_map(|(sel_id, _, _)| emb_map.get(sel_id))
                    .map(|sel_vec| cosine_sim(cand_vec, sel_vec))
                    .fold(0.0_f64, f64::max)
            } else {
                0.0 // No embedding for this chunk — assume zero similarity
            };

            let mmr_score = MMR_LAMBDA * relevance - (1.0 - MMR_LAMBDA) * max_sim;

            if mmr_score > best_mmr {
                best_mmr = mmr_score;
                best_idx_in_remaining = ri;
            }
        }

        let ci = remaining.remove(best_idx_in_remaining);
        selected.push(candidates[ci].clone());
    }

    Ok(selected)
}

/// Fast cosine similarity between two vectors (for MMR inter-chunk diversity).
fn cosine_sim(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b.iter()) {
        let (xf, yf) = (*x as f64, *y as f64);
        dot += xf * yf;
        na += xf * xf;
        nb += yf * yf;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_fuse_overlap() {
        let bm25 = vec![
            Bm25Hit {
                chunk_id: "a".into(),
                score: 5.0,
            },
            Bm25Hit {
                chunk_id: "b".into(),
                score: 3.0,
            },
            Bm25Hit {
                chunk_id: "c".into(),
                score: 1.0,
            },
        ];
        let vector = vec![
            VectorHit {
                chunk_id: "b".into(),
                score: 0.95,
            },
            VectorHit {
                chunk_id: "d".into(),
                score: 0.80,
            },
            VectorHit {
                chunk_id: "a".into(),
                score: 0.60,
            },
        ];

        let fused = rrf_fuse(&bm25, &vector);

        // "a" and "b" appear in both lists, should have higher combined scores
        let a_score = fused.iter().find(|(id, _, _)| id == "a").unwrap().1;
        let b_score = fused.iter().find(|(id, _, _)| id == "b").unwrap().1;
        let c_score = fused.iter().find(|(id, _, _)| id == "c").unwrap().1;
        let d_score = fused.iter().find(|(id, _, _)| id == "d").unwrap().1;

        // Items in both lists should score higher than items in one
        assert!(a_score > c_score);
        assert!(b_score > d_score);

        // Check source annotations
        let a_source = &fused.iter().find(|(id, _, _)| id == "a").unwrap().2;
        assert!(matches!(a_source, SearchSource::Both));
        let c_source = &fused.iter().find(|(id, _, _)| id == "c").unwrap().2;
        assert!(matches!(c_source, SearchSource::Bm25Only));
        let d_source = &fused.iter().find(|(id, _, _)| id == "d").unwrap().2;
        assert!(matches!(d_source, SearchSource::VectorOnly));
    }

    #[test]
    fn test_rrf_fuse_empty() {
        let fused = rrf_fuse(&[], &[]);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_rrf_bm25_only() {
        let bm25 = vec![Bm25Hit {
            chunk_id: "x".into(),
            score: 2.0,
        }];
        let fused = rrf_fuse(&bm25, &[]);
        assert_eq!(fused.len(), 1);
        assert!(matches!(fused[0].2, SearchSource::Bm25Only));
    }

    #[test]
    fn test_cosine_sim_identical() {
        let a = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_sim(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_sim_orthogonal() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!(cosine_sim(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_sim_dimension_mismatch() {
        let a = vec![1.0f32, 2.0];
        let b = vec![1.0f32];
        assert_eq!(cosine_sim(&a, &b), 0.0);
    }
}
