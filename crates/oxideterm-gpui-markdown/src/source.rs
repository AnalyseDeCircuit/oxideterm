// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Lossless source ranges for Markdown editing surfaces.
//!
//! The renderer model intentionally normalizes Markdown semantics and therefore cannot be used to
//! reconstruct the user's source. This projection keeps byte ranges into the exact input while the
//! existing parser and renderer continue to own presentation.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownSourceBlockKind {
    Heading,
    Paragraph,
    CodeBlock,
    List,
    BlockQuote,
    Table,
    FootnoteDefinition,
    Metadata,
    Html,
    ThematicBreak,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownSourceBlock {
    pub range: Range<usize>,
    pub kind: MarkdownSourceBlockKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownSourceDocument {
    pub blocks: Vec<MarkdownSourceBlock>,
    /// Source outside visible blocks, including blank separators and trailing newlines.
    pub preserved_ranges: Vec<Range<usize>>,
}

/// Projects top-level Markdown blocks onto byte ranges in the original UTF-8 source.
pub fn parse_source_blocks(source: &str) -> MarkdownSourceDocument {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_GFM
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;
    let mut blocks = Vec::new();
    let mut block_depth = 0usize;
    let mut active: Option<(usize, usize, MarkdownSourceBlockKind)> = None;

    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(tag) if is_block_tag(&tag) => {
                if block_depth == 0 {
                    active = Some((range.start, range.end, source_block_kind(&tag)));
                } else if let Some((_, end, _)) = active.as_mut() {
                    *end = (*end).max(range.end);
                }
                block_depth += 1;
            }
            Event::End(tag) if block_depth > 0 && is_block_tag_end(tag) => {
                if let Some((_, end, _)) = active.as_mut() {
                    *end = (*end).max(range.end);
                }
                block_depth -= 1;
                if block_depth == 0
                    && let Some((start, end, kind)) = active.take()
                {
                    push_source_block(&mut blocks, start..end, kind);
                }
            }
            Event::Rule if block_depth == 0 => {
                push_source_block(&mut blocks, range, MarkdownSourceBlockKind::ThematicBreak);
            }
            Event::Html(_) if block_depth == 0 => {
                push_source_block(&mut blocks, range, MarkdownSourceBlockKind::Html);
            }
            _ => {
                if let Some((_, end, _)) = active.as_mut() {
                    *end = (*end).max(range.end);
                } else if block_depth == 0 && !range.is_empty() {
                    // Malformed or future syntax must remain addressable even when pulldown-cmark
                    // does not wrap it in a currently known block tag.
                    push_source_block(&mut blocks, range, MarkdownSourceBlockKind::Other);
                }
            }
        }
    }

    if let Some((start, end, kind)) = active {
        push_source_block(&mut blocks, start..end, kind);
    }
    blocks.sort_by_key(|block| block.range.start);
    let preserved_ranges = source_complement_ranges(source.len(), &blocks);
    MarkdownSourceDocument {
        blocks,
        preserved_ranges,
    }
}

fn is_block_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::Table(_)
            | Tag::MetadataBlock(_)
    )
}

fn source_block_kind(tag: &Tag<'_>) -> MarkdownSourceBlockKind {
    match tag {
        Tag::Heading { .. } => MarkdownSourceBlockKind::Heading,
        Tag::Paragraph => MarkdownSourceBlockKind::Paragraph,
        Tag::CodeBlock(_) => MarkdownSourceBlockKind::CodeBlock,
        Tag::HtmlBlock => MarkdownSourceBlockKind::Html,
        Tag::List(_) => MarkdownSourceBlockKind::List,
        Tag::BlockQuote(_) => MarkdownSourceBlockKind::BlockQuote,
        Tag::Table(_) => MarkdownSourceBlockKind::Table,
        Tag::FootnoteDefinition(_) => MarkdownSourceBlockKind::FootnoteDefinition,
        Tag::MetadataBlock(_) => MarkdownSourceBlockKind::Metadata,
        _ => MarkdownSourceBlockKind::Other,
    }
}

fn is_block_tag_end(tag: TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::Table
            | TagEnd::MetadataBlock(_)
    )
}

fn push_source_block(
    blocks: &mut Vec<MarkdownSourceBlock>,
    range: Range<usize>,
    kind: MarkdownSourceBlockKind,
) {
    if range.is_empty() {
        return;
    }
    if let Some(previous) = blocks.last_mut()
        && range.start < previous.range.end
    {
        previous.range.end = previous.range.end.max(range.end);
        return;
    }
    blocks.push(MarkdownSourceBlock { range, kind });
}

fn source_complement_ranges(
    source_len: usize,
    blocks: &[MarkdownSourceBlock],
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    for block in blocks {
        if cursor < block.range.start {
            ranges.push(cursor..block.range.start);
        }
        cursor = cursor.max(block.range.end);
    }
    if cursor < source_len {
        ranges.push(cursor..source_len);
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_slices(source: &str) -> Vec<&str> {
        parse_source_blocks(source)
            .blocks
            .iter()
            .map(|block| &source[block.range.clone()])
            .collect()
    }

    fn assert_partition_is_lossless(source: &str) {
        let document = parse_source_blocks(source);
        let mut ranges = document
            .blocks
            .iter()
            .map(|block| block.range.clone())
            .chain(document.preserved_ranges.iter().cloned())
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| range.start);
        assert_eq!(ranges.first().map(|range| range.start).unwrap_or(0), 0);
        assert_eq!(
            ranges.last().map(|range| range.end).unwrap_or(0),
            source.len()
        );
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        let rebuilt = ranges
            .iter()
            .map(|range| &source[range.clone()])
            .collect::<String>();
        assert_eq!(rebuilt, source);
    }

    #[test]
    fn projects_adjacent_top_level_blocks_without_normalizing_source() {
        let source = "# Title\n\nParagraph with **bold**.\n\n```rust\nfn main() {}\n```\n";
        let document = parse_source_blocks(source);

        assert_eq!(
            document
                .blocks
                .iter()
                .map(|block| block.kind)
                .collect::<Vec<_>>(),
            vec![
                MarkdownSourceBlockKind::Heading,
                MarkdownSourceBlockKind::Paragraph,
                MarkdownSourceBlockKind::CodeBlock,
            ]
        );
        assert_eq!(
            block_slices(source),
            vec![
                "# Title\n",
                "Paragraph with **bold**.\n",
                "```rust\nfn main() {}\n```"
            ]
        );
        assert_partition_is_lossless(source);
    }

    #[test]
    fn nested_list_is_one_top_level_editing_block() {
        let source = "- parent\n  - child\n- sibling\n\nAfter\n";
        let document = parse_source_blocks(source);

        assert_eq!(document.blocks.len(), 2);
        assert_eq!(document.blocks[0].kind, MarkdownSourceBlockKind::List);
        assert_eq!(
            &source[document.blocks[0].range.clone()],
            "- parent\n  - child\n- sibling\n\n"
        );
        assert_partition_is_lossless(source);
    }

    #[test]
    fn metadata_footnotes_tables_and_unicode_keep_byte_ranges() {
        let source = "---\ntitle: 知识库 🚀\n---\n\n| 列 | 值 |\n|---|---|\n| 一 | café |\n\n正文[^a]\n\n[^a]: 注释\n";
        let document = parse_source_blocks(source);

        assert!(
            document
                .blocks
                .iter()
                .any(|block| block.kind == MarkdownSourceBlockKind::Metadata)
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| block.kind == MarkdownSourceBlockKind::Table)
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| block.kind == MarkdownSourceBlockKind::FootnoteDefinition)
        );
        for block in &document.blocks {
            assert!(source.is_char_boundary(block.range.start));
            assert!(source.is_char_boundary(block.range.end));
        }
        assert_partition_is_lossless(source);
    }

    #[test]
    fn incomplete_markdown_and_crlf_remain_lossless() {
        for source in [
            "```rust\r\nunfinished",
            "[unfinished link](",
            "- item\r\n\r\n",
        ] {
            assert_partition_is_lossless(source);
        }
    }

    #[test]
    fn complex_atomic_blocks_keep_their_original_source() {
        let source = "> quoted **text**\n\n$$\na^2 + b^2 = c^2\n$$\n\n```mermaid\ngraph TD; A-->B\n```\n\n<section data-note=\"raw\">value</section>\n";
        let document = parse_source_blocks(source);

        assert!(
            document
                .blocks
                .iter()
                .any(|block| block.kind == MarkdownSourceBlockKind::BlockQuote)
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| block.kind == MarkdownSourceBlockKind::CodeBlock)
        );
        assert!(
            document
                .blocks
                .iter()
                .any(|block| block.kind == MarkdownSourceBlockKind::Html)
        );
        assert_partition_is_lossless(source);
    }

    #[test]
    fn blank_document_is_one_preserved_range() {
        let document = parse_source_blocks("\n\n");

        assert!(document.blocks.is_empty());
        assert_eq!(document.preserved_ranges, vec![0..2]);
    }
}
