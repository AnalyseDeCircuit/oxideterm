// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Block-level layout estimates for virtualized markdown rendering.
//!
//! GPUI's variable-size virtual list needs a stable height before it decides
//! which blocks to render.  These estimates intentionally live beside the
//! markdown model so consumers such as SFTP preview do not need to approximate
//! rendered markdown from the outside.

use std::rc::Rc;

use gpui::{Pixels, Size, px, size};

use crate::model::{Block, FootnoteDefinition, Inline, ListItem, MarkdownDocument};
use crate::options::MarkdownOptions;

const ESTIMATED_CONTENT_WIDTH: f32 = 920.0;
const BODY_AVERAGE_CHAR_WIDTH: f32 = 0.55;
const CODE_AVERAGE_CHAR_WIDTH: f32 = 0.6;
const BODY_LINE_HEIGHT: f32 = 1.45;
const CODE_LINE_HEIGHT: f32 = 1.4;
const HEADING_LINE_HEIGHT: f32 = 1.2;
const TABLE_ROW_EXTRA: f32 = 16.0;
const MIN_BLOCK_HEIGHT: f32 = 18.0;
const LINE_BREAK_ESTIMATED_TEXT_LEN: usize = 128;

/// A block-level markdown item that can be independently virtualized.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkdownLayoutItem {
    Block(Block),
    Footnotes(Vec<FootnoteDefinition>),
}

/// Precomputed item order and estimated sizes for rendered markdown.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownBlockLayout {
    items: Rc<Vec<MarkdownLayoutItem>>,
    item_sizes: Rc<Vec<Size<Pixels>>>,
}

impl MarkdownBlockLayout {
    /// Build virtual-list layout items from a parsed markdown document.
    pub fn from_document(document: &MarkdownDocument, opts: &MarkdownOptions) -> Self {
        let mut items: Vec<MarkdownLayoutItem> = document
            .blocks
            .iter()
            .cloned()
            .map(MarkdownLayoutItem::Block)
            .collect();

        if opts.enable_footnotes && !document.footnotes.is_empty() {
            items.push(MarkdownLayoutItem::Footnotes(document.footnotes.clone()));
        }

        let item_sizes = items
            .iter()
            .map(|item| {
                size(
                    px(ESTIMATED_CONTENT_WIDTH),
                    px(estimate_item_height(item, opts)),
                )
            })
            .collect();

        Self {
            items: Rc::new(items),
            item_sizes: Rc::new(item_sizes),
        }
    }

    /// Items in the same order expected by [`Self::item_sizes`].
    pub fn items(&self) -> Rc<Vec<MarkdownLayoutItem>> {
        self.items.clone()
    }

    /// Estimated GPUI sizes for `gpui_component::v_virtual_list`.
    pub fn item_sizes(&self) -> Rc<Vec<Size<Pixels>>> {
        self.item_sizes.clone()
    }
}

fn estimate_item_height(item: &MarkdownLayoutItem, opts: &MarkdownOptions) -> f32 {
    match item {
        MarkdownLayoutItem::Block(block) => estimate_block_height(block, opts),
        MarkdownLayoutItem::Footnotes(footnotes) => {
            let content: f32 = footnotes
                .iter()
                .map(|footnote| {
                    estimate_blocks_height(&footnote.blocks, opts) * opts.footnote_font_scale
                })
                .sum();
            content + opts.block_gap * 2.0
        }
    }
    .max(MIN_BLOCK_HEIGHT)
}

fn estimate_blocks_height(blocks: &[Block], opts: &MarkdownOptions) -> f32 {
    let content: f32 = blocks
        .iter()
        .map(|block| estimate_block_height(block, opts))
        .sum();
    content + opts.block_gap * blocks.len().saturating_sub(1) as f32
}

fn estimate_block_height(block: &Block, opts: &MarkdownOptions) -> f32 {
    match block {
        Block::Heading { level, inlines } => {
            let font_size = opts.base_font_size
                * opts
                    .heading_font_scales
                    .get(level.saturating_sub(1) as usize)
                    .copied()
                    .unwrap_or(1.0);
            estimate_wrapped_lines(inlines_text_len(inlines), chars_per_line(font_size, false))
                * font_size
                * HEADING_LINE_HEIGHT
        }
        Block::Paragraph { inlines } => {
            estimate_wrapped_lines(
                inlines_text_len(inlines),
                chars_per_line(opts.base_font_size, false),
            ) * opts.base_font_size
                * BODY_LINE_HEIGHT
        }
        Block::Html(html) => {
            estimate_wrapped_lines(
                html.chars().count(),
                chars_per_line(opts.base_font_size, false),
            ) * opts.base_font_size
                * BODY_LINE_HEIGHT
        }
        Block::CodeBlock { language, code } => {
            let code_size = opts.base_font_size * opts.code_font_scale;
            let label_height = language
                .as_ref()
                .map(|_| code_size * opts.code_label_font_scale * BODY_LINE_HEIGHT)
                .unwrap_or(0.0);
            let code_lines: f32 = code
                .lines()
                .map(|line| {
                    estimate_wrapped_lines(line.chars().count(), chars_per_line(code_size, true))
                })
                .sum::<f32>()
                .max(1.0);
            label_height + code_lines * code_size * CODE_LINE_HEIGHT + opts.code_block_padding * 2.0
        }
        Block::UnorderedList { items } | Block::OrderedList { items, .. } => {
            estimate_list_height(items, opts)
        }
        Block::HorizontalRule => opts.block_gap * 2.0 + 1.0,
        Block::Blockquote { blocks } => {
            estimate_blocks_height(blocks, opts) + opts.block_gap + opts.blockquote_border_width
        }
        Block::Table { headers, rows, .. } => {
            let row_font_height = opts.base_font_size * BODY_LINE_HEIGHT + TABLE_ROW_EXTRA;
            let header_lines = headers
                .iter()
                .map(|cell| {
                    estimate_wrapped_lines(
                        inlines_text_len(cell),
                        chars_per_line(opts.base_font_size, false) / headers.len().max(1) as f32,
                    )
                })
                .fold(1.0, f32::max);
            let body_lines: f32 = rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| {
                            estimate_wrapped_lines(
                                inlines_text_len(cell),
                                chars_per_line(opts.base_font_size, false)
                                    / headers.len().max(1) as f32,
                            )
                        })
                        .fold(1.0, f32::max)
                })
                .sum();
            (header_lines + body_lines) * row_font_height + 2.0
        }
    }
    .max(MIN_BLOCK_HEIGHT)
}

fn estimate_list_height(items: &[ListItem], opts: &MarkdownOptions) -> f32 {
    let item_gap = 4.0;
    let rows: f32 = items
        .iter()
        .map(|item| {
            let own = estimate_wrapped_lines(
                inlines_text_len(&item.inlines),
                chars_per_line(opts.base_font_size, false),
            ) * opts.base_font_size
                * BODY_LINE_HEIGHT;
            let nested = if item.children.is_empty() {
                0.0
            } else {
                opts.block_gap + estimate_blocks_height(&item.children, opts)
            };
            own + nested
        })
        .sum();
    rows + item_gap * items.len().saturating_sub(1) as f32
}

fn chars_per_line(font_size: f32, code: bool) -> f32 {
    let average = if code {
        CODE_AVERAGE_CHAR_WIDTH
    } else {
        BODY_AVERAGE_CHAR_WIDTH
    };
    (ESTIMATED_CONTENT_WIDTH / (font_size * average)).max(24.0)
}

fn estimate_wrapped_lines(chars: usize, chars_per_line: f32) -> f32 {
    ((chars.max(1) as f32) / chars_per_line.max(1.0)).ceil()
}

fn inlines_text_len(inlines: &[Inline]) -> usize {
    inlines.iter().map(inline_text_len).sum()
}

fn inline_text_len(inline: &Inline) -> usize {
    match inline {
        Inline::Text(text) | Inline::Code(text) | Inline::Html(text) => text.chars().count(),
        Inline::Bold(children) | Inline::Italic(children) | Inline::Strikethrough(children) => {
            inlines_text_len(children)
        }
        Inline::Link { text, url } => inlines_text_len(text).max(url.chars().count()),
        Inline::Image { alt, .. } => alt.chars().count().max(24),
        Inline::Math { latex, display } => {
            if *display {
                latex.chars().count().max(80)
            } else {
                latex.chars().count().max(8)
            }
        }
        Inline::FootnoteReference { label, .. } => label.chars().count() + 2,
        Inline::LineBreak => LINE_BREAK_ESTIMATED_TEXT_LEN,
    }
}

#[cfg(test)]
mod tests {
    use crate::parser;

    use super::*;

    #[test]
    fn layout_has_one_item_per_block_plus_footnotes() {
        let document = parser::parse("# Title\n\nBody\n\n[^a]: footnote");
        let opts = MarkdownOptions::default();

        let layout = MarkdownBlockLayout::from_document(&document, &opts);

        assert_eq!(layout.items().len(), document.blocks.len() + 1);
        assert_eq!(layout.item_sizes().len(), layout.items().len());
    }

    #[test]
    fn code_blocks_scale_with_line_count() {
        let one_line = parser::parse("```rs\nlet x = 1;\n```");
        let many_lines = parser::parse("```rs\none\ntwo\nthree\nfour\n```");
        let opts = MarkdownOptions::default();

        let one = MarkdownBlockLayout::from_document(&one_line, &opts).item_sizes()[0].height;
        let many = MarkdownBlockLayout::from_document(&many_lines, &opts).item_sizes()[0].height;

        assert!(many > one);
    }
}
