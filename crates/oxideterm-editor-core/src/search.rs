// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, EditTransaction, TextEdit, TextRange};

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
    let mut search_from = 0;
    while let Some(start) = next_match(source, query, search_from, options.case_sensitive) {
        let end = start + query.len();
        if source.is_char_boundary(start)
            && source.is_char_boundary(end)
            && (!options.whole_word || is_whole_word_match(source, start, end))
        {
            matches.push(FindMatch {
                range: TextRange::new(BufferOffset(start), BufferOffset(end)),
            });
        }
        search_from = next_search_offset(source, start);
        if search_from >= source.len() {
            break;
        }
    }
    matches
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
    fn finds_case_insensitive_matches_on_utf8_boundaries() {
        let hits = find_all(
            "Alpha alpha alp",
            "ALPHA",
            FindOptions {
                case_sensitive: false,
                whole_word: false,
            },
        );

        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0].range,
            TextRange::new(BufferOffset(0), BufferOffset(5))
        );
        assert_eq!(
            hits[1].range,
            TextRange::new(BufferOffset(6), BufferOffset(11))
        );
    }

    #[test]
    fn whole_word_ignores_embedded_matches() {
        let hits = find_all(
            "one stone one_two one",
            "one",
            FindOptions {
                case_sensitive: true,
                whole_word: true,
            },
        );

        assert_eq!(
            hits.into_iter()
                .map(|hit| hit.range.start.0)
                .collect::<Vec<_>>(),
            vec![0, 18]
        );
    }

    #[test]
    fn builds_replace_all_transaction() {
        let transaction = replace_all_transaction(
            "foo bar foo",
            "foo",
            "baz",
            FindOptions {
                case_sensitive: true,
                whole_word: true,
            },
        );

        assert_eq!(transaction.edits().len(), 2);
    }

    #[test]
    fn insensitive_search_keeps_original_byte_ranges() {
        let hits = find_all(
            "İstanbul alpha",
            "ALPHA",
            FindOptions {
                case_sensitive: false,
                whole_word: true,
            },
        );

        assert_eq!(
            hits[0].range,
            TextRange::new(BufferOffset(10), BufferOffset(15))
        );
    }
}
