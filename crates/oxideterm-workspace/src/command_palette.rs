// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Query parsing and text matching for workspace command palettes.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandPaletteMode {
    All,
    Commands,
    Sessions,
    Connections,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandPaletteMatch {
    pub score: f32,
    pub highlights: Vec<usize>,
}

pub fn parse_command_palette_query(raw_query: &str) -> (CommandPaletteMode, String) {
    let trimmed = raw_query.trim_start();
    if let Some(rest) = trimmed.strip_prefix('>') {
        (CommandPaletteMode::Commands, rest.trim_start().to_string())
    } else if let Some(rest) = trimmed.strip_prefix('@') {
        (CommandPaletteMode::Sessions, rest.trim_start().to_string())
    } else if let Some(rest) = trimmed.strip_prefix('#') {
        (
            CommandPaletteMode::Connections,
            rest.trim_start().to_string(),
        )
    } else {
        (CommandPaletteMode::All, trimmed.to_string())
    }
}

pub fn command_palette_match(
    label: &str,
    searchable_value: &str,
    query: &str,
) -> Option<CommandPaletteMatch> {
    if query.trim().is_empty() {
        return Some(CommandPaletteMatch {
            score: 1.0,
            highlights: Vec::new(),
        });
    }

    let searchable_value = searchable_value.to_lowercase();
    let normalized_query = query.to_lowercase();
    let mut result = CommandPaletteMatch {
        score: 1.0,
        highlights: Vec::new(),
    };
    // Each term must match, but separators and term order need not match the label.
    for term in normalized_query.split_whitespace() {
        if searchable_value.contains(term) {
            result
                .highlights
                .extend(substring_highlights(label, term).unwrap_or_default());
        } else {
            result
                .highlights
                .extend(subsequence_highlights(label, term)?);
            result.score = 0.5;
        }
    }
    result.highlights.sort_unstable();
    result.highlights.dedup();
    Some(result)
}

fn substring_highlights(label: &str, normalized_query: &str) -> Option<Vec<usize>> {
    let (normalized_label, original_indices) = lowercase_with_original_indices(label);
    let start_byte = normalized_label.find(normalized_query)?;
    let start = normalized_label[..start_byte].chars().count();
    let len = normalized_query.chars().count();
    Some(unique_original_indices(
        &original_indices[start..start + len],
    ))
}

fn subsequence_highlights(label: &str, normalized_query: &str) -> Option<Vec<usize>> {
    let (normalized_label, original_indices) = lowercase_with_original_indices(label);
    let mut highlights = Vec::new();
    let mut query_chars = normalized_query.chars();
    let mut current = query_chars.next()?;
    for (normalized_index, character) in normalized_label.chars().enumerate() {
        if character == current {
            let original_index = original_indices[normalized_index];
            if highlights.last().copied() != Some(original_index) {
                highlights.push(original_index);
            }
            if let Some(next) = query_chars.next() {
                current = next;
            } else {
                return Some(highlights);
            }
        }
    }
    None
}

fn lowercase_with_original_indices(input: &str) -> (String, Vec<usize>) {
    let mut normalized = String::new();
    let mut original_indices = Vec::new();
    for (original_index, character) in input.chars().enumerate() {
        for lowercase_character in character.to_lowercase() {
            normalized.push(lowercase_character);
            original_indices.push(original_index);
        }
    }
    (normalized, original_indices)
}

fn unique_original_indices(indices: &[usize]) -> Vec<usize> {
    let mut unique = Vec::new();
    for index in indices {
        if unique.last() != Some(index) {
            unique.push(*index);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_prefix_selects_mode_and_trims_only_leading_space() {
        assert_eq!(
            parse_command_palette_query("  >  close tab"),
            (CommandPaletteMode::Commands, "close tab".to_string())
        );
        assert_eq!(
            parse_command_palette_query("@server "),
            (CommandPaletteMode::Sessions, "server ".to_string())
        );
        assert_eq!(
            parse_command_palette_query("plain"),
            (CommandPaletteMode::All, "plain".to_string())
        );
    }

    #[test]
    fn matching_prefers_substring_then_falls_back_to_label_subsequence() {
        assert_eq!(
            command_palette_match("Open Settings", "open settings preferences", "settings"),
            Some(CommandPaletteMatch {
                score: 1.0,
                highlights: (5..13).collect(),
            })
        );
        assert_eq!(
            command_palette_match("Open Settings", "open settings preferences", "osg"),
            Some(CommandPaletteMatch {
                score: 0.5,
                highlights: vec![0, 5, 11],
            })
        );
        assert_eq!(
            command_palette_match("Open Settings", "open settings preferences", "xyz"),
            None
        );
    }

    #[test]
    fn multiple_keywords_require_every_term_regardless_of_order_or_separator() {
        let names = [
            "xxxxx-xxxxxxxxxxx-user_prod",
            "xxxxx-xxxxxxxxxxx-user_test",
            "xxxxx-xxxxxxxxxxx-order_prod",
            "xxxxx-xxxxxxxxxxx-order_test",
            "yyyyy-yyyyyyyyyyyyyyyyy-aaaa-bbb",
            "service-user-extra-prod",
        ];
        for query in [
            "user prod",
            "prod user",
            " USER \t prod  ",
            "user\u{3000}prod",
        ] {
            let matches: Vec<_> = names
                .iter()
                .copied()
                .filter(|name| command_palette_match(name, name, query).is_some())
                .collect();
            assert_eq!(matches, vec![names[0], names[5]], "query={query:?}");
        }
        assert_eq!(
            command_palette_match(names[0], names[0], "user missing"),
            None
        );
    }

    #[test]
    fn keyword_highlights_merge_label_matches_and_allow_other_search_fields() {
        assert_eq!(
            command_palette_match(
                "user_prod",
                "user_prod operator@db.example ssh",
                "prod ssh user user"
            ),
            Some(CommandPaletteMatch {
                score: 1.0,
                highlights: vec![0, 1, 2, 3, 5, 6, 7, 8]
            }),
        );
        assert_eq!(
            command_palette_match("user_prod", "user_prod ssh", "usr prod"),
            Some(CommandPaletteMatch {
                score: 0.5,
                highlights: vec![0, 1, 3, 5, 6, 7, 8]
            }),
        );
        assert_eq!(
            command_palette_match("İnfo 设置", "İnfo 设置", "设置 i"),
            Some(CommandPaletteMatch {
                score: 1.0,
                highlights: vec![0, 5, 6]
            }),
        );
        assert_eq!(
            command_palette_match("user_prod", "user_prod", " \t "),
            Some(CommandPaletteMatch {
                score: 1.0,
                highlights: vec![]
            }),
        );
    }

    #[test]
    fn highlights_map_unicode_lowercase_expansion_back_to_original_label() {
        assert_eq!(
            command_palette_match("İnfo 设置", "İnfo 设置", "i"),
            Some(CommandPaletteMatch {
                score: 1.0,
                highlights: vec![0],
            })
        );
        assert_eq!(
            command_palette_match("打开设置", "打开设置", "设置"),
            Some(CommandPaletteMatch {
                score: 1.0,
                highlights: vec![2, 3],
            })
        );
    }
}
