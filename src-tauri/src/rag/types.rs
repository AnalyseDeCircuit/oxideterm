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

/// Input struct for storing embeddings from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingInput {
    pub chunk_id: String,
    pub vector: Vec<f32>,
    pub model_name: String,
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

    // ─── CJK Unified ──────────────────────────────────────────────────

    #[test]
    fn test_is_cjk_chinese_character() {
        assert!(is_cjk('中')); // U+4E2D — CJK Unified
        assert!(is_cjk('国')); // U+56FD
    }

    #[test]
    fn test_is_cjk_cjk_extension_a() {
        assert!(is_cjk('\u{3400}')); // first char of Extension A
        assert!(is_cjk('\u{4DBF}')); // last char of Extension A
    }

    #[test]
    fn test_is_cjk_compat_ideograph() {
        assert!(is_cjk('\u{F900}')); // CJK Compat Ideographs start
    }

    // ─── CJK Symbols ─────────────────────────────────────────────────

    #[test]
    fn test_is_cjk_symbols() {
        assert!(is_cjk('〇')); // U+3007 — CJK Symbols & Punctuation
        assert!(is_cjk('\u{3000}')); // Ideographic space
    }

    // ─── Hiragana / Katakana ──────────────────────────────────────────

    #[test]
    fn test_is_cjk_hiragana() {
        assert!(is_cjk('あ')); // U+3042
        assert!(is_cjk('ん')); // U+3093
    }

    #[test]
    fn test_is_cjk_katakana() {
        assert!(is_cjk('ア')); // U+30A2
        assert!(is_cjk('ン')); // U+30F3
    }

    // ─── Hangul ───────────────────────────────────────────────────────

    #[test]
    fn test_is_cjk_hangul() {
        assert!(is_cjk('가')); // U+AC00 — first Hangul syllable
        assert!(is_cjk('힣')); // U+D7A3 — last Hangul syllable
    }

    // ─── Non-CJK ─────────────────────────────────────────────────────

    #[test]
    fn test_is_not_cjk_ascii() {
        assert!(!is_cjk('A'));
        assert!(!is_cjk('z'));
        assert!(!is_cjk('0'));
        assert!(!is_cjk(' '));
    }

    #[test]
    fn test_is_not_cjk_latin_extended() {
        assert!(!is_cjk('é')); // Latin
        assert!(!is_cjk('ñ'));
    }

    #[test]
    fn test_is_not_cjk_cyrillic() {
        assert!(!is_cjk('Д')); // Cyrillic
    }

    #[test]
    fn test_is_not_cjk_arabic() {
        assert!(!is_cjk('ع')); // Arabic
    }

    // ─── DocScope equality ────────────────────────────────────────────

    #[test]
    fn test_doc_scope_equality() {
        assert_eq!(DocScope::Global, DocScope::Global);
        assert_eq!(
            DocScope::Connection("abc".into()),
            DocScope::Connection("abc".into())
        );
        assert_ne!(DocScope::Global, DocScope::Connection("abc".into()));
    }

    // ─── DocFormat equality ───────────────────────────────────────────

    #[test]
    fn test_doc_format_equality() {
        assert_eq!(DocFormat::Markdown, DocFormat::Markdown);
        assert_eq!(DocFormat::PlainText, DocFormat::PlainText);
        assert_ne!(DocFormat::Markdown, DocFormat::PlainText);
    }
}
