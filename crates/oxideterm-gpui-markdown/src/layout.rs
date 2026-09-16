// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Block-level layout estimates for virtualized markdown rendering.
//!
//! GPUI's variable-size virtual list needs a stable height before it decides
//! which blocks to render.  These estimates intentionally live beside the
//! markdown model so consumers such as SFTP preview do not need to approximate
//! rendered markdown from the outside.

use std::{cell::RefCell, rc::Rc};

use gpui::{Pixels, Size, px, size};

use crate::model::{Block, FootnoteDefinition, Inline, ListItem, MarkdownDocument};
use crate::options::MarkdownOptions;
use crate::style::BODY_LINE_HEIGHT;

const ESTIMATED_CONTENT_WIDTH: f32 = 920.0;
const BODY_AVERAGE_CHAR_WIDTH: f32 = 0.55;
const CODE_AVERAGE_CHAR_WIDTH: f32 = 0.6;
const CODE_LINE_HEIGHT: f32 = 1.5;
const HEADING_LINE_HEIGHT: f32 = 1.25;
const TABLE_ROW_EXTRA: f32 = 10.0;
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
    measured: Option<Rc<RefCell<Vec<Option<f32>>>>>,
    pub(crate) disclosures: crate::disclosure::DisclosureState,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MarkdownMeasurements(Rc<RefCell<Option<MeasurementState>>>);

#[derive(Debug)]
struct MeasurementState {
    layout: MarkdownBlockLayout,
    width: f32,
    fonts: (String, String),
    geometry_style: Vec<f32>,
}

impl MarkdownMeasurements {
    pub(crate) fn prepare(
        &self,
        mut layout: MarkdownBlockLayout,
        width: f32,
        opts: &MarkdownOptions,
    ) -> MarkdownBlockLayout {
        let mut state = self.0.borrow_mut();
        let fonts = (opts.body_font_family.clone(), opts.code_font_family.clone());
        let geometry_style = vec![
            opts.base_font_size,
            opts.code_font_scale,
            opts.code_label_font_scale,
            opts.footnote_font_scale,
            opts.block_gap,
            opts.list_indent,
            opts.code_block_padding,
            opts.blockquote_border_width,
            opts.max_image_width,
            opts.math_inline_scale,
            opts.math_display_scale,
            opts.math_inline_padding,
            opts.math_display_padding,
            opts.heading_font_scales[0],
            opts.heading_font_scales[1],
            opts.heading_font_scales[2],
            opts.heading_font_scales[3],
            opts.heading_font_scales[4],
            opts.heading_font_scales[5],
        ];
        if let Some(previous) = state.as_ref()
            && previous.layout.items == layout.items
        {
            layout.disclosures = previous.layout.disclosures.clone();
        }
        if let Some(previous) = state.as_ref()
            && previous.width == width
            && previous.fonts == fonts
            && previous.geometry_style == geometry_style
            && previous.layout.items == layout.items
            && previous.layout.item_sizes == layout.item_sizes
        {
            layout.measured = previous.layout.measured.clone();
            layout.disclosures = previous.layout.disclosures.clone();
            return layout;
        }
        let mut measured = vec![None; layout.items.len()];
        if let Some(previous) = state.as_ref() {
            if opts.scroll_sync.is_some() {
                // A synchronized handle belongs to one document across edits.
                layout.disclosures = previous.layout.disclosures.clone();
            }
            if previous.width == width
                && previous.fonts == fonts
                && previous.geometry_style == geometry_style
            {
                let clean = |item: &MarkdownLayoutItem| match item {
                    MarkdownLayoutItem::Block(block) => {
                        MarkdownLayoutItem::Block(block.without_sources())
                    }
                    MarkdownLayoutItem::Footnotes(notes) => MarkdownLayoutItem::Footnotes(
                        notes
                            .iter()
                            .map(|note| FootnoteDefinition {
                                label: note.label.clone(),
                                blocks: note.blocks.iter().map(Block::without_sources).collect(),
                            })
                            .collect(),
                    ),
                };
                let old = previous.layout.items.iter().map(clean).collect::<Vec<_>>();
                let new = layout.items.iter().map(clean).collect::<Vec<_>>();
                if let Some(heights) = &previous.layout.measured {
                    let heights = heights.borrow();
                    for index in 0..old.len().min(new.len()) {
                        if old[index] == new[index]
                            && previous.layout.item_sizes[index] == layout.item_sizes[index]
                        {
                            measured[index] = heights[index];
                        }
                    }
                    for offset in 0..old.len().min(new.len()) {
                        let a = old.len() - 1 - offset;
                        let b = new.len() - 1 - offset;
                        if old[a] != new[b] {
                            break;
                        }
                        if previous.layout.item_sizes[a] == layout.item_sizes[b] {
                            measured[b] = heights[a];
                        }
                    }
                }
            }
        }
        layout.measured = Some(Rc::new(RefCell::new(measured)));
        *state = Some(MeasurementState {
            layout: layout.clone(),
            width,
            fonts,
            geometry_style,
        });
        layout
    }
}

impl MarkdownBlockLayout {
    pub(crate) fn measurement_id(&self) -> usize {
        self.measured
            .as_ref()
            .map_or(0, |value| Rc::as_ptr(value) as usize)
    }
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
            .enumerate()
            .map(|(index, item)| {
                let top_padding = match item {
                    MarkdownLayoutItem::Block(block) => {
                        crate::style::block_top_padding(block, index, opts)
                    }
                    MarkdownLayoutItem::Footnotes(_) => 0.0,
                };
                size(
                    px(ESTIMATED_CONTENT_WIDTH),
                    px(estimate_item_height(item, opts) + top_padding),
                )
            })
            .collect();

        Self {
            items: Rc::new(items),
            item_sizes: Rc::new(item_sizes),
            measured: None,
            disclosures: Default::default(),
        }
    }

    /// Items in the same order expected by [`Self::item_sizes`].
    pub fn items(&self) -> Rc<Vec<MarkdownLayoutItem>> {
        self.items.clone()
    }

    /// Estimated GPUI sizes used by windowed markdown rendering.
    pub fn item_sizes(&self) -> Rc<Vec<Size<Pixels>>> {
        let Some(measured) = &self.measured else {
            return self.item_sizes.clone();
        };
        let measured = measured.borrow();
        Rc::new(
            self.item_sizes
                .iter()
                .enumerate()
                .map(|(index, size)| {
                    gpui::size(
                        size.width,
                        px(measured[index].unwrap_or(f32::from(size.height))),
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn measure(&self, index: usize, child: gpui::AnyElement) -> gpui::AnyElement {
        use gpui::{IntoElement, ParentElement, Styled};
        if self.measured.is_none() {
            return child;
        }
        let layout = self.clone();
        gpui::div()
            .relative()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .child(child)
            .child(
                gpui::canvas(
                    move |bounds, window, _| {
                        let height = f32::from(bounds.size.height);
                        if layout.record_height(index, height) {
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    pub(crate) fn record_height(&self, index: usize, height: f32) -> bool {
        let Some(measured) = &self.measured else {
            return false;
        };
        let mut sizes = measured.borrow_mut();
        if sizes[index].is_some_and(|old| (old - height).abs() <= 0.5) {
            return false;
        }
        sizes[index] = Some(height);
        true
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
        Block::Located { block, .. } => estimate_block_height(block, opts),
        Block::Heading { level, inlines, .. } => {
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
        Block::HtmlContainer { blocks, .. } => estimate_blocks_height(blocks, opts),
        Block::Details { blocks, open, .. } => {
            opts.base_font_size * BODY_LINE_HEIGHT
                + if *open {
                    opts.block_gap + estimate_blocks_height(blocks, opts)
                } else {
                    0.0
                }
        }
        Block::CodeBlock { code, .. } => {
            let code_size = opts.base_font_size * opts.code_font_scale;
            let label_height = code_size * opts.code_label_font_scale * 1.3 + 8.0 + 3.0;
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
        Block::Blockquote { blocks, .. } => {
            estimate_blocks_height(blocks, opts) + opts.block_gap + opts.blockquote_border_width
        }
        Block::Table { headers, rows, .. } => {
            let row_height = |row: &[Vec<Block>]| {
                row.iter()
                    .map(|cell| estimate_blocks_height(cell, opts))
                    .fold(opts.base_font_size * BODY_LINE_HEIGHT, f32::max)
                    + TABLE_ROW_EXTRA
            };
            row_height(headers) + rows.iter().map(|row| row_height(row)).sum::<f32>() + 2.0
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
        Inline::Bold(children)
        | Inline::Italic(children)
        | Inline::Strikethrough(children)
        | Inline::Kbd(children)
        | Inline::Subscript(children)
        | Inline::Superscript(children)
        | Inline::Underline(children)
        | Inline::Highlight(children) => inlines_text_len(children),
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
    use super::*;

    #[test]
    fn unchanged_blocks_keep_measured_heights_after_source_offsets_shift() {
        let opts = MarkdownOptions::default();
        let measurements = MarkdownMeasurements::default();
        let document = crate::parser::parse_with_source_ranges("first\n\nsecond\n\nthird");
        let layout = measurements.prepare(
            MarkdownBlockLayout::from_document(&document, &opts),
            640.0,
            &opts,
        );
        layout.record_height(0, 40.0);
        layout.record_height(1, 80.0);
        layout.record_height(2, 60.0);
        let document = crate::parser::parse_with_source_ranges("new\n\nfirst\n\nsecond\n\nthird");
        let changed = measurements.prepare(
            MarkdownBlockLayout::from_document(&document, &opts),
            640.0,
            &opts,
        );
        assert_eq!(
            changed
                .item_sizes()
                .iter()
                .map(|size| f32::from(size.height))
                .collect::<Vec<_>>(),
            [22.0, 40.0, 80.0, 60.0]
        );
        let document =
            crate::parser::parse_with_source_ranges("new\n\nfirst\n\nreplacement\n\nthird");
        let changed = measurements.prepare(
            MarkdownBlockLayout::from_document(&document, &opts),
            640.0,
            &opts,
        );
        assert_eq!(
            changed
                .item_sizes()
                .iter()
                .map(|size| f32::from(size.height))
                .collect::<Vec<_>>(),
            [22.0, 40.0, 22.0, 60.0]
        );
    }

    #[test]
    fn disclosure_state_survives_relayout_but_not_replacement_documents() {
        let document = crate::parser::parse(
            "<details><summary>A</summary><p>Body</p></details><details open><summary>B</summary><p>Other</p></details>",
        );
        let opts = MarkdownOptions::default();
        let measurements = MarkdownMeasurements::default();
        let layout = measurements.prepare(
            MarkdownBlockLayout::from_document(&document, &opts),
            640.0,
            &opts,
        );
        layout.disclosures.toggle("html-details", false);
        layout.disclosures.toggle("html-details-2", true);
        for width in [640.0, 320.0] {
            let layout = measurements.prepare(
                MarkdownBlockLayout::from_document(&document, &opts),
                width,
                &opts,
            );
            assert!(layout.disclosures.is_open("html-details", false));
            assert!(!layout.disclosures.is_open("html-details-2", true));
        }
        let replacement =
            crate::parser::parse("<details><summary>Different</summary><p>New body</p></details>");
        let layout = measurements.prepare(
            MarkdownBlockLayout::from_document(&replacement, &opts),
            320.0,
            &opts,
        );
        assert!(!layout.disclosures.is_open("html-details", false));
    }
}
