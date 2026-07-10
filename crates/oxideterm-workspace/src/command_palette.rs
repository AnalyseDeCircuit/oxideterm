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
    if query.is_empty() {
        return Some(CommandPaletteMatch {
            score: 1.0,
            highlights: Vec::new(),
        });
    }

    let searchable_value = searchable_value.to_lowercase();
    let normalized_query = query.to_lowercase();
    if searchable_value.contains(&normalized_query) {
        return Some(CommandPaletteMatch {
            score: 1.0,
            highlights: substring_highlights(label, &normalized_query).unwrap_or_default(),
        });
    }

    subsequence_highlights(label, &normalized_query).map(|highlights| CommandPaletteMatch {
        score: 0.5,
        highlights,
    })
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
