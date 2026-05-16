// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::rag::types::{DocChunk, DocFormat, is_cjk};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════
// Configuration
// ═══════════════════════════════════════════════════════════════════════════

/// Target chunk size range in estimated tokens.
const MAX_CHUNK_TOKENS: usize = 1500;
/// Overlap window in characters to avoid semantic breaks.
const OVERLAP_CHARS: usize = 200;

// ═══════════════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════════════

/// Split a document into chunks suitable for indexing.
pub fn chunk_document(doc_id: &str, content: &str, format: &DocFormat) -> Vec<DocChunk> {
    match format {
        DocFormat::Markdown => chunk_markdown(doc_id, content),
        DocFormat::PlainText => chunk_plaintext(doc_id, content),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Markdown Chunker
// ═══════════════════════════════════════════════════════════════════════════

/// Split Markdown by heading hierarchy, producing section-aware chunks.
fn chunk_markdown(doc_id: &str, content: &str) -> Vec<DocChunk> {
    let mut chunks = Vec::new();
    let mut heading_stack: Vec<(usize, String)> = Vec::new(); // (level, title)
    let mut current_text = String::new();
    let mut section_offset: usize = 0;

    for (line_offset, line) in line_offsets(content) {
        if let Some((level, title)) = parse_heading(line) {
            // Flush accumulated text before this heading
            if !current_text.trim().is_empty() {
                let section_path = build_section_path(&heading_stack);
                emit_chunks(
                    doc_id,
                    &current_text,
                    section_path.as_deref(),
                    section_offset,
                    &mut chunks,
                );
            }

            // Update heading stack
            while heading_stack.last().map_or(false, |(l, _)| *l >= level) {
                heading_stack.pop();
            }
            heading_stack.push((level, title));
            current_text.clear();
            section_offset = line_offset;
        } else {
            if current_text.is_empty() && line.trim().is_empty() {
                continue; // skip leading blank lines
            }
            current_text.push_str(line);
            current_text.push('\n');
        }
    }

    // Flush remaining text
    if !current_text.trim().is_empty() {
        let section_path = build_section_path(&heading_stack);
        emit_chunks(
            doc_id,
            &current_text,
            section_path.as_deref(),
            section_offset,
            &mut chunks,
        );
    }

    // Edge case: empty document or no headings
    if chunks.is_empty() && !content.trim().is_empty() {
        emit_chunks(doc_id, content, None, 0, &mut chunks);
    }

    chunks
}

// ═══════════════════════════════════════════════════════════════════════════
// Plain Text Chunker
// ═══════════════════════════════════════════════════════════════════════════

/// Split plain text by paragraph boundaries (double newlines).
fn chunk_plaintext(doc_id: &str, content: &str) -> Vec<DocChunk> {
    let mut chunks = Vec::new();
    emit_chunks(doc_id, content, None, 0, &mut chunks);
    chunks
}

// ═══════════════════════════════════════════════════════════════════════════
// Core Splitting Logic
// ═══════════════════════════════════════════════════════════════════════════

/// Emit one or more chunks from a text block, splitting if too large.
fn emit_chunks(
    doc_id: &str,
    text: &str,
    section_path: Option<&str>,
    base_offset: usize,
    out: &mut Vec<DocChunk>,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }

    let est = estimate_tokens(trimmed);
    if est <= MAX_CHUNK_TOKENS {
        out.push(make_chunk(doc_id, trimmed, section_path, base_offset));
        return;
    }

    // Split into paragraphs and group into chunks
    let paragraphs = split_paragraphs(trimmed);
    let mut buf = String::new();
    let mut buf_offset = base_offset;

    for para in &paragraphs {
        let para_est = estimate_tokens(para);

        // If a single paragraph is too large, hard-split by sentence
        if para_est > MAX_CHUNK_TOKENS {
            if !buf.trim().is_empty() {
                out.push(make_chunk(doc_id, buf.trim(), section_path, buf_offset));
                buf.clear();
            }
            split_large_paragraph(doc_id, para, section_path, base_offset, out);
            buf_offset = base_offset + para.len();
            continue;
        }

        let combined_est = estimate_tokens(&buf) + para_est;
        if combined_est > MAX_CHUNK_TOKENS && !buf.trim().is_empty() {
            out.push(make_chunk(doc_id, buf.trim(), section_path, buf_offset));

            // Overlap: keep tail of previous chunk
            let overlap = tail_chars(&buf, OVERLAP_CHARS).to_string();
            buf.clear();
            buf.push_str(&overlap);
        }

        buf.push_str(para);
        buf.push_str("\n\n");
    }

    if !buf.trim().is_empty() {
        out.push(make_chunk(doc_id, buf.trim(), section_path, buf_offset));
    }
}

/// Hard-split a very large paragraph by sentence boundaries.
fn split_large_paragraph(
    doc_id: &str,
    text: &str,
    section_path: Option<&str>,
    base_offset: usize,
    out: &mut Vec<DocChunk>,
) {
    let mut buf = String::new();
    for sentence in split_sentences(text) {
        if estimate_tokens(&buf) + estimate_tokens(sentence) > MAX_CHUNK_TOKENS
            && !buf.trim().is_empty()
        {
            out.push(make_chunk(doc_id, buf.trim(), section_path, base_offset));
            let overlap = tail_chars(&buf, OVERLAP_CHARS).to_string();
            buf.clear();
            buf.push_str(&overlap);
        }
        buf.push_str(sentence);
    }
    if !buf.trim().is_empty() {
        out.push(make_chunk(doc_id, buf.trim(), section_path, base_offset));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn make_chunk(doc_id: &str, content: &str, section_path: Option<&str>, offset: usize) -> DocChunk {
    DocChunk {
        id: Uuid::new_v4().to_string(),
        doc_id: doc_id.to_string(),
        section_path: section_path.map(|s| s.to_string()),
        tokens_estimate: estimate_tokens(content),
        offset,
        length: content.len(),
        content: content.to_string(),
        context_prefix: None,
    }
}

/// CJK-aware token estimate matching frontend `estimateTokens()`.
pub fn estimate_tokens(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    let raw = (cjk as f64 * 1.5 + other as f64 * 0.25) * 1.15;
    raw.ceil() as usize
}

/// Parse a line as a Markdown heading, returning (level, title).
fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level > 6 {
        return None;
    }
    let title = trimmed[level..].trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some((level, title))
}

/// Build a section path like "Deployment > Docker > Troubleshooting".
fn build_section_path(stack: &[(usize, String)]) -> Option<String> {
    if stack.is_empty() {
        return None;
    }
    Some(
        stack
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join(" > "),
    )
}

/// Iterate lines with their byte offsets.
fn line_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut offset = 0;
    for line in text.lines() {
        result.push((offset, line));
        offset += line.len() + 1; // +1 for \n
    }
    result
}

/// Split text into paragraphs (by double newline).
fn split_paragraphs(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if i + 1 < len && bytes[i] == b'\n' && bytes[i + 1] == b'\n' {
            if i > start {
                result.push(&text[start..i]);
            }
            // skip consecutive newlines
            while i < len && bytes[i] == b'\n' {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    if start < len {
        result.push(&text[start..]);
    }
    result
}

/// Split text into sentences (rough heuristic).
fn split_sentences(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for i in 0..chars.len() {
        let (byte_idx, ch) = chars[i];
        if (ch == '.' || ch == '!' || ch == '?' || ch == '。' || ch == '！' || ch == '？')
            && (i + 1 >= chars.len() || chars[i + 1].1.is_whitespace() || is_cjk(chars[i + 1].1))
        {
            let end = byte_idx + ch.len_utf8();
            result.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        result.push(&text[start..]);
    }
    result
}

/// Get the last N characters of a string.
fn tail_chars(s: &str, n: usize) -> &str {
    let char_count = s.chars().count();
    if char_count <= n {
        return s;
    }
    let skip = char_count - n;
    let byte_offset = s.char_indices().nth(skip).map(|(i, _)| i).unwrap_or(0);
    &s[byte_offset..]
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::types::DocFormat;

    #[test]
    fn test_chunk_simple_markdown() {
        let md = "# Introduction\n\nThis is the intro.\n\n## Setup\n\nSetup instructions here.\n\n### Docker\n\nDocker stuff.\n";
        let chunks = chunk_document("doc1", md, &DocFormat::Markdown);
        assert!(!chunks.is_empty());
        // Should have sections
        let paths: Vec<_> = chunks.iter().map(|c| c.section_path.clone()).collect();
        assert!(paths.iter().any(|p| p.as_deref() == Some("Introduction")));
        assert!(
            paths
                .iter()
                .any(|p| p.as_deref() == Some("Introduction > Setup > Docker"))
        );
    }

    #[test]
    fn test_chunk_plaintext() {
        let text = "First paragraph about deployment.\n\nSecond paragraph about monitoring.\n\nThird about cleanup.\n";
        let chunks = chunk_document("doc2", text, &DocFormat::PlainText);
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.section_path.is_none()));
    }

    #[test]
    fn test_estimate_tokens_cjk() {
        // ~40 CJK chars → ~69 tokens (40*1.5*1.15)
        let cjk =
            "运维文档检索增强生成系统的设计与实现需要考虑多个方面包括分块策略索引构建和检索算法";
        let est = estimate_tokens(cjk);
        assert!(est > 50);
        assert!(est < 120);
    }

    #[test]
    fn test_estimate_tokens_english() {
        // ~100 chars → ~29 tokens (100*0.25*1.15)
        let en = "This is a test string with about a hundred characters to verify our token estimation is reasonable.";
        let est = estimate_tokens(en);
        assert!(est > 20);
        assert!(est < 50);
    }

    #[test]
    fn test_parse_heading() {
        assert_eq!(parse_heading("# Hello"), Some((1, "Hello".to_string())));
        assert_eq!(
            parse_heading("### Deep Section"),
            Some((3, "Deep Section".to_string()))
        );
        assert_eq!(parse_heading("Not a heading"), None);
        assert_eq!(parse_heading("#"), None); // empty title
    }

    #[test]
    fn test_empty_document() {
        let chunks = chunk_document("doc3", "", &DocFormat::Markdown);
        assert!(chunks.is_empty());

        let chunks = chunk_document("doc3", "   \n  \n  ", &DocFormat::PlainText);
        assert!(chunks.is_empty());
    }
}
