// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared document layout and measured heights for windowed Markdown rendering.

use std::{
    cell::{Ref, RefCell},
    ops::Range,
    rc::Rc,
};

use gpui::{Pixels, Size, px, size};

use crate::model::{
    Block, FootnoteDefinition, Inline, ListItem, MarkdownDocument, blocks_same_content,
};
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

/// A borrowed block-level item. Layout never owns another copy of the parsed tree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkdownLayoutItem<'a> {
    Block(&'a Block),
    Footnotes(&'a [FootnoteDefinition]),
}

impl MarkdownLayoutItem<'_> {
    fn same_content(self, other: Self) -> bool {
        match (self, other) {
            (Self::Block(a), Self::Block(b)) => a.same_content(b),
            (Self::Footnotes(a), Self::Footnotes(b)) => {
                a.len() == b.len()
                    && a.iter().zip(b).all(|(a, b)| {
                        a.label == b.label && blocks_same_content(&a.blocks, &b.blocks)
                    })
            }
            _ => false,
        }
    }
}

/// Fenwick sums include each item's following gap; the final gap is not content.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockHeights {
    sums: Vec<f64>,
    gap: f64,
}

impl BlockHeights {
    pub(crate) fn new(sizes: &[Size<Pixels>], gap: f32) -> Self {
        let gap = finite_height(gap) as f64;
        let mut sums = vec![0.0; sizes.len() + 1];
        for (index, size) in sizes.iter().enumerate() {
            let slot = index + 1;
            sums[slot] += finite_height(f32::from(size.height)) as f64 + gap;
            let parent = slot + (slot & slot.wrapping_neg());
            if parent < sums.len() {
                sums[parent] += sums[slot];
            }
        }
        Self { sums, gap }
    }

    fn update(&mut self, index: usize, delta: f32) {
        let mut slot = index + 1;
        while slot < self.sums.len() {
            self.sums[slot] += delta as f64;
            slot += slot & slot.wrapping_neg();
        }
    }

    fn prefix(&self, mut count: usize) -> f64 {
        let mut total = 0.0;
        while count > 0 {
            total += self.sums[count];
            count &= count - 1;
        }
        total
    }

    pub(crate) fn len(&self) -> usize {
        self.sums.len() - 1
    }
    pub(crate) fn top(&self, index: usize) -> f32 {
        self.prefix(index) as f32
    }
    pub(crate) fn bottom(&self, index: usize) -> f32 {
        (self.prefix(index + 1) - self.gap) as f32
    }
    pub(crate) fn total(&self) -> f32 {
        if self.len() == 0 {
            0.0
        } else {
            (self.prefix(self.len()) - self.gap).max(0.0) as f32
        }
    }

    // Return the count of complete weighted items before the requested prefix.
    fn search(&self, target: f64, inclusive: bool) -> usize {
        let mut index = 0;
        let mut sum = 0.0;
        let mut step = self.sums.len().next_power_of_two() >> 1;
        while step > 0 {
            let next = index + step;
            if next < self.sums.len() {
                let value = sum + self.sums[next];
                if value < target || (inclusive && value == target) {
                    index = next;
                    sum = value;
                }
            }
            step >>= 1;
        }
        index
    }

    pub(crate) fn visible_range(&self, top: f32, bottom: f32) -> Range<usize> {
        let first = self.search(top as f64 + self.gap, false).min(self.len());
        let end = (self.search(bottom as f64, true) + 1).min(self.len());
        first..end.max(first)
    }
}

fn finite_height(height: f32) -> f32 {
    if height.is_finite() {
        height.max(0.0)
    } else {
        0.0
    }
}

#[derive(Clone, Debug, PartialEq)]
struct MeasuredGeometry {
    heights: Vec<Option<f32>>,
    index: BlockHeights,
    sizes: Option<Rc<Vec<Size<Pixels>>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownBlockLayout {
    document: Option<MarkdownDocument>,
    footnotes: bool,
    item_sizes: Rc<Vec<Size<Pixels>>>,
    geometry: Rc<RefCell<MeasuredGeometry>>,
    measured: bool,
    pub(crate) disclosures: crate::disclosure::DisclosureState,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MarkdownMeasurements(Rc<RefCell<Option<MeasurementState>>>);

#[derive(Debug)]
struct MeasurementState {
    layout: MarkdownBlockLayout,
    document_released: bool,
    width: f32,
    fonts: (String, String),
    geometry_style: Vec<f32>,
}

impl MarkdownMeasurements {
    pub(crate) fn release_document(&self) {
        if let Some(state) = self.0.borrow_mut().as_mut() {
            state.layout.document = None;
            state.document_released = true;
        }
    }

    pub(crate) fn prepare(
        &self,
        document: &MarkdownDocument,
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
        let same_document = state.as_ref().is_some_and(|previous| {
            previous.document_released
                || previous
                    .layout
                    .document
                    .as_ref()
                    .is_some_and(|old| old.same_identity(document) || old == document)
        });
        if let Some(previous) = state.as_mut()
            && same_document
            && previous.layout.footnotes == opts.enable_footnotes
            && previous.width == width
            && previous.fonts == fonts
            && previous.geometry_style == geometry_style
        {
            if previous.document_released {
                previous.layout.document = Some(document.clone());
                previous.document_released = false;
            }
            return previous.layout.clone();
        }
        let mut layout = MarkdownBlockLayout::from_document(document, opts);
        layout.measured = true;
        layout.geometry.borrow_mut().heights = vec![None; layout.item_sizes.len()];
        if let Some(previous) = state.as_ref() {
            if same_document || opts.scroll_sync.is_some() {
                layout.disclosures = previous.layout.disclosures.clone();
            }
            if previous.width == width
                && previous.fonts == fonts
                && previous.geometry_style == geometry_style
                && !previous.document_released
            {
                let old_heights = previous.layout.geometry.borrow();
                for index in 0..previous.layout.len().min(layout.len()) {
                    if previous
                        .layout
                        .item(index)
                        .unwrap()
                        .same_content(layout.item(index).unwrap())
                        && previous.layout.item_sizes[index] == layout.item_sizes[index]
                        && let Some(height) = old_heights.heights[index]
                    {
                        layout.record_height(index, height);
                    }
                }
                for offset in 0..previous.layout.len().min(layout.len()) {
                    let a = previous.layout.len() - 1 - offset;
                    let b = layout.len() - 1 - offset;
                    if !previous
                        .layout
                        .item(a)
                        .unwrap()
                        .same_content(layout.item(b).unwrap())
                    {
                        break;
                    }
                    if previous.layout.item_sizes[a] == layout.item_sizes[b]
                        && let Some(height) = old_heights.heights[a]
                    {
                        layout.record_height(b, height);
                    }
                }
            }
        }
        *state = Some(MeasurementState {
            layout: layout.clone(),
            document_released: false,
            width,
            fonts,
            geometry_style,
        });
        layout
    }
}

impl MarkdownBlockLayout {
    pub(crate) fn measurement_id(&self) -> usize {
        if self.measured {
            Rc::as_ptr(&self.geometry) as usize
        } else {
            0
        }
    }

    pub fn from_document(document: &MarkdownDocument, opts: &MarkdownOptions) -> Self {
        let mut layout = Self {
            document: Some(document.clone()),
            footnotes: opts.enable_footnotes,
            item_sizes: Rc::new(Vec::new()),
            geometry: Rc::new(RefCell::new(MeasuredGeometry {
                heights: Vec::new(),
                index: BlockHeights::new(&[], opts.block_gap),
                sizes: None,
            })),
            measured: false,
            disclosures: Default::default(),
        };
        let sizes = layout
            .items()
            .enumerate()
            .map(|(index, item)| {
                let padding = match item {
                    MarkdownLayoutItem::Block(block) => {
                        crate::style::block_top_padding(block, index, opts)
                    }
                    MarkdownLayoutItem::Footnotes(_) => 0.0,
                };
                size(
                    px(ESTIMATED_CONTENT_WIDTH),
                    px(estimate_item_height(&item, opts) + padding),
                )
            })
            .collect::<Vec<_>>();
        layout.geometry.borrow_mut().index = BlockHeights::new(&sizes, opts.block_gap);
        layout.item_sizes = Rc::new(sizes);
        layout
    }

    pub(crate) fn len(&self) -> usize {
        self.document.as_ref().map_or(0, |doc| {
            doc.blocks.len() + usize::from(self.footnotes && !doc.footnotes.is_empty())
        })
    }

    pub(crate) fn item(&self, index: usize) -> Option<MarkdownLayoutItem<'_>> {
        let document = self.document.as_ref()?;
        if let Some(block) = document.blocks.get(index) {
            return Some(MarkdownLayoutItem::Block(block));
        }
        (index == document.blocks.len() && self.footnotes && !document.footnotes.is_empty())
            .then(|| MarkdownLayoutItem::Footnotes(document.footnotes.as_slice()))
    }

    pub fn items(
        &self,
    ) -> impl ExactSizeIterator<Item = MarkdownLayoutItem<'_>> + DoubleEndedIterator {
        (0..self.len()).map(|index| self.item(index).expect("layout item is in bounds"))
    }

    pub(crate) fn heights(&self) -> Ref<'_, BlockHeights> {
        Ref::map(self.geometry.borrow(), |geometry| &geometry.index)
    }

    pub fn item_sizes(&self) -> Rc<Vec<Size<Pixels>>> {
        if !self.measured {
            return self.item_sizes.clone();
        }
        let mut geometry = self.geometry.borrow_mut();
        if let Some(sizes) = &geometry.sizes {
            return sizes.clone();
        }
        let sizes = Rc::new(
            self.item_sizes
                .iter()
                .enumerate()
                .map(|(index, size)| {
                    gpui::size(
                        size.width,
                        px(geometry.heights[index].unwrap_or(f32::from(size.height))),
                    )
                })
                .collect::<Vec<_>>(),
        );
        geometry.sizes = Some(sizes.clone());
        sizes
    }

    pub(crate) fn measure(&self, index: usize, child: gpui::AnyElement) -> gpui::AnyElement {
        use gpui::{IntoElement, ParentElement, Styled};
        if !self.measured {
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
                        if layout.record_height(index, f32::from(bounds.size.height)) {
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
        if !self.measured || !height.is_finite() || height < 0.0 {
            return false;
        }
        let mut geometry = self.geometry.borrow_mut();
        if geometry.heights[index].is_some_and(|old| (old - height).abs() <= 0.5) {
            return false;
        }
        let old = geometry.heights[index].unwrap_or(f32::from(self.item_sizes[index].height));
        geometry.heights[index] = Some(height);
        geometry.index.update(index, height - finite_height(old));
        geometry.sizes = None;
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
    fn height_index_updates_preserve_gap_and_boundary_visibility() {
        let sizes = [10.0, 20.0, 30.0].map(|height| size(px(100.0), px(height)));
        let mut heights = BlockHeights::new(&sizes, 5.0);
        assert_eq!(
            (heights.total(), heights.top(1), heights.bottom(1)),
            (70.0, 15.0, 35.0)
        );
        for (top, bottom, expected) in [
            (0.0, 10.0, 0..1),
            (10.0, 15.0, 0..2),
            (11.0, 14.0, 1..1),
            (35.0, 40.0, 1..3),
            (70.0, 90.0, 2..3),
        ] {
            assert_eq!(heights.visible_range(top, bottom), expected);
        }
        heights.update(1, 10.0);
        assert_eq!(
            (heights.total(), heights.top(2), heights.bottom(1)),
            (80.0, 50.0, 45.0)
        );
        assert_eq!(heights.visible_range(40.0, 50.0), 1..3);
        let sizes = [0.0, 10.0, 0.0].map(|height| size(px(100.0), px(height)));
        let heights = BlockHeights::new(&sizes, 0.0);
        assert_eq!(heights.visible_range(0.0, 0.0), 0..2);
        assert_eq!(heights.visible_range(10.0, 10.0), 1..3);
    }

    #[test]
    fn nested_blocks_and_footnotes_reuse_heights_after_source_offsets_shift() {
        let source = "- item\n  - nested\n\n| h |\n| - |\n| value |\n\n```rs\nlet x = 1;\n```\n\nend[^n]\n\n[^n]: note";
        let opts = MarkdownOptions::default();
        let measurements = MarkdownMeasurements::default();
        let document = crate::parser::parse_with_source_ranges(source);
        let layout = measurements.prepare(&document, 640.0, &opts);
        for (index, height) in [111.0, 222.0, 333.0, 444.0, 555.0].into_iter().enumerate() {
            layout.record_height(index, height);
        }
        let reused = measurements.prepare(&document, 640.0, &opts);
        assert!(Rc::ptr_eq(&layout.geometry, &reused.geometry));
        assert!(Rc::ptr_eq(&layout.item_sizes(), &reused.item_sizes()));
        let changed = crate::parser::parse_with_source_ranges(&format!("new\n\n{source}"));
        let changed = measurements.prepare(&changed, 640.0, &opts);
        assert_eq!(
            changed
                .item_sizes()
                .iter()
                .map(|size| f32::from(size.height))
                .collect::<Vec<_>>(),
            [22.0, 111.0, 222.0, 333.0, 444.0, 555.0]
        );
    }

    #[test]
    fn unchanged_blocks_keep_measured_heights_after_source_offsets_shift() {
        let opts = MarkdownOptions::default();
        let measurements = MarkdownMeasurements::default();
        let document = crate::parser::parse_with_source_ranges("first\n\nsecond\n\nthird");
        let layout = measurements.prepare(&document, 640.0, &opts);
        layout.record_height(0, 40.0);
        layout.record_height(1, 80.0);
        layout.record_height(2, 60.0);
        let document = crate::parser::parse_with_source_ranges("new\n\nfirst\n\nsecond\n\nthird");
        let changed = measurements.prepare(&document, 640.0, &opts);
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
        let changed = measurements.prepare(&document, 640.0, &opts);
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
        let layout = measurements.prepare(&document, 640.0, &opts);
        layout.disclosures.toggle("html-details", false);
        layout.disclosures.toggle("html-details-2", true);
        layout.record_height(0, 245.0);
        let blocks = std::sync::Arc::downgrade(&document.blocks);
        drop(layout);
        measurements.release_document();
        assert_eq!(
            blocks.strong_count(),
            1,
            "only the caller should retain the document"
        );
        for width in [640.0, 320.0] {
            let layout = measurements.prepare(&document, width, &opts);
            assert!(layout.disclosures.is_open("html-details", false));
            assert!(!layout.disclosures.is_open("html-details-2", true));
            if width == 640.0 {
                assert_eq!(layout.geometry.borrow().heights[0], Some(245.0));
            }
        }
        let replacement =
            crate::parser::parse("<details><summary>Different</summary><p>New body</p></details>");
        let layout = measurements.prepare(&replacement, 320.0, &opts);
        assert!(!layout.disclosures.is_open("html-details", false));
    }
}
