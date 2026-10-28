// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, EditTransaction, TextBuffer, TextEdit, TextRange};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FindOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FindMatch {
    pub range: TextRange,
}

pub fn find_all(source: &str, query: &str, options: FindOptions) -> Vec<FindMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    append_window_matches(source, query, options, 0, 0, source.len(), &mut matches);
    matches
}

impl TextBuffer {
    /// Search pieces with bounded overlap, including matches crossing edits or newlines.
    pub fn find_all(&self, query: &str, options: FindOptions) -> Vec<FindMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        const CHUNK_BYTES: usize = 64 * 1024;
        let mut window = String::new();
        let mut base = 0;
        let mut scanned = 0;
        let mut matches = Vec::new();
        for piece in self.chunks() {
            let mut remaining = piece;
            while !remaining.is_empty() {
                let mut end = remaining.len().min(CHUNK_BYTES);
                while !remaining.is_char_boundary(end) {
                    end -= 1;
                }
                window.push_str(&remaining[..end]);
                remaining = &remaining[end..];
                // Leave the longest candidate plus one Unicode neighbour for
                // whole-word checks; retain another neighbour before the cursor.
                let mut limit = window.len().saturating_sub(query.len().saturating_add(4));
                while !window.is_char_boundary(limit) {
                    limit -= 1;
                }
                if base + limit <= scanned {
                    continue;
                }
                append_window_matches(
                    &window,
                    query,
                    options,
                    base,
                    scanned,
                    base + limit,
                    &mut matches,
                );
                scanned = base + limit;
                let mut discard = limit.saturating_sub(4);
                while !window.is_char_boundary(discard) {
                    discard -= 1;
                }
                window.drain(..discard);
                base += discard;
            }
        }
        append_window_matches(
            &window,
            query,
            options,
            base,
            scanned,
            self.len(),
            &mut matches,
        );
        matches
    }
}

fn append_window_matches(
    source: &str,
    query: &str,
    options: FindOptions,
    base: usize,
    start: usize,
    end: usize,
    matches: &mut Vec<FindMatch>,
) {
    // Keep leading context in source rather than slicing at start: whole-word
    // matching must see the character immediately before the candidate.
    let mut search_from = start - base;
    while let Some(relative) = next_match(source, query, search_from, options.case_sensitive) {
        if base + relative >= end {
            break;
        }
        let match_end = relative + query.len();
        if source.is_char_boundary(relative)
            && source.is_char_boundary(match_end)
            && (!options.whole_word || is_whole_word_match(source, relative, match_end))
        {
            matches.push(FindMatch {
                range: TextRange::new(
                    BufferOffset(base + relative),
                    BufferOffset(base + match_end),
                ),
            });
        }
        search_from = next_search_offset(source, relative);
        if search_from >= source.len() {
            break;
        }
    }
}

pub fn replace_all_transaction(
    source: &str,
    query: &str,
    replacement: &str,
    options: FindOptions,
) -> EditTransaction {
    let edits = find_all(source, query, options)
        .into_iter()
        .map(|hit| TextEdit::new(hit.range, replacement))
        .collect();
    EditTransaction::new(edits)
}

fn next_search_offset(source: &str, start: usize) -> usize {
    source[start..]
        .chars()
        .next()
        .map(|ch| start + ch.len_utf8())
        .unwrap_or(source.len())
}

fn next_match(
    source: &str,
    query: &str,
    search_from: usize,
    case_sensitive: bool,
) -> Option<usize> {
    if case_sensitive {
        return source[search_from..]
            .find(query)
            .map(|relative| search_from + relative);
    }

    // Keep all ranges in the original buffer's byte coordinates. Full Unicode
    // case folding can change byte length, so Phase 4 intentionally limits
    // insensitive search to same-length slices instead of searching a lowered
    // copy and mapping ranges back after the fact.
    source[search_from..]
        .char_indices()
        .map(|(relative, _)| search_from + relative)
        .find(|start| {
            let end = start + query.len();
            end <= source.len()
                && source.is_char_boundary(end)
                && source[*start..end].eq_ignore_ascii_case(query)
        })
}

fn is_whole_word_match(source: &str, start: usize, end: usize) -> bool {
    let before = source[..start].chars().next_back();
    let after = source[end..].chars().next();
    !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
}

fn is_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_returns_exact_original_ranges_for_case_and_word_options() {
        for (source, query, case_sensitive, whole_word, ranges) in [
            ("Alpha alpha alp", "ALPHA", false, false, vec![0..5, 6..11]),
            (
                "one stone one_two one",
                "one",
                true,
                true,
                vec![0..3, 18..21],
            ),
            ("İstanbul alpha", "ALPHA", false, true, vec![10..15]),
            ("aaaaa", "aaa", true, false, vec![0..3, 1..4, 2..5]),
            ("你\n好 你\n好", "你\n好", true, false, vec![0..7, 8..15]),
        ] {
            let options = FindOptions {
                case_sensitive,
                whole_word,
            };
            // Force a piece boundary inside the first word without changing the
            // final source, so the buffer path cannot rely on contiguous storage.
            let mut buffer = TextBuffer::new(source);
            let first = source.chars().next().unwrap().len_utf8();
            buffer
                .replace_selection(
                    crate::Selection::new(BufferOffset(0), BufferOffset(first)),
                    &source[..first],
                )
                .unwrap();
            for hits in [
                find_all(source, query, options),
                buffer.find_all(query, options),
            ] {
                assert_eq!(
                    hits.into_iter()
                        .map(|hit| hit.range.as_range())
                        .collect::<Vec<_>>(),
                    ranges,
                    "{source}"
                );
            }
        }
    }

    #[test]
    fn streaming_search_keeps_chunk_boundary_context_and_long_queries() {
        let padding = 64 * 1024 - 2;
        let source = format!("{}你abc\nDEF🙂 abc\nDEF ", " ".repeat(padding));
        let buffer = TextBuffer::new(source);
        let options = FindOptions {
            case_sensitive: false,
            whole_word: true,
        };
        // The first candidate is excluded by its Unicode left neighbour.
        assert_eq!(
            buffer
                .find_all("ABC\nDEF", options)
                .into_iter()
                .map(|hit| hit.range.as_range())
                .collect::<Vec<_>>(),
            [padding + 15..padding + 22]
        );
        let query = format!("{}🙂", "q".repeat(70 * 1024));
        let mut buffer = TextBuffer::new(format!(".{query}."));
        buffer
            .replace_selection(crate::Selection::caret(BufferOffset(20)), "z")
            .unwrap();
        buffer.undo().unwrap();
        assert_eq!(
            buffer
                .find_all(&query, FindOptions::default())
                .into_iter()
                .map(|hit| hit.range.as_range())
                .collect::<Vec<_>>(),
            [1..1 + query.len()]
        );
    }

    #[test]
    fn replace_all_changes_only_matches_and_undo_restores_the_source() {
        let source = "你好 foo bar food foo";
        let mut buffer = crate::TextBuffer::new(source);
        let transaction = replace_all_transaction(
            source,
            "foo",
            "🚀🚀",
            FindOptions {
                case_sensitive: true,
                whole_word: true,
            },
        );
        buffer.apply_transaction(transaction).unwrap();
        assert_eq!(buffer.text(), "你好 🚀🚀 bar food 🚀🚀");
        buffer.undo().unwrap();
        assert_eq!(buffer.text(), source);
    }
}
