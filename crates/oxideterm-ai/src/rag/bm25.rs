// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::rag::chunker::estimate_tokens;
use crate::rag::error::RagError;
use crate::rag::store::RagStore;
use crate::rag::types::{Bm25Stats, PostingEntry};
use crate::rag_keyword::{idf, term_frequencies, term_score, tokenize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

// ═══════════════════════════════════════════════════════════════════════════
// BM25 Index Building
// ═══════════════════════════════════════════════════════════════════════════

/// Rebuild the global BM25 index from all collections.
///
/// `cancel` — if provided, checked periodically; if set to `true`, the
///   rebuild aborts early and returns `RagError::Cancelled`.
/// `on_progress` — called with (current, total) after each chunk is processed.
pub fn reindex_all(
    store: &RagStore,
    cancel: Option<&AtomicBool>,
    mut on_progress: Option<&mut dyn FnMut(usize, usize)>,
) -> Result<usize, RagError> {
    // All global rebuilds share one owner so an import, removal, manual rebuild, and autosave
    // cannot race and let an older snapshot overwrite a newer index.
    let _operation_guard = store.bm25_operation_lock()?;
    let source_revision = store.bm25_content_revision()?;
    let all_col_ids = store.get_all_collection_ids()?;
    let chunk_ids = store.get_chunk_ids_in_collections(&all_col_ids)?;
    let total = chunk_ids.len();

    let mut postings: HashMap<String, Vec<PostingEntry>> = HashMap::new();
    let mut total_dl: f64 = 0.0;
    let mut count: usize = 0;

    for (idx, cid) in chunk_ids.iter().enumerate() {
        // Check cancellation
        if let Some(flag) = cancel {
            if flag.load(Ordering::Relaxed) {
                return Err(RagError::Cancelled);
            }
        }

        if let Some(chunk) = store.get_chunk(cid)? {
            // Prepend context_prefix for context-aware indexing
            let full_text = match &chunk.context_prefix {
                Some(prefix) if !prefix.is_empty() => format!("{} {}", prefix, chunk.content),
                _ => chunk.content.clone(),
            };
            let tokens = tokenize(&full_text);
            let doc_length = estimate_tokens(&full_text);
            let tf_map = term_frequencies(&tokens);

            for (term, tf) in &tf_map {
                postings
                    .entry(term.clone())
                    .or_default()
                    .push(PostingEntry {
                        chunk_id: cid.clone(),
                        tf: *tf,
                        doc_length,
                    });
            }

            total_dl += doc_length as f64;
            count += 1;
        }

        // Report progress
        if let Some(ref mut cb) = on_progress {
            cb(idx + 1, total);
        }
    }

    let avg_dl = if count > 0 {
        total_dl / count as f64
    } else {
        0.0
    };
    let stats = Bm25Stats {
        doc_count: count,
        avg_dl,
    };

    store.write_bm25_index(&postings, &stats, source_revision)?;
    Ok(count)
}

// ═══════════════════════════════════════════════════════════════════════════
// BM25 Search
// ═══════════════════════════════════════════════════════════════════════════

/// A scored chunk from BM25 search.
#[derive(Debug, Clone)]
pub struct Bm25Hit {
    pub chunk_id: String,
    pub score: f64,
}

/// Search using BM25 scoring, returning top-K results.
pub fn search_bm25(
    store: &RagStore,
    query: &str,
    collection_ids: &[String],
    top_k: usize,
) -> Result<Vec<Bm25Hit>, RagError> {
    let query_tokens = tokenize(query);
    let query_terms: Vec<String> = query_tokens
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if query_terms.is_empty() {
        return Ok(Vec::new());
    }

    let stats = store.get_bm25_stats()?;
    let (doc_count, avg_dl) = match stats {
        Some(s) => (s.doc_count, s.avg_dl),
        None => return Ok(Vec::new()),
    };

    if doc_count == 0 || avg_dl <= 0.0 {
        return Ok(Vec::new());
    }

    // Subset of chunk IDs in the target collections (for filtering)
    let valid_chunks: std::collections::HashSet<String> = if collection_ids.is_empty() {
        std::collections::HashSet::new()
    } else {
        store
            .get_chunk_ids_in_collections(collection_ids)?
            .into_iter()
            .collect()
    };

    let filter = !collection_ids.is_empty();

    // Accumulate scores
    let mut scores: HashMap<String, f64> = HashMap::new();

    for term in &query_terms {
        let postings = store.get_bm25_postings(term)?;
        if postings.is_empty() {
            continue;
        }

        // IDF: log((N - df + 0.5) / (df + 0.5) + 1)
        let idf = idf(doc_count, postings.len());

        for entry in &postings {
            if filter && !valid_chunks.contains(&entry.chunk_id) {
                continue;
            }

            // BM25 TF component: (tf * (k1 + 1)) / (tf + k1 * (1 - b + b * dl/avgdl))
            let tf = entry.tf as f64;
            let dl = if entry.doc_length > 0 {
                entry.doc_length as f64
            } else {
                // Fallback for legacy postings without stored doc_length
                avg_dl
            };
            *scores.entry(entry.chunk_id.clone()).or_insert(0.0) += term_score(idf, tf, dl, avg_dl);
        }
    }

    // Sort by score descending
    let mut hits: Vec<Bm25Hit> = scores
        .into_iter()
        .map(|(chunk_id, score)| Bm25Hit { chunk_id, score })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(top_k);

    Ok(hits)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenization_handles_supported_text_classes() {
        let tokens = tokenize("Hello world, this is a test!");
        // Stemmed: "hello" → "hello", "world" → "world", "test" → "test"
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"test".to_string()));
        // "this" and "is" are stop words — should be filtered
        assert!(!tokens.contains(&"this".to_string()));
        assert!(!tokens.contains(&"is".to_string()));
        // Single chars like "a" should be filtered (stop word)
        assert!(!tokens.contains(&"a".to_string()));
        let tokens = tokenize("the quick brown fox jumps over the lazy dog");
        // Stop words filtered
        assert!(!tokens.contains(&"the".to_string()));
        assert!(!tokens.contains(&"over".to_string()));
        // Content words preserved (stemmed forms)
        assert!(tokens.contains(&"quick".to_string()));
        assert!(tokens.contains(&"brown".to_string()));
        assert!(tokens.contains(&"fox".to_string()));
        assert!(tokens.contains(&"jump".to_string())); // "jumps" → "jump"
        assert!(tokens.contains(&"lazi".to_string())); // "lazy" → "lazi"
        assert!(tokens.contains(&"dog".to_string()));
        // Snowball English stemmer should normalize inflected forms
        let tokens = tokenize("deployments deploying deployed containers running");
        assert!(tokens.contains(&"deploy".to_string())); // all three → "deploy"
        assert_eq!(tokens.iter().filter(|t| *t == "deploy").count(), 3);
        assert!(tokens.contains(&"contain".to_string())); // "containers" → "contain"
        assert!(tokens.contains(&"run".to_string())); // "running" → "run"
        let tokens = tokenize("运维文档");
        // Should produce bigrams: "运维", "维文", "文档"
        assert!(tokens.contains(&"运维".to_string()));
        assert!(tokens.contains(&"维文".to_string()));
        assert!(tokens.contains(&"文档".to_string()));
        let tokens = tokenize("Docker 部署指南 version 2.0");
        assert!(tokens.contains(&"docker".to_string()));
        assert!(tokens.contains(&"部署".to_string()));
        assert!(tokens.contains(&"署指".to_string()));
        assert!(tokens.contains(&"指南".to_string()));
        assert!(tokens.contains(&"version".to_string())); // "version" stem = "version"
        // "2" and "0" are single-char non-stop tokens — kept
        assert!(tokens.contains(&"2".to_string()));
        assert!(tokens.contains(&"0".to_string()));

        assert!(tokenize("").is_empty());
    }
}
