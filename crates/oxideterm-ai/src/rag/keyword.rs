//! Keyword retrieval shared by the native persistent index and the browser's document library.
use crate::rag_chunker::{chunk_document, estimate_tokens};
use crate::rag_model::{
    DocChunk, DocFormat, DocumentResponse, SearchRequest, SearchResultResponse,
};
use rust_stemmers::{Algorithm, Stemmer};
use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

const K1: f64 = 1.2;
pub(crate) const MAX_QUERY_LENGTH: usize = 10_000;
const B: f64 = 0.75;

/// Common English stop words filtered from BM25 tokens to reduce noise.
/// CJK tokens are not affected (bigram tokenizer has no equivalent concept).
static ENGLISH_STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "a",
        "about",
        "above",
        "after",
        "again",
        "against",
        "all",
        "also",
        "am",
        "an",
        "and",
        "any",
        "are",
        "as",
        "at",
        "be",
        "because",
        "been",
        "before",
        "being",
        "below",
        "between",
        "both",
        "but",
        "by",
        "can",
        "cannot",
        "could",
        "did",
        "do",
        "does",
        "doing",
        "down",
        "during",
        "each",
        "few",
        "for",
        "from",
        "further",
        "get",
        "got",
        "had",
        "has",
        "have",
        "having",
        "he",
        "her",
        "here",
        "hers",
        "herself",
        "him",
        "himself",
        "his",
        "how",
        "if",
        "in",
        "into",
        "is",
        "it",
        "its",
        "itself",
        "just",
        "let",
        "ll",
        "me",
        "might",
        "more",
        "most",
        "must",
        "my",
        "myself",
        "no",
        "nor",
        "not",
        "now",
        "of",
        "off",
        "on",
        "once",
        "only",
        "or",
        "other",
        "our",
        "ours",
        "ourselves",
        "out",
        "over",
        "own",
        "re",
        "same",
        "shall",
        "she",
        "should",
        "so",
        "some",
        "such",
        "than",
        "that",
        "the",
        "their",
        "theirs",
        "them",
        "themselves",
        "then",
        "there",
        "these",
        "they",
        "this",
        "those",
        "through",
        "to",
        "too",
        "under",
        "until",
        "up",
        "upon",
        "ve",
        "very",
        "was",
        "we",
        "were",
        "what",
        "when",
        "where",
        "which",
        "while",
        "who",
        "whom",
        "why",
        "will",
        "with",
        "won",
        "would",
        "you",
        "your",
        "yours",
        "yourself",
        "yourselves",
    ])
});

// ═══════════════════════════════════════════════════════════════════════════
// Tokenizer — character bigram for CJK, whitespace split for ASCII
// ═══════════════════════════════════════════════════════════════════════════

/// Tokenize text into a set of terms for BM25 indexing.
/// CJK characters are split into overlapping character bigrams.
/// ASCII/Latin text is split on whitespace and lowercased.
pub(crate) fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut ascii_buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    let mut i = 0;
    while i < len {
        let ch = chars[i];

        if is_cjk(ch) {
            // Flush any accumulated ASCII token
            flush_ascii(&mut ascii_buf, &mut tokens);

            // Emit character bigrams
            if i + 1 < len && is_cjk(chars[i + 1]) {
                let mut bigram = String::with_capacity(8);
                bigram.push(ch);
                bigram.push(chars[i + 1]);
                tokens.push(bigram);
            } else {
                // Single CJK char (end of run) — emit as unigram
                tokens.push(ch.to_string());
            }
            i += 1;
        } else if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            ascii_buf.push(ch.to_ascii_lowercase());
            i += 1;
        } else {
            // Whitespace or punctuation — flush ASCII buffer
            flush_ascii(&mut ascii_buf, &mut tokens);
            i += 1;
        }
    }
    flush_ascii(&mut ascii_buf, &mut tokens);
    tokens
}

/// Compute term frequency map: term → count.
pub(crate) fn term_frequencies(tokens: &[String]) -> HashMap<String, f32> {
    let mut freqs: HashMap<String, f32> = HashMap::new();
    for t in tokens {
        *freqs.entry(t.clone()).or_insert(0.0) += 1.0;
    }
    freqs
}

/// Snowball English stemmer (thread-safe, zero-alloc internal state).
static STEMMER: LazyLock<Stemmer> = LazyLock::new(|| Stemmer::create(Algorithm::English));

fn flush_ascii(buf: &mut String, tokens: &mut Vec<String>) {
    if !buf.is_empty() {
        // Filter out English stop words; allow all other tokens (including single-char
        // technical terms like "c", "r", "w" that are common in terminal/ops context)
        if !ENGLISH_STOP_WORDS.contains(buf.as_str()) {
            // Apply Snowball stemming so "deployments" and "deploying" match "deploy"
            let stemmed = STEMMER.stem(buf);
            tokens.push(stemmed.into_owned());
        }
        buf.clear();
    }
}

/// Check if a character belongs to a CJK script (Chinese, Japanese, Korean).
pub(crate) fn is_cjk(c: char) -> bool {
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

pub(crate) fn build_context_prefix(title: &str, section_path: Option<&str>) -> String {
    match section_path {
        Some(path) if !path.is_empty() => {
            format!("From document '{}', section: {}.", title, path)
        }
        _ => format!("From document '{}'.", title),
    }
}

pub(crate) fn doc_format_from_wire(value: &str) -> Result<DocFormat, String> {
    match value {
        "markdown" => Ok(DocFormat::Markdown),
        "plaintext" | "txt" => Ok(DocFormat::PlainText),
        other => Err(format!("Unsupported format: {other}")),
    }
}

pub(crate) fn idf(n: usize, df: usize) -> f64 {
    ((n as f64 - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln()
}

pub(crate) fn term_score(idf: f64, tf: f64, length: f64, average: f64) -> f64 {
    idf * ((tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * length / average)))
}

struct IndexedChunk {
    chunk: DocChunk,
    frequencies: HashMap<String, f32>,
    length: usize,
}

/// Searchable chunks owned by one in-memory document. Tokenization happens on edits, not searches.
pub struct KeywordDocument(Vec<IndexedChunk>);

impl KeywordDocument {
    pub fn new(meta: &DocumentResponse, content: &str) -> Result<Self, String> {
        let format = doc_format_from_wire(&meta.format)?;
        let chunks = chunk_document(&meta.id, content, &format);
        Ok(Self(
            chunks
                .into_iter()
                .map(|chunk| {
                    let (frequencies, length) = index_terms(&chunk, &meta.title);
                    IndexedChunk {
                        chunk,
                        frequencies,
                        length,
                    }
                })
                .collect(),
        ))
    }

    pub fn chunk_count(&self) -> usize {
        self.0.len()
    }

    pub fn retitle(&mut self, title: &str) {
        for entry in &mut self.0 {
            (entry.frequencies, entry.length) = index_terms(&entry.chunk, title);
        }
    }
}

fn index_terms(chunk: &DocChunk, title: &str) -> (HashMap<String, f32>, usize) {
    let prefix = build_context_prefix(title, chunk.section_path.as_deref());
    let text = format!("{prefix} {}", chunk.content);
    (term_frequencies(&tokenize(&text)), estimate_tokens(&text))
}

pub fn search_documents<'a>(
    documents: impl Iterator<Item = (&'a DocumentResponse, &'a KeywordDocument)> + Clone,
    request: &SearchRequest,
) -> Result<Vec<SearchResultResponse>, String> {
    if request.query.len() > MAX_QUERY_LENGTH {
        return Err("Search query too long".into());
    }
    let top_k = request.top_k.unwrap_or(5);
    let mut terms = tokenize(&request.query);
    terms.sort_unstable();
    terms.dedup();
    if top_k == 0 || terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut frequencies = vec![0; terms.len()];
    let mut count = 0;
    let mut total_length = 0;
    // Like the persistent index, compute IDF across the corpus, then filter eligible collections.
    for entry in documents.clone().flat_map(|(_, doc)| &doc.0) {
        count += 1;
        total_length += entry.length;
        for (term, df) in terms.iter().zip(&mut frequencies) {
            *df += usize::from(entry.frequencies.contains_key(term));
        }
    }
    if total_length == 0 {
        return Ok(Vec::new());
    }
    let average = total_length as f64 / count as f64;
    let weights: Vec<_> = terms
        .into_iter()
        .zip(frequencies)
        .filter(|(_, df)| *df > 0)
        .map(|(term, df)| (term, idf(count, df)))
        .collect();
    let mut matches = Vec::new();
    for (meta, doc) in documents {
        if !request.collection_ids.is_empty()
            && !request.collection_ids.contains(&meta.collection_id)
        {
            continue;
        }
        for entry in &doc.0 {
            let score: f64 = weights
                .iter()
                .filter_map(|(term, weight)| {
                    let tf = *entry.frequencies.get(term)?;
                    Some(term_score(
                        *weight,
                        f64::from(tf),
                        entry.length as f64,
                        average,
                    ))
                })
                .sum();
            if score > 0. {
                matches.push((score, meta, &entry.chunk));
            }
        }
    }
    matches.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.id.cmp(&b.1.id))
            .then_with(|| a.2.offset.cmp(&b.2.offset))
    });
    Ok(matches
        .into_iter()
        .take(top_k)
        .map(|(score, meta, chunk)| SearchResultResponse {
            chunk_id: chunk.id.clone(),
            doc_id: meta.id.clone(),
            doc_title: meta.title.clone(),
            section_path: chunk.section_path.clone(),
            content: chunk.content.clone(),
            score,
            source: "bm25".into(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_search_ranks_matching_sections_before_limiting_the_collection() {
        let docs: Vec<_> = [
            ("early", "ops", "# Service\nA deployment is scheduled."),
            (
                "private",
                "other",
                "# Secret\nDeploy deploy deploy. 数据库部署",
            ),
            (
                "runbook",
                "ops",
                "# Maintenance\nRestart the worker.\n## Database\nDeploy deployments: 数据库部署。",
            ),
        ]
        .into_iter()
        .map(|(id, collection, body)| {
            let meta = DocumentResponse {
                id: id.into(),
                collection_id: collection.into(),
                title: "Notes".into(),
                source_path: None,
                format: "markdown".into(),
                chunk_count: 0,
                indexed_at: 0,
                version: 1,
            };
            let doc = KeywordDocument::new(&meta, body).unwrap();
            (meta, doc)
        })
        .collect();
        let search = |query: &str, limit| {
            search_documents(
                docs.iter().map(|(m, d)| (m, d)),
                &SearchRequest {
                    query: query.into(),
                    collection_ids: vec!["ops".into()],
                    query_vector: None,
                    top_k: Some(limit),
                },
            )
            .unwrap()
        };
        let hits = search("deploying 部署", 1);
        assert_eq!(
            hits.iter()
                .map(|r| (
                    r.doc_id.as_str(),
                    r.section_path.as_deref(),
                    r.content.trim(),
                    r.source.as_str()
                ))
                .collect::<Vec<_>>(),
            [(
                "runbook",
                Some("Maintenance > Database"),
                "Deploy deployments: 数据库部署。",
                "bm25"
            )]
        );
        let hits = search("deploying 部署", 3);
        assert_eq!(
            hits.iter().map(|h| h.doc_id.as_str()).collect::<Vec<_>>(),
            ["runbook", "early"]
        );
        assert!(hits[0].score > hits[1].score);
        assert!(search("deploying", 0).is_empty());
        assert!(search("the", 5).is_empty());
    }
}
