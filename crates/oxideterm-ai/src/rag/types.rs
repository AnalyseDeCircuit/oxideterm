// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Core Types
// ═══════════════════════════════════════════════════════════════════════════

/// A collection of documents with a defined scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocCollection {
    pub id: String,
    pub name: String,
    pub scope: DocScope,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Scope determines which sessions can see a collection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DocScope {
    /// Visible to all sessions.
    Global,
    /// Visible only when connected to the given connection_id.
    Connection(String),
}

/// Metadata for an imported document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocMetadata {
    pub id: String,
    pub collection_id: String,
    pub title: String,
    pub source_path: Option<String>,
    pub format: DocFormat,
    pub content_hash: String,
    pub indexed_at: i64,
    pub chunk_count: usize,
    /// Monotonically increasing version for optimistic locking.
    #[serde(default)]
    pub version: u64,
}

/// Supported document formats.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DocFormat {
    Markdown,
    PlainText,
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

/// Stored vector embedding for a chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRecord {
    pub chunk_id: String,
    pub vector: Vec<f32>,
    pub model_name: String,
    pub dimensions: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// Search Types
// ═══════════════════════════════════════════════════════════════════════════

/// A single search result with provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub doc_id: String,
    pub doc_title: String,
    pub section_path: Option<String>,
    pub content: String,
    pub score: f64,
    pub source: SearchSource,
}

/// Indicates which retrieval path produced the result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchSource {
    Bm25Only,
    VectorOnly,
    Both,
}

/// Statistics for a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    pub doc_count: usize,
    pub chunk_count: usize,
    pub embedded_chunk_count: usize,
    pub last_updated: i64,
}

// ═══════════════════════════════════════════════════════════════════════════
// BM25 Internal Types
// ═══════════════════════════════════════════════════════════════════════════

/// A posting list entry: chunk_id + term frequency + document length.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingEntry {
    pub chunk_id: String,
    pub tf: f32,
    /// Token count of the chunk (for BM25 length normalization).
    #[serde(default)]
    pub doc_length: usize,
}

/// Global BM25 statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25Stats {
    /// Total number of indexed chunks.
    pub doc_count: usize,
    /// Average document length in tokens.
    pub avg_dl: f64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Check if a character belongs to a CJK script (Chinese, Japanese, Korean).
pub fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compat
        | '\u{3000}'..='\u{303F}' // CJK Symbols
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{AC00}'..='\u{D7AF}' // Hangul
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_cjk_scripts() {
        for (script, samples) in [
            ("CJK Unified", &['中', '国'][..]),
            ("CJK Extension A", &['\u{3400}', '\u{4DBF}'][..]),
            ("CJK Compat", &['\u{F900}'][..]),
            ("CJK Symbols", &['〇', '\u{3000}'][..]),
            ("Hiragana", &['あ', 'ん'][..]),
            ("Katakana", &['ア', 'ン'][..]),
            ("Hangul", &['가', '힣'][..]),
        ] {
            for character in samples {
                assert!(is_cjk(*character), "{script}: {character:?}");
            }
        }
    }

    #[test]
    fn rejects_non_cjk_scripts() {
        for (script, samples) in [
            ("ASCII", &['A', 'z', '0', ' '][..]),
            ("Latin", &['é', 'ñ'][..]),
            ("Cyrillic", &['Д'][..]),
            ("Arabic", &['ع'][..]),
        ] {
            for character in samples {
                assert!(!is_cjk(*character), "{script}: {character:?}");
            }
        }
    }
}
